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
use super::layout::{display_label, display_label_width, dropdown_size, menu_title_x};
use super::model::{MenuBar, MenuItem};

impl MenuBar {
    pub fn draw(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }

        fill_line(
            frame.buffer_mut(),
            area.x,
            area.y,
            area.width,
            theme.menu_bar,
        );

        let mut x = area.x;
        for (idx, menu) in self.menus.iter().enumerate() {
            let is_active = self.state.active && idx == self.state.menu_index;
            let style = if is_active {
                theme.menu_bar_active
            } else {
                theme.menu_bar
            };
            let title = menu.title.get();
            let title_width = display_label_width(&title) as u16;
            let w = title_width.saturating_add(2);
            fill_line(frame.buffer_mut(), x, area.y, w, style);
            draw_mnemonic_text(
                frame.buffer_mut(),
                x.saturating_add(1),
                area.y,
                &title,
                title_width,
                style,
                mnemonic_style(style, theme),
            );
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
                .border_style(
                    theme.menu_item.patch(
                        theme
                            .named_style("menu-border")
                            .unwrap_or(theme.window_border),
                    ),
                )
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
        let accelerator = item.accelerator_text();
        let accelerator_width = accelerator
            .as_ref()
            .map(|accelerator| UnicodeWidthStr::width(accelerator.as_str()) as u16)
            .unwrap_or(0);
        let accelerator_reserved = if accelerator_width > 0 {
            accelerator_width.saturating_add(2)
        } else {
            0
        };
        let arrow_reserved = if item.submenu.is_empty() { 0 } else { 2 };
        let reserved = accelerator_reserved.saturating_add(arrow_reserved);
        let label_width = area.width.saturating_sub(reserved);
        let mnemonic_style = if item.enabled.get() {
            mnemonic_style(style, theme)
        } else {
            style
        };
        draw_mnemonic_text(buf, area.x, y, &label, label_width, style, mnemonic_style);

        if let Some(accelerator) = accelerator
            && accelerator_width < area.width
        {
            let accelerator_right = area
                .x
                .saturating_add(area.width)
                .saturating_sub(arrow_reserved);
            let x = accelerator_right.saturating_sub(accelerator_width);
            draw_text_clipped(
                buf,
                x,
                y,
                &accelerator,
                accelerator_width,
                shortcut_style(style, theme),
            );
        }

        if !item.submenu.is_empty() && area.width >= 1 {
            let x = area.x + area.width - 1;
            draw_text_clipped(buf, x, y, "▶", 1, style);
        }
    }
}

fn mnemonic_style(style: Style, theme: &Theme) -> Style {
    style.patch(
        theme
            .named_style("menu-mnemonic")
            .unwrap_or(theme.status_bar_key),
    )
}

fn shortcut_style(style: Style, theme: &Theme) -> Style {
    style.patch(theme.named_style("menu-item-shortcut").unwrap_or(style))
}

fn fill_line(buf: &mut Buffer, x: u16, y: u16, width: u16, style: Style) {
    for dx in 0..width {
        if let Some(cell) = buf.cell_mut((x + dx, y)) {
            cell.set_style(style);
            cell.set_symbol(" ");
        }
    }
}

fn draw_text_clipped(buf: &mut Buffer, x: u16, y: u16, text: &str, max_width: u16, style: Style) {
    let mut cx = x;
    let mut drawn_width: u16 = 0;
    let buf_right = buf.area.x.saturating_add(buf.area.width);
    for g in text.graphemes(true) {
        let w = (UnicodeWidthStr::width(g) as u16).max(1);
        if drawn_width.saturating_add(w) > max_width {
            break;
        }
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

        drawn_width = drawn_width.saturating_add(w);
        cx = cx.saturating_add(w);
    }
}

