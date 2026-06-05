use std::time::{Duration, Instant};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use editor_core::{
    Command, CursorCommand, EditCommand, EditorStateManager, Position, Selection,
    SelectionDirection, StyleCommand, TabKeyBehavior, ViewCommand, layout::char_width,
};
use editor_core_highlight_simple::{RegexHighlightProcessor, SimpleIniStyles, SimpleJsonStyles};
use editor_core_lsp::{LspContentChange, LspSession, locations_from_value};
use editor_core_sublime::{SublimeProcessor, SublimeSyntaxSet};
use editor_core_treesitter::{TreeSitterProcessor, TreeSitterProcessorConfig};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use serde_json::json;
use std::process::Command as ProcessCommand;

use atto_ui::composable::{ComponentContext, EventResult, ScrollConfig, Scrollable};
use atto_ui::reactive::{DirtyObserver, EventQueue};
use atto_ui::{ComponentError, ComponentValue, ComponentValueCodec};

use super::config::{EditorConfig, EditorLspGotoKind, EditorLspMode, EditorSyntaxConfig};
use super::keymap::{EditorAction, EditorKeymap, KeyChord};
use super::popup::{CompletionPopupModel, HoverPopupModel, LspCompletionItemEdit};
use super::theme::{EditorTheme, EditorThemeSet};

mod input;
mod lsp;
mod render;
mod search;
mod selection;
mod syntax;

#[derive(Clone, Debug)]
pub enum EditorEvent {
    LspGoto {
        kind: EditorLspGotoKind,
        locations: Vec<editor_core_lsp::LspLocation>,
    },
}

#[derive(Clone, Debug)]
pub struct EditorViewHandle {
    pub events: EventQueue<EditorEvent>,
    pub hover_popup: atto_ui::reactive::Binding<Option<HoverPopupModel>>,
    pub hover_popup_dismissed: atto_ui::reactive::Binding<Option<Position>>,
    pub completion_popup: atto_ui::reactive::Binding<Option<CompletionPopupModel>>,
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

#[allow(clippy::large_enum_variant)]
enum SyntaxProcessor {
    Regex(RegexHighlightProcessor),
    Sublime(SublimeProcessor),
    TreeSitter(TreeSitterProcessor),
}

impl SyntaxProcessor {
    fn apply(&mut self, state: &mut EditorStateManager) {
        match self {
            SyntaxProcessor::Regex(p) => {
                let _ = state.apply_processor(p);
            }
            SyntaxProcessor::Sublime(p) => {
                let _ = state.apply_processor(p);
            }
            SyntaxProcessor::TreeSitter(p) => {
                let _ = state.apply_processor(p);
            }
        }
    }
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
}

pub struct EditorView {
    config: EditorConfig,

    theme: atto_ui::reactive::Binding<EditorThemeSet>,

    // Outputs / host integration
    events: EventQueue<EditorEvent>,
    hover_popup: atto_ui::reactive::Binding<Option<HoverPopupModel>>,
    hover_popup_dismissed: atto_ui::reactive::Binding<Option<Position>>,
    completion_popup: atto_ui::reactive::Binding<Option<CompletionPopupModel>>,

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

        let handle = EditorViewHandle {
            events: events.clone(),
            hover_popup: hover_popup.clone(),
            hover_popup_dismissed: hover_popup_dismissed.clone(),
            completion_popup: completion_popup.clone(),
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

    fn start_lsp_if_enabled(&mut self) {
        let EditorLspMode::Enabled(cfg) = self.config.lsp.get() else {
            return;
        };
        self.start_lsp_session(cfg);
    }

    fn active_cursor_position(&self) -> Position {
        self.state_manager.get_cursor_state().position
    }

    fn editor_theme(&self) -> EditorTheme {
        let theme_set = self.theme.get();
        let language_id = self.config.language_id.get();
        theme_set.for_language(language_id.as_str()).clone()
    }

    fn sync_external_text_if_dirty(&mut self) {
        if !self.config.text.check_dirty(&mut self.text_observer) {
            return;
        }

        let external = self.config.text.get();
        let current = self.state_manager.editor().get_text();
        if external == current {
            return;
        }

        let old_char_count = self.state_manager.editor().char_count();
        let _ = self
            .state_manager
            .execute(Command::Edit(EditCommand::Replace {
                start: 0,
                length: old_char_count,
                text: external,
            }));

        self.last_insert_time = None;
        self.maybe_apply_syntax_highlighting();
        self.hide_popups();
    }

    fn hide_popups(&mut self) {
        if self.hover_popup.get().is_some() {
            self.hover_popup.set(None);
        }
        if self.completion_popup.get().is_some() {
            self.completion_popup.set(None);
        }
        self.lsp.hover_due = None;
        self.lsp.hover_pending_request = None;
        self.lsp.hover_anchor = None;
        self.lsp.hover_target = None;
        self.lsp.hover_requested = None;
        self.lsp.hover_suppressed_position = None;
        self.hover_popup_dismissed.set(None);
        self.lsp.completion_pending_request = None;
        self.lsp.completion_requested_position = None;
        self.lsp.pending_goto = None;
    }

    fn ensure_viewport(&mut self, text_area: Rect) {
        let viewport_changed = self.viewport_size != (text_area.width, text_area.height);

        let viewport_height = text_area.height as usize;
        self.state_manager.set_viewport_height(viewport_height);

        let viewport_width = text_area.width.max(1) as usize;
        if viewport_width != self.state_manager.editor().viewport_width {
            let _ = self
                .state_manager
                .execute(Command::View(ViewCommand::SetViewportWidth {
                    width: viewport_width,
                }));
        }

        if viewport_height > 0 {
            let max_scroll_top = self.max_scroll_top(viewport_height);
            let current_scroll_top = self.state_manager.get_viewport_state().scroll_top;
            if current_scroll_top > max_scroll_top {
                self.state_manager.set_scroll_top(max_scroll_top);
            }
            // Auto-scroll to keep the cursor visible when the viewport geometry changes
            // (e.g. terminal resize). When the viewport is stable, allow user-driven scrolling
            // (mouse wheel / scrollbars) without snapping back to the cursor every frame.
            if viewport_changed {
                self.adjust_scroll();
            }
        }

        self.viewport_size = (text_area.width, text_area.height);
        let total_visual = self.state_manager.editor().visual_line_count();
        self.content_size = (text_area.width, total_visual.min(u16::MAX as usize) as u16);
    }

    fn max_scroll_top(&self, viewport_height: usize) -> usize {
        let total_visual = self.state_manager.editor().visual_line_count();
        total_visual.saturating_sub(viewport_height)
    }

    fn adjust_scroll(&mut self) {
        let viewport_height = self.state_manager.get_viewport_state().height.unwrap_or(0);
        if viewport_height == 0 {
            return;
        }

        let editor = self.state_manager.editor();
        let cursor_pos = self.active_cursor_position();
        let Some((cursor_visual_row, _)) =
            editor.logical_position_to_visual(cursor_pos.line, cursor_pos.column)
        else {
            return;
        };

        let mut scroll_top = self.state_manager.get_viewport_state().scroll_top;
        if cursor_visual_row < scroll_top {
            scroll_top = cursor_visual_row;
        }
        if cursor_visual_row >= scroll_top + viewport_height {
            scroll_top = cursor_visual_row
                .saturating_sub(viewport_height)
                .saturating_add(1);
        }

        scroll_top = scroll_top.min(self.max_scroll_top(viewport_height));
        self.state_manager.set_scroll_top(scroll_top);
    }

    fn selection_offsets(&self, selection: &Selection) -> (usize, usize) {
        let start_offset = self
            .state_manager
            .editor()
            .line_index
            .position_to_char_offset(selection.start.line, selection.start.column);
        let end_offset = self
            .state_manager
            .editor()
            .line_index
            .position_to_char_offset(selection.end.line, selection.end.column);
        (start_offset.min(end_offset), start_offset.max(end_offset))
    }

    fn cursor_offset(&self) -> usize {
        let pos = self.active_cursor_position();
        self.state_manager
            .editor()
            .line_index
            .position_to_char_offset(pos.line, pos.column)
    }

    fn execute(&mut self, command: Command) -> bool {
        self.state_manager.execute(command).is_ok()
    }

    fn execute_and_sync_text(&mut self, command: Command) -> bool {
        let before = self.state_manager.editor().get_text();
        if !self.execute(command) {
            return false;
        }
        let after = self.state_manager.editor().get_text();
        if after != before {
            self.config.text.set(after);
        }
        true
    }

    fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        let has_multi = !self
            .state_manager
            .editor()
            .secondary_selections()
            .is_empty();

        let mut full_lsp_change = None::<LspContentChange>;
        let mut lsp_change = None::<LspContentChange>;
        if let Some(lsp) = self.lsp.session.as_ref() {
            if has_multi {
                let old_char_count = self.state_manager.editor().char_count();
                full_lsp_change = Some(lsp.full_document_change(
                    &self.state_manager.editor().line_index,
                    old_char_count,
                    "",
                ));
            } else {
                let cursor_state = self.state_manager.get_cursor_state();
                let primary = cursor_state.primary_selection_index;
                let selection = cursor_state.selections.get(primary);
                let (start, end) = selection
                    .filter(|s| s.start != s.end)
                    .map(|s| self.selection_offsets(s))
                    .unwrap_or_else(|| {
                        let offset = self.cursor_offset();
                        (offset, offset)
                    });
                lsp_change = Some(lsp.content_change_for_offsets(
                    &self.state_manager.editor().line_index,
                    start,
                    end,
                    text,
                ));
            }
        }

        if !self.execute_and_sync_text(Command::Edit(EditCommand::InsertText {
            text: text.to_string(),
        })) {
            return;
        }

        self.last_insert_time = Some(Instant::now());
        self.maybe_apply_syntax_highlighting();
        self.hide_hover_popup_only();

        if let Some(change) = lsp_change {
            self.lsp_did_change(change);
        } else if let Some(mut change) = full_lsp_change {
            change.text = self.state_manager.editor().get_text();
            self.lsp_did_change(change);
        }
    }

