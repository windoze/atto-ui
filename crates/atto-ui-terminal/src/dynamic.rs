use atto_ui::composable::Component;
use atto_ui::runtime::{
    component_schema, event_handle, invalid_prop_reason, prop_bool, prop_string, prop_u64,
    prop_usize, prop_vec_string, register_registry_extension, wrap_with_id,
};
use atto_ui::{
    CallbackRegistry, ComponentPropertySchema, ComponentRegistry, ComponentSchema, ComponentValue,
    EventMeta, PropertyMeta, ValueType,
};

use crate::{TerminalEmulator, TerminalShellIntegration};

impl ComponentPropertySchema for TerminalEmulator {
    fn property_schema() -> Vec<PropertyMeta> {
        // All of these are consumed only at construction; `TerminalEmulator`
        // does not implement `get_property`, so declaring them readable would
        // make introspection/scripting clients believe they can query values
        // that always come back `not_found`. Mark them write-only to match.
        vec![
            PropertyMeta::new("command", ValueType::String).write_only(),
            PropertyMeta::new("args", ValueType::StringList).write_only(),
            PropertyMeta::new("scrollback_len", ValueType::U64).write_only(),
            PropertyMeta::new("capture", ValueType::Bool).write_only(),
            PropertyMeta::new("capture_on_click", ValueType::Bool).write_only(),
            PropertyMeta::new("prefix_key", ValueType::String).write_only(),
            PropertyMeta::new("shell_integration", ValueType::Bool).write_only(),
            PropertyMeta::new("scroll_step", ValueType::U64).write_only(),
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

        if let Some(key) = prop_string(spec, "prefix_key")? {
            let mut chars = key.chars();
            let letter = chars.next().filter(|_| chars.next().is_none());
            let Some(letter) = letter.filter(char::is_ascii_alphabetic) else {
                return Err(invalid_prop_reason(
                    spec,
                    "prefix_key",
                    "terminal prefix key must be a single ASCII letter",
                ));
            };
            view = view
                .prefix_key(letter)
                .map_err(|err| invalid_prop_reason(spec, "prefix_key", err.to_string()))?;
        }

        if let Some(step) = prop_u64(spec, "scroll_step")? {
            // scroll_step is a u16; reject oversized values instead of silently
            // clamping (which `prop_u16` would do).
            if step > u16::MAX as u64 {
                return Err(invalid_prop_reason(
                    spec,
                    "scroll_step",
                    format!("terminal scroll_step must be <= {}", u16::MAX),
                ));
            }
            view = view.scroll_step(step as u16);
        }

        if let Some(enabled) = prop_bool(spec, "shell_integration")? {
            view = view.shell_integration(if enabled {
                TerminalShellIntegration::enabled()
            } else {
                TerminalShellIntegration::Disabled
            });
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
