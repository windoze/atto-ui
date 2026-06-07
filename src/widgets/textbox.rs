use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
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
pub struct TextBox {
    title: Binding<String>,
    placeholder: Option<Binding<String>>,
    buffer: TextBuffer,
    #[component(rename = "text")]
    binding: Binding<String>,
    enabled: Binding<bool>,
    clipboard: Binding<String>,
    scroll: u16,
    last_area: Option<Rect>,
    selection_anchor: Option<usize>,
    last_click_at: Option<Instant>,
    last_click_col: Option<u16>,
    click_count: u8,
    on_change_callback: Option<CallbackHandle>,
    on_submit_callback: Option<CallbackHandle>,
}

impl TextBox {
    pub fn new(title: impl Into<Binding<String>>, binding: Binding<String>) -> Self {
        let initial = binding.get();
        Self {
            title: title.into(),
            placeholder: None,
            buffer: TextBuffer::with_text(initial),
            binding,
            enabled: true.into(),
            clipboard: String::new().into(),
            scroll: 0,
            last_area: None,
            selection_anchor: None,
            last_click_at: None,
            last_click_col: None,
            click_count: 0,
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

    pub fn on_change_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_change_callback = Some(callback);
        self
    }

    pub fn on_submit_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_submit_callback = Some(callback);
        self
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

    fn set_binding_text(&mut self, text: String) {
        self.binding.set(text);
        self.emit_change();
    }

    fn sync_binding_from_buffer(&mut self) {
        let text = self.buffer.text().to_string();
        self.binding.set(text);
        self.emit_change();
    }
}

#[component_properties]
impl Component for TextBox {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.height == 0 || area.width == 0 {
            return;
        }
        let external = self.binding.get();
        if external != self.buffer.text() {
            self.buffer.set_text(external);
            self.selection_anchor = None;
            self.scroll = 0;
        }
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

        let cursor_col = self.buffer.cursor_display_col();
        self.adjust_scroll(cursor_col, inner.width);
        let text = self.buffer.text();
        let selection = self.selection_range();
        let placeholder = self
            .placeholder
            .as_ref()
            .map(|binding| binding.get())
            .filter(|s| !s.is_empty());
        let line = if text.is_empty() {
            if let Some(ph) = placeholder {
                let placeholder_style = if enabled {
                    ctx.theme.widget.dim
                } else {
                    ctx.theme.widget.disabled
                };
                Line::from(vec![Span::styled(
                    slice_by_width(&ph, 0, inner.width),
                    placeholder_style,
                )])
            } else {
                Line::raw("")
            }
        } else {
            build_visible_line(
                text,
                self.scroll,
                inner.width,
                style,
                ctx.theme.selection,
                selection,
            )
        };
        frame.render_widget(Paragraph::new(line).style(style), inner);

        if !text.is_empty() {
            let content_width = display_width(text);
            if self.scroll > 0 {
                draw_indicator(
                    frame,
                    inner.x,
                    inner.y,
                    ctx.theme
                        .glyph("scrollbar-left-arrow")
                        .unwrap_or("\u{25C4}"),
                    ctx.theme.widget.accent,
                );
            }
            if content_width > self.scroll.saturating_add(inner.width) {
                let x = inner.x.saturating_add(inner.width.saturating_sub(1));
                draw_indicator(
                    frame,
                    x,
                    inner.y,
                    ctx.theme
                        .glyph("scrollbar-right-arrow")
                        .unwrap_or("\u{25BA}"),
                    ctx.theme.widget.accent,
                );
            }
        }

        if ctx.is_focused {
            let x = inner
                .x
                .saturating_add(cursor_col.saturating_sub(self.scroll).min(inner.width - 1));
            let y = inner.y;
            frame.set_cursor_position((x, y));
        }
    }
}

impl Layout for TextBox {
    fn min_width(&self) -> u16 {
        3
    }

    fn min_height(&self) -> u16 {
        3
    }

    fn desired_height(&self) -> Option<u16> {
        Some(3)
    }
}

impl FocusNav for TextBox {
    fn is_focusable(&self) -> bool {
        self.enabled.get()
    }
}

