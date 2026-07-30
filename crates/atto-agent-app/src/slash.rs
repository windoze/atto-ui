//! Slash-command handling and session operations (clear, plan mode, abort,
//! cancel, help, skill listing, tool listing).

use crate::*;

pub(crate) fn agent_slash_commands() -> Vec<ChatSlashCommand> {
    vec![
        ChatSlashCommand::new("/help")
            .detail("Show available commands")
            .submit_on_accept(),
        ChatSlashCommand::new("/clear")
            .detail("Clear the conversation")
            .submit_on_accept(),
        ChatSlashCommand::new("/plan")
            .detail("Cycle plan mode, or type /plan on|off|auto")
            .submit_on_accept(),
        ChatSlashCommand::new("/skills")
            .detail("List available skills")
            .submit_on_accept(),
        ChatSlashCommand::new("/skill")
            .detail("Activate a skill by name")
            .submit_on_accept(),
        ChatSlashCommand::new("/tools")
            .detail("List available tools")
            .submit_on_accept(),
        ChatSlashCommand::new("/abort")
            .detail("Cancel the active turn")
            .submit_on_accept(),
    ]
}

pub(crate) fn submit_slash_command_text(store: &ChatMessageStore, runtime: &SlashRuntime, text: &str) -> bool {
    let trimmed = text.trim();
    let Some(rest) = trimmed.strip_prefix('/') else {
        return false;
    };
    let mut parts = rest.split_whitespace();
    let Some(command) = parts.next().filter(|command| !command.is_empty()) else {
        return false;
    };
    let args = parts.collect::<Vec<_>>();
    let normalized = command.to_ascii_lowercase();

    match normalized.as_str() {
        "help" => append_system_message(store, help_text()),
        "clear" => clear_session(
            store,
            &runtime.input_handle,
            &runtime.mock_turns,
            &runtime.status_state,
            &runtime.turn_budgets,
        ),
        "plan" => apply_plan_command(store, &runtime.plan_mode_state, &args),
        "skills" => append_system_message(
            store,
            skills_text(&runtime.skill_registry, &runtime.loaded_skills),
        ),
        "skill" => apply_skill_command(
            store,
            &runtime.skill_registry,
            &runtime.loaded_skills,
            &runtime.skill_count_state,
            &args,
        ),
        "tools" => append_system_message(store, tools_text()),
        "abort" => apply_abort_command(
            store,
            &runtime.input_handle,
            &runtime.mock_turns,
            &runtime.status_state,
            &runtime.transcript_status,
            &runtime.turn_budgets,
        ),
        _ => append_system_message(
            store,
            format!("Unknown slash command `/{command}`. Type `/help` for available commands."),
        ),
    }
    runtime.transcript_status.sync(store);
    true
}

pub(crate) fn append_system_message(store: &ChatMessageStore, text: impl Into<String>) {
    let message_id = store.next_message_id();
    store.push(ChatMessage::text(message_id, ChatRole::System, text.into()));
}

pub(crate) fn clear_session(
    store: &ChatMessageStore,
    input_handle: &ChatInputHandle,
    mock_turns: &MockTurnRegistry,
    status_state: &Property<String>,
    turn_budgets: &TurnBudgetTracker,
) {
    if let Some(message_id) = store
        .messages()
        .iter()
        .rev()
        .find(|message| message.status.is_streaming())
        .map(|message| message.id)
    {
        let _ = mock_turns.cancel(message_id);
    }
    store.replace_all(Vec::new());
    input_handle.streaming_binding().set(false);
    input_handle.clear_queued_responses();
    input_handle.clear_references();
    status_state.set(STATUS_READY.to_string());
    turn_budgets.clear();
}

pub(crate) fn apply_plan_command(store: &ChatMessageStore, plan_mode_state: &Property<String>, args: &[&str]) {
    let current = plan_mode_from_status(&plan_mode_state.get()).unwrap_or(PlanMode::Off);
    let next = match args {
        [] => Some(current.next()),
        [value] => value.parse().ok(),
        _ => None,
    };
    let Some(next) = next else {
        append_system_message(store, "Usage: /plan [on|off|auto]");
        return;
    };

    plan_mode_state.set(next.status());
    append_system_message(store, format!("Plan mode set to {next}."));
}

pub(crate) fn plan_mode_from_status(status: &str) -> Option<PlanMode> {
    status.strip_prefix("plan: ")?.parse().ok()
}

pub(crate) fn apply_abort_command(
    store: &ChatMessageStore,
    input_handle: &ChatInputHandle,
    mock_turns: &MockTurnRegistry,
    status_state: &Property<String>,
    transcript_status: &TranscriptStatusState,
    turn_budgets: &TurnBudgetTracker,
) {
    if cancel_latest_streaming_turn(
        store,
        input_handle,
        mock_turns,
        status_state,
        transcript_status,
        turn_budgets,
    ) {
        append_system_message(store, "Aborted active turn.");
    } else {
        append_system_message(store, "No active turn to abort.");
    }
}

