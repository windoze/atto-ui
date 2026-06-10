#![deny(unsafe_code)]
#![allow(unsafe_op_in_unsafe_fn)]

//! Native Node.js binding entry points for atto-ui.

use std::collections::HashMap;
use std::time::Duration;

use atto_ui::app::{
    AppControl, AppHost as CoreAppHost, CrosstermAppConfig, CursorMode, Desktop, DesktopAction,
    DesktopEventResult, MenuBar, MenuItem, MenuSpec, WindowInfo,
};
use atto_ui::inspect::{DesktopSnapshot, DesktopSnapshotNode, NodeKind};
use atto_ui::runtime::{
    CallbackInvocation, CallbackRegistry, Rect as RuntimeRect, global_registry,
};
use atto_ui::theme::Theme;
use atto_ui::{WindowId, WindowKind, WindowState};
use napi_derive::napi;
use ratatui::layout::Rect as TuiRect;
use serde_json::{Map, Number, Value};

pub mod convert;
pub mod error;
mod event;
pub mod ids;

use crate::convert::{
    callback_invocation_to_json, component_schema_to_json, component_spec_from_json,
    component_value_from_json, component_value_to_json, tree_ops_from_json,
};
use crate::ids::{CallbackHandles, WindowHandles};

/// JavaScript-facing rectangle used by window construction.
#[napi(object)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// Configuration for `new AppHost(...)`.
#[napi(object)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AppHostConfig {
    /// Headless hosts use an in-memory terminal and never touch the real TTY.
    pub headless: Option<bool>,
    /// Headless terminal width. Defaults to 80.
    pub cols: Option<u16>,
    /// Headless terminal height. Defaults to 24.
    pub rows: Option<u16>,
    /// Crossterm polling interval in milliseconds. Defaults to 0 for non-blocking `step()`.
    pub tick_rate: Option<u32>,
    /// Enable terminal mouse capture. Defaults to true.
    pub mouse_capture: Option<bool>,
    /// Hide the terminal cursor while the host is active. Defaults to true.
    pub hide_cursor: Option<bool>,
    /// Enable bracketed paste in real-terminal mode. Defaults to false.
    pub bracketed_paste: Option<bool>,
    /// Enable keyboard enhancement flags in real-terminal mode. Defaults to true.
    pub keyboard_enhancement: Option<bool>,
}

impl AppHostConfig {
    fn headless(self) -> bool {
        self.headless.unwrap_or(false)
    }

    fn cols(self) -> u16 {
        self.cols.unwrap_or(80)
    }

    fn rows(self) -> u16 {
        self.rows.unwrap_or(24)
    }

    fn tick_rate(self) -> Duration {
        Duration::from_millis(self.tick_rate.unwrap_or(0) as u64)
    }

    fn mouse_capture(self) -> bool {
        self.mouse_capture.unwrap_or(true)
    }

    fn cursor(self) -> CursorMode {
        if self.hide_cursor.unwrap_or(true) {
            CursorMode::Hide
        } else {
            CursorMode::Show
        }
    }

    fn bracketed_paste(self) -> bool {
        self.bracketed_paste.unwrap_or(false)
    }

    fn keyboard_enhancement(self) -> bool {
        self.keyboard_enhancement.unwrap_or(true)
    }
}

/// Node binding wrapper around the runtime `atto_ui::app::AppHost`.
#[napi]
pub struct AppHost {
    host: CoreAppHost,
    callbacks: CallbackRegistry,
    callback_handles: CallbackHandles,
    window_handles: WindowHandles,
    /// Last-seen window states, used to derive window lifecycle events in
    /// `drain_window_events`. Methods that mutate window state on behalf of JS
    /// patch this baseline immediately so they do not echo back as events.
    window_baseline: HashMap<WindowId, WindowState>,
}

