use std::borrow::Cow;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::{
    Component, ComponentContext, EventHandling, EventResult, FocusNav, Layout, Scrollable,
};
use crate::reactive::Binding;

const DEFAULT_SOFT_LIMIT: usize = 200;

#[derive(Clone)]
pub struct WindowedText {
    text: Binding<String>,
    expanded: Binding<bool>,
    soft_limit: usize,
    style: Option<Style>,
    footer_style: Option<Style>,
    scroll_x: u16,
    scroll_y: u16,
    viewport: (u16, u16),
}

impl WindowedText {
    pub fn new(text: impl Into<Binding<String>>) -> Self {
        Self {
            text: text.into(),
            expanded: false.into(),
            soft_limit: DEFAULT_SOFT_LIMIT,
            style: None,
            footer_style: None,
            scroll_x: 0,
            scroll_y: 0,
            viewport: (0, 0),
        }
    }

    pub fn expanded(mut self, expanded: impl Into<Binding<bool>>) -> Self {
        self.expanded = expanded.into();
        self
    }

    pub fn soft_limit(mut self, soft_limit: usize) -> Self {
        self.soft_limit = soft_limit.max(1);
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn footer_style(mut self, style: Style) -> Self {
        self.footer_style = Some(style);
        self
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded.get()
    }

    pub fn set_expanded(&mut self, expanded: bool) {
        self.expanded.set(expanded);
        self.clamp_scroll();
    }

    fn text(&self) -> String {
        self.text.get()
    }

    fn total_lines(&self) -> usize {
        line_count(&self.text())
    }

    fn render_line_count_for_text(&self, text: &str) -> usize {
        render_line_count(text, self.expanded.get(), self.soft_limit)
    }

    fn content_width_for_text(&self, text: &str) -> u16 {
        content_width(text, self.expanded.get(), self.soft_limit)
    }

    fn max_offsets_for_text(&self, text: &str) -> (u16, u16) {
        let (viewport_w, viewport_h) = self.viewport;
        let content_w = self.content_width_for_text(text) as usize;
        let content_h = self.render_line_count_for_text(text);
        let max_x = content_w
            .saturating_sub(viewport_w as usize)
            .min(u16::MAX as usize) as u16;
        let max_y = content_h
            .saturating_sub(viewport_h as usize)
            .min(u16::MAX as usize) as u16;
        (max_x, max_y)
    }

    fn clamp_scroll(&mut self) {
        let text = self.text();
        let (max_x, max_y) = self.max_offsets_for_text(&text);
        self.scroll_x = self.scroll_x.min(max_x);
        self.scroll_y = self.scroll_y.min(max_y);
    }
}

impl Component for WindowedText {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.viewport = (area.width, area.height);
        self.clamp_scroll();
        if area.width == 0 || area.height == 0 {
            return;
        }

        let text = self.text();
        let base_style = self.style.unwrap_or(ctx.theme.widget.normal);
        let footer_style = self
            .footer_style
            .or_else(|| ctx.theme.named_style("windowed-text-footer"))
            .unwrap_or(ctx.theme.widget.dim);
        let lines = visible_lines(
            &text,
            VisibleTextWindow {
                expanded: self.expanded.get(),
                soft_limit: self.soft_limit,
                scroll_x: self.scroll_x,
                scroll_y: self.scroll_y,
                height: area.height,
                base_style,
                footer_style,
            },
        );
        frame.render_widget(Paragraph::new(lines), area);
    }
}

impl Layout for WindowedText {
    fn desired_width(&self) -> Option<u16> {
        Some(self.content_width_for_text(&self.text()))
    }

    fn desired_height(&self) -> Option<u16> {
        Some(
            self.render_line_count_for_text(&self.text())
                .min(u16::MAX as usize) as u16,
        )
    }
}

impl Scrollable for WindowedText {
    fn is_scrollable(&self) -> bool {
        true
    }

    fn content_size(&self) -> (u16, u16) {
        let text = self.text();
        (
            self.content_width_for_text(&text),
            self.render_line_count_for_text(&text)
                .min(u16::MAX as usize) as u16,
        )
    }

    fn scroll_offset(&self) -> (u16, u16) {
        (self.scroll_x, self.scroll_y)
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.viewport
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.scroll_x = x;
        self.scroll_y = y;
        self.clamp_scroll();
    }
}

