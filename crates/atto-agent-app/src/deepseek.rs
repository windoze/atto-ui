//! DeepSeek OpenAI-compatible Chat Completions protocol models.
//!
//! This module defines request/response shapes, deterministic request
//! construction, and line-level SSE parsing. The network client and UI mapping
//! land in later M2 tasks.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::AgentConfig;

pub const CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
pub const STREAM_DONE_SENTINEL: &str = "[DONE]";

/// Fully prepared OpenAI-compatible chat completions request parts.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatCompletionsRequestParts {
    pub method: HttpMethod,
    pub url: String,
    pub body: ChatCompletionRequest,
}

/// HTTP method used by DeepSeek protocol requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Post,
}

impl HttpMethod {
    /// Returns the method token expected by HTTP clients.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Post => "POST",
        }
    }
}

/// Role names accepted by OpenAI-compatible chat messages.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A single chat message sent to or received from DeepSeek.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChatCompletionMessage {
    pub role: ChatMessageRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ChatToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatCompletionMessage {
    /// Builds a system instruction message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::text(ChatMessageRole::System, content)
    }

    /// Builds a user prompt message.
    pub fn user(content: impl Into<String>) -> Self {
        Self::text(ChatMessageRole::User, content)
    }

    /// Builds an assistant text message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::text(ChatMessageRole::Assistant, content)
    }

    /// Builds an assistant message containing function calls.
    pub fn assistant_tool_calls(tool_calls: Vec<ChatToolCall>) -> Self {
        Self {
            role: ChatMessageRole::Assistant,
            content: None,
            reasoning_content: None,
            tool_calls,
            tool_call_id: None,
        }
    }

    /// Builds a tool result message tied to an assistant tool call id.
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ChatMessageRole::Tool,
            content: Some(content.into()),
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    fn text(role: ChatMessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: Some(content.into()),
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }
}

/// Request body for `POST /chat/completions`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatCompletionMessage>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub stream: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ChatTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
}

impl ChatCompletionRequest {
    /// Creates a streaming chat completion request from resolved app config.
    pub fn from_config(config: &AgentConfig, messages: Vec<ChatCompletionMessage>) -> Self {
        Self {
            model: config.model.clone(),
            messages,
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            stream: true,
            tools: Vec::new(),
            tool_choice: None,
        }
    }

    /// Attaches OpenAI-compatible tool definitions to the request body.
    pub fn with_tools(mut self, tools: Vec<ChatTool>) -> Self {
        self.tools = tools;
        self
    }

    /// Sets the OpenAI-compatible tool choice policy for this request.
    pub fn with_tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = Some(tool_choice);
        self
    }
}

/// OpenAI-compatible tool definition.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChatTool {
    #[serde(rename = "type")]
    pub kind: ChatToolKind,
    pub function: ChatToolFunction,
}

impl ChatTool {
    /// Builds a function tool definition using a JSON Schema parameters object.
    pub fn function(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
    ) -> Self {
        Self {
            kind: ChatToolKind::Function,
            function: ChatToolFunction {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

/// Tool definition kind used by OpenAI-compatible chat completions.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatToolKind {
    Function,
}

/// Function metadata inside a tool definition.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChatToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Tool choice policy accepted by OpenAI-compatible chat completions.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ToolChoice {
    Mode(ToolChoiceMode),
    Function(ToolChoiceFunction),
}

/// String form of OpenAI-compatible tool choice policies.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoiceMode {
    Auto,
    None,
    Required,
}

/// Object form used to force a specific function tool.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolChoiceFunction {
    #[serde(rename = "type")]
    pub kind: ChatToolKind,
    pub function: ToolChoiceFunctionName,
}

impl ToolChoiceFunction {
    /// Builds a tool choice object for a specific function name.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            kind: ChatToolKind::Function,
            function: ToolChoiceFunctionName { name: name.into() },
        }
    }
}

/// Function name payload inside an object-form tool choice.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ToolChoiceFunctionName {
    pub name: String,
}