#[napi]
impl AppHost {
    /// Create an AppHost. By default `step()` is non-blocking (`tickRate = 0`).
    #[napi(constructor)]
    pub fn new(config: Option<AppHostConfig>) -> napi::Result<Self> {
        register_all_runtime_components();

        let config = config.unwrap_or_default();
        let callbacks = CallbackRegistry::new();
        let host = if config.headless() {
            match CoreAppHost::new_headless(
                TuiRect::new(0, 0, config.cols(), config.rows()),
                build_empty_desktop,
            ) {
                Ok(host) => {
                    atto_ui::reactive::set_global_tick_rate(config.tick_rate());
                    Ok(host)
                }
                Err(err) => Err(err),
            }
        } else {
            let crossterm_config = CrosstermAppConfig::default()
                .tick_rate(config.tick_rate())
                .mouse_capture(config.mouse_capture())
                .cursor(config.cursor())
                .bracketed_paste(config.bracketed_paste())
                .keyboard_enhancement(config.keyboard_enhancement());
            CoreAppHost::new(crossterm_config, build_empty_desktop)
        }
        .map_err(error::anyhow_error)?;

        Ok(Self {
            host,
            callbacks,
            callback_handles: CallbackHandles::new(),
            window_handles: WindowHandles::new(),
            window_baseline: HashMap::new(),
        })
    }

    /// Add a dynamic runtime window and return its opaque string handle.
    #[napi]
    pub fn add_dynamic_window(
        &mut self,
        title: String,
        #[napi(ts_arg_type = "Rect | [number, number, number, number]")] rect: Value,
        root: Value,
    ) -> napi::Result<String> {
        let root = component_spec_from_json(root, &self.callback_handles)?;
        let rect = rect_from_json(rect)?;
        let screen = self.host.screen().map_err(error::anyhow_error)?;
        let id = self
            .host
            .desktop()
            .add_dynamic_window(
                WindowKind::Normal,
                title,
                rect,
                root,
                self.callbacks.clone(),
                screen,
            )
            .map_err(error::tree_error)?;
        self.window_baseline.insert(id, WindowState::Normal);
        Ok(self.window_handles.handle_for(id))
    }

    /// Apply one TreeOp object or an array of TreeOp objects to a dynamic window.
    #[napi]
    pub fn apply_tree_ops(&mut self, window_id: String, ops: Value) -> napi::Result<bool> {
        let window_id = self.resolve_window(&window_id)?;
        let ops = tree_ops_from_json(ops, &self.callback_handles)?;
        self.host
            .desktop()
            .apply_tree_ops(window_id, &ops)
            .map_err(error::tree_error)
    }

    /// Advance the host by one frame. Returns false when the host requests exit.
    #[napi]
    pub fn step(&mut self) -> napi::Result<bool> {
        match self.host.step().map_err(error::anyhow_error)? {
            AppControl::Continue => Ok(true),
            AppControl::Exit => Ok(false),
        }
    }

    /// Restore terminal state for real-terminal hosts. Idempotent and a no-op for headless hosts.
    #[napi]
    pub fn dispose(&mut self) {
        self.host.restore_terminal();
    }

    /// Drain queued UI callback invocations.
    #[napi]
    pub fn drain_callbacks(&mut self) -> napi::Result<Value> {
        let callbacks = self.callbacks.drain();
        let mut out = Vec::new();
        for event in &callbacks {
            if !self.callback_handles.contains_id(event.callback_id) {
                continue;
            }
            out.push(callback_invocation_to_json(
                event,
                &mut self.callback_handles,
            )?);
        }
        Ok(Value::Array(out))
    }

    /// Allocate a callback id handle for use in component event props.
    #[napi]
    pub fn alloc_callback(&mut self) -> String {
        let id = self.callbacks.register();
        self.callback_handles.handle_for(id)
    }

    /// Release a callback id handle after its component event binding is removed.
    #[napi]
    pub fn release_callback(&mut self, callback_id: String) -> napi::Result<bool> {
        self.callback_handles.release_handle(&callback_id)?;
        Ok(true)
    }

