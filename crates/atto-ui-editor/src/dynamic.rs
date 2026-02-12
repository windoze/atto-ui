use atto_ui::composable::Component;
use atto_ui::reactive::Binding;
use atto_ui::runtime::{
    component_schema, prop_bool, prop_string, prop_usize, register_registry_extension, wrap_with_id,
};
use atto_ui::{
    CallbackRegistry, ComponentPropertySchema, ComponentRegistry, ComponentSchema, PropertyMeta,
    ValueType,
};

use crate::{EditorConfig, EditorThemeSet, EditorView};

impl ComponentPropertySchema for EditorView {
    fn property_schema() -> Vec<PropertyMeta> {
        vec![
            PropertyMeta::new("text", ValueType::String),
            PropertyMeta::new("language_id", ValueType::String),
            PropertyMeta::new("show_line_numbers", ValueType::Bool),
            PropertyMeta::new("show_folding_markers", ValueType::Bool),
            PropertyMeta::new("tab_width", ValueType::U64),
            PropertyMeta::new("insert_spaces", ValueType::Bool),
        ]
    }
}

pub fn editor_schema() -> ComponentSchema {
    component_schema::<EditorView>("Editor").allow_children(false)
}

pub fn register_editor(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    _callbacks: CallbackRegistry,
) {
    let schema = editor_schema();

    registry.register(schema, move |spec, _registry| {
        let initial_text = prop_string(spec, "text")?.unwrap_or_default();
        let text: Binding<String> = initial_text.into();
        let theme: Binding<EditorThemeSet> = EditorThemeSet::default().into();

        let config = EditorConfig::new(text);

        if let Some(language_id) = prop_string(spec, "language_id")? {
            config.language_id.set(language_id);
        }
        if let Some(show) = prop_bool(spec, "show_line_numbers")? {
            config.show_line_numbers.set(show);
        }
        if let Some(show) = prop_bool(spec, "show_folding_markers")? {
            config.show_folding_markers.set(show);
        }
        if let Some(width) = prop_usize(spec, "tab_width")? {
            config.indent.tab_width.set(width);
        }
        if let Some(insert_spaces) = prop_bool(spec, "insert_spaces")? {
            config.indent.insert_spaces.set(insert_spaces);
        }

        let (view, _handle) = EditorView::new(config, theme);
        Ok(wrap_with_id(spec, Box::new(view)))
    });
}

fn register_editor_extension(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    register_editor(registry, callbacks);
}

/// 将 `EditorView`（`type: "Editor"`）注册到 `atto-ui` 的全局动态组件注册表中。
///
/// 返回：
/// - `true`：本次注册成功
/// - `false`：已注册过（幂等）
pub fn register_runtime_components() -> bool {
    register_registry_extension("atto-ui-editor", register_editor_extension)
}
