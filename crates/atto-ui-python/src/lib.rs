#![forbid(unsafe_code)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::collections::BTreeMap;
use std::time::Duration;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyByteArray, PyBytes, PyDict, PyList, PyString, PyTuple};
use pyo3::{Bound, IntoPyObjectExt, Py};
use ratatui::layout::Rect;

use ::atto_ui as atto_ui_crate;
use atto_ui_components::register_all_runtime_components as register_all_components;
use atto_ui_crate::app::{
    AppControl, AppHost, CrosstermAppConfig, CursorMode, Desktop, DesktopAction,
    DesktopEventResult, MenuBar, WindowInfo,
};
use atto_ui_crate::runtime::{
    AlignSpec, AnchorPlacementSpec, AnchorSpec, EdgeInsetsSpec, LayoutSpec, Rect as RuntimeRect,
    SizeSpec, global_registry,
};
use atto_ui_crate::theme::Theme;
use atto_ui_crate::wm::{WindowId, WindowKind};
use atto_ui_crate::{
    ActionMeta, CallbackId, CallbackInvocation, CallbackRegistry, ComponentSchema, ComponentSpec,
    ComponentSpecChild, ComponentValue, DesktopSnapshot, DesktopSnapshotNode, EventMeta, NodeKind,
    PropertyMeta, TreeOp, ValueType,
};

type PyObject = Py<PyAny>;

#[pyclass(name = "AppHost", unsendable)]
struct PyAppHost {
    host: AppHost,
    callbacks: CallbackRegistry,
}

#[pymethods]
impl PyAppHost {
    #[new]
    #[pyo3(signature = (cols = 80, rows = 24, headless = true))]
    fn new(cols: u16, rows: u16, headless: bool) -> PyResult<Self> {
        register_all_components();

        let callbacks = CallbackRegistry::new();
        let host = if headless {
            AppHost::new_headless(Rect::new(0, 0, cols, rows), build_empty_desktop)
                .map_err(to_py_err)?
        } else {
            let config = CrosstermAppConfig::default()
                .tick_rate(Duration::from_millis(16))
                .mouse_capture(true)
                .cursor(CursorMode::Hide);
            AppHost::new(config, build_empty_desktop).map_err(to_py_err)?
        };

        Ok(Self { host, callbacks })
    }

    fn add_dynamic_window(
        &mut self,
        title: String,
        rect: &Bound<'_, PyAny>,
        root: &Bound<'_, PyAny>,
    ) -> PyResult<u64> {
        let root = py_to_component_spec(root)?;
        let rect = py_to_rect(rect)?;
        let screen = self.host.screen().map_err(to_py_err)?;
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
            .map_err(to_py_err)?;
        Ok(id.raw())
    }

    fn apply_tree_ops(&mut self, window_id: u64, ops: &Bound<'_, PyAny>) -> PyResult<bool> {
        let ops = py_to_tree_ops(ops)?;
        self.host
            .desktop()
            .apply_tree_ops(WindowId::from_raw(window_id), &ops)
            .map_err(to_py_err)
    }

    fn step(&mut self) -> PyResult<bool> {
        match self.host.step().map_err(to_py_err)? {
            AppControl::Continue => Ok(true),
            AppControl::Exit => Ok(false),
        }
    }

    fn run(&mut self) -> PyResult<()> {
        self.host.run().map_err(to_py_err)
    }

    fn drain_callbacks(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        let events = self.callbacks.drain();
        let list = PyList::empty(py);
        for event in events {
            list.append(callback_invocation_to_py(py, &event)?)?;
        }
        Ok(list.into_any().unbind())
    }

    fn send_event(
        &mut self,
        py: Python<'_>,
        window_id: u64,
        event: &Bound<'_, PyAny>,
    ) -> PyResult<PyObject> {
        let event = py_to_event(event)?;
        let result = self
            .host
            .send_event(WindowId::from_raw(window_id), event)
            .map_err(to_py_err)?;
        desktop_event_result_to_py(py, &result)
    }

    fn close_window(&mut self, window_id: u64) -> bool {
        self.host.close_window(WindowId::from_raw(window_id))
    }

    fn focus_window(&mut self, window_id: u64) -> bool {
        self.host.focus_window(WindowId::from_raw(window_id))
    }

    fn move_window(&mut self, window_id: u64, x: u16, y: u16) -> PyResult<bool> {
        self.host
            .move_window(WindowId::from_raw(window_id), x, y)
            .map_err(to_py_err)
    }

    fn resize_window(&mut self, window_id: u64, width: u16, height: u16) -> PyResult<bool> {
        self.host
            .resize_window(WindowId::from_raw(window_id), width, height)
            .map_err(to_py_err)
    }

    fn list_windows(&self, py: Python<'_>) -> PyResult<PyObject> {
        let list = PyList::empty(py);
        for window in self.host.list_windows() {
            list.append(window_info_to_py(py, &window)?)?;
        }
        Ok(list.into_any().unbind())
    }

    fn set_title(&mut self, window_id: u64, title: String) -> bool {
        self.host.set_title(WindowId::from_raw(window_id), title)
    }

    fn set_property(&mut self, id: String, name: String, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = py_to_component_value(value)?;
        self.host.set_property(id, name, value).map_err(to_py_err)
    }

    fn get_property(&mut self, py: Python<'_>, id: String, name: String) -> PyResult<PyObject> {
        let value = self
            .host
            .get_property(&id, &name)
            .map_err(|err| to_py_err(format!("{err:?}")))?;
        component_value_to_py(py, &value)
    }

    fn snapshot(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        let snapshot = self.host.snapshot().map_err(to_py_err)?;
        desktop_snapshot_to_py(py, &snapshot)
    }

    fn set_theme(&mut self, name: String) -> PyResult<()> {
        self.host.desktop().theme = theme_by_name(&name)?;
        Ok(())
    }

    #[pyo3(signature = (path, base = "dark".to_string()))]
    fn load_theme(&mut self, path: String, base: String) -> PyResult<()> {
        let base = theme_by_name(&base)?;
        self.host.desktop().theme =
            Theme::load_from_path_with_base(path, base).map_err(to_py_err)?;
        Ok(())
    }

    fn schemas(&self, py: Python<'_>) -> PyResult<PyObject> {
        let registry = global_registry(self.callbacks.clone());
        let list = PyList::empty(py);
        for schema in registry.schemas() {
            list.append(component_schema_to_py(py, schema)?)?;
        }
        Ok(list.into_any().unbind())
    }
}

#[pymodule]
fn _native(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyAppHost>()?;
    m.add_function(wrap_pyfunction!(py_register_all_runtime_components, m)?)?;
    Ok(())
}

#[pyfunction(name = "register_all_runtime_components")]
fn py_register_all_runtime_components() {
    register_all_components();
}

