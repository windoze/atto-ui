use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;

#[derive(Clone, Debug, Default)]
pub struct StatusBar {
    left: String,
    right: String,
    custom: Option<(String, String)>,
}

impl StatusBar {
    pub fn set_left(&mut self, text: impl Into<String>) {
        self.left = text.into();
    }

    pub fn set_right(&mut self, text: impl Into<String>) {
        self.right = text.into();
    }

    pub fn set_custom(&mut self, left: impl Into<String>, right: impl Into<String>) {
        self.custom = Some((left.into(), right.into()));
    }

    pub fn clear_custom(&mut self) {
        self.custom = None;
    }

    pub fn has_custom(&self) -> bool {
        self.custom.is_some()
    }

    pub fn draw(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }
        let width = area.width as usize;
        let (left, right) = self
            .custom
            .as_ref()
            .map(|(left, right)| (left.as_str(), right.as_str()))
            .unwrap_or((self.left.as_str(), self.right.as_str()));
        let line = build_status_line(left, right, width);

        let p = Paragraph::new(Line::from(vec![Span::styled(line, theme.status_bar)]));
        frame.render_widget(p, area);
    }
}

fn build_status_line(left: &str, right: &str, width: usize) -> String {
    let left_w = UnicodeWidthStr::width(left);
    let right_w = UnicodeWidthStr::width(right);
    let mut line = String::new();

    if left_w >= width {
        let used = push_graphemes_up_to_width(&mut line, left, width);
        line.push_str(&" ".repeat(width.saturating_sub(used)));
        return line;
    }

    line.push_str(left);
    let remaining = width.saturating_sub(left_w);
    if right_w <= remaining {
        line.push_str(&" ".repeat(remaining.saturating_sub(right_w)));
        line.push_str(right);
    } else {
        line.push_str(&" ".repeat(remaining));
    }
    line
}

fn push_graphemes_up_to_width(out: &mut String, text: &str, max_width: usize) -> usize {
    if max_width == 0 {
        return 0;
    }

    let mut used = 0usize;
    for (_, grapheme) in text.grapheme_indices(true) {
        let grapheme_w = UnicodeWidthStr::width(grapheme);
        let next = used.saturating_add(grapheme_w);
        if next > max_width {
            break;
        }
        out.push_str(grapheme);
        used = next;
    }
    used
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_line_aligns_right_text_by_display_width() {
        let line = build_status_line("状态", "🦀", 12);

        assert_eq!(UnicodeWidthStr::width(line.as_str()), 12);
        assert!(line.starts_with("状态"));
        assert!(line.ends_with("🦀"));
    }

    #[test]
    fn status_line_right_aligns_ascii() {
        let line = build_status_line("left", "right", 12);

        assert_eq!(line, "left   right");
        assert_eq!(UnicodeWidthStr::width(line.as_str()), 12);
    }

    #[test]
    fn status_line_right_aligns_cjk() {
        let line = build_status_line("左", "右", 10);

        assert_eq!(line, "左      右");
        assert_eq!(UnicodeWidthStr::width(line.as_str()), 10);
    }

    #[test]
    fn status_line_right_aligns_mixed_width_text() {
        let line = build_status_line("A你", "B🦀", 10);

        assert_eq!(line, "A你    B🦀");
        assert_eq!(UnicodeWidthStr::width(line.as_str()), 10);
    }

    #[test]
    fn status_line_truncates_left_on_grapheme_boundary() {
        let line = build_status_line("你好你好", "", 5);

        assert_eq!(line, "你好 ");
        assert_eq!(UnicodeWidthStr::width(line.as_str()), 5);
    }

    #[test]
    fn status_line_truncates_emoji_sequence_on_grapheme_boundary() {
        let line = build_status_line("👨‍👩‍👧‍👦abc", "", 1);

        assert_eq!(line, " ");
        assert_eq!(UnicodeWidthStr::width(line.as_str()), 1);
    }
}
