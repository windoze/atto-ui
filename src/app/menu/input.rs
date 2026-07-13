use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use crate::wm::WindowId;

use super::MenuCallback;
use super::layout::{
    contains, label_mnemonic_or_first, menu_title_width, menu_title_x, menu_titles_start_x,
    next_menu_title_x,
};
use super::minimized::minimized_window_id;
use super::model::{MenuAction, MenuBar, MenuItem, WindowMenuOp};
use super::nav::item_mnemonic_or_first;
use super::window_ops::window_menu_op;

fn char_eq_ignore_case(a: char, b: char) -> bool {
    a == b || a.to_lowercase().to_string() == b.to_lowercase().to_string()
}

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
                window_op: Option<WindowMenuOp>,
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

            let (has_submenu, enabled, on_activate, restore_id, window_op) = {
                let item = &items[row];
                let has_submenu = !item.submenu.is_empty();
                let enabled = item.enabled.get();
                let on_activate = if enabled {
                    item.on_activate.clone()
                } else {
                    None
                };
                let restore_id = minimized_window_id(item);
                let window_op = window_menu_op(item);
                (has_submenu, enabled, on_activate, restore_id, window_op)
            };

            hit = Some(DropdownHit::Item {
                depth,
                row,
                has_submenu,
                on_activate,
                restore_id,
                window_op,
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
                window_op,
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
                    if let Some(op) = window_op {
                        self.deactivate();
                        return MenuAction::WindowOp(op);
                    }
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
        if self.state.menu_index >= self.menus.len() {
            return;
        }
        // Disjoint field borrows: `menus` (shared) and `state.stack` (mut).
        let root = &self.menus[self.state.menu_index].items;
        super::nav::move_in_stack(root, &mut self.state.stack, delta);
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
        super::nav::items_at(&menu.items, &self.state.stack)
    }

    fn selected_item(&self) -> Option<&MenuItem> {
        let menu = self.menus.get(self.state.menu_index)?;
        super::nav::item_at(&menu.items, &self.state.stack)
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
        if let Some(op) = window_menu_op(item) {
            self.deactivate();
            return MenuAction::WindowOp(op);
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
        self.menus.iter().position(|menu| {
            label_mnemonic_or_first(&menu.title.get())
                .is_some_and(|mnemonic| char_eq_ignore_case(mnemonic, c))
        })
    }

    fn handle_shortcut_char(&mut self, c: char) -> MenuAction {
        if self.menus.is_empty() {
            return MenuAction::None;
        }
        if self.state.stack.is_empty() {
            self.state.stack = vec![0];
        }

        let depth = self.state.stack.len().saturating_sub(1);

        let (hit_idx, has_submenu, on_activate, enabled, restore_id, window_op) = {
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
                Option<WindowMenuOp>,
            )> = None;
            for (idx, item) in items.iter().enumerate() {
                let enabled = item.enabled.get();
                if !enabled {
                    continue;
                }
                let Some(mnemonic) = item_mnemonic_or_first(item) else {
                    continue;
                };
                if char_eq_ignore_case(mnemonic, c) {
                    hit = Some((
                        idx,
                        !item.submenu.is_empty(),
                        item.on_activate.clone(),
                        enabled,
                        minimized_window_id(item),
                        window_menu_op(item),
                    ));
                    break;
                }
            }

            let Some((idx, has_submenu, on_activate, enabled, restore_id, window_op)) = hit else {
                return MenuAction::None;
            };
            (
                idx,
                has_submenu,
                on_activate,
                enabled,
                restore_id,
                window_op,
            )
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
            if let Some(op) = window_op {
                self.deactivate();
                return MenuAction::WindowOp(op);
            }
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
        let origin = (
            menu_title_x(
                &self.menus,
                menu_titles_start_x(menu_bar_area),
                self.state.menu_index,
            ),
            menu_bar_area.y.saturating_add(1),
        );
        super::nav::levels_from_origin(&menu.items, &self.state.stack, origin)
    }

    fn hit_test_menu_title(&self, x: u16, menu_bar_area: Rect) -> Option<usize> {
        let mut cur_x = menu_titles_start_x(menu_bar_area);
        for (idx, menu) in self.menus.iter().enumerate() {
            let title = menu.title.get();
            let w = menu_title_width(&title);
            let start = cur_x;
            let end = cur_x.saturating_add(w);
            if x >= start && x < end {
                return Some(idx);
            }
            cur_x = next_menu_title_x(cur_x, &title);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::super::model::MenuSpec;
    use super::*;

    #[test]
    fn top_level_shortcut_uses_marker_before_first_char_fallback() {
        let menu = MenuBar::new(vec![
            MenuSpec::new("&File", Vec::new()),
            MenuSpec::new("Tools", Vec::new()),
        ]);

        assert_eq!(menu.menu_index_for_shortcut('f'), Some(0));
        assert_eq!(menu.menu_index_for_shortcut('t'), Some(1));
    }

    #[test]
    fn item_mnemonic_activates_without_using_accelerator_text() {
        let activated = Arc::new(AtomicBool::new(false));
        let menu_activated = activated.clone();
        let mut menu = MenuBar::new(vec![MenuSpec::new(
            "&File",
            vec![
                MenuItem::action("Save", move || {
                    menu_activated.store(true, Ordering::SeqCst);
                })
                .accelerator("Ctrl+S")
                .mnemonic('S'),
            ],
        )]);
        menu.activate();

        assert_eq!(menu.handle_shortcut_char('s'), MenuAction::Closed);
        assert!(activated.load(Ordering::SeqCst));
    }

    #[test]
    fn shortcut_builder_single_character_still_sets_mnemonic() {
        let activated = Arc::new(AtomicBool::new(false));
        let menu_activated = activated.clone();
        let mut menu = MenuBar::new(vec![MenuSpec::new(
            "&File",
            vec![
                MenuItem::action("Quit", move || {
                    menu_activated.store(true, Ordering::SeqCst);
                })
                .shortcut("q"),
            ],
        )]);
        menu.activate();

        assert_eq!(menu.handle_shortcut_char('Q'), MenuAction::Closed);
        assert!(activated.load(Ordering::SeqCst));
    }

    #[test]
    fn shortcut_binding_single_character_still_acts_as_legacy_mnemonic() {
        let activated = Arc::new(AtomicBool::new(false));
        let menu_activated = activated.clone();
        let shortcut = crate::reactive::Binding::from(Some("x".to_string()));
        let mut menu = MenuBar::new(vec![MenuSpec::new(
            "&File",
            vec![
                MenuItem::action("Quit", move || {
                    menu_activated.store(true, Ordering::SeqCst);
                })
                .shortcut_binding(shortcut),
            ],
        )]);
        menu.activate();

        assert_eq!(menu.handle_shortcut_char('x'), MenuAction::Closed);
        assert!(activated.load(Ordering::SeqCst));
    }

    #[test]
    fn window_op_item_activates_to_window_op_action() {
        use super::super::model::WindowMenuOp;

        let mut menu = MenuBar::new(vec![MenuSpec::new(
            "&Window",
            vec![MenuItem::window_op(WindowMenuOp::Cascade, "Cascade")],
        )]);
        menu.activate();

        // Keyboard Enter on the predefined item yields the native op, not a callback.
        assert_eq!(
            menu.handle_event(&Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE
            ))),
            MenuAction::WindowOp(WindowMenuOp::Cascade)
        );
        assert!(!menu.state.active);
    }

    #[test]
    fn window_op_item_activates_via_mnemonic_shortcut() {
        use super::super::model::WindowMenuOp;

        let mut menu = MenuBar::new(vec![MenuSpec::new(
            "&Window",
            vec![MenuItem::window_op(WindowMenuOp::Tile, "Tile").shortcut("t")],
        )]);
        menu.activate();

        assert_eq!(
            menu.handle_shortcut_char('t'),
            MenuAction::WindowOp(WindowMenuOp::Tile)
        );
    }

    #[test]
    fn mouse_title_hit_testing_accounts_for_system_menu_icon() {
        let mut menu = MenuBar::new(vec![
            MenuSpec::new("&File", Vec::new()),
            MenuSpec::new("&Edit", Vec::new()),
        ]);
        let area = Rect::new(0, 0, 32, 1);
        let click = |column| MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row: 0,
            modifiers: KeyModifiers::empty(),
        };

        assert_eq!(menu.handle_mouse(&click(0), area), MenuAction::None);
        assert!(!menu.state.active);

        assert_eq!(menu.handle_mouse(&click(2), area), MenuAction::None);
        assert!(menu.state.active);
        assert_eq!(menu.state.menu_index, 0);

        assert_eq!(menu.handle_mouse(&click(7), area), MenuAction::None);
        assert!(menu.state.active);
        assert_eq!(menu.state.menu_index, 1);
    }
}