    /// Send an input event directly to one window.
    #[napi]
    pub fn send_event(&mut self, window_id: String, event: Value) -> napi::Result<Value> {
        let window_id = self.resolve_window(&window_id)?;
        let event = event::event_from_json(event)?;
        let result = self
            .host
            .send_event(window_id, event)
            .map_err(error::anyhow_error)?;
        let json = desktop_event_result_to_json(&result, &mut self.window_handles);
        if let DesktopAction::CloseWindow(id) = result.action {
            self.window_handles.release(id);
            self.window_baseline.remove(&id);
        }
        Ok(json)
    }

    /// Close a window and invalidate its handle when successful.
    #[napi]
    pub fn close_window(&mut self, window_id: String) -> napi::Result<bool> {
        let window_id = self.resolve_window(&window_id)?;
        let closed = self.host.close_window(window_id);
        if closed {
            self.window_handles.release(window_id);
            self.window_baseline.remove(&window_id);
        }
        Ok(closed)
    }

    /// Minimize a window by handle. Patches the baseline so the change is not
    /// echoed back through `drain_window_events`.
    #[napi]
    pub fn minimize_window(&mut self, window_id: String) -> napi::Result<bool> {
        let window_id = self.resolve_window(&window_id)?;
        let ok = self.host.minimize_window(window_id);
        if ok {
            self.window_baseline
                .insert(window_id, WindowState::Minimized);
        }
        Ok(ok)
    }

    /// Restore a minimized window by handle.
    #[napi]
    pub fn restore_window(&mut self, window_id: String) -> napi::Result<bool> {
        let window_id = self.resolve_window(&window_id)?;
        let ok = self.host.restore_window(window_id);
        if ok {
            self.window_baseline.insert(window_id, WindowState::Normal);
        }
        Ok(ok)
    }

    /// Toggle maximize for a window by handle. Returns true when the state changed.
    #[napi]
    pub fn maximize_window(&mut self, window_id: String) -> napi::Result<bool> {
        let window_id = self.resolve_window(&window_id)?;
        let changed = self
            .host
            .maximize_window(window_id)
            .map_err(error::anyhow_error)?;
        if changed
            && let Some(state) = self
                .host
                .list_windows()
                .iter()
                .find(|w| w.id == window_id)
                .map(|w| w.state)
        {
            self.window_baseline.insert(window_id, state);
        }
        Ok(changed)
    }

    /// Drain window lifecycle events (close/minimize/maximize/restore) that
    /// originated from user interaction inside the TUI. Returns `[]` when nothing
    /// changed. Events are derived by diffing the current window list against the
    /// baseline; JS-initiated mutations patch the baseline and so never appear here.
    #[napi]
    pub fn drain_window_events(&mut self) -> napi::Result<Value> {
        let current: HashMap<WindowId, WindowState> = self
            .host
            .list_windows()
            .iter()
            .map(|w| (w.id, w.state))
            .collect();

        let mut events = Vec::new();

        let closed_ids: Vec<WindowId> = self
            .window_baseline
            .keys()
            .filter(|id| !current.contains_key(id))
            .copied()
            .collect();
        for id in &closed_ids {
            // Generate the handle string while the mapping is still valid; the
            // release happens only after every closed event has been emitted.
            let handle = self.window_handles.handle_for(*id);
            events.push(window_event_json("closed", handle, None));
        }

        for (id, state) in &current {
            if let Some(prev) = self.window_baseline.get(id)
                && prev != state
            {
                let kind = match state {
                    WindowState::Minimized => "minimized",
                    WindowState::Maximized => "maximized",
                    WindowState::Normal => "restored",
                };
                let handle = self.window_handles.handle_for(*id);
                events.push(window_event_json(kind, handle, Some(*state)));
            }
        }

        self.window_baseline = current;
        for id in &closed_ids {
            self.window_handles.release(*id);
        }

        Ok(Value::Array(events))
    }

    /// Focus a window by handle.
    #[napi]
    pub fn focus_window(&mut self, window_id: String) -> napi::Result<bool> {
        let window_id = self.resolve_window(&window_id)?;
        Ok(self.host.focus_window(window_id))
    }