fn to_py_err<E: std::fmt::Display>(err: E) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(err.to_string())
}

fn build_empty_desktop(_screen: Rect) -> anyhow::Result<Desktop> {
    let theme = Theme::dark();
    let menu = MenuBar::new(vec![]);
    Ok(Desktop::new(theme, menu))
}

fn theme_by_name(name: &str) -> PyResult<Theme> {
    match normalize_name(name).as_str() {
        "dark" => Ok(Theme::dark()),
        "light" => Ok(Theme::light()),
        _ => Err(to_py_err(format!(
            "unknown theme {name:?}; expected 'dark' or 'light'"
        ))),
    }
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '_' | '-' | ' '))
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn py_to_rect(obj: &Bound<'_, PyAny>) -> PyResult<Rect> {
    if let Ok(tuple) = obj.cast::<PyTuple>() {
        if tuple.len() != 4 {
            return Err(to_py_err("rect tuple must have 4 elements"));
        }
        let x = py_to_u16(&tuple.get_item(0)?, "rect.x")?;
        let y = py_to_u16(&tuple.get_item(1)?, "rect.y")?;
        let width = py_to_u16(&tuple.get_item(2)?, "rect.width")?;
        let height = py_to_u16(&tuple.get_item(3)?, "rect.height")?;
        return Ok(Rect::new(x, y, width, height));
    }

    if let Ok(list) = obj.cast::<PyList>() {
        if list.len() != 4 {
            return Err(to_py_err("rect list must have 4 elements"));
        }
        let x = py_to_u16(&list.get_item(0)?, "rect.x")?;
        let y = py_to_u16(&list.get_item(1)?, "rect.y")?;
        let width = py_to_u16(&list.get_item(2)?, "rect.width")?;
        let height = py_to_u16(&list.get_item(3)?, "rect.height")?;
        return Ok(Rect::new(x, y, width, height));
    }

    if let Ok(dict) = obj.cast::<PyDict>() {
        let x = py_to_u16(&expect_key(dict, "x")?, "rect.x")?;
        let y = py_to_u16(&expect_key(dict, "y")?, "rect.y")?;
        let width = py_to_u16(&expect_key(dict, "width")?, "rect.width")?;
        let height = py_to_u16(&expect_key(dict, "height")?, "rect.height")?;
        return Ok(Rect::new(x, y, width, height));
    }

    Err(to_py_err("rect must be tuple/list/dict"))
}

fn py_to_event(obj: &Bound<'_, PyAny>) -> PyResult<Event> {
    if let Ok(value) = obj.extract::<String>() {
        return Ok(Event::Key(KeyEvent::new(
            py_to_key_code_name(&value)?,
            KeyModifiers::NONE,
        )));
    }

    let dict = obj
        .cast::<PyDict>()
        .map_err(|_| to_py_err("event must be string or dict"))?;
    let kind = expect_string_key(dict, "type").or_else(|_| expect_string_key(dict, "event"))?;
    match normalize_name(&kind).as_str() {
        "key" => py_to_key_event(dict).map(Event::Key),
        "mouse" => py_to_mouse_event(dict).map(Event::Mouse),
        "paste" => {
            let text = expect_string_key(dict, "text")?;
            Ok(Event::Paste(text))
        }
        "resize" => {
            let cols = py_to_u16(&expect_key(dict, "cols")?, "cols")?;
            let rows = py_to_u16(&expect_key(dict, "rows")?, "rows")?;
            Ok(Event::Resize(cols, rows))
        }
        "focusgained" => Ok(Event::FocusGained),
        "focuslost" => Ok(Event::FocusLost),
        _ => Err(to_py_err(format!("unknown event type: {kind}"))),
    }
}

fn py_to_key_event(dict: &Bound<'_, PyDict>) -> PyResult<KeyEvent> {
    let code = if let Some(value) = dict_get(dict, "char")? {
        py_to_key_char(&value)?
    } else {
        let key = expect_string_key(dict, "key")?;
        py_to_key_code_name(&key)?
    };
    let modifiers = dict_get(dict, "modifiers")?
        .map(|value| py_to_key_modifiers(&value))
        .transpose()?
        .unwrap_or(KeyModifiers::NONE);
    let kind = dict_get(dict, "kind")?
        .map(|value| py_to_key_event_kind(&value))
        .transpose()?
        .unwrap_or(KeyEventKind::Press);
    Ok(KeyEvent {
        code,
        modifiers,
        kind,
        state: KeyEventState::empty(),
    })
}

fn py_to_key_char(obj: &Bound<'_, PyAny>) -> PyResult<KeyCode> {
    let value: String = obj.extract()?;
    let mut chars = value.chars();
    let Some(ch) = chars.next() else {
        return Err(to_py_err("key char must not be empty"));
    };
    if chars.next().is_some() {
        return Err(to_py_err("key char must contain exactly one character"));
    }
    Ok(KeyCode::Char(ch))
}

fn py_to_key_code_name(name: &str) -> PyResult<KeyCode> {
    let normalized = normalize_name(name);
    match normalized.as_str() {
        "backspace" => Ok(KeyCode::Backspace),
        "enter" | "return" => Ok(KeyCode::Enter),
        "left" => Ok(KeyCode::Left),
        "right" => Ok(KeyCode::Right),
        "up" => Ok(KeyCode::Up),
        "down" => Ok(KeyCode::Down),
        "home" => Ok(KeyCode::Home),
        "end" => Ok(KeyCode::End),
        "pageup" => Ok(KeyCode::PageUp),
        "pagedown" => Ok(KeyCode::PageDown),
        "tab" => Ok(KeyCode::Tab),
        "backtab" => Ok(KeyCode::BackTab),
        "delete" | "del" => Ok(KeyCode::Delete),
        "insert" | "ins" => Ok(KeyCode::Insert),
        "esc" | "escape" => Ok(KeyCode::Esc),
        value if value.starts_with('f') => {
            let n = value[1..]
                .parse::<u8>()
                .map_err(|_| to_py_err(format!("invalid function key: {name}")))?;
            Ok(KeyCode::F(n))
        }
        value => {
            let mut chars = value.chars();
            if let Some(ch) = chars.next()
                && chars.next().is_none()
            {
                return Ok(KeyCode::Char(ch));
            }
            Err(to_py_err(format!("unknown key: {name}")))
        }
    }
}

fn py_to_key_event_kind(obj: &Bound<'_, PyAny>) -> PyResult<KeyEventKind> {
    let value: String = obj.extract()?;
    match normalize_name(&value).as_str() {
        "press" | "down" => Ok(KeyEventKind::Press),
        "release" | "up" => Ok(KeyEventKind::Release),
        "repeat" => Ok(KeyEventKind::Repeat),
        _ => Err(to_py_err("invalid key event kind")),
    }
}