/// Tool call emitted in a complete assistant message.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChatToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: ChatToolKind,
    pub function: ChatFunctionCall,
}

/// Complete function call payload after all streamed argument fragments arrive.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ChatFunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Non-streaming chat completion response shape.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChatCompletionResponse {
    pub id: Option<String>,
    pub object: Option<String>,
    pub created: Option<u64>,
    pub model: Option<String>,
    #[serde(default)]
    pub choices: Vec<ChatCompletionChoice>,
    pub usage: Option<CompletionUsage>,
}

/// One choice in a non-streaming chat completion response.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChatCompletionChoice {
    pub index: u32,
    pub message: ChatCompletionMessage,
    pub finish_reason: Option<FinishReason>,
}

/// SSE JSON payload shape for one streamed chat completion chunk.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChatCompletionChunk {
    pub id: Option<String>,
    pub object: Option<String>,
    pub created: Option<u64>,
    pub model: Option<String>,
    #[serde(default)]
    pub choices: Vec<ChatCompletionChunkChoice>,
    pub usage: Option<CompletionUsage>,
}

/// One streamed choice delta in a chat completion chunk.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChatCompletionChunkChoice {
    pub index: u32,
    #[serde(default)]
    pub delta: ChatCompletionDelta,
    pub finish_reason: Option<FinishReason>,
}

/// Incremental streamed assistant delta fields.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ChatCompletionDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<ChatMessageRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ChatToolCallDelta>,
}

/// Incremental streamed tool call fields keyed by OpenAI's `index`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChatToolCallDelta {
    pub index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<ChatToolKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<ChatFunctionCallDelta>,
}

/// Incremental function call name and argument fragments.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ChatFunctionCallDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

/// Model token accounting returned by DeepSeek/OpenAI-compatible APIs.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CompletionUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Known finish reasons for both response and streaming choice payloads.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    #[serde(other)]
    Other,
}

/// Error response body returned by OpenAI-compatible APIs.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DeepSeekErrorResponse {
    pub error: DeepSeekErrorBody,
}

/// Detailed API error payload. Unknown `code` shapes are preserved as JSON.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DeepSeekErrorBody {
    pub message: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub code: Option<Value>,
    pub param: Option<String>,
}

/// Data events emitted by the SSE stream after line-level parsing.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatCompletionSseEvent {
    Chunk(ChatCompletionChunk),
    Done,
    Error(DeepSeekErrorResponse),
}

/// Incrementally parses DeepSeek/OpenAI-compatible SSE text into typed events.
#[derive(Debug, Default)]
pub struct ChatCompletionSseParser {
    buffer: String,
    data_lines: Vec<String>,
}

impl ChatCompletionSseParser {
    /// Creates an empty parser ready to receive UTF-8 SSE text chunks.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds a UTF-8 text fragment and returns every complete SSE event found.
    pub fn push_str(&mut self, input: &str) -> Result<Vec<ChatCompletionSseEvent>> {
        self.buffer.push_str(input);
        let mut events = Vec::new();

        while let Some(newline_index) = self.buffer.find('\n') {
            let mut line = self.buffer.drain(..=newline_index).collect::<String>();
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
            self.process_line(&line, &mut events)?;
        }

        Ok(events)
    }

    /// Flushes any final unterminated line or event at end-of-stream.
    pub fn finish(&mut self) -> Result<Vec<ChatCompletionSseEvent>> {
        let mut events = Vec::new();

        if !self.buffer.is_empty() {
            let mut line = std::mem::take(&mut self.buffer);
            if line.ends_with('\r') {
                line.pop();
            }
            self.process_line(&line, &mut events)?;
        }
        self.flush_event(&mut events)?;

        Ok(events)
    }

