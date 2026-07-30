//! Tag-based backend dispatch for the inspector.
//!
//! [`resolve_dispatch_target`] resolves which backend owns a tag using the
//! fixed `menu → window → component` precedence. The `menu_*`, `window_*` and
//! `component_*` families then implement property reads / writes / actions /
//! command-support queries and existence checks against that backend.

use ratatui::layout::Rect;

use crate::app::{MenuItem, MenuSpec};
use crate::composable::{Component, EventResult, find_by_tag, find_by_tag_mut};
use crate::runtime::{ComponentValue, Rect as RuntimeRect};
use crate::wm::Window;
use crate::{ComponentCommand, ComponentError, ComponentValueCodec};

/// Which backend owns a tag. The `menu → window → component` precedence lives here alone, so the
/// property/action methods resolve ownership once and dispatch, instead of each re-deriving the
/// same fallthrough chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DispatchTarget {
    Menu,
    Window,
    Component,
}

pub(super) fn resolve_dispatch_target(desktop: &crate::app::Desktop, id: &str) -> Option<DispatchTarget> {
    if menu_exists(&desktop.menu, id) {
        Some(DispatchTarget::Menu)
    } else if window_exists(&desktop.wm, id) {
        Some(DispatchTarget::Window)
    } else if component_exists(&desktop.wm, id) {
        Some(DispatchTarget::Component)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Menu backend
// ---------------------------------------------------------------------------

pub(super) fn menu_get_property(menu: &crate::app::MenuBar, id: &str, name: &str) -> Option<ComponentValue> {
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

pub(super) fn menu_property_names(menu: &crate::app::MenuBar, id: &str) -> Option<Vec<String>> {
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

pub(super) fn menu_set_property(
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

pub(super) fn menu_action(
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

pub(super) fn menu_command_supported(
    menu: &crate::app::MenuBar,
    id: &str,
    action: &ComponentCommand,
) -> Option<bool> {
    menu_find_item(menu, id)?;
    Some(matches!(
        action,
        ComponentCommand::Click | ComponentCommand::Submit
    ))
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

// ---------------------------------------------------------------------------
// Window backend
// ---------------------------------------------------------------------------

pub(super) fn window_get_property(
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

pub(super) fn window_property_names(wm: &crate::wm::WindowManager, id: &str) -> Option<Vec<String>> {
    window_find(wm, id).map(|_| {
        vec![
            "title".to_string(),
            "rect".to_string(),
            "state".to_string(),
            "kind".to_string(),
        ]
    })
}

pub(super) fn window_set_property(
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

pub(super) fn window_action(
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

pub(super) fn window_command_supported(
    wm: &crate::wm::WindowManager,
    id: &str,
    action: &ComponentCommand,
) -> Option<bool> {
    window_find(wm, id).map(|_| matches!(action, ComponentCommand::Click))
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

// ---------------------------------------------------------------------------
// Component backend
// ---------------------------------------------------------------------------

pub(super) fn component_get_property(
    wm: &crate::wm::WindowManager,
    id: &str,
    name: &str,
) -> Option<ComponentValue> {
    for window in wm.windows() {
        if let Some(found) = find_by_tag(window.view.as_ref(), id) {
            return found.get_property(name);
        }
    }
    None
}

pub(super) fn component_property_names(
    wm: &crate::wm::WindowManager,
    id: &str,
) -> Option<Vec<String>> {
    for window in wm.windows() {
        if let Some(found) = find_by_tag(window.view.as_ref(), id) {
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

pub(super) fn component_set_property(
    wm: &mut crate::wm::WindowManager,
    id: &str,
    name: &str,
    value: ComponentValue,
) -> Result<bool, ComponentError> {
    for window in wm.windows_mut() {
        if let Some(found) = find_by_tag_mut(window.view.as_mut(), id) {
            found.set_property(name, value)?;
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn component_action(
    wm: &mut crate::wm::WindowManager,
    id: &str,
    action: &ComponentCommand,
) -> Option<EventResult> {
    for window in wm.windows_mut() {
        if let Some(found) = find_by_tag_mut(window.view.as_mut(), id) {
            return Some(found.apply_command(action.clone()));
        }
    }
    None
}

pub(super) fn component_command_supported(
    wm: &crate::wm::WindowManager,
    id: &str,
    action: &ComponentCommand,
) -> Option<bool> {
    for window in wm.windows() {
        if let Some(found) = find_by_tag(window.view.as_ref(), id) {
            return Some(found.supports_command(action));
        }
    }
    None
}

fn component_exists(wm: &crate::wm::WindowManager, id: &str) -> bool {
    for window in wm.windows() {
        if find_by_tag(window.view.as_ref(), id).is_some() {
            return true;
        }
    }
    false
}

pub(super) fn focused_component_mut(
    wm: &mut crate::wm::WindowManager,
) -> Option<&mut dyn Component> {
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
