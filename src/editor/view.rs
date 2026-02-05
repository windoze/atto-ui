use std::time::{Duration, Instant};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use editor_core::{
    Command, CursorCommand, EditCommand, EditorStateManager, Position, Selection,
    SelectionDirection, StyleCommand, ViewCommand, layout::char_width,
};
use editor_core_highlight_simple::{RegexHighlightProcessor, SimpleIniStyles, SimpleJsonStyles};
use editor_core_lsp::{LspContentChange, LspSession, locations_from_value};
use editor_core_sublime::{SublimeProcessor, SublimeSyntaxSet};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use serde_json::json;
use std::process::Command as ProcessCommand;

use crate::reactive::{DirtyObserver, EventQueue};
use crate::view::{View, ViewContext, ViewEventResult};
use crate::views::ScrollConfig;

use super::config::{EditorConfig, EditorLspGotoKind, EditorLspMode, EditorSyntaxConfig};
use super::keymap::{EditorAction, EditorKeymap, KeyChord};
use super::popup::{CompletionPopupModel, HoverPopupModel, LspCompletionItemEdit};
use super::theme::{EditorTheme, EditorThemeSet};

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
    pub hover_popup: crate::reactive::Binding<Option<HoverPopupModel>>,
    pub completion_popup: crate::reactive::Binding<Option<CompletionPopupModel>>,
    pub theme: crate::reactive::Binding<EditorThemeSet>,
    pub language_id: crate::reactive::Binding<String>,
}

#[derive(Debug, Clone, Copy)]
struct MouseDrag {
    anchor: Position,
    rect: bool,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum SyntaxProcessor {
    Regex(RegexHighlightProcessor),
    Sublime(SublimeProcessor),
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
        }
    }
}

pub struct EditorView {
    config: EditorConfig,

    theme: crate::reactive::Binding<EditorThemeSet>,

    // Outputs / host integration
    events: EventQueue<EditorEvent>,
    hover_popup: crate::reactive::Binding<Option<HoverPopupModel>>,
    completion_popup: crate::reactive::Binding<Option<CompletionPopupModel>>,

    state_manager: EditorStateManager,

    last_area: Option<Rect>,
    viewport_size: (u16, u16),
    content_size: (u16, u16),

    text_observer: DirtyObserver,
    syntax_observer: DirtyObserver,
    lsp_observer: DirtyObserver,

    syntax_processor: Option<SyntaxProcessor>,
    lsp: Option<LspSession>,

    // Mouse + selection
    mouse_drag: Option<MouseDrag>,
    rect_selection_mode: bool,

    // Undo grouping
    last_insert_time: Option<Instant>,

    // Hover scheduling (LSP)
    hover_due: Option<Instant>,
    hover_pending_request: Option<u64>,
    hover_target_position: Option<Position>,
    hover_requested_position: Option<Position>,

    // Completion scheduling/state (LSP)
    completion_pending_request: Option<u64>,
    completion_requested_position: Option<Position>,

    // Pending goto request id -> kind.
    pending_goto: Option<(u64, EditorLspGotoKind)>,

    focused_last_frame: bool,
}

impl EditorView {
    pub fn new(
        config: EditorConfig,
        theme: impl Into<crate::reactive::Binding<EditorThemeSet>>,
    ) -> (Self, EditorViewHandle) {
        let initial = config.text.get();

        let theme = theme.into();
        let events = EventQueue::new();
        let hover_popup = crate::reactive::Binding::new(None);
        let completion_popup = crate::reactive::Binding::new(None);

        let handle = EditorViewHandle {
            events: events.clone(),
            hover_popup: hover_popup.clone(),
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
            completion_popup,
            state_manager: EditorStateManager::new(&initial, 1),
            last_area: None,
            viewport_size: (0, 0),
            content_size: (0, 0),
            syntax_processor: None,
            lsp: None,
            mouse_drag: None,
            rect_selection_mode: false,
            last_insert_time: None,
            hover_due: None,
            hover_pending_request: None,
            hover_target_position: None,
            hover_requested_position: None,
            completion_pending_request: None,
            completion_requested_position: None,
            pending_goto: None,
            focused_last_frame: false,
        };

        view.configure_syntax_processor();
        (view, handle)
    }

    fn editor_theme(&self) -> EditorTheme {
        let theme_set = self.theme.get();
        let language_id = self.config.language_id.get();
        theme_set.for_language(language_id.as_str()).clone()
    }

    fn configure_syntax_processor(&mut self) {
        let cfg = self.config.syntax.get();
        self.syntax_processor = match cfg {
            EditorSyntaxConfig::None => None,
            EditorSyntaxConfig::SimpleJson => {
                RegexHighlightProcessor::json_default(SimpleJsonStyles::default())
                    .ok()
                    .map(SyntaxProcessor::Regex)
            }
            EditorSyntaxConfig::SimpleIni => {
                RegexHighlightProcessor::ini_default(SimpleIniStyles::default())
                    .ok()
                    .map(SyntaxProcessor::Regex)
            }
            EditorSyntaxConfig::Sublime {
                syntax_file,
                include_paths,
            } => {
                let mut set = SublimeSyntaxSet::new();
                for p in include_paths {
                    set.add_search_path(p);
                }
                match set.load_from_path(&syntax_file) {
                    Ok(syntax) => {
                        Some(SyntaxProcessor::Sublime(SublimeProcessor::new(syntax, set)))
                    }
                    Err(_) => None,
                }
            }
        };

        if let Some(processor) = self.syntax_processor.as_mut() {
            processor.apply(&mut self.state_manager);
        }
    }