    fn process_line(&mut self, line: &str, events: &mut Vec<ChatCompletionSseEvent>) -> Result<()> {
        if line.is_empty() {
            self.flush_event(events)?;
            return Ok(());
        }
        if line.starts_with(':') {
            return Ok(());
        }

        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        if field == "data" {
            self.data_lines
                .push(value.strip_prefix(' ').unwrap_or(value).to_string());
        }
        Ok(())
    }

    fn flush_event(&mut self, events: &mut Vec<ChatCompletionSseEvent>) -> Result<()> {
        if self.data_lines.is_empty() {
            return Ok(());
        }

        let data = self.data_lines.join("\n");
        self.data_lines.clear();
        events.push(parse_chat_completion_sse_data(&data)?);
        Ok(())
    }
}

/// Parses a full SSE payload string into typed DeepSeek stream events.
pub fn parse_chat_completion_sse(input: &str) -> Result<Vec<ChatCompletionSseEvent>> {
    let mut parser = ChatCompletionSseParser::new();
    let mut events = parser.push_str(input)?;
    events.extend(parser.finish()?);
    Ok(events)
}

/// Parses one completed SSE `data:` payload.
pub fn parse_chat_completion_sse_data(data: &str) -> Result<ChatCompletionSseEvent> {
    let data = data.trim();
    if data == STREAM_DONE_SENTINEL {
        return Ok(ChatCompletionSseEvent::Done);
    }

    let value = serde_json::from_str::<Value>(data)
        .with_context(|| format!("invalid DeepSeek SSE data: {}", sse_data_preview(data)))?;
    if value.get("error").is_some() {
        return serde_json::from_value(value)
            .map(ChatCompletionSseEvent::Error)
            .context("invalid DeepSeek SSE error payload");
    }

    serde_json::from_value(value)
        .map(ChatCompletionSseEvent::Chunk)
        .context("invalid DeepSeek SSE chunk payload")
}

fn sse_data_preview(data: &str) -> String {
    const MAX_CHARS: usize = 160;
    let mut preview = data.chars().take(MAX_CHARS).collect::<String>();
    if data.chars().count() > MAX_CHARS {
        preview.push_str("...");
    }
    preview
}

/// Builds deterministic request parts for `POST /chat/completions`.
pub fn build_chat_completions_request(
    config: &AgentConfig,
    messages: Vec<ChatCompletionMessage>,
) -> Result<ChatCompletionsRequestParts> {
    Ok(ChatCompletionsRequestParts {
        method: HttpMethod::Post,
        url: chat_completions_url(&config.base_url)?,
        body: ChatCompletionRequest::from_config(config, messages),
    })
}

