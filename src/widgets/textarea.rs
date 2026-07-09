//! Multi-line text input widget with history navigation and a small kill-ring.

use std::cmp::Ordering;
use std::ops::Range;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::composable::{
    Component, ComponentContext, EventHandling, EventResult, FocusNav, Layout,
};
use crate::reactive::Binding;
use crate::runtime::{CallbackHandle, ComponentValue};
use crate::text::TextBuffer;
use atto_ui_macros::{ComponentProperties, component_properties};

use super::util::{mouse_coords_local_to_area, widget_style};

#[derive(Clone, Debug, ComponentProperties)]
pub struct TextArea {
    title: Binding<String>,
    placeholder: Option<Binding<String>>,
    buffer: TextBuffer,
    #[component(rename = "text")]
    binding: Binding<String>,
    enabled: Binding<bool>,
    clipboard: Binding<String>,
    kill_ring: Binding<String>,
    history: Binding<Vec<String>>,
    height: Binding<u16>,
    enter_submits: Binding<bool>,
    scroll_row: u16,
    scroll_col: u16,
    preferred_col: Option<u16>,
    history_pos: Option<usize>,
    history_draft: Option<String>,
    last_area: Option<Rect>,
    on_change_callback: Option<CallbackHandle>,
    on_submit_callback: Option<CallbackHandle>,
}

impl TextArea {
    pub fn new(title: impl Into<Binding<String>>, binding: Binding<String>) -> Self {
        let initial = binding.get();
        Self {
            title: title.into(),
            placeholder: None,
            buffer: TextBuffer::with_text(initial),
            binding,
            enabled: true.into(),
            clipboard: String::new().into(),
            kill_ring: String::new().into(),
            history: Vec::<String>::new().into(),
            height: 5u16.into(),
            enter_submits: false.into(),
            scroll_row: 0,
            scroll_col: 0,
            preferred_col: None,
            history_pos: None,
            history_draft: None,
            last_area: None,
            on_change_callback: None,
            on_submit_callback: None,
        }
    }

    pub fn title(mut self, title: impl Into<Binding<String>>) -> Self {
        self.title = title.into();
        self
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<Binding<String>>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn clipboard(mut self, clipboard: impl Into<Binding<String>>) -> Self {
        self.clipboard = clipboard.into();
        self
    }

    pub fn kill_ring(mut self, kill_ring: impl Into<Binding<String>>) -> Self {
        self.kill_ring = kill_ring.into();
        self
    }

    pub fn history(mut self, history: impl Into<Binding<Vec<String>>>) -> Self {
        self.history = history.into();
        self
    }

    pub fn height(mut self, height: impl Into<Binding<u16>>) -> Self {
        self.height = height.into();
        self
    }

    pub fn enter_submits(mut self, enter_submits: impl Into<Binding<bool>>) -> Self {
        self.enter_submits = enter_submits.into();
        self
    }

    pub fn on_change_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_change_callback = Some(callback);
        self
    }

    pub fn on_submit_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_submit_callback = Some(callback);
        self
    }

    /// Returns the current cursor byte index in the underlying UTF-8 text.
    pub fn cursor_byte_index(&self) -> usize {
        self.buffer.cursor_byte_index()
    }

    /// Moves the cursor to the closest grapheme boundary at or before the given byte index.
    pub fn set_cursor_byte_index(&mut self, byte_index: usize) {
        self.sync_external_binding();
        self.buffer.set_cursor_byte_index(byte_index);
        self.preferred_col = None;
    }

    /// Replaces a UTF-8 byte range and places the cursor after the inserted text.
    pub fn replace_byte_range(&mut self, range: Range<usize>, replacement: &str) -> EventResult {
        self.sync_external_binding();
        let text = self.buffer.text().to_string();
        let start = self.buffer.align_to_grapheme_boundary(range.start);
        let end = self
            .buffer
            .align_to_grapheme_boundary(range.end)
            .min(text.len());

        if start > end {
            return EventResult::ignored();
        }

        let mut next = text;
        next.replace_range(start..end, replacement);
        let cursor = start.saturating_add(replacement.len());
        self.buffer.set_text(next);
        self.buffer.set_cursor_byte_index(cursor);
        self.after_edit();
        self.sync_binding_from_buffer();
        EventResult::changed()
    }

    fn emit_change(&self) {
        if let Some(cb) = &self.on_change_callback {
            cb.emit_with(Some(ComponentValue::String(self.binding.get())));
        }
    }

    fn emit_submit(&self) {
        if let Some(cb) = &self.on_submit_callback {
            cb.emit();
        }
    }

    fn sync_binding_from_buffer(&mut self) {
        self.binding.set(self.buffer.text().to_string());
        self.emit_change();
    }

    fn sync_external_binding(&mut self) {
        let external = self.binding.get();
        if external != self.buffer.text() {
            self.buffer.set_text(external);
            self.scroll_row = 0;
            self.scroll_col = 0;
            self.preferred_col = None;
            self.history_pos = None;
            self.history_draft = None;
        }
    }
}