    /// Move a window to an absolute work-area position.
    #[napi]
    pub fn move_window(&mut self, window_id: String, x: u16, y: u16) -> napi::Result<bool> {
        let window_id = self.resolve_window(&window_id)?;
        self.host
            .move_window(window_id, x, y)
            .map_err(error::anyhow_error)
    }

    /// Resize a window.
    #[napi]
    pub fn resize_window(
        &mut self,
        window_id: String,
        width: u16,
        height: u16,
    ) -> napi::Result<bool> {
        let window_id = self.resolve_window(&window_id)?;
        self.host
            .resize_window(window_id, width, height)
            .map_err(error::anyhow_error)
    }

    /// List known windows using opaque string handles.
    #[napi]
    pub fn list_windows(&mut self) -> Value {
        Value::Array(
            self.host
                .list_windows()
                .iter()
                .map(|window| window_info_to_json(window, &mut self.window_handles))
                .collect(),
        )
    }

    /// Set a window title.
    #[napi]
    pub fn set_title(&mut self, window_id: String, title: String) -> napi::Result<bool> {
        let window_id = self.resolve_window(&window_id)?;
        Ok(self.host.set_title(window_id, title))
    }

    /// Replace the desktop menu bar from a JavaScript menu spec.
    #[napi]
    pub fn set_menu_bar(&mut self, spec: Value) -> napi::Result<()> {
        self.host.desktop().menu =
            menu_bar_from_json(spec, self.callbacks.clone(), &self.callback_handles)?;
        Ok(())
    }

    /// Replace or clear custom desktop status bar text.
    #[napi]
    pub fn set_status_bar(&mut self, left: Option<String>, right: Option<String>) {
        match (left, right) {
            (None, None) => self.host.desktop().status.clear_custom(),
            (left, right) => self
                .host
                .desktop()
                .status
                .set_custom(left.unwrap_or_default(), right.unwrap_or_default()),
        }
    }

    /// Set a component property by component id.
    #[napi]
    pub fn set_property(&mut self, id: String, name: String, value: Value) -> napi::Result<()> {
        let value = component_value_from_json(value)?;
        self.host
            .set_property(id, name, value)
            .map_err(error::tree_error)
    }

    /// Read a component property by component id.
    #[napi]
    pub fn get_property(&mut self, id: String, name: String) -> napi::Result<Value> {
        let value = self
            .host
            .get_property(&id, &name)
            .map_err(error::component_error)?;
        component_value_to_json(&value)
    }

    /// Export a deterministic snapshot of the desktop tree.
    #[napi]
    pub fn snapshot(&mut self) -> napi::Result<Value> {
        let snapshot = self.host.snapshot().map_err(error::anyhow_error)?;
        desktop_snapshot_to_json(&snapshot, &mut self.window_handles)
    }

    /// Set the active theme to `dark`, `light`, or `turbo`.
    #[napi]
    pub fn set_theme(&mut self, name: String) -> napi::Result<()> {
        self.host.desktop().theme = theme_by_name(&name)?;
        Ok(())
    }

    /// Load a theme from disk, using `base` (`dark`, `light`, or `turbo`) as fallback.
    #[napi]
    pub fn load_theme(&mut self, path: String, base: Option<String>) -> napi::Result<()> {
        let base = theme_by_name(base.as_deref().unwrap_or("dark"))?;
        self.host.desktop().theme =
            Theme::load_from_path_with_base(path, base).map_err(error::anyhow_error)?;
        Ok(())
    }

    /// Return registered runtime component schemas.
    #[napi]
    pub fn schemas(&mut self) -> napi::Result<Value> {
        let registry = global_registry(self.callbacks.clone());
        registry
            .schemas()
            .map(component_schema_to_json)
            .collect::<napi::Result<Vec<_>>>()
            .map(Value::Array)
    }
}

/// Return the native package version exposed to JavaScript smoke tests.
#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Register optional runtime components from workspace companion crates.
#[napi]
pub fn register_all_runtime_components() {
    atto_ui_components::register_all_runtime_components();
}