impl FocusNav for WindowedText {
    fn is_focusable(&self) -> bool {
        true
    }
}

impl EventHandling for WindowedText {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        match event {
            Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Press,
                ..
            }) => self.handle_key(*code),
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll_y = self.scroll_y.saturating_sub(3);
                    EventResult::consumed()
                }
                MouseEventKind::ScrollDown => {
                    let next = self.scroll_y.saturating_add(3);
                    self.set_scroll_offset(self.scroll_x, next);
                    EventResult::consumed()
                }
                MouseEventKind::ScrollLeft => {
                    self.scroll_x = self.scroll_x.saturating_sub(3);
                    EventResult::consumed()
                }
                MouseEventKind::ScrollRight => {
                    let next = self.scroll_x.saturating_add(3);
                    self.set_scroll_offset(next, self.scroll_y);
                    EventResult::consumed()
                }
                _ => EventResult::ignored(),
            },
            _ => EventResult::ignored(),
        }
    }
}

impl WindowedText {
    fn handle_key(&mut self, code: KeyCode) -> EventResult {
        match code {
            KeyCode::Char('e') | KeyCode::Char('E') => {
                if self.expanded.get() || self.total_lines() <= self.soft_limit {
                    return EventResult::ignored();
                }
                self.expanded.set(true);
                self.clamp_scroll();
                EventResult::changed()
            }
            KeyCode::Up => {
                self.scroll_y = self.scroll_y.saturating_sub(1);
                EventResult::consumed()
            }
            KeyCode::Down => {
                let next = self.scroll_y.saturating_add(1);
                self.set_scroll_offset(self.scroll_x, next);
                EventResult::consumed()
            }
            KeyCode::Left => {
                self.scroll_x = self.scroll_x.saturating_sub(1);
                EventResult::consumed()
            }
            KeyCode::Right => {
                let next = self.scroll_x.saturating_add(1);
                self.set_scroll_offset(next, self.scroll_y);
                EventResult::consumed()
            }
            KeyCode::PageUp => {
                let step = self.viewport.1.max(1);
                self.scroll_y = self.scroll_y.saturating_sub(step);
                EventResult::consumed()
            }
            KeyCode::PageDown => {
                let step = self.viewport.1.max(1);
                let next = self.scroll_y.saturating_add(step);
                self.set_scroll_offset(self.scroll_x, next);
                EventResult::consumed()
            }
            KeyCode::Home => {
                self.set_scroll_offset(self.scroll_x, 0);
                EventResult::consumed()
            }
            KeyCode::End => {
                let text = self.text();
                let (_, max_y) = self.max_offsets_for_text(&text);
                self.set_scroll_offset(self.scroll_x, max_y);
                EventResult::consumed()
            }
            _ => EventResult::ignored(),
        }
    }
}

impl crate::composable::DynamicTree for WindowedText {}

#[derive(Clone, Copy)]
struct VisibleTextWindow {
    expanded: bool,
    soft_limit: usize,
    scroll_x: u16,
    scroll_y: u16,
    height: u16,
    base_style: Style,
    footer_style: Style,
}

fn visible_lines(text: &str, window: VisibleTextWindow) -> Vec<Line<'static>> {
    let offset = window.scroll_y as usize;
    let limit = window.height as usize;
    windowed_text_lines(text, window.expanded, window.soft_limit, offset, limit)
        .into_iter()
        .map(|line| {
            let style = if line.is_footer {
                window.footer_style
            } else {
                window.base_style
            };
            Line::styled(
                crop_display_start(line.text.as_ref(), window.scroll_x),
                style,
            )
        })
        .collect()
}

fn windowed_text_lines(
    text: &str,
    expanded: bool,
    soft_limit: usize,
    offset: usize,
    limit: usize,
) -> Vec<TextLine<'_>> {
    if limit == 0 {
        return Vec::new();
    }

    if expanded {
        return text
            .split('\n')
            .skip(offset)
            .take(limit)
            .map(|line| TextLine {
                text: Cow::Borrowed(line),
                is_footer: false,
            })
            .collect();
    }

    let total = line_count(text);
    let visible = total.min(soft_limit);
    let mut lines = text
        .split('\n')
        .take(visible)
        .skip(offset)
        .take(limit)
        .map(|line| TextLine {
            text: Cow::Borrowed(line),
            is_footer: false,
        })
        .collect::<Vec<_>>();

    let footer_index = visible;
    if total > soft_limit && offset <= footer_index && footer_index < offset.saturating_add(limit) {
        lines.push(TextLine {
            text: Cow::Owned(format!(
                "... {} more lines - press e to expand all",
                total.saturating_sub(soft_limit)
            )),
            is_footer: true,
        });
    }

    lines
}