#[component_properties]
impl Component for TextArea {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.height == 0 || area.width == 0 {
            return;
        }

        self.sync_external_binding();
        let enabled = self.enabled.get();
        let style = widget_style(ctx.theme, enabled, ctx.is_focused);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(ctx.theme.border_set(false))
            .title(self.title.get());
        frame.render_widget(block.border_style(style), area);

        let inner = Rect {
            x: area.x.saturating_add(1),
            y: area.y.saturating_add(1),
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let (cursor_line, cursor_col) = self.cursor_line_col();
        self.adjust_scroll(cursor_line, cursor_col, inner);
        self.draw_lines(frame, inner, style, ctx);

        if ctx.is_focused {
            let x = inner.x.saturating_add(
                cursor_col
                    .saturating_sub(self.scroll_col)
                    .min(inner.width - 1),
            );
            let y = inner.y.saturating_add(
                (cursor_line as u16)
                    .saturating_sub(self.scroll_row)
                    .min(inner.height - 1),
            );
            frame.set_cursor_position((x, y));
        }
    }
}

impl Layout for TextArea {
    fn min_width(&self) -> u16 {
        4
    }

    fn min_height(&self) -> u16 {
        3
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.height.get().max(3))
    }
}

impl FocusNav for TextArea {
    fn is_focusable(&self) -> bool {
        self.enabled.get()
    }
}

impl EventHandling for TextArea {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }

        match event {
            Event::Mouse(m) => {
                let Some(area) = self.last_area else {
                    return EventResult::ignored();
                };
                let Some((local_x, local_y)) =
                    mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
                else {
                    return EventResult::ignored();
                };
                let inner = Rect {
                    x: 1,
                    y: 1,
                    width: area.width.saturating_sub(2),
                    height: area.height.saturating_sub(2),
                };
                if inner.width == 0
                    || inner.height == 0
                    || local_x < inner.x
                    || local_y < inner.y
                    || local_x >= inner.x.saturating_add(inner.width)
                    || local_y >= inner.y.saturating_add(inner.height)
                {
                    return EventResult::ignored();
                }

                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let line = self
                            .scroll_row
                            .saturating_add(local_y.saturating_sub(inner.y))
                            as usize;
                        let col = self
                            .scroll_col
                            .saturating_add(local_x.saturating_sub(inner.x));
                        self.set_cursor_line_col(line, col);
                        self.preferred_col = None;
                        self.history_pos = None;
                        self.history_draft = None;
                        EventResult::consumed()
                    }
                    _ => EventResult::ignored(),
                }
            }
            Event::Paste(s) => self.insert_text(s),
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) => {
                if matches!(kind, KeyEventKind::Release) {
                    return EventResult::ignored();
                }

                let mods = *modifiers;
                match code {
                    KeyCode::Char('j') if mods.contains(KeyModifiers::CONTROL) => {
                        self.insert_newline()
                    }
                    KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => {
                        self.kill_before_cursor()
                    }
                    KeyCode::Char('k') if mods.contains(KeyModifiers::CONTROL) => {
                        self.kill_after_cursor()
                    }
                    KeyCode::Char('y') if mods.contains(KeyModifiers::CONTROL) => {
                        let killed = self.kill_ring.get();
                        if killed.is_empty() {
                            EventResult::ignored()
                        } else {
                            self.insert_text(&killed)
                        }
                    }
                    KeyCode::Char('v') if mods.contains(KeyModifiers::CONTROL) => {
                        let text = self.clipboard.get();
                        if text.is_empty() {
                            EventResult::ignored()
                        } else {
                            self.insert_text(&text)
                        }
                    }
                    KeyCode::Char('a') if mods.contains(KeyModifiers::CONTROL) => {
                        self.buffer.set_cursor_byte_index(self.current_line_start());
                        self.preferred_col = None;
                        EventResult::consumed()
                    }
                    KeyCode::Char('e') if mods.contains(KeyModifiers::CONTROL) => {
                        self.buffer.set_cursor_byte_index(self.current_line_end());
                        self.preferred_col = None;
                        EventResult::consumed()
                    }
                    KeyCode::Backspace => {
                        self.buffer.backspace();
                        self.after_edit();
                        self.sync_binding_from_buffer();
                        EventResult::changed()
                    }
                    KeyCode::Delete => {
                        self.buffer.delete();
                        self.after_edit();
                        self.sync_binding_from_buffer();
                        EventResult::changed()
                    }
                    KeyCode::Left => {
                        self.buffer.move_left();
                        self.preferred_col = None;
                        EventResult::consumed()
                    }
                    KeyCode::Right => {
                        self.buffer.move_right();
                        self.preferred_col = None;
                        EventResult::consumed()
                    }
                    KeyCode::Up if !mods.contains(KeyModifiers::CONTROL) => {
                        self.move_up_or_history()
                    }
                    KeyCode::Down if !mods.contains(KeyModifiers::CONTROL) => {
                        self.move_down_or_history()
                    }
                    KeyCode::Home => {
                        self.buffer.set_cursor_byte_index(self.current_line_start());
                        self.preferred_col = None;
                        EventResult::consumed()
                    }
                    KeyCode::End => {
                        self.buffer.set_cursor_byte_index(self.current_line_end());
                        self.preferred_col = None;
                        EventResult::consumed()
                    }
                    KeyCode::Enter if mods.contains(KeyModifiers::SHIFT) => self.insert_newline(),
                    KeyCode::Enter if self.enter_submits.get() && mods.is_empty() => {
                        self.commit_history_entry();
                        self.history_pos = None;
                        self.history_draft = None;
                        self.emit_submit();
                        EventResult::submitted()
                    }
                    KeyCode::Enter if mods.is_empty() => self.insert_newline(),
                    KeyCode::Char(c)
                        if !mods.contains(KeyModifiers::CONTROL)
                            && !mods.contains(KeyModifiers::ALT) =>
                    {
                        self.buffer.insert_char(*c);
                        self.after_edit();
                        self.sync_binding_from_buffer();
                        EventResult::changed()
                    }
                    _ => EventResult::ignored(),
                }
            }
            _ => EventResult::ignored(),
        }
    }
}