fn build_empty_desktop(_screen: TuiRect) -> anyhow::Result<Desktop> {
    Ok(Desktop::new(Theme::dark(), MenuBar::new(vec![])))
}

fn menu_bar_from_json(
    value: Value,
    callbacks: CallbackRegistry,
    handles: &CallbackHandles,
) -> napi::Result<MenuBar> {
    let object = value
        .as_object()
        .ok_or_else(|| error::invalid_arg("menu bar spec must be an object"))?;
    let menus = optional_array_field(object, "menus")?
        .unwrap_or(&[])
        .iter()
        .map(|menu| menu_spec_from_json(menu, callbacks.clone(), handles))
        .collect::<napi::Result<Vec<_>>>()?;
    Ok(MenuBar::new(menus))
}

fn menu_spec_from_json(
    value: &Value,
    callbacks: CallbackRegistry,
    handles: &CallbackHandles,
) -> napi::Result<MenuSpec> {
    let object = value
        .as_object()
        .ok_or_else(|| error::invalid_arg("menu spec must be an object"))?;
    let title = required_string_field(object, "title")?;
    let items = optional_array_field(object, "items")?
        .unwrap_or(&[])
        .iter()
        .map(|item| menu_item_from_json(item, callbacks.clone(), handles))
        .collect::<napi::Result<Vec<_>>>()?;

    Ok(MenuSpec {
        tag: optional_string_field(object, "id")?,
        title: title.into(),
        items,
    })
}

fn menu_item_from_json(
    value: &Value,
    callbacks: CallbackRegistry,
    handles: &CallbackHandles,
) -> napi::Result<MenuItem> {
    let object = value
        .as_object()
        .ok_or_else(|| error::invalid_arg("menu item spec must be an object"))?;
    let tag = optional_string_field(object, "id")?;
    let label = required_string_field(object, "label")?;
    let shortcut = optional_string_field(object, "shortcut")?;
    let accelerator = optional_string_field(object, "accelerator")?.or_else(|| shortcut.clone());
    let mnemonic = optional_string_field(object, "mnemonic")?
        .map(|value| {
            let mut chars = value.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => Ok(ch),
                _ => Err(error::invalid_arg(
                    "field mnemonic must be a single character",
                )),
            }
        })
        .transpose()?
        .or_else(|| {
            shortcut.as_ref().and_then(|shortcut| {
                let mut chars = shortcut.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => Some(ch),
                    _ => None,
                }
            })
        });
    let enabled = optional_bool_field(object, "enabled")?.unwrap_or(true);
    let submenu = optional_array_field(object, "items")?
        .unwrap_or(&[])
        .iter()
        .map(|item| menu_item_from_json(item, callbacks.clone(), handles))
        .collect::<napi::Result<Vec<_>>>()?;
    let callback = optional_string_field(object, "callback")?
        .map(|handle| handles.resolve(&handle))
        .transpose()?;
    let target_id = tag.clone();
    let on_activate: Option<std::sync::Arc<dyn Fn() + Send + Sync>> = callback.map(|callback_id| {
        let callbacks = callbacks.clone();
        std::sync::Arc::new(move || {
            callbacks.emit(CallbackInvocation {
                callback_id,
                target_id: target_id.clone(),
                event: "click".to_string(),
                payload: None,
            });
        }) as std::sync::Arc<dyn Fn() + Send + Sync>
    });

    Ok(MenuItem {
        tag,
        label: label.into(),
        shortcut: shortcut.into(),
        accelerator: accelerator.into(),
        mnemonic: mnemonic.into(),
        enabled: enabled.into(),
        on_activate,
        submenu,
    })
}

fn required_string_field(object: &Map<String, Value>, name: &str) -> napi::Result<String> {
    optional_string_field(object, name)?
        .ok_or_else(|| error::invalid_arg(format!("missing string field {name}")))
}