fn draw_mnemonic_text(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    label: &str,
    max_width: u16,
    style: Style,
    mnemonic_style: Style,
) {
    let display = display_label(label);
    let mut cx = x;
    let mut drawn_width: u16 = 0;
    let buf_right = buf.area.x.saturating_add(buf.area.width);

    for (byte_offset, grapheme) in display.text.grapheme_indices(true) {
        let w = (UnicodeWidthStr::width(grapheme) as u16).max(1);
        if drawn_width.saturating_add(w) > max_width {
            break;
        }
        if cx >= buf_right || cx.saturating_add(w) > buf_right {
            break;
        }

        let cell_style = if display.mnemonic_byte == Some(byte_offset) {
            mnemonic_style
        } else {
            style
        };
        let Some(cell) = buf.cell_mut((cx, y)) else {
            break;
        };
        cell.set_style(cell_style);
        cell.set_symbol(grapheme);

        for dx in 1..w {
            if let Some(trailing) = buf.cell_mut((cx.saturating_add(dx), y)) {
                trailing.reset();
            }
        }

        drawn_width = drawn_width.saturating_add(w);
        cx = cx.saturating_add(w);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::{Color, Modifier, Style};

    use super::super::model::MenuSpec;
    use super::*;

    fn screen_contents(terminal: &Terminal<TestBackend>, width: u16, height: u16) -> String {
        let buf = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..height {
            for x in 0..width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn draw_strips_mnemonic_markers_and_shows_accelerator() {
        let theme = Theme::dark();
        let mut menu = MenuBar::new(vec![MenuSpec::new(
            "&File",
            vec![MenuItem::action("&Open", || {}).accelerator("Ctrl+O")],
        )]);
        menu.activate();

        let mut terminal = Terminal::new(TestBackend::new(32, 5)).expect("terminal");
        terminal
            .draw(|frame| menu.draw(frame, Rect::new(0, 0, 32, 1), &theme))
            .expect("draw");

        let screen = screen_contents(&terminal, 32, 5);
        assert!(screen.contains("File"), "screen was:\n{screen}");
        assert!(screen.contains("Open"), "screen was:\n{screen}");
        assert!(screen.contains("Ctrl+O"), "screen was:\n{screen}");
        assert!(!screen.contains("&File"), "screen was:\n{screen}");
        assert!(!screen.contains("&Open"), "screen was:\n{screen}");
    }

    #[test]
    fn draw_unicode_mnemonic_uses_display_columns_for_accelerator_layout() {
        let theme = Theme::dark();
        let mut menu = MenuBar::new(vec![MenuSpec::new(
            "_文件",
            vec![MenuItem::action("_打开", || {}).accelerator("Ctrl+O")],
        )]);
        menu.activate();

        let mut terminal = Terminal::new(TestBackend::new(32, 5)).expect("terminal");
        terminal
            .draw(|frame| menu.draw(frame, Rect::new(0, 0, 32, 1), &theme))
            .expect("draw");

        let buf = terminal.backend().buffer();
        assert_eq!(buf[(1, 0)].symbol(), "文");
        assert_eq!(buf[(3, 0)].symbol(), "件");
        assert_eq!(buf[(1, 2)].symbol(), "打");
        assert_eq!(buf[(3, 2)].symbol(), "开");
        assert_eq!(buf[(7, 2)].symbol(), "C");
    }

    #[test]
    fn draw_mnemonic_letters_use_menu_mnemonic_accent() {
        let theme = Theme::dark();
        let mut menu = MenuBar::new(vec![MenuSpec::new(
            "&File",
            vec![MenuItem::action("&Open", || {})],
        )]);
        menu.activate();

        let mut terminal = Terminal::new(TestBackend::new(24, 5)).expect("terminal");
        terminal
            .draw(|frame| menu.draw(frame, Rect::new(0, 0, 24, 1), &theme))
            .expect("draw");

        let buf = terminal.backend().buffer();
        let title_mnemonic = buf[(1, 0)].style();
        assert_eq!(title_mnemonic.fg, Some(Color::Red));
        assert_eq!(title_mnemonic.bg, theme.menu_bar_active.bg);
        assert!(title_mnemonic.has_modifier(Modifier::UNDERLINED));

        let title_rest = buf[(2, 0)].style();
        assert_eq!(title_rest.fg, theme.menu_bar_active.fg);
        assert_eq!(title_rest.bg, theme.menu_bar_active.bg);
        assert!(!title_rest.has_modifier(Modifier::UNDERLINED));

        let item_mnemonic = buf[(1, 2)].style();
        assert_eq!(item_mnemonic.fg, Some(Color::Red));
        assert_eq!(item_mnemonic.bg, theme.menu_item_selected.bg);
        assert!(item_mnemonic.has_modifier(Modifier::UNDERLINED));

        let item_rest = buf[(2, 2)].style();
        assert_eq!(item_rest.fg, theme.menu_item_selected.fg);
        assert_eq!(item_rest.bg, theme.menu_item_selected.bg);
        assert!(!item_rest.has_modifier(Modifier::UNDERLINED));
    }

    #[test]
    fn draw_fills_entire_menu_bar_row_before_titles() {
        let theme = Theme::dark();
        let menu = MenuBar::new(vec![MenuSpec::new("&File", Vec::new())]);

        let mut terminal = Terminal::new(TestBackend::new(32, 3)).expect("terminal");
        terminal
            .draw(|frame| menu.draw(frame, Rect::new(0, 0, 32, 1), &theme))
            .expect("draw");

        let buf = terminal.backend().buffer();
        assert_eq!(buf[(31, 0)].symbol(), " ");
        let style = buf[(31, 0)].style();
        assert_eq!(style.fg, theme.menu_bar.fg);
        assert_eq!(style.bg, theme.menu_bar.bg);
    }

    #[test]
    fn draw_active_top_level_title_uses_active_style_without_title_shadow() {
        let mut theme = Theme::dark();
        theme.window_shadow = Style::default().bg(Color::Magenta);
        let mut menu = MenuBar::new(vec![
            MenuSpec::new("&File", Vec::new()),
            MenuSpec::new("&Edit", Vec::new()),
        ]);
        menu.state.active = true;
        menu.state.menu_index = 0;
        menu.state.stack.clear();

        let mut terminal = Terminal::new(TestBackend::new(24, 3)).expect("terminal");
        terminal
            .draw(|frame| menu.draw(frame, Rect::new(0, 0, 24, 1), &theme))
            .expect("draw");

        let buf = terminal.backend().buffer();
        assert_eq!(buf[(0, 0)].style().bg, theme.menu_bar_active.bg);
        assert_eq!(buf[(2, 0)].symbol(), "i");
        assert_eq!(buf[(2, 0)].style().bg, theme.menu_bar_active.bg);
        assert_eq!(buf[(5, 0)].style().bg, theme.menu_bar_active.bg);
        assert_eq!(buf[(6, 0)].style().bg, theme.menu_bar.bg);
        for x in 1..6 {
            assert_ne!(buf[(x, 1)].style().bg, theme.window_shadow.bg);
        }
    }
}
