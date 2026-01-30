use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::text::TextBuffer;
use crate::theme::Theme;

use super::{Control, ControlOutcome, FormAction};

#[derive(Clone, Debug)]
pub struct TextBox {
    title: String,
    buffer: TextBuffer,
    focused: bool,
    scroll: u16,
    area: Option<Rect>,
}

impl TextBox {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            buffer: TextBuffer::new(),
            focused: false,
            scroll: 0,
            area: None,
        }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.buffer.set_text(text);
        self
    }

    pub fn text(&self) -> &str {
        self.buffer.text()
    }
}

impl Control for TextBox {
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn set_area(&mut self, area: Rect) {
        self.area = Some(area);
    }

    fn desired_height(&self) -> u16 {
        3
    }

    fn handle_event(&mut self, event: &Event) -> (ControlOutcome, FormAction) {
        match event {
            Event::Mouse(m) => {
                use crossterm::event::MouseButton;
                use crossterm::event::MouseEventKind;

                if m.kind != MouseEventKind::Down(MouseButton::Left) {
                    return (ControlOutcome::Ignored, FormAction::None);
                }

                let Some(area) = self.area else {
                    return (ControlOutcome::Ignored, FormAction::None);
                };
                let inner = Rect {
                    x: area.x.saturating_add(1),
                    y: area.y.saturating_add(1),
                    width: area.width.saturating_sub(2),
                    height: area.height.saturating_sub(2),
                };
                if inner.width == 0 || inner.height == 0 {
                    return (ControlOutcome::Ignored, FormAction::None);
                }

                // TextBox is a single-line editor.
                if m.row != inner.y {
                    return (ControlOutcome::Ignored, FormAction::None);
                }
                if m.column < inner.x || m.column >= inner.x.saturating_add(inner.width) {
                    return (ControlOutcome::Ignored, FormAction::None);
                }

                let col = self.scroll.saturating_add(m.column.saturating_sub(inner.x));
                self.buffer.set_cursor_display_col(col);
                (ControlOutcome::Consumed, FormAction::None)
            }
            Event::Paste(s) => {
                self.buffer.insert_str(s);
                (ControlOutcome::Consumed, FormAction::Changed)
            }
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => {
                let mods = *modifiers;
                match code {
                    KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => {
                        self.buffer.set_text("");
                        (ControlOutcome::Consumed, FormAction::Changed)
                    }
                    KeyCode::Backspace => {
                        self.buffer.backspace();
                        (ControlOutcome::Consumed, FormAction::Changed)
                    }
                    KeyCode::Delete => {
                        self.buffer.delete();
                        (ControlOutcome::Consumed, FormAction::Changed)
                    }
                    KeyCode::Left => {
                        self.buffer.move_left();
                        (ControlOutcome::Consumed, FormAction::None)
                    }
                    KeyCode::Right => {
                        self.buffer.move_right();
                        (ControlOutcome::Consumed, FormAction::None)
                    }
                    KeyCode::Home => {
                        self.buffer.move_home();
                        (ControlOutcome::Consumed, FormAction::None)
                    }
                    KeyCode::End => {
                        self.buffer.move_end();
                        (ControlOutcome::Consumed, FormAction::None)
                    }
                    KeyCode::Enter => (ControlOutcome::Consumed, FormAction::Submitted),
                    KeyCode::Char(c)
                        if !mods.contains(KeyModifiers::CONTROL)
                            && !mods.contains(KeyModifiers::ALT) =>
                    {
                        self.buffer.insert_char(*c);
                        (ControlOutcome::Consumed, FormAction::Changed)
                    }
                    _ => (ControlOutcome::Ignored, FormAction::None),
                }
            }
            _ => (ControlOutcome::Ignored, FormAction::None),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let style = if self.focused {
            theme.widget.focused
        } else {
            theme.widget.normal
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.title.clone());
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

        if self.focused {
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