fn optional_string_field(object: &Map<String, Value>, name: &str) -> napi::Result<Option<String>> {
    match object.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(error::invalid_arg(format!("field {name} must be a string"))),
    }
}

fn optional_bool_field(object: &Map<String, Value>, name: &str) -> napi::Result<Option<bool>> {
    match object.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(error::invalid_arg(format!(
            "field {name} must be a boolean"
        ))),
    }
}

fn optional_array_field<'a>(
    object: &'a Map<String, Value>,
    name: &str,
) -> napi::Result<Option<&'a [Value]>> {
    match object.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(values)) => Ok(Some(values.as_slice())),
        Some(_) => Err(error::invalid_arg(format!("field {name} must be an array"))),
    }
}

fn rect_from_json(value: Value) -> napi::Result<TuiRect> {
    match value {
        Value::Array(values) => rect_from_array(&values),
        Value::Object(object) => rect_from_object(&object),
        _ => Err(error::invalid_arg(
            "rect must be [x, y, width, height] or { x, y, width, height }",
        )),
    }
}

fn rect_from_array(values: &[Value]) -> napi::Result<TuiRect> {
    if values.len() != 4 {
        return Err(error::invalid_arg("rect array must have 4 elements"));
    }
    Ok(TuiRect::new(
        u16_from_json(&values[0], "rect.x")?,
        u16_from_json(&values[1], "rect.y")?,
        u16_from_json(&values[2], "rect.width")?,
        u16_from_json(&values[3], "rect.height")?,
    ))
}

fn rect_from_object(object: &Map<String, Value>) -> napi::Result<TuiRect> {
    Ok(TuiRect::new(
        u16_from_json(expect_rect_field(object, "x")?, "rect.x")?,
        u16_from_json(expect_rect_field(object, "y")?, "rect.y")?,
        u16_from_json(expect_rect_field(object, "width")?, "rect.width")?,
        u16_from_json(expect_rect_field(object, "height")?, "rect.height")?,
    ))
}

fn expect_rect_field<'a>(object: &'a Map<String, Value>, name: &str) -> napi::Result<&'a Value> {
    object
        .get(name)
        .ok_or_else(|| error::invalid_arg(format!("rect missing {name}")))
}

fn u16_from_json(value: &Value, context: &str) -> napi::Result<u16> {
    let Some(value) = value.as_f64() else {
        return Err(error::invalid_arg(format!("{context} must be a number")));
    };
    if !value.is_finite() || value.fract() != 0.0 || value < 0.0 || value > u16::MAX as f64 {
        return Err(error::invalid_arg(format!(
            "{context} must be an integer between 0 and {}",
            u16::MAX
        )));
    }
    Ok(value as u16)
}

fn rect_to_json(rect: RuntimeRect) -> Value {
    let mut object = Map::new();
    object.insert("x".to_string(), Value::Number(Number::from(rect.x)));
    object.insert("y".to_string(), Value::Number(Number::from(rect.y)));
    object.insert("width".to_string(), Value::Number(Number::from(rect.width)));
    object.insert(
        "height".to_string(),
        Value::Number(Number::from(rect.height)),
    );
    Value::Object(object)
}

fn tui_rect_to_json(rect: TuiRect) -> Value {
    rect_to_json(RuntimeRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    })
}

fn desktop_event_result_to_json(result: &DesktopEventResult, windows: &mut WindowHandles) -> Value {
    let mut object = Map::new();
    object.insert("consumed".to_string(), Value::Bool(result.is_consumed()));
    object.insert(
        "outcome".to_string(),
        Value::String(format!("{:?}", result.outcome)),
    );
    object.insert(
        "action".to_string(),
        match result.action {
            DesktopAction::None => Value::Null,
            DesktopAction::CloseWindow(id) => {
                let mut action = Map::new();
                action.insert(
                    "type".to_string(),
                    Value::String("close_window".to_string()),
                );
                action.insert(
                    "windowId".to_string(),
                    Value::String(windows.handle_for(id)),
                );
                Value::Object(action)
            }
        },
    );
    Value::Object(object)
}

