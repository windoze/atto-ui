//! Tool execution and approval: tool-call preparation, plan-decision handling,
//! approval flow, denied/failed results, and worker-thread execution.

use crate::*;

#[derive(Clone, Debug)]
pub(crate) struct PreparedToolCall {
    pub(crate) tool_use: ToolUseBlock,
    pub(crate) result: Option<ToolResultBlock>,
}

pub(crate) fn prepare_tool_call(
    mut tool_use: ToolUseBlock,
    registry: &ToolRegistry,
    permissions: &Arc<Mutex<ToolPermissionPolicy>>,
    mutating_tools_allowed: bool,
) -> PreparedToolCall {
    let Some(spec) = registry.spec(&tool_use.name) else {
        tool_use.status = ToolStatus::Error;
        return PreparedToolCall {
            result: Some(failed_tool_result(
                &tool_use.call_id,
                format!("Tool `{}` is not registered.", tool_use.name),
            )),
            tool_use,
        };
    };

    if !mutating_tools_allowed && spec.can_have_side_effects() {
        tool_use.status = ToolStatus::Canceled;
        return PreparedToolCall {
            result: Some(failed_tool_result(
                &tool_use.call_id,
                PLAN_MODE_MUTATING_TOOL_BLOCKED_RESULT,
            )),
            tool_use,
        };
    }

    let decision = permissions
        .lock()
        .expect("tool permission policy lock poisoned")
        .resolve(spec);
    match decision {
        ToolPermissionDecision::Allow => {
            tool_use.status = ToolStatus::Running;
            tool_use.approval = None;
            PreparedToolCall {
                tool_use,
                result: None,
            }
        }
        ToolPermissionDecision::RequestApproval { allow_project } => {
            tool_use.status = ToolStatus::Pending;
            tool_use.approval = Some(tool_approval_request(&tool_use, allow_project));
            PreparedToolCall {
                tool_use,
                result: None,
            }
        }
        ToolPermissionDecision::Deny => {
            tool_use.status = ToolStatus::Canceled;
            PreparedToolCall {
                result: Some(failed_tool_result(
                    &tool_use.call_id,
                    format!("Tool `{}` is not allowed by policy.", tool_use.name),
                )),
                tool_use,
            }
        }
    }
}

pub(crate) fn tool_approval_request(
    tool_use: &ToolUseBlock,
    allow_project: bool,
) -> ApprovalRequest {
    let mut options = vec![ApprovalOption::allow_once("allow_once", "Allow once")];
    if allow_project {
        options.push(ApprovalOption::allow_project(
            "allow_project",
            "Allow project",
        ));
    }
    options.push(ApprovalOption::deny("deny", "Deny"));

    ApprovalRequest {
        id: format!("approval:{}", tool_use.call_id),
        prompt: format!("Allow tool `{}` to run?", tool_use.name),
        options,
        resolved: None,
    }
}

pub(crate) fn handle_plan_decision(
    store: &ChatMessageStore,
    runtime: &PlanDecisionRuntime,
    event: PlanDecisionEvent,
) {
    if event.decision == PlanDecision::Pending {
        return;
    }
    let is_pending_plan = store
        .with_block(event.block_id, |block| {
            matches!(block, ChatBlock::Plan(plan) if plan.decision == PlanDecision::Pending)
        })
        .unwrap_or(false);
    if is_pending_plan {
        if !store.set_plan_decision(event.block_id, event.decision) {
            return;
        }
        match event.decision {
            PlanDecision::Accepted => {
                continue_after_accepted_plan(store, runtime, event.message_id)
            }
            PlanDecision::Rejected => finish_plan_decision_turn(store, runtime, event.message_id),
            PlanDecision::Pending => {}
        }
    }
}

