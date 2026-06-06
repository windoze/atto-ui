use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use crate::wm::WindowId;

use super::MenuCallback;
use super::layout::{contains, dropdown_size, menu_title_x};
use super::minimized::minimized_window_id;
use super::model::{MenuAction, MenuBar, MenuItem};

impl MenuBar {
    pub fn handle_event(&mut self, event: &Event) -> MenuAction {
        if !self.state.active {
            return MenuAction::None;
        }

        let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event
        else {
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
                // Turbo Vision-ish behavior:
                // - At the top-level drop-down, Right switches to the next menu.
                // - Within submenus, Right opens deeper submenus when available.
                if self.state.stack.len() > 1 && self.open_selected_submenu() {
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
                self.activate_selected_item()
            }
            KeyCode::Char(c)
                if !modifiers.contains(KeyModifiers::CONTROL)
                    && !modifiers.contains(KeyModifiers::ALT) =>
            {
                self.handle_shortcut_char(*c)
            }
            KeyCode::Char(' ') if modifiers.contains(KeyModifiers::CONTROL) => {
                // Turbo Vision-style: Ctrl+Space toggles menu.
                self.deactivate();
                MenuAction::Closed
            }
            _ => MenuAction::None,
        }
    }

    pub fn handle_mouse(&mut self, m: &MouseEvent, menu_bar_area: Rect) -> MenuAction {
        if self.menus.is_empty() {
            return MenuAction::None;
        }

        if m.kind != MouseEventKind::Down(MouseButton::Left) {
            return MenuAction::None;
        }

        if menu_bar_area.height == 0 || menu_bar_area.width == 0 {
            return MenuAction::None;
        }

        if m.row == menu_bar_area.y
            && m.column >= menu_bar_area.x
            && m.column < menu_bar_area.x.saturating_add(menu_bar_area.width)
        {
            if let Some(menu_idx) = self.hit_test_menu_title(m.column, menu_bar_area) {
                self.activate_menu(menu_idx);
                return MenuAction::None;
            }

            // Clicking the menu bar but not on a title closes an open menu.
            if self.state.active {
                self.deactivate();
                return MenuAction::Closed;
            }
            return MenuAction::None;
        }

        if !self.state.active {
            return MenuAction::None;
        }

        enum DropdownHit {
            InsideDropdown,
            Item {
                depth: usize,
                row: usize,
                has_submenu: bool,
                on_activate: Option<MenuCallback>,
                restore_id: Option<WindowId>,
                enabled: bool,
            },
        }

        let mut hit: Option<DropdownHit> = None;
        for (depth, (rect, items)) in self.dropdown_levels(menu_bar_area).into_iter().enumerate() {
            if !contains(rect, m.column, m.row) {
                continue;
            }

            let inner = Rect {
                x: rect.x.saturating_add(1),
                y: rect.y.saturating_add(1),
                width: rect.width.saturating_sub(2),
                height: rect.height.saturating_sub(2),
            };
            if inner.width == 0 || inner.height == 0 {
                hit = Some(DropdownHit::InsideDropdown);
                continue;
            }

            if m.row < inner.y
                || m.row >= inner.y.saturating_add(inner.height)
                || m.column < inner.x
                || m.column >= inner.x.saturating_add(inner.width)
            {
                hit = Some(DropdownHit::InsideDropdown);
                continue;
            }

            let row = m.row.saturating_sub(inner.y) as usize;
            if row >= items.len() {
                hit = Some(DropdownHit::InsideDropdown);
                continue;
            }

            let (has_submenu, enabled, on_activate, restore_id) = {
                let item = &items[row];
                let has_submenu = !item.submenu.is_empty();
                let enabled = item.enabled.get();
                let on_activate = if enabled {
                    item.on_activate.clone()
                } else {
                    None
                };
                let restore_id = minimized_window_id(item);
                (has_submenu, enabled, on_activate, restore_id)
            };

            hit = Some(DropdownHit::Item {
                depth,
                row,
                has_submenu,
                on_activate,
                restore_id,
                enabled,
            });
        }

        match hit {
            Some(DropdownHit::InsideDropdown) => return MenuAction::None,
            Some(DropdownHit::Item {
                depth,
                row,
                has_submenu,
                on_activate,
                restore_id,
                enabled,
            }) => {
                self.state.stack.truncate(depth.saturating_add(1));
                if self.state.stack.len() < depth.saturating_add(1) {
                    self.state.stack.resize(depth.saturating_add(1), 0);
                }
                self.state.stack[depth] = row;

                if has_submenu {
                    self.state.stack.push(0);
                    return MenuAction::None;
                }

                if enabled {
                    if let Some(cb) = on_activate {
                        cb();
                        self.deactivate();
                        return MenuAction::Closed;
                    }
                    if let Some(id) = restore_id {
                        self.deactivate();
                        return MenuAction::RestoreWindow(id);
                    }
                }

                return MenuAction::None;
            }
            None => {}
        }

        // Click outside closes the open menu.
        self.deactivate();
        MenuAction::Closed
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
        for &idx in self
            .state
            .stack
            .iter()
            .take(self.state.stack.len().saturating_sub(1))
        {
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

    fn activate_selected_item(&mut self) -> MenuAction {
        let Some(item) = self.selected_item() else {
            return MenuAction::None;
        };
        if !item.enabled.get() {
            return MenuAction::None;
        }
        if !item.submenu.is_empty() {
            return MenuAction::None;
        }
        if let Some(cb) = &item.on_activate {
            cb();
            self.deactivate();
            return MenuAction::Closed;
        }
        if let Some(id) = minimized_window_id(item) {
            self.deactivate();
            return MenuAction::RestoreWindow(id);
        }
        MenuAction::None
    }

    pub fn activate_menu(&mut self, menu_index: usize) {
        self.state.active = true;
        self.state.menu_index = menu_index.min(self.menus.len().saturating_sub(1));
        self.state.stack = vec![0];
    }

    pub fn menu_index_for_shortcut(&self, c: char) -> Option<usize> {
        let target = c.to_ascii_lowercase();
        self.menus.iter().position(|menu| {
            menu.title
                .get()
                .trim_start()
                .chars()
                .next()
                .is_some_and(|first| first.to_ascii_lowercase() == target)
        })
    }

    fn handle_shortcut_char(&mut self, c: char) -> MenuAction {
        if self.menus.is_empty() {
            return MenuAction::None;
        }
        if self.state.stack.is_empty() {
            self.state.stack = vec![0];
        }

        let target = c.to_ascii_lowercase();
        let depth = self.state.stack.len().saturating_sub(1);

        let (hit_idx, has_submenu, on_activate, enabled, restore_id) = {
            let Some(items) = self.selected_items() else {
                return MenuAction::None;
            };

            #[allow(clippy::type_complexity)]
            let mut hit: Option<(
                usize,
                bool,
                Option<MenuCallback>,
                bool,
                Option<WindowId>,
            )> = None;
            for (idx, item) in items.iter().enumerate() {
                let enabled = item.enabled.get();
                if !enabled {
                    continue;
                }
                let Some(sc) = item.shortcut.get() else {
                    continue;
                };
                if sc.chars().count() != 1 {
                    continue;
                }
                let Some(sc_char) = sc.chars().next() else {
                    continue;
                };
                if sc_char.to_ascii_lowercase() == target {
                    hit = Some((
                        idx,
                        !item.submenu.is_empty(),
                        item.on_activate.clone(),
                        enabled,
                        minimized_window_id(item),
                    ));
                    break;
                }
            }

            let Some((idx, has_submenu, on_activate, enabled, restore_id)) = hit else {
                return MenuAction::None;
            };
            (idx, has_submenu, on_activate, enabled, restore_id)
        };

        if depth < self.state.stack.len() {
            self.state.stack[depth] = hit_idx;
        } else {
            self.state.stack.push(hit_idx);
        }

        if has_submenu {
            self.state.stack.push(0);
            return MenuAction::None;
        }

        if enabled {
            if let Some(cb) = on_activate {
                cb();
                self.deactivate();
                return MenuAction::Closed;
            }
            if let Some(id) = restore_id {
                self.deactivate();
                return MenuAction::RestoreWindow(id);
            }
        }

        MenuAction::None
    }

    fn dropdown_levels(&self, menu_bar_area: Rect) -> Vec<(Rect, &[MenuItem])> {
        let Some(menu) = self.menus.get(self.state.menu_index) else {
            return Vec::new();
        };

        let dropdown_y = menu_bar_area.y.saturating_add(1);
        let mut origin_x = menu_title_x(&self.menus, menu_bar_area.x, self.state.menu_index);
        let mut origin_y = dropdown_y;
        let mut items: &[MenuItem] = &menu.items;

        let mut levels = Vec::new();
        for (depth, &selected_idx) in self.state.stack.iter().enumerate() {
            let (w, h) = dropdown_size(items);
            let rect = Rect {
                x: origin_x,
                y: origin_y,
                width: w,
                height: h,
            };
            levels.push((rect, items));

            let Some(sel_item) = items.get(selected_idx) else {
                break;
            };
            if depth + 1 < self.state.stack.len() {
                items = &sel_item.submenu;
                origin_x = rect.x.saturating_add(rect.width);
                origin_y = rect.y.saturating_add(1 + selected_idx as u16);
            }
        }
        levels
    }

    fn hit_test_menu_title(&self, x: u16, menu_bar_area: Rect) -> Option<usize> {
        let mut cur_x = menu_bar_area.x;
        for (idx, menu) in self.menus.iter().enumerate() {
            let label = format!(" {} ", menu.title.get());
            let w = UnicodeWidthStr::width(label.as_str()) as u16;
            let start = cur_x;
            let end = cur_x.saturating_add(w);
            if x >= start && x < end {
                return Some(idx);
            }
            cur_x = cur_x.saturating_add(w).saturating_add(1);
        }
        None
    }
}
