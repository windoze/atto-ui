use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Clear};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::reactive::Binding;
use crate::theme::Theme;
use crate::wm::{WindowId, WindowManager, WindowState};

use super::status::Fill;

pub type MenuCallback = std::sync::Arc<dyn Fn() + Send + Sync>;

const MINIMIZED_WINDOWS_MENU_ID: &str = "atto_ui:minimized_windows";
const MINIMIZED_WINDOW_ITEM_PREFIX: &str = "atto_ui:minimized_window:";

#[derive(Clone)]
pub struct MenuItem {
    pub tag: Option<String>,
    pub label: Binding<String>,
    pub shortcut: Binding<Option<String>>,
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
        self.shortcut.set(Some(shortcut.into()));
        self
    }

    pub fn shortcut_binding(mut self, shortcut: impl Into<Binding<Option<String>>>) -> Self {
        self.shortcut = shortcut.into();
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
struct MenuState {
    active: bool,
    menu_index: usize,
    stack: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    None,
    Closed,
    RestoreWindow(WindowId),
}

#[derive(Clone, Default)]
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

    pub fn menus(&self) -> &[MenuSpec] {
        &self.menus
    }

    pub fn menus_mut(&mut self) -> &mut [MenuSpec] {
        &mut self.menus
    }

    pub fn refresh_minimized_windows(&mut self, wm: &WindowManager) {
        let items = build_minimized_window_items(wm);
        for menu in &mut self.menus {
            refresh_minimized_windows_in_items(&mut menu.items, &items);
        }
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

            let mut hit: Option<(usize, bool, Option<MenuCallback>, bool, Option<WindowId>)> = None;
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

fn minimized_window_id(item: &MenuItem) -> Option<WindowId> {
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

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn dropdown_size(items: &[MenuItem]) -> (u16, u16) {
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

fn menu_title_x(menus: &[MenuSpec], start_x: u16, menu_index: usize) -> u16 {
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

fn draw_shadow(buf: &mut Buffer, rect: Rect, bounds: Rect, style: Style) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    let style = reset_style(style);
    let shadow_x = rect.x.saturating_add(rect.width);
    let shadow_y = rect.y.saturating_add(rect.height);

    // Vertical shadow.
    if shadow_x < bounds.x.saturating_add(bounds.width) {
        for y in rect.y.saturating_add(1)..rect.y.saturating_add(rect.height) {
            if y >= bounds.y.saturating_add(bounds.height) {
                break;
            }
            if shadow_x < bounds.x || y < bounds.y {
                continue;
            }
            if let Some(cell) = buf.cell_mut((shadow_x, y)) {
                cell.set_symbol(" ");
                cell.set_style(style);
            }
        }
    }

    // Horizontal shadow.
    if shadow_y < bounds.y.saturating_add(bounds.height) {
        for x in rect.x.saturating_add(1)..rect.x.saturating_add(rect.width) {
            if x >= bounds.x.saturating_add(bounds.width) {
                break;
            }
            if x < bounds.x || shadow_y < bounds.y {
                continue;
            }
            if let Some(cell) = buf.cell_mut((x, shadow_y)) {
                cell.set_symbol(" ");
                cell.set_style(style);
            }
        }
    }

    // Bottom-right corner.
    if shadow_x < bounds.x.saturating_add(bounds.width)
        && shadow_y < bounds.y.saturating_add(bounds.height)
        && shadow_x >= bounds.x
        && shadow_y >= bounds.y
        && let Some(cell) = buf.cell_mut((shadow_x, shadow_y))
    {
        cell.set_symbol(" ");
        cell.set_style(style);
    }
}

fn reset_style(style: Style) -> Style {
    Style::reset().patch(style)
}
