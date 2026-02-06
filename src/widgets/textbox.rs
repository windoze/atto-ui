use std::time::{Duration, Instant};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::composable::{Component, ComponentContext, EventResult};
use crate::reactive::Binding;
use crate::text::TextBuffer;

static TEXTBOX_CLIPBOARD: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

const MULTI_CLICK_THRESHOLD: Duration = Duration::from_millis(500);

fn mouse_coords_local_to_area(area: Rect, m: MouseEvent) -> Option<(u16, u16)> {
    if area.width == 0 || area.height == 0 {
        return None;
    }

    if m.column >= area.x
        && m.column < area.x.saturating_add(area.width)
        && m.row >= area.y
        && m.row < area.y.saturating_add(area.height)
    {
        return Some((
            m.column.saturating_sub(area.x),
            m.row.saturating_sub(area.y),
        ));
    }

    if m.column < area.width && m.row < area.height {
        return Some((m.column, m.row));
    }

    None
}

#[derive(Clone, Copy, Debug)]
struct ClickState {
    at: Instant,
    pos: (u16, u16),
    count: u8,
}

#[derive(Clone, Debug)]
pub struct TextBox {
    title: Binding<String>,
    placeholder: Option<Binding<String>>,
    buffer: TextBuffer,
    binding: Binding<String>,
    enabled: Binding<bool>,
    scroll: u16,
    last_area: Option<Rect>,
    selection_anchor: Option<usize>,
    mouse_selecting: bool,
    click_state: Option<ClickState>,
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
            scroll: 0,
            last_area: None,
            selection_anchor: None,
            mouse_selecting: false,
            click_state: None,
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
}

impl Component for TextBox {
    fn min_width(&self) -> u16 {
        3
    }

    fn min_height(&self) -> u16 {
        3
    }