pub(crate) fn continue_after_accepted_plan(
    store: &ChatMessageStore,
    runtime: &PlanDecisionRuntime,
    plan_message_id: ChatMessageId,
) {
    finish_plan_decision_turn(store, runtime, plan_message_id);
    let instruction = ACCEPTED_PLAN_EXECUTION_INSTRUCTION.to_string();
    store.push(ChatMessage::text(
        store.next_message_id(),
        ChatRole::System,
        instruction.clone(),
    ));
    let _ = compact_store_if_needed(store, runtime.turn_launcher.compact_policy);
    let _ = start_agent_turn_for_request(
        store,
        &runtime.input_handle,
        &runtime.mock_turns,
        &runtime.status_state,
        &runtime.turn_launcher,
        AgentTurnStartRequest {
            prompt: instruction,
            plan_decision: PlanTurnDecision::Direct,
            mutating_tools_allowed: true,
            skill_registry: runtime.skill_registry.clone(),
            loaded_skills: runtime.loaded_skills.clone(),
        },
    );
    runtime.transcript_status.sync(store);
}

pub(crate) fn finish_plan_decision_turn(
    store: &ChatMessageStore,
    runtime: &PlanDecisionRuntime,
    message_id: ChatMessageId,
) {
    let _ = runtime.mock_turns.cancel(message_id);
    let _ = store.set_turn_status(message_id, ChatTurnStatus::Complete);
    runtime.turn_launcher.turn_budgets.finish_turn(message_id);
    runtime.input_handle.streaming_binding().set(false);
    runtime.status_state.set(STATUS_READY.to_string());
    runtime.transcript_status.sync(store);
}

pub(crate) fn handle_tool_approval(
    store: &ChatMessageStore,
    tool_runtime: &ToolRuntime,
    decision: ApprovalDecision,
) {
    let Some(tool_use) = tool_use_for_approval(store, &decision) else {
        return;
    };
    if tool_use.status != ToolStatus::Pending
        || tool_use
            .approval
            .as_ref()
            .and_then(|approval| approval.resolved.as_ref())
            .is_some()
    {
        return;
    }
    if !store.resolve_approval(decision.block_id, decision.option_id) {
        return;
    }

    match decision.action {
        ApprovalAction::Allow if decision.level == ApprovalLevel::Project => {
            tool_runtime
                .permissions
                .lock()
                .expect("tool permission policy lock poisoned")
                .allow_for_project(tool_use.name.clone());
            spawn_tool_execution(ToolExecutionRequest {
                branch: store.branch_token(),
                message_id: decision.message_id,
                tool_use,
                config: tool_runtime.config.clone(),
                registry: tool_runtime.registry.clone(),
                limits: tool_runtime.limits,
                action_sender: tool_runtime.action_sender.clone(),
                mutating_tools_allowed: true,
                continue_after_tools: true,
            });
        }
        ApprovalAction::Allow => {
            spawn_tool_execution(ToolExecutionRequest {
                branch: store.branch_token(),
                message_id: decision.message_id,
                tool_use,
                config: tool_runtime.config.clone(),
                registry: tool_runtime.registry.clone(),
                limits: tool_runtime.limits,
                action_sender: tool_runtime.action_sender.clone(),
                mutating_tools_allowed: true,
                continue_after_tools: true,
            });
        }
        ApprovalAction::Deny => {
            let call_id = tool_use.call_id.clone();
            store.upsert_tool_result(
                call_id.clone(),
                denied_tool_result(&call_id, &tool_use.name),
            );
            let _ = maybe_continue_deepseek_tool_loop(
                store,
                tool_runtime,
                decision.message_id,
                true,
                true,
            );
        }
    }
    tool_runtime.transcript_status.sync(store);
}

pub(crate) fn tool_use_for_approval(
    store: &ChatMessageStore,
    decision: &ApprovalDecision,
) -> Option<ToolUseBlock> {
    store
        .with_block(decision.block_id, |block| match block {
            ChatBlock::ToolUse(tool_use) => Some(tool_use.clone()),
            _ => None,
        })
        .flatten()
        .filter(|tool_use| {
            tool_use
                .approval
                .as_ref()
                .is_some_and(|approval| approval.id == decision.approval_id)
        })
}

