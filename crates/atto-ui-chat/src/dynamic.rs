use std::collections::BTreeMap;

use atto_ui::composable::Component;
use atto_ui::runtime::{
    component_schema, event_handle, invalid_prop, invalid_prop_reason, prop_bool, prop_string,
    prop_u16, prop_usize, prop_vec_string, register_registry_extension, wrap_with_id,
};
use atto_ui::{
    CallbackRegistry, ComponentPropertySchema, ComponentRegistry, ComponentSchema, ComponentValue,
    ComponentValueCodec, EventMeta, PropertyMeta, ValueType,
};

use crate::input::{
    chat_input_response_to_component_value, chat_mention_context_to_component_value,
    chat_slash_command_to_component_value, parse_chat_input_mode_value,
    parse_chat_mention_candidates_value, parse_chat_slash_commands_value,
};
use crate::{
    ApprovalAction, ApprovalDecision, ApprovalLevel, ApprovalOption, ApprovalRequest,
    ApprovalResolution, ArtifactBlock, ArtifactId, ArtifactKind, AttachmentBlock, ChatBlock,
    ChatBlockId, ChatError, ChatErrorKind, ChatInputHandle, ChatInputPanel, ChatMessage,
    ChatMessageId, ChatMessageList, ChatMessageMeta, ChatMessageStore, ChatRole, ChatTurnStatus,
    CompactBlock, CompactStatus, DiffBlock, DiffData, EditDecision, EditDecisionEvent,
    MessageAction, MessageActionKind, NoticeBlock, NoticeLevel, PlanBlock, PlanDecision,
    PlanDecisionEvent, PlanItem, StopReason, TaskBlock, TaskStatus, TaskTranscriptItem, TextBlock,
    ThinkingBlock, TodoBlock, TodoItem, TodoState, TokenUsage, ToolInput, ToolOutput,
    ToolResultBlock, ToolStatus, ToolUseBlock,
};

type ValueMap = BTreeMap<String, ComponentValue>;

