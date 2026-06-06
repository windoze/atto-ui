use std::collections::BTreeMap;

use atto_ui::composable::Component;
use atto_ui::reactive::Binding;
use atto_ui::runtime::{
    component_schema, event_handle, invalid_prop, invalid_prop_reason, prop_bool, prop_string,
    prop_u16, prop_usize, prop_vec_string, register_registry_extension, wrap_with_id,
};
use atto_ui::{
    CallbackRegistry, ComponentPropertySchema, ComponentRegistry, ComponentSchema, ComponentValue,
    ComponentValueCodec, EventMeta, PropertyMeta, ValueType,
};

use crate::input::{chat_input_response_to_component_value, parse_chat_input_mode_value};
use crate::{
    ArtifactId, ArtifactKind, ChatInputHandle, ChatInputPanel, ChatMessage, ChatMessageContent,
    ChatMessageId, ChatMessageList, ChatMessageStatus, ChatSender, ChatToolCallStatus,
};

impl ComponentPropertySchema for ChatMessageList {
    fn property_schema() -> Vec<PropertyMeta> {
        vec![
            PropertyMeta::new("messages", ValueType::List),
            PropertyMeta::new("spacing", ValueType::U64),
            PropertyMeta::new("padding", ValueType::Map),
            PropertyMeta::new("wrap_width", ValueType::U64),
            PropertyMeta::new("show_timestamps", ValueType::Bool),
            PropertyMeta::new("auto_scroll", ValueType::Bool),
        ]
    }
}

impl ComponentPropertySchema for ChatInputPanel {
    fn property_schema() -> Vec<PropertyMeta> {
        vec![
            PropertyMeta::new("mode", ValueType::Map),
            PropertyMeta::new("draft", ValueType::String),
            PropertyMeta::new("custom", ValueType::String),
            PropertyMeta::new("history", ValueType::StringList),
            PropertyMeta::new("selection", ValueType::U64),
            PropertyMeta::new("enabled", ValueType::Bool),
            PropertyMeta::new("clear_on_submit", ValueType::Bool),
        ]
    }
}

pub fn chat_message_list_schema() -> ComponentSchema {
    component_schema::<ChatMessageList>("ChatMessageList")
        .with_event(EventMeta::new("load_more"))
        .with_event(EventMeta::new("open_artifact").with_payload(ValueType::String))
        .allow_children(false)
}

pub fn chat_input_panel_schema() -> ComponentSchema {
    component_schema::<ChatInputPanel>("ChatInputPanel")
        .with_event(EventMeta::new("submit").with_payload(ValueType::Map))
        .allow_children(false)
}

pub(crate) fn messages_to_component_value(messages: &[ChatMessage]) -> ComponentValue {
    ComponentValue::List(messages.iter().map(message_to_value).collect())
}

pub(crate) fn parse_messages_value(value: &ComponentValue) -> Result<Vec<ChatMessage>, String> {
    match value {
        ComponentValue::Null => Ok(Vec::new()),
        ComponentValue::List(items) => items.iter().map(parse_message_value).collect(),
        other => Err(format!("expected list, got {other:?}")),
    }
}

fn message_to_value(message: &ChatMessage) -> ComponentValue {
    let mut out = BTreeMap::<String, ComponentValue>::new();
    out.insert("id".to_string(), ComponentValue::U64(message.id.0));
    out.insert(
        "sender".to_string(),
        ComponentValue::String(sender_to_string(&message.sender)),
    );
    out.insert(
        "timestamp".to_string(),
        message
            .timestamp
            .as_ref()
            .map(|v| ComponentValue::String(v.clone()))
            .unwrap_or(ComponentValue::Null),
    );
    out.insert("status".to_string(), status_to_value(&message.status));
    out.insert("content".to_string(), content_to_value(&message.content));
    ComponentValue::Map(out)
}

fn sender_to_string(sender: &ChatSender) -> String {
    match sender {
        ChatSender::User => "user".to_string(),
        ChatSender::Assistant => "assistant".to_string(),
        ChatSender::System => "system".to_string(),
        ChatSender::Tool(name) => format!("tool:{name}"),
        ChatSender::Custom(name) => format!("custom:{name}"),
    }
}

