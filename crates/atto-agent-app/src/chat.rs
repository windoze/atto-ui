//! Chat-panel wiring, input dispatch (submit / edit-and-resubmit / message action),
//! agent-turn launch, DeepSeek request building, skill auto-loading.

use crate::*;

pub(crate) fn build_chat_panel(
    store: &ChatMessageStore,
    turn_launcher: AgentTurnLauncher,
    slash_runtime: SlashRuntime,
    tool_runtime: ToolRuntime,
    context_menus: EventQueue<ChatContextMenuRequest>,
) -> ChatPanel {
    // Compose the reusable chat list and input controls around shared state handles.
    let input_handle_for_cancel = slash_runtime.input_handle.clone();
    let status_for_cancel = slash_runtime.status_state.clone();
    let mock_turns_for_cancel = slash_runtime.mock_turns.clone();
    let turn_budgets_for_cancel = slash_runtime.turn_budgets.clone();
    let store_for_cancel = store.clone();
    let transcript_status_for_cancel = slash_runtime.transcript_status.clone();
    let store_for_approval = store.clone();
    let tool_runtime_for_approval = tool_runtime.clone();
    let store_for_plan_decision = store.clone();
    let plan_runtime = PlanDecisionRuntime {
        input_handle: slash_runtime.input_handle.clone(),
        mock_turns: slash_runtime.mock_turns.clone(),
        status_state: slash_runtime.status_state.clone(),
        skill_registry: slash_runtime.skill_registry.clone(),
        loaded_skills: slash_runtime.loaded_skills.clone(),
        transcript_status: slash_runtime.transcript_status.clone(),
        turn_launcher: turn_launcher.clone(),
    };
    let store_for_edit_resubmit = store.clone();
    let slash_runtime_for_edit_resubmit = slash_runtime.clone();
    let turn_launcher_for_edit_resubmit = turn_launcher.clone();
    let store_for_message_action = store.clone();
    let slash_runtime_for_message_action = slash_runtime.clone();
    let turn_launcher_for_message_action = turn_launcher.clone();
    let list = ChatMessageList::new(store.clone())
        .show_timestamps(false)
        .on_approve(move |decision| {
            handle_tool_approval(&store_for_approval, &tool_runtime_for_approval, decision);
        })
        .on_plan_decision(move |event| {
            handle_plan_decision(&store_for_plan_decision, &plan_runtime, event);
        })
        .on_edit_and_resubmit(&slash_runtime.input_handle, move |event| {
            handle_edit_and_resubmit(
                &store_for_edit_resubmit,
                &slash_runtime_for_edit_resubmit,
                &turn_launcher_for_edit_resubmit,
                event,
            );
        })
        .on_message_action(move |action| {
            handle_message_action(
                &store_for_message_action,
                &slash_runtime_for_message_action,
                &turn_launcher_for_message_action,
                action,
            );
        })
        .on_cancel(move |message_id| {
            finish_canceled_turn(
                &store_for_cancel,
                &input_handle_for_cancel,
                &mock_turns_for_cancel,
                &status_for_cancel,
                &transcript_status_for_cancel,
                &turn_budgets_for_cancel,
                message_id,
            );
        })
        .on_context_menu(move |request| context_menus.push(request));
    let store_for_submit = store.clone();
    let slash_runtime_for_submit = slash_runtime.clone();
    let turn_launcher_for_submit = turn_launcher.clone();
    let store_for_slash = store.clone();
    let slash_runtime_for_slash = slash_runtime.clone();
    slash_runtime
        .input_handle
        .set_slash_commands(agent_slash_commands());
    let input = slash_runtime
        .input_handle
        .panel()
        .on_submit(move |response| {
            submit_input_response(
                &store_for_submit,
                &slash_runtime_for_submit,
                &turn_launcher_for_submit,
                response,
            );
        })
        .on_slash_command(move |command| {
            let _ = submit_slash_command_text(
                &store_for_slash,
                &slash_runtime_for_slash,
                &command.replacement,
            );
        });
    ChatPanel::new(list, input)
}

pub(crate) fn submit_input_response(
    store: &ChatMessageStore,
    slash_runtime: &SlashRuntime,
    turn_launcher: &AgentTurnLauncher,
    response: ChatInputResponse,
) {
    let text = input_response_text(response);
    if text.trim().is_empty() {
        return;
    }

    if submit_slash_command_text(store, slash_runtime, &text) {
        return;
    }

    let _ = start_agent_turn_from_user_prompt(store, slash_runtime, turn_launcher, text, true);
}

