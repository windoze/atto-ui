use ratatui::layout::Rect;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::model::{MenuItem, MenuSpec};

pub(super) const SYSTEM_MENU_ICON_FALLBACK: &str = "≡";
const SYSTEM_MENU_ICON_WIDTH: u16 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DisplayLabel {
    pub(super) text: String,
    pub(super) mnemonic_byte: Option<usize>,
}

pub(super) fn display_label(label: &str) -> DisplayLabel {
    let mut text = String::with_capacity(label.len());
    let mut mnemonic_byte = None;
    let mut graphemes = label.graphemes(true).peekable();

    while let Some(grapheme) = graphemes.next() {
        let marker_boundary = text.chars().last().is_none_or(|ch| ch.is_whitespace());
        let next_is_mnemonic = graphemes
            .peek()
            .is_some_and(|next| next.chars().any(|ch| !ch.is_whitespace()));
        if marker_boundary && next_is_mnemonic && (grapheme == "&" || grapheme == "_") {
            let Some(next) = graphemes.next() else {
                break;
            };
            let start = text.len();
            text.push_str(next);
            if mnemonic_byte.is_none() {
                mnemonic_byte = Some(start);
            }
            continue;
        }

        text.push_str(grapheme);
    }

    DisplayLabel {
        text,
        mnemonic_byte,
    }
}

pub(super) fn display_label_width(label: &str) -> usize {
    UnicodeWidthStr::width(display_label(label).text.as_str())
}

pub(super) fn explicit_mnemonic(label: &str) -> Option<char> {
    let display = display_label(label);
    let offset = display.mnemonic_byte?;
    display.text.get(offset..)?.chars().next()
}

pub(super) fn label_mnemonic_or_first(label: &str) -> Option<char> {
    explicit_mnemonic(label).or_else(|| display_label(label).text.trim_start().chars().next())
}

pub(super) fn contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

pub(super) fn dropdown_size(items: &[MenuItem]) -> (u16, u16) {
    let mut w: usize = 8;
    for item in items {
        let label = item.label.get();
        let mut row_w = display_label_width(&label);
        if let Some(accelerator) = item.accelerator_text() {
            row_w += 2 + UnicodeWidthStr::width(accelerator.as_str());
        }
        if !item.submenu.is_empty() {
            row_w += 2;
        }
        w = w.max(row_w);
    }
    let width = (w + 2).min(u16::MAX as usize) as u16; // + borders
    let height = (items.len() + 2).min(u16::MAX as usize) as u16;
    (width, height)
}

pub(super) fn menu_titles_start_x(menu_bar_area: Rect) -> u16 {
    menu_bar_area
        .x
        .saturating_add(menu_bar_area.width.min(SYSTEM_MENU_ICON_WIDTH))
}

pub(super) fn menu_title_x(menus: &[MenuSpec], start_x: u16, menu_index: usize) -> u16 {
    let mut x = start_x;
    for (idx, menu) in menus.iter().enumerate() {
        if idx == menu_index {
            return x;
        }
        let label = format!(" {} ", display_label(&menu.title.get()).text);
        x = x.saturating_add(UnicodeWidthStr::width(label.as_str()) as u16 + 1);
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_label_strips_marker_and_tracks_mnemonic() {
        let label = display_label("&File");
        assert_eq!(label.text, "File");
        assert_eq!(label.mnemonic_byte, Some(0));
        assert_eq!(explicit_mnemonic("&File"), Some('F'));

        let unicode = display_label("_文件");
        assert_eq!(unicode.text, "文件");
        assert_eq!(unicode.mnemonic_byte, Some(0));
        assert_eq!(explicit_mnemonic("_文件"), Some('文'));
    }

    #[test]
    fn display_label_preserves_literal_marker_characters() {
        assert_eq!(display_label("file_name").text, "file_name");
        assert_eq!(display_label("Rock & Roll").text, "Rock & Roll");
    }

    #[test]
    fn dropdown_size_uses_stripped_label_and_accelerator() {
        let item = MenuItem::action("&Open", || {}).accelerator("Ctrl+O");
        let (width, height) = dropdown_size(&[item]);

        assert_eq!(height, 3);
        assert_eq!(width, 14);
    }

    #[test]
    fn dropdown_size_reserves_space_for_submenu_arrow() {
        let item = MenuItem::submenu("&More", vec![MenuItem::action("Child", || {})])
            .accelerator("Ctrl+M");
        let (width, height) = dropdown_size(&[item]);

        assert_eq!(height, 3);
        assert_eq!(width, 16);
    }
}