fn status_to_value(status: &ChatMessageStatus) -> ComponentValue {
    match status {
        ChatMessageStatus::Final => ComponentValue::String("final".to_string()),
        ChatMessageStatus::InProgress => ComponentValue::String("in_progress".to_string()),
        ChatMessageStatus::Failed(reason) => {
            let mut map = BTreeMap::new();
            map.insert("failed".to_string(), ComponentValue::String(reason.clone()));
            ComponentValue::Map(map)
        }
    }
}

fn content_to_value(content: &ChatMessageContent) -> ComponentValue {
    match content {
        ChatMessageContent::Text { markdown } => {
            let mut map = BTreeMap::new();
            map.insert(
                "markdown".to_string(),
                ComponentValue::String(markdown.clone()),
            );
            ComponentValue::Map(map)
        }
        ChatMessageContent::File { name, url } => {
            let mut file = BTreeMap::new();
            file.insert("name".to_string(), ComponentValue::String(name.clone()));
            file.insert(
                "url".to_string(),
                url.as_ref()
                    .map(|v| ComponentValue::String(v.clone()))
                    .unwrap_or(ComponentValue::Null),
            );
            let mut map = BTreeMap::new();
            map.insert("file".to_string(), ComponentValue::Map(file));
            ComponentValue::Map(map)
        }
        ChatMessageContent::ToolCall {
            name,
            status,
            output,
        } => {
            let mut tool = BTreeMap::new();
            tool.insert("name".to_string(), ComponentValue::String(name.clone()));
            tool.insert(
                "status".to_string(),
                ComponentValue::String(tool_status_to_string(status).to_string()),
            );
            tool.insert("output".to_string(), ComponentValue::String(output.clone()));
            let mut map = BTreeMap::new();
            map.insert("tool_call".to_string(), ComponentValue::Map(tool));
            ComponentValue::Map(map)
        }
        ChatMessageContent::Artifact {
            kind,
            anchor,
            title,
        } => {
            let mut artifact = BTreeMap::new();
            artifact.insert(
                "kind".to_string(),
                ComponentValue::String(kind.as_str().to_string()),
            );
            artifact.insert(
                "anchor".to_string(),
                ComponentValue::String(anchor.to_string()),
            );
            artifact.insert("title".to_string(), ComponentValue::String(title.clone()));
            let mut map = BTreeMap::new();
            map.insert("artifact".to_string(), ComponentValue::Map(artifact));
            ComponentValue::Map(map)
        }
    }
}

fn tool_status_to_string(status: &ChatToolCallStatus) -> &'static str {
    match status {
        ChatToolCallStatus::Running => "running",
        ChatToolCallStatus::Done => "done",
        ChatToolCallStatus::Error => "error",
    }
}

fn parse_message_value(value: &ComponentValue) -> Result<ChatMessage, String> {
    let ComponentValue::Map(map) = value else {
        return Err(format!("expected map, got {value:?}"));
    };

    let id = map
        .get("id")
        .and_then(ComponentValue::as_u64)
        .ok_or_else(|| "missing id".to_string())?;

    let sender = map
        .get("sender")
        .map(parse_sender_value)
        .transpose()?
        .unwrap_or(ChatSender::Assistant);

    let timestamp = map
        .get("timestamp")
        .and_then(ComponentValue::as_str)
        .map(|s| s.to_string());

    let status = map
        .get("status")
        .map(parse_status_value)
        .transpose()?
        .unwrap_or(ChatMessageStatus::Final);

    let content_value = map
        .get("content")
        .or_else(|| map.get("markdown"))
        .ok_or_else(|| "missing content".to_string())?;
    let content = parse_content_value(content_value)?;

    Ok(ChatMessage {
        id: ChatMessageId(id),
        sender,
        timestamp,
        status,
        content,
    })
}