impl ComponentPropertySchema for ChatMessageList {
    fn property_schema() -> Vec<PropertyMeta> {
        vec![
            PropertyMeta::new("messages", ValueType::List),
            PropertyMeta::new("spacing", ValueType::U64),
            PropertyMeta::new("padding", ValueType::Map),
            PropertyMeta::new("wrap_width", ValueType::U64),
            PropertyMeta::new("show_timestamps", ValueType::Bool),
            PropertyMeta::new("bubble_width_percent", ValueType::U64),
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
            PropertyMeta::new("slash_commands", ValueType::List),
            PropertyMeta::new("mention_candidates", ValueType::List),
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
        .with_event(EventMeta::new("approve").with_payload(ValueType::Map))
        .with_event(EventMeta::new("edit_decision").with_payload(ValueType::Map))
        .with_event(EventMeta::new("plan_decision").with_payload(ValueType::Map))
        .with_event(EventMeta::new("cancel").with_payload(ValueType::Map))
        .with_event(EventMeta::new("message_action").with_payload(ValueType::Map))
        .allow_children(false)
}

pub fn chat_input_panel_schema() -> ComponentSchema {
    component_schema::<ChatInputPanel>("ChatInputPanel")
        .with_event(EventMeta::new("submit").with_payload(ValueType::Map))
        .with_event(EventMeta::new("slash_command").with_payload(ValueType::Map))
        .with_event(EventMeta::new("mention_query").with_payload(ValueType::Map))
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
    let mut out = ValueMap::new();
    out.insert("id".to_string(), ComponentValue::U64(message.id.0));
    out.insert(
        "role".to_string(),
        ComponentValue::String(role_to_string(&message.role)),
    );
    out.insert("status".to_string(), status_to_value(&message.status));
    if let Some(meta) = meta_to_value(&message.meta) {
        out.insert("meta".to_string(), meta);
    }
    out.insert(
        "blocks".to_string(),
        ComponentValue::List(message.blocks.iter().map(block_to_value).collect()),
    );
    ComponentValue::Map(out)
}

fn role_to_string(role: &ChatRole) -> String {
    match role {
        ChatRole::User => "user".to_string(),
        ChatRole::Assistant => "assistant".to_string(),
        ChatRole::System => "system".to_string(),
        ChatRole::Custom(name) => format!("custom:{name}"),
    }
}

fn status_to_value(status: &ChatTurnStatus) -> ComponentValue {
    match status {
        ChatTurnStatus::Complete => ComponentValue::String("complete".to_string()),
        ChatTurnStatus::Streaming => ComponentValue::String("streaming".to_string()),
        ChatTurnStatus::Canceled => ComponentValue::String("canceled".to_string()),
        ChatTurnStatus::Failed(error) => {
            let mut map = ValueMap::new();
            map.insert("failed".to_string(), error_to_value(error));
            ComponentValue::Map(map)
        }
    }
}

fn error_to_value(error: &ChatError) -> ComponentValue {
    let mut map = ValueMap::new();
    map.insert(
        "kind".to_string(),
        ComponentValue::String(error_kind_to_string(&error.kind).to_string()),
    );
    map.insert(
        "message".to_string(),
        ComponentValue::String(error.message.clone()),
    );
    if let Some(detail) = &error.detail {
        map.insert("detail".to_string(), ComponentValue::String(detail.clone()));
    }
    ComponentValue::Map(map)
}

fn meta_to_value(meta: &ChatMessageMeta) -> Option<ComponentValue> {
    if meta.timestamp.is_none()
        && meta.model.is_none()
        && meta.usage.is_none()
        && meta.elapsed_ms.is_none()
        && meta.stop_reason.is_none()
    {
        return None;
    }

    let mut map = ValueMap::new();
    if let Some(timestamp) = &meta.timestamp {
        map.insert(
            "timestamp".to_string(),
            ComponentValue::String(timestamp.clone()),
        );
    }
    if let Some(model) = &meta.model {
        map.insert("model".to_string(), ComponentValue::String(model.clone()));
    }
    if let Some(usage) = &meta.usage {
        let mut usage_map = ValueMap::new();
        usage_map.insert("input".to_string(), ComponentValue::U64(usage.input));
        usage_map.insert("output".to_string(), ComponentValue::U64(usage.output));
        map.insert("usage".to_string(), ComponentValue::Map(usage_map));
    }
    if let Some(elapsed_ms) = meta.elapsed_ms {
        map.insert("elapsed_ms".to_string(), ComponentValue::U64(elapsed_ms));
    }
    if let Some(stop_reason) = &meta.stop_reason {
        map.insert(
            "stop_reason".to_string(),
            ComponentValue::String(stop_reason_to_string(stop_reason).to_string()),
        );
    }
    Some(ComponentValue::Map(map))
}

fn block_to_value(block: &ChatBlock) -> ComponentValue {
    match block {
        ChatBlock::Text(block) => {
            let mut map = block_base("text", block.id);
            map.insert(
                "markdown".to_string(),
                ComponentValue::String(block.markdown.clone()),
            );
            insert_bool_if_true(&mut map, "streaming", block.streaming);
            ComponentValue::Map(map)
        }
        ChatBlock::Thinking(block) => {
            let mut map = block_base("thinking", block.id);
            map.insert(
                "markdown".to_string(),
                ComponentValue::String(block.markdown.clone()),
            );
            insert_bool_if_true(&mut map, "streaming", block.streaming);
            map.insert(
                "collapsed".to_string(),
                ComponentValue::Bool(block.collapsed),
            );
            ComponentValue::Map(map)
        }
        ChatBlock::ToolUse(block) => {
            let mut map = block_base("tool_use", block.id);
            map.insert(
                "call_id".to_string(),
                ComponentValue::String(block.call_id.clone()),
            );
            map.insert(
                "name".to_string(),
                ComponentValue::String(block.name.clone()),
            );
            map.insert("input".to_string(), tool_input_to_value(&block.input));
            map.insert(
                "status".to_string(),
                ComponentValue::String(tool_status_to_string(&block.status).to_string()),
            );
            if let Some(approval) = &block.approval {
                map.insert("approval".to_string(), approval_to_value(approval));
            }
            insert_bool_if_true(&mut map, "collapsed", block.collapsed);
            ComponentValue::Map(map)
        }
        ChatBlock::ToolResult(block) => {
            let mut map = block_base("tool_result", block.id);
            map.insert(
                "call_id".to_string(),
                ComponentValue::String(block.call_id.clone()),
            );
            map.insert("ok".to_string(), ComponentValue::Bool(block.ok));
            if let Some(exit_code) = block.exit_code {
                map.insert(
                    "exit_code".to_string(),
                    ComponentValue::I64(i64::from(exit_code)),
                );
            }
            map.insert("output".to_string(), tool_output_to_value(&block.output));
            insert_bool_if_true(&mut map, "collapsed", block.collapsed);
            ComponentValue::Map(map)
        }
        ChatBlock::Diff(block) => {
            let mut map = block_base("diff", block.id);
            map.insert(
                "path".to_string(),
                ComponentValue::String(block.path.clone()),
            );
            map.insert(
                "diff".to_string(),
                ComponentValue::String(block.diff.unified.clone()),
            );
            map.insert(
                "decision".to_string(),
                ComponentValue::String(edit_decision_to_string(block.decision).to_string()),
            );
            ComponentValue::Map(map)
        }
        ChatBlock::Plan(block) => {
            let mut map = block_base("plan", block.id);
            map.insert(
                "items".to_string(),
                ComponentValue::List(block.items.iter().map(plan_item_to_value).collect()),
            );
            map.insert(
                "decision".to_string(),
                ComponentValue::String(plan_decision_to_string(block.decision).to_string()),
            );
            ComponentValue::Map(map)
        }
        ChatBlock::Task(block) => {
            let mut map = block_base("task", block.id);
            map.insert(
                "title".to_string(),
                ComponentValue::String(block.title.clone()),
            );
            map.insert(
                "status".to_string(),
                ComponentValue::String(task_status_to_string(block.status).to_string()),
            );
            map.insert(
                "summary".to_string(),
                ComponentValue::String(block.summary.clone()),
            );
            map.insert(
                "transcript".to_string(),
                ComponentValue::List(
                    block
                        .transcript
                        .iter()
                        .map(task_transcript_item_to_value)
                        .collect(),
                ),
            );
            map.insert(
                "collapsed".to_string(),
                ComponentValue::Bool(block.collapsed),
            );
            ComponentValue::Map(map)
        }
        ChatBlock::Todo(block) => {
            let mut map = block_base("todo", block.id);
            map.insert(
                "items".to_string(),
                ComponentValue::List(block.items.iter().map(todo_item_to_value).collect()),
            );
            ComponentValue::Map(map)
        }
        ChatBlock::Attachment(block) => {
            let mut map = block_base("attachment", block.id);
            map.insert(
                "name".to_string(),
                ComponentValue::String(block.name.clone()),
            );
            insert_optional_string(&mut map, "url", block.url.as_deref());
            insert_optional_string(&mut map, "mime", block.mime.as_deref());
            ComponentValue::Map(map)
        }
        ChatBlock::Notice(block) => {
            let mut map = block_base("notice", block.id);
            map.insert(
                "level".to_string(),
                ComponentValue::String(notice_level_to_string(block.level).to_string()),
            );
            map.insert(
                "text".to_string(),
                ComponentValue::String(block.text.clone()),
            );
            ComponentValue::Map(map)
        }
        ChatBlock::Compact(block) => {
            let mut map = block_base("compact", block.id);
            map.insert(
                "status".to_string(),
                ComponentValue::String(block.status.as_str().to_string()),
            );
            insert_optional_u64(&mut map, "before_tokens", block.before_tokens);
            insert_optional_u64(&mut map, "after_tokens", block.after_tokens);
            map.insert(
                "summary".to_string(),
                ComponentValue::String(block.summary.clone()),
            );
            ComponentValue::Map(map)
        }
        ChatBlock::Artifact(block) => {
            let mut map = block_base("artifact", block.id);
            map.insert(
                "kind".to_string(),
                ComponentValue::String(block.kind.as_str().to_string()),
            );
            map.insert(
                "anchor".to_string(),
                ComponentValue::String(block.anchor.to_string()),
            );
            map.insert(
                "title".to_string(),
                ComponentValue::String(block.title.clone()),
            );
            ComponentValue::Map(map)
        }
    }
}

fn block_base(kind: &str, id: ChatBlockId) -> ValueMap {
    let mut map = ValueMap::new();
    map.insert("type".to_string(), ComponentValue::String(kind.to_string()));
    map.insert("block_id".to_string(), ComponentValue::U64(id.0));
    map
}

fn tool_input_to_value(input: &ToolInput) -> ComponentValue {
    let mut map = ValueMap::new();
    match input {
        ToolInput::Text(text) => {
            map.insert("text".to_string(), ComponentValue::String(text.clone()));
        }
        ToolInput::Json(value) => {
            map.insert("json".to_string(), value.clone());
        }
    }
    ComponentValue::Map(map)
}

fn tool_output_to_value(output: &ToolOutput) -> ComponentValue {
    let mut map = ValueMap::new();
    match output {
        ToolOutput::Ansi(output) => {
            map.insert("ansi".to_string(), ComponentValue::String(output.clone()));
        }
        ToolOutput::Markdown(output) => {
            map.insert(
                "markdown".to_string(),
                ComponentValue::String(output.clone()),
            );
        }
        ToolOutput::Diff(diff) => {
            map.insert(
                "diff".to_string(),
                ComponentValue::String(diff.unified.clone()),
            );
        }
    }
    ComponentValue::Map(map)
}

fn approval_to_value(approval: &ApprovalRequest) -> ComponentValue {
    let mut map = ValueMap::new();
    map.insert(
        "id".to_string(),
        ComponentValue::String(approval.id.clone()),
    );
    map.insert(
        "prompt".to_string(),
        ComponentValue::String(approval.prompt.clone()),
    );
    map.insert(
        "options".to_string(),
        ComponentValue::List(
            approval
                .options
                .iter()
                .map(approval_option_to_value)
                .collect(),
        ),
    );
    insert_optional_string(
        &mut map,
        "resolved",
        approval
            .resolved
            .as_ref()
            .map(|resolved| resolved.option_id.as_str()),
    );
    if let Some(resolved) = &approval.resolved {
        map.insert(
            "resolved_action".to_string(),
            ComponentValue::String(resolved.action.as_str().to_string()),
        );
        map.insert(
            "resolved_level".to_string(),
            ComponentValue::String(resolved.level.as_str().to_string()),
        );
    }
    ComponentValue::Map(map)
}

fn approval_option_to_value(option: &ApprovalOption) -> ComponentValue {
    let mut map = ValueMap::new();
    map.insert("id".to_string(), ComponentValue::String(option.id.clone()));
    map.insert(
        "label".to_string(),
        ComponentValue::String(option.label.clone()),
    );
    map.insert(
        "action".to_string(),
        ComponentValue::String(option.action.as_str().to_string()),
    );
    map.insert(
        "level".to_string(),
        ComponentValue::String(option.level.as_str().to_string()),
    );
    ComponentValue::Map(map)
}

fn approval_decision_to_value(decision: ApprovalDecision) -> ComponentValue {
    let mut map = ValueMap::new();
    map.insert(
        "message_id".to_string(),
        ComponentValue::U64(decision.message_id.0),
    );
    map.insert(
        "block_id".to_string(),
        ComponentValue::U64(decision.block_id.0),
    );
    map.insert(
        "approval_id".to_string(),
        ComponentValue::String(decision.approval_id),
    );
    map.insert(
        "option_id".to_string(),
        ComponentValue::String(decision.option_id),
    );
    map.insert(
        "action".to_string(),
        ComponentValue::String(decision.action.as_str().to_string()),
    );
    map.insert(
        "level".to_string(),
        ComponentValue::String(decision.level.as_str().to_string()),
    );
    ComponentValue::Map(map)
}

fn edit_decision_event_to_value(event: EditDecisionEvent) -> ComponentValue {
    let mut map = ValueMap::new();
    map.insert(
        "message_id".to_string(),
        ComponentValue::U64(event.message_id.0),
    );
    map.insert(
        "block_id".to_string(),
        ComponentValue::U64(event.block_id.0),
    );
    map.insert(
        "decision".to_string(),
        ComponentValue::String(edit_decision_to_string(event.decision).to_string()),
    );
    ComponentValue::Map(map)
}

fn plan_decision_event_to_value(event: PlanDecisionEvent) -> ComponentValue {
    let mut map = ValueMap::new();
    map.insert(
        "message_id".to_string(),
        ComponentValue::U64(event.message_id.0),
    );
    map.insert(
        "block_id".to_string(),
        ComponentValue::U64(event.block_id.0),
    );
    map.insert(
        "decision".to_string(),
        ComponentValue::String(plan_decision_to_string(event.decision).to_string()),
    );
    ComponentValue::Map(map)
}

fn cancel_event_to_value(message_id: ChatMessageId) -> ComponentValue {
    let mut map = ValueMap::new();
    map.insert("message_id".to_string(), ComponentValue::U64(message_id.0));
    ComponentValue::Map(map)
}

fn message_action_to_value(action: MessageAction) -> ComponentValue {
    let mut map = ValueMap::new();
    map.insert(
        "message_id".to_string(),
        ComponentValue::U64(action.message_id.0),
    );
    match action.kind {
        MessageActionKind::Copy => {
            map.insert(
                "kind".to_string(),
                ComponentValue::String("copy".to_string()),
            );
        }
        MessageActionKind::Retry => {
            map.insert(
                "kind".to_string(),
                ComponentValue::String("retry".to_string()),
            );
        }
        MessageActionKind::Regenerate => {
            map.insert(
                "kind".to_string(),
                ComponentValue::String("regenerate".to_string()),
            );
        }
        MessageActionKind::EditUser => {
            map.insert(
                "kind".to_string(),
                ComponentValue::String("edit_user".to_string()),
            );
        }
        MessageActionKind::CopyBlock(block_id) => {
            map.insert(
                "kind".to_string(),
                ComponentValue::String("copy_block".to_string()),
            );
            map.insert("block_id".to_string(), ComponentValue::U64(block_id.0));
        }
    }
    ComponentValue::Map(map)
}

fn todo_item_to_value(item: &TodoItem) -> ComponentValue {
    let mut map = ValueMap::new();
    map.insert(
        "text".to_string(),
        ComponentValue::String(item.text.clone()),
    );
    map.insert(
        "state".to_string(),
        ComponentValue::String(todo_state_to_string(item.state).to_string()),
    );
    ComponentValue::Map(map)
}

fn plan_item_to_value(item: &PlanItem) -> ComponentValue {
    let mut map = ValueMap::new();
    map.insert(
        "text".to_string(),
        ComponentValue::String(item.text.clone()),
    );
    ComponentValue::Map(map)
}

fn task_transcript_item_to_value(item: &TaskTranscriptItem) -> ComponentValue {
    let mut map = ValueMap::new();
    map.insert(
        "role".to_string(),
        ComponentValue::String(role_to_string(&item.role)),
    );
    map.insert(
        "blocks".to_string(),
        ComponentValue::List(item.blocks.iter().map(block_to_value).collect()),
    );
    ComponentValue::Map(map)
}

fn insert_bool_if_true(map: &mut ValueMap, key: &str, value: bool) {
    if value {
        map.insert(key.to_string(), ComponentValue::Bool(true));
    }
}

fn insert_optional_string(map: &mut ValueMap, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        map.insert(key.to_string(), ComponentValue::String(value.to_string()));
    }
}

fn insert_optional_u64(map: &mut ValueMap, key: &str, value: Option<u64>) {
    if let Some(value) = value {
        map.insert(key.to_string(), ComponentValue::U64(value));
    }
}

fn error_kind_to_string(kind: &ChatErrorKind) -> &'static str {
    match kind {
        ChatErrorKind::Api => "api",
        ChatErrorKind::Tool => "tool",
        ChatErrorKind::RateLimit => "rate_limit",
        ChatErrorKind::Refusal => "refusal",
        ChatErrorKind::Network => "network",
        ChatErrorKind::Other => "other",
    }
}