impl EventHandling for TextBox {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }
        match event {
            Event::Mouse(m) => {
                use crossterm::event::MouseButton;
                use crossterm::event::MouseEventKind;

                let Some(area) = self.last_area else {
                    return EventResult::ignored();
                };
                let Some((local_x, local_y)) =
                    mouse_coords_local_to_area(area, *m, _ctx.mouse_coordinate_space)
                else {
                    return EventResult::ignored();
                };

                let inner = Rect {
                    x: 1,
                    y: 1,
                    width: area.width.saturating_sub(2),
                    height: area.height.saturating_sub(2),
                };
                if inner.width == 0 || inner.height == 0 {
                    return EventResult::ignored();
                }

                // TextBox is a single-line editor.
                if local_y != inner.y {
                    return EventResult::ignored();
                }
                if local_x < inner.x || local_x >= inner.x.saturating_add(inner.width) {
                    return EventResult::ignored();
                }

                let col = self.scroll.saturating_add(local_x.saturating_sub(inner.x));

                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let click_count = self.register_click(col);
                        if click_count == 3 {
                            self.select_all();
                            return EventResult::consumed();
                        }
                        if click_count == 2 && self.select_word_at_col(col) {
                            return EventResult::consumed();
                        }

                        let cursor_before = self.aligned_cursor_byte_index();
                        self.buffer.set_cursor_display_col(col);
                        let anchor = if m.modifiers.contains(KeyModifiers::SHIFT) {
                            self.selection_anchor.unwrap_or(cursor_before)
                        } else {
                            self.aligned_cursor_byte_index()
                        };
                        self.set_selection_anchor(anchor);
                        EventResult::consumed()
                    }
                    MouseEventKind::Drag(MouseButton::Left) => {
                        if self.selection_anchor.is_none() {
                            self.set_selection_anchor(self.aligned_cursor_byte_index());
                        }
                        self.buffer.set_cursor_display_col(col);
                        EventResult::consumed()
                    }
                    _ => EventResult::ignored(),
                }
            }
            Event::Paste(s) => {
                self.replace_selection_if_any();
                self.buffer.insert_str(s);
                self.sync_binding_from_buffer();
                self.selection_anchor = None;
                EventResult::changed()
            }
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
                    KeyCode::Char('a') if mods.contains(KeyModifiers::CONTROL) => {
                        self.select_all();
                        EventResult::consumed()
                    }
                    KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => {
                        if let Some(text) = self.selected_text() {
                            self.copy_selection_text(text);
                            return EventResult::consumed();
                        }
                        EventResult::ignored()
                    }
                    KeyCode::Char('x') if mods.contains(KeyModifiers::CONTROL) => {
                        if let Some(text) = self.selected_text() {
                            self.copy_selection_text(text);
                            if self.delete_selection() {
                                self.sync_binding_from_buffer();
                                return EventResult::changed();
                            }
                        }
                        EventResult::ignored()
                    }
                    KeyCode::Char('v') if mods.contains(KeyModifiers::CONTROL) => {
                        let text = self.clipboard.get();
                        if !text.is_empty() {
                            self.replace_selection_if_any();
                            self.buffer.insert_str(&text);
                            self.sync_binding_from_buffer();
                            self.selection_anchor = None;
                            return EventResult::changed();
                        }
                        EventResult::ignored()
                    }
                    KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => {
                        self.buffer.set_text("");
                        self.set_binding_text(String::new());
                        self.selection_anchor = None;
                        EventResult::changed()
                    }
                    KeyCode::Backspace => {
                        if self.delete_selection() {
                            self.sync_binding_from_buffer();
                            return EventResult::changed();
                        }
                        self.buffer.backspace();
                        self.sync_binding_from_buffer();
                        self.selection_anchor = None;
                        EventResult::changed()
                    }
                    KeyCode::Delete => {
                        if self.delete_selection() {
                            self.sync_binding_from_buffer();
                            return EventResult::changed();
                        }
                        self.buffer.delete();
                        self.sync_binding_from_buffer();
                        self.selection_anchor = None;
                        EventResult::changed()
                    }
                    KeyCode::Left => {
                        if mods.contains(KeyModifiers::SHIFT) {
                            self.ensure_selection_anchor();
                            self.buffer.move_left();
                        } else if self.collapse_selection_to_start() {
                            self.selection_anchor = None;
                        } else {
                            self.buffer.move_left();
                            self.selection_anchor = None;
                        }
                        EventResult::consumed()
                    }
                    KeyCode::Right => {
                        if mods.contains(KeyModifiers::SHIFT) {
                            self.ensure_selection_anchor();
                            self.buffer.move_right();
                        } else if self.collapse_selection_to_end() {
                            self.selection_anchor = None;
                        } else {
                            self.buffer.move_right();
                            self.selection_anchor = None;
                        }
                        EventResult::consumed()
                    }
                    KeyCode::Home => {
                        if mods.contains(KeyModifiers::SHIFT) {
                            self.ensure_selection_anchor();
                            self.buffer.move_home();
                        } else {
                            self.buffer.move_home();
                            self.selection_anchor = None;
                        }
                        EventResult::consumed()
                    }
                    KeyCode::End => {
                        if mods.contains(KeyModifiers::SHIFT) {
                            self.ensure_selection_anchor();
                            self.buffer.move_end();
                        } else {
                            self.buffer.move_end();
                            self.selection_anchor = None;
                        }
                        EventResult::consumed()
                    }
                    KeyCode::Enter => {
                        self.emit_submit();
                        EventResult::submitted()
                    }
                    KeyCode::Char(c)
                        if !mods.contains(KeyModifiers::CONTROL)
                            && !mods.contains(KeyModifiers::ALT) =>
                    {
                        self.replace_selection_if_any();
                        self.buffer.insert_char(*c);
                        self.sync_binding_from_buffer();
                        self.selection_anchor = None;
                        EventResult::changed()
                    }
                    _ => EventResult::ignored(),
                }
            }
            _ => EventResult::ignored(),
        }
    }
}

