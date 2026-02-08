use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::app::{Desktop, DesktopLayout, MenuItem, MenuSpec};
use crate::{ComponentCommand, ComponentError, ComponentTarget, ComponentValueCodec};
use crate::composable::{Component, EventResult};
use crate::wm::{Window, WindowId};
use atto_ui_runtime::ComponentValue;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Desktop,
    MenuBar,
    Menu,
    MenuItem,
    StatusBar,
    Window,
    Component,
}

#[derive(Clone, Debug)]
pub struct InspectNode {
    pub kind: NodeKind,
    pub id: Option<String>,
    pub name: String,
    pub type_id: String,
    pub bounds: Option<Rect>,
    pub properties: Vec<String>,
    pub window_id: Option<WindowId>,
    pub children: Vec<InspectNode>,
}

impl InspectNode {
    pub fn find_by_id(&self, id: &str) -> Option<&InspectNode> {
        if self.id.as_deref() == Some(id) {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_by_id(id) {
                return Some(found);
            }
        }
        None
    }
}

#[derive(Clone, Debug)]
pub struct InspectSnapshot {
    pub buffer: Buffer,
    pub tree: InspectNode,
}

impl InspectSnapshot {
    pub fn contents(&self) -> String {
        buffer_to_string(&self.buffer)
    }

    pub fn component_buffer(&self, id: &str) -> Option<Buffer> {
        let node = self.tree.find_by_id(id)?;
        let area = node.bounds?;
        Some(crop_buffer(&self.buffer, area))
    }
}

pub struct DesktopInspector<'a> {
    desktop: &'a mut Desktop,
}

impl<'a> DesktopInspector<'a> {
    pub fn new(desktop: &'a mut Desktop) -> Self {
        Self { desktop }
    }

    pub fn tree(&mut self, screen: Rect) -> Result<InspectNode, ComponentError> {
        Ok(self.snapshot(screen)?.tree)
    }

    pub fn snapshot(&mut self, screen: Rect) -> Result<InspectSnapshot, ComponentError> {
        let backend = TestBackend::new(screen.width, screen.height);
        let mut terminal = Terminal::new(backend).map_err(ComponentError::render_failed)?;
        terminal
            .draw(|f| self.desktop.draw(f))
            .map_err(ComponentError::render_failed)?;
        let buffer = terminal.backend().buffer().clone();
        let tree = build_desktop_tree(self.desktop, screen);
        Ok(InspectSnapshot { buffer, tree })
    }

    pub fn get_property(
        &mut self,
        id: &str,
        name: &str,
    ) -> Result<ComponentValue, ComponentError> {
        if let Some(value) = menu_get_property(&self.desktop.menu, id, name) {
            return Ok(value);
        }
        if let Some(value) = window_get_property(&self.desktop.wm, id, name) {
            return Ok(value);
        }
        if let Some(value) = component_get_property(&self.desktop.wm, id, name) {
            return Ok(value);
        }
        Err(ComponentError::not_found(id))
    }

    pub fn set_property(
        &mut self,
        id: &str,
        name: &str,
        value: ComponentValue,
    ) -> Result<(), ComponentError> {
        if menu_set_property(&mut self.desktop.menu, id, name, value.clone())? {
            return Ok(());
        }
        if window_set_property(&mut self.desktop.wm, id, name, value.clone())? {
            return Ok(());
        }
        if component_set_property(&mut self.desktop.wm, id, name, value)? {
            return Ok(());
        }
        Err(ComponentError::not_found(id))
    }

    pub fn action(
        &mut self,
        screen: Rect,
        id: &str,
        action: ComponentCommand,
    ) -> Result<EventResult, ComponentError> {
        self.action_target(screen, ComponentTarget::Id(id.to_string()), action)
    }