pub(crate) fn handle_edit_and_resubmit(
    store: &ChatMessageStore,
    slash_runtime: &SlashRuntime,
    turn_launcher: &AgentTurnLauncher,
    event: EditAndResubmitEvent,
) {
    cancel_active_turn_after_transcript_truncation(
        &slash_runtime.input_handle,
        &slash_runtime.mock_turns,
        &slash_runtime.status_state,
        &slash_runtime.turn_budgets,
    );
    let _ = start_agent_turn_from_user_prompt(
        store,
        slash_runtime,
        turn_launcher,
        event.edited_text,
        true,
    );
}

pub(crate) fn handle_message_action(
    store: &ChatMessageStore,
    slash_runtime: &SlashRuntime,
    turn_launcher: &AgentTurnLauncher,
    action: MessageAction,
) {
    if !matches!(
        action.kind,
        MessageActionKind::Retry | MessageActionKind::Regenerate
    ) {
        return;
    }

    cancel_active_turn_after_transcript_truncation(
        &slash_runtime.input_handle,
        &slash_runtime.mock_turns,
        &slash_runtime.status_state,
        &slash_runtime.turn_budgets,
    );
    let Some(prompt) = latest_user_prompt(store) else {
        append_system_message(
            store,
            "Cannot retry or regenerate: no prior user prompt remains in the transcript.",
        );
        slash_runtime.transcript_status.sync(store);
        return;
    };
    let _ = start_agent_turn_from_user_prompt(store, slash_runtime, turn_launcher, prompt, false);
}

pub(crate) fn start_agent_turn_from_user_prompt(
    store: &ChatMessageStore,
    slash_runtime: &SlashRuntime,
    turn_launcher: &AgentTurnLauncher,
    text: String,
    append_user_message: bool,
) -> Option<ChatMessageId> {
    if text.trim().is_empty() {
        return None;
    }

    auto_load_matching_skills(
        &slash_runtime.skill_registry,
        &slash_runtime.loaded_skills,
        &slash_runtime.skill_count_state,
        &text,
    );
    let plan_mode =
        plan_mode_from_status(&slash_runtime.plan_mode_state.get()).unwrap_or(PlanMode::Off);
    let plan_decision = decide_plan_for_turn(plan_mode, &text, &turn_launcher.tool_registry);
    let mutating_tools_allowed = mutating_tools_allowed_for_turn(plan_mode, &plan_decision);

    if append_user_message {
        let user_id = store.next_message_id();
        store.push(ChatMessage::text(user_id, ChatRole::User, text.clone()));
    }
    let _ = compact_store_if_needed(store, turn_launcher.compact_policy);

    let assistant_id = start_agent_turn_for_request(
        store,
        &slash_runtime.input_handle,
        &slash_runtime.mock_turns,
        &slash_runtime.status_state,
        turn_launcher,
        AgentTurnStartRequest {
            prompt: text,
            plan_decision,
            mutating_tools_allowed,
            skill_registry: slash_runtime.skill_registry.clone(),
            loaded_skills: slash_runtime.loaded_skills.clone(),
        },
    );
    slash_runtime.transcript_status.sync(store);
    assistant_id
}

pub(crate) fn cancel_active_turn_after_transcript_truncation(
    input_handle: &ChatInputHandle,
    mock_turns: &MockTurnRegistry,
    status_state: &Property<String>,
    turn_budgets: &TurnBudgetTracker,
) {
    if let Some(message_id) = mock_turns.cancel_current() {
        turn_budgets.finish_turn(message_id);
    }
    input_handle.streaming_binding().set(false);
    status_state.set(STATUS_READY.to_string());
}

pub(crate) fn latest_user_prompt(store: &ChatMessageStore) -> Option<String> {
    store.messages().iter().rev().find_map(user_prompt_text)
}