fn parse_sender_value(value: &ComponentValue) -> Result<ChatSender, String> {
    match value {
        ComponentValue::String(raw) => parse_sender_string(raw),
        ComponentValue::Map(map) => {
            if let Some(name) = map.get("tool").and_then(ComponentValue::as_str) {
                return Ok(ChatSender::Tool(name.to_string()));
            }
            if let Some(name) = map.get("custom").and_then(ComponentValue::as_str) {
                return Ok(ChatSender::Custom(name.to_string()));
            }
            Err("sender map must contain 'tool' or 'custom'".to_string())
        }
        other => Err(format!("sender must be string or map, got {other:?}")),
    }
}

fn parse_sender_string(raw: &str) -> Result<ChatSender, String> {
    let raw = raw.trim();
    let lower = raw.to_ascii_lowercase();
    match lower.as_str() {
        "user" => Ok(ChatSender::User),
        "assistant" => Ok(ChatSender::Assistant),
        "system" => Ok(ChatSender::System),
        _ => {
            if let Some(rest) = raw.strip_prefix("tool:") {
                return Ok(ChatSender::Tool(rest.trim().to_string()));
            }
            if let Some(rest) = raw.strip_prefix("custom:") {
                return Ok(ChatSender::Custom(rest.trim().to_string()));
            }
            Err(format!("unknown sender '{raw}'"))
        }
    }
}

fn parse_status_value(value: &ComponentValue) -> Result<ChatMessageStatus, String> {
    match value {
        ComponentValue::String(raw) => parse_status_string(raw),
        ComponentValue::Map(map) => {
            if let Some(reason) = map.get("failed").and_then(ComponentValue::as_str) {
                return Ok(ChatMessageStatus::Failed(reason.to_string()));
            }
            Err("status map must contain 'failed'".to_string())
        }
        other => Err(format!("status must be string or map, got {other:?}")),
    }
}

fn parse_status_string(raw: &str) -> Result<ChatMessageStatus, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "final" => Ok(ChatMessageStatus::Final),
        "inprogress" | "in_progress" => Ok(ChatMessageStatus::InProgress),
        _ => Err(format!("unknown status '{raw}'")),
    }
}

fn parse_content_value(value: &ComponentValue) -> Result<ChatMessageContent, String> {
    match value {
        ComponentValue::String(markdown) => Ok(ChatMessageContent::Text {
            markdown: markdown.clone(),
        }),
        ComponentValue::Map(map) => {
            if let Some(markdown) = map.get("markdown").and_then(ComponentValue::as_str) {
                return Ok(ChatMessageContent::Text {
                    markdown: markdown.to_string(),
                });
            }
            if let Some(text) = map.get("text").and_then(ComponentValue::as_str) {
                return Ok(ChatMessageContent::Text {
                    markdown: text.to_string(),
                });
            }
            if let Some(ComponentValue::Map(file)) = map.get("file") {
                let name = file
                    .get("name")
                    .and_then(ComponentValue::as_str)
                    .ok_or_else(|| "file missing name".to_string())?
                    .to_string();
                let url = file
                    .get("url")
                    .and_then(ComponentValue::as_str)
                    .map(|s| s.to_string());
                return Ok(ChatMessageContent::File { name, url });
            }
            if let Some(ComponentValue::Map(tool)) = map.get("tool_call") {
                let name = tool
                    .get("name")
                    .and_then(ComponentValue::as_str)
                    .ok_or_else(|| "tool_call missing name".to_string())?
                    .to_string();
                let status = tool
                    .get("status")
                    .map(parse_tool_status_value)
                    .transpose()?
                    .unwrap_or(ChatToolCallStatus::Running);
                let output = tool
                    .get("output")
                    .and_then(ComponentValue::as_str)
                    .unwrap_or_default()
                    .to_string();
                return Ok(ChatMessageContent::ToolCall {
                    name,
                    status,
                    output,
                });
            }
            if let Some(ComponentValue::Map(artifact)) = map.get("artifact") {
                let kind = artifact
                    .get("kind")
                    .map(parse_artifact_kind_value)
                    .transpose()?
                    .unwrap_or(ArtifactKind::File);
                let anchor = artifact
                    .get("anchor")
                    .or_else(|| artifact.get("id"))
                    .map(parse_artifact_id_value)
                    .transpose()?
                    .ok_or_else(|| "artifact missing anchor".to_string())?;
                let title = artifact
                    .get("title")
                    .and_then(ComponentValue::as_str)
                    .ok_or_else(|| "artifact missing title".to_string())?
                    .to_string();
                return Ok(ChatMessageContent::Artifact {
                    kind,
                    anchor,
                    title,
                });
            }
            Err(
                "content must contain 'markdown'/'text', 'file', 'tool_call', or 'artifact'"
                    .to_string(),
            )
        }
        other => Err(format!("content must be string or map, got {other:?}")),
    }
}