    fn hide_hover_popup_only(&mut self) {
        if self.hover_popup.get().is_some() {
            self.hover_popup.set(None);
        }
        self.lsp.hover_due = None;
        self.lsp.hover_pending_request = None;
        self.lsp.hover_target = None;
        self.lsp.hover_requested = None;
    }

    fn consume_hover_popup_dismissed(&mut self) {
        let Some(pos) = self.hover_popup_dismissed.get() else {
            return;
        };
        self.hover_popup_dismissed.set(None);

        // Suppress re-showing at the same hover position until the mouse moves elsewhere.
        self.lsp.hover_suppressed_position = Some(pos);
        self.lsp.hover_due = None;
        self.lsp.hover_pending_request = None;
        self.lsp.hover_target = None;
        self.lsp.hover_requested = None;
    }

    fn update_hover_anchor(&mut self, pos: Position, screen: (u16, u16)) {
        if self.lsp.hover_suppressed_position.is_some_and(|p| p != pos) {
            self.lsp.hover_suppressed_position = None;
        }

        let prev_pos = self.lsp.hover_anchor.map(|a| a.position);
        self.lsp.hover_anchor = Some(HoverAnchor {
            position: pos,
            screen,
        });

        // When the hovered position changes, any visible tooltip and any in-flight request become
        // stale. Don't treat this as an explicit dismissal: allow the tooltip to show again after
        // the normal idle delay.
        if prev_pos != Some(pos) {
            if self.hover_popup.get().is_some() {
                self.hover_popup.set(None);
            }
            self.lsp.hover_due = None;
            self.lsp.hover_pending_request = None;
            self.lsp.hover_target = None;
            self.lsp.hover_requested = None;
            return;
        }

        // Same token/position: keep the tooltip close to the mouse.
        if let Some(mut popup) = self.hover_popup.get()
            && popup.anchor == pos
        {
            popup.rect = self.hover_popup_rect_for_screen_point(screen, popup.contents.lines());
            self.hover_popup.set(Some(popup));
        }
    }

    fn backspace(&mut self) {
        let has_multi = !self
            .state_manager
            .editor()
            .secondary_selections()
            .is_empty();

        let cursor_state = self.state_manager.get_cursor_state();
        let has_any_selection = cursor_state.selections.iter().any(|s| s.start != s.end);

        if has_any_selection && !has_multi {
            self.delete_selection();
            return;
        }

        let mut full_lsp_change = None::<LspContentChange>;
        let mut lsp_change = None::<LspContentChange>;
        if let Some(lsp) = self.lsp.session.as_ref() {
            if has_multi {
                let old_char_count = self.state_manager.editor().char_count();
                full_lsp_change = Some(lsp.full_document_change(
                    &self.state_manager.editor().line_index,
                    old_char_count,
                    "",
                ));
            } else {
                let offset = self.cursor_offset();
                if offset > 0 {
                    lsp_change = Some(lsp.content_change_for_offsets(
                        &self.state_manager.editor().line_index,
                        offset - 1,
                        offset,
                        "",
                    ));
                }
            }
        }

        let before_text = self.state_manager.editor().get_text();
        if !self.execute(Command::Edit(EditCommand::Backspace)) {
            return;
        }
        let after_text = self.state_manager.editor().get_text();
        if after_text == before_text {
            return;
        }
        self.config.text.set(after_text.clone());

        self.last_insert_time = None;
        self.maybe_apply_syntax_highlighting();
        self.hide_popups();

        if let Some(change) = lsp_change {
            self.lsp_did_change(change);
        } else if let Some(mut change) = full_lsp_change {
            change.text = after_text;
            self.lsp_did_change(change);
        }
    }