fn py_to_mouse_event(dict: &Bound<'_, PyDict>) -> PyResult<MouseEvent> {
    let kind_name = expect_string_key(dict, "kind")?;
    let kind = py_to_mouse_event_kind(&kind_name, dict)?;
    let column = if let Some(value) = dict_get(dict, "column")? {
        py_to_u16(&value, "column")?
    } else {
        py_to_u16(&expect_key(dict, "x")?, "x")?
    };
    let row = if let Some(value) = dict_get(dict, "row")? {
        py_to_u16(&value, "row")?
    } else {
        py_to_u16(&expect_key(dict, "y")?, "y")?
    };
    let modifiers = dict_get(dict, "modifiers")?
        .map(|value| py_to_key_modifiers(&value))
        .transpose()?
        .unwrap_or(KeyModifiers::NONE);
    Ok(MouseEvent {
        kind,
        column,
        row,
        modifiers,
    })
}

fn py_to_mouse_event_kind(name: &str, dict: &Bound<'_, PyDict>) -> PyResult<MouseEventKind> {
    match normalize_name(name).as_str() {
        "down" => Ok(MouseEventKind::Down(py_to_mouse_button_from_dict(dict)?)),
        "up" => Ok(MouseEventKind::Up(py_to_mouse_button_from_dict(dict)?)),
        "drag" => Ok(MouseEventKind::Drag(py_to_mouse_button_from_dict(dict)?)),
        "move" | "moved" => Ok(MouseEventKind::Moved),
        "scrollup" => Ok(MouseEventKind::ScrollUp),
        "scrolldown" => Ok(MouseEventKind::ScrollDown),
        "scrollleft" => Ok(MouseEventKind::ScrollLeft),
        "scrollright" => Ok(MouseEventKind::ScrollRight),
        _ => Err(to_py_err(format!("unknown mouse event kind: {name}"))),
    }
}

fn py_to_mouse_button_from_dict(dict: &Bound<'_, PyDict>) -> PyResult<MouseButton> {
    let value = dict_get(dict, "button")?
        .map(|button| button.extract::<String>())
        .transpose()?
        .unwrap_or_else(|| "left".to_string());
    match normalize_name(&value).as_str() {
        "left" => Ok(MouseButton::Left),
        "right" => Ok(MouseButton::Right),
        "middle" => Ok(MouseButton::Middle),
        _ => Err(to_py_err(format!("unknown mouse button: {value}"))),
    }
}

fn py_to_key_modifiers(obj: &Bound<'_, PyAny>) -> PyResult<KeyModifiers> {
    if obj.is_none() {
        return Ok(KeyModifiers::NONE);
    }
    if let Ok(value) = obj.extract::<String>() {
        return key_modifiers_from_names(std::iter::once(value));
    }
    if let Ok(list) = obj.cast::<PyList>() {
        let mut names = Vec::with_capacity(list.len());
        for item in list.iter() {
            names.push(item.extract::<String>()?);
        }
        return key_modifiers_from_names(names);
    }
    if let Ok(tuple) = obj.cast::<PyTuple>() {
        let mut names = Vec::with_capacity(tuple.len());
        for item in tuple.iter() {
            names.push(item.extract::<String>()?);
        }
        return key_modifiers_from_names(names);
    }
    Err(to_py_err("modifiers must be string/list/tuple"))
}

fn key_modifiers_from_names<I>(names: I) -> PyResult<KeyModifiers>
where
    I: IntoIterator<Item = String>,
{
    let mut modifiers = KeyModifiers::NONE;
    for name in names {
        match normalize_name(&name).as_str() {
            "shift" => modifiers |= KeyModifiers::SHIFT,
            "control" | "ctrl" => modifiers |= KeyModifiers::CONTROL,
            "alt" | "option" => modifiers |= KeyModifiers::ALT,
            "super" | "cmd" | "command" => modifiers |= KeyModifiers::SUPER,
            "hyper" => modifiers |= KeyModifiers::HYPER,
            "meta" => modifiers |= KeyModifiers::META,
            "none" | "" => {}
            _ => return Err(to_py_err(format!("unknown modifier: {name}"))),
        }
    }
    Ok(modifiers)
}

fn py_to_tree_ops(obj: &Bound<'_, PyAny>) -> PyResult<Vec<TreeOp>> {
    if let Ok(list) = obj.cast::<PyList>() {
        let mut ops = Vec::with_capacity(list.len());
        for item in list.iter() {
            ops.push(py_to_tree_op(&item)?);
        }
        return Ok(ops);
    }

    if let Ok(tuple) = obj.cast::<PyTuple>() {
        let mut ops = Vec::with_capacity(tuple.len());
        for item in tuple.iter() {
            ops.push(py_to_tree_op(&item)?);
        }
        return Ok(ops);
    }

    Ok(vec![py_to_tree_op(obj)?])
}

fn py_to_tree_op(obj: &Bound<'_, PyAny>) -> PyResult<TreeOp> {
    let dict = obj
        .cast::<PyDict>()
        .map_err(|_| to_py_err("tree op must be dict"))?;

    let (op_name, payload) = extract_op_name(dict)?;
    let op = normalize_name(&op_name);
    let payload_dict = payload
        .as_ref()
        .and_then(|value| value.cast::<PyDict>().ok());
    let data = payload_dict.unwrap_or(dict);

    match op.as_str() {
        "settree" => {
            if let Some(value) = payload.as_ref() {
                let spec = py_to_component_spec(value)?;
                return Ok(TreeOp::SetTree(spec));
            }
            let mut candidate = dict_get(data, "tree")?;
            if candidate.is_none() {
                candidate = dict_get(data, "spec")?;
            }
            if candidate.is_none() {
                candidate = dict_get(data, "root")?;
            }
            let candidate = candidate.ok_or_else(|| to_py_err("set_tree requires 'tree'"))?;
            let spec = py_to_component_spec(&candidate)?;
            Ok(TreeOp::SetTree(spec))
        }
        "insert" => {
            let parent_id = expect_string_key(data, "parent_id")?;
            let index = py_to_usize(&expect_key(data, "index")?, "index")?;
            let child_obj =
                dict_get(data, "child")?.ok_or_else(|| to_py_err("insert requires 'child'"))?;
            let child = py_to_component_spec_child(&child_obj)?;
            Ok(TreeOp::Insert {
                parent_id,
                index,
                child,
            })
        }
        "remove" => {
            let id = expect_string_key(data, "id")?;
            Ok(TreeOp::Remove { id })
        }
        "replace" => {
            let id = expect_string_key(data, "id")?;
            let node_obj =
                dict_get(data, "node")?.ok_or_else(|| to_py_err("replace requires 'node'"))?;
            let node = py_to_component_spec_child(&node_obj)?;
            Ok(TreeOp::Replace { id, node })
        }
        "move" => {
            let id = expect_string_key(data, "id")?;
            let new_parent_id = expect_string_key(data, "new_parent_id")?;
            let index = py_to_usize(&expect_key(data, "index")?, "index")?;
            Ok(TreeOp::Move {
                id,
                new_parent_id,
                index,
            })
        }
        "setprop" => {
            let id = expect_string_key(data, "id")?;
            let name = expect_string_key(data, "name")?;
            let value_obj =
                dict_get(data, "value")?.ok_or_else(|| to_py_err("set_prop requires 'value'"))?;
            let value = py_to_component_value(&value_obj)?;
            Ok(TreeOp::SetProp { id, name, value })
        }
        "clearprop" => {
            let id = expect_string_key(data, "id")?;
            let name = expect_string_key(data, "name")?;
            Ok(TreeOp::ClearProp { id, name })
        }
        "bindevent" => {
            let id = expect_string_key(data, "id")?;
            let event = expect_string_key(data, "event")?;
            let callback_obj = dict_get(data, "callback")?
                .ok_or_else(|| to_py_err("bind_event requires 'callback'"))?;
            let callback = py_to_callback_id(&callback_obj)?;
            Ok(TreeOp::BindEvent {
                id,
                event,
                callback,
            })
        }
        "clearevent" => {
            let id = expect_string_key(data, "id")?;
            let event = expect_string_key(data, "event")?;
            Ok(TreeOp::ClearEvent { id, event })
        }
        _ => Err(to_py_err(format!("unknown tree op: {op_name}"))),
    }
}