fn parse_artifact_id_value(value: &ComponentValue) -> Result<ArtifactId, String> {
    match value {
        ComponentValue::String(raw) => Ok(ArtifactId::new(raw.clone())),
        ComponentValue::U64(raw) => Ok(ArtifactId::from(*raw)),
        ComponentValue::I64(raw) if *raw >= 0 => Ok(ArtifactId::from(*raw as u64)),
        other => Err(format!(
            "artifact anchor must be string or u64, got {other:?}"
        )),
    }
}

fn parse_artifact_kind_value(value: &ComponentValue) -> Result<ArtifactKind, String> {
    match value {
        ComponentValue::String(raw) => parse_artifact_kind_string(raw),
        other => Err(format!("artifact kind must be string, got {other:?}")),
    }
}

fn parse_artifact_kind_string(raw: &str) -> Result<ArtifactKind, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "code" => Ok(ArtifactKind::Code),
        "diff" => Ok(ArtifactKind::Diff),
        "file" => Ok(ArtifactKind::File),
        _ => Err(format!("unknown artifact kind '{raw}'")),
    }
}

fn parse_tool_status_value(value: &ComponentValue) -> Result<ChatToolCallStatus, String> {
    match value {
        ComponentValue::String(raw) => parse_tool_status_string(raw),
        other => Err(format!("tool_call status must be string, got {other:?}")),
    }
}

fn parse_tool_status_string(raw: &str) -> Result<ChatToolCallStatus, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "running" => Ok(ChatToolCallStatus::Running),
        "done" => Ok(ChatToolCallStatus::Done),
        "error" => Ok(ChatToolCallStatus::Error),
        _ => Err(format!("unknown tool_call status '{raw}'")),
    }
}

pub fn register_chat_message_list(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = chat_message_list_schema();

    registry.register(schema, move |spec, _registry| {
        let messages = match spec.props.get("messages") {
            Some(value) => parse_messages_value(value)
                .map_err(|reason| invalid_prop(spec, "messages", &reason, value))?,
            None => Vec::new(),
        };

        let messages: Binding<Vec<ChatMessage>> = messages.into();
        let mut view = ChatMessageList::new(messages);

        if let Some(spacing) = prop_u16(spec, "spacing")? {
            view = view.spacing(spacing);
        }

        if let Some(value) = spec.props.get("padding") {
            let padding =
                <atto_ui::composable::EdgeInsets as ComponentValueCodec>::from_component_value(
                    value.clone(),
                    "padding",
                )
                .map_err(|err| invalid_prop_reason(spec, "padding", format!("{err:?}")))?;
            view = view.padding_insets(padding);
        }

        if let Some(width) = prop_u16(spec, "wrap_width")? {
            view = view.wrap_width(width);
        }

        if let Some(show) = prop_bool(spec, "show_timestamps")? {
            view = view.show_timestamps(show);
        }

        if let Some(enabled) = prop_bool(spec, "auto_scroll")? {
            view = view.auto_scroll(enabled);
        }

        if let Some(cb) = event_handle(spec, "load_more", callbacks.clone()) {
            view = view.on_load_more(move || cb.emit());
        }

        if let Some(cb) = event_handle(spec, "open_artifact", callbacks.clone()) {
            view = view.on_open_artifact(move |artifact_id| {
                cb.emit_with(Some(ComponentValue::String(artifact_id.to_string())));
            });
        }

        Ok(wrap_with_id(spec, Box::new(view)))
    });
}