    fn delete_forward(&mut self) {
        let has_multi = !self
            .state_manager
            .editor()
            .secondary_selections()
            .is_empty();

        let cursor_state = self.state_manager.get_cursor_state();
        let has_any_selection = cursor_state.selections.iter().any(|s| s.start != s.end);

        if has_any_selection && !has_multi {
            self.delete_selection();
            return;
        }

        let mut full_lsp_change = None::<LspContentChange>;
        let mut lsp_change = None::<LspContentChange>;
        if let Some(lsp) = self.lsp.session.as_ref() {
            if has_multi {
                let old_char_count = self.state_manager.editor().char_count();
                full_lsp_change = Some(lsp.full_document_change(
                    &self.state_manager.editor().line_index,
                    old_char_count,
                    "",
                ));
            } else {
                let offset = self.cursor_offset();
                let max_offset = self.state_manager.editor().char_count();
                if offset < max_offset {
                    lsp_change = Some(lsp.content_change_for_offsets(
                        &self.state_manager.editor().line_index,
                        offset,
                        offset + 1,
                        "",
                    ));
                }
            }
        }

        let before_text = self.state_manager.editor().get_text();
        if !self.execute(Command::Edit(EditCommand::DeleteForward)) {
            return;
        }
        let after_text = self.state_manager.editor().get_text();
        if after_text == before_text {
            return;
        }
        self.config.text.set(after_text.clone());

        self.last_insert_time = None;
        self.maybe_apply_syntax_highlighting();
        self.hide_popups();

        if let Some(change) = lsp_change {
            self.lsp_did_change(change);
        } else if let Some(mut change) = full_lsp_change {
            change.text = after_text;
            self.lsp_did_change(change);
        }
    }

    fn delete_selection(&mut self) {
        let has_multi = !self
            .state_manager
            .editor()
            .secondary_selections()
            .is_empty();

        let cursor_state = self.state_manager.get_cursor_state();
        let primary = cursor_state.primary_selection_index;
        let Some(selection) = cursor_state.selections.get(primary) else {
            return;
        };
        let (start, end) = self.selection_offsets(selection);

        if start == end && !has_multi {
            let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
            return;
        }

        let mut full_lsp_change = None::<LspContentChange>;
        let mut lsp_change = None::<LspContentChange>;
        if let Some(lsp) = self.lsp.session.as_ref() {
            if has_multi {
                let old_char_count = self.state_manager.editor().char_count();
                full_lsp_change = Some(lsp.full_document_change(
                    &self.state_manager.editor().line_index,
                    old_char_count,
                    "",
                ));
            } else {
                lsp_change = Some(lsp.content_change_for_offsets(
                    &self.state_manager.editor().line_index,
                    start,
                    end,
                    "",
                ));
            }
        }

        let before_text = self.state_manager.editor().get_text();
        if !self.execute(Command::Edit(EditCommand::Backspace)) {
            return;
        }
        let after_text = self.state_manager.editor().get_text();
        if after_text == before_text {
            return;
        }
        self.config.text.set(after_text.clone());

        self.last_insert_time = None;
        self.maybe_apply_syntax_highlighting();
        self.hide_popups();

        if let Some(change) = lsp_change {
            self.lsp_did_change(change);
        } else if let Some(mut change) = full_lsp_change {
            change.text = after_text;
            self.lsp_did_change(change);
        }
    }

    fn copy_selection(&mut self) {
        let cursor_state = self.state_manager.get_cursor_state();
        let selections: Vec<Selection> = cursor_state
            .selections
            .into_iter()
            .filter(|s| s.start != s.end)
            .collect();
        if selections.is_empty() {
            return;
        }

        let mut parts = Vec::<String>::new();
        for sel in selections {
            let (start, end) = self.selection_offsets(&sel);
            let len = end.saturating_sub(start);
            if len == 0 {
                continue;
            }
            parts.push(
                self.state_manager
                    .editor()
                    .piece_table
                    .get_range(start, len),
            );
        }

        self.config.clipboard.set(parts.join("\n"));
    }

    fn cut_selection(&mut self) {
        self.copy_selection();
        self.delete_selection();
    }

    fn paste_clipboard(&mut self) {
        let text = self.config.clipboard.get();
        if text.is_empty() {
            return;
        }
        self.insert_text(&text);
    }

    fn indent_or_tab(&mut self) {
        let tab_width = self.config.indent.tab_width.get().max(1);
        let insert_spaces = self.config.indent.insert_spaces.get();

        let _ = self.execute(Command::View(ViewCommand::SetTabWidth { width: tab_width }));
        let _ = self.execute(Command::View(ViewCommand::SetTabKeyBehavior {
            behavior: if insert_spaces {
                TabKeyBehavior::Spaces
            } else {
                TabKeyBehavior::Tab
            },
        }));

        // LSP sync: use full-document change since InsertTab can expand to a variable number of
        // spaces (and also applies to multi-cursor / rectangular selections).
        let full_lsp_change = self.lsp.session.as_ref().map(|lsp| {
            let old_char_count = self.state_manager.editor().char_count();
            lsp.full_document_change(&self.state_manager.editor().line_index, old_char_count, "")
        });

        let before_text = self.state_manager.editor().get_text();
        if !self.execute(Command::Edit(EditCommand::InsertTab)) {
            return;
        }
        let after_text = self.state_manager.editor().get_text();
        if after_text == before_text {
            return;
        }
        self.config.text.set(after_text.clone());

        self.last_insert_time = Some(Instant::now());
        self.maybe_apply_syntax_highlighting();
        self.hide_hover_popup_only();

        if let Some(mut change) = full_lsp_change {
            change.text = after_text;
            self.lsp_did_change(change);
        }
    }

    fn select_all(&mut self) {
        let editor = self.state_manager.editor();
        let last_line = editor.line_index.line_count().saturating_sub(1);
        let last_col = editor
            .line_index
            .get_line_text(last_line)
            .map(|s| s.chars().count())
            .unwrap_or(0);

        let _ = self.execute(Command::Cursor(CursorCommand::SetSelection {
            start: Position::new(0, 0),
            end: Position::new(last_line, last_col),
        }));
    }

    fn toggle_fold_at_cursor(&mut self) {
        let line = self.active_cursor_position().line;
        self.toggle_fold_at_line(line);
    }

