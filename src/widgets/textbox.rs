use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::composable::{Component, ComponentContext, EventResult};
use crate::reactive::Binding;
use crate::text::TextBuffer;

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

#[derive(Clone, Debug)]
pub struct TextBox {
    title: Binding<String>,
    buffer: TextBuffer,
    binding: Binding<String>,
    enabled: Binding<bool>,
    scroll: u16,
    last_area: Option<Rect>,
}

impl TextBox {
    pub fn new(title: impl Into<Binding<String>>, binding: Binding<String>) -> Self {
        let initial = binding.get();
        Self {
            title: title.into(),
            buffer: TextBuffer::with_text(initial),
            binding,
            enabled: true.into(),
            scroll: 0,
            last_area: None,
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
                use crossterm::event::MouseButton;
                use crossterm::event::MouseEventKind;

                if m.kind != MouseEventKind::Down(MouseButton::Left) {
                    return EventResult::ignored();
                }

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
                if local_y != inner.y {
                    return EventResult::ignored();
                }
                if local_x < inner.x || local_x >= inner.x.saturating_add(inner.width) {
                    return EventResult::ignored();
                }

                let col = self.scroll.saturating_add(local_x.saturating_sub(inner.x));
                self.buffer.set_cursor_display_col(col);
                EventResult::consumed()
            }
            Event::Paste(s) => {
                self.buffer.insert_str(s);
                self.binding.set(self.buffer.text().to_string());
                EventResult::changed()
            }
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => {
                let mods = *modifiers;
                match code {
                    KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => {
                        self.buffer.set_text("");
                        self.binding.set(String::new());
                        EventResult::changed()
                    }
                    KeyCode::Backspace => {
                        self.buffer.backspace();
                        self.binding.set(self.buffer.text().to_string());
                        EventResult::changed()
                    }
                    KeyCode::Delete => {
                        self.buffer.delete();
                        self.binding.set(self.buffer.text().to_string());
                        EventResult::changed()
                    }
                    KeyCode::Left => {
                        self.buffer.move_left();
                        EventResult::consumed()
                    }
                    KeyCode::Right => {
                        self.buffer.move_right();
                        EventResult::consumed()
                    }
                    KeyCode::Home => {
                        self.buffer.move_home();
                        EventResult::consumed()
                    }
                    KeyCode::End => {
                        self.buffer.move_end();
                        EventResult::consumed()
                    }
                    KeyCode::Enter => EventResult::submitted(),
                    KeyCode::Char(c)
                        if !mods.contains(KeyModifiers::CONTROL)
                            && !mods.contains(KeyModifiers::ALT) =>
                    {
                        self.buffer.insert_char(*c);
                        self.binding.set(self.buffer.text().to_string());
                        EventResult::changed()
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
        let visible = slice_by_width(self.buffer.text(), self.scroll, inner.width);
        frame.render_widget(Paragraph::new(Line::raw(visible)).style(style), inner);

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