fn materialized_lines(
    text: &str,
    expanded: bool,
    soft_limit: usize,
) -> impl Iterator<Item = TextLine<'_>> + '_ {
    let total = line_count(text);
    let visible = if expanded {
        total
    } else {
        total.min(soft_limit)
    };
    let footer = (!expanded && total > soft_limit).then(move || TextLine {
        text: Cow::Owned(format!(
            "... {} more lines - press e to expand all",
            total.saturating_sub(soft_limit)
        )),
        is_footer: true,
    });

    text.split('\n')
        .take(visible)
        .map(|line| TextLine {
            text: Cow::Borrowed(line),
            is_footer: false,
        })
        .chain(footer)
}

struct TextLine<'a> {
    text: Cow<'a, str>,
    is_footer: bool,
}

fn render_line_count(text: &str, expanded: bool, soft_limit: usize) -> usize {
    let total = line_count(text);
    if expanded || total <= soft_limit {
        total
    } else {
        soft_limit.saturating_add(1)
    }
}

fn line_count(text: &str) -> usize {
    text.split('\n').count()
}

fn content_width(text: &str, expanded: bool, soft_limit: usize) -> u16 {
    materialized_lines(text, expanded, soft_limit)
        .map(|line| UnicodeWidthStr::width(line.text.as_ref()))
        .max()
        .unwrap_or(0)
        .min(u16::MAX as usize) as u16
}

fn crop_display_start(text: &str, scroll_x: u16) -> String {
    if scroll_x == 0 {
        return text.to_string();
    }

    let mut col = 0u16;
    for (byte_idx, grapheme) in text.grapheme_indices(true) {
        let width = UnicodeWidthStr::width(grapheme).max(1) as u16;
        let next = col.saturating_add(width);
        if next > scroll_x {
            return text[byte_idx..].to_string();
        }
        col = next;
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_windowed_text_adds_expand_footer() {
        let text = "a\nb\nc\nd";

        let lines = visible_lines(
            text,
            VisibleTextWindow {
                expanded: false,
                soft_limit: 2,
                scroll_x: 0,
                scroll_y: 0,
                height: 4,
                base_style: Style::default(),
                footer_style: Style::default(),
            },
        );
        let rendered = lines
            .iter()
            .map(|line| line.spans[0].content.as_ref())
            .collect::<Vec<_>>();

        assert_eq!(
            rendered,
            vec!["a", "b", "... 2 more lines - press e to expand all"]
        );
    }

    #[test]
    fn visible_lines_only_materializes_viewport_window() {
        let text = (0..10_000)
            .map(|i| format!("line-{i:05}"))
            .collect::<Vec<_>>()
            .join("\n");

        let lines = visible_lines(
            &text,
            VisibleTextWindow {
                expanded: true,
                soft_limit: 200,
                scroll_x: 0,
                scroll_y: 9_990,
                height: 3,
                base_style: Style::default(),
                footer_style: Style::default(),
            },
        );
        let rendered = lines
            .iter()
            .map(|line| line.spans[0].content.as_ref())
            .collect::<Vec<_>>();

        assert_eq!(rendered, vec!["line-09990", "line-09991", "line-09992"]);
    }

    #[test]
    fn expanded_window_rows_borrow_visible_slice() {
        let text = (0..10_000)
            .map(|i| format!("line-{i:05}"))
            .collect::<Vec<_>>()
            .join("\n");

        let lines = windowed_text_lines(&text, true, 200, 9_990, 3);
        let rendered = lines
            .iter()
            .map(|line| line.text.as_ref())
            .collect::<Vec<_>>();

        assert_eq!(rendered, vec!["line-09990", "line-09991", "line-09992"]);
        assert!(
            lines
                .iter()
                .all(|line| matches!(line.text, Cow::Borrowed(_))),
            "visible regular lines should be borrowed rather than materialized"
        );
    }
}