    fn toggle_fold_at_line(&mut self, logical_line: usize) {
        // LSP and syntax providers populate `folding_manager` with *possible* fold regions
        // (`is_collapsed = false`). When the user folds/unfolds, we should toggle that region
        // in-place rather than adding a duplicate folded region (which makes clicking the gutter
        // marker unable to reliably unfold).
        let mut regions = self
            .state_manager
            .editor()
            .folding_manager
            .regions()
            .to_vec();

        // Match `editor_core::FoldingManager::toggle_region_starting_at_line`: among all regions
        // starting at this line, choose the innermost one (smallest `end_line`).
        let mut best_idx = None::<usize>;
        let mut best_end = usize::MAX;
        for (idx, region) in regions.iter().enumerate() {
            if region.start_line != logical_line {
                continue;
            }
            if region.end_line <= region.start_line {
                continue;
            }
            if region.end_line < best_end {
                best_end = region.end_line;
                best_idx = Some(idx);
            }
        }

        let Some(idx) = best_idx else {
            return;
        };

        regions[idx].is_collapsed = !regions[idx].is_collapsed;
        self.state_manager.replace_folding_regions(regions, false);
        self.adjust_scroll();
    }

    fn unfold_all(&mut self) {
        let _ = self.execute(Command::Style(StyleCommand::UnfoldAll));
    }

    fn clear_secondary_selections(&mut self) {
        let _ = self.execute(Command::Cursor(CursorCommand::ClearSecondarySelections));
    }

    fn toggle_rect_selection(&mut self) {
        self.rect_selection_mode = !self.rect_selection_mode;
        self.rect_selection_anchor = None;
        self.mouse_drag = None;
        self.last_insert_time = None;
    }

    fn maybe_end_undo_group_after_idle(&mut self) {
        let Some(last_insert_time) = self.last_insert_time else {
            return;
        };
        if last_insert_time.elapsed() < Duration::from_millis(750) {
            return;
        }
        if self
            .state_manager
            .get_undo_redo_state()
            .current_change_group
            .is_none()
        {
            self.last_insert_time = None;
            return;
        }
        let _ = self.execute(Command::Edit(EditCommand::EndUndoGroup));
        self.last_insert_time = None;
    }

    fn move_cursor(&mut self, delta_line: isize, delta_column: isize, extend: bool) {
        // Clamp positive line movements to avoid `editor_core` rejecting out-of-bounds `MoveBy` /
        // `ExtendSelection` targets (notably `PageDown` near EOF).
        let line_count = self.state_manager.editor().line_index.line_count();
        if line_count == 0 {
            return;
        }
        let last_line = line_count.saturating_sub(1);

        if extend {
            let pos = self.active_cursor_position();
            let target_line = if delta_line.is_negative() {
                pos.line.saturating_sub((-delta_line) as usize)
            } else {
                pos.line.saturating_add(delta_line as usize).min(last_line)
            };
            let target_col = if delta_column.is_negative() {
                pos.column.saturating_sub((-delta_column) as usize)
            } else {
                pos.column.saturating_add(delta_column as usize)
            };
            let target = Position::new(target_line, target_col);
            if self.rect_selection_mode {
                let anchor = self.rect_selection_anchor.unwrap_or(pos);
                if self.rect_selection_anchor.is_none() {
                    self.rect_selection_anchor = Some(anchor);
                }
                let _ = self.execute(Command::Cursor(CursorCommand::SetRectSelection {
                    anchor,
                    active: target,
                }));
            } else {
                self.rect_selection_anchor = None;
                let _ = self.execute(Command::Cursor(CursorCommand::ExtendSelection {
                    to: target,
                }));
            }
        } else {
            // If there is an active primary selection, collapse it first (VSCode-like).
            let cursor_state = self.state_manager.get_cursor_state();
            if let Some(primary) = cursor_state
                .selections
                .get(cursor_state.primary_selection_index)
                && primary.start != primary.end
            {
                let (min_pos, max_pos) = if primary.start <= primary.end {
                    (primary.start, primary.end)
                } else {
                    (primary.end, primary.start)
                };

                let collapse_to = if delta_line.is_negative() || delta_column.is_negative() {
                    min_pos
                } else {
                    max_pos
                };

                let _ = self.execute(Command::Cursor(CursorCommand::MoveTo {
                    line: collapse_to.line,
                    column: collapse_to.column,
                }));
                let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
                self.rect_selection_anchor = None;
                self.adjust_scroll();
                self.hide_popups();
                return;
            }

            let pos = self.active_cursor_position();
            let max_down = last_line.saturating_sub(pos.line) as isize;
            let delta_line = if delta_line.is_positive() {
                delta_line.min(max_down)
            } else {
                delta_line
            };

            let _ = self.execute(Command::Cursor(CursorCommand::MoveBy {
                delta_line,
                delta_column,
            }));
            let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
            self.rect_selection_anchor = None;
        }
        self.adjust_scroll();
        self.hide_popups();
    }

    fn move_home_end(&mut self, end: bool, extend: bool) {
        let editor = self.state_manager.editor();
        let pos = self.active_cursor_position();
        let line_text = editor
            .line_index
            .get_line_text(pos.line)
            .unwrap_or_default();
        let col = if end { line_text.chars().count() } else { 0 };
        let target = Position::new(pos.line, col);

        if extend {
            let _ = self.execute(Command::Cursor(CursorCommand::ExtendSelection {
                to: target,
            }));
        } else {
            let _ = self.execute(Command::Cursor(CursorCommand::MoveTo {
                line: target.line,
                column: target.column,
            }));
            let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
        }
        self.adjust_scroll();
        self.hide_popups();
    }

    fn page_scroll(&mut self, down: bool, extend: bool) {
        let viewport_height = self.state_manager.get_viewport_state().height.unwrap_or(0);
        if viewport_height == 0 {
            return;
        }
        let delta = if down {
            viewport_height as isize
        } else {
            -(viewport_height as isize)
        };
        self.move_cursor(delta, 0, extend);
    }

    fn scroll_by_rows(&mut self, delta_rows: isize) {
        let viewport_height = self.state_manager.get_viewport_state().height.unwrap_or(0);
        if viewport_height == 0 {
            return;
        }
        let mut scroll_top = self.state_manager.get_viewport_state().scroll_top;
        if delta_rows.is_positive() {
            scroll_top = scroll_top.saturating_add(delta_rows as usize);
        } else {
            scroll_top = scroll_top.saturating_sub((-delta_rows) as usize);
        }
        scroll_top = scroll_top.min(self.max_scroll_top(viewport_height));
        self.state_manager.set_scroll_top(scroll_top);
    }