    fn is_focusable(&self) -> bool {
        self.enabled.get()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(3)
    }

    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }
        match event {
            Event::Mouse(m) => {
                let Some(area) = self.last_area else {
                    return EventResult::ignored();
                };
                let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
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
                if local_y != inner.y
                    || local_x < inner.x
                    || local_x >= inner.x.saturating_add(inner.width)
                {
                    return EventResult::ignored();
                }

                let col = self.scroll.saturating_add(local_x.saturating_sub(inner.x));

                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let now = Instant::now();
                        let pos = (local_x, local_y);
                        let count = match self.click_state {
                            Some(prev)
                                if now.saturating_duration_since(prev.at)
                                    <= MULTI_CLICK_THRESHOLD
                                    && prev.pos == pos =>
                            {
                                prev.count.saturating_add(1)
                            }
                            _ => 1,
                        }
                        .min(3);
                        self.click_state = Some(ClickState {
                            at: now,
                            pos,
                            count,
                        });

                        if count >= 3 {
                            self.select_all();
                            self.mouse_selecting = false;
                            return EventResult::consumed();
                        }

                        if count == 2 {
                            self.buffer.set_cursor_display_col(col);
                            self.select_word_at_cursor();
                            self.mouse_selecting = false;
                            return EventResult::consumed();
                        }

                        let extend = m.modifiers.contains(KeyModifiers::SHIFT);
                        if extend {
                            if self.selection_anchor.is_none() {
                                self.selection_anchor = Some(self.buffer.cursor_byte_index());
                            }
                            self.buffer.set_cursor_display_col(col);
                            return EventResult::consumed();
                        }

                        self.buffer.set_cursor_display_col(col);
                        self.selection_anchor = Some(self.buffer.cursor_byte_index());
                        self.mouse_selecting = true;
                        EventResult::consumed()
                    }
                    MouseEventKind::Drag(MouseButton::Left) => {
                        if !self.mouse_selecting {
                            return EventResult::ignored();
                        }
                        self.buffer.set_cursor_display_col(col);
                        EventResult::consumed()
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        if !self.mouse_selecting {
                            return EventResult::ignored();
                        }
                        self.mouse_selecting = false;
                        if self.selection_range().is_none() {
                            self.selection_anchor = None;
                        }
                        EventResult::consumed()
                    }
                    _ => EventResult::ignored(),
                }
            }
            Event::Paste(s) => {
                if self.insert_str(s) {
                    EventResult::changed()
                } else {
                    EventResult::ignored()
                }
            }
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => {
                let mods = *modifiers;
                match code {
                    KeyCode::Char('a') if mods.contains(KeyModifiers::CONTROL) => {
                        self.select_all();
                        EventResult::consumed()
                    }
                    KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => {
                        self.copy_selection();
                        EventResult::consumed()
                    }
                    KeyCode::Char('x') if mods.contains(KeyModifiers::CONTROL) => {
                        if self.cut_selection() {
                            EventResult::changed()
                        } else {
                            EventResult::consumed()
                        }
                    }
                    KeyCode::Char('v') if mods.contains(KeyModifiers::CONTROL) => {
                        if self.paste_clipboard() {
                            EventResult::changed()
                        } else {
                            EventResult::consumed()
                        }
                    }
                    KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => {
                        self.buffer.set_text("");
                        self.binding.set(String::new());
                        self.selection_anchor = None;
                        EventResult::changed()
                    }
                    KeyCode::Backspace => {
                        if self.delete_selection_if_any() {
                            return EventResult::changed();
                        }
                        self.buffer.backspace();
                        self.binding.set(self.buffer.text().to_string());
                        EventResult::changed()
                    }
                    KeyCode::Delete => {
                        if self.delete_selection_if_any() {
                            return EventResult::changed();
                        }
                        self.buffer.delete();
                        self.binding.set(self.buffer.text().to_string());
                        EventResult::changed()
                    }
                    KeyCode::Left => {
                        if !mods.contains(KeyModifiers::SHIFT) && self.collapse_selection_to_start()
                        {
                            return EventResult::consumed();
                        }
                        self.update_selection_anchor(mods);
                        self.buffer.move_left();
                        EventResult::consumed()
                    }
                    KeyCode::Right => {
                        if !mods.contains(KeyModifiers::SHIFT) && self.collapse_selection_to_end() {
                            return EventResult::consumed();
                        }
                        self.update_selection_anchor(mods);
                        self.buffer.move_right();
                        EventResult::consumed()
                    }
                    KeyCode::Home => {
                        self.update_selection_anchor(mods);
                        self.buffer.move_home();
                        EventResult::consumed()
                    }
                    KeyCode::End => {
                        self.update_selection_anchor(mods);
                        self.buffer.move_end();
                        EventResult::consumed()
                    }
                    KeyCode::Enter => EventResult::submitted(),
                    KeyCode::Char(c)
                        if !mods.contains(KeyModifiers::CONTROL)
                            && !mods.contains(KeyModifiers::ALT) =>
                    {
                        if self.insert_char(*c) {
                            EventResult::changed()
                        } else {
                            EventResult::ignored()
                        }
                    }
                    _ => EventResult::ignored(),
                }
            }
            _ => EventResult::ignored(),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.height == 0 || area.width == 0 {
            return;
        }
        let external = self.binding.get();
        if external != self.buffer.text() {
            self.buffer.set_text(external);
            self.selection_anchor = None;
            self.mouse_selecting = false;
        }
        let enabled = self.enabled.get();
        let style = if !enabled {
            ctx.theme.widget.disabled
        } else if ctx.is_focused {
            ctx.theme.widget.focused
        } else {
            ctx.theme.widget.normal
        };

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

        let line = if self.buffer.text().is_empty()
            && let Some(placeholder) = self.placeholder.as_ref()
        {
            let placeholder_style = if enabled {
                ctx.theme.widget.dim
            } else {
                ctx.theme.widget.disabled
            };
            let visible = slice_by_width(placeholder.get().as_str(), 0, inner.width);
            Line::styled(visible, placeholder_style)
        } else {
            let sel_style = if enabled {
                ctx.theme.selection
            } else {
                ctx.theme.selection.patch(ctx.theme.widget.disabled)
            };
            render_text_line(
                self.buffer.text(),
                self.scroll,
                inner.width,
                style,
                sel_style,
                self.selection_range(),
            )
        };
        frame.render_widget(Paragraph::new(line).style(style), inner);

        if ctx.is_focused {
            let x = inner
                .x
                .saturating_add(cursor_col.saturating_sub(self.scroll).min(inner.width - 1));
            let y = inner.y;
            frame.set_cursor_position((x, y));
        }
    }
}

impl TextBox {
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
}