fn window_info_to_json(window: &WindowInfo, windows: &mut WindowHandles) -> Value {
    let mut object = Map::new();
    object.insert(
        "id".to_string(),
        Value::String(windows.handle_for(window.id)),
    );
    object.insert(
        "tag".to_string(),
        window
            .tag
            .as_ref()
            .map(|tag| Value::String(tag.clone()))
            .unwrap_or(Value::Null),
    );
    object.insert("title".to_string(), Value::String(window.title.clone()));
    object.insert(
        "kind".to_string(),
        Value::String(format!("{:?}", window.kind)),
    );
    object.insert(
        "state".to_string(),
        Value::String(format!("{:?}", window.state)),
    );
    object.insert("rect".to_string(), tui_rect_to_json(window.rect));
    object.insert("isFocused".to_string(), Value::Bool(window.is_focused));
    Value::Object(object)
}

fn window_event_json(kind: &str, window_id: String, state: Option<WindowState>) -> Value {
    let mut object = Map::new();
    object.insert("type".to_string(), Value::String(kind.to_string()));
    object.insert("windowId".to_string(), Value::String(window_id));
    object.insert(
        "state".to_string(),
        state
            .map(|s| Value::String(format!("{s:?}")))
            .unwrap_or(Value::Null),
    );
    Value::Object(object)
}

fn desktop_snapshot_to_json(
    snapshot: &DesktopSnapshot,
    windows: &mut WindowHandles,
) -> napi::Result<Value> {
    let mut object = Map::new();
    object.insert("bounds".to_string(), rect_to_json(snapshot.bounds));
    object.insert(
        "tree".to_string(),
        desktop_snapshot_node_to_json(&snapshot.tree, windows)?,
    );
    Ok(Value::Object(object))
}

fn desktop_snapshot_node_to_json(
    node: &DesktopSnapshotNode,
    windows: &mut WindowHandles,
) -> napi::Result<Value> {
    let mut object = Map::new();
    object.insert(
        "kind".to_string(),
        Value::String(node_kind_to_string(node.kind).to_string()),
    );
    object.insert("id".to_string(), optional_string(node.id.as_ref()));
    object.insert("tag".to_string(), optional_string(node.tag.as_ref()));
    object.insert("name".to_string(), Value::String(node.name.clone()));
    object.insert(
        "typeName".to_string(),
        Value::String(node.type_name.clone()),
    );
    object.insert(
        "bounds".to_string(),
        node.bounds.map(rect_to_json).unwrap_or(Value::Null),
    );
    object.insert("text".to_string(), optional_string(node.text.as_ref()));
    object.insert("state".to_string(), optional_string(node.state.as_ref()));
    object.insert(
        "windowId".to_string(),
        node.window_id
            .map(|id| Value::String(windows.handle_for(WindowId::from_raw(id))))
            .unwrap_or(Value::Null),
    );
    object.insert(
        "properties".to_string(),
        value_map_to_json(&node.properties)?,
    );
    object.insert(
        "children".to_string(),
        Value::Array(
            node.children
                .iter()
                .map(|child| desktop_snapshot_node_to_json(child, windows))
                .collect::<napi::Result<Vec<_>>>()?,
        ),
    );
    Ok(Value::Object(object))
}

fn value_map_to_json(
    values: &std::collections::BTreeMap<String, atto_ui::ComponentValue>,
) -> napi::Result<Value> {
    let mut object = Map::new();
    for (key, value) in values {
        object.insert(key.clone(), component_value_to_json(value)?);
    }
    Ok(Value::Object(object))
}

fn optional_string(value: Option<&String>) -> Value {
    value.cloned().map(Value::String).unwrap_or(Value::Null)
}

fn node_kind_to_string(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Desktop => "desktop",
        NodeKind::MenuBar => "menu_bar",
        NodeKind::Menu => "menu",
        NodeKind::MenuItem => "menu_item",
        NodeKind::StatusBar => "status_bar",
        NodeKind::Window => "window",
        NodeKind::Component => "component",
    }
}