    fn style_for_style_ids(&self, style_ids: &[u32]) -> Style {
        let theme = self.editor_theme();

        let mut fg = None;
        let mut bg = None;
        let mut mods = Modifier::empty();

        let semantic_legend = self
            .lsp
            .session
            .as_ref()
            .and_then(|lsp| lsp.semantic_legend());

        for &style_id in style_ids {
            if let Some(style) = theme.style_ids.get(&style_id) {
                if style.fg.is_some() {
                    fg = style.fg;
                }
                if style.bg.is_some() {
                    bg = style.bg;
                }
                mods |= style.add_modifier;
                continue;
            }

            // Sublime: map StyleId -> scope string (if configured).
            if let Some(scope) = self.syntax_processor.as_ref().and_then(|p| match p {
                SyntaxProcessor::Sublime(p) => p.scope_mapper.scope_for_style_id(style_id),
                _ => None,
            }) {
                if let Some(style) = syntax::style_for_sublime_scope(&theme, scope) {
                    if style.fg.is_some() {
                        fg = style.fg;
                    }
                    if style.bg.is_some() {
                        bg = style.bg;
                    }
                    mods |= style.add_modifier;
                    continue;
                }

                mods |= theme.unknown_scope.add_modifier;
                if theme.unknown_scope.fg.is_some() {
                    fg = theme.unknown_scope.fg;
                }
                if theme.unknown_scope.bg.is_some() {
                    bg = theme.unknown_scope.bg;
                }
                continue;
            }

            // LSP semantic tokens default encoding: high 16 bits token_type, low 16 bits modifiers.
            if style_id < 0x0100_0000 {
                let (token_type_idx, token_mod_bits) =
                    editor_core_lsp::decode_semantic_style_id(style_id);
                let token_type_name = semantic_legend
                    .and_then(|legend| legend.token_types.get(token_type_idx as usize))
                    .map(|s| s.as_str());

                let token_style = token_type_name
                    .and_then(|name| theme.semantic_tokens.token_types.get(name))
                    .copied()
                    .unwrap_or(theme.semantic_tokens.unknown_token_type);

                if token_style.fg.is_some() {
                    fg = token_style.fg;
                }
                if token_style.bg.is_some() {
                    bg = token_style.bg;
                }
                mods |= token_style.add_modifier;

                if let Some(legend) = semantic_legend {
                    for (i, name) in legend.token_modifiers.iter().enumerate() {
                        if i >= 32 {
                            break;
                        }
                        if token_mod_bits & (1u32 << i) == 0 {
                            continue;
                        }
                        mods |= theme
                            .semantic_tokens
                            .token_modifiers
                            .get(name)
                            .copied()
                            .unwrap_or(theme.semantic_tokens.unknown_token_modifier);
                    }
                }

                continue;
            }

            // Unknown StyleId.
            if theme.unknown_style_id.fg.is_some() {
                fg = theme.unknown_style_id.fg;
            }
            if theme.unknown_style_id.bg.is_some() {
                bg = theme.unknown_style_id.bg;
            }
            mods |= theme.unknown_style_id.add_modifier;
        }

        let mut style = theme.text;
        if let Some(fg) = fg {
            style = style.fg(fg);
        }
        if let Some(bg) = bg {
            style = style.bg(bg);
        }
        style.add_modifier(mods)
    }

