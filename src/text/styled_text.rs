use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct InlineStyleFlags {
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    color: Option<Color>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StyledTextSegment {
    text: String,
    style: InlineStyleFlags,
    link_url: Option<String>,
}

impl StyledTextSegment {
    pub(crate) fn structured(
        text: impl Into<String>,
        bold: bool,
        italic: bool,
        underline: bool,
        strike: bool,
        color: Option<Color>,
        link_url: Option<String>,
    ) -> Self {
        Self {
            text: text.into(),
            style: InlineStyleFlags {
                bold,
                italic,
                underline,
                strikethrough: strike,
                color,
            },
            link_url,
        }
    }
}

pub(crate) fn normalize_segments(
    segments: impl IntoIterator<Item = StyledTextSegment>,
) -> Vec<StyledTextSegment> {
    let mut out = Vec::new();
    for segment in segments {
        push_segment(&mut out, segment);
    }
    out
}

pub(crate) fn parse_inline(input: &str) -> Vec<StyledTextSegment> {
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
                    StyledTextSegment {
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
                    StyledTextSegment {
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
                    StyledTextSegment {
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
                    StyledTextSegment {
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
                    StyledTextSegment {
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

pub(crate) fn inline_display_width(input: &str) -> u16 {
    segments_display_width(&parse_inline(input))
}

pub(crate) fn segments_display_width(segments: &[StyledTextSegment]) -> u16 {
    let mut total = 0_u16;
    for seg in segments {
        let w = UnicodeWidthStr::width(seg.text.as_str());
        total = total.saturating_add(w.min(u16::MAX as usize) as u16);
    }
    total
}

pub(crate) fn hit_test_link(segments: &[StyledTextSegment], x: u16) -> Option<&str> {
    let mut col: u16 = 0;
    for seg in segments {
        let w = UnicodeWidthStr::width(seg.text.as_str());
        let width = w.min(u16::MAX as usize) as u16;
        if seg.link_url.is_some() && x >= col && x < col.saturating_add(width) {
            return seg.link_url.as_deref();
        }
        col = col.saturating_add(width);
    }
    None
}

pub(crate) fn spans_from_inline(
    input: &str,
    base: Style,
    link_overlay: Option<Style>,
) -> Vec<Span<'static>> {
    let segments = parse_inline(input);
    spans_from_segments(&segments, base, link_overlay)
}

pub(crate) fn spans_from_segments(
    segments: &[StyledTextSegment],
    base: Style,
    link_overlay: Option<Style>,
) -> Vec<Span<'static>> {
    segments
        .iter()
        .filter(|seg| !seg.text.is_empty())
        .map(|seg| segment_to_span(seg, base, link_overlay))
        .collect()
}

pub(crate) fn slice_spans_from_segments(
    segments: &[StyledTextSegment],
    start_col: u16,
    width: u16,
    base: Style,
    link_overlay: Option<Style>,
) -> Vec<Span<'static>> {
    let sliced = slice_segments(segments, start_col, width);
    spans_from_segments(&sliced, base, link_overlay)
}

fn segment_to_span(
    seg: &StyledTextSegment,
    base: Style,
    link_overlay: Option<Style>,
) -> Span<'static> {
    let mut style = base;
    if seg.style.bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if seg.style.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if seg.style.underline {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if seg.style.strikethrough {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    if let Some(color) = seg.style.color {
        style = style.fg(color);
    }

    if seg.link_url.is_some() {
        if let Some(overlay) = link_overlay {
            style = style.patch(overlay);
        } else {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
    }

    Span::styled(seg.text.clone(), style)
}

fn slice_segments(
    segments: &[StyledTextSegment],
    start_col: u16,
    width: u16,
) -> Vec<StyledTextSegment> {
    if width == 0 {
        return Vec::new();
    }
    let end = start_col.saturating_add(width);
    let mut out = Vec::new();
    let mut col: u16 = 0;

    for seg in segments {
        if col >= end {
            break;
        }

        let mut current = StyledTextSegment {
            text: String::new(),
            style: seg.style,
            link_url: seg.link_url.clone(),
        };

        for g in seg.text.graphemes(true) {
            let w = (UnicodeWidthStr::width(g) as u16).max(1);
            let next = col.saturating_add(w);

            if next <= start_col {
                col = next;
                continue;
            }
            if col >= end {
                break;
            }

            current.text.push_str(g);
            col = next;
        }

        push_segment(&mut out, current);
    }

    out
}

fn push_segment(segments: &mut Vec<StyledTextSegment>, seg: StyledTextSegment) {
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

fn flush_plain(segments: &mut Vec<StyledTextSegment>, plain: &mut String) {
    if plain.is_empty() {
        return;
    }
    let text = std::mem::take(plain);
    push_segment(
        segments,
        StyledTextSegment {
            text,
            style: InlineStyleFlags::default(),
            link_url: None,
        },
    );
}

pub(crate) fn parse_text_color(input: &str) -> Result<Color, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("expected color name or #RGB/#RRGGBB".to_string());
    }

    if let Some(hex) = trimmed.strip_prefix('#') {
        let bytes = hex.as_bytes();
        let (r, g, b) = match bytes.len() {
            3 => (
                parse_short_hex_channel(bytes[0])?,
                parse_short_hex_channel(bytes[1])?,
                parse_short_hex_channel(bytes[2])?,
            ),
            6 => (
                parse_hex_channel(bytes[0], bytes[1])?,
                parse_hex_channel(bytes[2], bytes[3])?,
                parse_hex_channel(bytes[4], bytes[5])?,
            ),
            _ => return Err(format!("expected #RGB or #RRGGBB, got {trimmed:?}")),
        };
        return Ok(Color::Rgb(r, g, b));
    }

    if let Some(index) = trimmed
        .strip_prefix("indexed:")
        .or_else(|| trimmed.strip_prefix("ansi:"))
    {
        let value = index
            .parse::<u8>()
            .map_err(|_| format!("invalid indexed color {trimmed:?}"))?;
        return Ok(Color::Indexed(value));
    }

    let color = match trimmed.to_ascii_lowercase().as_str() {
        "reset" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        "white" => Color::White,
        _ => return Err(format!("unknown color {trimmed:?}")),
    };
    Ok(color)
}

fn parse_short_hex_channel(hex: u8) -> Result<u8, String> {
    let value = hex_value(hex)?;
    Ok((value << 4) | value)
}

fn parse_hex_channel(high: u8, low: u8) -> Result<u8, String> {
    Ok((hex_value(high)? << 4) | hex_value(low)?)
}

fn hex_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err("invalid hex digit".to_string()),
    }
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

    #[test]
    fn hit_test_resolves_link() {
        let segs = parse_inline("go [here](url)!");
        let link_x = "go ".len() as u16;
        assert_eq!(hit_test_link(&segs, link_x), Some("url"));
    }

    #[test]
    fn structured_segments_merge_adjacent_matching_styles() {
        let segments = normalize_segments([
            StyledTextSegment::structured("", false, false, false, false, None, None),
            StyledTextSegment::structured("a", true, false, false, false, Some(Color::Red), None),
            StyledTextSegment::structured("b", true, false, false, false, Some(Color::Red), None),
            StyledTextSegment::structured("c", true, false, false, false, Some(Color::Blue), None),
        ]);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "ab");
        assert_eq!(segments[0].style.color, Some(Color::Red));
        assert_eq!(segments[1].text, "c");
    }

    #[test]
    fn parses_structured_text_colors() {
        assert_eq!(parse_text_color("red"), Ok(Color::Red));
        assert_eq!(parse_text_color("#0a7"), Ok(Color::Rgb(0x00, 0xaa, 0x77)));
        assert_eq!(
            parse_text_color("#112233"),
            Ok(Color::Rgb(0x11, 0x22, 0x33))
        );
        assert_eq!(parse_text_color("indexed:42"), Ok(Color::Indexed(42)));
        assert!(parse_text_color("not-a-color").is_err());
    }
}
