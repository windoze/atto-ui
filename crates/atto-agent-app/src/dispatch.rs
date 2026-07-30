//! [`apply_app_action`]: the central `AppAction` dispatcher, plus tool-loop
//! continuation decisions.

use crate::*;

pub(crate) fn apply_app_action(
    store: &ChatMessageStore,
    input_handle: &ChatInputHandle,
    mock_turns: &MockTurnRegistry,
    status_state: &Property<String>,
    transcript_status: &TranscriptStatusState,
    tool_runtime: &ToolRuntime,
    action: AppAction,
) -> bool {
    let changed = match action {
        AppAction::TextDelta {
            branch,
            block_id,
            delta,
        } => store.is_branch_current(branch) && store.append_text_delta(block_id, &delta),
        AppAction::ThinkingDelta {
            branch,
            message_id,
            delta,
        } => store.is_branch_current(branch) && append_thinking_delta(store, message_id, &delta),
        AppAction::ToolCallsReady {
            branch,
            message_id,
            tool_calls,
            mutating_tools_allowed,
            continue_after_tools,
        } => {
            if !store.is_branch_current(branch) || tool_calls.is_empty() {
                return false;
            }
            if let Err(error) = tool_runtime.turn_budgets.consume_tool_calls(
                message_id,
                tool_calls.len(),
                tool_runtime.limits,
            ) {
                let found = store.fail_streaming_turn(message_id, error);
                if found {
                    mock_turns.clear(message_id);
                    input_handle.streaming_binding().set(false);
                    status_state.set(STATUS_READY.to_string());
                    tool_runtime.turn_budgets.finish_turn(message_id);
                }
                found
            } else {
                for tool_call in tool_calls {
                    let PreparedToolCall { tool_use, result } = prepare_tool_call(
                        tool_call,
                        &tool_runtime.registry,
                        &tool_runtime.permissions,
                        mutating_tools_allowed,
                    );
                    let mut tool_use = tool_use;
                    let call_id = tool_use.call_id.clone();
                    let Some(block_id) =
                        store.append_block(message_id, ChatBlock::ToolUse(tool_use.clone()))
                    else {
                        return false;
                    };
                    tool_use.id = block_id;
                    match result {
                        Some(result) => {
                            if store.upsert_tool_result(call_id, result).is_none() {
                                return false;
                            }
                        }
                        None if tool_use.status == ToolStatus::Running => {
                            spawn_tool_execution(ToolExecutionRequest {
                                branch,
                                message_id,
                                tool_use,
                                config: tool_runtime.config.clone(),
                                registry: tool_runtime.registry.clone(),
                                limits: tool_runtime.limits,
                                action_sender: tool_runtime.action_sender.clone(),
                                mutating_tools_allowed,
                                continue_after_tools,
                            });
                        }
                        None => {}
                    }
                }
                let _ = maybe_continue_deepseek_tool_loop(
                    store,
                    tool_runtime,
                    message_id,
                    mutating_tools_allowed,
                    continue_after_tools,
                );
                true
            }
        }
        AppAction::PlanReady {
            branch,
            message_id,
            items,
        } => {
            if !store.is_branch_current(branch) || items.is_empty() {
                return false;
            }
            store
                .append_block(
                    message_id,
                    ChatBlock::Plan(PlanBlock {
                        id: ChatBlockId::new(0),
                        items,
                        decision: PlanDecision::Pending,
                    }),
                )
                .is_some()
        }
        AppAction::ToolResultReady {
            branch,
            message_id,
            tool_block_id,
            call_id,
            result,
            mutating_tools_allowed,
            continue_after_tools,
        } => {
            if !store.is_branch_current(branch) {
                return false;
            }
            let status = if result.ok {
                ToolStatus::Done
            } else {
                ToolStatus::Error
            };
            let found_tool = store.set_tool_status(tool_block_id, status);
            let found_result = store.upsert_tool_result(call_id, result).is_some();
            if found_tool && found_result {
                let _ = maybe_continue_deepseek_tool_loop(
                    store,
                    tool_runtime,
                    message_id,
                    mutating_tools_allowed,
                    continue_after_tools,
                );
            }
            found_tool && found_result
        }
        AppAction::TurnDone {
            branch,
            message_id,
            meta,
        } => {
            if !store.is_branch_current(branch) {
                return false;
            }
            let found = store.set_turn_status(message_id, ChatTurnStatus::Complete);
            if found {
                if let Some(meta) = meta {
                    store.set_meta(message_id, meta);
                }
                mock_turns.clear(message_id);
                tool_runtime.turn_budgets.finish_turn(message_id);
                input_handle.streaming_binding().set(false);
                status_state.set(STATUS_READY.to_string());
            }
            found
        }
        AppAction::TurnFailed {
            branch,
            message_id,
            error,
        } => {
            if !store.is_branch_current(branch) {
                return false;
            }
            let found = store.fail_streaming_turn(message_id, error);
            if found {
                mock_turns.clear(message_id);
                tool_runtime.turn_budgets.finish_turn(message_id);
                input_handle.streaming_binding().set(false);
                status_state.set(STATUS_READY.to_string());
            }
            found
        }
    };
    if changed {
        transcript_status.sync(store);
    }
    changed
}