crate::impl_component_default_traits!(TextBox => Scrollable, DynamicTree);

impl TextBox {
    fn aligned_cursor_byte_index(&self) -> usize {
        self.buffer
            .align_to_grapheme_boundary(self.buffer.cursor_byte_index())
    }

    fn set_selection_anchor(&mut self, byte: usize) {
        self.selection_anchor = Some(self.buffer.align_to_grapheme_boundary(byte));
    }

    fn adjust_scroll(&mut self, cursor_col: u16, width: u16) {
        if width == 0 {
            self.scroll = 0;
            return;
        }
        if cursor_col < self.scroll {
            self.scroll = cursor_col;
        } else if cursor_col >= self.scroll.saturating_add(width) {
            self.scroll = cursor_col.saturating_sub(width.saturating_sub(1));
        }
    }

    fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self
            .buffer
            .align_to_grapheme_boundary(self.selection_anchor?);
        let cursor = self.aligned_cursor_byte_index();
        if anchor == cursor {
            return None;
        }
        Some((anchor.min(cursor), anchor.max(cursor)))
    }

    fn ensure_selection_anchor(&mut self) {
        if self.selection_anchor.is_none() {
            self.set_selection_anchor(self.aligned_cursor_byte_index());
        }
    }

    fn collapse_selection_to_start(&mut self) -> bool {
        let Some((start, _)) = self.selection_range() else {
            return false;
        };
        self.buffer.set_cursor_byte_index(start);
        true
    }

    fn collapse_selection_to_end(&mut self) -> bool {
        let Some((_, end)) = self.selection_range() else {
            return false;
        };
        self.buffer.set_cursor_byte_index(end);
        true
    }

    fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection_range() else {
            return false;
        };
        let mut text = self.buffer.text().to_string();
        let start = self.buffer.align_to_grapheme_boundary(start);
        let end = self.buffer.align_to_grapheme_boundary(end).min(text.len());
        if start >= end || start >= text.len() {
            self.selection_anchor = None;
            return false;
        }
        text.replace_range(start..end, "");
        self.buffer.set_text(text);
        self.buffer.set_cursor_byte_index(start);
        self.selection_anchor = None;
        true
    }

    fn replace_selection_if_any(&mut self) {
        let _ = self.delete_selection();
    }

    fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        let text = self.buffer.text();
        if start >= end || start >= text.len() {
            return None;
        }
        let end = end.min(text.len());
        Some(text[start..end].to_string())
    }

    fn copy_selection_text(&self, text: String) {
        self.clipboard.set(text.clone());
        let _ = crate::clipboard::copy_to_system_clipboard(&text);
    }

    fn select_all(&mut self) {
        let len = self.buffer.text().len();
        self.set_selection_anchor(0);
        self.buffer.set_cursor_byte_index(len);
    }

    fn register_click(&mut self, col: u16) -> u8 {
        const MULTI_CLICK_MS: u64 = 400;
        let now = Instant::now();
        let mut count = 1;
        if let Some(last) = self.last_click_at
            && now.duration_since(last) <= Duration::from_millis(MULTI_CLICK_MS)
            && self.last_click_col == Some(col)
        {
            count = self.click_count.saturating_add(1).min(3);
        }
        self.last_click_at = Some(now);
        self.last_click_col = Some(col);
        self.click_count = count;
        if count == 3 {
            self.last_click_at = None;
            self.click_count = 0;
        }
        count
    }

    fn select_word_at_col(&mut self, col: u16) -> bool {
        let text = self.buffer.text();
        if text.is_empty() {
            return false;
        }
        let byte_idx = byte_index_at_display_col(text, col);
        let Some((start, end)) = word_range_at(text, byte_idx) else {
            return false;
        };
        self.set_selection_anchor(start);
        self.buffer.set_cursor_byte_index(end);
        true
    }
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
    base_style: ratatui::style::Style,
    selection_style: ratatui::style::Style,
    selection: Option<(usize, usize)>,
) -> Line<'static> {
    if width == 0 {
        return Line::raw("");
    }
    let mut spans = Vec::new();
    let mut col: u16 = 0;
    let end_col = start_col.saturating_add(width);

    for (byte_idx, g) in text.grapheme_indices(true) {
        let w = (UnicodeWidthStr::width(g) as u16).max(1);
        let next = col.saturating_add(w);

        if next <= start_col {
            col = next;
            continue;
        }
        if col >= end_col {
            break;
        }

        let g_start = byte_idx;
        let g_end = byte_idx.saturating_add(g.len());
        let style = if let Some((sel_start, sel_end)) = selection {
            if sel_start < g_end && sel_end > g_start {
                selection_style
            } else {
                base_style
            }
        } else {
            base_style
        };
        spans.push(Span::styled(g.to_string(), style));
        col = next;
    }

    if spans.is_empty() {
        Line::raw("")
    } else {
        Line::from(spans)
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

fn word_range_at(text: &str, byte_idx: usize) -> Option<(usize, usize)> {
    if text.is_empty() {
        return None;
    }
    let mut last = None;
    for (start, seg) in text.split_word_bound_indices() {
        let end = start.saturating_add(seg.len());
        last = Some((start, end));
        if byte_idx >= start && byte_idx < end {
            return Some((start, end));
        }
    }
    if byte_idx >= text.len() {
        return last;
    }
    None
}

fn draw_indicator(
    frame: &mut Frame<'_>,
    x: u16,
    y: u16,
    symbol: &str,
    style: ratatui::style::Style,
) {
    if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
        cell.set_symbol(symbol);
        cell.set_style(style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(theme: &crate::theme::Theme) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: crate::wm::WindowId::default(),
            is_focused: true,
            scrollbar_host: crate::composable::ScrollbarHost::Component,
            tab_mode: crate::composable::TabMode::Cycle,
            mouse_coordinate_space: crate::composable::MouseCoordinateSpace::Absolute,
            drag: None,
        }
    }

    #[test]
    fn key_release_does_not_insert_text() {
        let value = Binding::new(String::new());
        let mut textbox = TextBox::new("Name", value.clone());
        let theme = crate::theme::Theme::dark();
        let release = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('Z'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));

        assert_eq!(
            textbox.handle_event(&release, context(&theme)),
            EventResult::ignored()
        );
        assert_eq!(value.get(), "");
    }
}
