//! Maps typed DeepSeek stream events into main-thread UI actions.

use std::collections::BTreeMap;

use atto_ui::ComponentValue;
use atto_ui_chat::{
    ChatBlockId, ChatBranchToken, ChatError, ChatErrorKind, ChatMessageId, ChatMessageMeta,
    StopReason, TokenUsage, ToolInput, ToolStatus, ToolUseBlock,
};
use serde_json::Value;

use crate::AppAction;
use crate::deepseek::{
    ChatCompletionChunk, ChatCompletionSseEvent, ChatToolCallDelta, CompletionUsage, FinishReason,
    chat_error_from_api_error,
};

/// Stateful mapper for one assistant turn's DeepSeek stream.
#[derive(Clone, Debug)]
pub(crate) struct DeepSeekUiStream {
    branch: ChatBranchToken,
    message_id: ChatMessageId,
    text_block_id: ChatBlockId,
    meta: ChatMessageMeta,
    tool_calls: ToolCallAccumulator,
    emitted_tool_calls: bool,
    finished: bool,
}

impl DeepSeekUiStream {
    /// Creates a mapper bound to the current transcript branch and assistant turn.
    pub(crate) fn new(
        branch: ChatBranchToken,
        message_id: ChatMessageId,
        text_block_id: ChatBlockId,
        model: impl Into<String>,
    ) -> Self {
        Self {
            branch,
            message_id,
            text_block_id,
            meta: ChatMessageMeta {
                model: Some(model.into()),
                ..ChatMessageMeta::default()
            },
            tool_calls: ToolCallAccumulator::default(),
            emitted_tool_calls: false,
            finished: false,
        }
    }

    /// Converts one parsed SSE event into UI actions for the main thread.
    pub(crate) fn map_event(&mut self, event: ChatCompletionSseEvent) -> Vec<AppAction> {
        if self.finished {
            return Vec::new();
        }

        match event {
            ChatCompletionSseEvent::Chunk(chunk) => self.map_chunk(chunk),
            ChatCompletionSseEvent::Done => {
                self.finished = true;
                vec![AppAction::TurnDone {
                    branch: self.branch,
                    message_id: self.message_id,
                    meta: Some(self.meta.clone()),
                }]
            }
            ChatCompletionSseEvent::Error(error) => {
                self.map_error(chat_error_from_api_error(error))
            }
        }
    }

    /// Converts a prepared chat error into a failed assistant turn.
    pub(crate) fn map_error(&mut self, error: ChatError) -> Vec<AppAction> {
        if self.finished {
            return Vec::new();
        }
        self.fail(error)
    }

    fn map_chunk(&mut self, chunk: ChatCompletionChunk) -> Vec<AppAction> {
        if let Some(model) = chunk.model.filter(|model| !model.is_empty()) {
            self.meta.model = Some(model);
        }
        if let Some(usage) = chunk.usage {
            self.meta.usage = Some(token_usage_from_completion(usage));
        }

        let mut actions = Vec::new();
        for choice in chunk.choices {
            let choice_index = choice.index;
            let delta = choice.delta;
            if let Some(reasoning) = delta.reasoning_content
                && !reasoning.is_empty()
            {
                actions.push(AppAction::ThinkingDelta {
                    branch: self.branch,
                    message_id: self.message_id,
                    delta: reasoning,
                });
            }
            if let Some(content) = delta.content
                && !content.is_empty()
            {
                actions.push(AppAction::TextDelta {
                    branch: self.branch,
                    block_id: self.text_block_id,
                    delta: content,
                });
            }
            for tool_call_delta in delta.tool_calls {
                self.tool_calls.push_delta(choice_index, tool_call_delta);
            }
            if let Some(finish_reason) = choice.finish_reason {
                self.meta.stop_reason = stop_reason_from_finish(finish_reason);
                if finish_reason == FinishReason::ToolCalls && !self.emitted_tool_calls {
                    match self.tool_calls.finish_blocks() {
                        Ok(tool_calls) => {
                            self.emitted_tool_calls = true;
                            actions.push(AppAction::ToolCallsReady {
                                branch: self.branch,
                                message_id: self.message_id,
                                tool_calls,
                            });
                        }
                        Err(error) => return self.fail(error),
                    }
                }
            }
        }
        actions
    }

    fn fail(&mut self, error: ChatError) -> Vec<AppAction> {
        self.finished = true;
        vec![AppAction::TurnFailed {
            branch: self.branch,
            message_id: self.message_id,
            error,
        }]
    }
}

#[derive(Clone, Debug, Default)]
struct ToolCallAccumulator {
    calls: BTreeMap<ToolCallKey, PartialToolCall>,
}