crate::impl_component_default_traits!(TextArea => Scrollable, DynamicTree);

impl TextArea {
    fn draw_lines(
        &self,
        frame: &mut Frame<'_>,
        inner: Rect,
        style: ratatui::style::Style,
        ctx: ComponentContext<'_>,
    ) {
        let text = self.buffer.text();
        let ranges = line_ranges(text);
        let placeholder = self
            .placeholder
            .as_ref()
            .map(|binding| binding.get())
            .filter(|s| !s.is_empty());

        for row in 0..inner.height {
            let line_idx = self.scroll_row.saturating_add(row) as usize;
            let row_area = Rect {
                x: inner.x,
                y: inner.y.saturating_add(row),
                width: inner.width,
                height: 1,
            };
            let line = if text.is_empty() && row == 0 {
                if let Some(ph) = placeholder.as_ref() {
                    Line::from(vec![Span::styled(
                        slice_by_width(ph, self.scroll_col, inner.width),
                        ctx.theme.widget.dim,
                    )])
                } else {
                    Line::raw("")
                }
            } else if let Some((start, end)) = ranges.get(line_idx).copied() {
                build_visible_line(&text[start..end], self.scroll_col, inner.width, style)
            } else {
                Line::raw("")
            };
            frame.render_widget(Paragraph::new(line).style(style), row_area);
        }
    }

    fn adjust_scroll(&mut self, cursor_line: usize, cursor_col: u16, inner: Rect) {
        let cursor_row = cursor_line.min(u16::MAX as usize) as u16;
        if cursor_row < self.scroll_row {
            self.scroll_row = cursor_row;
        } else if cursor_row >= self.scroll_row.saturating_add(inner.height) {
            self.scroll_row = cursor_row.saturating_sub(inner.height.saturating_sub(1));
        }

        if cursor_col < self.scroll_col {
            self.scroll_col = cursor_col;
        } else if cursor_col >= self.scroll_col.saturating_add(inner.width) {
            self.scroll_col = cursor_col.saturating_sub(inner.width.saturating_sub(1));
        }
    }