fn stop_reason_to_string(reason: &StopReason) -> &'static str {
    match reason {
        StopReason::EndTurn => "end_turn",
        StopReason::MaxTokens => "max_tokens",
        StopReason::ToolUse => "tool_use",
        StopReason::StopSequence => "stop_sequence",
        StopReason::Refusal => "refusal",
    }
}

fn edit_decision_to_string(decision: EditDecision) -> &'static str {
    match decision {
        EditDecision::Pending => "pending",
        EditDecision::Accepted => "accepted",
        EditDecision::Rejected => "rejected",
    }
}

fn plan_decision_to_string(decision: PlanDecision) -> &'static str {
    match decision {
        PlanDecision::Pending => "pending",
        PlanDecision::Accepted => "accepted",
        PlanDecision::Rejected => "rejected",
    }
}

fn task_status_to_string(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Complete => "complete",
        TaskStatus::Failed => "failed",
        TaskStatus::Canceled => "canceled",
    }
}

fn todo_state_to_string(state: TodoState) -> &'static str {
    match state {
        TodoState::Pending => "pending",
        TodoState::InProgress => "in_progress",
        TodoState::Done => "done",
    }
}

fn notice_level_to_string(level: NoticeLevel) -> &'static str {
    match level {
        NoticeLevel::Info => "info",
        NoticeLevel::Warning => "warning",
        NoticeLevel::Error => "error",
    }
}

fn tool_status_to_string(status: &ToolStatus) -> &'static str {
    match status {
        ToolStatus::Pending => "pending",
        ToolStatus::Running => "running",
        ToolStatus::Done => "done",
        ToolStatus::Error => "error",
        ToolStatus::Canceled => "canceled",
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

    let role = map
        .get("role")
        .or_else(|| map.get("sender"))
        .map(parse_role_value)
        .transpose()?
        .unwrap_or(ChatRole::Assistant);

    let status = map
        .get("status")
        .map(parse_status_value)
        .transpose()?
        .unwrap_or(ChatTurnStatus::Complete);

    if let Some(blocks_value) = map.get("blocks") {
        let meta = map
            .get("meta")
            .map(parse_meta_value)
            .transpose()?
            .unwrap_or_default();
        let blocks = parse_blocks_value(blocks_value)?;
        return Ok(ChatMessage {
            id: ChatMessageId(id),
            role,
            blocks,
            status,
            meta,
        });
    }

    let timestamp = map
        .get("timestamp")
        .and_then(ComponentValue::as_str)
        .map(|s| s.to_string());
    let content_value = legacy_content_value(map)?;
    let blocks = parse_content_value(ChatMessageId(id), &content_value)?;

    Ok(ChatMessage {
        id: ChatMessageId(id),
        role,
        blocks,
        status,
        meta: ChatMessageMeta {
            timestamp,
            ..ChatMessageMeta::default()
        },
    })
}

fn parse_meta_value(value: &ComponentValue) -> Result<ChatMessageMeta, String> {
    let map = expect_map(value, "meta")?;
    let timestamp = optional_string_field(map, "timestamp", "meta")?;
    let model = optional_string_field(map, "model", "meta")?;
    let usage = map.get("usage").map(parse_token_usage_value).transpose()?;
    let elapsed_ms = map.get("elapsed_ms").map(required_u64_value).transpose()?;
    let stop_reason = map
        .get("stop_reason")
        .map(parse_stop_reason_value)
        .transpose()?;

    Ok(ChatMessageMeta {
        timestamp,
        model,
        usage,
        elapsed_ms,
        stop_reason,
    })
}

fn parse_token_usage_value(value: &ComponentValue) -> Result<TokenUsage, String> {
    let map = expect_map(value, "usage")?;
    Ok(TokenUsage {
        input: required_u64_field(map, "input", "usage")?,
        output: required_u64_field(map, "output", "usage")?,
    })
}

fn parse_blocks_value(value: &ComponentValue) -> Result<Vec<ChatBlock>, String> {
    let ComponentValue::List(items) = value else {
        return Err(format!("blocks must be list, got {value:?}"));
    };
    items.iter().map(parse_block_value).collect()
}

fn parse_block_value(value: &ComponentValue) -> Result<ChatBlock, String> {
    let map = expect_map(value, "block")?;
    let kind = required_string_field(map, "type", "block")?;
    let id = ChatBlockId(required_u64_field(map, "block_id", "block")?);

    match kind.as_str() {
        "text" => Ok(ChatBlock::Text(TextBlock {
            id,
            markdown: required_string_field(map, "markdown", "text block")?,
            streaming: optional_bool_field(map, "streaming", "text block")?.unwrap_or(false),
        })),
        "thinking" => Ok(ChatBlock::Thinking(ThinkingBlock {
            id,
            markdown: required_string_field(map, "markdown", "thinking block")?,
            streaming: optional_bool_field(map, "streaming", "thinking block")?.unwrap_or(false),
            collapsed: optional_bool_field(map, "collapsed", "thinking block")?.unwrap_or(true),
        })),
        "tool_use" => Ok(ChatBlock::ToolUse(ToolUseBlock {
            id,
            call_id: required_string_field(map, "call_id", "tool_use block")?,
            name: required_string_field(map, "name", "tool_use block")?,
            input: map
                .get("input")
                .map(parse_tool_input_value)
                .transpose()?
                .unwrap_or_else(|| ToolInput::Text(String::new())),
            status: map
                .get("status")
                .map(parse_tool_status_value)
                .transpose()?
                .unwrap_or(ToolStatus::Pending),
            approval: map.get("approval").map(parse_approval_value).transpose()?,
            collapsed: optional_bool_field(map, "collapsed", "tool_use block")?.unwrap_or(false),
        })),
        "tool_result" => Ok(ChatBlock::ToolResult(ToolResultBlock {
            id,
            call_id: required_string_field(map, "call_id", "tool_result block")?,
            ok: required_bool_field(map, "ok", "tool_result block")?,
            exit_code: map.get("exit_code").map(parse_i32_value).transpose()?,
            output: map
                .get("output")
                .map(parse_tool_output_value)
                .transpose()?
                .unwrap_or_else(|| ToolOutput::Ansi(String::new())),
            collapsed: optional_bool_field(map, "collapsed", "tool_result block")?.unwrap_or(false),
        })),
        "diff" => Ok(ChatBlock::Diff(DiffBlock {
            id,
            path: required_string_field(map, "path", "diff block")?,
            diff: DiffData {
                unified: required_string_field(map, "diff", "diff block")?,
            },
            decision: map
                .get("decision")
                .map(parse_edit_decision_value)
                .transpose()?
                .unwrap_or(EditDecision::Pending),
        })),
        "plan" => Ok(ChatBlock::Plan(PlanBlock {
            id,
            items: map
                .get("items")
                .map(parse_plan_items_value)
                .transpose()?
                .unwrap_or_default(),
            decision: map
                .get("decision")
                .map(parse_plan_decision_value)
                .transpose()?
                .unwrap_or(PlanDecision::Pending),
        })),
        "task" => Ok(ChatBlock::Task(TaskBlock {
            id,
            title: required_string_field(map, "title", "task block")?,
            status: map
                .get("status")
                .map(parse_task_status_value)
                .transpose()?
                .unwrap_or(TaskStatus::Pending),
            summary: optional_string_field(map, "summary", "task block")?.unwrap_or_default(),
            transcript: map
                .get("transcript")
                .map(parse_task_transcript_value)
                .transpose()?
                .unwrap_or_default(),
            collapsed: optional_bool_field(map, "collapsed", "task block")?.unwrap_or(true),
        })),
        "todo" => Ok(ChatBlock::Todo(TodoBlock {
            id,
            items: map
                .get("items")
                .map(parse_todo_items_value)
                .transpose()?
                .unwrap_or_default(),
        })),
        "attachment" => Ok(ChatBlock::Attachment(AttachmentBlock {
            id,
            name: required_string_field(map, "name", "attachment block")?,
            url: optional_string_field(map, "url", "attachment block")?,
            mime: optional_string_field(map, "mime", "attachment block")?,
        })),
        "notice" => Ok(ChatBlock::Notice(NoticeBlock {
            id,
            level: map
                .get("level")
                .map(parse_notice_level_value)
                .transpose()?
                .unwrap_or(NoticeLevel::Info),
            text: required_string_field(map, "text", "notice block")?,
        })),
        "compact" => Ok(ChatBlock::Compact(CompactBlock {
            id,
            status: map
                .get("status")
                .map(parse_compact_status_value)
                .transpose()?
                .unwrap_or(CompactStatus::Complete),
            before_tokens: optional_u64_field(map, "before_tokens", "compact block")?,
            after_tokens: optional_u64_field(map, "after_tokens", "compact block")?,
            summary: optional_string_field(map, "summary", "compact block")?.unwrap_or_default(),
        })),
        "artifact" => Ok(ChatBlock::Artifact(ArtifactBlock {
            id,
            kind: map
                .get("kind")
                .map(parse_artifact_kind_value)
                .transpose()?
                .unwrap_or(ArtifactKind::File),
            anchor: map
                .get("anchor")
                .map(parse_artifact_id_value)
                .transpose()?
                .ok_or_else(|| "artifact block missing anchor".to_string())?,
            title: required_string_field(map, "title", "artifact block")?,
        })),
        other => Err(format!("unknown block type '{other}'")),
    }
}

