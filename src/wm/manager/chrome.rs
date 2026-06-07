// Window chrome helpers shared by rendering and event routing.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::composable::TitleBarContent;
use crate::composable::scroll::{scrollbar_layout_1d, should_show_scrollbar};
use crate::theme::Theme;
use crate::wm::WindowDecorations;

use super::*;

pub(super) fn draw_window_border_scrollbars(
    buf: &mut Buffer,
    rect: Rect,
    inner: Rect,
    view: &dyn crate::composable::Component,
    theme: &Theme,
) {
    if rect.width < 3 || rect.height < 3 || inner.width == 0 || inner.height == 0 {
        return;
    }
    if !view.is_scrollable() {
        return;
    }

    let cfg = view.scroll_config();
    let (content_w, content_h) = view.content_size();
    let (viewport_w, viewport_h) = view.viewport_size();
    let (scroll_x, scroll_y) = view.scroll_offset();

    let show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_h, viewport_h);
    let show_h = should_show_scrollbar(cfg.horizontal_scrollbar, content_w, viewport_w);

    let thumb_style = theme.window_bg.patch(theme.scrollbar_thumb);
    let arrow_style = theme.window_bg.patch(theme.scrollbar_arrow);

    let thumb = theme.glyph("scrollbar-thumb").unwrap_or("█");
    let arrow_up = theme.glyph("scrollbar-up-arrow").unwrap_or("▲");
    let arrow_down = theme.glyph("scrollbar-down-arrow").unwrap_or("▼");
    let arrow_left = theme.glyph("scrollbar-left-arrow").unwrap_or("◄");
    let arrow_right = theme.glyph("scrollbar-right-arrow").unwrap_or("►");

    // Vertical scrollbar on the right border (excluding corners).
    if show_v {
        let layout = scrollbar_layout_1d(inner.height, viewport_h, content_h, scroll_y, cfg.arrows);
        let x = rect.x.saturating_add(rect.width).saturating_sub(1);
        for i in 0..inner.height {
            let (symbol, style) = if layout.has_arrows && i == 0 {
                (arrow_up, arrow_style)
            } else if layout.has_arrows && i == layout.bar_len.saturating_sub(1) {
                (arrow_down, arrow_style)
            } else if i >= layout.thumb_start
                && i < layout.thumb_start.saturating_add(layout.thumb_len)
            {
                (thumb, thumb_style)
            } else {
                continue;
            };
            if let Some(cell) = buf.cell_mut((x, inner.y.saturating_add(i))) {
                cell.set_symbol(symbol);
                cell.set_style(style);
            }
        }
    }

    // Horizontal scrollbar on the bottom border (excluding corners).
    if show_h {
        let layout = scrollbar_layout_1d(inner.width, viewport_w, content_w, scroll_x, cfg.arrows);
        let y = rect.y.saturating_add(rect.height).saturating_sub(1);
        for i in 0..inner.width {
            let (symbol, style) = if layout.has_arrows && i == 0 {
                (arrow_left, arrow_style)
            } else if layout.has_arrows && i == layout.bar_len.saturating_sub(1) {
                (arrow_right, arrow_style)
            } else if i >= layout.thumb_start
                && i < layout.thumb_start.saturating_add(layout.thumb_len)
            {
                (thumb, thumb_style)
            } else {
                continue;
            };
            if let Some(cell) = buf.cell_mut((inner.x.saturating_add(i), y)) {
                cell.set_symbol(symbol);
                cell.set_style(style);
            }
        }
    }
}

pub(super) struct TitleBarLayout {
    pub(super) text_area: Rect,
    pub(super) button_cols: Vec<(HitRegion, u16)>,
}

pub(super) fn titlebar_layout(rect: Rect, buttons: &WindowButtons) -> TitleBarLayout {
    if rect.width < 3 {
        return TitleBarLayout {
            text_area: Rect {
                x: rect.x,
                y: rect.y,
                width: 0,
                height: 1,
            },
            button_cols: Vec::new(),
        };
    }

    let inner_left = rect.x.saturating_add(1);
    let inner_right = rect.x.saturating_add(rect.width).saturating_sub(2);

    let mut button_cols = Vec::new();
    if buttons.minimize {
        button_cols.push((HitRegion::MinimizeButton, inner_right.saturating_sub(4)));
    }
    if buttons.maximize {
        button_cols.push((HitRegion::MaximizeButton, inner_right.saturating_sub(2)));
    }
    if buttons.close {
        button_cols.push((HitRegion::CloseButton, inner_right));
    }

    let reserved_right = button_cols
        .iter()
        .map(|(_, col)| *col)
        .min()
        .unwrap_or(inner_right)
        .saturating_sub(2);

    let width = if reserved_right >= inner_left {
        reserved_right.saturating_sub(inner_left).saturating_add(1)
    } else {
        0
    };

    TitleBarLayout {
        text_area: Rect {
            x: inner_left,
            y: rect.y,
            width,
            height: 1,
        },
        button_cols,
    }
}