pub(crate) fn maybe_continue_deepseek_tool_loop(
    store: &ChatMessageStore,
    tool_runtime: &ToolRuntime,
    message_id: ChatMessageId,
    mutating_tools_allowed: bool,
    continue_after_tools: bool,
) -> bool {
    if !continue_after_tools || tool_runtime.config.provider != AgentProvider::DeepSeek {
        return false;
    }
    if !tool_loop_ready_for_continuation(&store.messages(), message_id) {
        return false;
    }

    if let Err(error) = tool_runtime
        .turn_budgets
        .consume_model_request(message_id, tool_runtime.limits)
    {
        let found = store.fail_streaming_turn(message_id, error);
        if found {
            tool_runtime.mock_turns.clear(message_id);
            tool_runtime.turn_budgets.finish_turn(message_id);
            tool_runtime.input_handle.streaming_binding().set(false);
            tool_runtime.status_state.set(STATUS_READY.to_string());
        }
        return found;
    }

    let request = deepseek_request_from_transcript_with_skills(
        &tool_runtime.config,
        &tool_runtime.registry,
        &tool_runtime.skill_registry,
        &tool_runtime.loaded_skills,
        &store.messages(),
    );
    let Some(block_id) = store.append_block(
        message_id,
        ChatBlock::Text(TextBlock {
            id: ChatBlockId::new(0),
            markdown: String::new(),
            streaming: true,
        }),
    ) else {
        return false;
    };
    let branch = store.branch_token();
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    let cancel = tool_runtime
        .mock_turns
        .start_with_abort_handle(message_id, Some(abort_handle));
    tool_runtime.input_handle.streaming_binding().set(true);
    tool_runtime.status_state.set(STATUS_STREAMING.to_string());
    spawn_deepseek_agent_turn(
        tool_runtime.action_sender.clone(),
        DeepSeekAgentTurnRequest {
            branch,
            message_id,
            block_id,
            cancel,
            config: tool_runtime.config.clone(),
            request,
            plan_decision: PlanTurnDecision::Direct,
            mutating_tools_allowed,
        },
        abort_registration,
    );
    true
}

pub(crate) fn tool_loop_ready_for_continuation(messages: &[ChatMessage], message_id: ChatMessageId) -> bool {
    let Some(message) = messages.iter().find(|message| message.id == message_id) else {
        return false;
    };
    if !message.status.is_streaming() {
        return false;
    }

    let result_call_ids = message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::ToolResult(result) => Some(result.call_id.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut saw_tool_call = false;
    for block in &message.blocks {
        let ChatBlock::ToolUse(tool_use) = block else {
            continue;
        };
        saw_tool_call = true;
        if matches!(tool_use.status, ToolStatus::Pending | ToolStatus::Running) {
            return false;
        }
        if !result_call_ids.contains(&tool_use.call_id.as_str()) {
            return false;
        }
    }

    saw_tool_call
}