pub(crate) fn cancel_latest_streaming_turn(
    store: &ChatMessageStore,
    input_handle: &ChatInputHandle,
    mock_turns: &MockTurnRegistry,
    status_state: &Property<String>,
    transcript_status: &TranscriptStatusState,
    turn_budgets: &TurnBudgetTracker,
) -> bool {
    let Some(message_id) = store
        .messages()
        .iter()
        .rev()
        .find(|message| message.status.is_streaming())
        .map(|message| message.id)
    else {
        return false;
    };

    if !store.cancel_streaming_turn(message_id) {
        return false;
    }
    finish_canceled_turn(
        store,
        input_handle,
        mock_turns,
        status_state,
        transcript_status,
        turn_budgets,
        message_id,
    );
    true
}

pub(crate) fn finish_canceled_turn(
    store: &ChatMessageStore,
    input_handle: &ChatInputHandle,
    mock_turns: &MockTurnRegistry,
    status_state: &Property<String>,
    transcript_status: &TranscriptStatusState,
    turn_budgets: &TurnBudgetTracker,
    message_id: ChatMessageId,
) {
    let _ = mock_turns.cancel(message_id);
    turn_budgets.finish_turn(message_id);
    input_handle.streaming_binding().set(false);
    status_state.set(STATUS_READY.to_string());
    transcript_status.sync(store);
}

pub(crate) fn help_text() -> &'static str {
    "Available commands:\n\
- /help: Show this help.\n\
- /clear: Clear the current conversation and keep app configuration.\n\
- /plan [on|off|auto]: Cycle or set the basic plan mode state.\n\
- /skills: List available skills.\n\
- `/skill <name>`: Activate a skill for this session.\n\
- /tools: List available tools and approval policy.\n\
- /abort: Cancel the active turn."
}

pub(crate) fn apply_skill_command(
    store: &ChatMessageStore,
    registry: &SkillRegistry,
    loaded_skills: &LoadedSkillSet,
    skill_count_state: &Property<String>,
    args: &[&str],
) {
    let [name] = args else {
        append_system_message(store, "Usage: /skill <name>");
        return;
    };
    let name = *name;

    let Some(skill) = registry.get(name) else {
        append_system_message(
            store,
            format!("Skill `{name}` not found. Type `/skills` to list available skills."),
        );
        return;
    };

    if loaded_skills.insert(skill.definition.name.clone()) {
        skill_count_state.set(loaded_skills.status());
        append_system_message(
            store,
            format!(
                "Loaded skill `{}`: {}",
                skill.definition.name, skill.definition.description
            ),
        );
    } else {
        append_system_message(
            store,
            format!("Skill `{}` is already active.", skill.definition.name),
        );
    }
}

pub(crate) fn skills_text(registry: &SkillRegistry, loaded_skills: &LoadedSkillSet) -> String {
    let mut text = format!(
        "Skills: {} discovered, {} loaded.\n",
        registry.len(),
        loaded_skills.len()
    );
    if registry.is_empty() {
        text.push_str("No skills found in .atto/skills or ~/.config/atto-agent/skills.\n");
    } else {
        for skill in registry.skills() {
            let loaded = if loaded_skills.contains(&skill.definition.name) {
                "loaded"
            } else {
                "available"
            };
            text.push_str(&format!(
                "- [{}] {}: {} (mode: {}, source: {}, path: {})\n",
                loaded,
                skill.definition.name,
                skill.definition.description,
                skill.definition.mode,
                skill.source,
                skill.path.display()
            ));
        }
    }
    if !registry.issues().is_empty() {
        text.push_str("Discovery issues:\n");
        for issue in registry.issues() {
            text.push_str(&format!("- {issue}\n"));
        }
    }
    if loaded_skills.is_empty() {
        text.push_str("No skills loaded. Type `/skill <name>` to activate one.");
    } else {
        text.push_str(&format!(
            "Loaded skills: {}.",
            loaded_skills.names().join(", ")
        ));
    }
    text
}

pub(crate) fn tools_text() -> String {
    let registry =
        crate::tool::builtin_tool_registry().expect("built-in tool registry must be valid");
    let mut text = format!("Tools: {} registered.\n", registry.len());
    for spec in registry.specs() {
        text.push_str(&format!(
            "- {}: {} (permission: {}, output: {})\n",
            spec.name,
            spec.description,
            tool_permission_label(spec.permission),
            tool_output_label(spec.output)
        ));
    }
    text.push_str("Mutating tools require approval before execution.");
    text
}

pub(crate) fn tool_permission_label(permission: crate::tool::ToolPermission) -> &'static str {
    match permission {
        crate::tool::ToolPermission::AlwaysAllow => "allow",
        crate::tool::ToolPermission::ApproveOnce => "approve once",
        crate::tool::ToolPermission::ApproveForProject => "approve for project",
        crate::tool::ToolPermission::NeverAllow => "deny",
    }
}

pub(crate) fn tool_output_label(output: crate::tool::ToolOutputKind) -> &'static str {
    match output {
        crate::tool::ToolOutputKind::Ansi => "ansi",
        crate::tool::ToolOutputKind::Markdown => "markdown",
        crate::tool::ToolOutputKind::Diff => "diff",
    }
}