fn extract_op_name<'py>(
    dict: &Bound<'py, PyDict>,
) -> PyResult<(String, Option<Bound<'py, PyAny>>)> {
    for key in ["op", "type", "kind"] {
        if let Some(value) = dict_get(dict, key)? {
            let name: String = value.extract()?;
            return Ok((name, None));
        }
    }

    if dict.len() == 1 {
        let (key, value) = dict
            .iter()
            .next()
            .ok_or_else(|| to_py_err("empty tree op dict"))?;
        let name: String = key.extract()?;
        return Ok((name, Some(value)));
    }

    Err(to_py_err("tree op requires 'op' key"))
}

fn py_to_component_spec(obj: &Bound<'_, PyAny>) -> PyResult<ComponentSpec> {
    let dict = obj
        .cast::<PyDict>()
        .map_err(|_| to_py_err("component spec must be dict"))?;

    let mut type_obj = dict_get(dict, "type")?;
    if type_obj.is_none() {
        type_obj = dict_get(dict, "type_name")?;
    }
    let type_name = type_obj
        .ok_or_else(|| to_py_err("component spec requires 'type'"))?
        .extract::<String>()?;

    let mut spec = ComponentSpec::new(type_name);
    if let Some(id_obj) = dict_get(dict, "id")?
        && !id_obj.is_none()
    {
        spec.id = Some(id_obj.extract()?);
    }

    if let Some(props_obj) = dict_get(dict, "props")? {
        let props_dict = props_obj
            .cast::<PyDict>()
            .map_err(|_| to_py_err("props must be dict"))?;
        spec.props = py_dict_to_value_map(props_dict)?;
    }

    if let Some(events_obj) = dict_get(dict, "events")? {
        let events_dict = events_obj
            .cast::<PyDict>()
            .map_err(|_| to_py_err("events must be dict"))?;
        spec.events = py_dict_to_event_map(events_dict)?;
    }

    if let Some(children_obj) = dict_get(dict, "children")? {
        let list = children_obj
            .cast::<PyList>()
            .map_err(|_| to_py_err("children must be list"))?;
        let mut children = Vec::with_capacity(list.len());
        for child_obj in list.iter() {
            children.push(py_to_component_spec_child(&child_obj)?);
        }
        spec.children = children;
    }

    Ok(spec)
}

fn py_to_component_spec_child(obj: &Bound<'_, PyAny>) -> PyResult<ComponentSpecChild> {
    if let Ok(dict) = obj.cast::<PyDict>() {
        let has_wrapper =
            dict.contains("node")? || dict.contains("layout")? || dict.contains("meta")?;
        if has_wrapper {
            let node = if let Some(node_obj) = dict_get(dict, "node")? {
                py_to_component_spec(&node_obj)?
            } else {
                py_to_component_spec(obj)?
            };
            let mut child = ComponentSpecChild::new(node);
            if let Some(layout_obj) = dict_get(dict, "layout")?
                && !layout_obj.is_none()
            {
                child.layout = Some(py_to_layout_spec(&layout_obj)?);
            }
            if let Some(meta_obj) = dict_get(dict, "meta")?
                && !meta_obj.is_none()
            {
                let meta_dict = meta_obj
                    .cast::<PyDict>()
                    .map_err(|_| to_py_err("meta must be dict"))?;
                child.meta = py_dict_to_value_map(meta_dict)?;
            }
            return Ok(child);
        }
    }

    Ok(ComponentSpecChild::new(py_to_component_spec(obj)?))
}

fn py_to_layout_spec(obj: &Bound<'_, PyAny>) -> PyResult<LayoutSpec> {
    let dict = obj
        .cast::<PyDict>()
        .map_err(|_| to_py_err("layout must be dict"))?;
    let mut layout = LayoutSpec::default();

    if let Some(value) = dict_get(dict, "width")?
        && !value.is_none()
    {
        layout.width = py_to_size_spec(&value)?;
    }
    if let Some(value) = dict_get(dict, "height")?
        && !value.is_none()
    {
        layout.height = py_to_size_spec(&value)?;
    }
    if let Some(value) = dict_get(dict, "margin")?
        && !value.is_none()
    {
        layout.margin = py_to_edge_insets_spec(&value)?;
    }
    if let Some(value) = dict_get(dict, "align_x")?
        && !value.is_none()
    {
        layout.align_x = py_to_align_spec(&value)?;
    }
    if let Some(value) = dict_get(dict, "align_y")?
        && !value.is_none()
    {
        layout.align_y = py_to_align_spec(&value)?;
    }
    if let Some(value) = dict_get(dict, "anchor")?
        && !value.is_none()
    {
        layout.anchor = Some(py_to_anchor_placement_spec(&value)?);
    }
    if let Some(value) = dict_get(dict, "tab_index")?
        && !value.is_none()
    {
        layout.tab_index = Some(py_to_i32(&value, "tab_index")?);
    }

    Ok(layout)
}

