//! Agent-turn spawning: mock provider turn thread, live DeepSeek turn thread
//! (async runtime), and stream-action forwarding.

use crate::*;

pub(crate) fn spawn_mock_agent_turn(action_sender: mpsc::Sender<AppAction>, request: MockAgentTurnRequest) {
    thread::spawn(move || {
        let mut stream = DeepSeekUiStream::new_with_plan_gate(
            request.branch,
            request.message_id,
            request.block_id,
            request.model,
            request.plan_decision.requires_plan(),
            request.mutating_tools_allowed,
        );
        for event in mock_agent_events(&request.prompt, &request.plan_decision) {
            if request.cancel.is_cancelled() {
                return;
            }
            thread::sleep(request.token_delay);
            if request.cancel.is_cancelled() {
                return;
            }
            if !send_stream_actions(&action_sender, stream.map_event(event)) {
                return;
            }
        }
    });
}

pub(crate) fn spawn_deepseek_agent_turn(
    action_sender: mpsc::Sender<AppAction>,
    request: DeepSeekAgentTurnRequest,
    abort_registration: AbortRegistration,
) {
    thread::spawn(move || {
        let runtime = match atto_ui_async::build_current_thread_runtime() {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = action_sender.send(AppAction::TurnFailed {
                    branch: request.branch,
                    message_id: request.message_id,
                    error: deepseek_runtime_error(error),
                });
                return;
            }
        };
        runtime.block_on(run_deepseek_agent_turn(
            action_sender,
            request,
            abort_registration,
        ));
    });
}

pub(crate) async fn run_deepseek_agent_turn(
    action_sender: mpsc::Sender<AppAction>,
    request: DeepSeekAgentTurnRequest,
    abort_registration: AbortRegistration,
) {
    if request.cancel.is_cancelled() {
        return;
    }

    let mut stream = DeepSeekUiStream::new_with_plan_gate_and_tool_loop(
        request.branch,
        request.message_id,
        request.block_id,
        request.config.model.clone(),
        request.plan_decision.requires_plan(),
        request.mutating_tools_allowed,
        !request.plan_decision.requires_plan(),
    );
    let cancel = request.cancel.clone();
    let result = Abortable::new(
        DeepSeekClient::new().stream_prepared_chat_completion_events(
            &request.config,
            request.request,
            |event| {
                if cancel.is_cancelled() {
                    return Err(deepseek_turn_cancelled_error());
                }
                if !send_stream_actions(&action_sender, stream.map_event(event)) {
                    return Err(ui_action_channel_closed_error());
                }
                if cancel.is_cancelled() {
                    return Err(deepseek_turn_cancelled_error());
                }
                Ok(())
            },
        ),
        abort_registration,
    )
    .await
    .unwrap_or_else(|_| Err(deepseek_turn_cancelled_error()));

    if let Err(error) = result
        && !request.cancel.is_cancelled()
    {
        let _ = send_stream_actions(&action_sender, stream.map_error(error));
    }
}

pub(crate) fn send_stream_actions(action_sender: &mpsc::Sender<AppAction>, actions: Vec<AppAction>) -> bool {
    for action in actions {
        if action_sender.send(action).is_err() {
            return false;
        }
    }
    true
}

pub(crate) fn deepseek_runtime_error(error: std::io::Error) -> ChatError {
    ChatError::new(
        ChatErrorKind::Other,
        "Failed to start DeepSeek async runtime.",
    )
    .with_detail(error.to_string())
}

pub(crate) fn deepseek_turn_cancelled_error() -> ChatError {
    ChatError::new(ChatErrorKind::Other, "DeepSeek turn was canceled.")
}

pub(crate) fn ui_action_channel_closed_error() -> ChatError {
    ChatError::new(
        ChatErrorKind::Other,
        "UI action channel closed before DeepSeek turn finished.",
    )
}