/// Joins a configured base URL with the chat completions endpoint path.
pub fn chat_completions_url(base_url: &str) -> Result<String> {
    let base_url = base_url.trim();
    if base_url.is_empty() {
        bail!("DeepSeek base URL must not be empty");
    }
    Ok(format!(
        "{}{}",
        base_url.trim_end_matches('/'),
        CHAT_COMPLETIONS_PATH
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::config::AgentConfig;

    use super::*;

    #[test]
    fn builds_streaming_chat_completions_request_from_config() {
        let mut config = AgentConfig::defaults(".");
        config.base_url = "https://api.deepseek.com/v1/".to_string();
        config.model = "deepseek-reasoner".to_string();
        config.temperature = 0.5;
        config.max_tokens = 2048;

        let request = build_chat_completions_request(
            &config,
            vec![
                ChatCompletionMessage::system("You are concise."),
                ChatCompletionMessage::user("Hello"),
            ],
        )
        .unwrap();

        assert_eq!(request.method.as_str(), "POST");
        assert_eq!(request.url, "https://api.deepseek.com/v1/chat/completions");
        assert_eq!(request.body.model, "deepseek-reasoner");
        assert_eq!(request.body.temperature, 0.5);
        assert_eq!(request.body.max_tokens, 2048);
        assert!(request.body.stream);
        assert!(request.body.tools.is_empty());
        assert_eq!(
            serde_json::to_value(&request.body).unwrap(),
            json!({
                "model": "deepseek-reasoner",
                "messages": [
                    { "role": "system", "content": "You are concise." },
                    { "role": "user", "content": "Hello" }
                ],
                "temperature": 0.5,
                "max_tokens": 2048,
                "stream": true
            })
        );
    }

    #[test]
    fn serializes_tools_and_tool_choice_like_openai() {
        let tool = ChatTool::function(
            "read_file",
            "Read a UTF-8 file under the workspace root.",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        );

        let request = ChatCompletionRequest::from_config(
            &AgentConfig::defaults("."),
            vec![ChatCompletionMessage::user("Read src/lib.rs")],
        )
        .with_tools(vec![tool])
        .with_tool_choice(ToolChoice::Mode(ToolChoiceMode::Auto));

        assert_eq!(
            serde_json::to_value(&request).unwrap()["tools"][0],
            json!({
                "type": "function",
                "function": {
                    "name": "read_file",
                    "description": "Read a UTF-8 file under the workspace root.",
                    "parameters": {
                        "type": "object",
                        "properties": { "path": { "type": "string" } },
                        "required": ["path"]
                    }
                }
            })
        );
        assert_eq!(
            serde_json::to_value(&request).unwrap()["tool_choice"],
            "auto"
        );

        let forced = ToolChoice::Function(ToolChoiceFunction::named("submit_plan"));
        assert_eq!(
            serde_json::to_value(forced).unwrap(),
            json!({ "type": "function", "function": { "name": "submit_plan" } })
        );
    }

    #[test]
    fn deserializes_stream_chunk_content_reasoning_and_finish_reason() {
        let chunk: ChatCompletionChunk = serde_json::from_value(json!({
            "id": "chatcmpl-1",
            "object": "chat.completion.chunk",
            "created": 1710000000,
            "model": "deepseek-chat",
            "choices": [
                {
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "reasoning_content": "thinking",
                        "content": "answer"
                    },
                    "finish_reason": "stop"
                }
            ]
        }))
        .unwrap();

        let choice = &chunk.choices[0];
        assert_eq!(choice.delta.role, Some(ChatMessageRole::Assistant));
        assert_eq!(choice.delta.reasoning_content.as_deref(), Some("thinking"));
        assert_eq!(choice.delta.content.as_deref(), Some("answer"));
        assert_eq!(choice.finish_reason, Some(FinishReason::Stop));
    }

    #[test]
    fn parses_sse_content_reasoning_finish_reason_and_done() {
        let events = parse_chat_completion_sse(concat!(
            "id: ignored\n",
            "event: message\n",
            "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"thinking\",\"content\":\"hel\"},\"finish_reason\":null}]}\r\n",
            "\r\n",
            ": keepalive\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}]}\n",
            "\n",
            "data: [DONE]\n\n",
        ))
        .unwrap();

        assert_eq!(events.len(), 3);
        let ChatCompletionSseEvent::Chunk(first) = &events[0] else {
            panic!("expected first SSE event to be a chunk");
        };
        let first_choice = &first.choices[0];
        assert_eq!(
            first_choice.delta.reasoning_content.as_deref(),
            Some("thinking")
        );
        assert_eq!(first_choice.delta.content.as_deref(), Some("hel"));
        assert_eq!(first_choice.finish_reason, None);

        let ChatCompletionSseEvent::Chunk(second) = &events[1] else {
            panic!("expected second SSE event to be a chunk");
        };
        assert_eq!(second.choices[0].delta.content.as_deref(), Some("lo"));
        assert_eq!(second.choices[0].finish_reason, Some(FinishReason::Stop));
        assert_eq!(events[2], ChatCompletionSseEvent::Done);
    }

    #[test]
    fn parses_multiline_sse_data_payload() {
        let events = parse_chat_completion_sse(concat!(
            "data: {\"choices\":[\n",
            "data: {\"index\":0,\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}\n",
            "data: ]}\n",
            "\n",
        ))
        .unwrap();

        let [ChatCompletionSseEvent::Chunk(chunk)] = events.as_slice() else {
            panic!("expected one chunk SSE event, got {events:?}");
        };
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("hello"));
    }

    #[test]
    fn parses_fragmented_sse_input() {
        let mut parser = ChatCompletionSseParser::new();

        assert!(
            parser
                .push_str("data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hel")
                .unwrap()
                .is_empty()
        );
        assert!(
            parser
                .push_str("lo\"},\"finish_reason\":null}]}\n")
                .unwrap()
                .is_empty()
        );
        let events = parser.push_str("\n").unwrap();

        let [ChatCompletionSseEvent::Chunk(chunk)] = events.as_slice() else {
            panic!("expected one chunk SSE event, got {events:?}");
        };
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("hello"));
        assert!(parser.finish().unwrap().is_empty());
    }

    #[test]
    fn parses_sse_error_fragments() {
        let event = parse_chat_completion_sse_data(
            r#"{"error":{"message":"bad api key","type":"invalid_request_error","code":"invalid_api_key","param":null}}"#,
        )
        .unwrap();

        let ChatCompletionSseEvent::Error(error) = event else {
            panic!("expected SSE error event");
        };
        assert_eq!(error.error.message, "bad api key");
        assert_eq!(error.error.kind.as_deref(), Some("invalid_request_error"));
        assert_eq!(error.error.code, Some(json!("invalid_api_key")));
    }

    #[test]
    fn reports_malformed_sse_json() {
        let error = parse_chat_completion_sse_data("{not json").unwrap_err();

        assert!(error.to_string().contains("invalid DeepSeek SSE data"));
        assert!(error.to_string().contains("{not json"));
    }

    #[test]
    fn deserializes_stream_tool_call_delta_shape() {
        let chunk: ChatCompletionChunk = serde_json::from_value(json!({
            "choices": [
                {
                    "index": 0,
                    "delta": {
                        "tool_calls": [
                            {
                                "index": 0,
                                "id": "call_1",
                                "type": "function",
                                "function": {
                                    "name": "read_file",
                                    "arguments": "{\"path\":"
                                }
                            }
                        ]
                    },
                    "finish_reason": null
                }
            ]
        }))
        .unwrap();

        let tool_call = &chunk.choices[0].delta.tool_calls[0];
        assert_eq!(tool_call.index, 0);
        assert_eq!(tool_call.id.as_deref(), Some("call_1"));
        assert_eq!(tool_call.kind, Some(ChatToolKind::Function));
        let function = tool_call.function.as_ref().unwrap();
        assert_eq!(function.name.as_deref(), Some("read_file"));
        assert_eq!(function.arguments.as_deref(), Some("{\"path\":"));
    }

    #[test]
    fn deserializes_response_and_error_shapes() {
        let response: ChatCompletionResponse = serde_json::from_value(json!({
            "id": "chatcmpl-2",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "done"
                    },
                    "finish_reason": "length"
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 3,
                "total_tokens": 13
            }
        }))
        .unwrap();

        assert_eq!(response.choices[0].message.content.as_deref(), Some("done"));
        assert_eq!(
            response.choices[0].finish_reason,
            Some(FinishReason::Length)
        );
        assert_eq!(response.usage.unwrap().total_tokens, 13);

        let error: DeepSeekErrorResponse = serde_json::from_value(json!({
            "error": {
                "message": "bad api key",
                "type": "invalid_request_error",
                "code": "invalid_api_key",
                "param": null
            }
        }))
        .unwrap();

        assert_eq!(error.error.message, "bad api key");
        assert_eq!(error.error.kind.as_deref(), Some("invalid_request_error"));
        assert_eq!(error.error.code, Some(json!("invalid_api_key")));
        assert_eq!(error.error.param, None);
    }

    #[test]
    fn rejects_empty_base_url() {
        let error = chat_completions_url("  ").unwrap_err();

        assert!(error.to_string().contains("base URL must not be empty"));
    }
}