fn py_to_size_spec(obj: &Bound<'_, PyAny>) -> PyResult<SizeSpec> {
    if let Ok(value) = obj.extract::<String>() {
        let name = normalize_name(&value);
        return match name.as_str() {
            "fill" => Ok(SizeSpec::Fill),
            "content" => Ok(SizeSpec::Content),
            _ => Err(to_py_err("invalid size spec string")),
        };
    }

    if let Some(v) = py_to_u16_opt(obj)? {
        return Ok(SizeSpec::Fixed(v));
    }

    if let Ok(dict) = obj.cast::<PyDict>() {
        if let Some(value) = dict_get(dict, "fixed")? {
            let v = py_to_u16(&value, "fixed")?;
            return Ok(SizeSpec::Fixed(v));
        }
        if let Some(value) = dict_get(dict, "weight")? {
            let v = py_to_u16(&value, "weight")?;
            return Ok(SizeSpec::Weight(v));
        }
        if let Some(value) = dict_get(dict, "fill")?
            && value.extract::<bool>().unwrap_or(false)
        {
            return Ok(SizeSpec::Fill);
        }
        if let Some(value) = dict_get(dict, "content")?
            && value.extract::<bool>().unwrap_or(false)
        {
            return Ok(SizeSpec::Content);
        }
    }

    Err(to_py_err("invalid size spec"))
}

fn py_to_align_spec(obj: &Bound<'_, PyAny>) -> PyResult<AlignSpec> {
    let value: String = obj.extract()?;
    let name = normalize_name(&value);
    match name.as_str() {
        "start" => Ok(AlignSpec::Start),
        "center" => Ok(AlignSpec::Center),
        "end" => Ok(AlignSpec::End),
        "stretch" => Ok(AlignSpec::Stretch),
        _ => Err(to_py_err("invalid align spec")),
    }
}

fn py_to_anchor_spec(obj: &Bound<'_, PyAny>) -> PyResult<AnchorSpec> {
    let value: String = obj.extract()?;
    let name = normalize_name(&value);
    match name.as_str() {
        "topleft" => Ok(AnchorSpec::TopLeft),
        "topright" => Ok(AnchorSpec::TopRight),
        "bottomleft" => Ok(AnchorSpec::BottomLeft),
        "bottomright" => Ok(AnchorSpec::BottomRight),
        "top" => Ok(AnchorSpec::Top),
        "bottom" => Ok(AnchorSpec::Bottom),
        "left" => Ok(AnchorSpec::Left),
        "right" => Ok(AnchorSpec::Right),
        "center" => Ok(AnchorSpec::Center),
        _ => Err(to_py_err("invalid anchor spec")),
    }
}

fn py_to_anchor_placement_spec(obj: &Bound<'_, PyAny>) -> PyResult<AnchorPlacementSpec> {
    let dict = obj
        .cast::<PyDict>()
        .map_err(|_| to_py_err("anchor must be dict"))?;
    let anchor_obj = expect_key(dict, "anchor")?;
    let anchor = py_to_anchor_spec(&anchor_obj)?;
    let offset_x = dict_get(dict, "offset_x")?
        .map(|v| py_to_i16(&v, "offset_x"))
        .transpose()?
        .unwrap_or(0);
    let offset_y = dict_get(dict, "offset_y")?
        .map(|v| py_to_i16(&v, "offset_y"))
        .transpose()?
        .unwrap_or(0);
    Ok(AnchorPlacementSpec {
        anchor,
        offset_x,
        offset_y,
    })
}

fn py_to_edge_insets_spec(obj: &Bound<'_, PyAny>) -> PyResult<EdgeInsetsSpec> {
    if let Some(value) = py_to_u16_opt(obj)? {
        return Ok(EdgeInsetsSpec {
            top: value,
            right: value,
            bottom: value,
            left: value,
        });
    }

    if let Ok(list) = obj.cast::<PyList>() {
        if list.len() != 4 {
            return Err(to_py_err("edge insets list must have 4 elements"));
        }
        return Ok(EdgeInsetsSpec {
            top: py_to_u16(&list.get_item(0)?, "top")?,
            right: py_to_u16(&list.get_item(1)?, "right")?,
            bottom: py_to_u16(&list.get_item(2)?, "bottom")?,
            left: py_to_u16(&list.get_item(3)?, "left")?,
        });
    }

    if let Ok(tuple) = obj.cast::<PyTuple>() {
        if tuple.len() != 4 {
            return Err(to_py_err("edge insets tuple must have 4 elements"));
        }
        return Ok(EdgeInsetsSpec {
            top: py_to_u16(&tuple.get_item(0)?, "top")?,
            right: py_to_u16(&tuple.get_item(1)?, "right")?,
            bottom: py_to_u16(&tuple.get_item(2)?, "bottom")?,
            left: py_to_u16(&tuple.get_item(3)?, "left")?,
        });
    }

    if let Ok(dict) = obj.cast::<PyDict>() {
        let top = dict_get(dict, "top")?
            .map(|v| py_to_u16(&v, "top"))
            .transpose()?
            .unwrap_or(0);
        let right = dict_get(dict, "right")?
            .map(|v| py_to_u16(&v, "right"))
            .transpose()?
            .unwrap_or(0);
        let bottom = dict_get(dict, "bottom")?
            .map(|v| py_to_u16(&v, "bottom"))
            .transpose()?
            .unwrap_or(0);
        let left = dict_get(dict, "left")?
            .map(|v| py_to_u16(&v, "left"))
            .transpose()?
            .unwrap_or(0);
        return Ok(EdgeInsetsSpec {
            top,
            right,
            bottom,
            left,
        });
    }

    Err(to_py_err("invalid edge insets"))
}

fn py_dict_to_value_map(dict: &Bound<'_, PyDict>) -> PyResult<BTreeMap<String, ComponentValue>> {
    let mut map = BTreeMap::new();
    for (key, value) in dict.iter() {
        let name: String = key.extract()?;
        map.insert(name, py_to_component_value(&value)?);
    }
    Ok(map)
}

fn py_dict_to_event_map(dict: &Bound<'_, PyDict>) -> PyResult<BTreeMap<String, CallbackId>> {
    let mut map = BTreeMap::new();
    for (key, value) in dict.iter() {
        let name: String = key.extract()?;
        map.insert(name, py_to_callback_id(&value)?);
    }
    Ok(map)
}

fn py_to_callback_id(obj: &Bound<'_, PyAny>) -> PyResult<CallbackId> {
    let value = py_to_u64(obj, "callback")?;
    Ok(CallbackId(value))
}

