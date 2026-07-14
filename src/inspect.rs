use std::collections::BTreeMap;

use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

use crate::app::{Desktop, DesktopLayout, MenuItem, MenuSpec};
use crate::composable::{Component, EventResult, find_by_tag, find_by_tag_mut};
use crate::reactive::{DirtySignal, DirtySignalSet};
use crate::runtime::{ComponentValue, Rect as RuntimeRect};
use crate::wm::{Window, WindowId};
use crate::{ComponentCommand, ComponentError, ComponentTarget, ComponentValueCodec};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    pub focusable: bool,
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

/// Serializable desktop snapshot for host-language assertions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesktopSnapshot {
    pub bounds: RuntimeRect,
    pub tree: DesktopSnapshotNode,
}

/// Serializable node in a desktop snapshot tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesktopSnapshotNode {
    pub kind: NodeKind,
    pub id: Option<String>,
    pub tag: Option<String>,
    pub name: String,
    pub type_name: String,
    pub bounds: Option<RuntimeRect>,
    pub text: Option<String>,
    pub state: Option<String>,
    pub window_id: Option<u64>,
    pub properties: BTreeMap<String, ComponentValue>,
    pub children: Vec<DesktopSnapshotNode>,
}

impl DesktopSnapshotNode {
    pub fn find_by_id(&self, id: &str) -> Option<&DesktopSnapshotNode> {
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

#[derive(Clone, Debug, Default)]
pub struct DesktopChangeTracker {
    signals: DirtySignalSet,
}

impl DesktopChangeTracker {
    pub fn new(signals: Vec<DirtySignal>) -> Self {
        Self {
            signals: DirtySignalSet::new(signals),
        }
    }

    pub fn changed_since_last_poll(&mut self) -> bool {
        self.signals.changed_since_last_poll()
    }

    pub fn refresh(&mut self, signals: Vec<DirtySignal>) {
        self.signals.refresh(signals);
    }

    pub fn signal_count(&self) -> usize {
        self.signals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }
}

impl<'a> DesktopInspector<'a> {
    pub fn new(desktop: &'a mut Desktop) -> Self {
        Self { desktop }
    }

    pub fn change_tracker(&self) -> DesktopChangeTracker {
        DesktopChangeTracker::new(collect_desktop_dirty_signals(self.desktop))
    }

    pub fn refresh_change_tracker(&self, tracker: &mut DesktopChangeTracker) {
        tracker.refresh(collect_desktop_dirty_signals(self.desktop));
    }

    pub fn tree(&mut self, screen: Rect) -> Result<InspectNode, ComponentError> {
        Ok(self.snapshot(screen)?.tree)
    }

    pub fn snapshot(&mut self, screen: Rect) -> Result<InspectSnapshot, ComponentError> {
        let terminal = draw_desktop(self.desktop, screen)?;
        let buffer = terminal.backend().buffer().clone();
        let tree = build_desktop_tree(self.desktop, screen);
        Ok(InspectSnapshot { buffer, tree })
    }

    pub fn export_snapshot(&mut self, screen: Rect) -> Result<DesktopSnapshot, ComponentError> {
        draw_desktop(self.desktop, screen)?;
        Ok(DesktopSnapshot {
            bounds: runtime_rect(screen),
            tree: build_desktop_snapshot_tree(self.desktop, screen),
        })
    }

    pub fn get_property(&mut self, id: &str, name: &str) -> Result<ComponentValue, ComponentError> {
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

    pub fn property_names(&mut self, id: &str) -> Result<Vec<String>, ComponentError> {
        if let Some(names) = menu_property_names(&self.desktop.menu, id) {
            return Ok(names);
        }
        if let Some(names) = window_property_names(&self.desktop.wm, id) {
            return Ok(names);
        }
        if let Some(names) = component_property_names(&self.desktop.wm, id) {
            return Ok(names);
        }
        Err(ComponentError::not_found(id))
    }

    /// Returns interactive nodes that cannot be targeted by tag-based scripts.
    pub fn untagged_interactive_nodes(&mut self, screen: Rect) -> Vec<InspectNode> {
        let _ = draw_desktop(self.desktop, screen);
        let tree = build_desktop_tree(self.desktop, screen);
        let mut nodes = Vec::new();
        collect_untagged_interactive_nodes(&tree, &mut nodes);
        nodes
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

        if let Some(name) = custom_name
            && (menu_exists(&self.desktop.menu, id)
                || window_exists(&self.desktop.wm, id)
                || component_exists(&self.desktop.wm, id))
        {
            return Err(ComponentError::action_not_supported(name));
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
                    capture: crate::composable::Capture::None,
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
                    capture: crate::composable::Capture::None,
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

    fn action_focused(&mut self, action: ComponentCommand) -> Result<EventResult, ComponentError> {
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

fn draw_desktop(
    desktop: &mut Desktop,
    screen: Rect,
) -> Result<Terminal<TestBackend>, ComponentError> {
    let backend = TestBackend::new(screen.width, screen.height);
    let mut terminal = Terminal::new(backend).map_err(ComponentError::render_failed)?;
    terminal
        .draw(|f| desktop.draw(f))
        .map_err(ComponentError::render_failed)?;
    Ok(terminal)
}

fn collect_desktop_dirty_signals(desktop: &Desktop) -> Vec<DirtySignal> {
    let mut signals = Vec::new();
    collect_menu_dirty_signals(&desktop.menu, &mut signals);
    signals.extend(desktop.status.dirty_signals());
    for window in desktop.wm.windows() {
        collect_window_dirty_signals(window, &mut signals);
    }
    signals
}

fn collect_menu_dirty_signals(menu: &crate::app::MenuBar, signals: &mut Vec<DirtySignal>) {
    for spec in menu.menus() {
        signals.push(spec.title.dirty_signal());
        collect_menu_item_dirty_signals(&spec.items, signals);
    }
}

fn collect_menu_item_dirty_signals(items: &[MenuItem], signals: &mut Vec<DirtySignal>) {
    for item in items {
        signals.push(item.label.dirty_signal());
        signals.push(item.shortcut.dirty_signal());
        signals.push(item.accelerator.dirty_signal());
        signals.push(item.mnemonic.dirty_signal());
        signals.push(item.enabled.dirty_signal());
        collect_menu_item_dirty_signals(&item.submenu, signals);
    }
}

fn collect_window_dirty_signals(window: &Window, signals: &mut Vec<DirtySignal>) {
    signals.push(window.title.dirty_signal());
    signals.push(window.rect.dirty_signal());
    signals.push(window.state.dirty_signal());
    signals.push(window.dock.dirty_signal());
    signals.push(window.decorations.dirty_signal());
    signals.push(window.min_size.dirty_signal());
    signals.push(window.min_size_mode.dirty_signal());
    signals.push(window.movable.dirty_signal());
    signals.push(window.resizable.dirty_signal());
    signals.push(window.closable.dirty_signal());
    collect_component_dirty_signals(window.view.as_ref(), signals);
}

fn collect_component_dirty_signals(view: &dyn Component, signals: &mut Vec<DirtySignal>) {
    signals.extend(view.dirty_signals());
    for child in view.children() {
        collect_component_dirty_signals(child.view.as_ref(), signals);
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
        focusable: false,
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
        focusable: false,
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
        focusable: false,
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
        focusable: false,
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
        focusable: false,
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
        focusable: window.kind.is_focusable(),
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
        focusable: view.is_focusable(),
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

fn collect_untagged_interactive_nodes(node: &InspectNode, nodes: &mut Vec<InspectNode>) {
    if node.id.is_none() && is_interactive_inspect_node(node) {
        nodes.push(node.clone());
    }

    for child in &node.children {
        collect_untagged_interactive_nodes(child, nodes);
    }
}

fn is_interactive_inspect_node(node: &InspectNode) -> bool {
    node.focusable
        || node
            .properties
            .iter()
            .any(|name| is_interactive_component_property(name))
}

fn is_interactive_component_property(name: &str) -> bool {
    matches!(
        name,
        "active"
            | "checked"
            | "index"
            | "progress"
            | "selected"
            | "selected_index"
            | "selection"
            | "text"
            | "value"
    )
}

fn build_desktop_snapshot_tree(desktop: &Desktop, screen: Rect) -> DesktopSnapshotNode {
    let layout = Desktop::layout(screen);
    let mut root = DesktopSnapshotNode {
        kind: NodeKind::Desktop,
        id: None,
        tag: None,
        name: "Desktop".to_string(),
        type_name: "Desktop".to_string(),
        bounds: Some(runtime_rect(screen)),
        text: None,
        state: None,
        window_id: None,
        properties: BTreeMap::new(),
        children: Vec::new(),
    };

    root.children
        .push(build_menu_snapshot_tree(&desktop.menu, layout));
    root.children.push(DesktopSnapshotNode {
        kind: NodeKind::StatusBar,
        id: None,
        tag: None,
        name: "StatusBar".to_string(),
        type_name: "StatusBar".to_string(),
        bounds: Some(runtime_rect(layout.status_bar)),
        text: None,
        state: None,
        window_id: None,
        properties: BTreeMap::new(),
        children: Vec::new(),
    });

    let focused = desktop.wm.focused();
    for window in desktop.wm.windows() {
        root.children.push(build_window_snapshot_tree(
            window,
            focused == Some(window.id()),
        ));
    }

    root
}

fn build_menu_snapshot_tree(
    menu: &crate::app::MenuBar,
    layout: DesktopLayout,
) -> DesktopSnapshotNode {
    let mut node = DesktopSnapshotNode {
        kind: NodeKind::MenuBar,
        id: None,
        tag: None,
        name: "MenuBar".to_string(),
        type_name: "MenuBar".to_string(),
        bounds: Some(runtime_rect(layout.menu_bar)),
        text: None,
        state: None,
        window_id: None,
        properties: BTreeMap::new(),
        children: Vec::new(),
    };
    for menu in menu.menus() {
        node.children.push(build_menu_spec_snapshot_tree(menu));
    }
    node
}

fn build_menu_spec_snapshot_tree(menu: &MenuSpec) -> DesktopSnapshotNode {
    let mut properties = BTreeMap::new();
    properties.insert(
        "title".to_string(),
        ComponentValue::String(menu.title.get()),
    );
    let text = text_from_properties(&properties);
    let tag = menu.tag.clone();
    let mut node = DesktopSnapshotNode {
        kind: NodeKind::Menu,
        id: tag.clone(),
        tag,
        name: menu.title.get(),
        type_name: "Menu".to_string(),
        bounds: None,
        text,
        state: state_from_properties(&properties),
        window_id: None,
        properties,
        children: Vec::new(),
    };
    for item in &menu.items {
        node.children.push(build_menu_item_snapshot_tree(item));
    }
    node
}

fn build_menu_item_snapshot_tree(item: &MenuItem) -> DesktopSnapshotNode {
    let mut properties = BTreeMap::new();
    properties.insert(
        "label".to_string(),
        ComponentValue::String(item.label.get()),
    );
    properties.insert(
        "shortcut".to_string(),
        ComponentValue::String(item.shortcut.get().unwrap_or_default()),
    );
    properties.insert(
        "enabled".to_string(),
        ComponentValue::Bool(item.enabled.get()),
    );
    let text = text_from_properties(&properties);
    let tag = item.tag.clone();
    let mut node = DesktopSnapshotNode {
        kind: NodeKind::MenuItem,
        id: tag.clone(),
        tag,
        name: item.label.get(),
        type_name: "MenuItem".to_string(),
        bounds: None,
        text,
        state: state_from_properties(&properties),
        window_id: None,
        properties,
        children: Vec::new(),
    };
    for child in &item.submenu {
        node.children.push(build_menu_item_snapshot_tree(child));
    }
    node
}

fn build_window_snapshot_tree(window: &Window, focused: bool) -> DesktopSnapshotNode {
    let inner = window.inner_rect();
    let mut properties = BTreeMap::new();
    properties.insert(
        "title".to_string(),
        ComponentValue::String(window.title.get()),
    );
    properties.insert(
        "rect".to_string(),
        ComponentValue::Rect(runtime_rect(window.rect.get())),
    );
    properties.insert(
        "state".to_string(),
        ComponentValue::String(format!("{:?}", window.state.get())),
    );
    properties.insert(
        "kind".to_string(),
        ComponentValue::String(format!("{:?}", window.kind)),
    );
    properties.insert("focused".to_string(), ComponentValue::Bool(focused));
    let text = text_from_properties(&properties);
    let tag = window.tag.clone();
    let mut node = DesktopSnapshotNode {
        kind: NodeKind::Window,
        id: tag.clone(),
        tag,
        name: window.title.get(),
        type_name: "Window".to_string(),
        bounds: Some(runtime_rect(window.rect.get())),
        text,
        state: state_from_properties(&properties),
        window_id: Some(window.id().raw()),
        properties,
        children: Vec::new(),
    };

    node.children.push(build_component_snapshot_tree(
        window.view.as_ref(),
        inner,
        window.id(),
    ));
    node
}

fn build_component_snapshot_tree(
    view: &dyn Component,
    bounds: Rect,
    window_id: WindowId,
) -> DesktopSnapshotNode {
    let (properties, text, state) = component_snapshot_fields(view);
    let tag = view.tag().map(|s| s.to_string());
    let mut node = DesktopSnapshotNode {
        kind: NodeKind::Component,
        id: tag.clone(),
        tag,
        name: short_type_name(view.type_name()),
        type_name: view.type_name().to_string(),
        bounds: Some(runtime_rect(bounds)),
        text,
        state,
        window_id: Some(window_id.raw()),
        properties,
        children: Vec::new(),
    };

    for child in view.children() {
        let child_bounds = child.bounds();
        let child_node =
            build_component_snapshot_tree(child.view.as_ref(), child_bounds, window_id);
        node.children.push(child_node);
    }

    node
}

fn component_snapshot_fields(
    view: &dyn Component,
) -> (
    BTreeMap<String, ComponentValue>,
    Option<String>,
    Option<String>,
) {
    let mut properties = BTreeMap::new();
    let mut text = None;
    let mut state = None;

    for name in view.property_names() {
        if !is_snapshot_component_property(name) {
            continue;
        }

        let Some(value) = view.get_property(name) else {
            continue;
        };

        if is_text_property(name) {
            match value {
                ComponentValue::String(value) if text.is_none() => {
                    text = Some(value);
                }
                value if is_bounded_snapshot_value(&value) => {
                    properties.insert(name.to_string(), value);
                }
                _ => {}
            }
            continue;
        }

        if name == "state"
            && let ComponentValue::String(value) = &value
        {
            state = Some(value.clone());
        }

        if is_bounded_snapshot_value(&value) {
            properties.insert(name.to_string(), value);
        }
    }

    (properties, text, state)
}

fn is_snapshot_component_property(name: &str) -> bool {
    is_text_property(name)
        || matches!(
            name,
            "active"
                | "checked"
                | "disabled"
                | "enabled"
                | "focused"
                | "height"
                | "index"
                | "kind"
                | "max"
                | "min"
                | "progress"
                | "rect"
                | "selected"
                | "selected_index"
                | "selection"
                | "state"
                | "visible"
                | "width"
        )
}

fn is_text_property(name: &str) -> bool {
    matches!(name, "text" | "label" | "value" | "title")
}

fn is_bounded_snapshot_value(value: &ComponentValue) -> bool {
    match value {
        ComponentValue::Null
        | ComponentValue::Bool(_)
        | ComponentValue::I64(_)
        | ComponentValue::U64(_)
        | ComponentValue::F64(_)
        | ComponentValue::Rect(_) => true,
        ComponentValue::String(value) => value.len() <= 1024,
        ComponentValue::StringList(_)
        | ComponentValue::Table(_)
        | ComponentValue::Bytes(_)
        | ComponentValue::List(_)
        | ComponentValue::Map(_) => false,
    }
}

fn text_from_properties(properties: &BTreeMap<String, ComponentValue>) -> Option<String> {
    for key in ["text", "label", "value", "title"] {
        if let Some(ComponentValue::String(value)) = properties.get(key) {
            return Some(value.clone());
        }
    }
    None
}

fn state_from_properties(properties: &BTreeMap<String, ComponentValue>) -> Option<String> {
    match properties.get("state") {
        Some(ComponentValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn runtime_rect(rect: Rect) -> RuntimeRect {
    RuntimeRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
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

fn menu_property_names(menu: &crate::app::MenuBar, id: &str) -> Option<Vec<String>> {
    if menu_find_spec(menu, id).is_some() {
        return Some(vec!["title".to_string()]);
    }
    menu_find_item(menu, id).map(|_| {
        vec![
            "label".to_string(),
            "shortcut".to_string(),
            "enabled".to_string(),
        ]
    })
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
                item.accelerator.set(None);
            } else {
                item.shortcut.set(Some(v.clone()));
                item.accelerator.set(Some(v));
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
        "rect" => Some(ComponentValue::Rect(RuntimeRect {
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

fn window_property_names(wm: &crate::wm::WindowManager, id: &str) -> Option<Vec<String>> {
    window_find(wm, id).map(|_| {
        vec![
            "title".to_string(),
            "rect".to_string(),
            "state".to_string(),
            "kind".to_string(),
        ]
    })
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
    wm.windows().iter().find(|w| w.tag.as_deref() == Some(id))
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

fn component_property_names(wm: &crate::wm::WindowManager, id: &str) -> Option<Vec<String>> {
    for window in wm.windows() {
        if let Some(found) = component_find(window.view.as_ref(), id) {
            return Some(
                found
                    .property_names()
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
            );
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

fn focused_component_in_view(view: &mut dyn Component) -> Option<&mut dyn Component> {
    let mut current: &mut dyn Component = view;
    loop {
        if let Some(child_id) = current.focused_child() {
            let children = current.children_mut()?;
            let idx = children.iter().position(|child| child.id == child_id)?;
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
    find_by_tag(view, id)
}

fn component_find_mut<'a>(view: &'a mut dyn Component, id: &str) -> Option<&'a mut dyn Component> {
    find_by_tag_mut(view, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::MenuBar;
    use crate::composable::{
        Checkbox, ComponentTagExt, Label, TabView, TableView, VStack, Visibility,
    };
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
    fn export_snapshot_contains_serializable_tree_bounds_and_text() {
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
        let window_id = desktop.add_window(window, screen);

        let mut inspector = desktop.inspect();
        let snapshot = inspector.export_snapshot(screen).expect("snapshot");
        serde_json::to_string(&snapshot).expect("serializable snapshot");

        assert_eq!(
            snapshot.bounds,
            RuntimeRect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            }
        );

        let menu_item = snapshot
            .tree
            .find_by_id("menu_open")
            .expect("menu item node");
        assert_eq!(menu_item.kind, NodeKind::MenuItem);
        assert_eq!(menu_item.text.as_deref(), Some("Open"));

        let win = snapshot.tree.find_by_id("win1").expect("window node");
        assert_eq!(win.kind, NodeKind::Window);
        assert_eq!(win.tag.as_deref(), Some("win1"));
        assert_eq!(win.text.as_deref(), Some("Win"));
        assert_eq!(win.state.as_deref(), Some("Normal"));
        assert_eq!(win.window_id, Some(window_id.raw()));
        assert_eq!(
            win.bounds,
            Some(RuntimeRect {
                x: 2,
                y: 2,
                width: 20,
                height: 6,
            })
        );
        assert_eq!(
            win.properties.get("focused"),
            Some(&ComponentValue::Bool(true))
        );

        let label = snapshot.tree.find_by_id("label").expect("label node");
        assert_eq!(label.kind, NodeKind::Component);
        assert_eq!(label.name, "Label");
        assert!(label.type_name.ends_with("Label"));
        assert_eq!(label.text.as_deref(), Some("Hello"));
        assert!(!label.properties.contains_key("text"));
        assert_eq!(
            label.bounds,
            Some(RuntimeRect {
                x: 3,
                y: 3,
                width: 18,
                height: 4,
            })
        );
    }

    #[test]
    fn export_snapshot_omits_large_collection_properties() {
        let screen = Rect::new(0, 0, 80, 24);
        let menu = MenuBar::new(vec![]);
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let rows = Binding::new(vec![vec!["a".to_string(), "b".to_string()]; 32]);
        let table = TableView::new(
            "Data",
            vec!["H1".to_string(), "H2".to_string()],
            rows,
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
        let snapshot = inspector.export_snapshot(screen).expect("snapshot");
        let table = snapshot.tree.find_by_id("table").expect("table node");

        assert_eq!(table.text.as_deref(), Some("Data"));
        assert_eq!(
            table.properties.get("enabled"),
            Some(&ComponentValue::Bool(true))
        );
        assert_eq!(
            table.properties.get("selection"),
            Some(&ComponentValue::U64(0))
        );
        assert!(!table.properties.contains_key("headers"));
        assert!(!table.properties.contains_key("rows"));
        assert!(!table.properties.contains_key("title"));
    }

    #[test]
    fn inspect_property_names_resolves_menu_window_and_component_ids() {
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

        assert_eq!(
            inspector.property_names("menu_file").expect("menu spec"),
            vec!["title".to_string()]
        );
        assert_eq!(
            inspector.property_names("menu_open").expect("menu item"),
            vec![
                "label".to_string(),
                "shortcut".to_string(),
                "enabled".to_string(),
            ]
        );
        assert_eq!(
            inspector.property_names("win1").expect("window"),
            vec![
                "title".to_string(),
                "rect".to_string(),
                "state".to_string(),
                "kind".to_string(),
            ]
        );

        let component_names = inspector.property_names("label").expect("component");
        assert!(component_names.contains(&"text".to_string()));
        assert!(component_names.contains(&"enabled".to_string()));
    }

    #[test]
    fn inspect_property_names_unknown_id_returns_not_found() {
        let menu = MenuBar::new(vec![]);
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let mut inspector = desktop.inspect();

        assert_eq!(
            inspector.property_names("missing"),
            Err(ComponentError::NotFound("missing".to_string()))
        );
    }

    #[test]
    fn untagged_interactive_nodes_reports_only_interactive_nodes_without_tags() {
        let screen = Rect::new(0, 0, 80, 24);
        let menu = MenuBar::new(vec![]);
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let view = VStack::new()
            .child(Checkbox::new("Tagged", Binding::new(false)).tag("tagged_checkbox"))
            .child(Checkbox::new("Missing tag", Binding::new(false)))
            .tag("root_stack");
        let window = Window::new(
            WindowKind::Normal,
            "Checks",
            Rect::new(1, 1, 32, 8),
            Box::new(view),
        )
        .with_tag("checks_window");
        desktop.add_window(window, screen);

        let mut inspector = desktop.inspect();
        let nodes = inspector.untagged_interactive_nodes(screen);

        assert_eq!(nodes.len(), 1);
        let node = &nodes[0];
        assert_eq!(node.kind, NodeKind::Component);
        assert_eq!(node.id, None);
        assert_eq!(node.name, "Checkbox");
        assert!(node.focusable);
        assert!(node.properties.contains(&"checked".to_string()));
    }

    #[test]
    fn desktop_change_tracker_reports_binding_changes_once() {
        let screen = Rect::new(0, 0, 80, 24);
        let menu = MenuBar::new(vec![]);
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let text = Binding::new("Hello".to_string());
        let view = Label::new(text.clone()).tag("label");
        let window = Window::new(
            WindowKind::Normal,
            "Win",
            Rect::new(2, 2, 20, 6),
            Box::new(view),
        );
        desktop.add_window(window, screen);

        let mut tracker = desktop.inspect().change_tracker();
        assert!(!tracker.is_empty());
        assert!(!tracker.changed_since_last_poll());

        text.set("Updated".to_string());
        assert!(tracker.changed_since_last_poll());
        assert!(!tracker.changed_since_last_poll());

        text.mark_clean();
        assert!(!tracker.changed_since_last_poll());
    }

    #[test]
    fn desktop_change_tracker_refreshes_new_binding_sources() {
        let screen = Rect::new(0, 0, 80, 24);
        let menu = MenuBar::new(vec![]);
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let mut tracker = desktop.inspect().change_tracker();
        assert!(tracker.is_empty());

        let text = Binding::new("First".to_string());
        let view = Label::new(text.clone()).tag("label");
        let window = Window::new(
            WindowKind::Normal,
            "Win",
            Rect::new(2, 2, 20, 6),
            Box::new(view),
        );
        desktop.add_window(window, screen);

        desktop.inspect().refresh_change_tracker(&mut tracker);
        assert!(!tracker.is_empty());
        assert!(!tracker.changed_since_last_poll());

        text.set("Second".to_string());
        assert!(tracker.changed_since_last_poll());
        assert!(!tracker.changed_since_last_poll());
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