    fn maybe_apply_syntax_highlighting(&mut self) {
        if self.lsp.is_some() {
            return;
        }
        if let Some(processor) = self.syntax_processor.as_mut() {
            processor.apply(&mut self.state_manager);
        }
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
        self.hover_due = None;
        self.hover_pending_request = None;
        self.hover_target_position = None;
        self.hover_requested_position = None;
        self.completion_pending_request = None;
        self.completion_requested_position = None;
        self.pending_goto = None;
    }

    fn ensure_viewport(&mut self, text_area: Rect) {
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
            self.adjust_scroll();
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
        let cursor_pos = editor.cursor_position();
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
        let pos = self.state_manager.editor().cursor_position();
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
        if let Some(lsp) = self.lsp.as_ref() {
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
        self.hover_due = None;
        self.hover_pending_request = None;
        self.hover_target_position = None;
        self.hover_requested_position = None;
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
        if let Some(lsp) = self.lsp.as_ref() {
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
        if let Some(lsp) = self.lsp.as_ref() {
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
        if let Some(lsp) = self.lsp.as_ref() {
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
        let insert_spaces = self.config.indent.insert_spaces.get();
        let tab_width = self.config.indent.tab_width.get().max(1);
        if insert_spaces {
            self.insert_text(&" ".repeat(tab_width));
        } else {
            self.insert_text("\t");
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
        let line = self.state_manager.editor().cursor_position().line;
        self.toggle_fold_at_line(line);
    }

    fn toggle_fold_at_line(&mut self, logical_line: usize) {
        let regions = self
            .state_manager
            .editor()
            .folding_manager
            .regions()
            .to_vec();
        let Some(region) = regions.iter().find(|r| r.start_line == logical_line) else {
            return;
        };

        if region.is_collapsed {
            let _ = self.execute(Command::Style(StyleCommand::Unfold {
                start_line: region.start_line,
            }));
        } else {
            let _ = self.execute(Command::Style(StyleCommand::Fold {
                start_line: region.start_line,
                end_line: region.end_line,
            }));
        }
    }

    fn unfold_all(&mut self) {
        let _ = self.execute(Command::Style(StyleCommand::UnfoldAll));
    }

    fn clear_secondary_selections(&mut self) {
        let _ = self.execute(Command::Cursor(CursorCommand::ClearSecondarySelections));
    }

    fn toggle_rect_selection(&mut self) {
        self.rect_selection_mode = !self.rect_selection_mode;
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
        if extend {
            let editor = self.state_manager.editor();
            let pos = editor.cursor_position();
            let target = Position::new(
                pos.line.saturating_add_signed(delta_line),
                pos.column.saturating_add_signed(delta_column),
            );
            let _ = self.execute(Command::Cursor(CursorCommand::ExtendSelection {
                to: target,
            }));
        } else {
            let _ = self.execute(Command::Cursor(CursorCommand::MoveBy {
                delta_line,
                delta_column,
            }));
            let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
        }
        self.adjust_scroll();
        self.hide_popups();
    }

    fn move_home_end(&mut self, end: bool, extend: bool) {
        let editor = self.state_manager.editor();
        let pos = editor.cursor_position();
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

        let semantic_legend = self.lsp.as_ref().and_then(|lsp| lsp.semantic_legend());

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
                if let Some(style) = style_for_sublime_scope(&theme, scope) {
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

    fn lsp_did_change(&mut self, change: LspContentChange) {
        let result = {
            let Some(lsp) = self.lsp.as_mut() else {
                return;
            };
            lsp.did_change(change)
        };

        if result.is_err() {
            self.lsp = None;
            editor_core_lsp::clear_lsp_state(&mut self.state_manager);
            self.maybe_apply_syntax_highlighting();
        }
    }

    fn maybe_poll_lsp(&mut self) {
        let poll_result = {
            let Some(lsp) = self.lsp.as_mut() else {
                return;
            };
            self.state_manager.apply_processor(lsp)
        };

        if poll_result.is_err() {
            self.lsp = None;
            editor_core_lsp::clear_lsp_state(&mut self.state_manager);
            self.maybe_apply_syntax_highlighting();
            return;
        }

        // Drain events (hover/completion/goto responses, UX messages, etc.)
        let Some(lsp) = self.lsp.as_mut() else {
            return;
        };
        for ev in lsp.drain_events() {
            let editor_core_lsp::LspEvent::Response(resp) = ev else {
                continue;
            };

            let method = resp.method;
            let id = resp.id;
            let result = resp.result;
            let error = resp.error;

            if let Some((pending_id, kind)) = self.pending_goto
                && pending_id == id {
                    let locs = result
                        .as_ref()
                        .map(locations_from_value)
                        .unwrap_or_default();
                    self.events.push(EditorEvent::LspGoto {
                        kind,
                        locations: locs,
                    });
                    self.pending_goto = None;
                }

            if let Some(pending_id) = self.hover_pending_request
                && pending_id == id && method.as_str() == "textDocument/hover" {
                    self.hover_pending_request = None;
                    self.hover_requested_position = None;
                    if error.is_some() {
                        self.hover_popup.set(None);
                    }
                    if let Some(result) = result.as_ref() {
                        self.handle_lsp_hover_response(result);
                    } else {
                        self.hover_popup.set(None);
                    }
                }

            if let Some(pending_id) = self.completion_pending_request
                && pending_id == id && method.as_str() == "textDocument/completion" {
                    self.completion_pending_request = None;
                    self.completion_requested_position = None;
                    if let Some(result) = result.as_ref() {
                        self.handle_lsp_completion_response(result);
                    } else {
                        self.completion_popup.set(None);
                    }
                }
        }
    }

    fn maybe_start_or_stop_lsp(&mut self) {
        if !self.config.lsp.check_dirty(&mut self.lsp_observer) {
            return;
        }

        match self.config.lsp.get() {
            EditorLspMode::Disabled => {
                self.lsp = None;
                editor_core_lsp::clear_lsp_state(&mut self.state_manager);
                self.maybe_apply_syntax_highlighting();
                self.hide_popups();
            }
            EditorLspMode::Enabled(cfg) => {
                // Best-effort restart on changes.
                self.lsp = None;
                editor_core_lsp::clear_lsp_state(&mut self.state_manager);
                self.hide_popups();
                self.start_lsp_session(cfg);
            }
        }
    }

    fn start_lsp_session(&mut self, cfg: super::config::EditorLspConfig) {
        if cfg.command.is_empty() {
            return;
        }

        let program = cfg.command[0].clone();
        let args = cfg.command.iter().skip(1).cloned().collect::<Vec<_>>();

        let mut cmd = ProcessCommand::new(&program);
        cmd.args(args);
        cmd.stderr(std::process::Stdio::null());

        let token_types = vec![
            "namespace",
            "type",
            "class",
            "enum",
            "interface",
            "struct",
            "typeParameter",
            "parameter",
            "variable",
            "property",
            "enumMember",
            "event",
            "function",
            "method",
            "macro",
            "keyword",
            "modifier",
            "comment",
            "string",
            "number",
            "regexp",
            "operator",
        ];

        let token_modifiers = vec![
            "declaration",
            "definition",
            "readonly",
            "static",
            "deprecated",
            "abstract",
            "async",
            "modification",
            "documentation",
            "defaultLibrary",
        ];

        let workspace_folders = cfg
            .workspace_folders
            .iter()
            .map(|uri| json!({ "uri": uri, "name": uri }))
            .collect::<Vec<_>>();

        let init_params = json!({
            "processId": std::process::id(),
            "rootUri": cfg.root_uri,
            "workspaceFolders": workspace_folders.clone(),
            "capabilities": {
                "workspace": {
                    "configuration": true,
                    "workspaceFolders": true,
                },
                "textDocument": {
                    "hover": { "dynamicRegistration": false },
                    "completion": {
                        "dynamicRegistration": false,
                        "completionItem": { "snippetSupport": false },
                    },
                    "semanticTokens": {
                        "dynamicRegistration": false,
                        "requests": { "range": false, "full": { "delta": false } },
                        "tokenTypes": token_types,
                        "tokenModifiers": token_modifiers,
                        "formats": ["relative"],
                        "multilineTokenSupport": true,
                        "overlappingTokenSupport": false,
                    },
                    "foldingRange": {
                        "dynamicRegistration": false,
                        "lineFoldingOnly": true,
                    },
                    "definition": { "dynamicRegistration": false },
                    "declaration": { "dynamicRegistration": false },
                    "typeDefinition": { "dynamicRegistration": false },
                    "implementation": { "dynamicRegistration": false },
                    "references": { "dynamicRegistration": false },
                },
            },
            "clientInfo": { "name": "chatty editor" },
        });

        let start = editor_core_lsp::LspSessionStartOptions {
            cmd,
            workspace_folders,
            initialize_params: init_params,
            initialize_timeout: cfg.initialize_timeout,
            document: editor_core_lsp::LspDocument {
                uri: cfg.document_uri.clone(),
                language_id: cfg.language_id.clone(),
                version: 1,
            },
            initial_text: self.state_manager.editor().get_text(),
        };

        if let Ok(mut session) = editor_core_lsp::LspSession::start(start) {
            session.set_auto_refresh_options(editor_core_lsp::editor::LspAutoRefreshOptions {
                semantic_tokens: cfg.semantic_tokens,
                folding_ranges: cfg.folding_ranges,
                delay: Duration::from_millis(150),
            });
            self.lsp = Some(session);
        }
    }

    fn handle_lsp_hover_response(&mut self, value: &serde_json::Value) {
        // Ignore stale responses.
        if self
            .hover_requested_position
            .is_some_and(|p| p != self.state_manager.editor().cursor_position())
        {
            return;
        }
        if self.completion_popup.get().is_some() {
            return;
        }

        // LSP hover: { contents, range? }
        let Some(contents) = value.get("contents") else {
            self.hover_popup.set(None);
            return;
        };

        let text = match hover_contents_to_plain_text(contents) {
            Some(lines) if !lines.is_empty() => lines,
            _ => {
                self.hover_popup.set(None);
                return;
            }
        };

        let Some(rect) = self.hover_popup_rect_for_cursor(text.as_slice()) else {
            self.hover_popup.set(None);
            return;
        };

        self.hover_popup.set(Some(HoverPopupModel {
            rect,
            contents: super::popup::LspHoverContents::PlainText(text),
        }));
    }

    fn handle_lsp_completion_response(&mut self, value: &serde_json::Value) {
        self.hide_hover_popup_only();
        if self
            .completion_requested_position
            .is_some_and(|p| p != self.state_manager.editor().cursor_position())
        {
            return;
        }

        // Completion: CompletionList { items } | CompletionItem[].
        let items_value = if let Some(items) = value.get("items") {
            items
        } else {
            value
        };

        let Some(arr) = items_value.as_array() else {
            self.completion_popup.set(None);
            return;
        };

        let max_items = self.config.completion.max_items.get().max(1);
        let mut items = Vec::new();
        for item in arr.iter().take(max_items) {
            let label = item
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if label.is_empty() {
                continue;
            }
            let detail = item
                .get("detail")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            items.push(super::popup::CompletionItem {
                label,
                detail,
                edit: LspCompletionItemEdit::Raw(item.clone()),
            });
        }

        if items.is_empty() {
            self.completion_popup.set(None);
            return;
        }

        let Some(rect) = self.completion_popup_rect_for_cursor(items.len()) else {
            self.completion_popup.set(None);
            return;
        };

        self.completion_popup.set(Some(CompletionPopupModel {
            rect,
            items,
            selected: 0,
            scroll: 0,
            accept: None,
        }));
    }

    fn hover_popup_rect_for_cursor(&self, lines: &[String]) -> Option<Rect> {
        let (cursor_x, cursor_y) = self.cursor_screen_position()??;
        let max_line = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        let width = (max_line + 2).clamp(10, 80) as u16;
        let height = (lines.len() + 2).clamp(3, 12) as u16;

        Some(Rect {
            x: cursor_x.saturating_add(1),
            y: cursor_y.saturating_add(1),
            width,
            height,
        })
    }

    fn completion_popup_rect_for_cursor(&self, item_count: usize) -> Option<Rect> {
        let (cursor_x, cursor_y) = self.cursor_screen_position()??;
        let height = (item_count.min(8) + 2).max(3) as u16;
        let width = 40u16;
        Some(Rect {
            x: cursor_x.saturating_add(1),
            y: cursor_y.saturating_add(1),
            width,
            height,
        })
    }

    fn cursor_screen_position(&self) -> Option<Option<(u16, u16)>> {
        let area = self.last_area?;
        let (_gutter, text_area) = self.layout_rects(area);
        if text_area.width == 0 || text_area.height == 0 {
            return Some(None);
        }

        let editor = self.state_manager.editor();
        let pos = editor.cursor_position();
        let Some((cursor_visual_row, cursor_x_in_row)) =
            editor.logical_position_to_visual_allow_virtual(pos.line, pos.column)
        else {
            return Some(None);
        };
        let scroll_top = self.state_manager.get_viewport_state().scroll_top;
        if cursor_visual_row < scroll_top {
            return Some(None);
        }
        let y = cursor_visual_row.saturating_sub(scroll_top);
        if y >= text_area.height as usize {
            return Some(None);
        }
        let x = cursor_x_in_row.min(text_area.width.saturating_sub(1) as usize) as u16;
        Some(Some((
            text_area.x.saturating_add(x),
            text_area.y.saturating_add(y as u16),
        )))
    }

    fn layout_rects(&self, area: Rect) -> (Rect, Rect) {
        let show_line_numbers = self.config.show_line_numbers.get();
        let show_folding_markers = self.config.show_folding_markers.get();

        let line_count = self.state_manager.editor().line_index.line_count().max(1);
        let digits = line_count.to_string().len().max(2) as u16;

        let mut gutter_w = 0u16;
        if show_line_numbers {
            gutter_w = gutter_w.saturating_add(digits.saturating_add(1));
        }
        if show_folding_markers {
            gutter_w = gutter_w.saturating_add(2);
        }

        // Add a separator if there is any gutter at all.
        let sep_w = if gutter_w > 0 { 1 } else { 0 };
        let gutter_total = gutter_w.saturating_add(sep_w).min(area.width);

        let gutter = Rect {
            x: area.x,
            y: area.y,
            width: gutter_total,
            height: area.height,
        };
        let text = Rect {
            x: area.x.saturating_add(gutter_total),
            y: area.y,
            width: area.width.saturating_sub(gutter_total),
            height: area.height,
        };
        (gutter, text)
    }

    fn handle_action(&mut self, action: EditorAction) -> bool {
        match action {
            EditorAction::Undo => {
                self.hide_popups();
                let old_char_count = self.state_manager.editor().char_count();
                let full_lsp_change = self.lsp.as_ref().map(|lsp| {
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
                let full_lsp_change = self.lsp.as_ref().map(|lsp| {
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
                if self.hover_popup.get().is_some() {
                    self.hover_popup.set(None);
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

    fn request_hover_now(&mut self) {
        let Some(lsp) = self.lsp.as_mut() else {
            return;
        };
        let pos = self.state_manager.editor().cursor_position();
        if let Ok(id) = lsp.request_hover(
            &self.state_manager.editor().line_index,
            pos.line,
            pos.column,
        ) {
            self.hover_pending_request = Some(id);
            self.hover_requested_position = Some(pos);
        }
    }

    fn request_completion_now(&mut self) {
        if !self.config.completion.enabled.get() {
            return;
        }
        let Some(lsp) = self.lsp.as_mut() else {
            return;
        };
        let pos = self.state_manager.editor().cursor_position();
        if let Ok(id) = lsp.request_completion(
            &self.state_manager.editor().line_index,
            pos.line,
            pos.column,
        ) {
            self.completion_pending_request = Some(id);
            self.completion_requested_position = Some(pos);
        }
    }

    fn request_goto(&mut self, kind: EditorLspGotoKind) {
        let Some(lsp) = self.lsp.as_mut() else {
            return;
        };
        let pos = self.state_manager.editor().cursor_position();
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
            self.pending_goto = Some((id, kind));
        }
    }

    fn schedule_hover_after_delay(&mut self) {
        if self.hover_popup.get().is_some() {
            return;
        }
        if self.completion_pending_request.is_some() {
            self.hover_due = None;
            self.hover_target_position = None;
            return;
        }
        if self.completion_popup.get().is_some() {
            self.hover_due = None;
            self.hover_target_position = None;
            return;
        }
        if !self.config.hover.enabled.get() {
            self.hover_due = None;
            self.hover_target_position = None;
            return;
        }
        if self.lsp.is_none() {
            self.hover_due = None;
            self.hover_target_position = None;
            return;
        }
        if self.hover_pending_request.is_some() {
            return;
        }

        let delay = self.config.hover.delay.get();
        self.hover_due = Some(Instant::now() + delay);
        self.hover_target_position = Some(self.state_manager.editor().cursor_position());
    }

    fn maybe_fire_hover(&mut self) {
        let Some(due) = self.hover_due else {
            return;
        };
        if Instant::now() < due {
            return;
        }
        if self.hover_pending_request.is_some() {
            return;
        }
        if self.completion_pending_request.is_some() {
            return;
        }
        if self.hover_popup.get().is_some() {
            self.hover_due = None;
            self.hover_target_position = None;
            return;
        }
        if self.completion_popup.get().is_some() {
            return;
        }

        let pos = self.state_manager.editor().cursor_position();
        if self.hover_target_position != Some(pos) {
            self.schedule_hover_after_delay();
            return;
        }

        self.hover_due = None;
        self.hover_target_position = None;
        self.request_hover_now();
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
            let full_lsp_change = self.lsp.as_ref().map(|lsp| {
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

    fn handle_mouse(&mut self, m: MouseEvent) -> ViewEventResult {
        let Some(area) = self.last_area else {
            return ViewEventResult::ignored();
        };
        let Some((local_x, local_y)) = mouse_coords_local_to_area(area, m) else {
            self.mouse_drag = None;
            return ViewEventResult::ignored();
        };

        let (gutter, text_area) = self.layout_rects(Rect {
            x: 0,
            y: 0,
            width: area.width,
            height: area.height,
        });

        let show_line_numbers = self.config.show_line_numbers.get();
        let show_folding_markers = self.config.show_folding_markers.get();

        // Folding marker hit testing (gutter-local coordinates).
        if matches!(m.kind, MouseEventKind::Down(MouseButton::Left))
            && show_folding_markers
            && gutter.width > 0
            && local_x < gutter.width.saturating_sub(1)
        {
            let mut x = 0u16;
            let line_count = self.state_manager.editor().line_index.line_count().max(1);
            let digits = line_count.to_string().len().max(2) as u16;
            if show_line_numbers {
                x = x.saturating_add(digits.saturating_add(1));
            }
            // Fold marker column is at x (0-based), plus a space at x+1.
            if local_x == x {
                let scroll_top = self.state_manager.get_viewport_state().scroll_top;
                let visual_row = scroll_top.saturating_add(local_y as usize);
                let (logical_line, visual_in_line) = self
                    .state_manager
                    .editor()
                    .visual_to_logical_line(visual_row);
                if visual_in_line == 0 {
                    self.toggle_fold_at_line(logical_line);
                    return ViewEventResult::consumed();
                }
            }
        }

        // Only handle mouse events within the text area.
        if local_x < text_area.x || local_y < text_area.y {
            self.mouse_drag = None;
            return ViewEventResult::ignored();
        }
        let x_in_text = local_x.saturating_sub(text_area.x);
        let y_in_text = local_y.saturating_sub(text_area.y);
        if x_in_text >= text_area.width || y_in_text >= text_area.height {
            self.mouse_drag = None;
            return ViewEventResult::ignored();
        }

        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.hide_popups();
                let pos = self.position_at_text_point(x_in_text, y_in_text);

                let is_shift = m.modifiers.contains(KeyModifiers::SHIFT);
                let is_alt = m.modifiers.contains(KeyModifiers::ALT);

                if is_alt {
                    self.add_secondary_cursor(pos);
                } else if self.rect_selection_mode {
                    let _ = self.execute(Command::Cursor(CursorCommand::SetRectSelection {
                        anchor: pos,
                        active: pos,
                    }));
                } else if is_shift {
                    let _ =
                        self.execute(Command::Cursor(CursorCommand::ExtendSelection { to: pos }));
                } else {
                    let _ = self.execute(Command::Cursor(CursorCommand::MoveTo {
                        line: pos.line,
                        column: pos.column,
                    }));
                    let _ = self.execute(Command::Cursor(CursorCommand::ClearSelection));
                    let _ = self.execute(Command::Cursor(CursorCommand::ClearSecondarySelections));
                }

                self.adjust_scroll();
                self.mouse_drag = Some(MouseDrag {
                    anchor: pos,
                    rect: self.rect_selection_mode,
                });
                ViewEventResult::consumed()
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(drag) = self.mouse_drag else {
                    return ViewEventResult::ignored();
                };
                let pos = self.position_at_text_point(x_in_text, y_in_text);

                if drag.rect {
                    let _ = self.execute(Command::Cursor(CursorCommand::SetRectSelection {
                        anchor: drag.anchor,
                        active: pos,
                    }));
                } else {
                    let _ = self.execute(Command::Cursor(CursorCommand::SetSelection {
                        start: drag.anchor,
                        end: pos,
                    }));
                }
                self.adjust_scroll();
                ViewEventResult::consumed()
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.mouse_drag = None;
                ViewEventResult::consumed()
            }
            MouseEventKind::ScrollUp => {
                let step = self.scroll_config().wheel_step.max(1) as isize;
                self.scroll_by_rows(-step);
                ViewEventResult::consumed()
            }
            MouseEventKind::ScrollDown => {
                let step = self.scroll_config().wheel_step.max(1) as isize;
                self.scroll_by_rows(step);
                ViewEventResult::consumed()
            }
            _ => ViewEventResult::ignored(),
        }
    }

    fn add_secondary_cursor(&mut self, pos: Position) {
        let cursor_state = self.state_manager.get_cursor_state();
        let mut selections = cursor_state.selections;

        // Deduplicate.
        if selections.iter().any(|s| s.start == pos && s.end == pos) {
            return;
        }

        selections.push(Selection {
            start: pos,
            end: pos,
            direction: SelectionDirection::Forward,
        });

        let _ = self.execute(Command::Cursor(CursorCommand::SetSelections {
            selections,
            primary_index: cursor_state.primary_selection_index,
        }));
    }

    fn position_at_text_point(&self, x: u16, y: u16) -> Position {
        let editor = self.state_manager.editor();
        let scroll_top = self.state_manager.get_viewport_state().scroll_top;
        let visual_row = scroll_top.saturating_add(y as usize);
        let (logical_line, visual_in_line) = editor.visual_to_logical_line(visual_row);

        let Some(layout) = editor.layout_engine.get_line_layout(logical_line) else {
            return Position::new(logical_line, x as usize);
        };

        let line_text = editor
            .line_index
            .get_line_text(logical_line)
            .unwrap_or_default();
        let line_char_len = line_text.chars().count();

        let segment_start_col = if visual_in_line == 0 {
            0
        } else {
            layout
                .wrap_points
                .get(visual_in_line - 1)
                .map(|wp| wp.char_index)
                .unwrap_or(0)
                .min(line_char_len)
        };

        let segment_end_col = layout
            .wrap_points
            .get(visual_in_line)
            .map(|wp| wp.char_index)
            .unwrap_or(line_char_len)
            .min(line_char_len);

        let mut col = segment_start_col;
        let mut cur_x = 0usize;
        for ch in line_text
            .chars()
            .skip(segment_start_col)
            .take(segment_end_col - segment_start_col)
        {
            let w = char_width(ch).max(1);
            if cur_x + w > x as usize {
                break;
            }
            cur_x += w;
            col += 1;
            if cur_x == x as usize {
                break;
            }
        }

        Position::new(logical_line, col.min(segment_end_col))
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.last_area = Some(area);

        let theme = self.editor_theme();
        frame.render_widget(Block::default().style(theme.background), area);

        let (gutter_area, text_area) = self.layout_rects(area);
        if text_area.width == 0 || text_area.height == 0 || area.height == 0 || area.width == 0 {
            return;
        }

        self.ensure_viewport(text_area);

        // Render gutter (line numbers + folding markers).
        if gutter_area.width > 0 {
            self.render_gutter(frame, gutter_area, &theme);
        }

        // Render text viewport.
        self.render_text(frame, text_area, ctx.is_focused, &theme);
    }

    fn render_gutter(&self, frame: &mut Frame<'_>, area: Rect, theme: &EditorTheme) {
        let show_line_numbers = self.config.show_line_numbers.get();
        let show_folding_markers = self.config.show_folding_markers.get();
        if !show_line_numbers && !show_folding_markers {
            return;
        }

        let editor = self.state_manager.editor();
        let cursor_line = editor.cursor_position().line;
        let scroll_top = self.state_manager.get_viewport_state().scroll_top;

        let line_count = editor.line_index.line_count().max(1);
        let digits = line_count.to_string().len().max(2);

        let mut fold_regions_by_start =
            std::collections::HashMap::<usize, editor_core::intervals::FoldRegion>::new();
        for r in editor.folding_manager.regions() {
            fold_regions_by_start
                .entry(r.start_line)
                .or_insert_with(|| r.clone());
        }

        let mut lines = Vec::<Line<'static>>::with_capacity(area.height as usize);
        for row in 0..(area.height as usize) {
            let visual_row = scroll_top.saturating_add(row);
            if visual_row >= editor.visual_line_count() {
                lines.push(Line::from(""));
                continue;
            }

            let (logical_line, visual_in_line) = editor.visual_to_logical_line(visual_row);
            let is_wrapped = visual_in_line > 0;

            let active = logical_line == cursor_line && !is_wrapped;
            let gutter_style = if active {
                theme.gutter_active
            } else {
                theme.gutter
            };

            let mut s = String::new();

            if show_line_numbers {
                if is_wrapped {
                    s.push_str(&" ".repeat(digits + 1));
                } else {
                    s.push_str(&format!("{:>width$} ", logical_line + 1, width = digits));
                }
            }

            if show_folding_markers {
                if is_wrapped {
                    s.push_str("  ");
                } else if let Some(region) = fold_regions_by_start.get(&logical_line) {
                    if region.is_collapsed {
                        s.push('▶');
                    } else {
                        s.push('▼');
                    }
                    s.push(' ');
                } else {
                    s.push_str("  ");
                }
            }

            // Separator at the end of gutter (if any).
            if area.width > 0 {
                // Ensure the separator is present even if gutter content is shorter.
                let expected_w = area.width.saturating_sub(1) as usize;
                if s.chars().count() < expected_w {
                    s.push_str(&" ".repeat(expected_w - s.chars().count()));
                }
                s.push('│');
            }

            lines.push(Line::from(Span::styled(s, gutter_style)));
        }

        frame.render_widget(Paragraph::new(lines).style(theme.gutter), area);
    }

    fn render_text(&self, frame: &mut Frame<'_>, area: Rect, focused: bool, theme: &EditorTheme) {
        let editor = self.state_manager.editor();
        let scroll_top = self.state_manager.get_viewport_state().scroll_top;
        let total_visual = editor.visual_line_count();

        let cursor_state = self.state_manager.get_cursor_state();
        let selections = cursor_state.selections;

        let grid = self
            .state_manager
            .get_viewport_content_styled(scroll_top, area.height as usize);

        let mut display_lines = Vec::<Line<'static>>::with_capacity(area.height as usize);

        for i in 0..(area.height as usize) {
            if area.width == 0 {
                display_lines.push(Line::from(""));
                continue;
            }
            let visual_row = scroll_top + i;
            if visual_row >= total_visual {
                display_lines.push(Line::from(""));
                continue;
            }

            let (logical_line, visual_in_line) = editor.visual_to_logical_line(visual_row);
            let Some(layout) = editor.layout_engine.get_line_layout(logical_line) else {
                display_lines.push(Line::from(""));
                continue;
            };

            let line_text = editor
                .line_index
                .get_line_text(logical_line)
                .unwrap_or_default();
            let line_char_len = line_text.chars().count();

            let segment_start_col = if visual_in_line == 0 {
                0
            } else {
                layout
                    .wrap_points
                    .get(visual_in_line - 1)
                    .map(|wp| wp.char_index)
                    .unwrap_or(0)
                    .min(line_char_len)
            };

            let mut selection_ranges: Vec<(usize, usize)> = Vec::new();
            for selection in &selections {
                if selection.start == selection.end {
                    continue;
                }

                let (sel_start, sel_end) = if selection.start <= selection.end {
                    (selection.start, selection.end)
                } else {
                    (selection.end, selection.start)
                };

                if logical_line < sel_start.line || logical_line > sel_end.line {
                    continue;
                }

                let start_col = if logical_line == sel_start.line {
                    sel_start.column.min(line_char_len)
                } else {
                    0
                };
                let end_col = if logical_line == sel_end.line {
                    sel_end.column.min(line_char_len)
                } else {
                    line_char_len
                };
                if start_col < end_col {
                    selection_ranges.push((start_col, end_col));
                }
            }

            let Some(headless_line) = grid.lines.get(i) else {
                display_lines.push(Line::from(""));
                continue;
            };
            if headless_line.cells.is_empty() {
                display_lines.push(Line::from(""));
                continue;
            }

            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut current_style: Option<Style> = None;
            let mut buffer = String::new();

            for (cell_idx, cell) in headless_line.cells.iter().enumerate() {
                let col = segment_start_col + cell_idx;
                let mut style = self.style_for_style_ids(&cell.styles);
                let is_selected = selection_ranges
                    .iter()
                    .any(|(start, end)| col >= *start && col < *end);
                if is_selected {
                    style = theme.selection;
                }

                if current_style.is_none() {
                    current_style = Some(style);
                }
                if current_style != Some(style) {
                    spans.push(Span::styled(
                        std::mem::take(&mut buffer),
                        current_style.unwrap_or(theme.text),
                    ));
                    current_style = Some(style);
                }
                buffer.push(cell.ch);
            }

            if !buffer.is_empty() {
                spans.push(Span::styled(buffer, current_style.unwrap_or(theme.text)));
            }
            display_lines.push(Line::from(spans));
        }

        frame.render_widget(Paragraph::new(display_lines).style(theme.text), area);

        if focused
            && let Some(Some((cursor_x, cursor_y))) = self.cursor_screen_position() {
                frame.set_cursor_position((cursor_x, cursor_y));
            }
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> ViewEventResult {
        let Some(chord) = KeyChord::from_key_event(key) else {
            return ViewEventResult::ignored();
        };

        // Completion popup keyboard navigation/accept (editor keeps focus, popup stays non-modal).
        if let Some(popup) = self.completion_popup.get() {
            match key.code {
                KeyCode::Esc => {
                    self.completion_popup.set(None);
                    return ViewEventResult::consumed();
                }
                KeyCode::Enter => {
                    if popup.selected < popup.items.len() {
                        let mut popup = popup;
                        popup.accept = Some(popup.selected);
                        self.completion_popup.set(Some(popup));
                        self.process_completion_accept();
                    } else {
                        self.completion_popup.set(None);
                    }
                    return ViewEventResult::consumed();
                }
                KeyCode::Up => {
                    self.select_completion_relative(-1);
                    return ViewEventResult::consumed();
                }
                KeyCode::Down => {
                    self.select_completion_relative(1);
                    return ViewEventResult::consumed();
                }
                KeyCode::PageUp => {
                    self.select_completion_relative(-5);
                    return ViewEventResult::consumed();
                }
                KeyCode::PageDown => {
                    self.select_completion_relative(5);
                    return ViewEventResult::consumed();
                }
                _ => {
                    // Any other key dismisses completion; the key is then handled normally.
                    self.completion_popup.set(None);
                }
            }
        }

        let keymap: EditorKeymap = self.config.keymap.get();
        if let Some(action) = keymap.get(chord) {
            let consumed = self.handle_action(action);
            return if consumed {
                ViewEventResult::consumed()
            } else {
                ViewEventResult::ignored()
            };
        }

        // Default text insertion: Char(c) without Ctrl/Alt.
        match key.code {
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.insert_text(&c.to_string());
                self.adjust_scroll();
                return ViewEventResult::consumed();
            }
            _ => {}
        }

        ViewEventResult::ignored()
    }

    fn select_completion_relative(&mut self, delta: isize) {
        let Some(mut popup) = self.completion_popup.get() else {
            return;
        };
        if popup.items.is_empty() {
            return;
        }
        let len = popup.items.len() as isize;
        let mut selected = popup.selected as isize + delta;
        if selected < 0 {
            selected = 0;
        }
        if selected >= len {
            selected = len - 1;
        }
        popup.selected = selected as usize;

        // Ensure selected is within visible window.
        let visible = popup.rect.height.saturating_sub(2) as usize;
        if visible > 0 {
            if popup.selected < popup.scroll {
                popup.scroll = popup.selected;
            } else if popup.selected >= popup.scroll + visible {
                popup.scroll = popup.selected.saturating_sub(visible.saturating_sub(1));
            }
        }

        self.completion_popup.set(Some(popup));
    }
}

impl View for EditorView {
    fn is_focusable(&self) -> bool {
        true
    }

    fn min_width(&self) -> u16 {
        8
    }

    fn min_height(&self) -> u16 {
        3
    }

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

    fn handle_event(&mut self, event: &Event, ctx: ViewContext<'_>) -> ViewEventResult {
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
            return ViewEventResult::ignored();
        }

        // Any interaction should dismiss hover immediately.
        self.hide_hover_popup_only();

        // Apply completion accept queued by mouse (from tooltip popup window).
        self.process_completion_accept();

        let res = match event {
            Event::Paste(text) => {
                self.insert_text(text);
                self.adjust_scroll();
                ViewEventResult::consumed()
            }
            Event::Key(key) => self.handle_key_event(*key),
            Event::Mouse(m) => self.handle_mouse(*m),
            _ => ViewEventResult::ignored(),
        };

        // Schedule hover for when the user goes idle again.
        self.schedule_hover_after_delay();

        res
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        // Sync external text changes and config at draw time too (tick-driven apps).
        self.sync_external_text_if_dirty();
        if self.config.syntax.check_dirty(&mut self.syntax_observer) {
            self.configure_syntax_processor();
        }
        self.maybe_start_or_stop_lsp();

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

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn mouse_coords_local_to_area(area: Rect, m: MouseEvent) -> Option<(u16, u16)> {
    if contains(area, m.column, m.row) {
        return Some((
            m.column.saturating_sub(area.x),
            m.row.saturating_sub(area.y),
        ));
    }

    // Nested containers may forward mouse coordinates already relative to this view.
    if m.column < area.width && m.row < area.height {
        return Some((m.column, m.row));
    }

    None
}

fn style_for_sublime_scope(theme: &EditorTheme, scope: &str) -> Option<Style> {
    let trimmed = scope.trim();
    if trimmed.is_empty() {
        return None;
    }

    // A scope string can contain multiple scopes separated by spaces.
    for scope in trimmed.split_whitespace() {
        let mut candidate = scope;
        loop {
            if let Some(style) = theme.sublime_scopes.get(candidate) {
                return Some(*style);
            }

            let Some((parent, _tail)) = candidate.rsplit_once('.') else {
                break;
            };
            candidate = parent;
        }
    }

    None
}

fn hover_contents_to_plain_text(contents: &serde_json::Value) -> Option<Vec<String>> {
    // Spec: `contents` can be MarkedString | MarkedString[] | MarkupContent.
    if let Some(s) = contents.as_str() {
        return Some(s.lines().map(|l| l.to_string()).collect());
    }

    if let Some(obj) = contents.as_object() {
        // MarkupContent: { kind: "markdown" | "plaintext", value: "..."}
        if let Some(value) = obj.get("value").and_then(|v| v.as_str()) {
            return Some(value.lines().map(|l| l.to_string()).collect());
        }
        // MarkedString: { language, value }
        if let Some(value) = obj.get("value").and_then(|v| v.as_str()) {
            return Some(value.lines().map(|l| l.to_string()).collect());
        }
    }

    if let Some(arr) = contents.as_array() {
        let mut out = Vec::<String>::new();
        for item in arr {
            if let Some(lines) = hover_contents_to_plain_text(item) {
                out.extend(lines);
            }
        }
        return Some(out);
    }

    None
}