impl ToolCallAccumulator {
    fn push_delta(&mut self, choice_index: u32, delta: ChatToolCallDelta) {
        let call = self
            .calls
            .entry(ToolCallKey {
                choice_index,
                call_index: delta.index,
            })
            .or_default();
        if let Some(id) = delta.id.filter(|id| !id.is_empty()) {
            call.id = Some(id);
        }
        if let Some(function) = delta.function {
            if let Some(name) = function.name.filter(|name| !name.is_empty()) {
                call.name.push_str(&name);
            }
            if let Some(arguments) = function.arguments {
                call.arguments.push_str(&arguments);
            }
        }
    }

    fn finish_blocks(&self) -> Result<Vec<ToolUseBlock>, ChatError> {
        if self.calls.is_empty() {
            return Err(tool_call_error(
                "DeepSeek requested tool execution without tool calls.",
                "finish_reason was tool_calls, but no tool_calls deltas were received.",
            ));
        }

        self.calls
            .iter()
            .map(|(key, call)| call.to_tool_use_block(*key))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ToolCallKey {
    choice_index: u32,
    call_index: u32,
}

#[derive(Clone, Debug, Default)]
struct PartialToolCall {
    id: Option<String>,
    name: String,
    arguments: String,
}

impl PartialToolCall {
    fn to_tool_use_block(&self, key: ToolCallKey) -> Result<ToolUseBlock, ChatError> {
        let call_id = self
            .id
            .as_deref()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                missing_tool_call_field_error(
                    key,
                    "id",
                    "tool call id was not present in the stream",
                )
            })?;
        let name = (!self.name.is_empty())
            .then_some(self.name.as_str())
            .ok_or_else(|| {
                missing_tool_call_field_error(
                    key,
                    "function.name",
                    "tool function name was not present in the stream",
                )
            })?;
        let arguments = serde_json::from_str::<Value>(&self.arguments).map_err(|error| {
            tool_call_error(
                "DeepSeek returned invalid tool call arguments.",
                format!(
                    "choice {} tool call index {} `{}` arguments were not valid JSON: {}; raw arguments: {}",
                    key.choice_index,
                    key.call_index,
                    name,
                    error,
                    tool_call_arguments_preview(&self.arguments)
                ),
            )
        })?;

        Ok(ToolUseBlock {
            id: ChatBlockId::new(0),
            call_id: call_id.to_string(),
            name: name.to_string(),
            input: ToolInput::Json(component_value_from_json(arguments)),
            status: ToolStatus::Pending,
            approval: None,
            collapsed: false,
        })
    }
}

fn missing_tool_call_field_error(key: ToolCallKey, field: &str, detail: &str) -> ChatError {
    tool_call_error(
        "DeepSeek returned an incomplete tool call.",
        format!(
            "choice {} tool call index {} missing {field}: {detail}",
            key.choice_index, key.call_index
        ),
    )
}

fn tool_call_error(message: impl Into<String>, detail: impl Into<String>) -> ChatError {
    ChatError::new(ChatErrorKind::Tool, message).with_detail(detail)
}

fn tool_call_arguments_preview(arguments: &str) -> String {
    const MAX_CHARS: usize = 240;
    let mut preview = arguments.chars().take(MAX_CHARS).collect::<String>();
    if arguments.chars().count() > MAX_CHARS {
        preview.push_str("...");
    }
    preview
}

fn component_value_from_json(value: Value) -> ComponentValue {
    match value {
        Value::Null => ComponentValue::Null,
        Value::Bool(value) => ComponentValue::Bool(value),
        Value::Number(value) => component_value_from_json_number(value),
        Value::String(value) => ComponentValue::String(value),
        Value::Array(values) => {
            ComponentValue::List(values.into_iter().map(component_value_from_json).collect())
        }
        Value::Object(values) => ComponentValue::Map(
            values
                .into_iter()
                .map(|(key, value)| (key, component_value_from_json(value)))
                .collect(),
        ),
    }
}

fn component_value_from_json_number(value: serde_json::Number) -> ComponentValue {
    if let Some(value) = value.as_i64() {
        ComponentValue::I64(value)
    } else if let Some(value) = value.as_u64() {
        ComponentValue::U64(value)
    } else if let Some(value) = value.as_f64() {
        ComponentValue::F64(value)
    } else {
        ComponentValue::String(value.to_string())
    }
}

fn token_usage_from_completion(usage: CompletionUsage) -> TokenUsage {
    TokenUsage {
        input: usage.prompt_tokens.into(),
        output: usage.completion_tokens.into(),
    }
}

fn stop_reason_from_finish(reason: FinishReason) -> Option<StopReason> {
    match reason {
        FinishReason::Stop => Some(StopReason::EndTurn),
        FinishReason::Length => Some(StopReason::MaxTokens),
        FinishReason::ToolCalls => Some(StopReason::ToolUse),
        FinishReason::ContentFilter => Some(StopReason::Refusal),
        FinishReason::Other => None,
    }
}
