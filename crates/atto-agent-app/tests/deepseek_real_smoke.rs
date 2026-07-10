#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use atto_agent_app::config::{AgentConfig, DEFAULT_BASE_URL, DEFAULT_MODEL};
use atto_agent_app::deepseek::{
    ChatCompletionMessage, ChatCompletionRequest, ChatCompletionSseEvent, ChatFunctionCall,
    ChatTool, ChatToolCall, ChatToolKind, FinishReason, ToolChoice, ToolChoiceFunction,
    ToolChoiceMode,
};
use atto_agent_app::deepseek_client::DeepSeekClient;
use serde_json::{Value, json};

#[tokio::test]
#[ignore = "requires DEEPSEEK_API_KEY and external DeepSeek network access"]
async fn deepseek_real_streaming_tool_round_trip_smoke() {
    let api_key = std::env::var("DEEPSEEK_API_KEY")
        .expect("set DEEPSEEK_API_KEY to run the real DeepSeek smoke test");
    let mut config = AgentConfig::defaults(std::env::current_dir().expect("read current dir"));
    config.api_key = Some(api_key);
    config.base_url =
        std::env::var("DEEPSEEK_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into());
    config.model = std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
    config.temperature = 0.0;
    config.max_tokens = 128;

    let client = DeepSeekClient::new();
    let tool = smoke_echo_tool();
    let messages = vec![
        ChatCompletionMessage::system(
            "You are running a deterministic smoke test. Use tools exactly as requested.",
        ),
        ChatCompletionMessage::user(
            "Call the atto_smoke_echo tool with text `atto smoke ok`. \
             After the tool result, reply with the phrase `atto smoke ok from tool`.",
        ),
    ];

    let tool_request = ChatCompletionRequest::from_config(&config, messages.clone())
        .with_tools(vec![tool.clone()])
        .with_tool_choice(ToolChoice::Function(ToolChoiceFunction::named(
            "atto_smoke_echo",
        )));
    let tool_stream = stream_request(&client, &config, tool_request).await;
    assert!(tool_stream.saw_done, "tool stream must end with [DONE]");
    assert!(
        tool_stream
            .finish_reasons
            .contains(&FinishReason::ToolCalls),
        "first stream should finish with a tool call"
    );

    let mut tool_calls = tool_stream.into_tool_calls();
    assert_eq!(tool_calls.len(), 1, "smoke test expects one tool call");
    let tool_call = tool_calls.pop().expect("tool call length checked");
    assert_eq!(tool_call.function.name, "atto_smoke_echo");

    let tool_output = execute_smoke_echo(&tool_call);
    let mut followup_messages = messages;
    followup_messages.push(ChatCompletionMessage::assistant_tool_calls(vec![
        tool_call.clone(),
    ]));
    followup_messages.push(ChatCompletionMessage::tool(tool_call.id, tool_output));

    let final_request = ChatCompletionRequest::from_config(&config, followup_messages)
        .with_tools(vec![tool])
        .with_tool_choice(ToolChoice::Mode(ToolChoiceMode::None));
    let final_stream = stream_request(&client, &config, final_request).await;
    assert!(final_stream.saw_done, "final stream must end with [DONE]");
    assert!(
        final_stream.finish_reasons.contains(&FinishReason::Stop),
        "final stream should finish normally"
    );
    assert!(
        final_stream
            .streamed_text
            .to_ascii_lowercase()
            .contains("atto smoke ok"),
        "real final stream should include the smoke phrase; got {:?}",
        final_stream.streamed_text
    );
}

/// Defines the minimal function tool used by the real DeepSeek smoke test.
fn smoke_echo_tool() -> ChatTool {
    ChatTool::function(
        "atto_smoke_echo",
        "Return the provided smoke-test text unchanged.",
        json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "The smoke-test text to echo."
                }
            },
            "required": ["text"],
            "additionalProperties": false
        }),
    )
}