    pub fn action_target(
        &mut self,
        screen: Rect,
        target: ComponentTarget,
        action: ComponentCommand,
    ) -> Result<EventResult, ComponentError> {
        match target {
            ComponentTarget::Id(id) => self.action_by_id(screen, &id, action),
            ComponentTarget::Focused => self.action_focused(action),
        }
    }

    fn action_by_id(
        &mut self,
        screen: Rect,
        id: &str,
        action: ComponentCommand,
    ) -> Result<EventResult, ComponentError> {
        let custom_name = match &action {
            ComponentCommand::Custom { name, .. } => Some(name.clone()),
            _ => None,
        };

        if let Some(result) = menu_action(&mut self.desktop.menu, id, &action) {
            return Ok(result);
        }
        if let Some(result) = window_action(&mut self.desktop.wm, id, &action) {
            return Ok(result);
        }
        if let Some(result) = component_action(&mut self.desktop.wm, id, &action) {
            if result.is_consumed() {
                return Ok(result);
            }
            if let Some(name) = custom_name {
                return Err(ComponentError::action_not_supported(name));
            }
        }

        if let Some(name) = custom_name {
            if menu_exists(&self.desktop.menu, id)
                || window_exists(&self.desktop.wm, id)
                || component_exists(&self.desktop.wm, id)
            {
                return Err(ComponentError::action_not_supported(name));
            }
        }

        match action {
            ComponentCommand::Click | ComponentCommand::Toggle | ComponentCommand::Submit => {
                let snapshot = self.snapshot(screen)?;
                let bounds = snapshot
                    .tree
                    .find_by_id(id)
                    .and_then(|node| node.bounds)
                    .ok_or_else(|| ComponentError::not_found(id))?;
                let (x, y) = center_point(bounds)
                    .ok_or_else(|| ComponentError::action_not_supported("empty bounds"))?;
                let event = Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: x,
                    row: y,
                    modifiers: KeyModifiers::NONE,
                });
                let result = self.desktop.handle_event(&event, screen);
                apply_desktop_action(self.desktop, &result.action);
                Ok(EventResult {
                    outcome: result.outcome,
                    action: crate::composable::ComponentAction::None,
                })
            }
            ComponentCommand::InputText(text) => {
                let snapshot = self.snapshot(screen)?;
                let bounds = snapshot
                    .tree
                    .find_by_id(id)
                    .and_then(|node| node.bounds)
                    .ok_or_else(|| ComponentError::not_found(id))?;
                if let Some((x, y)) = center_point(bounds) {
                    let click_event = Event::Mouse(MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: x,
                        row: y,
                        modifiers: KeyModifiers::NONE,
                    });
                    let click_result = self.desktop.handle_event(&click_event, screen);
                    apply_desktop_action(self.desktop, &click_result.action);
                }
                let event = Event::Paste(text);
                let result = self.desktop.handle_event(&event, screen);
                apply_desktop_action(self.desktop, &result.action);
                Ok(EventResult {
                    outcome: result.outcome,
                    action: crate::composable::ComponentAction::None,
                })
            }
            ComponentCommand::SelectIndex(_) => {
                Err(ComponentError::action_not_supported("SelectIndex"))
            }
            ComponentCommand::Custom { name, .. } => {
                Err(ComponentError::action_not_supported(name))
            }
        }
    }

    fn action_focused(
        &mut self,
        action: ComponentCommand,
    ) -> Result<EventResult, ComponentError> {
        let Some(focused) = focused_component_mut(&mut self.desktop.wm) else {
            return Err(ComponentError::not_found("focused"));
        };
        let result = focused.apply_command(action.clone());
        match action {
            ComponentCommand::Custom { name, .. } => {
                if result.is_consumed() {
                    Ok(result)
                } else {
                    Err(ComponentError::action_not_supported(name))
                }
            }
            ComponentCommand::SelectIndex(_) => {
                if result.is_consumed() {
                    Ok(result)
                } else {
                    Err(ComponentError::action_not_supported("SelectIndex"))
                }
            }
            _ => Ok(result),
        }
    }

    pub fn click(&mut self, screen: Rect, id: &str) -> Result<EventResult, ComponentError> {
        self.action(screen, id, ComponentCommand::Click)
    }

    pub fn input_text(
        &mut self,
        screen: Rect,
        id: &str,
        text: impl Into<String>,
    ) -> Result<EventResult, ComponentError> {
        self.action(screen, id, ComponentCommand::InputText(text.into()))
    }
}