    fn handle_action(&mut self, action: EditorAction) -> bool {
        match action {
            EditorAction::Undo => {
                self.hide_popups();
                let old_char_count = self.state_manager.editor().char_count();
                let full_lsp_change = self.lsp.session.as_ref().map(|lsp| {
                    lsp.full_document_change(
                        &self.state_manager.editor().line_index,
                        old_char_count,
                        "",
                    )
                });
                let before = self.state_manager.editor().get_text();
                if !self.execute(Command::Edit(EditCommand::Undo)) {
                    return false;
                }
                let after = self.state_manager.editor().get_text();
                if after != before {
                    self.config.text.set(after.clone());
                }
                self.maybe_apply_syntax_highlighting();
                if let Some(mut change) = full_lsp_change {
                    change.text = after;
                    self.lsp_did_change(change);
                }
                true
            }
            EditorAction::Redo => {
                self.hide_popups();
                let old_char_count = self.state_manager.editor().char_count();
                let full_lsp_change = self.lsp.session.as_ref().map(|lsp| {
                    lsp.full_document_change(
                        &self.state_manager.editor().line_index,
                        old_char_count,
                        "",
                    )
                });
                let before = self.state_manager.editor().get_text();
                if !self.execute(Command::Edit(EditCommand::Redo)) {
                    return false;
                }
                let after = self.state_manager.editor().get_text();
                if after != before {
                    self.config.text.set(after.clone());
                }
                self.maybe_apply_syntax_highlighting();
                if let Some(mut change) = full_lsp_change {
                    change.text = after;
                    self.lsp_did_change(change);
                }
                true
            }
            EditorAction::Copy => {
                self.copy_selection();
                true
            }
            EditorAction::Cut => {
                self.cut_selection();
                true
            }
            EditorAction::Paste => {
                self.paste_clipboard();
                true
            }
            EditorAction::SelectAll => {
                self.select_all();
                true
            }

            EditorAction::Find => {
                self.hide_popups();
                let seed = self.search_seed_from_selection();
                self.open_find(seed.as_deref());
                true
            }
            EditorAction::Replace => {
                self.hide_popups();
                let seed = self.search_seed_from_selection();
                self.open_replace(seed.as_deref());
                true
            }
            EditorAction::FindNext => {
                self.hide_popups();
                if self.search_query_is_empty() {
                    self.open_find(None);
                } else {
                    self.search_find_next();
                }
                true
            }
            EditorAction::FindPrev => {
                self.hide_popups();
                if self.search_query_is_empty() {
                    self.open_find(None);
                } else {
                    self.search_find_prev();
                }
                true
            }

            EditorAction::Backspace => {
                self.backspace();
                true
            }
            EditorAction::DeleteForward => {
                self.delete_forward();
                true
            }
            EditorAction::InsertNewline => {
                self.insert_text("\n");
                true
            }
            EditorAction::InsertTab => {
                self.indent_or_tab();
                true
            }

            EditorAction::MoveLeft => {
                self.move_cursor(0, -1, false);
                true
            }
            EditorAction::MoveRight => {
                self.move_cursor(0, 1, false);
                true
            }
            EditorAction::MoveUp => {
                self.move_cursor(-1, 0, false);
                true
            }
            EditorAction::MoveDown => {
                self.move_cursor(1, 0, false);
                true
            }
            EditorAction::MoveHome => {
                self.move_home_end(false, false);
                true
            }
            EditorAction::MoveEnd => {
                self.move_home_end(true, false);
                true
            }
            EditorAction::PageUp => {
                self.page_scroll(false, false);
                true
            }
            EditorAction::PageDown => {
                self.page_scroll(true, false);
                true
            }

            EditorAction::SelectLeft => {
                self.move_cursor(0, -1, true);
                true
            }
            EditorAction::SelectRight => {
                self.move_cursor(0, 1, true);
                true
            }
            EditorAction::SelectUp => {
                self.move_cursor(-1, 0, true);
                true
            }
            EditorAction::SelectDown => {
                self.move_cursor(1, 0, true);
                true
            }
            EditorAction::SelectHome => {
                self.move_home_end(false, true);
                true
            }
            EditorAction::SelectEnd => {
                self.move_home_end(true, true);
                true
            }
            EditorAction::SelectPageUp => {
                self.page_scroll(false, true);
                true
            }
            EditorAction::SelectPageDown => {
                self.page_scroll(true, true);
                true
            }

            EditorAction::ClearSecondarySelections => {
                self.clear_secondary_selections();
                true
            }
            EditorAction::ToggleRectSelection => {
                self.toggle_rect_selection();
                true
            }

            EditorAction::ToggleFoldAtCursor => {
                self.toggle_fold_at_cursor();
                true
            }
            EditorAction::UnfoldAll => {
                self.unfold_all();
                true
            }

            EditorAction::CancelPopup => {
                if self.completion_popup.get().is_some() {
                    self.completion_popup.set(None);
                    return true;
                }
                if let Some(model) = self.hover_popup.get() {
                    // Treat Esc dismissal as an explicit close: don't re-show at the same hover
                    // position unless the mouse moves.
                    self.lsp.hover_suppressed_position = Some(model.anchor);
                    self.hover_popup.set(None);
                    self.lsp.hover_due = None;
                    self.lsp.hover_pending_request = None;
                    self.lsp.hover_target = None;
                    self.lsp.hover_requested = None;
                    return true;
                }

                // No popups: Esc should clear selection (and multi-cursor) if present.
                let cursor_state = self.state_manager.get_cursor_state();
                let has_primary_selection = cursor_state
                    .selections
                    .get(cursor_state.primary_selection_index)
                    .is_some_and(|s| s.start != s.end);
                let has_secondary = !self
                    .state_manager
                    .editor()
                    .secondary_selections()
                    .is_empty();
                if has_primary_selection || has_secondary {
                    let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
                    let _ = self.execute(Command::Cursor(CursorCommand::ClearSecondarySelections));
                    self.rect_selection_anchor = None;
                    return true;
                }

                false
            }

            EditorAction::LspRequestHover => {
                self.request_hover_now();
                true
            }
            EditorAction::LspRequestCompletion => {
                self.request_completion_now();
                true
            }
            EditorAction::LspGotoDefinition => {
                self.request_goto(EditorLspGotoKind::Definition);
                true
            }
            EditorAction::LspGotoDeclaration => {
                self.request_goto(EditorLspGotoKind::Declaration);
                true
            }
            EditorAction::LspGotoTypeDefinition => {
                self.request_goto(EditorLspGotoKind::TypeDefinition);
                true
            }
            EditorAction::LspGotoImplementation => {
                self.request_goto(EditorLspGotoKind::Implementation);
                true
            }
            EditorAction::LspGotoReferences => {
                self.request_goto(EditorLspGotoKind::References);
                true
            }

            EditorAction::ToggleLineNumbers => {
                let cur = self.config.show_line_numbers.get();
                self.config.show_line_numbers.set(!cur);
                true
            }
            EditorAction::ToggleFoldingMarkers => {
                let cur = self.config.show_folding_markers.get();
                self.config.show_folding_markers.set(!cur);
                true
            }
        }
    }

    fn request_hover_at_anchor(&mut self, anchor: HoverAnchor) {
        let Some(lsp) = self.lsp.session.as_mut() else {
            return;
        };
        let pos = anchor.position;
        if let Ok(id) = lsp.request_hover(
            &self.state_manager.editor().line_index,
            pos.line,
            pos.column,
        ) {
            self.lsp.hover_pending_request = Some(id);
            self.lsp.hover_requested = Some(anchor);
        }
    }

    fn request_hover_now(&mut self) {
        let Some(screen) = self.cursor_screen_position().and_then(|p| p) else {
            return;
        };
        self.request_hover_at_anchor(HoverAnchor {
            position: self.active_cursor_position(),
            screen,
        });
    }

    fn request_completion_now(&mut self) {
        if !self.config.completion.enabled.get() {
            return;
        }
        let pos = self.active_cursor_position();
        let Some(lsp) = self.lsp.session.as_mut() else {
            return;
        };
        if let Ok(id) = lsp.request_completion(
            &self.state_manager.editor().line_index,
            pos.line,
            pos.column,
        ) {
            self.lsp.completion_pending_request = Some(id);
            self.lsp.completion_requested_position = Some(pos);
        }
    }

    fn request_goto(&mut self, kind: EditorLspGotoKind) {
        let pos = self.active_cursor_position();
        let Some(lsp) = self.lsp.session.as_mut() else {
            return;
        };
        let line_index = &self.state_manager.editor().line_index;
        let request = match kind {
            EditorLspGotoKind::Definition => {
                lsp.request_definition(line_index, pos.line, pos.column)
            }
            EditorLspGotoKind::Declaration => {
                lsp.request_declaration(line_index, pos.line, pos.column)
            }
            EditorLspGotoKind::TypeDefinition => {
                lsp.request_type_definition(line_index, pos.line, pos.column)
            }
            EditorLspGotoKind::Implementation => {
                lsp.request_implementation(line_index, pos.line, pos.column)
            }
            EditorLspGotoKind::References => {
                lsp.request_references(line_index, pos.line, pos.column, true)
            }
        };
        if let Ok(id) = request {
            self.lsp.pending_goto = Some((id, kind));
        }
    }

    fn schedule_hover_after_delay(&mut self) {
        if self.hover_popup.get().is_some() {
            return;
        }
        if self.lsp.completion_pending_request.is_some() {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }
        if self.completion_popup.get().is_some() {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }
        if !self.config.hover.enabled.get() {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }
        if self.lsp.session.is_none() {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }
        if self.lsp.hover_pending_request.is_some() {
            return;
        }

        let Some(anchor) = self.lsp.hover_anchor else {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        };
        if self.lsp.hover_suppressed_position == Some(anchor.position) {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }

        let delay = self.config.hover.delay.get();
        self.lsp.hover_due = Some(Instant::now() + delay);
        self.lsp.hover_target = Some(anchor);
    }

