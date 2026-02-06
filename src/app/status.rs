use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::theme::Theme;

#[derive(Clone, Debug, Default)]
pub struct StatusBar {
    left: String,
    right: String,
}

impl StatusBar {
    pub fn set_left(&mut self, text: impl Into<String>) {
        self.left = text.into();
    }

    pub fn set_right(&mut self, text: impl Into<String>) {
        self.right = text.into();
    }

    pub fn draw(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }
        let width = area.width as usize;
        let mut line = self.left.clone();
        if line.len() < width {
            let remaining = width.saturating_sub(line.len());
            let right = self.right.clone();
            if right.len() < remaining {
                line.push_str(&" ".repeat(remaining.saturating_sub(right.len())));
                line.push_str(&right);
            } else {
                line.push_str(&" ".repeat(remaining));
            }
        } else if line.len() > width {
            line.truncate(width);
        }

        let p = Paragraph::new(Line::from(vec![Span::styled(line, theme.status_bar)]));
        frame.render_widget(p, area);
    }
}

/// Utility widget for filling areas with a style.
pub(crate) struct Fill {
    pub style: ratatui::style::Style,
    pub ch: char,
}

impl Widget for Fill {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let s = self.ch.to_string();
        let style = ratatui::style::Style::reset().patch(self.style);
        for y in area.y..area.y.saturating_add(area.height) {
            for x in area.x..area.x.saturating_add(area.width) {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(style);
                    cell.set_symbol(&s);
                }
            }
        }
    }
}