impl Desktop {
    pub fn inspect(&mut self) -> DesktopInspector<'_> {
        DesktopInspector::new(self)
    }
}

fn apply_desktop_action(desktop: &mut Desktop, action: &crate::app::DesktopAction) {
    if let crate::app::DesktopAction::CloseWindow(id) = *action {
        desktop.wm.close(id);
    }
}

fn build_desktop_tree(desktop: &Desktop, screen: Rect) -> InspectNode {
    let layout = Desktop::layout(screen);
    let mut root = InspectNode {
        kind: NodeKind::Desktop,
        id: None,
        name: "Desktop".to_string(),
        type_id: "Desktop".to_string(),
        bounds: Some(screen),
        properties: Vec::new(),
        window_id: None,
        children: Vec::new(),
    };

    root.children.push(build_menu_tree(&desktop.menu, layout));
    root.children.push(InspectNode {
        kind: NodeKind::StatusBar,
        id: None,
        name: "StatusBar".to_string(),
        type_id: "StatusBar".to_string(),
        bounds: Some(layout.status_bar),
        properties: Vec::new(),
        window_id: None,
        children: Vec::new(),
    });

    for window in desktop.wm.windows() {
        root.children.push(build_window_tree(window));
    }

    root
}

fn build_menu_tree(menu: &crate::app::MenuBar, layout: DesktopLayout) -> InspectNode {
    let mut node = InspectNode {
        kind: NodeKind::MenuBar,
        id: None,
        name: "MenuBar".to_string(),
        type_id: "MenuBar".to_string(),
        bounds: Some(layout.menu_bar),
        properties: Vec::new(),
        window_id: None,
        children: Vec::new(),
    };
    for menu in menu.menus() {
        node.children.push(build_menu_spec_tree(menu));
    }
    node
}

fn build_menu_spec_tree(menu: &MenuSpec) -> InspectNode {
    let mut node = InspectNode {
        kind: NodeKind::Menu,
        id: menu.tag.clone(),
        name: menu.title.get(),
        type_id: "Menu".to_string(),
        bounds: None,
        properties: vec!["title".to_string()],
        window_id: None,
        children: Vec::new(),
    };
    for item in &menu.items {
        node.children.push(build_menu_item_tree(item));
    }
    node
}

fn build_menu_item_tree(item: &MenuItem) -> InspectNode {
    let mut node = InspectNode {
        kind: NodeKind::MenuItem,
        id: item.tag.clone(),
        name: item.label.get(),
        type_id: "MenuItem".to_string(),
        bounds: None,
        properties: vec!["label".to_string(), "enabled".to_string()],
        window_id: None,
        children: Vec::new(),
    };
    for child in &item.submenu {
        node.children.push(build_menu_item_tree(child));
    }
    node
}

fn build_window_tree(window: &Window) -> InspectNode {
    let inner = window.inner_rect();
    let mut node = InspectNode {
        kind: NodeKind::Window,
        id: window.tag.clone(),
        name: window.title.get(),
        type_id: "Window".to_string(),
        bounds: Some(window.rect.get()),
        properties: vec![
            "title".to_string(),
            "rect".to_string(),
            "state".to_string(),
            "kind".to_string(),
        ],
        window_id: Some(window.id),
        children: Vec::new(),
    };

    let view_node = build_component_tree(window.view.as_ref(), inner, window.id);
    node.children.push(view_node);
    node
}

