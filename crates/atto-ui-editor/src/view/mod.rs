use std::process::Command as ProcessCommand;
use std::time::{Duration, Instant};

use atto_ui::composable::{
    ComponentContext, EventResult, MouseCoordinateSpace, ScrollConfig, Scrollable,
};
use atto_ui::reactive::{DirtyObserver, EventQueue};
use atto_ui::{ComponentError, ComponentValue, ComponentValueCodec};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use editor_core::{
    Command, CursorCommand, DiagnosticSeverity, EditCommand, EditorStateManager, Position,
    SearchOptions, Selection, SelectionDirection, StyleCommand, TabKeyBehavior, ViewCommand,
    char_width,
};
use editor_core_lsp::{
    LspContentChange, LspDiagnostic, LspDiagnosticSeverity, LspSession, locations_from_value,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use serde_json::json;

use super::config::{EditorConfig, EditorLspGotoKind, EditorLspMode};
use super::keymap::{EditorAction, EditorKeymap, KeyChord};
use super::popup::{CompletionPopupModel, HoverPopupModel, LspCompletionItemEdit};
use super::theme::{EditorTheme, EditorThemeSet};
use crate::syntax::SyntaxProcessor;

mod actions;
mod component_impl;
mod editing;
mod input;
mod lsp;
mod render;
mod scrolling;
mod search;
mod selection;
mod state;
mod syntax;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub enum EditorEvent {
    LspGoto {
        kind: EditorLspGotoKind,
        locations: Vec<editor_core_lsp::LspLocation>,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticsSummary {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub hints: usize,
}

impl DiagnosticsSummary {
    pub(crate) fn from_diagnostics(diagnostics: &[LspDiagnostic]) -> Self {
        let mut summary = Self::default();
        for diagnostic in diagnostics {
            match diagnostic.severity {
                Some(LspDiagnosticSeverity::Error) => summary.errors += 1,
                Some(LspDiagnosticSeverity::Warning) => summary.warnings += 1,
                Some(LspDiagnosticSeverity::Information) => summary.infos += 1,
                Some(LspDiagnosticSeverity::Hint) => summary.hints += 1,
                None => summary.infos += 1,
            }
        }
        summary
    }
}

#[derive(Clone, Debug)]
pub struct EditorViewHandle {
    pub events: EventQueue<EditorEvent>,
    pub hover_popup: atto_ui::reactive::Binding<Option<HoverPopupModel>>,
    pub hover_popup_dismissed: atto_ui::reactive::Binding<Option<Position>>,
    pub completion_popup: atto_ui::reactive::Binding<Option<CompletionPopupModel>>,
    pub diagnostics_summary: atto_ui::reactive::Binding<DiagnosticsSummary>,
    pub theme: atto_ui::reactive::Binding<EditorThemeSet>,
    pub language_id: atto_ui::reactive::Binding<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HoverAnchor {
    position: Position,
    /// Anchor point in absolute screen coordinates (0-based).
    screen: (u16, u16),
}

#[derive(Debug, Clone, Copy)]
struct MouseDrag {
    anchor: Position,
    rect: bool,
}

#[derive(Debug, Clone, Copy)]
struct ClickState {
    at: Instant,
    pos: Position,
    count: u8,
}

#[derive(Default)]
struct EditorLspController {
    session: Option<LspSession>,

    // Hover scheduling/state.
    hover_due: Option<Instant>,
    hover_pending_request: Option<u64>,
    hover_anchor: Option<HoverAnchor>,
    hover_target: Option<HoverAnchor>,
    hover_requested: Option<HoverAnchor>,
    hover_suppressed_position: Option<Position>,

    // Completion scheduling/state.
    completion_pending_request: Option<u64>,
    completion_requested_position: Option<Position>,

    // Pending goto request id -> kind.
    pending_goto: Option<(u64, EditorLspGotoKind)>,

    // Diagnostics state.
    diagnostics: Vec<LspDiagnostic>,
    diagnostic_result_id: Option<String>,
    pending_document_diagnostic: Option<u64>,
    diagnostic_cursor: Option<usize>,
    diagnostics_revision: u64,
}

pub struct EditorView {
    config: EditorConfig,

    theme: atto_ui::reactive::Binding<EditorThemeSet>,

    // Outputs / host integration
    events: EventQueue<EditorEvent>,
    hover_popup: atto_ui::reactive::Binding<Option<HoverPopupModel>>,
    hover_popup_dismissed: atto_ui::reactive::Binding<Option<Position>>,
    completion_popup: atto_ui::reactive::Binding<Option<CompletionPopupModel>>,
    diagnostics_summary: atto_ui::reactive::Binding<DiagnosticsSummary>,

    state_manager: EditorStateManager,

    last_area: Option<Rect>,
    viewport_size: (u16, u16),
    content_size: (u16, u16),

    text_observer: DirtyObserver,
    syntax_observer: DirtyObserver,
    lsp_observer: DirtyObserver,

    syntax_processor: Option<SyntaxProcessor>,
    lsp: EditorLspController,
    search: search::SearchState,

    // Mouse + selection
    mouse_drag: Option<MouseDrag>,
    rect_selection_mode: bool,
    rect_selection_anchor: Option<Position>,
    last_click: Option<ClickState>,

    // Undo grouping
    last_insert_time: Option<Instant>,

    focused_last_frame: bool,
}

impl EditorView {
    pub fn new(
        config: EditorConfig,
        theme: impl Into<atto_ui::reactive::Binding<EditorThemeSet>>,
    ) -> (Self, EditorViewHandle) {
        let initial = config.text.get();

        let theme = theme.into();
        let events = EventQueue::new();
        let hover_popup = atto_ui::reactive::Binding::new(None);
        let hover_popup_dismissed = atto_ui::reactive::Binding::new(None);
        let completion_popup = atto_ui::reactive::Binding::new(None);
        let diagnostics_summary = atto_ui::reactive::Binding::new(DiagnosticsSummary::default());

        let handle = EditorViewHandle {
            events: events.clone(),
            hover_popup: hover_popup.clone(),
            hover_popup_dismissed: hover_popup_dismissed.clone(),
            completion_popup: completion_popup.clone(),
            diagnostics_summary: diagnostics_summary.clone(),
            theme: theme.clone(),
            language_id: config.language_id.clone(),
        };

        let mut view = Self {
            text_observer: config.text.dirty_observer(),
            syntax_observer: config.syntax.dirty_observer(),
            lsp_observer: config.lsp.dirty_observer(),
            config,
            theme,
            events,
            hover_popup,
            hover_popup_dismissed,
            completion_popup,
            diagnostics_summary,
            state_manager: EditorStateManager::new(&initial, 1),
            last_area: None,
            viewport_size: (0, 0),
            content_size: (0, 0),
            syntax_processor: None,
            lsp: EditorLspController::default(),
            search: search::SearchState::default(),
            mouse_drag: None,
            rect_selection_mode: false,
            rect_selection_anchor: None,
            last_click: None,
            last_insert_time: None,
            focused_last_frame: false,
        };

        view.configure_syntax_processor();
        view.start_lsp_if_enabled();
        (view, handle)
    }
}