fn py_to_component_value(obj: &Bound<'_, PyAny>) -> PyResult<ComponentValue> {
    if obj.is_none() {
        return Ok(ComponentValue::Null);
    }

    if let Ok(value) = obj.extract::<bool>() {
        return Ok(ComponentValue::Bool(value));
    }

    if let Ok(value) = obj.extract::<i64>() {
        if value < 0 {
            return Ok(ComponentValue::I64(value));
        }
        return Ok(ComponentValue::U64(value as u64));
    }

    if let Ok(value) = obj.extract::<u64>() {
        return Ok(ComponentValue::U64(value));
    }

    if let Ok(value) = obj.extract::<f64>() {
        return Ok(ComponentValue::F64(value));
    }

    if let Ok(value) = obj.extract::<String>() {
        return Ok(ComponentValue::String(value));
    }

    if let Ok(bytes) = obj.cast::<PyBytes>() {
        return Ok(ComponentValue::Bytes(bytes.as_bytes().to_vec()));
    }

    if let Ok(bytes) = obj.cast::<PyByteArray>() {
        return Ok(ComponentValue::Bytes(bytes.to_vec()));
    }

    if let Ok(list) = obj.cast::<PyList>() {
        return py_list_to_component_value(list);
    }

    if let Ok(tuple) = obj.cast::<PyTuple>() {
        return py_tuple_to_component_value(tuple);
    }

    if let Ok(dict) = obj.cast::<PyDict>() {
        return py_dict_to_component_value(dict);
    }

    Err(to_py_err("unsupported value type"))
}

fn py_list_to_component_value(list: &Bound<'_, PyList>) -> PyResult<ComponentValue> {
    if list.len() == 0 {
        return Ok(ComponentValue::List(Vec::new()));
    }

    let items: Vec<Bound<'_, PyAny>> = list.iter().collect();

    if items.iter().all(|item| item.cast::<PyString>().is_ok()) {
        let mut out = Vec::with_capacity(items.len());
        for item in &items {
            out.push(item.extract::<String>()?);
        }
        return Ok(ComponentValue::StringList(out));
    }

    if items.iter().all(|item| {
        if let Ok(list) = item.cast::<PyList>() {
            return list.iter().all(|cell| cell.cast::<PyString>().is_ok());
        }
        if let Ok(tuple) = item.cast::<PyTuple>() {
            return tuple.iter().all(|cell| cell.cast::<PyString>().is_ok());
        }
        false
    }) {
        let mut table = Vec::with_capacity(items.len());
        for row in &items {
            let mut row_out = Vec::new();
            if let Ok(list) = row.cast::<PyList>() {
                row_out.reserve(list.len());
                for cell in list.iter() {
                    row_out.push(cell.extract::<String>()?);
                }
            } else if let Ok(tuple) = row.cast::<PyTuple>() {
                row_out.reserve(tuple.len());
                for cell in tuple.iter() {
                    row_out.push(cell.extract::<String>()?);
                }
            }
            table.push(row_out);
        }
        return Ok(ComponentValue::Table(table));
    }

    let mut out = Vec::with_capacity(items.len());
    for item in &items {
        out.push(py_to_component_value(item)?);
    }
    Ok(ComponentValue::List(out))
}

fn py_tuple_to_component_value(tuple: &Bound<'_, PyTuple>) -> PyResult<ComponentValue> {
    let py = tuple.py();
    let list = PyList::empty(py);
    for item in tuple.iter() {
        list.append(item)?;
    }
    py_list_to_component_value(&list)
}

fn py_dict_to_component_value(dict: &Bound<'_, PyDict>) -> PyResult<ComponentValue> {
    if dict.len() == 4
        && dict.contains("x")?
        && dict.contains("y")?
        && dict.contains("width")?
        && dict.contains("height")?
    {
        let rect = ComponentValue::Rect(RuntimeRect {
            x: py_to_u16(&expect_key(dict, "x")?, "x")?,
            y: py_to_u16(&expect_key(dict, "y")?, "y")?,
            width: py_to_u16(&expect_key(dict, "width")?, "width")?,
            height: py_to_u16(&expect_key(dict, "height")?, "height")?,
        });
        return Ok(rect);
    }

    let mut map = BTreeMap::new();
    for (key, value) in dict.iter() {
        let name: String = key.extract()?;
        map.insert(name, py_to_component_value(&value)?);
    }
    Ok(ComponentValue::Map(map))
}

