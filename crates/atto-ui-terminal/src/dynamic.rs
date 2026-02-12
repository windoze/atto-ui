use atto_ui::composable::Component;
use atto_ui::runtime::{
    component_schema, event_handle, invalid_prop_reason, prop_bool, prop_string, prop_u16,
    prop_usize, prop_vec_string, register_registry_extension, wrap_with_id,
};
use atto_ui::{
    CallbackRegistry, ComponentPropertySchema, ComponentRegistry, ComponentSchema, ComponentValue,
    EventMeta, PropertyMeta, ValueType,
};

use crate::TerminalEmulator;

impl ComponentPropertySchema for TerminalEmulator {
    fn property_schema() -> Vec<PropertyMeta> {
        vec![
            PropertyMeta::new("command", ValueType::String).write_only(),
            PropertyMeta::new("args", ValueType::StringList).write_only(),
            PropertyMeta::new("scrollback_len", ValueType::U64),
            PropertyMeta::new("capture", ValueType::Bool),
            PropertyMeta::new("capture_on_click", ValueType::Bool),
            PropertyMeta::new("scroll_step", ValueType::U64),
        ]
    }
}

pub fn terminal_emulator_schema() -> ComponentSchema {
    component_schema::<TerminalEmulator>("TerminalEmulator")
        .with_event(EventMeta::new("input").with_payload(ValueType::Bytes))
        .with_event(EventMeta::new("close"))
        .allow_children(false)
}

pub fn register_terminal_emulator(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = terminal_emulator_schema();

    registry.register(schema, move |spec, _registry| {
        let mut view = TerminalEmulator::new();

        if let Some(len) = prop_usize(spec, "scrollback_len")? {
            view = view.scrollback_len(len);
        }

        if let Some(capture) = prop_bool(spec, "capture")? {
            view = view.capture(capture);
        }

        if let Some(enabled) = prop_bool(spec, "capture_on_click")? {
            view = view.capture_on_click(enabled);
        }

        if let Some(step) = prop_u16(spec, "scroll_step")? {
            view = view.scroll_step(step);
        }

        if let Some(cb) = event_handle(spec, "input", callbacks.clone()) {
            view = view.on_input(move |bytes| {
                cb.emit_with(Some(ComponentValue::Bytes(bytes.to_vec())));
            });
        }

        if let Some(cb) = event_handle(spec, "close", callbacks.clone()) {
            view = view.on_close(move || {
                cb.emit();
            });
        }

        let command = prop_string(spec, "command")?;
        if let Some(command) = command {
            let args = prop_vec_string(spec, "args")?.unwrap_or_default();
            view.spawn_process(&command, &args)
                .map_err(|err| invalid_prop_reason(spec, "command", err.to_string()))?;
        }

        Ok(wrap_with_id(spec, Box::new(view)))
    });
}

fn register_terminal_emulator_extension(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    register_terminal_emulator(registry, callbacks);
}

/// 将 `TerminalEmulator` 注册到 `atto-ui` 的全局动态组件注册表中。
///
/// 返回：
/// - `true`：本次注册成功
/// - `false`：已注册过（幂等）
pub fn register_runtime_components() -> bool {
    register_registry_extension("atto-ui-terminal", register_terminal_emulator_extension)
}
