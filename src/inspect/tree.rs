//! Inspect-node tree and serializable snapshot-tree builders.
//!
//! [`build_desktop_tree`] produces the lightweight [`InspectNode`] tree used by
//! `tree()`/`snapshot()`; [`build_desktop_snapshot_tree`] produces the
//! [`DesktopSnapshotNode`] tree used by `export_snapshot()`. The two walks are
//! structurally similar but keep separate field-shaping helpers because the
//! snapshot tree clips and classifies values differently.

use std::collections::BTreeMap;

use ratatui::layout::Rect;

use crate::app::{Desktop, DesktopLayout, MenuItem, MenuSpec};
use crate::composable::Component;
use crate::runtime::ComponentValue;
use crate::wm::{Window, WindowId};

use super::{
    DesktopSnapshotNode, InspectNode, NodeKind, runtime_rect, short_type_name,
};

// ---------------------------------------------------------------------------
// InspectNode tree
// ---------------------------------------------------------------------------

pub(super) fn build_desktop_tree(desktop: &Desktop, screen: Rect) -> InspectNode {
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

pub(super) fn collect_untagged_interactive_nodes(node: &InspectNode, nodes: &mut Vec<InspectNode>) {
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

// ---------------------------------------------------------------------------
// DesktopSnapshotNode tree
// ---------------------------------------------------------------------------

pub(super) fn build_desktop_snapshot_tree(desktop: &Desktop, screen: Rect) -> DesktopSnapshotNode {
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
