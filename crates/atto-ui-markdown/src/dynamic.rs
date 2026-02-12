use atto_ui::composable::{Component, ScrollbarVisibility};
use atto_ui::runtime::{
    component_schema, event_handle, invalid_prop, prop_bool, prop_string, prop_u16,
    register_registry_extension, wrap_with_id,
};
use atto_ui::{
    CallbackRegistry, ComponentPropertySchema, ComponentRegistry, ComponentSchema, ComponentSpec,
    ComponentValue, EventMeta, PropertyMeta, TreeError, ValueType,
};

use crate::MarkdownViewer;

impl ComponentPropertySchema for MarkdownViewer {
    fn property_schema() -> Vec<PropertyMeta> {
        vec![
            PropertyMeta::new("markdown", ValueType::String),
            PropertyMeta::new("wrap_width", ValueType::U64),
            PropertyMeta::new("show_markers", ValueType::Bool),
            PropertyMeta::new("vertical_scrollbar", ValueType::String),
            PropertyMeta::new("code_block_max_height", ValueType::U64),
            PropertyMeta::new("table_max_height", ValueType::U64),
        ]
    }
}

pub fn markdown_viewer_schema() -> ComponentSchema {
    component_schema::<MarkdownViewer>("MarkdownViewer")
        .with_event(EventMeta::new("link").with_payload(ValueType::String))
        .allow_children(false)
}

fn parse_scrollbar_visibility(raw: &str) -> Option<ScrollbarVisibility> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "always" => Some(ScrollbarVisibility::Always),
        "auto" => Some(ScrollbarVisibility::Auto),
        "never" => Some(ScrollbarVisibility::Never),
        _ => None,
    }
}

fn prop_scrollbar_visibility(
    spec: &ComponentSpec,
    name: &str,
) -> Result<Option<ScrollbarVisibility>, TreeError> {
    let Some(value) = spec.props.get(name) else {
        return Ok(None);
    };

    let ComponentValue::String(raw) = value else {
        return Err(invalid_prop(
            spec,
            name,
            "scrollbar visibility string (auto|always|never)",
            value,
        ));
    };

    parse_scrollbar_visibility(raw)
        .ok_or_else(|| {
            invalid_prop(
                spec,
                name,
                "scrollbar visibility string (auto|always|never)",
                value,
            )
        })
        .map(Some)
}

pub fn register_markdown_viewer(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = markdown_viewer_schema();

    registry.register(schema, move |spec, _registry| {
        let markdown = match prop_string(spec, "markdown")? {
            Some(value) => Some(value),
            None => prop_string(spec, "text")?,
        }
        .unwrap_or_default();

        let mut view = MarkdownViewer::new(markdown);

        if let Some(width) = prop_u16(spec, "wrap_width")?.or(prop_u16(spec, "width")?) {
            view = view.wrap_width(width);
        }

        if let Some(show) = prop_bool(spec, "show_markers")? {
            view = view.show_markers(show);
        }

        if let Some(vis) = prop_scrollbar_visibility(spec, "vertical_scrollbar")? {
            view = view.vertical_scrollbar(vis);
        }

        if let Some(height) = prop_u16(spec, "code_block_max_height")? {
            view = view.code_block_max_height(height);
        }

        if let Some(height) = prop_u16(spec, "table_max_height")? {
            view = view.table_max_height(height);
        }

        if let Some(cb) = event_handle(spec, "link", callbacks.clone()) {
            view = view.on_link(move |url| {
                cb.emit_with(Some(ComponentValue::String(url.to_string())));
            });
        }

        Ok(wrap_with_id(spec, Box::new(view)))
    });
}

fn register_markdown_viewer_extension(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    register_markdown_viewer(registry, callbacks);
}

/// 将 `MarkdownViewer` 注册到 `atto-ui` 的全局动态组件注册表中。
///
/// 返回：
/// - `true`：本次注册成功
/// - `false`：已注册过（幂等）
pub fn register_runtime_components() -> bool {
    register_registry_extension("atto-ui-markdown", register_markdown_viewer_extension)
}