fn build_component_tree(view: &dyn Component, bounds: Rect, window_id: WindowId) -> InspectNode {
    let mut node = InspectNode {
        kind: NodeKind::Component,
        id: view.tag().map(|s| s.to_string()),
        name: short_type_name(view.type_name()),
        type_id: view.type_name().to_string(),
        bounds: Some(bounds),
        properties: view
            .property_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect(),
        window_id: Some(window_id),
        children: Vec::new(),
    };

    for child in view.children() {
        let child_bounds = child.bounds();
        let child_node = build_component_tree(child.view.as_ref(), child_bounds, window_id);
        node.children.push(child_node);
    }

    node
}

fn short_type_name(full: &'static str) -> String {
    full.rsplit("::").next().unwrap_or(full).to_string()
}

fn buffer_to_string(buffer: &Buffer) -> String {
    let mut out = String::new();
    let width = buffer.area.width;
    let height = buffer.area.height;
    for y in 0..height {
        for x in 0..width {
            if let Some(cell) = buffer.cell((x, y)) {
                out.push_str(cell.symbol());
            }
        }
        if y + 1 < height {
            out.push('\n');
        }
    }
    out
}

fn crop_buffer(buffer: &Buffer, area: Rect) -> Buffer {
    let mut out = Buffer::empty(Rect::new(0, 0, area.width, area.height));
    for y in 0..area.height {
        for x in 0..area.width {
            let src_x = area.x.saturating_add(x);
            let src_y = area.y.saturating_add(y);
            if let Some(cell) = buffer.cell((src_x, src_y)) {
                out[(x, y)] = cell.clone();
            }
        }
    }
    out
}

fn center_point(bounds: Rect) -> Option<(u16, u16)> {
    if bounds.width == 0 || bounds.height == 0 {
        return None;
    }
    let x = bounds.x.saturating_add(bounds.width / 2);
    let y = bounds.y.saturating_add(bounds.height / 2);
    Some((x, y))
}

fn menu_get_property(menu: &crate::app::MenuBar, id: &str, name: &str) -> Option<ComponentValue> {
    if let Some(spec) = menu_find_spec(menu, id) {
        return match name {
            "title" => Some(ComponentValue::String(spec.title.get())),
            _ => None,
        };
    }
    let item = menu_find_item(menu, id)?;
    match name {
        "label" => Some(ComponentValue::String(item.label.get())),
        "shortcut" => item
            .shortcut
            .get()
            .map(ComponentValue::String)
            .or_else(|| Some(ComponentValue::String(String::new()))),
        "enabled" => Some(ComponentValue::Bool(item.enabled.get())),
        _ => None,
    }
}

fn menu_set_property(
    menu: &mut crate::app::MenuBar,
    id: &str,
    name: &str,
    value: ComponentValue,
) -> Result<bool, ComponentError> {
    if let Some(spec) = menu_find_spec_mut(menu, id) {
        return match name {
            "title" => {
                let v: String = ComponentValueCodec::from_component_value(value, name)?;
                spec.title.set(v);
                Ok(true)
            }
            _ => Err(ComponentError::unsupported_property(name)),
        };
    }
    let Some(item) = menu_find_item_mut(menu, id) else {
        return Ok(false);
    };
    match name {
        "label" => {
            let v: String = ComponentValueCodec::from_component_value(value, name)?;
            item.label.set(v);
            Ok(true)
        }
        "shortcut" => {
            let v: String = ComponentValueCodec::from_component_value(value, name)?;
            if v.is_empty() {
                item.shortcut.set(None);
            } else {
                item.shortcut.set(Some(v));
            }
            Ok(true)
        }
        "enabled" => {
            let v: bool = ComponentValueCodec::from_component_value(value, name)?;
            item.enabled.set(v);
            Ok(true)
        }
        _ => Err(ComponentError::unsupported_property(name)),
    }
}

