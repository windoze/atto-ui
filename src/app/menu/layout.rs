use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use super::model::{MenuItem, MenuSpec};

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
        let mut row_w = UnicodeWidthStr::width(label.as_str());
        if let Some(sc) = item.shortcut.get() {
            row_w += 2 + UnicodeWidthStr::width(sc.as_str());
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

pub(super) fn menu_title_x(menus: &[MenuSpec], start_x: u16, menu_index: usize) -> u16 {
    let mut x = start_x;
    for (idx, menu) in menus.iter().enumerate() {
        if idx == menu_index {
            return x;
        }
        let label = format!(" {} ", menu.title.get());
        x = x.saturating_add(UnicodeWidthStr::width(label.as_str()) as u16 + 1);
    }
    x
}