pub(crate) fn denied_tool_result(call_id: &str, tool_name: &str) -> ToolResultBlock {
    failed_tool_result(
        call_id,
        format!("User denied tool call `{tool_name}`. The tool was not executed."),
    )
}

pub(crate) fn failed_tool_result(call_id: &str, output: impl Into<String>) -> ToolResultBlock {
    ToolResultBlock {
        id: ChatBlockId::new(0),
        call_id: call_id.to_string(),
        ok: false,
        exit_code: None,
        output: ToolOutput::Markdown(output.into()),
        collapsed: false,
    }
}

pub(crate) fn spawn_tool_execution(request: ToolExecutionRequest) {
    thread::spawn(move || {
        let call_id = request.tool_use.call_id.clone();
        let tool_block_id = request.tool_use.id;
        let result = execute_tool_use_to_result_block(
            &request.registry,
            &request.config,
            &request.tool_use,
            request.limits,
        );
        let _ = request.action_sender.send(AppAction::ToolResultReady {
            branch: request.branch,
            message_id: request.message_id,
            tool_block_id,
            call_id,
            result,
            mutating_tools_allowed: request.mutating_tools_allowed,
            continue_after_tools: request.continue_after_tools,
        });
    });
}

pub(crate) fn execute_tool_use_to_result_block(
    registry: &ToolRegistry,
    config: &AgentConfig,
    tool_use: &ToolUseBlock,
    limits: AgentTurnLimits,
) -> ToolResultBlock {
    let result = execute_tool_use_with_timeout(registry, config, tool_use, limits);
    match result {
        Ok(result) => tool_result_block(&tool_use.call_id, result),
        Err(error) => failed_tool_result(
            &tool_use.call_id,
            format!("Tool `{}` failed: {error:#}", tool_use.name),
        ),
    }
}

pub(crate) fn execute_tool_use_with_timeout(
    registry: &ToolRegistry,
    config: &AgentConfig,
    tool_use: &ToolUseBlock,
    limits: AgentTurnLimits,
) -> Result<ToolResult> {
    let args = tool_input_to_json(&tool_use.input)?;
    let tool_name = tool_use.name.clone();
    let timeout = limits.tool_timeout;
    let registry = registry.clone();
    let ctx = ToolContext::new(config.workspace.clone()).with_timeout(timeout);
    let (sender, receiver) = mpsc::channel();

    let worker_tool_name = tool_name.clone();
    thread::spawn(move || {
        let result = registry.execute(&worker_tool_name, ctx, args);
        let _ = sender.send(result);
    });

    match receiver.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(ToolResult::failure(
            format!(
                "Tool `{tool_name}` timed out after {}.",
                format_duration(timeout)
            ),
            ToolOutputKind::Markdown,
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(anyhow::anyhow!(
            "tool `{tool_name}` execution ended without returning a result"
        )),
    }
}

pub(crate) fn format_duration(duration: Duration) -> String {
    if duration.as_secs() > 0 && duration.subsec_millis() == 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

pub(crate) fn tool_input_to_json(input: &ToolInput) -> Result<Value> {
    match input {
        ToolInput::Text(text) => match serde_json::from_str(text) {
            Ok(value) => Ok(value),
            Err(_) => Ok(Value::String(text.clone())),
        },
        ToolInput::Json(value) => Ok(component_value_to_json(value)),
    }
}

pub(crate) fn tool_result_block(call_id: &str, result: ToolResult) -> ToolResultBlock {
    ToolResultBlock {
        id: ChatBlockId::new(0),
        call_id: call_id.to_string(),
        ok: result.ok,
        exit_code: result.exit_code,
        output: tool_output_from_result(result.output_kind, result.output),
        collapsed: false,
    }
}

pub(crate) fn tool_output_from_result(kind: ToolOutputKind, output: String) -> ToolOutput {
    match kind {
        ToolOutputKind::Ansi => ToolOutput::Ansi(output),
        ToolOutputKind::Markdown => ToolOutput::Markdown(output),
        ToolOutputKind::Diff => ToolOutput::Diff(DiffData { unified: output }),
    }
}