pub fn register_chat_input_panel(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = chat_input_panel_schema();

    registry.register(schema, move |spec, _registry| {
        let handle = ChatInputHandle::new();

        if let Some(value) = spec.props.get("mode") {
            let mode = parse_chat_input_mode_value(value).map_err(|reason| {
                invalid_prop_reason(spec, "mode", format!("{reason}; got {value:?}"))
            })?;
            handle.set_mode(mode);
        }

        if let Some(draft) = prop_string(spec, "draft")? {
            handle.draft_binding().set(draft);
        }

        if let Some(custom) = prop_string(spec, "custom")? {
            handle.custom_binding().set(custom);
        }

        if let Some(history) = prop_vec_string(spec, "history")? {
            handle.history_binding().set(history);
        }

        if let Some(selection) = prop_usize(spec, "selection")? {
            handle.selection_binding().set(selection);
        }

        if let Some(enabled) = prop_bool(spec, "enabled")? {
            handle.enabled_binding().set(enabled);
        }

        if let Some(clear) = prop_bool(spec, "clear_on_submit")? {
            handle.clear_on_submit_binding().set(clear);
        }

        let mut panel = handle.panel();
        if let Some(cb) = event_handle(spec, "submit", callbacks.clone()) {
            panel = panel.on_submit(move |resp| {
                cb.emit_with(Some(chat_input_response_to_component_value(resp)));
            });
        }

        Ok(wrap_with_id(spec, Box::new(panel)))
    });
}

fn register_chat_extension(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    register_chat_message_list(registry, callbacks.clone());
    register_chat_input_panel(registry, callbacks);
}

/// 将 `atto-ui-chat` 的动态组件注册到 `atto-ui` 的全局动态组件注册表中。
///
/// 返回：
/// - `true`：本次注册成功
/// - `false`：已注册过（幂等）
pub fn register_runtime_components() -> bool {
    register_registry_extension("atto-ui-chat", register_chat_extension)
}

#[cfg(test)]
mod tests {
    use super::*;
    use atto_ui::ComponentSpec;

    #[test]
    fn chat_input_panel_dynamic_builds_and_updates_properties() {
        let callbacks = CallbackRegistry::new();
        let mut registry = ComponentRegistry::<Box<dyn Component>>::new();
        register_chat_input_panel(&mut registry, callbacks);

        let mut mode = BTreeMap::<String, ComponentValue>::new();
        mode.insert(
            "type".to_string(),
            ComponentValue::String("text".to_string()),
        );
        mode.insert(
            "title".to_string(),
            ComponentValue::String("Hello".to_string()),
        );
        mode.insert("placeholder".to_string(), ComponentValue::Null);
        mode.insert("height".to_string(), ComponentValue::U64(5));

        let spec = ComponentSpec::new("ChatInputPanel")
            .with_prop("mode", ComponentValue::Map(mode.clone()))
            .with_prop("draft", ComponentValue::String("hi".to_string()))
            .with_prop("enabled", ComponentValue::Bool(false));

        let mut view = registry.build(&spec).expect("build ChatInputPanel");
        assert_eq!(
            view.get_property("draft"),
            Some(ComponentValue::String("hi".to_string()))
        );
        assert_eq!(
            view.get_property("enabled"),
            Some(ComponentValue::Bool(false))
        );
        assert_eq!(view.get_property("mode"), Some(ComponentValue::Map(mode)));

        view.set_property("draft", ComponentValue::String("next".to_string()))
            .expect("set draft");
        assert_eq!(
            view.get_property("draft"),
            Some(ComponentValue::String("next".to_string()))
        );
    }

    #[test]
    fn chat_messages_round_trip_tool_call_content() {
        let messages = vec![ChatMessage::tool_call(
            ChatMessageId(42),
            "build",
            ChatToolCallStatus::Running,
            "cargo test",
        )];

        let value = messages_to_component_value(&messages);
        let parsed = parse_messages_value(&value).expect("parse messages");

        assert_eq!(parsed, messages);
    }

    #[test]
    fn chat_messages_round_trip_artifact_content() {
        let messages = vec![ChatMessage::artifact(
            ChatMessageId(43),
            ChatSender::Assistant,
            ArtifactKind::Diff,
            ArtifactId::new("diff-1"),
            "main.patch",
        )];

        let value = messages_to_component_value(&messages);
        let parsed = parse_messages_value(&value).expect("parse messages");

        assert_eq!(parsed, messages);
    }
}