    fn maybe_fire_hover(&mut self) {
        let Some(due) = self.lsp.hover_due else {
            return;
        };
        if Instant::now() < due {
            return;
        }
        if self.lsp.hover_pending_request.is_some() {
            return;
        }
        if self.lsp.completion_pending_request.is_some() {
            return;
        }
        if self.hover_popup.get().is_some() {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }
        if self.completion_popup.get().is_some() {
            return;
        }

        let Some(target) = self.lsp.hover_target else {
            self.lsp.hover_due = None;
            return;
        };

        if self.lsp.hover_anchor.map(|a| a.position) != Some(target.position) {
            self.schedule_hover_after_delay();
            return;
        }
        if self.lsp.hover_suppressed_position == Some(target.position) {
            self.lsp.hover_due = None;
            self.lsp.hover_target = None;
            return;
        }

        self.lsp.hover_due = None;
        self.lsp.hover_target = None;
        self.request_hover_at_anchor(target);
    }

    fn process_completion_accept(&mut self) {
        let Some(mut popup) = self.completion_popup.get() else {
            return;
        };
        let Some(idx) = popup.accept.take() else {
            return;
        };
        self.completion_popup.set(Some(popup.clone()));
        self.apply_completion_index(idx);
        self.completion_popup.set(None);
    }

    fn apply_completion_index(&mut self, idx: usize) {
        let Some(popup) = self.completion_popup.get() else {
            return;
        };
        let Some(item) = popup.items.get(idx) else {
            return;
        };

        let LspCompletionItemEdit::Raw(raw) = &item.edit;
        let Some(obj) = raw.as_object() else {
            return;
        };

        // Basic insertion strategy:
        // - prefer `textEdit` if present (TextEdit shape)
        // - else use `insertText`
        // - else insert `label`
        if let Some(text_edit) = obj.get("textEdit") {
            let full_lsp_change = self.lsp.session.as_ref().map(|lsp| {
                let old_char_count = self.state_manager.editor().char_count();
                lsp.full_document_change(
                    &self.state_manager.editor().line_index,
                    old_char_count,
                    "",
                )
            });

            let edits = editor_core_lsp::text_edits_from_value(&serde_json::Value::Array(vec![
                text_edit.clone(),
            ]));
            let _ = editor_core_lsp::apply_text_edits(&mut self.state_manager, &edits);
            let after_text = self.state_manager.editor().get_text();
            self.config.text.set(after_text.clone());
            self.maybe_apply_syntax_highlighting();
            self.hide_hover_popup_only();
            if let Some(mut change) = full_lsp_change {
                change.text = after_text;
                self.lsp_did_change(change);
            }
            return;
        }

        if let Some(insert_text) = obj.get("insertText").and_then(|v| v.as_str()) {
            self.insert_text(insert_text);
            return;
        }

        self.insert_text(item.label.as_str());
    }
}

impl ::atto_ui::composable::Component for EditorView {
    fn property_names(&self) -> Vec<&'static str> {
        vec![
            "text",
            "language_id",
            "show_line_numbers",
            "show_folding_markers",
            "tab_width",
            "insert_spaces",
        ]
    }

    fn get_property(&self, name: &str) -> Option<ComponentValue> {
        match name {
            "text" => Some(ComponentValue::String(self.config.text.get())),
            "language_id" => Some(ComponentValue::String(self.config.language_id.get())),
            "show_line_numbers" => Some(ComponentValue::Bool(self.config.show_line_numbers.get())),
            "show_folding_markers" => {
                Some(ComponentValue::Bool(self.config.show_folding_markers.get()))
            }
            "tab_width" => Some(ComponentValue::U64(
                self.config.indent.tab_width.get() as u64
            )),
            "insert_spaces" => Some(ComponentValue::Bool(self.config.indent.insert_spaces.get())),
            _ => None,
        }
    }

    fn set_property(&mut self, name: &str, value: ComponentValue) -> Result<(), ComponentError> {
        match name {
            "text" => {
                let v = <String as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.text.set(v);
                Ok(())
            }
            "language_id" => {
                let v = <String as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.language_id.set(v);
                Ok(())
            }
            "show_line_numbers" => {
                let v = <bool as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.show_line_numbers.set(v);
                Ok(())
            }
            "show_folding_markers" => {
                let v = <bool as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.show_folding_markers.set(v);
                Ok(())
            }
            "tab_width" => {
                let v = <usize as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.indent.tab_width.set(v);
                Ok(())
            }
            "insert_spaces" => {
                let v = <bool as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.indent.insert_spaces.set(v);
                Ok(())
            }
            _ => Err(ComponentError::unsupported_property(name)),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        // Sync external text changes and config at draw time too (tick-driven apps).
        self.sync_external_text_if_dirty();
        if self.config.syntax.check_dirty(&mut self.syntax_observer) {
            self.configure_syntax_processor();
        }
        self.maybe_start_or_stop_lsp();

        // Hover popup can be dismissed from its own tooltip window; reflect that here.
        self.consume_hover_popup_dismissed();

        // Poll LSP + hover timers.
        self.maybe_poll_lsp();
        self.maybe_end_undo_group_after_idle();

        if ctx.is_focused {
            if !self.focused_last_frame {
                self.schedule_hover_after_delay();
            }
            self.maybe_fire_hover();
        } else {
            self.hide_popups();
        }
        self.focused_last_frame = ctx.is_focused;

        // Handle completion accept requested by mouse events on the completion popup window.
        self.process_completion_accept();

        self.render(frame, area, ctx);
    }
}

impl ::atto_ui::composable::Layout for EditorView {
    fn min_width(&self) -> u16 {
        8
    }

    fn min_height(&self) -> u16 {
        3
    }
}

impl ::atto_ui::composable::Scrollable for EditorView {
    fn is_scrollable(&self) -> bool {
        true
    }

    fn content_size(&self) -> (u16, u16) {
        self.content_size
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.viewport_size
    }

    fn scroll_offset(&self) -> (u16, u16) {
        let scroll_top = self.state_manager.get_viewport_state().scroll_top;
        (0, (scroll_top.min(u16::MAX as usize)) as u16)
    }

    fn set_scroll_offset(&mut self, _x: u16, y: u16) {
        let viewport_height = self.state_manager.get_viewport_state().height.unwrap_or(0);
        if viewport_height == 0 {
            return;
        }
        let desired = y as usize;
        let max = self.max_scroll_top(viewport_height);
        self.state_manager.set_scroll_top(desired.min(max));
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.config.scroll.config.get()
    }
}

impl ::atto_ui::composable::FocusNav for EditorView {
    fn is_focusable(&self) -> bool {
        true
    }
}

impl ::atto_ui::composable::DynamicTree for EditorView {}

impl ::atto_ui::composable::EventHandling for EditorView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        // Keep internal state in sync with external bindings (without constantly cloning).
        self.sync_external_text_if_dirty();