fn legacy_content_value(map: &ValueMap) -> Result<ComponentValue, String> {
    if let Some(value) = map.get("content") {
        return Ok(value.clone());
    }
    if let Some(value) = map.get("markdown") {
        return Ok(value.clone());
    }
    for key in ["tool_call", "file", "artifact"] {
        if let Some(value) = map.get(key) {
            let mut wrapper = ValueMap::new();
            wrapper.insert(key.to_string(), value.clone());
            return Ok(ComponentValue::Map(wrapper));
        }
    }
    Err("missing content".to_string())
}

fn parse_role_value(value: &ComponentValue) -> Result<ChatRole, String> {
    match value {
        ComponentValue::String(raw) => parse_role_string(raw),
        ComponentValue::Map(map) => {
            if map.contains_key("tool") {
                return Ok(ChatRole::Assistant);
            }
            if let Some(name) = map.get("custom").and_then(ComponentValue::as_str) {
                return Ok(ChatRole::Custom(name.to_string()));
            }
            Err("role map must contain 'tool' or 'custom'".to_string())
        }
        other => Err(format!("role must be string or map, got {other:?}")),
    }
}

fn parse_role_string(raw: &str) -> Result<ChatRole, String> {
    let raw = raw.trim();
    let lower = raw.to_ascii_lowercase();
    match lower.as_str() {
        "user" => Ok(ChatRole::User),
        "assistant" => Ok(ChatRole::Assistant),
        "system" => Ok(ChatRole::System),
        _ => {
            if raw.strip_prefix("tool:").is_some() {
                return Ok(ChatRole::Assistant);
            }
            if let Some(rest) = raw.strip_prefix("custom:") {
                return Ok(ChatRole::Custom(rest.trim().to_string()));
            }
            Err(format!("unknown role '{raw}'"))
        }
    }
}

fn parse_status_value(value: &ComponentValue) -> Result<ChatTurnStatus, String> {
    match value {
        ComponentValue::String(raw) => parse_status_string(raw),
        ComponentValue::Map(map) => {
            if let Some(failed) = map.get("failed") {
                return Ok(ChatTurnStatus::Failed(parse_error_value(failed)?));
            }
            Err("status map must contain 'failed'".to_string())
        }
        other => Err(format!("status must be string or map, got {other:?}")),
    }
}

fn parse_error_value(value: &ComponentValue) -> Result<ChatError, String> {
    match value {
        ComponentValue::String(raw) => Ok(ChatError::new(ChatErrorKind::Other, raw.clone())),
        ComponentValue::Map(map) => Ok(ChatError {
            kind: map
                .get("kind")
                .map(parse_error_kind_value)
                .transpose()?
                .unwrap_or(ChatErrorKind::Other),
            message: required_string_field(map, "message", "failed status")?,
            detail: optional_string_field(map, "detail", "failed status")?,
        }),
        other => Err(format!(
            "failed status must be string or map, got {other:?}"
        )),
    }
}

fn parse_status_string(raw: &str) -> Result<ChatTurnStatus, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "final" | "complete" => Ok(ChatTurnStatus::Complete),
        "inprogress" | "in_progress" | "streaming" => Ok(ChatTurnStatus::Streaming),
        "canceled" | "cancelled" => Ok(ChatTurnStatus::Canceled),
        _ => Err(format!("unknown status '{raw}'")),
    }
}

fn parse_content_value(
    message_id: ChatMessageId,
    value: &ComponentValue,
) -> Result<Vec<ChatBlock>, String> {
    match value {
        ComponentValue::String(markdown) => Ok(vec![ChatBlock::Text(TextBlock {
            id: legacy_block_id(message_id, 0),
            markdown: markdown.clone(),
            streaming: false,
        })]),
        ComponentValue::Map(map) => {
            if let Some(markdown) = map.get("markdown").and_then(ComponentValue::as_str) {
                return Ok(vec![ChatBlock::Text(TextBlock {
                    id: legacy_block_id(message_id, 0),
                    markdown: markdown.to_string(),
                    streaming: false,
                })]);
            }
            if let Some(text) = map.get("text").and_then(ComponentValue::as_str) {
                return Ok(vec![ChatBlock::Text(TextBlock {
                    id: legacy_block_id(message_id, 0),
                    markdown: text.to_string(),
                    streaming: false,
                })]);
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
                return Ok(vec![ChatBlock::Attachment(AttachmentBlock {
                    id: legacy_block_id(message_id, 0),
                    name,
                    url,
                    mime: None,
                })]);
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
                    .unwrap_or(ToolStatus::Running);
                let output = tool
                    .get("output")
                    .and_then(ComponentValue::as_str)
                    .unwrap_or_default()
                    .to_string();
                let call_id = format!("tool-{}", message_id.0);
                let mut blocks = vec![ChatBlock::ToolUse(ToolUseBlock {
                    id: legacy_block_id(message_id, 0),
                    call_id: call_id.clone(),
                    name,
                    input: ToolInput::Text(String::new()),
                    status,
                    approval: None,
                    collapsed: false,
                })];
                if !output.is_empty() {
                    blocks.push(ChatBlock::ToolResult(ToolResultBlock {
                        id: legacy_block_id(message_id, 1),
                        call_id,
                        ok: status != ToolStatus::Error,
                        exit_code: None,
                        output: ToolOutput::Ansi(output),
                        collapsed: false,
                    }));
                }
                return Ok(blocks);
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
                return Ok(vec![ChatBlock::Artifact(ArtifactBlock {
                    id: legacy_block_id(message_id, 0),
                    kind,
                    anchor,
                    title,
                })]);
            }
            Err(
                "content must contain 'markdown'/'text', 'file', 'tool_call', or 'artifact'"
                    .to_string(),
            )
        }
        other => Err(format!("content must be string or map, got {other:?}")),
    }
}

fn parse_tool_input_value(value: &ComponentValue) -> Result<ToolInput, String> {
    let map = expect_map(value, "tool input")?;
    if let Some(value) = map.get("text") {
        return match value {
            ComponentValue::String(text) => Ok(ToolInput::Text(text.clone())),
            other => Err(format!("tool input text must be string, got {other:?}")),
        };
    }
    if let Some(value) = map.get("json") {
        return Ok(ToolInput::Json(value.clone()));
    }
    Err("tool input must contain 'text' or 'json'".to_string())
}