impl TextBox {
    fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        let cursor = self.buffer.cursor_byte_index();
        if anchor == cursor {
            return None;
        }
        Some((anchor.min(cursor), anchor.max(cursor)))
    }

    fn update_selection_anchor(&mut self, mods: KeyModifiers) {
        if mods.contains(KeyModifiers::SHIFT) {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.buffer.cursor_byte_index());
            }
            return;
        }
        self.selection_anchor = None;
    }

    fn collapse_selection_to_start(&mut self) -> bool {
        let Some((start, _end)) = self.selection_range() else {
            self.selection_anchor = None;
            return false;
        };
        self.buffer.set_cursor_byte_index(start);
        self.selection_anchor = None;
        true
    }

    fn collapse_selection_to_end(&mut self) -> bool {
        let Some((_start, end)) = self.selection_range() else {
            self.selection_anchor = None;
            return false;
        };
        self.buffer.set_cursor_byte_index(end);
        self.selection_anchor = None;
        true
    }

    fn delete_selection_if_any(&mut self) -> bool {
        let Some((start, end)) = self.selection_range() else {
            self.selection_anchor = None;
            return false;
        };
        self.buffer.delete_range(start, end);
        self.selection_anchor = None;
        self.binding.set(self.buffer.text().to_string());
        true
    }

    fn insert_char(&mut self, c: char) -> bool {
        let _ = self.delete_selection_if_any();
        self.buffer.insert_char(c);
        self.selection_anchor = None;
        self.binding.set(self.buffer.text().to_string());
        true
    }

    fn insert_str(&mut self, s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        let _ = self.delete_selection_if_any();
        self.buffer.insert_str(s);
        self.selection_anchor = None;
        self.binding.set(self.buffer.text().to_string());
        true
    }

    fn select_all(&mut self) {
        if self.buffer.text().is_empty() {
            self.selection_anchor = None;
            return;
        }
        self.selection_anchor = Some(0);
        self.buffer.move_end();
    }

    fn select_word_at_cursor(&mut self) {
        let text = self.buffer.text();
        if text.is_empty() {
            self.selection_anchor = None;
            return;
        }
        let len = text.len();
        let idx = self.buffer.cursor_byte_index().min(len);
        let probe = if idx == len && len > 0 { idx - 1 } else { idx };

        let mut range = text
            .unicode_word_indices()
            .find_map(|(start, w)| {
                let end = start.saturating_add(w.len());
                (probe >= start && probe < end).then_some((start, end))
            })
            .or_else(|| {
                text.split_word_bound_indices().find_map(|(start, w)| {
                    let end = start.saturating_add(w.len());
                    (probe >= start && probe < end).then_some((start, end))
                })
            });

        if let Some((start, end)) = range.take() {
            self.buffer.set_cursor_byte_index(start);
            let start = self.buffer.cursor_byte_index();
            self.buffer.set_cursor_byte_index(end);
            self.selection_anchor = Some(start);
        } else {
            self.selection_anchor = None;
        }
    }

    fn copy_selection(&self) {
        let Some((start, end)) = self.selection_range() else {
            return;
        };
        let text = self.buffer.text();
        let start = start.min(text.len());
        let end = end.min(text.len());
        if start >= end {
            return;
        }
        *TEXTBOX_CLIPBOARD.lock() = text[start..end].to_string();
    }

    fn cut_selection(&mut self) -> bool {
        if self.selection_range().is_none() {
            return false;
        }
        self.copy_selection();
        self.delete_selection_if_any();
        true
    }

    fn paste_clipboard(&mut self) -> bool {
        let s = TEXTBOX_CLIPBOARD.lock().clone();
        self.insert_str(&s)
    }
}

fn render_text_line<'a>(
    text: &'a str,
    start_col: u16,
    width: u16,
    style: ratatui::style::Style,
    selection_style: ratatui::style::Style,
    selection: Option<(usize, usize)>,
) -> Line<'a> {
    if width == 0 {
        return Line::styled("", style);
    }

    let mut spans: Vec<Span<'a>> = Vec::new();
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

        let g_style = if let Some((sel_start, sel_end)) = selection
            && byte_idx < sel_end
            && byte_idx.saturating_add(g.len()) > sel_start
        {
            selection_style
        } else {
            style
        };

        spans.push(Span::styled(g, g_style));
        col = next;
    }

    if spans.is_empty() {
        Line::styled("", style)
    } else {
        Line::from(spans)
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