fn theme_by_name(name: &str) -> napi::Result<Theme> {
    Theme::named(name).map_err(|err| error::invalid_arg(err.to_string()))
}

impl AppHost {
    fn resolve_window(&self, handle: &str) -> napi::Result<WindowId> {
        self.window_handles.resolve(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn version_matches_crate_version() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn headless_host_drives_tree_ops_and_callbacks() {
        let mut host = AppHost::new(Some(AppHostConfig {
            headless: Some(true),
            cols: Some(40),
            rows: Some(12),
            ..AppHostConfig::default()
        }))
        .unwrap();

        let callback = host.alloc_callback();
        let window = host
            .add_dynamic_window(
                "Smoke".to_string(),
                json!({ "x": 1, "y": 1, "width": 24, "height": 8 }),
                json!({
                    "type": "VStack",
                    "id": "root",
                    "children": [
                        { "type": "Label", "id": "title", "props": { "text": "Before" } },
                        { "type": "Button", "id": "ok", "props": { "label": "OK" }, "events": { "click": callback } }
                    ]
                }),
            )
            .unwrap();

        assert!(host.step().unwrap());
        assert!(
            !host
                .apply_tree_ops(
                    window.clone(),
                    json!([{ "op": "set_prop", "id": "title", "name": "text", "value": "After" }])
                )
                .unwrap()
        );

        let snapshot = host.snapshot().unwrap();
        let title = find_snapshot_node(&snapshot["tree"], "title").unwrap();
        assert_eq!(title["text"], json!("After"));

        let result = host
            .send_event(window, json!({ "type": "key", "key": "enter" }))
            .unwrap();
        assert_eq!(result["consumed"], json!(true));

        let callbacks = host.drain_callbacks().unwrap();
        assert_eq!(callbacks[0]["callbackId"], json!(callback));
        assert_eq!(callbacks[0]["targetId"], json!("ok"));
        assert_eq!(callbacks[0]["event"], json!("click"));
    }

    #[test]
    fn released_callback_handles_are_not_drained() {
        let mut host = AppHost::new(Some(AppHostConfig {
            headless: Some(true),
            cols: Some(40),
            rows: Some(12),
            ..AppHostConfig::default()
        }))
        .unwrap();

        let callback = host.alloc_callback();
        let window = host
            .add_dynamic_window(
                "Release".to_string(),
                json!({ "x": 1, "y": 1, "width": 24, "height": 8 }),
                json!({
                    "type": "Button",
                    "id": "button",
                    "props": { "label": "OK" },
                    "events": { "click": callback.clone() }
                }),
            )
            .unwrap();

        host.send_event(window, json!({ "type": "key", "key": "enter" }))
            .unwrap();
        assert!(host.release_callback(callback).unwrap());
        assert_eq!(host.drain_callbacks().unwrap(), json!([]));
    }

    #[test]
    fn rect_decoder_accepts_object_and_array_shapes() {
        assert_eq!(
            rect_from_json(json!({ "x": 1, "y": 2, "width": 3, "height": 4 })).unwrap(),
            TuiRect::new(1, 2, 3, 4)
        );
        assert_eq!(
            rect_from_json(json!([5, 6, 7, 8])).unwrap(),
            TuiRect::new(5, 6, 7, 8)
        );
    }

    #[test]
    fn rect_decoder_rejects_invalid_values() {
        assert!(rect_from_json(json!([1, 2, 3])).is_err());
        assert!(rect_from_json(json!({ "x": 1, "y": 2, "width": 3 })).is_err());
        assert!(rect_from_json(json!([1, 2, -3, 4])).is_err());
        assert!(rect_from_json(json!([1, 2, 3.5, 4])).is_err());
    }

    fn find_snapshot_node<'a>(node: &'a Value, id: &str) -> Option<&'a Value> {
        if node["id"] == id {
            return Some(node);
        }
        node["children"]
            .as_array()?
            .iter()
            .find_map(|child| find_snapshot_node(child, id))
    }
}
