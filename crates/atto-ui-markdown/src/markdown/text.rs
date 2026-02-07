use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(super) fn slice_by_width(s: &str, start: u16, max_width: u16) -> (String, u16) {
    if s.is_empty() || max_width == 0 {
        return (String::new(), 0);
    }
    let mut result = String::new();
    let mut width: u16 = 0;
    let mut col: u16 = 0;

    for g in UnicodeSegmentation::graphemes(s, true) {
        let w = UnicodeWidthStr::width(g).max(1) as u16;
        let next = col.saturating_add(w);
        if next <= start {
            col = next;
            continue;
        }
        if width.saturating_add(w) > max_width {
            break;
        }
        result.push_str(g);
        width = width.saturating_add(w);
        col = next;
    }

    (result, width)
}

pub(super) fn split_text_at_width(s: &str, max_width: u16) -> (String, u16, String) {
    if s.is_empty() || max_width == 0 {
        return (String::new(), 0, s.to_string());
    }
    let mut head = String::new();
    let mut tail = String::new();
    let mut width: u16 = 0;
    let mut in_tail = false;

    for g in UnicodeSegmentation::graphemes(s, true) {
        let w = UnicodeWidthStr::width(g).max(1) as u16;
        if !in_tail && width.saturating_add(w) <= max_width {
            head.push_str(g);
            width = width.saturating_add(w);
        } else {
            in_tail = true;
            tail.push_str(g);
        }
    }

    (head, width, tail)
}

pub(super) fn text_width(s: &str) -> u16 {
    UnicodeWidthStr::width(s).min(u16::MAX as usize) as u16
}

pub(super) fn normalize_tabs(s: &str) -> String {
    s.replace('\t', "    ")
}
