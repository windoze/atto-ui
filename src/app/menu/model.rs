use crate::reactive::Binding;
use crate::wm::WindowId;

use super::MenuCallback;
use super::minimized::MINIMIZED_WINDOWS_MENU_ID;

#[derive(Clone)]
pub struct MenuItem {
    pub tag: Option<String>,
    pub label: Binding<String>,
    pub shortcut: Binding<Option<String>>,
    pub accelerator: Binding<Option<String>>,
    pub mnemonic: Binding<Option<char>>,
    pub enabled: Binding<bool>,
    pub on_activate: Option<MenuCallback>,
    pub submenu: Vec<MenuItem>,
}

impl MenuItem {
    pub fn action<F>(label: impl Into<Binding<String>>, on_activate: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        Self {
            tag: None,
            label: label.into(),
            shortcut: None.into(),
            accelerator: None.into(),
            mnemonic: None.into(),
            enabled: true.into(),
            on_activate: Some(std::sync::Arc::new(on_activate)),
            submenu: Vec::new(),
        }
    }

    pub fn submenu(label: impl Into<Binding<String>>, submenu: Vec<MenuItem>) -> Self {
        Self {
            tag: None,
            label: label.into(),
            shortcut: None.into(),
            accelerator: None.into(),
            mnemonic: None.into(),
            enabled: true.into(),
            on_activate: None,
            submenu,
        }
    }

    pub fn minimized_windows(label: impl Into<Binding<String>>) -> Self {
        let mut item = Self::submenu(label, Vec::new());
        item.tag = Some(MINIMIZED_WINDOWS_MENU_ID.to_string());
        item
    }

    pub fn label(mut self, label: impl Into<Binding<String>>) -> Self {
        self.label = label.into();
        self
    }

    pub fn shortcut(self, shortcut: impl Into<String>) -> Self {
        let shortcut = shortcut.into();
        self.shortcut.set(Some(shortcut.clone()));
        self.accelerator.set(Some(shortcut.clone()));
        if shortcut.chars().count() == 1 {
            self.mnemonic.set(shortcut.chars().next());
        }
        self
    }

    pub fn shortcut_binding(mut self, shortcut: impl Into<Binding<Option<String>>>) -> Self {
        let shortcut = shortcut.into();
        self.shortcut = shortcut.clone();
        self.accelerator = shortcut;
        self
    }

    pub fn accelerator(self, accelerator: impl Into<String>) -> Self {
        self.accelerator.set(Some(accelerator.into()));
        self
    }

    pub fn accelerator_binding(mut self, accelerator: impl Into<Binding<Option<String>>>) -> Self {
        self.accelerator = accelerator.into();
        self
    }

    pub fn mnemonic(self, mnemonic: char) -> Self {
        self.mnemonic.set(Some(mnemonic));
        self
    }

    pub fn mnemonic_binding(mut self, mnemonic: impl Into<Binding<Option<char>>>) -> Self {
        self.mnemonic = mnemonic.into();
        self
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    pub fn with_tag(mut self, id: impl Into<String>) -> Self {
        self.tag = Some(id.into());
        self
    }

    pub(super) fn accelerator_text(&self) -> Option<String> {
        self.accelerator.get().or_else(|| self.shortcut.get())
    }
}

#[derive(Clone)]
pub struct MenuSpec {
    pub tag: Option<String>,
    pub title: Binding<String>,
    pub items: Vec<MenuItem>,
}

impl MenuSpec {
    pub fn new(title: impl Into<Binding<String>>, items: Vec<MenuItem>) -> Self {
        Self {
            tag: None,
            title: title.into(),
            items,
        }
    }

    pub fn title(mut self, title: impl Into<Binding<String>>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_tag(mut self, id: impl Into<String>) -> Self {
        self.tag = Some(id.into());
        self
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct MenuState {
    pub(super) active: bool,
    pub(super) menu_index: usize,
    pub(super) stack: Vec<usize>,
}

/// Standard window-management operations bound to predefined menu item ids.
/// A menu item carrying one of the `atto_ui:window_*` ids triggers the matching
/// operation natively when activated; user-defined callbacks are not invoked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowMenuOp {
    Cascade,
    Tile,
    MinimizeFocused,
    MaximizeFocused,
    RestoreFocused,
    CloseFocused,
    FocusNext,
    FocusPrevious,
    MinimizeAll,
    RestoreAll,
    CloseAll,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    None,
    Closed,
    RestoreWindow(WindowId),
    WindowOp(WindowMenuOp),
}

#[derive(Clone, Default)]
pub struct MenuBar {
    pub(super) menus: Vec<MenuSpec>,
    pub(super) state: MenuState,
}

impl MenuBar {
    pub fn new(menus: Vec<MenuSpec>) -> Self {
        Self {
            menus,
            state: MenuState::default(),
        }
    }

    pub fn menus(&self) -> &[MenuSpec] {
        &self.menus
    }

    pub fn menus_mut(&mut self) -> &mut [MenuSpec] {
        &mut self.menus
    }

    pub fn is_active(&self) -> bool {
        self.state.active
    }

    pub fn activate(&mut self) {
        self.state.active = true;
        self.state.menu_index = self
            .state
            .menu_index
            .min(self.menus.len().saturating_sub(1));
        self.state.stack = vec![0];
    }

    pub fn deactivate(&mut self) {
        self.state.active = false;
        self.state.stack.clear();
    }
}