fn component_value_to_py(py: Python<'_>, value: &ComponentValue) -> PyResult<PyObject> {
    match value {
        ComponentValue::Null => Ok(py.None()),
        ComponentValue::Bool(v) => v.into_py_any(py),
        ComponentValue::I64(v) => v.into_py_any(py),
        ComponentValue::U64(v) => v.into_py_any(py),
        ComponentValue::F64(v) => v.into_py_any(py),
        ComponentValue::String(v) => v.into_py_any(py),
        ComponentValue::StringList(values) => {
            let list = PyList::empty(py);
            for value in values {
                list.append(value)?;
            }
            Ok(list.into_any().unbind())
        }
        ComponentValue::Table(rows) => {
            let list = PyList::empty(py);
            for row in rows {
                let row_list = PyList::empty(py);
                for cell in row {
                    row_list.append(cell)?;
                }
                list.append(row_list)?;
            }
            Ok(list.into_any().unbind())
        }
        ComponentValue::Rect(rect) => {
            let dict = PyDict::new(py);
            dict.set_item("x", rect.x)?;
            dict.set_item("y", rect.y)?;
            dict.set_item("width", rect.width)?;
            dict.set_item("height", rect.height)?;
            Ok(dict.into_any().unbind())
        }
        ComponentValue::Bytes(bytes) => Ok(PyBytes::new(py, bytes).into_any().unbind()),
        ComponentValue::List(values) => {
            let list = PyList::empty(py);
            for value in values {
                list.append(component_value_to_py(py, value)?)?;
            }
            Ok(list.into_any().unbind())
        }
        ComponentValue::Map(map) => {
            let dict = PyDict::new(py);
            for (key, value) in map {
                dict.set_item(key, component_value_to_py(py, value)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

fn callback_invocation_to_py(py: Python<'_>, event: &CallbackInvocation) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("callback_id", event.callback_id.0)?;
    if let Some(target_id) = &event.target_id {
        dict.set_item("target_id", target_id)?;
    } else {
        dict.set_item("target_id", py.None())?;
    }
    dict.set_item("event", &event.event)?;
    if let Some(payload) = &event.payload {
        dict.set_item("payload", component_value_to_py(py, payload)?)?;
    } else {
        dict.set_item("payload", py.None())?;
    }
    Ok(dict.into_any().unbind())
}

fn desktop_event_result_to_py(py: Python<'_>, result: &DesktopEventResult) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("consumed", result.is_consumed())?;
    dict.set_item("outcome", format!("{:?}", result.outcome))?;
    match result.action {
        DesktopAction::None => dict.set_item("action", py.None())?,
        DesktopAction::CloseWindow(id) => {
            let action = PyDict::new(py);
            action.set_item("type", "close_window")?;
            action.set_item("window_id", id.raw())?;
            dict.set_item("action", action)?;
        }
    }
    Ok(dict.into_any().unbind())
}

fn window_info_to_py(py: Python<'_>, window: &WindowInfo) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("id", window.id.raw())?;
    if let Some(tag) = &window.tag {
        dict.set_item("tag", tag)?;
    } else {
        dict.set_item("tag", py.None())?;
    }
    dict.set_item("title", &window.title)?;
    dict.set_item("kind", format!("{:?}", window.kind))?;
    dict.set_item("state", format!("{:?}", window.state))?;
    dict.set_item("rect", ratatui_rect_to_py(py, window.rect)?)?;
    dict.set_item("is_focused", window.is_focused)?;
    Ok(dict.into_any().unbind())
}

fn desktop_snapshot_to_py(py: Python<'_>, snapshot: &DesktopSnapshot) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("bounds", runtime_rect_to_py(py, snapshot.bounds)?)?;
    dict.set_item("tree", desktop_snapshot_node_to_py(py, &snapshot.tree)?)?;
    Ok(dict.into_any().unbind())
}

fn desktop_snapshot_node_to_py(py: Python<'_>, node: &DesktopSnapshotNode) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("kind", node_kind_to_py(node.kind))?;
    if let Some(id) = &node.id {
        dict.set_item("id", id)?;
    } else {
        dict.set_item("id", py.None())?;
    }
    if let Some(tag) = &node.tag {
        dict.set_item("tag", tag)?;
    } else {
        dict.set_item("tag", py.None())?;
    }
    dict.set_item("name", &node.name)?;
    dict.set_item("type_name", &node.type_name)?;
    if let Some(bounds) = node.bounds {
        dict.set_item("bounds", runtime_rect_to_py(py, bounds)?)?;
    } else {
        dict.set_item("bounds", py.None())?;
    }
    if let Some(text) = &node.text {
        dict.set_item("text", text)?;
    } else {
        dict.set_item("text", py.None())?;
    }
    if let Some(state) = &node.state {
        dict.set_item("state", state)?;
    } else {
        dict.set_item("state", py.None())?;
    }
    if let Some(window_id) = node.window_id {
        dict.set_item("window_id", window_id)?;
    } else {
        dict.set_item("window_id", py.None())?;
    }

    let properties = PyDict::new(py);
    for (key, value) in &node.properties {
        properties.set_item(key, component_value_to_py(py, value)?)?;
    }
    dict.set_item("properties", properties)?;

    let children = PyList::empty(py);
    for child in &node.children {
        children.append(desktop_snapshot_node_to_py(py, child)?)?;
    }
    dict.set_item("children", children)?;
    Ok(dict.into_any().unbind())
}

fn node_kind_to_py(kind: NodeKind) -> &'static str {
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

fn ratatui_rect_to_py(py: Python<'_>, rect: Rect) -> PyResult<PyObject> {
    runtime_rect_to_py(
        py,
        RuntimeRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        },
    )
}

fn runtime_rect_to_py(py: Python<'_>, rect: RuntimeRect) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("x", rect.x)?;
    dict.set_item("y", rect.y)?;
    dict.set_item("width", rect.width)?;
    dict.set_item("height", rect.height)?;
    Ok(dict.into_any().unbind())
}

fn component_schema_to_py(py: Python<'_>, schema: &ComponentSchema) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("type", &schema.type_name)?;
    dict.set_item("allows_children", schema.allows_children)?;
    dict.set_item(
        "properties",
        property_meta_list_to_py(py, &schema.properties)?,
    )?;
    dict.set_item("actions", action_meta_list_to_py(py, &schema.actions)?)?;
    dict.set_item("events", event_meta_list_to_py(py, &schema.events)?)?;
    Ok(dict.into_any().unbind())
}

fn property_meta_list_to_py(py: Python<'_>, list: &[PropertyMeta]) -> PyResult<PyObject> {
    let out = PyList::empty(py);
    for item in list {
        let dict = PyDict::new(py);
        dict.set_item("name", &item.name)?;
        dict.set_item("value_type", value_type_to_py(py, item.value_type))?;
        dict.set_item("readable", item.readable)?;
        dict.set_item("writable", item.writable)?;
        out.append(dict)?;
    }
    Ok(out.into_any().unbind())
}

fn action_meta_list_to_py(py: Python<'_>, list: &[ActionMeta]) -> PyResult<PyObject> {
    let out = PyList::empty(py);
    for item in list {
        let dict = PyDict::new(py);
        dict.set_item("name", &item.name)?;
        dict.set_item(
            "payload",
            item.payload
                .map(|v| value_type_to_py(py, v))
                .unwrap_or_else(|| py.None()),
        )?;
        out.append(dict)?;
    }
    Ok(out.into_any().unbind())
}

fn event_meta_list_to_py(py: Python<'_>, list: &[EventMeta]) -> PyResult<PyObject> {
    let out = PyList::empty(py);
    for item in list {
        let dict = PyDict::new(py);
        dict.set_item("name", &item.name)?;
        dict.set_item(
            "payload",
            item.payload
                .map(|v| value_type_to_py(py, v))
                .unwrap_or_else(|| py.None()),
        )?;
        out.append(dict)?;
    }
    Ok(out.into_any().unbind())
}

fn value_type_to_py(py: Python<'_>, value: ValueType) -> PyObject {
    let name = match value {
        ValueType::Bool => "Bool",
        ValueType::I64 => "I64",
        ValueType::U64 => "U64",
        ValueType::F64 => "F64",
        ValueType::String => "String",
        ValueType::StringList => "StringList",
        ValueType::Table => "Table",
        ValueType::Rect => "Rect",
        ValueType::Bytes => "Bytes",
        ValueType::List => "List",
        ValueType::Map => "Map",
        ValueType::Unknown => "Unknown",
    };
    PyString::new(py, name).into_any().unbind()
}

fn dict_get<'py>(dict: &Bound<'py, PyDict>, key: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
    dict.get_item(key)
}

fn expect_key<'py>(dict: &Bound<'py, PyDict>, key: &str) -> PyResult<Bound<'py, PyAny>> {
    dict_get(dict, key)?.ok_or_else(|| to_py_err(format!("missing key: {key}")))
}

fn expect_string_key(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<String> {
    expect_key(dict, key)?.extract()
}

fn py_to_u16(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<u16> {
    let value =
        py_to_u16_opt(obj)?.ok_or_else(|| to_py_err(format!("expected {name} as integer")))?;
    Ok(value)
}

fn py_to_u16_opt(obj: &Bound<'_, PyAny>) -> PyResult<Option<u16>> {
    if let Ok(value) = obj.extract::<i64>() {
        if value < 0 {
            return Err(to_py_err("integer must be non-negative"));
        }
        let value = (value as u64).min(u16::MAX as u64) as u16;
        return Ok(Some(value));
    }
    if let Ok(value) = obj.extract::<u64>() {
        let value = value.min(u16::MAX as u64) as u16;
        return Ok(Some(value));
    }
    if let Ok(value) = obj.extract::<f64>() {
        if value < 0.0 {
            return Err(to_py_err("number must be non-negative"));
        }
        let value = value.min(u16::MAX as f64) as u16;
        return Ok(Some(value));
    }
    Ok(None)
}

fn py_to_usize(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<usize> {
    if let Ok(value) = obj.extract::<i64>() {
        if value < 0 {
            return Err(to_py_err(format!("{name} must be non-negative")));
        }
        return Ok(value as usize);
    }
    if let Ok(value) = obj.extract::<u64>() {
        return Ok(value as usize);
    }
    Err(to_py_err(format!("expected {name} as integer")))
}

fn py_to_u64(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<u64> {
    if let Ok(value) = obj.extract::<i64>() {
        if value < 0 {
            return Err(to_py_err(format!("{name} must be non-negative")));
        }
        return Ok(value as u64);
    }
    if let Ok(value) = obj.extract::<u64>() {
        return Ok(value);
    }
    Err(to_py_err(format!("expected {name} as integer")))
}

fn py_to_i16(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<i16> {
    let value = py_to_i64(obj, name)?;
    if value > i16::MAX as i64 {
        return Ok(i16::MAX);
    }
    if value < i16::MIN as i64 {
        return Ok(i16::MIN);
    }
    Ok(value as i16)
}

fn py_to_i32(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<i32> {
    let value = py_to_i64(obj, name)?;
    if value > i32::MAX as i64 {
        return Ok(i32::MAX);
    }
    if value < i32::MIN as i64 {
        return Ok(i32::MIN);
    }
    Ok(value as i32)
}

fn py_to_i64(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<i64> {
    if let Ok(value) = obj.extract::<i64>() {
        return Ok(value);
    }
    if let Ok(value) = obj.extract::<u64>() {
        if value > i64::MAX as u64 {
            return Err(to_py_err(format!("{name} is too large")));
        }
        return Ok(value as i64);
    }
    Err(to_py_err(format!("expected {name} as integer")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::{PyDict, PyList, PyTuple};
    use std::sync::Once;

    fn with_py<F, R>(f: F) -> R
    where
        F: for<'py> FnOnce(Python<'py>) -> R,
    {
        static INIT: Once = Once::new();
        INIT.call_once(Python::initialize);
        Python::attach(f)
    }

    #[test]
    fn parse_component_spec_basic() {
        with_py(|py| {
            let spec = PyDict::new(py);
            spec.set_item("type", "Label").unwrap();
            spec.set_item("id", "title").unwrap();

            let props = PyDict::new(py);
            props.set_item("text", "Hello").unwrap();
            spec.set_item("props", props).unwrap();

            let events = PyDict::new(py);
            events.set_item("click", 1).unwrap();
            spec.set_item("events", events).unwrap();

            let parsed = py_to_component_spec(&spec).unwrap();
            assert_eq!(parsed.type_name, "Label");
            assert_eq!(parsed.id.as_deref(), Some("title"));
            assert_eq!(
                parsed.props.get("text"),
                Some(&ComponentValue::String("Hello".to_string()))
            );
            assert_eq!(parsed.events.get("click"), Some(&CallbackId(1)));
        });
    }

    #[test]
    fn parse_tree_ops_list() {
        with_py(|py| {
            let ops = PyList::empty(py);
            let op = PyDict::new(py);
            op.set_item("op", "set_prop").unwrap();
            op.set_item("id", "title").unwrap();
            op.set_item("name", "text").unwrap();
            op.set_item("value", "Hi").unwrap();
            ops.append(op).unwrap();
            let clear = PyDict::new(py);
            clear.set_item("op", "clear_prop").unwrap();
            clear.set_item("id", "title").unwrap();
            clear.set_item("name", "text").unwrap();
            ops.append(clear).unwrap();

            let parsed = py_to_tree_ops(&ops).unwrap();
            assert_eq!(parsed.len(), 2);
            match &parsed[0] {
                TreeOp::SetProp { id, name, value } => {
                    assert_eq!(id, "title");
                    assert_eq!(name, "text");
                    assert_eq!(value, &ComponentValue::String("Hi".to_string()));
                }
                other => panic!("unexpected op: {other:?}"),
            }
            match &parsed[1] {
                TreeOp::ClearProp { id, name } => {
                    assert_eq!(id, "title");
                    assert_eq!(name, "text");
                }
                other => panic!("unexpected op: {other:?}"),
            }
        });
    }

    #[test]
    fn parse_child_with_layout_and_meta() {
        with_py(|py| {
            let node = PyDict::new(py);
            node.set_item("type", "Label").unwrap();
            node.set_item("id", "tab1").unwrap();

            let layout = PyDict::new(py);
            let width = PyDict::new(py);
            width.set_item("fixed", 12).unwrap();
            layout.set_item("width", width).unwrap();
            layout.set_item("align_x", "center").unwrap();

            let meta = PyDict::new(py);
            meta.set_item("title", "Tab 1").unwrap();

            let child = PyDict::new(py);
            child.set_item("node", node).unwrap();
            child.set_item("layout", layout).unwrap();
            child.set_item("meta", meta).unwrap();

            let parsed = py_to_component_spec_child(&child).unwrap();
            assert_eq!(parsed.node.type_name, "Label");
            assert_eq!(parsed.node.id.as_deref(), Some("tab1"));
            assert_eq!(parsed.layout.as_ref().unwrap().width, SizeSpec::Fixed(12));
            assert_eq!(
                parsed.meta.get("title").and_then(|v| v.as_str()),
                Some("Tab 1")
            );
        });
    }

    #[test]
    fn parse_component_value_lists() {
        with_py(|py| {
            let list = PyList::new(py, ["a", "b", "c"]).unwrap();
            let parsed = py_to_component_value(&list).unwrap();
            assert_eq!(
                parsed,
                ComponentValue::StringList(vec!["a".to_string(), "b".to_string(), "c".to_string()])
            );

            let row1 = PyTuple::new(py, ["x", "y"]).unwrap();
            let row2 = PyTuple::new(py, ["1", "2"]).unwrap();
            let table = PyList::new(py, [row1, row2]).unwrap();
            let parsed = py_to_component_value(&table).unwrap();
            assert_eq!(
                parsed,
                ComponentValue::Table(vec![
                    vec!["x".to_string(), "y".to_string()],
                    vec!["1".to_string(), "2".to_string()],
                ])
            );
        });
    }
}