fn parse_tool_output_value(value: &ComponentValue) -> Result<ToolOutput, String> {
    let map = expect_map(value, "tool output")?;
    if let Some(value) = map.get("ansi") {
        return match value {
            ComponentValue::String(output) => Ok(ToolOutput::Ansi(output.clone())),
            other => Err(format!("tool output ansi must be string, got {other:?}")),
        };
    }
    if let Some(value) = map.get("markdown") {
        return match value {
            ComponentValue::String(output) => Ok(ToolOutput::Markdown(output.clone())),
            other => Err(format!(
                "tool output markdown must be string, got {other:?}"
            )),
        };
    }
    if let Some(value) = map.get("diff") {
        return match value {
            ComponentValue::String(output) => Ok(ToolOutput::Diff(DiffData {
                unified: output.clone(),
            })),
            other => Err(format!("tool output diff must be string, got {other:?}")),
        };
    }
    Err("tool output must contain 'ansi', 'markdown', or 'diff'".to_string())
}

fn parse_approval_value(value: &ComponentValue) -> Result<ApprovalRequest, String> {
    let map = expect_map(value, "approval")?;
    let options = map
        .get("options")
        .map(parse_approval_options_value)
        .transpose()?
        .unwrap_or_default();
    let resolved_action = map
        .get("resolved_action")
        .map(parse_approval_action_value)
        .transpose()?;
    let resolved_level = map
        .get("resolved_level")
        .map(parse_approval_level_value)
        .transpose()?;
    let resolved = optional_string_field(map, "resolved", "approval")?.map(|option_id| {
        let option = options.iter().find(|option| option.id == option_id);
        ApprovalResolution {
            option_id,
            action: resolved_action
                .or_else(|| option.map(|option| option.action))
                .unwrap_or_default(),
            level: resolved_level
                .or_else(|| option.map(|option| option.level))
                .unwrap_or_default(),
        }
    });
    Ok(ApprovalRequest {
        id: required_string_field(map, "id", "approval")?,
        prompt: required_string_field(map, "prompt", "approval")?,
        options,
        resolved,
    })
}

fn parse_approval_options_value(value: &ComponentValue) -> Result<Vec<ApprovalOption>, String> {
    let ComponentValue::List(items) = value else {
        return Err(format!("approval options must be list, got {value:?}"));
    };
    items.iter().map(parse_approval_option_value).collect()
}

fn parse_approval_option_value(value: &ComponentValue) -> Result<ApprovalOption, String> {
    let map = expect_map(value, "approval option")?;
    let id = required_string_field(map, "id", "approval option")?;
    let label = required_string_field(map, "label", "approval option")?;
    let legacy = ApprovalOption::from_legacy(id.clone(), label.clone());
    let action = map
        .get("action")
        .map(parse_approval_action_value)
        .transpose()?
        .unwrap_or(legacy.action);
    let level = map
        .get("level")
        .map(parse_approval_level_value)
        .transpose()?
        .unwrap_or(legacy.level);
    Ok(ApprovalOption::new(id, label, action, level))
}

fn parse_approval_action_value(value: &ComponentValue) -> Result<ApprovalAction, String> {
    let ComponentValue::String(raw) = value else {
        return Err(format!("approval action must be string, got {value:?}"));
    };
    match raw.as_str() {
        "allow" => Ok(ApprovalAction::Allow),
        "deny" => Ok(ApprovalAction::Deny),
        _ => Err(format!("unknown approval action: {raw}")),
    }
}

fn parse_approval_level_value(value: &ComponentValue) -> Result<ApprovalLevel, String> {
    let ComponentValue::String(raw) = value else {
        return Err(format!("approval level must be string, got {value:?}"));
    };
    match raw.as_str() {
        "once" => Ok(ApprovalLevel::Once),
        "always" => Ok(ApprovalLevel::Always),
        "project" => Ok(ApprovalLevel::Project),
        _ => Err(format!("unknown approval level: {raw}")),
    }
}

fn parse_todo_items_value(value: &ComponentValue) -> Result<Vec<TodoItem>, String> {
    let ComponentValue::List(items) = value else {
        return Err(format!("todo items must be list, got {value:?}"));
    };
    items.iter().map(parse_todo_item_value).collect()
}

fn parse_plan_items_value(value: &ComponentValue) -> Result<Vec<PlanItem>, String> {
    let ComponentValue::List(items) = value else {
        return Err(format!("plan items must be list, got {value:?}"));
    };
    items.iter().map(parse_plan_item_value).collect()
}

fn parse_plan_item_value(value: &ComponentValue) -> Result<PlanItem, String> {
    let map = expect_map(value, "plan item")?;
    Ok(PlanItem {
        text: required_string_field(map, "text", "plan item")?,
    })
}

fn parse_task_transcript_value(value: &ComponentValue) -> Result<Vec<TaskTranscriptItem>, String> {
    let ComponentValue::List(items) = value else {
        return Err(format!("task transcript must be list, got {value:?}"));
    };
    items.iter().map(parse_task_transcript_item_value).collect()
}

fn parse_task_transcript_item_value(value: &ComponentValue) -> Result<TaskTranscriptItem, String> {
    let map = expect_map(value, "task transcript item")?;
    Ok(TaskTranscriptItem {
        role: map
            .get("role")
            .map(parse_role_value)
            .transpose()?
            .unwrap_or(ChatRole::Assistant),
        blocks: map
            .get("blocks")
            .map(parse_blocks_value)
            .transpose()?
            .unwrap_or_default(),
    })
}

fn parse_todo_item_value(value: &ComponentValue) -> Result<TodoItem, String> {
    let map = expect_map(value, "todo item")?;
    Ok(TodoItem {
        text: required_string_field(map, "text", "todo item")?,
        state: map
            .get("state")
            .map(parse_todo_state_value)
            .transpose()?
            .unwrap_or(TodoState::Pending),
    })
}

fn parse_error_kind_value(value: &ComponentValue) -> Result<ChatErrorKind, String> {
    match value {
        ComponentValue::String(raw) => parse_error_kind_string(raw),
        other => Err(format!("error kind must be string, got {other:?}")),
    }
}

fn parse_error_kind_string(raw: &str) -> Result<ChatErrorKind, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "api" => Ok(ChatErrorKind::Api),
        "tool" => Ok(ChatErrorKind::Tool),
        "rate_limit" | "ratelimit" => Ok(ChatErrorKind::RateLimit),
        "refusal" => Ok(ChatErrorKind::Refusal),
        "network" => Ok(ChatErrorKind::Network),
        "other" => Ok(ChatErrorKind::Other),
        _ => Err(format!("unknown error kind '{raw}'")),
    }
}

fn parse_stop_reason_value(value: &ComponentValue) -> Result<StopReason, String> {
    match value {
        ComponentValue::String(raw) => parse_stop_reason_string(raw),
        other => Err(format!("stop_reason must be string, got {other:?}")),
    }
}

fn parse_stop_reason_string(raw: &str) -> Result<StopReason, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "end_turn" | "endturn" => Ok(StopReason::EndTurn),
        "max_tokens" | "maxtokens" => Ok(StopReason::MaxTokens),
        "tool_use" | "tooluse" => Ok(StopReason::ToolUse),
        "stop_sequence" | "stopsequence" => Ok(StopReason::StopSequence),
        "refusal" => Ok(StopReason::Refusal),
        _ => Err(format!("unknown stop_reason '{raw}'")),
    }
}

fn parse_edit_decision_value(value: &ComponentValue) -> Result<EditDecision, String> {
    match value {
        ComponentValue::String(raw) => parse_edit_decision_string(raw),
        other => Err(format!("edit decision must be string, got {other:?}")),
    }
}

fn parse_edit_decision_string(raw: &str) -> Result<EditDecision, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "pending" => Ok(EditDecision::Pending),
        "accepted" | "accept" => Ok(EditDecision::Accepted),
        "rejected" | "reject" => Ok(EditDecision::Rejected),
        _ => Err(format!("unknown edit decision '{raw}'")),
    }
}

fn parse_plan_decision_value(value: &ComponentValue) -> Result<PlanDecision, String> {
    match value {
        ComponentValue::String(raw) => parse_plan_decision_string(raw),
        other => Err(format!("plan decision must be string, got {other:?}")),
    }
}

fn parse_plan_decision_string(raw: &str) -> Result<PlanDecision, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "pending" => Ok(PlanDecision::Pending),
        "accepted" | "accept" => Ok(PlanDecision::Accepted),
        "rejected" | "reject" => Ok(PlanDecision::Rejected),
        _ => Err(format!("unknown plan decision '{raw}'")),
    }
}

fn parse_task_status_value(value: &ComponentValue) -> Result<TaskStatus, String> {
    match value {
        ComponentValue::String(raw) => parse_task_status_string(raw),
        other => Err(format!("task status must be string, got {other:?}")),
    }
}

fn parse_task_status_string(raw: &str) -> Result<TaskStatus, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "pending" => Ok(TaskStatus::Pending),
        "running" => Ok(TaskStatus::Running),
        "complete" | "completed" | "done" => Ok(TaskStatus::Complete),
        "failed" | "error" => Ok(TaskStatus::Failed),
        "canceled" | "cancelled" => Ok(TaskStatus::Canceled),
        _ => Err(format!("unknown task status '{raw}'")),
    }
}