fn menu_action(
    menu: &mut crate::app::MenuBar,
    id: &str,
    action: &ComponentCommand,
) -> Option<EventResult> {
    let item = menu_find_item(menu, id)?;
    if !item.enabled.get() {
        return Some(EventResult::ignored());
    }
    match action {
        ComponentCommand::Click | ComponentCommand::Submit => {
            if item.submenu.is_empty()
                && let Some(cb) = &item.on_activate
            {
                cb();
                return Some(EventResult::submitted());
            }
            Some(EventResult::ignored())
        }
        _ => None,
    }
}

fn menu_find_item<'a>(menu: &'a crate::app::MenuBar, id: &str) -> Option<&'a MenuItem> {
    for spec in menu.menus() {
        if let Some(item) = menu_find_item_in_list(&spec.items, id) {
            return Some(item);
        }
    }
    None
}

fn menu_find_spec<'a>(menu: &'a crate::app::MenuBar, id: &str) -> Option<&'a MenuSpec> {
    menu.menus()
        .iter()
        .find(|spec| spec.tag.as_deref() == Some(id))
}

fn menu_find_spec_mut<'a>(menu: &'a mut crate::app::MenuBar, id: &str) -> Option<&'a mut MenuSpec> {
    menu.menus_mut()
        .iter_mut()
        .find(|spec| spec.tag.as_deref() == Some(id))
}

fn menu_find_item_mut<'a>(menu: &'a mut crate::app::MenuBar, id: &str) -> Option<&'a mut MenuItem> {
    for spec in menu.menus_mut() {
        if let Some(item) = menu_find_item_in_list_mut(&mut spec.items, id) {
            return Some(item);
        }
    }
    None
}

fn menu_find_item_in_list<'a>(items: &'a [MenuItem], id: &str) -> Option<&'a MenuItem> {
    for item in items {
        if item.tag.as_deref() == Some(id) {
            return Some(item);
        }
        if let Some(found) = menu_find_item_in_list(&item.submenu, id) {
            return Some(found);
        }
    }
    None
}

fn menu_find_item_in_list_mut<'a>(items: &'a mut [MenuItem], id: &str) -> Option<&'a mut MenuItem> {
    for item in items {
        if item.tag.as_deref() == Some(id) {
            return Some(item);
        }
        if let Some(found) = menu_find_item_in_list_mut(&mut item.submenu, id) {
            return Some(found);
        }
    }
    None
}

fn menu_exists(menu: &crate::app::MenuBar, id: &str) -> bool {
    menu_find_spec(menu, id).is_some() || menu_find_item(menu, id).is_some()
}

fn window_get_property(
    wm: &crate::wm::WindowManager,
    id: &str,
    name: &str,
) -> Option<ComponentValue> {
    let window = window_find(wm, id)?;
    match name {
        "title" => Some(ComponentValue::String(window.title.get())),
        "rect" => Some(ComponentValue::Rect(atto_ui_runtime::Rect {
            x: window.rect.get().x,
            y: window.rect.get().y,
            width: window.rect.get().width,
            height: window.rect.get().height,
        })),
        "state" => Some(ComponentValue::String(format!("{:?}", window.state.get()))),
        "kind" => Some(ComponentValue::String(format!("{:?}", window.kind))),
        _ => None,
    }
}

fn window_set_property(
    wm: &mut crate::wm::WindowManager,
    id: &str,
    name: &str,
    value: ComponentValue,
) -> Result<bool, ComponentError> {
    let Some(window) = window_find_mut(wm, id) else {
        return Ok(false);
    };
    match name {
        "title" => {
            let v: String = ComponentValueCodec::from_component_value(value, name)?;
            window.title.set(v);
            Ok(true)
        }
        "rect" => {
            let v: Rect = ComponentValueCodec::from_component_value(value, name)?;
            window.rect.set(v);
            Ok(true)
        }
        "state" => {
            let v: String = ComponentValueCodec::from_component_value(value, name)?;
            let state = match v.as_str() {
                "Normal" | "normal" => crate::wm::WindowState::Normal,
                "Minimized" | "minimized" => crate::wm::WindowState::Minimized,
                "Maximized" | "maximized" => crate::wm::WindowState::Maximized,
                _ => return Err(ComponentError::invalid_value(name, "WindowState")),
            };
            window.state.set(state);
            Ok(true)
        }
        _ => Err(ComponentError::unsupported_property(name)),
    }
}