        // Runtime config changes.
        if self.config.syntax.check_dirty(&mut self.syntax_observer) {
            self.configure_syntax_processor();
        }
        self.maybe_start_or_stop_lsp();

        // Popups should be dismissed whenever focus is lost.
        if !ctx.is_focused {
            self.hide_popups();
            return EventResult::ignored();
        }

        // Hover popup can be dismissed from its own tooltip window; reflect that here.
        self.consume_hover_popup_dismissed();

        // Keyboard input and clicks should dismiss hover immediately, but mouse movement should
        // allow the hover tooltip to track the pointer. Esc is special-cased so the popup close
        // path can set suppression state.
        let preserve_hover = matches!(
            event,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Moved,
                ..
            })
        ) || matches!(
            event,
            Event::Key(KeyEvent {
                code: KeyCode::Esc,
                ..
            })
        );
        if !preserve_hover {
            self.hide_hover_popup_only();
        }

        // Apply completion accept queued by mouse (from tooltip popup window).
        self.process_completion_accept();

        let res = match event {
            Event::Paste(text) => {
                self.insert_text(text);
                self.adjust_scroll();
                EventResult::consumed()
            }
            Event::Key(key) => self.handle_key_event(*key),
            Event::Mouse(m) => self.handle_mouse(*m),
            _ => EventResult::ignored(),
        };

        // Schedule hover for when the user goes idle again.
        self.schedule_hover_after_delay();

        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atto_ui::composable::{Component, EventHandling};

    fn buffer_row_string(
        terminal: &ratatui::Terminal<ratatui::backend::TestBackend>,
        y: u16,
    ) -> String {
        let buf = terminal.backend().buffer();
        let width = buf.area.width;
        let mut out = String::new();
        for x in 0..width {
            out.push_str(buf[(x, y)].symbol());
        }
        out
    }

    #[test]
    fn editor_view_applies_simple_json_highlighting_on_new() {
        let text: atto_ui::reactive::Binding<String> =
            r#"{"s": "hello", "n": 42}"#.to_string().into();
        let cfg = EditorConfig::new(text);
        cfg.syntax.set(EditorSyntaxConfig::SimpleJson);

        let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
        let (view, _handle) = EditorView::new(cfg, theme);

        // 'h' in "hello" is at column 7 in the sample.
        let offset = view
            .state_manager
            .editor()
            .line_index
            .position_to_char_offset(0, 7);
        let styles = view.state_manager.get_styles_at(offset);

        assert!(
            styles.contains(&editor_core_highlight_simple::SIMPLE_STYLE_STRING),
            "expected SIMPLE_STYLE_STRING at \"hello\"; got {styles:?}"
        );
    }

    #[test]
    fn editor_view_renders_simple_json_highlight_as_green_cells() {
        let text: atto_ui::reactive::Binding<String> = ["tab:ab", r#"{"s": "hello", "n": 42}"#, ""]
            .join("\n")
            .into();

        let cfg = EditorConfig::new(text);
        cfg.syntax.set(EditorSyntaxConfig::SimpleJson);

        let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
        let (mut view, _handle) = EditorView::new(cfg, theme);

        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).expect("terminal");

        let app_theme = atto_ui::theme::Theme::dark();
        let ctx = atto_ui::composable::ComponentContext {
            theme: &app_theme,
            window_id: atto_ui::wm::WindowId::default(),
            is_focused: true,
            scrollbar_host: atto_ui::composable::ScrollbarHost::Component,
            tab_mode: atto_ui::composable::TabMode::Cycle,
        };

        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, 80, 10);
                view.draw(f, area, ctx);
            })
            .expect("draw");

        let buf = terminal.backend().buffer();

        // JSON line is the second document line; with default gutter enabled the text starts at
        // x=6. 'h' in "hello" is at column 7 in the JSON line.
        let x = 6 + 7;
        let y = 1;
        let cell = buf.cell((x as u16, y as u16));
        assert!(cell.is_some(), "expected buffer cell at ({x}, {y})");
        let cell = cell.unwrap();
        assert_eq!(cell.symbol(), "h", "expected to sample the 'h' in hello");
        assert_eq!(
            cell.style().fg,
            Some(ratatui::style::Color::Green),
            "expected syntax-highlighted string cell to be green"
        );
    }

    #[test]
    fn editor_view_mouse_wheel_scrolls_even_at_viewport_edge() {
        let text: atto_ui::reactive::Binding<String> = (0..80)
            .map(|i| format!("LINE {:02}", i))
            .collect::<Vec<_>>()
            .join("\n")
            .into();

        let cfg = EditorConfig::new(text);

        let theme: atto_ui::reactive::Binding<EditorThemeSet> = EditorThemeSet::default().into();
        let (mut view, _handle) = EditorView::new(cfg, theme);

        let backend = ratatui::backend::TestBackend::new(40, 8);
        let mut terminal = ratatui::Terminal::new(backend).expect("terminal");

        let app_theme = atto_ui::theme::Theme::dark();
        let ctx = atto_ui::composable::ComponentContext {
            theme: &app_theme,
            window_id: atto_ui::wm::WindowId::default(),
            is_focused: true,
            scrollbar_host: atto_ui::composable::ScrollbarHost::Component,
            tab_mode: atto_ui::composable::TabMode::Cycle,
        };

        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, 40, 8);
                view.draw(f, area, ctx);
            })
            .expect("draw");

        let row0 = buffer_row_string(&terminal, 0);
        assert!(
            row0.contains("LINE 00"),
            "expected initial top line to be visible; got row0={row0:?}"
        );

        // Scroll while the pointer is over the text area.
        let (_gutter, text_area) = view.layout_rects(Rect::new(0, 0, 40, 8));
        let ev = Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: text_area.x.saturating_add(1),
            row: text_area.y,
            modifiers: KeyModifiers::NONE,
        });
        let _ = view.handle_event(&ev, ctx);

        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, 40, 8);
                view.draw(f, area, ctx);
            })
            .expect("draw after scroll");

        let row0 = buffer_row_string(&terminal, 0);
        assert!(
            row0.contains("LINE 03"),
            "expected wheel scroll to advance content by 3 rows; got row0={row0:?}"
        );

        // Scroll again while the pointer is over the gutter (should still scroll).
        let ev = Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: text_area.y,
            modifiers: KeyModifiers::NONE,
        });
        let _ = view.handle_event(&ev, ctx);

        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, 40, 8);
                view.draw(f, area, ctx);
            })
            .expect("draw after second scroll");

        let row0 = buffer_row_string(&terminal, 0);
        assert!(
            row0.contains("LINE 06"),
            "expected second wheel scroll to advance content again; got row0={row0:?}"
        );
    }
}
