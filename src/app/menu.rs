use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Clear};
use ratatui::Frame;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;

#[derive(Clone, Debug)]
pub struct MenuItem {
    pub label: String,
    pub shortcut: Option<String>,
    pub command: Option<String>,
    pub enabled: bool,
    pub submenu: Vec<MenuItem>,
}

impl MenuItem {
    pub fn command(label: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            shortcut: None,
            command: Some(command.into()),
            enabled: true,
            submenu: Vec::new(),
        }
    }

    pub fn submenu(label: impl Into<String>, submenu: Vec<MenuItem>) -> Self {
        Self {
            label: label.into(),
            shortcut: None,
            command: None,
            enabled: true,
            submenu,
        }
    }

    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
}

#[derive(Clone, Debug)]
pub struct MenuSpec {
    pub title: String,
    pub items: Vec<MenuItem>,
}

impl MenuSpec {
    pub fn new(title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            title: title.into(),
            items,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct MenuState {
    active: bool,
    menu_index: usize,
    stack: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    None,
    Command(String),
    Closed,
}

#[derive(Clone, Debug, Default)]
pub struct MenuBar {
    menus: Vec<MenuSpec>,
    state: MenuState,
}

impl MenuBar {
    pub fn new(menus: Vec<MenuSpec>) -> Self {
        Self {
            menus,
            state: MenuState::default(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.state.active
    }

    pub fn activate(&mut self) {
        self.state.active = true;
        self.state.menu_index = self.state.menu_index.min(self.menus.len().saturating_sub(1));
        self.state.stack = vec![0];
    }

    pub fn deactivate(&mut self) {
        self.state.active = false;
        self.state.stack.clear();
    }

    pub fn handle_event(&mut self, event: &Event) -> MenuAction {
        if !self.state.active {
            return MenuAction::None;
        }

        let Event::Key(KeyEvent { code, modifiers, .. }) = event else {
            return MenuAction::None;
        };

        match code {
            KeyCode::Esc => {
                self.deactivate();
                MenuAction::Closed
            }
            KeyCode::Left => {
                if self.state.stack.len() > 1 {
                    self.state.stack.pop();
                } else {
                    self.prev_menu();
                }
                MenuAction::None
            }
            KeyCode::Right => {
                if self.open_selected_submenu() {
                    MenuAction::None
                } else {
                    self.next_menu();
                    MenuAction::None
                }
            }
            KeyCode::Up => {
                self.move_selection(-1);
                MenuAction::None
            }
            KeyCode::Down => {
                self.move_selection(1);
                MenuAction::None
            }
            KeyCode::Enter => {
                if self.open_selected_submenu() {
                    return MenuAction::None;
                }
                if let Some(cmd) = self.selected_command() {
                    self.deactivate();
                    return MenuAction::Command(cmd);
                }
                MenuAction::None
            }
            KeyCode::Char(' ') if modifiers.contains(KeyModifiers::CONTROL) => {
                // Turbo Vision-style: Ctrl+Space toggles menu.
                self.deactivate();
                MenuAction::Closed
            }
            _ => MenuAction::None,
        }
    }

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
            let label = format!(" {} ", menu.title);
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
            frame.render_widget(Clear, rect);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(theme.window_border);
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

    fn next_menu(&mut self) {
        if self.menus.is_empty() {
            return;
        }
        self.state.menu_index = (self.state.menu_index + 1) % self.menus.len();
        self.state.stack = vec![0];
    }

    fn prev_menu(&mut self) {
        if self.menus.is_empty() {
            return;
        }
        if self.state.menu_index == 0 {
            self.state.menu_index = self.menus.len() - 1;
        } else {
            self.state.menu_index -= 1;
        }
        self.state.stack = vec![0];
    }

    fn move_selection(&mut self, delta: i32) {
        let Some(items) = self.selected_items() else {
            return;
        };
        if items.is_empty() {
            return;
        }
        let depth = self.state.stack.len().saturating_sub(1);
        let cur = self.state.stack[depth] as i32;
        let mut next = cur + delta;
        if next < 0 {
            next = items.len() as i32 - 1;
        }
        if next as usize >= items.len() {
            next = 0;
        }
        self.state.stack[depth] = next as usize;
    }

    fn open_selected_submenu(&mut self) -> bool {
        let Some(item) = self.selected_item() else {
            return false;
        };
        if item.submenu.is_empty() {
            return false;
        }
        self.state.stack.push(0);
        true
    }

    fn selected_items(&self) -> Option<&[MenuItem]> {
        let menu = self.menus.get(self.state.menu_index)?;
        let mut items: &[MenuItem] = &menu.items;
        for &idx in self.state.stack.iter().take(self.state.stack.len().saturating_sub(1)) {
            let item = items.get(idx)?;
            items = &item.submenu;
        }
        Some(items)
    }

    fn selected_item(&self) -> Option<&MenuItem> {
        let items = self.selected_items()?;
        let idx = *self.state.stack.last().unwrap_or(&0);
        items.get(idx)
    }

    fn selected_command(&self) -> Option<String> {
        let item = self.selected_item()?;
        if !item.enabled {
            return None;
        }
        item.command.clone()
    }
}

fn dropdown_size(items: &[MenuItem]) -> (u16, u16) {
    let mut w: usize = 8;
    for item in items {
        let mut row_w = UnicodeWidthStr::width(item.label.as_str());
        if let Some(sc) = &item.shortcut {
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

fn menu_title_x(menus: &[MenuSpec], start_x: u16, menu_index: usize) -> u16 {
    let mut x = start_x;
    for (idx, menu) in menus.iter().enumerate() {
        if idx == menu_index {
            return x;
        }
        let label = format!(" {} ", menu.title);
        x = x.saturating_add(UnicodeWidthStr::width(label.as_str()) as u16 + 1);
    }
    x
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
        let style = if is_selected {
            theme.menu_item_selected
        } else {
            theme.menu_item
        };
        fill_line(buf, area.x, y, area.width, style);

        let label = &item.label;
        draw_text(buf, area.x, y, label, style);

        if let Some(sc) = &item.shortcut {
            let sc_w = UnicodeWidthStr::width(sc.as_str()) as u16;
            if sc_w + 1 < area.width {
                let x = area.x + area.width - sc_w - 1;
                draw_text(buf, x, y, sc, style);
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
    for g in text.graphemes(true) {
        let Some(cell) = buf.cell_mut((cx, y)) else {
            break;
        };
        cell.set_style(style);
        cell.set_symbol(g);
        let w = UnicodeWidthStr::width(g) as u16;
        cx = cx.saturating_add(w.max(1));
    }
}