fn parse_todo_state_value(value: &ComponentValue) -> Result<TodoState, String> {
    match value {
        ComponentValue::String(raw) => parse_todo_state_string(raw),
        other => Err(format!("todo state must be string, got {other:?}")),
    }
}

fn parse_todo_state_string(raw: &str) -> Result<TodoState, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "pending" => Ok(TodoState::Pending),
        "in_progress" | "inprogress" => Ok(TodoState::InProgress),
        "done" => Ok(TodoState::Done),
        _ => Err(format!("unknown todo state '{raw}'")),
    }
}

fn parse_notice_level_value(value: &ComponentValue) -> Result<NoticeLevel, String> {
    match value {
        ComponentValue::String(raw) => parse_notice_level_string(raw),
        other => Err(format!("notice level must be string, got {other:?}")),
    }
}

fn parse_notice_level_string(raw: &str) -> Result<NoticeLevel, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "info" => Ok(NoticeLevel::Info),
        "warning" | "warn" => Ok(NoticeLevel::Warning),
        "error" => Ok(NoticeLevel::Error),
        _ => Err(format!("unknown notice level '{raw}'")),
    }
}

fn parse_compact_status_value(value: &ComponentValue) -> Result<CompactStatus, String> {
    match value {
        ComponentValue::String(raw) => parse_compact_status_string(raw),
        other => Err(format!("compact status must be string, got {other:?}")),
    }
}

fn parse_compact_status_string(raw: &str) -> Result<CompactStatus, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "pending" => Ok(CompactStatus::Pending),
        "running" | "in_progress" | "inprogress" => Ok(CompactStatus::Running),
        "complete" | "completed" | "done" => Ok(CompactStatus::Complete),
        "failed" | "error" => Ok(CompactStatus::Failed),
        "canceled" | "cancelled" => Ok(CompactStatus::Canceled),
        _ => Err(format!("unknown compact status '{raw}'")),
    }
}

fn parse_i32_value(value: &ComponentValue) -> Result<i32, String> {
    let raw = value
        .as_i64()
        .ok_or_else(|| format!("expected i32-compatible value, got {value:?}"))?;
    i32::try_from(raw).map_err(|_| format!("value out of range for i32: {raw}"))
}

fn expect_map<'a>(value: &'a ComponentValue, context: &str) -> Result<&'a ValueMap, String> {
    match value {
        ComponentValue::Map(map) => Ok(map),
        other => Err(format!("{context} must be map, got {other:?}")),
    }
}

fn required_string_field(map: &ValueMap, key: &str, context: &str) -> Result<String, String> {
    match map.get(key) {
        Some(ComponentValue::String(value)) => Ok(value.clone()),
        Some(other) => Err(format!(
            "{context} field '{key}' must be string, got {other:?}"
        )),
        None => Err(format!("{context} missing {key}")),
    }
}

fn optional_string_field(
    map: &ValueMap,
    key: &str,
    context: &str,
) -> Result<Option<String>, String> {
    match map.get(key) {
        Some(ComponentValue::String(value)) => Ok(Some(value.clone())),
        Some(ComponentValue::Null) | None => Ok(None),
        Some(other) => Err(format!(
            "{context} field '{key}' must be string, got {other:?}"
        )),
    }
}

fn required_u64_field(map: &ValueMap, key: &str, context: &str) -> Result<u64, String> {
    match map.get(key) {
        Some(value) => required_u64_value(value),
        None => Err(format!("{context} missing {key}")),
    }
}

fn required_u64_value(value: &ComponentValue) -> Result<u64, String> {
    value
        .as_u64()
        .ok_or_else(|| format!("expected u64-compatible value, got {value:?}"))
}

fn optional_u64_field(map: &ValueMap, key: &str, context: &str) -> Result<Option<u64>, String> {
    match map.get(key) {
        Some(ComponentValue::Null) | None => Ok(None),
        Some(value) => required_u64_value(value)
            .map(Some)
            .map_err(|err| format!("{context} field '{key}' must be u64-compatible: {err}")),
    }
}

fn required_bool_field(map: &ValueMap, key: &str, context: &str) -> Result<bool, String> {
    match map.get(key) {
        Some(ComponentValue::Bool(value)) => Ok(*value),
        Some(other) => Err(format!(
            "{context} field '{key}' must be bool, got {other:?}"
        )),
        None => Err(format!("{context} missing {key}")),
    }
}

fn optional_bool_field(map: &ValueMap, key: &str, context: &str) -> Result<Option<bool>, String> {
    match map.get(key) {
        Some(ComponentValue::Bool(value)) => Ok(Some(*value)),
        Some(ComponentValue::Null) | None => Ok(None),
        Some(other) => Err(format!(
            "{context} field '{key}' must be bool, got {other:?}"
        )),
    }
}

