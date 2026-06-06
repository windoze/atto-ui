use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Clear};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::drawing::draw_shadow;
use crate::theme::Theme;

use super::super::status::Fill;
use super::layout::{dropdown_size, menu_title_x};
use super::model::{MenuBar, MenuItem};

impl MenuBar {
    pub fn draw(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }

        let mut x = area.x;
        for (idx, menu) in self.menus.iter().enumerate() {
            let is_active = self.state.active && idx == self.state.menu_index;
            let style = if is_active {
                theme.menu_bar_active
            } else {
                theme.menu_bar
            };
            let label = format!(" {} ", menu.title.get());
            let w = UnicodeWidthStr::width(label.as_str()) as u16;
            draw_text(frame.buffer_mut(), x, area.y, &label, style);
            x = x.saturating_add(w).saturating_add(1);
        }

        if self.state.active {
            self.draw_dropdowns(frame, area, theme);
        }
    }

    fn draw_dropdowns(&self, frame: &mut Frame<'_>, menu_bar_area: Rect, theme: &Theme) {
        let Some(menu) = self.menus.get(self.state.menu_index) else {
            return;
        };
        let screen = frame.area();

        let menu_x = menu_title_x(&self.menus, menu_bar_area.x, self.state.menu_index);
        let dropdown_y = menu_bar_area.y.saturating_add(1);

        let mut origin_x = menu_x;
        let mut origin_y = dropdown_y;
        let mut items = &menu.items;

        for (depth, &selected_idx) in self.state.stack.iter().enumerate() {
            let (w, h) = dropdown_size(items);
            let rect = Rect {
                x: origin_x,
                y: origin_y,
                width: w,
                height: h,
            };
            draw_shadow(frame.buffer_mut(), rect, screen, theme.window_shadow);
            frame.render_widget(Clear, rect);
            frame.render_widget(
                Fill {
                    style: theme.menu_item,
                    ch: ' ',
                },
                rect,
            );
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(theme.menu_item.patch(theme.window_border))
                .border_set(theme.border_set(false));
            frame.render_widget(block, rect);

            let inner = Rect {
                x: rect.x.saturating_add(1),
                y: rect.y.saturating_add(1),
                width: rect.width.saturating_sub(2),
                height: rect.height.saturating_sub(2),
            };
            draw_menu_items(frame.buffer_mut(), inner, items, selected_idx, theme);

            let Some(sel_item) = items.get(selected_idx) else {
                break;
            };
            if depth + 1 < self.state.stack.len() {
                items = &sel_item.submenu;
                origin_x = rect.x.saturating_add(rect.width);
                origin_y = rect.y.saturating_add(1 + selected_idx as u16);
            }
        }
    }
}

fn draw_menu_items(
    buf: &mut Buffer,
    area: Rect,
    items: &[MenuItem],
    selected: usize,
    theme: &Theme,
) {
    for (row, item) in items.iter().enumerate() {
        if row as u16 >= area.height {
            break;
        }
        let y = area.y + row as u16;
        let is_selected = row == selected;
        let mut style = if is_selected {
            theme.menu_item_selected
        } else {
            theme.menu_item
        };
        if !item.enabled.get() {
            style = style.patch(theme.widget.disabled);
        }
        fill_line(buf, area.x, y, area.width, style);

        let label = item.label.get();
        draw_text(buf, area.x, y, &label, style);

        if let Some(sc) = item.shortcut.get() {
            let sc_w = UnicodeWidthStr::width(sc.as_str()) as u16;
            if sc_w + 1 < area.width {
                let x = area.x + area.width - sc_w - 1;
                draw_text(buf, x, y, &sc, style);
            }
        }

        if !item.submenu.is_empty() && area.width >= 2 {
            let x = area.x + area.width - 2;
            draw_text(buf, x, y, "▶", style);
        }
    }
}

fn fill_line(buf: &mut Buffer, x: u16, y: u16, width: u16, style: Style) {
    for dx in 0..width {
        if let Some(cell) = buf.cell_mut((x + dx, y)) {
            cell.set_style(style);
            cell.set_symbol(" ");
        }
    }
}

fn draw_text(buf: &mut Buffer, x: u16, y: u16, text: &str, style: Style) {
    let mut cx = x;
    let buf_right = buf.area.x.saturating_add(buf.area.width);
    for g in text.graphemes(true) {
        let w = (UnicodeWidthStr::width(g) as u16).max(1);
        if cx >= buf_right {
            break;
        }
        if cx.saturating_add(w) > buf_right {
            break;
        }

        let Some(cell) = buf.cell_mut((cx, y)) else {
            break;
        };
        cell.set_style(style);
        cell.set_symbol(g);

        // Keep ratatui's Buffer well-formed: wide graphemes must be followed by blank cells.
        for dx in 1..w {
            if let Some(trailing) = buf.cell_mut((cx.saturating_add(dx), y)) {
                trailing.reset();
            }
        }

        cx = cx.saturating_add(w);
    }
}
