use crate::wm::{WindowId, WindowManager, WindowState};

use super::model::{MenuBar, MenuItem};

pub(super) const MINIMIZED_WINDOWS_MENU_ID: &str = "atto_ui:minimized_windows";
const MINIMIZED_WINDOW_ITEM_PREFIX: &str = "atto_ui:minimized_window:";

impl MenuBar {
    pub fn refresh_minimized_windows(&mut self, wm: &WindowManager) {
        let items = build_minimized_window_items(wm);
        for menu in &mut self.menus {
            refresh_minimized_windows_in_items(&mut menu.items, &items);
        }
    }
}

pub(super) fn minimized_window_id(item: &MenuItem) -> Option<WindowId> {
    let id = item.tag.as_deref()?;
    let suffix = id.strip_prefix(MINIMIZED_WINDOW_ITEM_PREFIX)?;
    let parsed = suffix.parse::<u64>().ok()?;
    Some(WindowId(parsed))
}

fn minimized_window_menu_item(id: WindowId, label: String) -> MenuItem {
    let mut item = MenuItem::submenu(label, Vec::new());
    item.tag = Some(format!("{MINIMIZED_WINDOW_ITEM_PREFIX}{}", id.0));
    item
}

fn build_minimized_window_items(wm: &WindowManager) -> Vec<MenuItem> {
    let mut items = Vec::new();
    for window in wm.windows().iter().rev() {
        if window.state.get() != WindowState::Minimized {
            continue;
        }
        let mut label = window.title.get();
        if label.trim().is_empty() {
            label = format!("Window {}", window.id.0);
        }
        items.push(minimized_window_menu_item(window.id, label));
    }
    if items.is_empty() {
        items.push(MenuItem::submenu("No minimized windows", Vec::new()).enabled(false));
    }
    items
}

fn refresh_minimized_windows_in_items(items: &mut [MenuItem], minimized_items: &[MenuItem]) {
    for item in items {
        if item.tag.as_deref() == Some(MINIMIZED_WINDOWS_MENU_ID) {
            item.submenu = minimized_items.to_vec();
            continue;
        }
        if !item.submenu.is_empty() {
            refresh_minimized_windows_in_items(&mut item.submenu, minimized_items);
        }
    }
}