fn legacy_block_id(message_id: ChatMessageId, ordinal: u64) -> crate::ChatBlockId {
    crate::ChatBlockId::new(
        message_id
            .0
            .saturating_mul(1_000)
            .saturating_add(ordinal + 1),
    )
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

fn parse_tool_status_value(value: &ComponentValue) -> Result<ToolStatus, String> {
    match value {
        ComponentValue::String(raw) => parse_tool_status_string(raw),
        other => Err(format!("tool_call status must be string, got {other:?}")),
    }
}

fn parse_tool_status_string(raw: &str) -> Result<ToolStatus, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "pending" => Ok(ToolStatus::Pending),
        "running" => Ok(ToolStatus::Running),
        "done" => Ok(ToolStatus::Done),
        "error" => Ok(ToolStatus::Error),
        "canceled" | "cancelled" => Ok(ToolStatus::Canceled),
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

        let store = ChatMessageStore::new();
        store.replace_all(messages);
        let mut view = ChatMessageList::new(store);

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

        if let Some(percent) = prop_u16(spec, "bubble_width_percent")? {
            view = view.bubble_width_percent(percent);
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

        if let Some(cb) = event_handle(spec, "approve", callbacks.clone()) {
            view = view.on_approve(move |decision| {
                cb.emit_with(Some(approval_decision_to_value(decision)));
            });
        }

        if let Some(cb) = event_handle(spec, "edit_decision", callbacks.clone()) {
            view = view.on_edit_decision(move |event| {
                cb.emit_with(Some(edit_decision_event_to_value(event)));
            });
        }

        if let Some(cb) = event_handle(spec, "plan_decision", callbacks.clone()) {
            view = view.on_plan_decision(move |event| {
                cb.emit_with(Some(plan_decision_event_to_value(event)));
            });
        }

        if let Some(cb) = event_handle(spec, "cancel", callbacks.clone()) {
            view = view.on_cancel(move |message_id| {
                cb.emit_with(Some(cancel_event_to_value(message_id)));
            });
        }

        if let Some(cb) = event_handle(spec, "message_action", callbacks.clone()) {
            view = view.on_message_action(move |action| {
                cb.emit_with(Some(message_action_to_value(action)));
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

        if let Some(value) = spec.props.get("slash_commands") {
            let commands = parse_chat_slash_commands_value(value).map_err(|reason| {
                invalid_prop_reason(spec, "slash_commands", format!("{reason}; got {value:?}"))
            })?;
            handle.set_slash_commands(commands);
        }

        if let Some(value) = spec.props.get("mention_candidates") {
            let candidates = parse_chat_mention_candidates_value(value).map_err(|reason| {
                invalid_prop_reason(
                    spec,
                    "mention_candidates",
                    format!("{reason}; got {value:?}"),
                )
            })?;
            handle.set_mention_candidates(candidates);
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

        if let Some(cb) = event_handle(spec, "slash_command", callbacks.clone()) {
            panel = panel.on_slash_command(move |command| {
                cb.emit_with(Some(chat_slash_command_to_component_value(&command)));
            });
        }

        if let Some(cb) = event_handle(spec, "mention_query", callbacks.clone()) {
            let candidates = handle.mention_candidates_binding();
            panel = panel.mention_provider(move |context| {
                cb.emit_with(Some(chat_mention_context_to_component_value(&context)));
                candidates.get()
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
    fn chat_message_list_schema_exposes_approve_event_payload() {
        let schema = chat_message_list_schema();

        assert!(
            schema
                .events
                .iter()
                .any(|event| { event.name == "approve" && event.payload == Some(ValueType::Map) })
        );
    }

    #[test]
    fn chat_message_list_schema_exposes_edit_decision_event_payload() {
        let schema = chat_message_list_schema();

        assert!(schema.events.iter().any(|event| {
            event.name == "edit_decision" && event.payload == Some(ValueType::Map)
        }));
    }

    #[test]
    fn chat_message_list_schema_exposes_plan_decision_event_payload() {
        let schema = chat_message_list_schema();

        assert!(schema.events.iter().any(|event| {
            event.name == "plan_decision" && event.payload == Some(ValueType::Map)
        }));
    }

    #[test]
    fn chat_message_list_schema_exposes_message_action_event_payload() {
        let schema = chat_message_list_schema();

        assert!(schema.events.iter().any(|event| {
            event.name == "message_action" && event.payload == Some(ValueType::Map)
        }));
    }

    #[test]
    fn chat_message_list_schema_exposes_cancel_event_payload() {
        let schema = chat_message_list_schema();

        assert!(
            schema
                .events
                .iter()
                .any(|event| event.name == "cancel" && event.payload == Some(ValueType::Map))
        );
    }

    #[test]
    fn chat_input_panel_schema_exposes_completion_protocol() {
        let schema = chat_input_panel_schema();

        assert!(schema.properties.iter().any(|property| {
            property.name == "slash_commands" && property.value_type == ValueType::List
        }));
        assert!(schema.properties.iter().any(|property| {
            property.name == "mention_candidates" && property.value_type == ValueType::List
        }));
        assert!(schema.events.iter().any(|event| {
            event.name == "slash_command" && event.payload == Some(ValueType::Map)
        }));
        assert!(schema.events.iter().any(|event| {
            event.name == "mention_query" && event.payload == Some(ValueType::Map)
        }));
    }

    #[test]
    fn chat_input_panel_dynamic_builds_completion_properties() {
        let callbacks = CallbackRegistry::new();
        let mut registry = ComponentRegistry::<Box<dyn Component>>::new();
        register_chat_input_panel(&mut registry, callbacks);

        let slash_command = value_map([
            ("id", ComponentValue::String("clear".to_string())),
            ("label", ComponentValue::String("/clear".to_string())),
            (
                "detail",
                ComponentValue::String("Clear the conversation".to_string()),
            ),
            ("action", ComponentValue::String("submit".to_string())),
        ]);
        let mention_candidate = value_map([
            ("id", ComponentValue::String("cargo".to_string())),
            ("label", ComponentValue::String("Cargo.toml".to_string())),
            ("detail", ComponentValue::String("file".to_string())),
            (
                "replacement",
                ComponentValue::String("@Cargo.toml ".to_string()),
            ),
        ]);

        let spec = ComponentSpec::new("ChatInputPanel")
            .with_prop("slash_commands", ComponentValue::List(vec![slash_command]))
            .with_prop(
                "mention_candidates",
                ComponentValue::List(vec![mention_candidate]),
            );

        let view = registry.build(&spec).expect("build ChatInputPanel");
        let Some(ComponentValue::List(commands)) = view.get_property("slash_commands") else {
            panic!("slash_commands property must be a list");
        };
        let ComponentValue::Map(command) = &commands[0] else {
            panic!("slash command must be a map");
        };
        assert_eq!(
            command.get("action"),
            Some(&ComponentValue::String("submit".to_string()))
        );
        assert_eq!(
            command.get("replacement"),
            Some(&ComponentValue::String("/clear".to_string()))
        );

        let Some(ComponentValue::List(candidates)) = view.get_property("mention_candidates") else {
            panic!("mention_candidates property must be a list");
        };
        let ComponentValue::Map(candidate) = &candidates[0] else {
            panic!("mention candidate must be a map");
        };
        assert_eq!(
            candidate.get("replacement"),
            Some(&ComponentValue::String("@Cargo.toml ".to_string()))
        );
    }

    #[test]
    fn approval_decision_serializes_to_runtime_payload() {
        let value = approval_decision_to_value(ApprovalDecision {
            message_id: ChatMessageId::new(10),
            block_id: ChatBlockId::new(20),
            approval_id: "approval-1".to_string(),
            option_id: "allow_always".to_string(),
            action: ApprovalAction::Allow,
            level: ApprovalLevel::Always,
        });

        assert_eq!(
            value,
            value_map([
                ("message_id", ComponentValue::U64(10)),
                ("block_id", ComponentValue::U64(20)),
                (
                    "approval_id",
                    ComponentValue::String("approval-1".to_string())
                ),
                (
                    "option_id",
                    ComponentValue::String("allow_always".to_string())
                ),
                ("action", ComponentValue::String("allow".to_string())),
                ("level", ComponentValue::String("always".to_string())),
            ])
        );
    }

    #[test]
    fn edit_decision_event_serializes_to_runtime_payload() {
        let value = edit_decision_event_to_value(EditDecisionEvent {
            message_id: ChatMessageId::new(11),
            block_id: ChatBlockId::new(21),
            decision: EditDecision::Accepted,
        });

        assert_eq!(
            value,
            value_map([
                ("message_id", ComponentValue::U64(11)),
                ("block_id", ComponentValue::U64(21)),
                ("decision", ComponentValue::String("accepted".to_string())),
            ])
        );
    }

    #[test]
    fn plan_decision_event_serializes_to_runtime_payload() {
        let value = plan_decision_event_to_value(PlanDecisionEvent {
            message_id: ChatMessageId::new(14),
            block_id: ChatBlockId::new(24),
            decision: PlanDecision::Accepted,
        });

        assert_eq!(
            value,
            value_map([
                ("message_id", ComponentValue::U64(14)),
                ("block_id", ComponentValue::U64(24)),
                ("decision", ComponentValue::String("accepted".to_string())),
            ])
        );
    }

    #[test]
    fn cancel_event_serializes_to_runtime_payload() {
        let value = cancel_event_to_value(ChatMessageId::new(13));

        assert_eq!(value, value_map([("message_id", ComponentValue::U64(13))]));
    }

    #[test]
    fn message_action_serializes_to_runtime_payload() {
        let value = message_action_to_value(MessageAction {
            message_id: ChatMessageId::new(12),
            kind: MessageActionKind::CopyBlock(ChatBlockId::new(22)),
        });

        assert_eq!(
            value,
            value_map([
                ("message_id", ComponentValue::U64(12)),
                ("kind", ComponentValue::String("copy_block".to_string())),
                ("block_id", ComponentValue::U64(22)),
            ])
        );
    }

    #[test]
    fn chat_messages_serialize_to_new_block_shape() {
        let messages = vec![ChatMessage::text(
            ChatMessageId(41),
            ChatRole::Assistant,
            "hello",
        )];

        let value = messages_to_component_value(&messages);
        let ComponentValue::List(items) = value else {
            panic!("messages must serialize to list");
        };
        let ComponentValue::Map(message) = &items[0] else {
            panic!("message must serialize to map");
        };

        assert_eq!(
            message.get("role"),
            Some(&ComponentValue::String("assistant".to_string()))
        );
        assert!(message.contains_key("blocks"));
        assert!(!message.contains_key("sender"));
        assert!(!message.contains_key("content"));

        let Some(ComponentValue::List(blocks)) = message.get("blocks") else {
            panic!("message blocks must serialize to list");
        };
        let ComponentValue::Map(block) = &blocks[0] else {
            panic!("block must serialize to map");
        };
        assert_eq!(
            block.get("type"),
            Some(&ComponentValue::String("text".to_string()))
        );
        assert_eq!(block.get("block_id"), Some(&ComponentValue::U64(41_001)));
    }

    #[test]
    fn chat_messages_round_trip_new_block_shape() {
        let json_input = value_map([("path", ComponentValue::String("src/lib.rs".to_string()))]);
        let message = ChatMessage {
            id: ChatMessageId(7),
            role: ChatRole::Custom("agent".to_string()),
            blocks: vec![
                ChatBlock::Text(TextBlock {
                    id: ChatBlockId::new(71),
                    markdown: "answer".to_string(),
                    streaming: true,
                }),
                ChatBlock::Thinking(ThinkingBlock {
                    id: ChatBlockId::new(72),
                    markdown: "reasoning".to_string(),
                    streaming: true,
                    collapsed: true,
                }),
                ChatBlock::ToolUse(ToolUseBlock {
                    id: ChatBlockId::new(73),
                    call_id: "call-1".to_string(),
                    name: "bash".to_string(),
                    input: ToolInput::Json(json_input),
                    status: ToolStatus::Running,
                    approval: Some(ApprovalRequest {
                        id: "approval-1".to_string(),
                        prompt: "Run command?".to_string(),
                        options: vec![ApprovalOption::allow_once("allow", "Allow")],
                        resolved: Some(ApprovalResolution {
                            option_id: "allow".to_string(),
                            action: ApprovalAction::Allow,
                            level: ApprovalLevel::Once,
                        }),
                    }),
                    collapsed: true,
                }),
                ChatBlock::ToolResult(ToolResultBlock {
                    id: ChatBlockId::new(74),
                    call_id: "call-1".to_string(),
                    ok: false,
                    exit_code: Some(1),
                    output: ToolOutput::Diff(DiffData {
                        unified: "--- a\n+++ b".to_string(),
                    }),
                    collapsed: true,
                }),
                ChatBlock::Diff(DiffBlock {
                    id: ChatBlockId::new(75),
                    path: "src/lib.rs".to_string(),
                    diff: DiffData {
                        unified: "@@ hunk".to_string(),
                    },
                    decision: EditDecision::Accepted,
                }),
                ChatBlock::Plan(PlanBlock {
                    id: ChatBlockId::new(76),
                    items: vec![
                        PlanItem {
                            text: "write tests".to_string(),
                        },
                        PlanItem {
                            text: "verify output".to_string(),
                        },
                    ],
                    decision: PlanDecision::Accepted,
                }),
                ChatBlock::Task(TaskBlock {
                    id: ChatBlockId::new(81),
                    title: "search subagent".to_string(),
                    status: TaskStatus::Running,
                    summary: "SEARCH-SUMMARY".to_string(),
                    transcript: vec![TaskTranscriptItem {
                        role: ChatRole::Assistant,
                        blocks: vec![ChatBlock::Text(TextBlock {
                            id: ChatBlockId::new(82),
                            markdown: "NESTED-TEXT".to_string(),
                            streaming: false,
                        })],
                    }],
                    collapsed: false,
                }),
                ChatBlock::Todo(TodoBlock {
                    id: ChatBlockId::new(77),
                    items: vec![
                        TodoItem {
                            text: "write tests".to_string(),
                            state: TodoState::Done,
                        },
                        TodoItem {
                            text: "ship".to_string(),
                            state: TodoState::InProgress,
                        },
                    ],
                }),
                ChatBlock::Attachment(AttachmentBlock {
                    id: ChatBlockId::new(78),
                    name: "report.txt".to_string(),
                    url: Some("file:///tmp/report.txt".to_string()),
                    mime: Some("text/plain".to_string()),
                }),
                ChatBlock::Notice(NoticeBlock {
                    id: ChatBlockId::new(79),
                    level: NoticeLevel::Warning,
                    text: "context compacted".to_string(),
                }),
                ChatBlock::Compact(CompactBlock {
                    id: ChatBlockId::new(83),
                    status: CompactStatus::Complete,
                    before_tokens: Some(12_000),
                    after_tokens: Some(3_500),
                    summary: "kept current task context".to_string(),
                }),
                ChatBlock::Artifact(ArtifactBlock {
                    id: ChatBlockId::new(80),
                    kind: ArtifactKind::Diff,
                    anchor: ArtifactId::new("artifact-1"),
                    title: "patch".to_string(),
                }),
            ],
            status: ChatTurnStatus::Failed(
                ChatError::new(ChatErrorKind::Tool, "tool failed").with_detail("exit 1"),
            ),
            meta: ChatMessageMeta {
                timestamp: Some("2026-06-12T00:00:00Z".to_string()),
                model: Some("test-model".to_string()),
                usage: Some(TokenUsage {
                    input: 12,
                    output: 34,
                }),
                elapsed_ms: Some(567),
                stop_reason: Some(StopReason::ToolUse),
            },
        };

        let value = messages_to_component_value(std::slice::from_ref(&message));
        let parsed = parse_messages_value(&value).expect("parse messages");

        assert_eq!(parsed, vec![message]);
    }

    #[test]
    fn chat_thinking_blocks_default_to_collapsed_when_omitted() {
        let parsed = parse_block_value(&value_map([
            ("type", ComponentValue::String("thinking".to_string())),
            ("block_id", ComponentValue::U64(72)),
            ("markdown", ComponentValue::String("reasoning".to_string())),
        ]))
        .expect("parse thinking block");

        assert!(matches!(parsed, ChatBlock::Thinking(block) if block.collapsed));

        let expanded = ChatBlock::Thinking(ThinkingBlock {
            id: ChatBlockId::new(73),
            markdown: "reasoning".to_string(),
            streaming: false,
            collapsed: false,
        });
        let ComponentValue::Map(serialized) = block_to_value(&expanded) else {
            panic!("thinking block must serialize to map");
        };
        assert_eq!(
            serialized.get("collapsed"),
            Some(&ComponentValue::Bool(false))
        );

        let parsed = parse_block_value(&ComponentValue::Map(serialized))
            .expect("parse expanded thinking block");
        assert!(matches!(parsed, ChatBlock::Thinking(block) if !block.collapsed));
    }

    #[test]
    fn chat_plan_blocks_default_to_pending_when_decision_omitted() {
        let parsed = parse_block_value(&value_map([
            ("type", ComponentValue::String("plan".to_string())),
            ("block_id", ComponentValue::U64(82)),
            (
                "items",
                ComponentValue::List(vec![value_map([(
                    "text",
                    ComponentValue::String("PLAN-STEP".to_string()),
                )])]),
            ),
        ]))
        .expect("parse plan block");

        assert!(
            matches!(parsed, ChatBlock::Plan(block) if block.decision == PlanDecision::Pending)
        );
    }

    #[test]
    fn chat_task_blocks_default_to_pending_and_collapsed() {
        let parsed = parse_block_value(&value_map([
            ("type", ComponentValue::String("task".to_string())),
            ("block_id", ComponentValue::U64(83)),
            ("title", ComponentValue::String("subagent".to_string())),
        ]))
        .expect("parse task block");

        assert!(
            matches!(parsed, ChatBlock::Task(block) if block.status == TaskStatus::Pending && block.collapsed)
        );
    }

    #[test]
    fn chat_messages_round_trip_tool_call_content() {
        let messages = vec![ChatMessage::tool_call(
            ChatMessageId(42),
            "build",
            ToolStatus::Running,
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
            ChatRole::Assistant,
            ArtifactKind::Diff,
            ArtifactId::new("diff-1"),
            "main.patch",
        )];

        let value = messages_to_component_value(&messages);
        let parsed = parse_messages_value(&value).expect("parse messages");

        assert_eq!(parsed, messages);
    }

    #[test]
    fn chat_messages_parse_legacy_top_level_content_variants() {
        let content = value_map([(
            "markdown",
            ComponentValue::String("from content".to_string()),
        )]);
        let parsed = parse_message_value(&value_map([
            ("id", ComponentValue::U64(50)),
            ("sender", ComponentValue::String("user".to_string())),
            ("content", content),
        ]))
        .expect("parse legacy content");
        assert_eq!(
            parsed,
            ChatMessage::text(50, ChatRole::User, "from content")
        );

        let parsed = parse_message_value(&value_map([
            ("id", ComponentValue::U64(51)),
            ("sender", ComponentValue::String("assistant".to_string())),
            (
                "markdown",
                ComponentValue::String("top markdown".to_string()),
            ),
        ]))
        .expect("parse legacy markdown");
        assert_eq!(
            parsed,
            ChatMessage::text(51, ChatRole::Assistant, "top markdown")
        );

        let file = value_map([
            ("name", ComponentValue::String("report.txt".to_string())),
            (
                "url",
                ComponentValue::String("file:///report.txt".to_string()),
            ),
        ]);
        let parsed = parse_message_value(&value_map([
            ("id", ComponentValue::U64(52)),
            ("sender", ComponentValue::String("assistant".to_string())),
            ("file", file),
        ]))
        .expect("parse legacy file");
        assert_eq!(
            parsed,
            ChatMessage::file(
                52,
                ChatRole::Assistant,
                "report.txt",
                Some("file:///report.txt".to_string())
            )
        );

        let artifact = value_map([
            ("kind", ComponentValue::String("diff".to_string())),
            ("anchor", ComponentValue::String("artifact-1".to_string())),
            ("title", ComponentValue::String("patch".to_string())),
        ]);
        let parsed = parse_message_value(&value_map([
            ("id", ComponentValue::U64(53)),
            ("sender", ComponentValue::String("assistant".to_string())),
            ("artifact", artifact),
        ]))
        .expect("parse legacy artifact");
        assert_eq!(
            parsed,
            ChatMessage::artifact(
                53,
                ChatRole::Assistant,
                ArtifactKind::Diff,
                ArtifactId::new("artifact-1"),
                "patch"
            )
        );

        let tool_call = value_map([
            ("name", ComponentValue::String("build".to_string())),
            ("status", ComponentValue::String("running".to_string())),
            ("output", ComponentValue::String("cargo test".to_string())),
        ]);
        let parsed = parse_message_value(&value_map([
            ("id", ComponentValue::U64(54)),
            ("sender", ComponentValue::String("assistant".to_string())),
            ("status", ComponentValue::String("in_progress".to_string())),
            ("timestamp", ComponentValue::String("now".to_string())),
            ("tool_call", tool_call),
        ]))
        .expect("parse legacy tool call");

        assert_eq!(parsed.role, ChatRole::Assistant);
        assert_eq!(parsed.status, ChatTurnStatus::Streaming);
        assert_eq!(parsed.meta.timestamp.as_deref(), Some("now"));
        assert_eq!(parsed.blocks.len(), 2);
        assert!(matches!(
            &parsed.blocks[0],
            ChatBlock::ToolUse(block)
                if block.id == ChatBlockId::new(54_001)
                    && block.call_id == "tool-54"
                    && block.name == "build"
                    && block.status == ToolStatus::Running
        ));
        assert!(matches!(
            &parsed.blocks[1],
            ChatBlock::ToolResult(block)
                if block.id == ChatBlockId::new(54_002)
                    && block.call_id == "tool-54"
                    && block.output == ToolOutput::Ansi("cargo test".to_string())
        ));
    }

    fn value_map(
        entries: impl IntoIterator<Item = (&'static str, ComponentValue)>,
    ) -> ComponentValue {
        let mut map = ValueMap::new();
        for (key, value) in entries {
            map.insert(key.to_string(), value);
        }
        ComponentValue::Map(map)
    }
}