/// Streams one prepared request and captures text, tool-call deltas, and finish reasons.
async fn stream_request(
    client: &DeepSeekClient,
    config: &AgentConfig,
    request: ChatCompletionRequest,
) -> StreamCapture {
    let mut capture = StreamCapture::default();
    client
        .stream_prepared_chat_completion_events(config, request, |event| {
            capture.push(event);
            Ok(())
        })
        .await
        .unwrap_or_else(|error| panic!("real DeepSeek streaming smoke failed: {error:?}"));
    capture
}

/// Executes the smoke echo tool locally and returns the model-visible tool output.
fn execute_smoke_echo(tool_call: &ChatToolCall) -> String {
    let args = serde_json::from_str::<Value>(&tool_call.function.arguments)
        .unwrap_or_else(|error| panic!("smoke tool arguments must be JSON: {error}"));
    let text = args
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("smoke tool arguments must include string `text`: {args}"));
    assert!(
        text.to_ascii_lowercase().contains("atto smoke ok"),
        "smoke tool text should contain the expected phrase; got {text:?}"
    );
    format!("atto_smoke_echo output: {text}")
}

/// Captures the pieces needed to verify an end-to-end streamed tool round trip.
#[derive(Default)]
struct StreamCapture {
    saw_done: bool,
    streamed_text: String,
    finish_reasons: Vec<FinishReason>,
    tool_calls: BTreeMap<(u32, u32), ToolCallParts>,
}

impl StreamCapture {
    /// Records one parsed SSE event from DeepSeek.
    fn push(&mut self, event: ChatCompletionSseEvent) {
        match event {
            ChatCompletionSseEvent::Chunk(chunk) => {
                for choice in chunk.choices {
                    if let Some(reasoning) = choice.delta.reasoning_content {
                        self.streamed_text.push_str(&reasoning);
                    }
                    if let Some(content) = choice.delta.content {
                        self.streamed_text.push_str(&content);
                    }
                    if let Some(finish_reason) = choice.finish_reason {
                        self.finish_reasons.push(finish_reason);
                    }
                    for tool_call in choice.delta.tool_calls {
                        self.tool_calls
                            .entry((choice.index, tool_call.index))
                            .or_default()
                            .push(tool_call);
                    }
                }
            }
            ChatCompletionSseEvent::Done => self.saw_done = true,
            ChatCompletionSseEvent::Error(error) => {
                panic!("DeepSeek returned an SSE error event: {error:?}");
            }
        }
    }

    /// Converts accumulated streamed tool deltas into complete tool calls.
    fn into_tool_calls(self) -> Vec<ChatToolCall> {
        self.tool_calls
            .into_values()
            .map(ToolCallParts::into_tool_call)
            .collect()
    }
}

/// Accumulates streamed fragments for one function call.
#[derive(Default)]
struct ToolCallParts {
    id: Option<String>,
    name: String,
    arguments: String,
}

impl ToolCallParts {
    /// Appends one streamed tool-call delta fragment.
    fn push(&mut self, tool_call: atto_agent_app::deepseek::ChatToolCallDelta) {
        if let Some(id) = tool_call.id {
            self.id = Some(id);
        }
        if let Some(function) = tool_call.function {
            if let Some(name) = function.name {
                self.name.push_str(&name);
            }
            if let Some(arguments) = function.arguments {
                self.arguments.push_str(&arguments);
            }
        }
    }

    /// Builds the complete OpenAI-compatible tool call for follow-up messages.
    fn into_tool_call(self) -> ChatToolCall {
        let id = self
            .id
            .unwrap_or_else(|| panic!("streamed tool call must include an id"));
        assert!(
            !self.name.trim().is_empty(),
            "streamed tool call must include a function name"
        );
        assert!(
            !self.arguments.trim().is_empty(),
            "streamed tool call must include JSON arguments"
        );
        ChatToolCall {
            id,
            kind: ChatToolKind::Function,
            function: ChatFunctionCall {
                name: self.name,
                arguments: self.arguments,
            },
        }
    }
}