pub(super) fn draw_titlebar_text(
    buf: &mut Buffer,
    layout: &TitleBarLayout,
    title: &str,
    style: Style,
) {
    if layout.text_area.width == 0 {
        return;
    }
    let mut cursor = layout.text_area.x;
    let right = layout
        .text_area
        .x
        .saturating_add(layout.text_area.width)
        .saturating_sub(1);

    for g in title.graphemes(true) {
        let w = (UnicodeWidthStr::width(g) as u16).max(1);
        let end = cursor.saturating_add(w).saturating_sub(1);
        if cursor > right || end > right {
            break;
        }
        let Some(cell) = buf.cell_mut((cursor, layout.text_area.y)) else {
            break;
        };
        cell.set_style(style);
        cell.set_symbol(g);

        for dx in 1..w {
            if let Some(trailing) = buf.cell_mut((cursor.saturating_add(dx), layout.text_area.y)) {
                trailing.reset();
            }
        }

        cursor = cursor.saturating_add(w);
    }
}

pub(super) fn draw_titlebar_spans(
    buf: &mut Buffer,
    layout: &TitleBarLayout,
    content: &TitleBarContent,
    fallback_style: Style,
) {
    if layout.text_area.width == 0 {
        return;
    }
    let mut cursor = layout.text_area.x;
    let right = layout
        .text_area
        .x
        .saturating_add(layout.text_area.width)
        .saturating_sub(1);

    for span in &content.spans {
        let style = span.style.unwrap_or(fallback_style);
        for g in span.text.graphemes(true) {
            let w = (UnicodeWidthStr::width(g) as u16).max(1);
            let end = cursor.saturating_add(w).saturating_sub(1);
            if cursor > right || end > right {
                return;
            }
            let Some(cell) = buf.cell_mut((cursor, layout.text_area.y)) else {
                return;
            };
            cell.set_style(style);
            cell.set_symbol(g);
            for dx in 1..w {
                if let Some(trailing) =
                    buf.cell_mut((cursor.saturating_add(dx), layout.text_area.y))
                {
                    trailing.reset();
                }
            }
            cursor = cursor.saturating_add(w);
        }
    }
}

pub(super) fn draw_titlebar_buttons(
    buf: &mut Buffer,
    layout: &TitleBarLayout,
    style: Style,
    theme: &Theme,
) {
    for (region, col) in &layout.button_cols {
        let symbol = match region {
            HitRegion::MinimizeButton => theme.glyph("minimize-button").unwrap_or("−"),
            HitRegion::MaximizeButton => theme.glyph("maximize-button").unwrap_or("□"),
            HitRegion::CloseButton => theme.glyph("close-button").unwrap_or("×"),
            _ => "?",
        };
        if let Some(cell) = buf.cell_mut((*col, layout.text_area.y)) {
            cell.set_style(style);
            cell.set_symbol(symbol);
        }
    }
}

pub(super) fn hit_test_buttons(w: &Window, x: u16, y: u16) -> Option<HitRegion> {
    let rect = w.rect.get();
    let deco = w.decorations.get();
    let buttons = effective_titlebar_buttons(w, &deco);
    if y != rect.y || rect.width < 3 {
        return None;
    }
    let inner_right = rect.x.saturating_add(rect.width).saturating_sub(2);
    if buttons.close && x == inner_right {
        return Some(HitRegion::CloseButton);
    }
    if buttons.maximize && x == inner_right.saturating_sub(2) {
        return Some(HitRegion::MaximizeButton);
    }
    if buttons.minimize && x == inner_right.saturating_sub(4) {
        return Some(HitRegion::MinimizeButton);
    }
    None
}

pub(super) fn effective_titlebar_buttons(w: &Window, deco: &WindowDecorations) -> WindowButtons {
    let mut buttons = deco.buttons.clone();
    if w.dock.get().is_some() {
        buttons.minimize = false;
        buttons.maximize = false;
        return buttons;
    }
    if !w.resizable.get() {
        buttons.minimize = false;
        buttons.maximize = false;
    }
    buttons
}

pub(super) fn can_toggle_maximize(w: &Window) -> bool {
    let decorations = w.decorations.get();
    let state = w.state.get();
    if state == WindowState::Maximized {
        // Always allow restoring from maximized, even if the window later becomes fixed-size.
        return true;
    }
    effective_titlebar_buttons(w, &decorations).maximize
}

pub(super) fn can_minimize(w: &Window) -> bool {
    let decorations = w.decorations.get();
    effective_titlebar_buttons(w, &decorations).minimize
}