pub(crate) fn mock_agent_events(
    prompt: &str,
    plan_decision: &PlanTurnDecision,
) -> Vec<ChatCompletionSseEvent> {
    match prompt.trim() {
        MOCK_READ_FILE_PROMPT => vec![
            mock_stream_tool_call_event(
                "call_read_cargo",
                "read_file",
                serde_json::json!({ "path": ".atto/skills/pty-fixture/SKILL.md" }),
            ),
            ChatCompletionSseEvent::Done,
        ],
        MOCK_RUN_COMMAND_PROMPT => vec![
            mock_stream_tool_call_event(
                "call_run_echo",
                "run_command",
                serde_json::json!({
                    "argv": ["/bin/echo", "AGENT-ALLOW-OUTPUT"],
                    "cwd": "."
                }),
            ),
            ChatCompletionSseEvent::Done,
        ],
        prompt if prompt.starts_with(MOCK_CONTEXT_PROBE_PREFIX) => {
            mock_context_probe_events(prompt)
        }
        _ if plan_decision.requires_plan() => vec![
            mock_stream_tool_call_event(
                "call_submit_plan",
                crate::plan::SUBMIT_PLAN_TOOL_NAME,
                serde_json::json!({
                    "items": [
                        "Review the request and relevant context.",
                        "Implement the requested change in the appropriate files.",
                        "Run formatting, linting, and tests before reporting back."
                    ]
                }),
            ),
            ChatCompletionSseEvent::Done,
        ],
        _ => {
            let mut events = mock_agent_deltas(prompt)
                .into_iter()
                .map(mock_stream_content_event)
                .collect::<Vec<_>>();
            events.push(mock_stream_finish_event());
            events.push(ChatCompletionSseEvent::Done);
            events
        }
    }
}

pub(crate) fn mock_context_probe_events(prompt: &str) -> Vec<ChatCompletionSseEvent> {
    vec![
        mock_stream_content_event("Mock context probe:\n".to_string()),
        mock_stream_content_event(mock_context_probe_text(prompt)),
        mock_stream_finish_event(),
        ChatCompletionSseEvent::Done,
    ]
}

pub(crate) fn mock_context_probe_text(prompt: &str) -> String {
    let config = AgentConfig::defaults(env!("CARGO_MANIFEST_DIR"));
    let registry = crate::tool::builtin_tool_registry().expect("built-in tool registry is valid");
    let transcript = vec![ChatMessage::text(
        ChatMessageId::new(1),
        ChatRole::User,
        prompt.to_string(),
    )];
    let request = deepseek_request_from_transcript(&config, &registry, &transcript);
    let context = request
        .messages
        .iter()
        .filter_map(|message| message.content.as_deref())
        .find_map(|content| {
            content
                .find("<context_files>")
                .map(|start| &content[start..])
        });
    context
        .unwrap_or("No context files were injected into the model request.")
        .to_string()
}

pub(crate) fn mock_stream_content_event(delta: String) -> ChatCompletionSseEvent {
    ChatCompletionSseEvent::Chunk(ChatCompletionChunk {
        id: None,
        object: None,
        created: None,
        model: None,
        choices: vec![ChatCompletionChunkChoice {
            index: 0,
            delta: ChatCompletionDelta {
                content: Some(delta),
                ..ChatCompletionDelta::default()
            },
            finish_reason: None,
        }],
        usage: None,
    })
}

pub(crate) fn mock_stream_tool_call_event(
    call_id: &str,
    name: &str,
    arguments: serde_json::Value,
) -> ChatCompletionSseEvent {
    ChatCompletionSseEvent::Chunk(ChatCompletionChunk {
        id: None,
        object: None,
        created: None,
        model: None,
        choices: vec![ChatCompletionChunkChoice {
            index: 0,
            delta: ChatCompletionDelta {
                tool_calls: vec![ChatToolCallDelta {
                    index: 0,
                    id: Some(call_id.to_string()),
                    kind: Some(ChatToolKind::Function),
                    function: Some(ChatFunctionCallDelta {
                        name: Some(name.to_string()),
                        arguments: Some(arguments.to_string()),
                    }),
                }],
                ..ChatCompletionDelta::default()
            },
            finish_reason: Some(FinishReason::ToolCalls),
        }],
        usage: None,
    })
}

pub(crate) fn mock_stream_finish_event() -> ChatCompletionSseEvent {
    ChatCompletionSseEvent::Chunk(ChatCompletionChunk {
        id: None,
        object: None,
        created: None,
        model: None,
        choices: vec![ChatCompletionChunkChoice {
            index: 0,
            delta: ChatCompletionDelta::default(),
            finish_reason: Some(FinishReason::Stop),
        }],
        usage: None,
    })
}

pub(crate) fn mock_agent_deltas(prompt: &str) -> Vec<String> {
    let prompt = prompt.trim();
    if prompt.starts_with(MOCK_RETRY_EDIT_PROMPT) {
        let turn = MOCK_RETRY_EDIT_TURN_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
        return vec![
            format!("Mock retry/edit turn {turn}: "),
            prompt.to_string(),
            "\n".to_string(),
            "Done.".to_string(),
        ];
    }

    vec![
        "Mock assistant: ".to_string(),
        prompt.to_string(),
        "\n".to_string(),
        "Done.".to_string(),
    ]
}