pub(crate) fn user_prompt_text(message: &ChatMessage) -> Option<String> {
    if message.role != ChatRole::User {
        return None;
    }
    let text = message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text(text) if !text.markdown.trim().is_empty() => {
                Some(text.markdown.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    (!text.trim().is_empty()).then_some(text)
}

pub(crate) fn mutating_tools_allowed_for_turn(plan_mode: PlanMode, plan_decision: &PlanTurnDecision) -> bool {
    plan_mode == PlanMode::Off && !plan_decision.requires_plan()
}

pub(crate) fn start_agent_turn_for_request(
    store: &ChatMessageStore,
    input_handle: &ChatInputHandle,
    mock_turns: &MockTurnRegistry,
    status_state: &Property<String>,
    turn_launcher: &AgentTurnLauncher,
    request: AgentTurnStartRequest,
) -> Option<ChatMessageId> {
    let AgentTurnStartRequest {
        prompt,
        plan_decision,
        mutating_tools_allowed,
        skill_registry,
        loaded_skills,
    } = request;
    let assistant_id = store.next_message_id();
    let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
        .with_status(ChatTurnStatus::Streaming);
    let text_block_id = assistant.blocks[0].id();
    store.push(assistant);
    let branch = store.branch_token();
    turn_launcher
        .turn_budgets
        .start_turn(assistant_id, turn_launcher.limits);
    if let Err(error) = turn_launcher
        .turn_budgets
        .consume_model_request(assistant_id, turn_launcher.limits)
    {
        store.fail_streaming_turn(assistant_id, error);
        turn_launcher.turn_budgets.finish_turn(assistant_id);
        return None;
    }
    input_handle.streaming_binding().set(true);
    status_state.set(STATUS_STREAMING.to_string());
    match turn_launcher.config.provider {
        AgentProvider::Mock => {
            let cancel = mock_turns.start(assistant_id);
            spawn_mock_agent_turn(
                turn_launcher.action_sender.clone(),
                MockAgentTurnRequest {
                    branch,
                    message_id: assistant_id,
                    block_id: text_block_id,
                    cancel,
                    token_delay: mock_turns.token_delay(),
                    model: turn_launcher.config.model.clone(),
                    prompt,
                    plan_decision,
                    mutating_tools_allowed,
                },
            );
        }
        AgentProvider::DeepSeek => {
            let (abort_handle, abort_registration) = AbortHandle::new_pair();
            let cancel = mock_turns.start_with_abort_handle(assistant_id, Some(abort_handle));
            spawn_deepseek_agent_turn(
                turn_launcher.action_sender.clone(),
                DeepSeekAgentTurnRequest {
                    branch,
                    message_id: assistant_id,
                    block_id: text_block_id,
                    cancel,
                    config: turn_launcher.config.clone(),
                    request: deepseek_live_request_for_turn(
                        &turn_launcher.config,
                        &turn_launcher.tool_registry,
                        &skill_registry,
                        &loaded_skills,
                        &store.messages(),
                        &plan_decision,
                    ),
                    plan_decision,
                    mutating_tools_allowed,
                },
                abort_registration,
            );
        }
    }
    Some(assistant_id)
}

pub(crate) fn deepseek_live_request_for_turn(
    config: &AgentConfig,
    registry: &ToolRegistry,
    skill_registry: &SkillRegistry,
    loaded_skills: &LoadedSkillSet,
    messages: &[ChatMessage],
    plan_decision: &PlanTurnDecision,
) -> ChatCompletionRequest {
    if plan_decision.requires_plan() {
        deepseek_plan_request_from_transcript_with_skills(
            config,
            skill_registry,
            loaded_skills,
            messages,
        )
    } else {
        deepseek_request_from_transcript_with_skills(
            config,
            registry,
            skill_registry,
            loaded_skills,
            messages,
        )
    }
}

pub(crate) fn auto_load_matching_skills(
    registry: &SkillRegistry,
    loaded_skills: &LoadedSkillSet,
    skill_count_state: &Property<String>,
    prompt: &str,
) -> Vec<String> {
    let matching_names =
        registry.matching_auto_skill_names(prompt, loaded_skills, DEFAULT_MAX_AUTO_LOADED_SKILLS);
    let mut loaded_names = Vec::new();
    for name in matching_names {
        if loaded_skills.insert(name.clone()) {
            loaded_names.push(name);
        }
    }
    if !loaded_names.is_empty() {
        skill_count_state.set(loaded_skills.status());
    }
    loaded_names
}

pub(crate) fn input_response_text(response: ChatInputResponse) -> String {
    match response {
        ChatInputResponse::Text(text) | ChatInputResponse::Custom(text) => text,
        ChatInputResponse::Choice { label, .. } => label,
    }
}