    fn cursor_line_col(&self) -> (usize, u16) {
        cursor_line_col(self.buffer.text(), self.buffer.cursor_byte_index())
    }

    fn current_line_start(&self) -> usize {
        let (line, _) = self.cursor_line_col();
        line_ranges(self.buffer.text())
            .get(line)
            .map(|(start, _)| *start)
            .unwrap_or(0)
    }

    fn current_line_end(&self) -> usize {
        let (line, _) = self.cursor_line_col();
        line_ranges(self.buffer.text())
            .get(line)
            .map(|(_, end)| *end)
            .unwrap_or_else(|| self.buffer.text().len())
    }

    fn set_cursor_line_col(&mut self, line: usize, col: u16) {
        let ranges = line_ranges(self.buffer.text());
        let idx = line.min(ranges.len().saturating_sub(1));
        let (start, end) = ranges.get(idx).copied().unwrap_or((0, 0));
        let offset = byte_index_at_display_col(&self.buffer.text()[start..end], col);
        self.buffer
            .set_cursor_byte_index(start.saturating_add(offset));
    }

    fn insert_text(&mut self, text: &str) -> EventResult {
        if text.is_empty() {
            return EventResult::ignored();
        }
        self.buffer.insert_str(text);
        self.after_edit();
        self.sync_binding_from_buffer();
        EventResult::changed()
    }

    fn insert_newline(&mut self) -> EventResult {
        self.buffer.insert_char('\n');
        self.after_edit();
        self.sync_binding_from_buffer();
        EventResult::changed()
    }

    fn after_edit(&mut self) {
        self.preferred_col = None;
        self.history_pos = None;
        self.history_draft = None;
    }

    fn move_up_or_history(&mut self) -> EventResult {
        let (line, col) = self.cursor_line_col();
        if line > 0 {
            let desired = self.preferred_col.unwrap_or(col);
            self.preferred_col = Some(desired);
            self.set_cursor_line_col(line - 1, desired);
            return EventResult::consumed();
        }
        if self.history_previous() {
            EventResult::changed()
        } else {
            EventResult::consumed()
        }
    }

    fn move_down_or_history(&mut self) -> EventResult {
        let ranges = line_ranges(self.buffer.text());
        let last = ranges.len().saturating_sub(1);
        let (line, col) = self.cursor_line_col();
        if line < last {
            let desired = self.preferred_col.unwrap_or(col);
            self.preferred_col = Some(desired);
            self.set_cursor_line_col(line + 1, desired);
            return EventResult::consumed();
        }
        if self.history_next() {
            EventResult::changed()
        } else {
            EventResult::consumed()
        }
    }

    fn history_previous(&mut self) -> bool {
        let history = self.history.get();
        if history.is_empty() {
            return false;
        }
        let next_idx = match self.history_pos {
            Some(idx) => idx.saturating_sub(1),
            None => {
                self.history_draft = Some(self.buffer.text().to_string());
                history.len().saturating_sub(1)
            }
        };
        self.history_pos = Some(next_idx);
        self.buffer.set_text(history[next_idx].clone());
        self.sync_binding_from_buffer();
        self.preferred_col = None;
        true
    }

    fn history_next(&mut self) -> bool {
        let Some(idx) = self.history_pos else {
            return false;
        };
        let history = self.history.get();
        if idx + 1 < history.len() {
            let next_idx = idx + 1;
            self.history_pos = Some(next_idx);
            self.buffer.set_text(history[next_idx].clone());
        } else {
            self.history_pos = None;
            self.buffer
                .set_text(self.history_draft.take().unwrap_or_default());
        }
        self.sync_binding_from_buffer();
        self.preferred_col = None;
        true
    }

    fn commit_history_entry(&mut self) {
        let entry = self.buffer.text().to_string();
        if entry.trim().is_empty() {
            return;
        }
        let mut history = self.history.get();
        if history.last() != Some(&entry) {
            history.push(entry);
            self.history.set(history);
        }
    }

    fn kill_before_cursor(&mut self) -> EventResult {
        let start = self.current_line_start();
        let end = self.buffer.cursor_byte_index();
        self.kill_range(start, end)
    }

    fn kill_after_cursor(&mut self) -> EventResult {
        let start = self.buffer.cursor_byte_index();
        let mut end = self.current_line_end();
        if start == end && end < self.buffer.text().len() {
            end = end.saturating_add(1);
        }
        self.kill_range(start, end)
    }