fn window_action(
    wm: &mut crate::wm::WindowManager,
    id: &str,
    action: &ComponentCommand,
) -> Option<EventResult> {
    let window_id = window_find(wm, id)?.id;
    match action {
        ComponentCommand::Click => {
            wm.bring_to_front(window_id);
            Some(EventResult::consumed())
        }
        _ => None,
    }
}

fn window_find<'a>(wm: &'a crate::wm::WindowManager, id: &str) -> Option<&'a Window> {
    wm.windows()
        .iter()
        .find(|w| w.tag.as_deref() == Some(id))
}

fn window_find_mut<'a>(wm: &'a mut crate::wm::WindowManager, id: &str) -> Option<&'a mut Window> {
    wm.windows_mut()
        .iter_mut()
        .find(|w| w.tag.as_deref() == Some(id))
}

fn window_exists(wm: &crate::wm::WindowManager, id: &str) -> bool {
    window_find(wm, id).is_some()
}

fn component_get_property(
    wm: &crate::wm::WindowManager,
    id: &str,
    name: &str,
) -> Option<ComponentValue> {
    for window in wm.windows() {
        if let Some(found) = component_find(window.view.as_ref(), id) {
            return found.get_property(name);
        }
    }
    None
}

fn component_set_property(
    wm: &mut crate::wm::WindowManager,
    id: &str,
    name: &str,
    value: ComponentValue,
) -> Result<bool, ComponentError> {
    for window in wm.windows_mut() {
        if let Some(found) = component_find_mut(window.view.as_mut(), id) {
            found.set_property(name, value)?;
            return Ok(true);
        }
    }
    Ok(false)
}

fn component_action(
    wm: &mut crate::wm::WindowManager,
    id: &str,
    action: &ComponentCommand,
) -> Option<EventResult> {
    for window in wm.windows_mut() {
        if let Some(found) = component_find_mut(window.view.as_mut(), id) {
            return Some(found.apply_command(action.clone()));
        }
    }
    None
}

fn component_exists(wm: &crate::wm::WindowManager, id: &str) -> bool {
    for window in wm.windows() {
        if component_find(window.view.as_ref(), id).is_some() {
            return true;
        }
    }
    false
}

fn focused_component_mut(wm: &mut crate::wm::WindowManager) -> Option<&mut dyn Component> {
    let focused_window = wm.focused()?;
    let window = wm.window_mut(focused_window)?;
    focused_component_in_view(window.view.as_mut())
}

fn focused_component_in_view<'a>(view: &'a mut dyn Component) -> Option<&'a mut dyn Component> {
    let mut current: &mut dyn Component = view;
    loop {
        if let Some(child_id) = current.focused_child() {
            let Some(children) = current.children_mut() else {
                return None;
            };
            let Some(idx) = children.iter().position(|child| child.id == child_id) else {
                return None;
            };
            current = children[idx].view.as_mut();
            continue;
        }

        if !current.children().is_empty() {
            return None;
        }

        if current.is_focusable() {
            return Some(current);
        }

        return None;
    }
}

fn component_find<'a>(view: &'a dyn Component, id: &str) -> Option<&'a dyn Component> {
    if view.tag() == Some(id) {
        return Some(view);
    }
    for child in view.children() {
        if let Some(found) = component_find(child.view.as_ref(), id) {
            return Some(found);
        }
    }
    None
}

