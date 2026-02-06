use std::sync::Arc;

use crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::composable::{Component, ComponentContext, EventResult};
use crate::reactive::Binding;

type LinkCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// A single-line label that supports a small subset of inline markdown-like styling.
///
/// Supported syntax (markers are hidden in the rendered output):
/// - `**bold**`
/// - `*italic*`
/// - `__underline__`
/// - `~~strikethrough~~`
/// - `[link text](url)` (link text is underlined; clicking calls `on_link(url)`)
///
/// Parsing is intentionally simple (no full markdown support).
#[derive(Clone)]
pub struct StyledLabel {
    text: Binding<String>,
    enabled: Binding<bool>,
    on_link: Option<LinkCallback>,
    last_area: Option<Rect>,
}

impl StyledLabel {
    pub fn new(text: impl Into<Binding<String>>) -> Self {
        Self {
            text: text.into(),
            enabled: true.into(),
            on_link: None,
            last_area: None,
        }
    }

    pub fn text(mut self, text: impl Into<Binding<String>>) -> Self {
        self.text = text.into();
        self
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    pub fn on_link<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.on_link = Some(Arc::new(callback));
        self
    }
}

impl Component for StyledLabel {
    fn is_focusable(&self) -> bool {
        false
    }

    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }

        let Event::Mouse(m) = event else {
            return EventResult::ignored();
        };
        if m.kind != MouseEventKind::Down(MouseButton::Left) {
            return EventResult::ignored();
        }

        let Some(area) = self.last_area else {
            return EventResult::ignored();
        };
        let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
            return EventResult::ignored();
        };
        if local_y != 0 {
            return EventResult::ignored();
        }

        let segments = parse_inline(&self.text.get());
        let mut col: u16 = 0;
        for seg in segments {
            let w = seg.display_width_u16();
            if let Some(url) = &seg.link_url {
                if local_x >= col && local_x < col.saturating_add(w) {
                    if let Some(cb) = &self.on_link {
                        cb(url);
                        return EventResult::consumed();
                    }
                }
            }
            col = col.saturating_add(w);
        }

        EventResult::ignored()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }

    fn desired_width(&self) -> Option<u16> {
        let segments = parse_inline(&self.text.get());
        let mut total: u16 = 0;
        for seg in segments {
            total = total.saturating_add(seg.display_width_u16());
        }
        Some(total)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let base = if self.enabled.get() {
            ctx.theme.widget.dim
        } else {
            ctx.theme.widget.disabled
        };
        let link_overlay = ctx.theme.named_style("markdown-link");

        let spans = parse_inline(&self.text.get())
            .into_iter()
            .filter(|seg| !seg.text.is_empty())
            .map(|seg| seg.to_span(base, link_overlay))
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

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

    // Events from nested containers are already relative to our own origin.
    if m.column < area.width && m.row < area.height {
        return Some((m.column, m.row));
    }

    None
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct InlineStyleFlags {
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InlineSegment {
    text: String,
    style: InlineStyleFlags,
    link_url: Option<String>,
}

impl InlineSegment {
    fn display_width_u16(&self) -> u16 {
        let w = UnicodeWidthStr::width(self.text.as_str());
        w.min(u16::MAX as usize) as u16
    }

    fn to_span(self, base: Style, link_overlay: Option<Style>) -> Span<'static> {
        let mut style = base;
        if self.style.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.style.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if self.style.underline {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        if self.style.strikethrough {
            style = style.add_modifier(Modifier::CROSSED_OUT);
        }

        if self.link_url.is_some() {
            if let Some(overlay) = link_overlay {
                style = style.patch(overlay);
            } else {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
        }

        Span::styled(self.text, style)
    }
}

fn push_segment(segments: &mut Vec<InlineSegment>, seg: InlineSegment) {
    if seg.text.is_empty() {
        return;
    }
    if let Some(last) = segments.last_mut()
        && last.style == seg.style
        && last.link_url == seg.link_url
    {
        last.text.push_str(&seg.text);
        return;
    }
    segments.push(seg);
}

fn parse_inline(input: &str) -> Vec<InlineSegment> {
    let mut segments = Vec::new();
    let mut plain = String::new();

    let mut i = 0;
    while i < input.len() {
        let rest = &input[i..];

        if rest.starts_with("**") {
            if let Some((content, consumed)) = parse_delimited(rest, "**", "**") {
                flush_plain(&mut segments, &mut plain);
                push_segment(
                    &mut segments,
                    InlineSegment {
                        text: content.to_string(),
                        style: InlineStyleFlags {
                            bold: true,
                            ..Default::default()
                        },
                        link_url: None,
                    },
                );
                i += consumed;
                continue;
            }
            plain.push_str("**");
            i += 2;
            continue;
        }

        if rest.starts_with("__") {
            if let Some((content, consumed)) = parse_delimited(rest, "__", "__") {
                flush_plain(&mut segments, &mut plain);
                push_segment(
                    &mut segments,
                    InlineSegment {
                        text: content.to_string(),
                        style: InlineStyleFlags {
                            underline: true,
                            ..Default::default()
                        },
                        link_url: None,
                    },
                );
                i += consumed;
                continue;
            }
            plain.push_str("__");
            i += 2;
            continue;
        }

        if rest.starts_with("~~") {
            if let Some((content, consumed)) = parse_delimited(rest, "~~", "~~") {
                flush_plain(&mut segments, &mut plain);
                push_segment(
                    &mut segments,
                    InlineSegment {
                        text: content.to_string(),
                        style: InlineStyleFlags {
                            strikethrough: true,
                            ..Default::default()
                        },
                        link_url: None,
                    },
                );
                i += consumed;
                continue;
            }
            plain.push_str("~~");
            i += 2;
            continue;
        }

        if rest.starts_with('*') {
            if let Some((content, consumed)) = parse_italic(rest) {
                flush_plain(&mut segments, &mut plain);
                push_segment(
                    &mut segments,
                    InlineSegment {
                        text: content.to_string(),
                        style: InlineStyleFlags {
                            italic: true,
                            ..Default::default()
                        },
                        link_url: None,
                    },
                );
                i += consumed;
                continue;
            }
            plain.push('*');
            i += 1;
            continue;
        }

        if rest.starts_with('[') {
            if let Some(parsed) = parse_link(rest) {
                flush_plain(&mut segments, &mut plain);
                push_segment(
                    &mut segments,
                    InlineSegment {
                        text: parsed.text.to_string(),
                        style: InlineStyleFlags::default(),
                        link_url: Some(parsed.url.to_string()),
                    },
                );
                i += parsed.consumed;
                continue;
            }
            plain.push('[');
            i += 1;
            continue;
        }

        let ch = rest.chars().next().unwrap_or('\0');
        if ch == '\0' {
            break;
        }
        plain.push(ch);
        i += ch.len_utf8();
    }

    flush_plain(&mut segments, &mut plain);
    segments
}

fn flush_plain(segments: &mut Vec<InlineSegment>, plain: &mut String) {
    if plain.is_empty() {
        return;
    }
    let text = std::mem::take(plain);
    push_segment(
        segments,
        InlineSegment {
            text,
            style: InlineStyleFlags::default(),
            link_url: None,
        },
    );
}

fn parse_delimited<'a>(input: &'a str, open: &str, close: &str) -> Option<(&'a str, usize)> {
    if !input.starts_with(open) {
        return None;
    }
    let after_open = &input[open.len()..];
    let end_rel = after_open.find(close)?;
    let content = &after_open[..end_rel];
    let consumed = open.len() + end_rel + close.len();
    Some((content, consumed))
}

fn parse_italic(input: &str) -> Option<(&str, usize)> {
    if !input.starts_with('*') || input.starts_with("**") {
        return None;
    }
    let after_open = &input[1..];
    let end_rel = find_next_single_delim(after_open, b'*')?;
    let content = &after_open[..end_rel];
    let consumed = 1 + end_rel + 1;
    Some((content, consumed))
}

fn find_next_single_delim(input: &str, byte: u8) -> Option<usize> {
    let bytes = input.as_bytes();
    for (idx, b) in bytes.iter().enumerate() {
        if *b != byte {
            continue;
        }

        let prev_is = idx > 0 && bytes[idx.saturating_sub(1)] == byte;
        let next_is = idx + 1 < bytes.len() && bytes[idx.saturating_add(1)] == byte;
        if prev_is || next_is {
            continue;
        }

        return Some(idx);
    }
    None
}

struct ParsedLink<'a> {
    text: &'a str,
    url: &'a str,
    consumed: usize,
}

fn parse_link(input: &str) -> Option<ParsedLink<'_>> {
    if !input.starts_with('[') {
        return None;
    }

    let text_end = input[1..].find(']')? + 1;
    let text = &input[1..text_end];

    let after_bracket = input.get(text_end + 1..)?;
    if !after_bracket.starts_with('(') {
        return None;
    }

    let url_end = after_bracket[1..].find(')')? + 1;
    let url = &after_bracket[1..url_end];

    let consumed = text_end + url_end + 2;
    Some(ParsedLink {
        text,
        url,
        consumed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display_text(input: &str) -> String {
        parse_inline(input)
            .into_iter()
            .map(|seg| seg.text)
            .collect::<String>()
    }

    #[test]
    fn strips_supported_markers() {
        let input = "**BOLD** *ITALIC* __UNDER__ ~~STRIKE~~ [LINK](https://example.com)";
        assert_eq!(
            display_text(input),
            "BOLD ITALIC UNDER STRIKE LINK".to_string()
        );
    }

    #[test]
    fn leaves_unclosed_markers_literal() {
        assert_eq!(display_text("**oops"), "**oops".to_string());
        assert_eq!(display_text("*oops"), "*oops".to_string());
        assert_eq!(display_text("__oops"), "__oops".to_string());
        assert_eq!(display_text("~~oops"), "~~oops".to_string());
        assert_eq!(display_text("[x](y"), "[x](y".to_string());
    }

    #[test]
    fn captures_link_url_for_callback() {
        let segs = parse_inline("go [here](url)!");
        let link = segs.iter().find(|s| s.link_url.is_some()).unwrap();
        assert_eq!(link.text, "here");
        assert_eq!(link.link_url.as_deref(), Some("url"));
    }
}