    fn kill_range(&mut self, start: usize, end: usize) -> EventResult {
        let text = self.buffer.text().to_string();
        let start = self.buffer.align_to_grapheme_boundary(start);
        let end = self.buffer.align_to_grapheme_boundary(end).min(text.len());
        match start.cmp(&end) {
            Ordering::Less => {
                let killed = text[start..end].to_string();
                let mut next = text;
                next.replace_range(start..end, "");
                self.kill_ring.set(killed.clone());
                self.clipboard.set(killed);
                self.buffer.set_text(next);
                self.buffer.set_cursor_byte_index(start);
                self.after_edit();
                self.sync_binding_from_buffer();
                EventResult::changed()
            }
            _ => EventResult::consumed(),
        }
    }
}

fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (idx, ch) in text.char_indices() {
        if ch == '\n' {
            ranges.push((start, idx));
            start = idx + ch.len_utf8();
        }
    }
    ranges.push((start, text.len()));
    ranges
}

fn cursor_line_col(text: &str, cursor: usize) -> (usize, u16) {
    let cursor = cursor.min(text.len());
    for (idx, (start, end)) in line_ranges(text).into_iter().enumerate() {
        if cursor <= end {
            let prefix = &text[start..cursor.max(start).min(end)];
            return (idx, display_width(prefix));
        }
    }
    (0, 0)
}

fn slice_by_width(text: &str, start_col: u16, width: u16) -> String {
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut col: u16 = 0;
    let end = start_col.saturating_add(width);

    for g in text.graphemes(true) {
        let w = (UnicodeWidthStr::width(g) as u16).max(1);
        let next = col.saturating_add(w);
        if next <= start_col {
            col = next;
            continue;
        }
        if col >= end {
            break;
        }
        out.push_str(g);
        col = next;
    }

    out
}

fn display_width(text: &str) -> u16 {
    UnicodeWidthStr::width(text).min(u16::MAX as usize) as u16
}

fn build_visible_line(
    text: &str,
    start_col: u16,
    width: u16,
    style: ratatui::style::Style,
) -> Line<'static> {
    let visible = slice_by_width(text, start_col, width);
    if visible.is_empty() {
        Line::raw("")
    } else {
        Line::from(vec![Span::styled(visible, style)])
    }
}

fn byte_index_at_display_col(text: &str, target_col: u16) -> usize {
    let mut col: u16 = 0;
    for (byte_idx, g) in text.grapheme_indices(true) {
        let w = (UnicodeWidthStr::width(g) as u16).max(1);
        let next = col.saturating_add(w);
        if target_col < next {
            return byte_idx;
        }
        col = next;
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_line_col_tracks_newlines() {
        assert_eq!(cursor_line_col("one\ntwo", 0), (0, 0));
        assert_eq!(cursor_line_col("one\ntwo", 3), (0, 3));
        assert_eq!(cursor_line_col("one\ntwo", 4), (1, 0));
        assert_eq!(cursor_line_col("one\ntwo", 7), (1, 3));
    }

    #[test]
    fn history_navigation_restores_draft() {
        let draft = Binding::new("draft".to_string());
        let history = Binding::new(vec!["one".to_string(), "two".to_string()]);
        let mut area = TextArea::new("Message", draft.clone()).history(history);

        assert!(area.history_previous());
        assert_eq!(draft.get(), "two");
        assert!(area.history_previous());
        assert_eq!(draft.get(), "one");
        assert!(area.history_next());
        assert_eq!(draft.get(), "two");
        assert!(area.history_next());
        assert_eq!(draft.get(), "draft");
    }

    #[test]
    fn kill_ring_yanks_removed_text() {
        let draft = Binding::new("killme".to_string());
        let mut area = TextArea::new("Message", draft.clone());
        let _ = area.kill_before_cursor();
        assert_eq!(draft.get(), "");
        assert_eq!(area.kill_ring.get(), "killme");
        let killed = area.kill_ring.get();
        let _ = area.insert_text(&killed);
        assert_eq!(draft.get(), "killme");
    }

    #[test]
    fn replace_byte_range_updates_binding_and_cursor() {
        let draft = Binding::new("hello @ca world".to_string());
        let mut area = TextArea::new("Message", draft.clone());

        let result = area.replace_byte_range("hello ".len().."hello @ca".len(), "@Cargo.toml");

        assert_eq!(result, EventResult::changed());
        assert_eq!(draft.get(), "hello @Cargo.toml world");
        assert_eq!(area.cursor_byte_index(), "hello @Cargo.toml".len());
    }
}