fn component_find_mut<'a>(view: &'a mut dyn Component, id: &str) -> Option<&'a mut dyn Component> {
    if view.tag() == Some(id) {
        return Some(view);
    }
    let children = view.children_mut()?;
    for child in children {
        if let Some(found) = component_find_mut(child.view.as_mut(), id) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::MenuBar;
    use crate::composable::{ComponentTagExt, Label, TabView, Visibility};
    use crate::reactive::Binding;
    use crate::theme::Theme;
    use crate::wm::{Window, WindowKind};

    #[test]
    fn inspect_tree_finds_tags() {
        let screen = Rect::new(0, 0, 80, 24);
        let menu = MenuBar::new(vec![
            MenuSpec::new(
                "File",
                vec![MenuItem::action("Open", || {}).with_tag("menu_open")],
            )
            .with_tag("menu_file"),
        ]);
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let view = Label::new("Hello").tag("label");
        let window = Window::new(
            WindowKind::Normal,
            "Win",
            Rect::new(2, 2, 20, 6),
            Box::new(view),
        )
        .with_tag("win1");
        desktop.add_window(window, screen);

        let mut inspector = desktop.inspect();
        let tree = inspector.tree(screen).expect("tree");
        assert!(tree.find_by_id("menu_file").is_some());
        assert!(tree.find_by_id("menu_open").is_some());
        assert!(tree.find_by_id("win1").is_some());
        assert!(tree.find_by_id("label").is_some());
    }

    #[test]
    fn inspect_can_select_tab() {
        let screen = Rect::new(0, 0, 80, 24);
        let menu = MenuBar::new(vec![]);
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let selection = Binding::new(0usize);
        let tabs = TabView::new()
            .selection(selection.clone())
            .tab("One", Label::new("one"))
            .tab("Two", Label::new("two"))
            .tag("tabs");

        let window = Window::new(
            WindowKind::Normal,
            "Tabs",
            Rect::new(1, 1, 30, 8),
            Box::new(tabs),
        );
        desktop.add_window(window, screen);

        let mut inspector = desktop.inspect();
        inspector
            .action(screen, "tabs", ComponentCommand::SelectIndex(1))
            .expect("select");
        assert_eq!(selection.get(), 1);
    }

    #[test]
    fn inspect_can_set_table_rows() {
        let screen = Rect::new(0, 0, 80, 24);
        let menu = MenuBar::new(vec![]);
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let rows = Binding::new(vec![vec!["a".into(), "b".into()]]);
        let table = crate::composable::TableView::new(
            "Data",
            vec!["H1".into(), "H2".into()],
            rows.clone(),
            Binding::new(0usize),
        )
        .tag("table");

        let window = Window::new(
            WindowKind::Normal,
            "Table",
            Rect::new(1, 1, 40, 10),
            Box::new(table),
        );
        desktop.add_window(window, screen);

        let mut inspector = desktop.inspect();
        let new_rows = vec![vec!["x".into(), "y".into()], vec!["1".into(), "2".into()]];
        inspector
            .set_property("table", "rows", ComponentValue::Table(new_rows.clone()))
            .expect("rows");
        assert_eq!(rows.get(), new_rows);
    }

    #[test]
    fn inspect_can_toggle_visibility() {
        let screen = Rect::new(0, 0, 80, 24);
        let menu = MenuBar::new(vec![]);
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let visible = Binding::new(true);
        let view = Visibility::new(visible.clone(), Label::new("Hello")).tag("vis");
        let window = Window::new(
            WindowKind::Normal,
            "Vis",
            Rect::new(1, 1, 20, 6),
            Box::new(view),
        );
        desktop.add_window(window, screen);

        let mut inspector = desktop.inspect();
        inspector
            .set_property("vis", "visible", ComponentValue::Bool(false))
            .expect("visible");
        assert!(!visible.get());
    }
}
