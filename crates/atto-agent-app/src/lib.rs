#![forbid(unsafe_code)]

//! Application crate for the Atto TUI agent.
//!
//! The crate is intentionally thin at this stage: later milestones will compose
//! `atto-ui`, `atto-ui-chat`, and `atto-ui-async` here without adding network
//! dependencies to the reusable UI crates.

use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use atto_ui::CancellationToken;
use atto_ui::ComponentValue;
use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, MenuItem, MenuSpec,
    StatusSegment, StatusSegmentAlign, run_crossterm_desktop_with_actions,
};
use atto_ui::reactive::{Binding, EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowId, WindowKind};
use atto_ui_chat::{
    ApprovalAction, ApprovalDecision, ApprovalLevel, ApprovalOption, ApprovalRequest, ChatBlock,
    ChatBlockId, ChatBranchToken, ChatError, ChatInputHandle, ChatInputResponse, ChatMessage,
    ChatMessageId, ChatMessageList, ChatMessageMeta, ChatMessageStore, ChatPanel, ChatRole,
    ChatSlashCommand, ChatTurnStatus, DiffData, ThinkingBlock, ToolInput, ToolOutput,
    ToolResultBlock, ToolStatus, ToolUseBlock,
};
use ratatui::layout::Rect;
use serde_json::{Map, Number, Value};

pub mod config;
pub mod deepseek;
pub mod deepseek_client;
mod limits;
pub mod skill;
mod stream_ui;
pub mod tool;

use crate::config::{AgentConfig, PlanMode};
use crate::deepseek::{
    ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionDelta, ChatCompletionMessage,
    ChatCompletionRequest, ChatCompletionSseEvent, ChatFunctionCall, ChatFunctionCallDelta,
    ChatToolCall, ChatToolCallDelta, ChatToolKind, FinishReason, ToolChoice, ToolChoiceMode,
};
use crate::limits::{AgentTurnLimits, TurnBudgetTracker};
use crate::skill::{LoadedSkillSet, SkillRegistry};
use crate::stream_ui::DeepSeekUiStream;
use crate::tool::{
    ToolContext, ToolOutputKind, ToolPermissionDecision, ToolPermissionPolicy, ToolRegistry,
    ToolResult,
};

pub const APP_TITLE: &str = "Atto Agent";
const CHAT_WINDOW_TAG: &str = "atto-agent:chat";
const STATUS_READY: &str = "ready";
const STATUS_STREAMING: &str = "streaming";
const MOCK_TOKEN_DELAY: Duration = Duration::from_millis(24);
const SNAPSHOT_MOCK_TOKEN_DELAY: Duration = Duration::from_millis(96);
const MOCK_READ_FILE_PROMPT: &str = "agent-pty-read-file";
const MOCK_RUN_COMMAND_PROMPT: &str = "agent-pty-run-command";

#[derive(Clone, Debug)]
enum AppAction {
    TextDelta {
        branch: ChatBranchToken,
        block_id: ChatBlockId,
        delta: String,
    },
    ThinkingDelta {
        branch: ChatBranchToken,
        message_id: ChatMessageId,
        delta: String,
    },
    ToolCallsReady {
        branch: ChatBranchToken,
        message_id: ChatMessageId,
        tool_calls: Vec<ToolUseBlock>,
    },
    ToolResultReady {
        branch: ChatBranchToken,
        tool_block_id: ChatBlockId,
        call_id: String,
        result: ToolResultBlock,
    },
    TurnDone {
        branch: ChatBranchToken,
        message_id: ChatMessageId,
        meta: Option<ChatMessageMeta>,
    },
    TurnFailed {
        branch: ChatBranchToken,
        message_id: ChatMessageId,
        error: ChatError,
    },
}

#[derive(Clone, Debug)]
struct MockAgentTurnRequest {
    branch: ChatBranchToken,
    message_id: ChatMessageId,
    block_id: ChatBlockId,
    cancel: CancellationToken,
    token_delay: Duration,
    model: String,
    prompt: String,
}

#[derive(Clone)]
struct ToolExecutionRequest {
    branch: ChatBranchToken,
    tool_use: ToolUseBlock,
    config: AgentConfig,
    registry: ToolRegistry,
    limits: AgentTurnLimits,
    action_sender: mpsc::Sender<AppAction>,
}

#[derive(Clone, Debug)]
struct SlashRuntime {
    input_handle: ChatInputHandle,
    mock_turns: MockTurnRegistry,
    status_state: Property<String>,
    plan_mode_state: Property<String>,
    skill_registry: SkillRegistry,
    loaded_skills: LoadedSkillSet,
    skill_count_state: Property<String>,
    turn_budgets: TurnBudgetTracker,
}

#[derive(Clone, Debug)]
struct AgentTurnLauncher {
    model: String,
    action_sender: mpsc::Sender<AppAction>,
    turn_budgets: TurnBudgetTracker,
    limits: AgentTurnLimits,
}

#[derive(Clone, Debug)]
struct MockTurnRegistry {
    current: Arc<Mutex<Option<ActiveMockTurn>>>,
    token_delay: Duration,
}

#[derive(Clone, Debug)]
struct ActiveMockTurn {
    message_id: ChatMessageId,
    cancel: CancellationToken,
}

#[derive(Clone)]
struct AgentRuntime {
    config: AgentConfig,
    action_sender: mpsc::Sender<AppAction>,
    mock_turns: MockTurnRegistry,
    skill_registry: SkillRegistry,
    loaded_skills: LoadedSkillSet,
    tool_registry: ToolRegistry,
    tool_permissions: Arc<Mutex<ToolPermissionPolicy>>,
    turn_budgets: TurnBudgetTracker,
    limits: AgentTurnLimits,
    message_store: ChatMessageStore,
    input_handle: ChatInputHandle,
    status_state: Property<String>,
    model_state: Property<String>,
    plan_mode_state: Property<String>,
    skill_count_state: Property<String>,
}

#[derive(Clone)]
struct ToolRuntime {
    config: AgentConfig,
    action_sender: mpsc::Sender<AppAction>,
    registry: ToolRegistry,
    permissions: Arc<Mutex<ToolPermissionPolicy>>,
    turn_budgets: TurnBudgetTracker,
    limits: AgentTurnLimits,
}

impl AgentRuntime {
    fn new(
        config: AgentConfig,
        action_sender: mpsc::Sender<AppAction>,
        mock_turns: MockTurnRegistry,
    ) -> Self {
        let model_state = Property::new(format!("model: {}", config.model));
        let plan_mode_state = Property::new(config.plan_mode.status());
        let tool_registry =
            crate::tool::builtin_tool_registry().expect("built-in tool registry must be valid");
        let skill_registry = SkillRegistry::discover(&config.workspace, config.home_dir.as_deref());
        let loaded_skills = LoadedSkillSet::default();
        let limits = AgentTurnLimits::default();
        let turn_budgets = TurnBudgetTracker::default();
        let skill_count_state = Property::new(loaded_skills.status());
        Self {
            config,
            action_sender,
            mock_turns,
            skill_registry,
            loaded_skills,
            tool_registry,
            tool_permissions: Arc::new(Mutex::new(ToolPermissionPolicy::default())),
            turn_budgets,
            limits,
            message_store: ChatMessageStore::new(),
            input_handle: ChatInputHandle::new(),
            status_state: Property::new(STATUS_READY.to_string()),
            model_state,
            plan_mode_state,
            skill_count_state,
        }
    }

    fn tool_runtime(&self) -> ToolRuntime {
        ToolRuntime {
            config: self.config.clone(),
            action_sender: self.action_sender.clone(),
            registry: self.tool_registry.clone(),
            permissions: self.tool_permissions.clone(),
            turn_budgets: self.turn_budgets.clone(),
            limits: self.limits,
        }
    }

    fn slash_runtime(&self) -> SlashRuntime {
        SlashRuntime {
            input_handle: self.input_handle.clone(),
            mock_turns: self.mock_turns.clone(),
            status_state: self.status_state.clone(),
            plan_mode_state: self.plan_mode_state.clone(),
            skill_registry: self.skill_registry.clone(),
            loaded_skills: self.loaded_skills.clone(),
            skill_count_state: self.skill_count_state.clone(),
            turn_budgets: self.turn_budgets.clone(),
        }
    }
}

impl MockTurnRegistry {
    fn new() -> Self {
        Self::with_token_delay(MOCK_TOKEN_DELAY)
    }

    fn with_token_delay(token_delay: Duration) -> Self {
        Self {
            current: Arc::new(Mutex::new(None)),
            token_delay,
        }
    }

    fn token_delay(&self) -> Duration {
        self.token_delay
    }

    fn start(&self, message_id: ChatMessageId) -> CancellationToken {
        let cancel = CancellationToken::new();
        *self.current.lock().expect("mock turn lock poisoned") = Some(ActiveMockTurn {
            message_id,
            cancel: cancel.clone(),
        });
        cancel
    }

    fn cancel(&self, message_id: ChatMessageId) -> bool {
        let mut current = self.current.lock().expect("mock turn lock poisoned");
        let Some(turn) = current
            .as_ref()
            .filter(|turn| turn.message_id == message_id)
        else {
            return false;
        };
        turn.cancel.cancel();
        *current = None;
        true
    }

    fn clear(&self, message_id: ChatMessageId) {
        let mut current = self.current.lock().expect("mock turn lock poisoned");
        if current
            .as_ref()
            .is_some_and(|turn| turn.message_id == message_id)
        {
            *current = None;
        }
    }
}

/// Runtime state for the single-window agent UI.
pub struct AgentApp {
    config: AgentConfig,
    desktop: Desktop,
    message_store: ChatMessageStore,
    input_handle: ChatInputHandle,
    status_state: Property<String>,
    model_state: Property<String>,
    plan_mode_state: Property<String>,
    skill_count_state: Property<String>,
    skill_registry: SkillRegistry,
    loaded_skills: LoadedSkillSet,
    chat_window_id: WindowId,
}

impl AgentApp {
    /// Builds the initial desktop, status bar, chat panel, and chat state handles.
    pub fn new(screen: Rect) -> Self {
        let (action_sender, _action_receiver) = EventQueue::<AppAction>::channel();
        let runtime = AgentRuntime::new(
            AgentConfig::defaults("."),
            action_sender,
            MockTurnRegistry::new(),
        );
        Self::with_runtime_state(screen, EventQueue::new(), runtime)
    }

    /// Builds the initial app state from a resolved configuration.
    pub fn with_config(screen: Rect, config: AgentConfig) -> Self {
        let (action_sender, _action_receiver) = EventQueue::<AppAction>::channel();
        let runtime = AgentRuntime::new(config, action_sender, MockTurnRegistry::new());
        Self::with_runtime_state(screen, EventQueue::new(), runtime)
    }

    fn with_runtime_state(
        screen: Rect,
        quit_events: EventQueue<()>,
        runtime: AgentRuntime,
    ) -> Self {
        let chat_panel = build_chat_panel(
            &runtime.message_store,
            AgentTurnLauncher {
                model: runtime.config.model.clone(),
                action_sender: runtime.action_sender.clone(),
                turn_budgets: runtime.turn_budgets.clone(),
                limits: runtime.limits,
            },
            runtime.slash_runtime(),
            runtime.tool_runtime(),
        );

        let mut desktop = Desktop::new(Theme::dark(), agent_menu(quit_events));
        desktop.status.set_segments(status_segments(
            runtime.model_state.binding(),
            runtime.status_state.binding(),
            runtime.plan_mode_state.binding(),
            runtime.skill_count_state.binding(),
        ));

        let chat_window_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                APP_TITLE,
                chat_window_rect(screen),
                Box::new(chat_panel),
            )
            .with_tag(CHAT_WINDOW_TAG)
            .with_min_size(32, 12),
            screen,
        );

        Self {
            config: runtime.config,
            desktop,
            message_store: runtime.message_store,
            input_handle: runtime.input_handle,
            status_state: runtime.status_state,
            model_state: runtime.model_state,
            plan_mode_state: runtime.plan_mode_state,
            skill_count_state: runtime.skill_count_state,
            skill_registry: runtime.skill_registry,
            loaded_skills: runtime.loaded_skills,
            chat_window_id,
        }
    }

    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    pub fn desktop(&self) -> &Desktop {
        &self.desktop
    }

    pub fn desktop_mut(&mut self) -> &mut Desktop {
        &mut self.desktop
    }

    pub fn into_desktop(self) -> Desktop {
        self.desktop
    }

    pub fn message_store(&self) -> ChatMessageStore {
        self.message_store.clone()
    }

    pub fn input_handle(&self) -> ChatInputHandle {
        self.input_handle.clone()
    }

    pub fn status_state(&self) -> Property<String> {
        self.status_state.clone()
    }

    pub fn model_state(&self) -> Property<String> {
        self.model_state.clone()
    }

    pub fn plan_mode_state(&self) -> Property<String> {
        self.plan_mode_state.clone()
    }

    pub fn skill_count_state(&self) -> Property<String> {
        self.skill_count_state.clone()
    }

    pub fn skill_registry(&self) -> &SkillRegistry {
        &self.skill_registry
    }

    pub fn loaded_skills(&self) -> LoadedSkillSet {
        self.loaded_skills.clone()
    }

    pub fn chat_window_id(&self) -> WindowId {
        self.chat_window_id
    }
}

/// Runs the TUI agent application.
pub fn run() -> Result<()> {
    run_with_config_and_mock_token_delay(AgentConfig::load()?, MOCK_TOKEN_DELAY)
}

/// Runs the deterministic mock fixture used by PTY snapshot tests.
pub fn run_snapshot_fixture() -> Result<()> {
    run_with_config_and_mock_token_delay(
        AgentConfig::defaults(env!("CARGO_MANIFEST_DIR")),
        SNAPSHOT_MOCK_TOKEN_DELAY,
    )
}

fn run_with_config_and_mock_token_delay(
    config: AgentConfig,
    mock_token_delay: Duration,
) -> Result<()> {
    let quit_events = EventQueue::new();
    let quit_events_for_menu = quit_events.clone();
    let quit_events_for_loop = quit_events.clone();
    let (action_sender, action_receiver) = EventQueue::<AppAction>::channel();
    let runtime = AgentRuntime::new(
        config,
        action_sender.clone(),
        MockTurnRegistry::with_token_delay(mock_token_delay),
    );
    let runtime_for_build = runtime.clone();
    let runtime_for_actions = runtime.clone();

    run_crossterm_desktop_with_actions(
        CrosstermAppConfig::default()
            .bracketed_paste(true)
            .cursor(CursorMode::Show),
        move |screen| {
            Ok(AgentApp::with_runtime_state(
                screen,
                quit_events_for_menu,
                runtime_for_build.clone(),
            )
            .into_desktop())
        },
        action_receiver,
        move |_desktop, action, _screen| {
            let tool_runtime = runtime_for_actions.tool_runtime();
            apply_app_action(
                &runtime_for_actions.message_store,
                &runtime_for_actions.input_handle,
                &runtime_for_actions.mock_turns,
                &runtime_for_actions.status_state,
                &tool_runtime,
                action,
            );
            Ok(AppControl::Continue)
        },
        move |_desktop, _screen| {
            if quit_events_for_loop.pop().is_some() {
                Ok(AppControl::Exit)
            } else {
                Ok(AppControl::Continue)
            }
        },
        |_desktop, _event, _screen, _result| Ok(AppControl::Continue),
    )
}

fn build_chat_panel(
    store: &ChatMessageStore,
    turn_launcher: AgentTurnLauncher,
    slash_runtime: SlashRuntime,
    tool_runtime: ToolRuntime,
) -> ChatPanel {
    // Compose the reusable chat list and input controls around shared state handles.
    let input_handle_for_cancel = slash_runtime.input_handle.clone();
    let status_for_cancel = slash_runtime.status_state.clone();
    let mock_turns_for_cancel = slash_runtime.mock_turns.clone();
    let turn_budgets_for_cancel = slash_runtime.turn_budgets.clone();
    let store_for_approval = store.clone();
    let tool_runtime_for_approval = tool_runtime.clone();
    let list = ChatMessageList::new(store.clone())
        .show_timestamps(false)
        .on_approve(move |decision| {
            handle_tool_approval(&store_for_approval, &tool_runtime_for_approval, decision);
        })
        .on_cancel(move |message_id| {
            finish_canceled_turn(
                &input_handle_for_cancel,
                &mock_turns_for_cancel,
                &status_for_cancel,
                &turn_budgets_for_cancel,
                message_id,
            );
        });
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

fn submit_input_response(
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

    let user_id = store.next_message_id();
    store.push(ChatMessage::text(user_id, ChatRole::User, text.clone()));

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
        return;
    }
    let cancel = slash_runtime.mock_turns.start(assistant_id);

    slash_runtime.input_handle.streaming_binding().set(true);
    slash_runtime.status_state.set(STATUS_STREAMING.to_string());
    spawn_mock_agent_turn(
        turn_launcher.action_sender.clone(),
        MockAgentTurnRequest {
            branch,
            message_id: assistant_id,
            block_id: text_block_id,
            cancel,
            token_delay: slash_runtime.mock_turns.token_delay(),
            model: turn_launcher.model.clone(),
            prompt: text,
        },
    );
}

fn input_response_text(response: ChatInputResponse) -> String {
    match response {
        ChatInputResponse::Text(text) | ChatInputResponse::Custom(text) => text,
        ChatInputResponse::Choice { label, .. } => label,
    }
}

fn agent_slash_commands() -> Vec<ChatSlashCommand> {
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
            .detail("Cancel the active mock turn")
            .submit_on_accept(),
    ]
}

fn submit_slash_command_text(store: &ChatMessageStore, runtime: &SlashRuntime, text: &str) -> bool {
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
            &runtime.turn_budgets,
        ),
        _ => append_system_message(
            store,
            format!("Unknown slash command `/{command}`. Type `/help` for available commands."),
        ),
    }
    true
}

fn append_system_message(store: &ChatMessageStore, text: impl Into<String>) {
    let message_id = store.next_message_id();
    store.push(ChatMessage::text(message_id, ChatRole::System, text.into()));
}

fn clear_session(
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

fn apply_plan_command(store: &ChatMessageStore, plan_mode_state: &Property<String>, args: &[&str]) {
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

fn plan_mode_from_status(status: &str) -> Option<PlanMode> {
    status.strip_prefix("plan: ")?.parse().ok()
}

fn apply_abort_command(
    store: &ChatMessageStore,
    input_handle: &ChatInputHandle,
    mock_turns: &MockTurnRegistry,
    status_state: &Property<String>,
    turn_budgets: &TurnBudgetTracker,
) {
    if cancel_latest_streaming_turn(store, input_handle, mock_turns, status_state, turn_budgets) {
        append_system_message(store, "Aborted active turn.");
    } else {
        append_system_message(store, "No active turn to abort.");
    }
}

fn cancel_latest_streaming_turn(
    store: &ChatMessageStore,
    input_handle: &ChatInputHandle,
    mock_turns: &MockTurnRegistry,
    status_state: &Property<String>,
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
        input_handle,
        mock_turns,
        status_state,
        turn_budgets,
        message_id,
    );
    true
}

fn finish_canceled_turn(
    input_handle: &ChatInputHandle,
    mock_turns: &MockTurnRegistry,
    status_state: &Property<String>,
    turn_budgets: &TurnBudgetTracker,
    message_id: ChatMessageId,
) {
    let _ = mock_turns.cancel(message_id);
    turn_budgets.finish_turn(message_id);
    input_handle.streaming_binding().set(false);
    status_state.set(STATUS_READY.to_string());
}

fn help_text() -> &'static str {
    "Available commands:\n\
- /help: Show this help.\n\
- /clear: Clear the current conversation and keep app configuration.\n\
- /plan [on|off|auto]: Cycle or set the basic plan mode state.\n\
- /skills: List available skills.\n\
- `/skill <name>`: Activate a skill for this session.\n\
- /tools: List available tools and approval policy.\n\
- /abort: Cancel the active mock turn."
}

fn apply_skill_command(
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

fn skills_text(registry: &SkillRegistry, loaded_skills: &LoadedSkillSet) -> String {
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

fn tools_text() -> String {
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

fn tool_permission_label(permission: crate::tool::ToolPermission) -> &'static str {
    match permission {
        crate::tool::ToolPermission::AlwaysAllow => "allow",
        crate::tool::ToolPermission::ApproveOnce => "approve once",
        crate::tool::ToolPermission::ApproveForProject => "approve for project",
        crate::tool::ToolPermission::NeverAllow => "deny",
    }
}

fn tool_output_label(output: crate::tool::ToolOutputKind) -> &'static str {
    match output {
        crate::tool::ToolOutputKind::Ansi => "ansi",
        crate::tool::ToolOutputKind::Markdown => "markdown",
        crate::tool::ToolOutputKind::Diff => "diff",
    }
}

fn spawn_mock_agent_turn(action_sender: mpsc::Sender<AppAction>, request: MockAgentTurnRequest) {
    thread::spawn(move || {
        let mut stream = DeepSeekUiStream::new(
            request.branch,
            request.message_id,
            request.block_id,
            request.model,
        );
        for event in mock_agent_events(&request.prompt) {
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

fn send_stream_actions(action_sender: &mpsc::Sender<AppAction>, actions: Vec<AppAction>) -> bool {
    for action in actions {
        if action_sender.send(action).is_err() {
            return false;
        }
    }
    true
}

fn mock_agent_events(prompt: &str) -> Vec<ChatCompletionSseEvent> {
    match prompt.trim() {
        MOCK_READ_FILE_PROMPT => vec![
            mock_stream_tool_call_event(
                "call_read_cargo",
                "read_file",
                serde_json::json!({ "path": "Cargo.toml" }),
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

fn mock_stream_content_event(delta: String) -> ChatCompletionSseEvent {
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

fn mock_stream_tool_call_event(
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

fn mock_stream_finish_event() -> ChatCompletionSseEvent {
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

fn mock_agent_deltas(prompt: &str) -> Vec<String> {
    vec![
        "Mock assistant: ".to_string(),
        prompt.trim().to_string(),
        "\n".to_string(),
        "Done.".to_string(),
    ]
}

fn apply_app_action(
    store: &ChatMessageStore,
    input_handle: &ChatInputHandle,
    mock_turns: &MockTurnRegistry,
    status_state: &Property<String>,
    tool_runtime: &ToolRuntime,
    action: AppAction,
) -> bool {
    match action {
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
                return found;
            }
            for tool_call in tool_calls {
                let PreparedToolCall { tool_use, result } =
                    prepare_tool_call(tool_call, &tool_runtime.registry, &tool_runtime.permissions);
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
                            tool_use,
                            config: tool_runtime.config.clone(),
                            registry: tool_runtime.registry.clone(),
                            limits: tool_runtime.limits,
                            action_sender: tool_runtime.action_sender.clone(),
                        });
                    }
                    None => {}
                }
            }
            true
        }
        AppAction::ToolResultReady {
            branch,
            tool_block_id,
            call_id,
            result,
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
    }
}

#[derive(Clone, Debug)]
struct PreparedToolCall {
    tool_use: ToolUseBlock,
    result: Option<ToolResultBlock>,
}

fn prepare_tool_call(
    mut tool_use: ToolUseBlock,
    registry: &ToolRegistry,
    permissions: &Arc<Mutex<ToolPermissionPolicy>>,
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

fn tool_approval_request(tool_use: &ToolUseBlock, allow_project: bool) -> ApprovalRequest {
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

fn handle_tool_approval(
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
                tool_use,
                config: tool_runtime.config.clone(),
                registry: tool_runtime.registry.clone(),
                limits: tool_runtime.limits,
                action_sender: tool_runtime.action_sender.clone(),
            });
        }
        ApprovalAction::Allow => {
            spawn_tool_execution(ToolExecutionRequest {
                branch: store.branch_token(),
                tool_use,
                config: tool_runtime.config.clone(),
                registry: tool_runtime.registry.clone(),
                limits: tool_runtime.limits,
                action_sender: tool_runtime.action_sender.clone(),
            });
        }
        ApprovalAction::Deny => {
            let call_id = tool_use.call_id.clone();
            store.upsert_tool_result(
                call_id.clone(),
                denied_tool_result(&call_id, &tool_use.name),
            );
        }
    }
}

fn tool_use_for_approval(
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

fn denied_tool_result(call_id: &str, tool_name: &str) -> ToolResultBlock {
    failed_tool_result(
        call_id,
        format!("User denied tool call `{tool_name}`. The tool was not executed."),
    )
}

fn failed_tool_result(call_id: &str, output: impl Into<String>) -> ToolResultBlock {
    ToolResultBlock {
        id: ChatBlockId::new(0),
        call_id: call_id.to_string(),
        ok: false,
        exit_code: None,
        output: ToolOutput::Markdown(output.into()),
        collapsed: false,
    }
}

fn spawn_tool_execution(request: ToolExecutionRequest) {
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
            tool_block_id,
            call_id,
            result,
        });
    });
}

fn execute_tool_use_to_result_block(
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

fn execute_tool_use_with_timeout(
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

fn format_duration(duration: Duration) -> String {
    if duration.as_secs() > 0 && duration.subsec_millis() == 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

fn tool_input_to_json(input: &ToolInput) -> Result<Value> {
    match input {
        ToolInput::Text(text) => match serde_json::from_str(text) {
            Ok(value) => Ok(value),
            Err(_) => Ok(Value::String(text.clone())),
        },
        ToolInput::Json(value) => Ok(component_value_to_json(value)),
    }
}

fn tool_result_block(call_id: &str, result: ToolResult) -> ToolResultBlock {
    ToolResultBlock {
        id: ChatBlockId::new(0),
        call_id: call_id.to_string(),
        ok: result.ok,
        exit_code: result.exit_code,
        output: tool_output_from_result(result.output_kind, result.output),
        collapsed: false,
    }
}

fn tool_output_from_result(kind: ToolOutputKind, output: String) -> ToolOutput {
    match kind {
        ToolOutputKind::Ansi => ToolOutput::Ansi(output),
        ToolOutputKind::Markdown => ToolOutput::Markdown(output),
        ToolOutputKind::Diff => ToolOutput::Diff(DiffData { unified: output }),
    }
}

/// Builds the OpenAI-compatible request body for the current transcript.
pub fn deepseek_request_from_transcript(
    config: &AgentConfig,
    registry: &ToolRegistry,
    messages: &[ChatMessage],
) -> ChatCompletionRequest {
    ChatCompletionRequest::from_config(config, deepseek_messages_from_transcript(messages))
        .with_tools(registry.chat_tools())
        .with_tool_choice(ToolChoice::Mode(ToolChoiceMode::Auto))
}

/// Converts the UI transcript into DeepSeek/OpenAI-compatible chat messages.
pub fn deepseek_messages_from_transcript(messages: &[ChatMessage]) -> Vec<ChatCompletionMessage> {
    let mut result = Vec::new();
    for message in messages {
        match &message.role {
            ChatRole::User => push_text_message(
                &mut result,
                ChatCompletionMessage::user,
                text_content_for_message(message),
            ),
            ChatRole::System | ChatRole::Custom(_) => push_text_message(
                &mut result,
                ChatCompletionMessage::system,
                text_content_for_message(message),
            ),
            ChatRole::Assistant => push_assistant_messages(&mut result, message),
        }
    }
    result
}

fn push_text_message(
    result: &mut Vec<ChatCompletionMessage>,
    build: impl FnOnce(String) -> ChatCompletionMessage,
    content: String,
) {
    if !content.is_empty() {
        result.push(build(content));
    }
}

fn push_assistant_messages(result: &mut Vec<ChatCompletionMessage>, message: &ChatMessage) {
    let content = text_content_for_message(message);
    let tool_calls = message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::ToolUse(tool_use) => Some(chat_tool_call_from_tool_use(tool_use)),
            _ => None,
        })
        .collect::<Vec<_>>();

    if !content.is_empty() || !tool_calls.is_empty() {
        result.push(ChatCompletionMessage {
            role: crate::deepseek::ChatMessageRole::Assistant,
            content: (!content.is_empty()).then_some(content),
            reasoning_content: None,
            tool_calls,
            tool_call_id: None,
        });
    }

    for block in &message.blocks {
        if let ChatBlock::ToolResult(tool_result) = block {
            result.push(ChatCompletionMessage::tool(
                tool_result.call_id.clone(),
                tool_result_content(tool_result),
            ));
        }
    }
}

fn text_content_for_message(message: &ChatMessage) -> String {
    let mut content = String::new();
    for block in &message.blocks {
        match block {
            ChatBlock::Text(text) if !text.markdown.is_empty() => {
                push_section(&mut content, &text.markdown);
            }
            ChatBlock::Notice(notice) if !notice.text.is_empty() => {
                push_section(&mut content, &notice.text);
            }
            ChatBlock::Compact(compact) if !compact.summary.is_empty() => {
                push_section(&mut content, &compact.summary);
            }
            _ => {}
        }
    }
    content
}

fn push_section(content: &mut String, section: &str) {
    if !content.is_empty() {
        content.push_str("\n\n");
    }
    content.push_str(section);
}

fn chat_tool_call_from_tool_use(tool_use: &ToolUseBlock) -> ChatToolCall {
    ChatToolCall {
        id: tool_use.call_id.clone(),
        kind: ChatToolKind::Function,
        function: ChatFunctionCall {
            name: tool_use.name.clone(),
            arguments: tool_arguments_from_input(&tool_use.input),
        },
    }
}

fn tool_arguments_from_input(input: &ToolInput) -> String {
    match input {
        ToolInput::Text(text) if text.trim().is_empty() => "{}".to_string(),
        ToolInput::Text(text) => text.clone(),
        ToolInput::Json(value) => serde_json::to_string(&component_value_to_json(value))
            .unwrap_or_else(|_| "{}".to_string()),
    }
}

fn tool_result_content(result: &ToolResultBlock) -> String {
    let mut content = format!("ok: {}", result.ok);
    if let Some(exit_code) = result.exit_code {
        content.push_str(&format!("\nexit_code: {exit_code}"));
    }
    let output = result.output.as_text();
    if !output.is_empty() {
        content.push_str("\n\n");
        content.push_str(output);
    }
    content
}

fn component_value_to_json(value: &ComponentValue) -> Value {
    match value {
        ComponentValue::Null => Value::Null,
        ComponentValue::Bool(value) => Value::Bool(*value),
        ComponentValue::I64(value) => Value::Number(Number::from(*value)),
        ComponentValue::U64(value) => Value::Number(Number::from(*value)),
        ComponentValue::F64(value) => Number::from_f64(*value)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ComponentValue::String(value) => Value::String(value.clone()),
        ComponentValue::StringList(values) => Value::Array(
            values
                .iter()
                .map(|value| Value::String(value.clone()))
                .collect(),
        ),
        ComponentValue::Table(rows) => Value::Array(
            rows.iter()
                .map(|row| {
                    Value::Array(
                        row.iter()
                            .map(|value| Value::String(value.clone()))
                            .collect(),
                    )
                })
                .collect(),
        ),
        ComponentValue::Rect(rect) => Value::Object(
            [
                (
                    "x".to_string(),
                    Value::Number(Number::from(u64::from(rect.x))),
                ),
                (
                    "y".to_string(),
                    Value::Number(Number::from(u64::from(rect.y))),
                ),
                (
                    "width".to_string(),
                    Value::Number(Number::from(u64::from(rect.width))),
                ),
                (
                    "height".to_string(),
                    Value::Number(Number::from(u64::from(rect.height))),
                ),
            ]
            .into_iter()
            .collect::<Map<_, _>>(),
        ),
        ComponentValue::Bytes(bytes) => Value::Array(
            bytes
                .iter()
                .map(|byte| Value::Number(Number::from(u64::from(*byte))))
                .collect(),
        ),
        ComponentValue::List(values) => {
            Value::Array(values.iter().map(component_value_to_json).collect())
        }
        ComponentValue::Map(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), component_value_to_json(value)))
                .collect(),
        ),
    }
}

fn append_thinking_delta(store: &ChatMessageStore, message_id: ChatMessageId, delta: &str) -> bool {
    if let Some(block_id) = thinking_block_id(store, message_id) {
        return store.append_text_delta(block_id, delta);
    }
    if delta.is_empty() {
        return store
            .messages()
            .iter()
            .any(|message| message.id == message_id);
    }

    let block_id = store.next_block_id();
    let mut inserted = false;
    store.update_message(message_id, |message| {
        if message
            .blocks
            .iter()
            .any(|block| matches!(block, ChatBlock::Thinking(_)))
        {
            return;
        }
        let insert_at = message
            .blocks
            .iter()
            .position(|block| matches!(block, ChatBlock::Text(_)))
            .unwrap_or(message.blocks.len());
        message.blocks.insert(
            insert_at,
            ChatBlock::Thinking(ThinkingBlock {
                id: block_id,
                markdown: delta.to_string(),
                streaming: message.status.is_streaming(),
                collapsed: true,
            }),
        );
        inserted = true;
    });
    if inserted {
        true
    } else {
        thinking_block_id(store, message_id)
            .is_some_and(|block_id| store.append_text_delta(block_id, delta))
    }
}

fn thinking_block_id(store: &ChatMessageStore, message_id: ChatMessageId) -> Option<ChatBlockId> {
    store
        .messages()
        .iter()
        .find(|message| message.id == message_id)
        .and_then(|message| {
            message.blocks.iter().find_map(|block| match block {
                ChatBlock::Thinking(thinking) => Some(thinking.id),
                _ => None,
            })
        })
}

fn agent_menu(quit_events: EventQueue<()>) -> MenuBar {
    // Keep the initial app shell minimal while still offering a discoverable quit action.
    MenuBar::new(vec![MenuSpec::new(
        "File",
        vec![MenuItem::action("Quit", move || quit_events.push(())).shortcut("q")],
    )])
}

fn status_segments(
    model: Binding<String>,
    state: Binding<String>,
    plan_mode: Binding<String>,
    skills: Binding<String>,
) -> Vec<StatusSegment> {
    // Keep provider static until DeepSeek streaming lands while surfacing loaded model config.
    vec![
        StatusSegment::new("app", APP_TITLE)
            .priority(100)
            .min_width(10),
        StatusSegment::new("provider", "provider: mock")
            .priority(80)
            .min_width(14),
        StatusSegment::new("model", model)
            .priority(78)
            .min_width(18),
        StatusSegment::new("plan", plan_mode)
            .priority(75)
            .min_width(9),
        StatusSegment::new("skills", skills)
            .priority(74)
            .min_width(9),
        StatusSegment::new("state", state)
            .align(StatusSegmentAlign::Right)
            .priority(90)
            .min_width(9),
        StatusSegment::new("keys", "Esc cancel | Ctrl+Q quit | /help")
            .align(StatusSegmentAlign::Right)
            .priority(70)
            .min_width(28),
    ]
}

fn chat_window_rect(screen: Rect) -> Rect {
    // Fill the desktop work area with a small margin on normal terminal sizes.
    let work = Desktop::layout(screen).work_area;
    let margin_x = u16::from(work.width > 48);
    let margin_y = u16::from(work.height > 16);
    Rect {
        x: work.x.saturating_add(margin_x),
        y: work.y.saturating_add(margin_y),
        width: work.width.saturating_sub(margin_x.saturating_mul(2)),
        height: work.height.saturating_sub(margin_y.saturating_mul(2)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    use atto_ui::ComponentValue;
    use atto_ui::composable::{
        ComponentContext, EventHandling, MouseCoordinateSpace, ScrollbarHost, TabMode,
    };
    use atto_ui::theme::Theme;
    use atto_ui::wm::WindowId;
    use atto_ui_chat::{
        ApprovalAction, ApprovalDecision, ApprovalLevel, ChatBlock, ChatError, ChatErrorKind,
        ChatInputMode, ChatInputResponse, ChatMessage, ChatMessageStore, ChatRole,
        ChatSlashCommandAction, ChatTurnStatus, StopReason, TokenUsage, ToolInput, ToolOutput,
        ToolResultBlock, ToolStatus, ToolUseBlock,
    };
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::layout::Rect;

    use crate::config::{AgentConfig, PlanMode};
    use crate::deepseek::{
        ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionDelta,
        ChatCompletionSseEvent, ChatFunctionCallDelta, ChatMessageRole, ChatToolCallDelta,
        ChatToolKind, ToolChoice, ToolChoiceMode, chat_error_from_http_status,
        chat_error_from_json_error, chat_error_from_network_failure,
        chat_error_from_stream_disconnect, parse_chat_completion_sse,
        parse_chat_completion_sse_data,
    };
    use crate::skill::{LoadedSkillSet, SkillRegistry, SkillSearchPath};
    use crate::stream_ui::DeepSeekUiStream;
    use crate::tool::{
        ToolContext, ToolExecutor, ToolOutputKind, ToolPermission, ToolPermissionPolicy,
        ToolRegistry, ToolResult, ToolSpec,
    };

    use super::{
        APP_TITLE, AgentApp, AgentTurnLauncher, AgentTurnLimits, AppAction, MockTurnRegistry,
        STATUS_READY, STATUS_STREAMING, SlashRuntime, ToolRuntime, TurnBudgetTracker,
        apply_app_action, build_chat_panel, deepseek_request_from_transcript,
        execute_tool_use_to_result_block, handle_tool_approval, submit_input_response,
        submit_slash_command_text,
    };

    fn message_text(message: &ChatMessage) -> &str {
        match &message.blocks[0] {
            ChatBlock::Text(block) => &block.markdown,
            other => panic!("expected text block, got {other:?}"),
        }
    }

    fn new_test_stream() -> DeepSeekUiStream {
        let store = ChatMessageStore::new();
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let text_block_id = assistant.blocks[0].id();
        store.push(assistant);
        DeepSeekUiStream::new(
            store.branch_token(),
            assistant_id,
            text_block_id,
            "deepseek-chat",
        )
    }

    fn tool_call_delta(
        index: u32,
        id: Option<&str>,
        name: Option<&str>,
        arguments: Option<&str>,
    ) -> ChatToolCallDelta {
        ChatToolCallDelta {
            index,
            id: id.map(str::to_string),
            kind: id.map(|_| ChatToolKind::Function),
            function: Some(ChatFunctionCallDelta {
                name: name.map(str::to_string),
                arguments: arguments.map(str::to_string),
            }),
        }
    }

    fn single_failed_error(actions: Vec<AppAction>) -> ChatError {
        match actions.as_slice() {
            [AppAction::TurnFailed { error, .. }] => error.clone(),
            other => panic!("expected one failed action, got {other:?}"),
        }
    }

    struct TestSkillState {
        registry: SkillRegistry,
        loaded: LoadedSkillSet,
        count_state: atto_ui::reactive::Property<String>,
    }

    impl TestSkillState {
        fn new(registry: SkillRegistry) -> Self {
            let loaded = LoadedSkillSet::default();
            let count_state = atto_ui::reactive::Property::new(loaded.status());
            Self {
                registry,
                loaded,
                count_state,
            }
        }
    }

    impl Default for TestSkillState {
        fn default() -> Self {
            Self::new(SkillRegistry::default())
        }
    }

    fn test_slash_runtime(
        input_handle: &atto_ui_chat::ChatInputHandle,
        mock_turns: &MockTurnRegistry,
        status_state: &atto_ui::reactive::Property<String>,
        plan_mode_state: &atto_ui::reactive::Property<String>,
        turn_budgets: &TurnBudgetTracker,
    ) -> SlashRuntime {
        let skills = TestSkillState::default();
        test_slash_runtime_with_skills(
            input_handle,
            mock_turns,
            status_state,
            plan_mode_state,
            &skills,
            turn_budgets,
        )
    }

    fn test_slash_runtime_with_skills(
        input_handle: &atto_ui_chat::ChatInputHandle,
        mock_turns: &MockTurnRegistry,
        status_state: &atto_ui::reactive::Property<String>,
        plan_mode_state: &atto_ui::reactive::Property<String>,
        skills: &TestSkillState,
        turn_budgets: &TurnBudgetTracker,
    ) -> SlashRuntime {
        SlashRuntime {
            input_handle: input_handle.clone(),
            mock_turns: mock_turns.clone(),
            status_state: status_state.clone(),
            plan_mode_state: plan_mode_state.clone(),
            skill_registry: skills.registry.clone(),
            loaded_skills: skills.loaded.clone(),
            skill_count_state: skills.count_state.clone(),
            turn_budgets: turn_budgets.clone(),
        }
    }

    fn context(theme: &Theme) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        }
    }

    fn test_tool_registry() -> ToolRegistry {
        crate::tool::builtin_tool_registry().expect("built-in tool registry must be valid")
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "atto-agent-app-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn write_test_skill(workspace: &Path, dir_name: &str, name: &str, description: &str) {
        let dir = workspace.join(".atto/skills").join(dir_name);
        fs::create_dir_all(&dir).expect("test skill directory should be created");
        fs::write(
            dir.join("SKILL.md"),
            format!(
                r#"---
name: {name}
description: {description}
triggers: []
tools: []
mode: manual
---
Use this skill for {name} tasks.
"#
            ),
        )
        .expect("test skill file should be written");
    }

    fn test_skill_registry(skills: &[(&str, &str, &str)]) -> (PathBuf, SkillRegistry) {
        let workspace = unique_temp_dir("skills");
        for (dir_name, name, description) in skills {
            write_test_skill(&workspace, dir_name, name, description);
        }
        let registry =
            SkillRegistry::discover_from_paths(&[SkillSearchPath::workspace(&workspace)]);
        (workspace, registry)
    }

    fn test_tool_permissions() -> Arc<Mutex<ToolPermissionPolicy>> {
        Arc::new(Mutex::new(ToolPermissionPolicy::default()))
    }

    fn test_tool_runtime(
        config: AgentConfig,
        action_sender: std::sync::mpsc::Sender<AppAction>,
        registry: ToolRegistry,
        permissions: Arc<Mutex<ToolPermissionPolicy>>,
    ) -> ToolRuntime {
        test_tool_runtime_with_limits(
            config,
            action_sender,
            registry,
            permissions,
            AgentTurnLimits::default(),
        )
    }

    fn test_tool_runtime_with_limits(
        config: AgentConfig,
        action_sender: std::sync::mpsc::Sender<AppAction>,
        registry: ToolRegistry,
        permissions: Arc<Mutex<ToolPermissionPolicy>>,
        limits: AgentTurnLimits,
    ) -> ToolRuntime {
        ToolRuntime {
            config,
            action_sender,
            registry,
            permissions,
            turn_budgets: TurnBudgetTracker::default(),
            limits,
        }
    }

    fn apply_test_app_action(
        store: &ChatMessageStore,
        input_handle: &atto_ui_chat::ChatInputHandle,
        mock_turns: &MockTurnRegistry,
        status_state: &atto_ui::reactive::Property<String>,
        action: AppAction,
    ) -> bool {
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime =
            test_tool_runtime(AgentConfig::defaults("."), sender, registry, permissions);
        apply_app_action(
            store,
            input_handle,
            mock_turns,
            status_state,
            &tool_runtime,
            action,
        )
    }

    fn run_command_tool_call(call_id: &str) -> ToolUseBlock {
        ToolUseBlock {
            id: atto_ui_chat::ChatBlockId::new(0),
            call_id: call_id.to_string(),
            name: "run_command".to_string(),
            input: ToolInput::Json(ComponentValue::Map(BTreeMap::from([(
                "argv".to_string(),
                ComponentValue::List(vec![ComponentValue::String("cargo".to_string())]),
            )]))),
            status: ToolStatus::Pending,
            approval: None,
            collapsed: false,
        }
    }

    fn read_file_tool_call(call_id: &str, path: &str) -> ToolUseBlock {
        ToolUseBlock {
            id: atto_ui_chat::ChatBlockId::new(0),
            call_id: call_id.to_string(),
            name: "read_file".to_string(),
            input: ToolInput::Json(ComponentValue::Map(BTreeMap::from([(
                "path".to_string(),
                ComponentValue::String(path.to_string()),
            )]))),
            status: ToolStatus::Pending,
            approval: None,
            collapsed: false,
        }
    }

    fn unknown_tool_call(call_id: &str) -> ToolUseBlock {
        ToolUseBlock {
            id: atto_ui_chat::ChatBlockId::new(0),
            call_id: call_id.to_string(),
            name: "missing_tool".to_string(),
            input: ToolInput::Json(ComponentValue::Map(BTreeMap::new())),
            status: ToolStatus::Pending,
            approval: None,
            collapsed: false,
        }
    }

    #[derive(Clone, Copy)]
    struct SlowTool {
        delay: std::time::Duration,
    }

    impl ToolExecutor for SlowTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec::new(
                "slow_tool",
                "Sleep long enough to exercise app-level tool timeout handling.",
                serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                ToolPermission::AlwaysAllow,
                ToolOutputKind::Markdown,
            )
            .expect("slow test tool spec should be valid")
        }

        fn execute(
            &self,
            _ctx: ToolContext,
            _args: serde_json::Value,
        ) -> anyhow::Result<ToolResult> {
            std::thread::sleep(self.delay);
            Ok(ToolResult::success(
                "slow tool finished",
                ToolOutputKind::Markdown,
            ))
        }
    }

    fn slow_tool_call(call_id: &str) -> ToolUseBlock {
        ToolUseBlock {
            id: atto_ui_chat::ChatBlockId::new(0),
            call_id: call_id.to_string(),
            name: "slow_tool".to_string(),
            input: ToolInput::Json(ComponentValue::Map(BTreeMap::new())),
            status: ToolStatus::Pending,
            approval: None,
            collapsed: false,
        }
    }

    fn test_workspace(name: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "atto-agent-app-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create fixture workspace");
        path
    }

    fn append_tool_call_with_runtime(
        store: &ChatMessageStore,
        input_handle: &atto_ui_chat::ChatInputHandle,
        mock_turns: &MockTurnRegistry,
        status_state: &atto_ui::reactive::Property<String>,
        registry: &ToolRegistry,
        permissions: &Arc<Mutex<ToolPermissionPolicy>>,
        tool_call: ToolUseBlock,
    ) -> atto_ui_chat::ChatBlockId {
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime = test_tool_runtime(
            AgentConfig::defaults("."),
            sender,
            registry.clone(),
            permissions.clone(),
        );
        let expected_call_id = tool_call.call_id.clone();
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        store.push(assistant);
        let branch = store.branch_token();

        assert!(apply_app_action(
            store,
            input_handle,
            mock_turns,
            status_state,
            &tool_runtime,
            AppAction::ToolCallsReady {
                branch,
                message_id: assistant_id,
                tool_calls: vec![tool_call],
            },
        ));

        store
            .messages()
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                ChatBlock::ToolUse(tool) if tool.call_id == expected_call_id => Some(tool.id),
                _ => None,
            })
            .expect("tool use block should be appended")
    }

    fn tool_use_for_block(
        store: &ChatMessageStore,
        block_id: atto_ui_chat::ChatBlockId,
    ) -> ToolUseBlock {
        store
            .with_block(block_id, |block| match block {
                ChatBlock::ToolUse(tool) => Some(tool.clone()),
                other => panic!("expected tool use block, got {other:?}"),
            })
            .flatten()
            .expect("tool use block should exist")
    }

    fn tool_result_for_call(store: &ChatMessageStore, call_id: &str) -> ToolResultBlock {
        store
            .messages()
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                ChatBlock::ToolResult(result) if result.call_id == call_id => Some(result.clone()),
                _ => None,
            })
            .expect("tool result block should exist")
    }

    fn approval_decision(
        store: &ChatMessageStore,
        block_id: atto_ui_chat::ChatBlockId,
        approval_id: &str,
        option_id: &str,
        action: ApprovalAction,
        level: ApprovalLevel,
    ) -> ApprovalDecision {
        ApprovalDecision {
            message_id: store.messages()[0].id,
            block_id,
            approval_id: approval_id.to_string(),
            option_id: option_id.to_string(),
            action,
            level,
        }
    }

    #[test]
    fn builds_single_chat_window_with_status_bar() {
        let app = AgentApp::new(Rect::new(0, 0, 80, 24));

        let windows = app.desktop().list_windows();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].title, APP_TITLE);
        assert_eq!(windows[0].tag.as_deref(), Some("atto-agent:chat"));
        assert!(windows[0].is_focused);
        assert!(app.desktop().status.has_segments());
        assert_eq!(app.chat_window_id(), windows[0].id);
    }

    #[test]
    fn initializes_chat_store_and_input_handle() {
        let app = AgentApp::new(Rect::new(0, 0, 80, 24));

        assert!(app.message_store().messages().is_empty());
        assert_eq!(app.status_state().get(), STATUS_READY);
        assert_eq!(app.model_state().get(), "model: deepseek-chat");
        assert_eq!(app.plan_mode_state().get(), PlanMode::Auto.status());
        assert_eq!(app.skill_count_state().get(), "skills: 0");
        assert!(app.loaded_skills().is_empty());
        match app.input_handle().mode() {
            ChatInputMode::Text(config) => {
                assert_eq!(config.title, "Message");
                assert_eq!(config.placeholder.as_deref(), Some("Type a message..."));
            }
            other => panic!("expected text input mode, got {other:?}"),
        }
    }

    #[test]
    fn applies_configured_model_and_plan_mode_to_runtime_state() {
        let mut config = AgentConfig::defaults(".");
        config.model = "deepseek-reasoner".to_string();
        config.plan_mode = PlanMode::On;

        let app = AgentApp::with_config(Rect::new(0, 0, 80, 24), config);

        assert_eq!(app.config().model, "deepseek-reasoner");
        assert_eq!(app.model_state().get(), "model: deepseek-reasoner");
        assert_eq!(app.plan_mode_state().get(), PlanMode::On.status());
    }

    #[test]
    fn injects_submit_slash_commands() {
        let app = AgentApp::new(Rect::new(0, 0, 80, 24));
        let commands = app.input_handle().slash_commands();
        let labels = commands
            .iter()
            .map(|command| command.label.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "/help", "/clear", "/plan", "/skills", "/skill", "/tools", "/abort"
            ]
        );
        assert!(
            commands
                .iter()
                .all(|command| command.action == ChatSlashCommandAction::Submit)
        );
    }

    #[test]
    fn help_slash_command_outputs_available_commands_without_starting_turn() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let turn_budgets = TurnBudgetTracker::default();

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/help",
        ));

        let messages = store.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, ChatRole::System);
        assert!(message_text(&messages[0]).contains("/clear"));
        assert!(message_text(&messages[0]).contains("/skill <name>"));
        assert!(message_text(&messages[0]).contains("/abort"));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }

    #[test]
    fn clear_slash_command_removes_messages_and_resets_runtime_state() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::On.status());
        let turn_budgets = TurnBudgetTracker::default();
        input_handle.streaming_binding().set(true);
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "seed",
        ));
        let assistant_id = store.next_message_id();
        store.push(
            ChatMessage::text(assistant_id, ChatRole::Assistant, "partial")
                .with_status(ChatTurnStatus::Streaming),
        );
        let cancel = mock_turns.start(assistant_id);

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/clear",
        ));

        assert!(store.messages().is_empty());
        assert!(cancel.is_cancelled());
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert_eq!(plan_mode_state.get(), PlanMode::On.status());
    }

    #[test]
    fn plan_slash_command_sets_and_cycles_plan_mode() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let turn_budgets = TurnBudgetTracker::default();

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/plan on",
        ));
        assert_eq!(plan_mode_state.get(), PlanMode::On.status());

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/plan auto",
        ));
        assert_eq!(plan_mode_state.get(), PlanMode::Auto.status());

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/plan",
        ));
        assert_eq!(plan_mode_state.get(), PlanMode::Off.status());

        let messages = store.messages();
        assert!(message_text(&messages[0]).contains("Plan mode set to on."));
        assert!(message_text(&messages[1]).contains("Plan mode set to auto."));
        assert!(message_text(&messages[2]).contains("Plan mode set to off."));
    }

    #[test]
    fn skills_and_tools_slash_commands_report_current_registries() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let turn_budgets = TurnBudgetTracker::default();

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/skills",
        ));
        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/tools",
        ));

        let messages = store.messages();
        assert!(message_text(&messages[0]).contains("Skills: 0 discovered, 0 loaded"));
        assert!(message_text(&messages[0]).contains("No skills found"));
        assert!(message_text(&messages[0]).contains("No skills loaded"));
        assert!(message_text(&messages[1]).contains("Tools: 5 registered"));
        assert!(message_text(&messages[1]).contains("apply_patch"));
        assert!(message_text(&messages[1]).contains("read_file"));
        assert!(message_text(&messages[1]).contains("list_files"));
        assert!(message_text(&messages[1]).contains("run_command"));
        assert!(message_text(&messages[1]).contains("search_text"));
    }

    #[test]
    fn skill_slash_command_activates_skill_and_updates_listing() {
        let (workspace, registry) = test_skill_registry(&[
            ("rust", "rust-review", "Review Rust code."),
            ("docs", "docs", "Write documentation."),
        ]);
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let skills = TestSkillState::new(registry);
        let turn_budgets = TurnBudgetTracker::default();
        let runtime = test_slash_runtime_with_skills(
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            &skills,
            &turn_budgets,
        );

        assert!(submit_slash_command_text(
            &store,
            &runtime,
            "/skill rust-review"
        ));
        assert!(skills.loaded.contains("rust-review"));
        assert_eq!(skills.loaded.names(), vec!["rust-review"]);
        assert_eq!(skills.count_state.get(), "skills: 1");

        assert!(submit_slash_command_text(
            &store,
            &runtime,
            "/skill rust-review"
        ));
        assert_eq!(skills.loaded.names(), vec!["rust-review"]);
        assert_eq!(skills.count_state.get(), "skills: 1");

        assert!(submit_slash_command_text(&store, &runtime, "/skills"));

        let messages = store.messages();
        assert!(message_text(&messages[0]).contains("Loaded skill `rust-review`"));
        assert!(message_text(&messages[1]).contains("already active"));
        assert!(message_text(&messages[2]).contains("Skills: 2 discovered, 1 loaded"));
        assert!(message_text(&messages[2]).contains("- [available] docs"));
        assert!(message_text(&messages[2]).contains("- [loaded] rust-review"));
        assert!(message_text(&messages[2]).contains("Loaded skills: rust-review."));

        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn skill_slash_command_reports_usage_and_unknown_skill() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let skills = TestSkillState::default();
        let turn_budgets = TurnBudgetTracker::default();
        let runtime = test_slash_runtime_with_skills(
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            &skills,
            &turn_budgets,
        );

        assert!(submit_slash_command_text(&store, &runtime, "/skill"));
        assert!(submit_slash_command_text(
            &store,
            &runtime,
            "/skill missing"
        ));

        let messages = store.messages();
        assert!(message_text(&messages[0]).contains("Usage: /skill <name>"));
        assert!(message_text(&messages[1]).contains("Skill `missing` not found"));
        assert!(skills.loaded.is_empty());
        assert_eq!(skills.count_state.get(), "skills: 0");
    }

    #[test]
    fn abort_slash_command_cancels_latest_streaming_turn_and_rejects_late_tokens() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let turn_budgets = TurnBudgetTracker::default();
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let block_id = assistant.blocks[0].id();
        store.push(assistant);
        let stale_branch = store.branch_token();
        let cancel = mock_turns.start(assistant_id);

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/abort",
        ));

        let messages = store.messages();
        assert_eq!(messages[0].status, ChatTurnStatus::Canceled);
        assert_eq!(messages[1].role, ChatRole::System);
        assert!(message_text(&messages[1]).contains("Aborted active turn."));
        assert!(cancel.is_cancelled());
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert!(!apply_test_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            AppAction::TextDelta {
                branch: stale_branch,
                block_id,
                delta: "late".to_string(),
            },
        ));
        assert_eq!(message_text(&store.messages()[0]), "");
    }

    #[test]
    fn esc_cancel_through_chat_panel_cancels_mock_turn_and_rejects_late_tokens() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let block_id = assistant.blocks[0].id();
        store.push(assistant);
        let stale_branch = store.branch_token();
        let cancel = mock_turns.start(assistant_id);
        let mut panel = build_chat_panel(
            &store,
            AgentTurnLauncher {
                model: "deepseek-chat".to_string(),
                action_sender: sender.clone(),
                turn_budgets: turn_budgets.clone(),
                limits: AgentTurnLimits::default(),
            },
            test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            test_tool_runtime(
                AgentConfig::defaults("."),
                sender,
                test_tool_registry(),
                test_tool_permissions(),
            ),
        );
        let theme = Theme::dark();

        let result = panel.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            context(&theme),
        );

        assert!(result.is_consumed());
        assert!(cancel.is_cancelled());
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert_eq!(store.messages()[0].status, ChatTurnStatus::Canceled);
        assert!(!apply_test_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            AppAction::TextDelta {
                branch: stale_branch,
                block_id,
                delta: "late".to_string(),
            },
        ));
        assert_eq!(message_text(&store.messages()[0]), "");
    }

    #[test]
    fn text_submit_adds_user_and_streaming_assistant_turn() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let turn_launcher = AgentTurnLauncher {
            model: "deepseek-chat".to_string(),
            action_sender: sender,
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
        };

        submit_input_response(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            &turn_launcher,
            ChatInputResponse::Text("hello".to_string()),
        );

        let messages = store.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(message_text(&messages[0]), "hello");
        assert_eq!(messages[1].role, ChatRole::Assistant);
        assert_eq!(message_text(&messages[1]), "");
        assert_eq!(messages[1].status, ChatTurnStatus::Streaming);
        assert!(input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_STREAMING);
        drop(receiver);
    }

    #[test]
    fn app_actions_append_streaming_text_and_complete_turn() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let block_id = assistant.blocks[0].id();
        store.push(assistant);
        let branch = store.branch_token();
        let cancel = mock_turns.start(assistant_id);

        assert!(apply_test_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            AppAction::TextDelta {
                branch,
                block_id,
                delta: "Mock ".to_string(),
            },
        ));
        assert!(apply_test_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            AppAction::TextDelta {
                branch,
                block_id,
                delta: "done".to_string(),
            },
        ));
        assert!(apply_test_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            AppAction::TurnDone {
                branch,
                message_id: assistant_id,
                meta: None,
            },
        ));

        let messages = store.messages();
        assert_eq!(message_text(&messages[0]), "Mock done");
        assert_eq!(messages[0].status, ChatTurnStatus::Complete);
        assert!(!cancel.is_cancelled());
        assert!(!mock_turns.cancel(assistant_id));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }

    #[test]
    fn turn_budget_limits_model_requests() {
        let budgets = TurnBudgetTracker::default();
        let limits = AgentTurnLimits::new(2, 16, std::time::Duration::from_secs(30));
        let message_id = atto_ui_chat::ChatMessageId::new(42);
        budgets.start_turn(message_id, limits);

        assert!(budgets.consume_model_request(message_id, limits).is_ok());
        assert!(budgets.consume_model_request(message_id, limits).is_ok());
        let error = budgets
            .consume_model_request(message_id, limits)
            .expect_err("third model request should exceed the per-turn limit");

        assert_eq!(error.kind, ChatErrorKind::Other);
        assert!(error.message.contains("model request limit"));
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("per-turn limit is 2"))
        );
    }

    #[test]
    fn tool_call_budget_fails_turn_before_appending_over_limit() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        input_handle.streaming_binding().set(true);
        let limits = AgentTurnLimits::new(8, 1, std::time::Duration::from_secs(30));
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime = test_tool_runtime_with_limits(
            AgentConfig::defaults("."),
            sender,
            test_tool_registry(),
            test_tool_permissions(),
            limits,
        );
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        store.push(assistant);
        let branch = store.branch_token();
        let _cancel = mock_turns.start(assistant_id);

        assert!(apply_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &tool_runtime,
            AppAction::ToolCallsReady {
                branch,
                message_id: assistant_id,
                tool_calls: vec![
                    read_file_tool_call("call_1", "a.txt"),
                    read_file_tool_call("call_2", "b.txt"),
                ],
            },
        ));

        let messages = store.messages();
        let ChatTurnStatus::Failed(error) = &messages[0].status else {
            panic!("expected failed turn, got {:?}", messages[0].status);
        };
        assert_eq!(error.kind, ChatErrorKind::Tool);
        assert!(error.message.contains("tool call limit"));
        assert_eq!(messages[0].blocks.len(), 1);
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert!(!mock_turns.cancel(assistant_id));
    }

    #[test]
    fn tool_execution_timeout_writes_failed_result() {
        let mut registry = ToolRegistry::new();
        registry
            .register(SlowTool {
                delay: std::time::Duration::from_millis(80),
            })
            .expect("register slow tool");
        let limits = AgentTurnLimits::new(8, 16, std::time::Duration::from_millis(10));

        let result = execute_tool_use_to_result_block(
            &registry,
            &AgentConfig::defaults("."),
            &slow_tool_call("call_slow"),
            limits,
        );

        assert!(!result.ok);
        assert_eq!(result.call_id, "call_slow");
        match result.output {
            ToolOutput::Markdown(output) => {
                assert!(output.contains("Tool `slow_tool` timed out after 10ms"));
            }
            other => panic!("expected markdown timeout result, got {other:?}"),
        }
    }

    #[test]
    fn tool_calls_requiring_project_approval_render_approval_options() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();

        let block_id = append_tool_call_with_runtime(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &registry,
            &permissions,
            run_command_tool_call("call_run"),
        );

        let tool = tool_use_for_block(&store, block_id);
        assert_eq!(tool.status, ToolStatus::Pending);
        let approval = tool.approval.expect("run_command should require approval");
        let options = approval
            .options
            .iter()
            .map(|option| {
                (
                    option.id.as_str(),
                    option.label.as_str(),
                    option.action,
                    option.level,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(approval.id, "approval:call_run");
        assert!(approval.prompt.contains("run_command"));
        assert_eq!(
            options,
            vec![
                (
                    "allow_once",
                    "Allow once",
                    ApprovalAction::Allow,
                    ApprovalLevel::Once
                ),
                (
                    "allow_project",
                    "Allow project",
                    ApprovalAction::Allow,
                    ApprovalLevel::Project
                ),
                ("deny", "Deny", ApprovalAction::Deny, ApprovalLevel::Once),
            ]
        );
    }

    #[test]
    fn approval_allow_once_resolves_tool_without_project_grant() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime = test_tool_runtime(
            AgentConfig::defaults("."),
            sender,
            registry.clone(),
            permissions.clone(),
        );
        let block_id = append_tool_call_with_runtime(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &registry,
            &permissions,
            run_command_tool_call("call_once"),
        );
        let approval_id = tool_use_for_block(&store, block_id)
            .approval
            .expect("approval should exist")
            .id;

        handle_tool_approval(
            &store,
            &tool_runtime,
            approval_decision(
                &store,
                block_id,
                &approval_id,
                "allow_once",
                ApprovalAction::Allow,
                ApprovalLevel::Once,
            ),
        );

        let tool = tool_use_for_block(&store, block_id);
        assert_eq!(tool.status, ToolStatus::Running);
        assert_eq!(
            tool.approval.and_then(|approval| approval.resolved),
            Some(atto_ui_chat::ApprovalResolution {
                option_id: "allow_once".to_string(),
                action: ApprovalAction::Allow,
                level: ApprovalLevel::Once,
            })
        );
        assert!(
            !permissions
                .lock()
                .expect("tool permission policy lock poisoned")
                .is_project_allowed("run_command")
        );
    }

    #[test]
    fn approval_allow_project_records_grant_and_skips_future_approval() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime = test_tool_runtime(
            AgentConfig::defaults("."),
            sender,
            registry.clone(),
            permissions.clone(),
        );
        let first_block = append_tool_call_with_runtime(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &registry,
            &permissions,
            run_command_tool_call("call_project_1"),
        );
        let approval_id = tool_use_for_block(&store, first_block)
            .approval
            .expect("approval should exist")
            .id;

        handle_tool_approval(
            &store,
            &tool_runtime,
            approval_decision(
                &store,
                first_block,
                &approval_id,
                "allow_project",
                ApprovalAction::Allow,
                ApprovalLevel::Project,
            ),
        );

        assert!(
            permissions
                .lock()
                .expect("tool permission policy lock poisoned")
                .is_project_allowed("run_command")
        );
        let second_block = append_tool_call_with_runtime(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &registry,
            &permissions,
            run_command_tool_call("call_project_2"),
        );
        let second_tool = tool_use_for_block(&store, second_block);
        assert_eq!(second_tool.status, ToolStatus::Running);
        assert!(second_tool.approval.is_none());
    }

    #[test]
    fn approval_deny_cancels_tool_and_writes_failed_result() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime = test_tool_runtime(
            AgentConfig::defaults("."),
            sender,
            registry.clone(),
            permissions.clone(),
        );
        let block_id = append_tool_call_with_runtime(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &registry,
            &permissions,
            run_command_tool_call("call_deny"),
        );
        let approval_id = tool_use_for_block(&store, block_id)
            .approval
            .expect("approval should exist")
            .id;

        handle_tool_approval(
            &store,
            &tool_runtime,
            approval_decision(
                &store,
                block_id,
                &approval_id,
                "deny",
                ApprovalAction::Deny,
                ApprovalLevel::Once,
            ),
        );

        let tool = tool_use_for_block(&store, block_id);
        let result = tool_result_for_call(&store, "call_deny");
        assert_eq!(tool.status, ToolStatus::Canceled);
        assert_eq!(
            tool.approval.and_then(|approval| approval.resolved),
            Some(atto_ui_chat::ApprovalResolution {
                option_id: "deny".to_string(),
                action: ApprovalAction::Deny,
                level: ApprovalLevel::Once,
            })
        );
        assert!(!result.ok);
        assert_eq!(result.exit_code, None);
        match result.output {
            ToolOutput::Markdown(output) => {
                assert!(output.contains("User denied tool call `run_command`"));
            }
            other => panic!("expected markdown denial result, got {other:?}"),
        }
    }

    #[test]
    fn allowed_tool_execution_writes_tool_result_block() {
        let workspace = test_workspace("allowed-tool-result");
        fs::write(workspace.join("fixture.txt"), "tool output\n").expect("write fixture file");
        let mut config = AgentConfig::defaults(workspace.clone());
        config.workspace = workspace.clone();
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime = test_tool_runtime(
            config.clone(),
            sender,
            registry.clone(),
            permissions.clone(),
        );
        let assistant_id = store.next_message_id();
        store.push(
            ChatMessage::text(assistant_id, ChatRole::Assistant, "")
                .with_status(ChatTurnStatus::Streaming),
        );
        let branch = store.branch_token();

        assert!(apply_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &tool_runtime,
            AppAction::ToolCallsReady {
                branch,
                message_id: assistant_id,
                tool_calls: vec![read_file_tool_call("call_read", "fixture.txt")],
            },
        ));
        let tool_block_id = store
            .messages()
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                ChatBlock::ToolUse(tool) if tool.call_id == "call_read" => Some(tool.id),
                _ => None,
            })
            .expect("tool use should be appended");
        assert_eq!(
            tool_use_for_block(&store, tool_block_id).status,
            ToolStatus::Running
        );

        let action = receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("tool execution should send result action");
        assert!(apply_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &tool_runtime,
            action,
        ));

        let tool = tool_use_for_block(&store, tool_block_id);
        let result = tool_result_for_call(&store, "call_read");
        assert_eq!(tool.status, ToolStatus::Done);
        assert!(result.ok);
        assert_eq!(result.exit_code, None);
        match result.output {
            ToolOutput::Markdown(output) => {
                assert!(output.contains("Path: `fixture.txt`"));
                assert!(output.contains("tool output"));
            }
            other => panic!("expected markdown tool result, got {other:?}"),
        }

        fs::remove_dir_all(&workspace).expect("remove fixture workspace");
    }

    #[test]
    fn unknown_tool_call_writes_failed_tool_result_without_execution() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();

        let block_id = append_tool_call_with_runtime(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &registry,
            &permissions,
            unknown_tool_call("call_missing"),
        );

        let tool = tool_use_for_block(&store, block_id);
        let result = tool_result_for_call(&store, "call_missing");
        assert_eq!(tool.status, ToolStatus::Error);
        assert!(tool.approval.is_none());
        assert!(!result.ok);
        match result.output {
            ToolOutput::Markdown(output) => {
                assert!(output.contains("Tool `missing_tool` is not registered."));
            }
            other => panic!("expected markdown missing-tool result, got {other:?}"),
        }
    }

    #[test]
    fn deepseek_request_from_transcript_includes_tool_result_role_message() {
        let registry = test_tool_registry();
        let request = deepseek_request_from_transcript(
            &AgentConfig::defaults("."),
            &registry,
            &[
                ChatMessage::text(1, ChatRole::User, "Read the fixture."),
                ChatMessage::new(
                    2,
                    ChatRole::Assistant,
                    vec![
                        ChatBlock::ToolUse(read_file_tool_call("call_read", "fixture.txt")),
                        ChatBlock::ToolResult(ToolResultBlock {
                            id: atto_ui_chat::ChatBlockId::new(22),
                            call_id: "call_read".to_string(),
                            ok: true,
                            exit_code: None,
                            output: ToolOutput::Markdown("Path: `fixture.txt`\n\nbody".to_string()),
                            collapsed: false,
                        }),
                    ],
                ),
            ],
        );

        assert_eq!(request.messages.len(), 3);
        assert_eq!(request.messages[0].role, ChatMessageRole::User);
        assert_eq!(
            request.messages[0].content.as_deref(),
            Some("Read the fixture.")
        );
        assert_eq!(request.messages[1].role, ChatMessageRole::Assistant);
        assert_eq!(request.messages[1].tool_calls.len(), 1);
        let tool_call = &request.messages[1].tool_calls[0];
        assert_eq!(tool_call.id, "call_read");
        assert_eq!(tool_call.function.name, "read_file");
        assert_eq!(tool_call.function.arguments, r#"{"path":"fixture.txt"}"#);
        assert_eq!(request.messages[2].role, ChatMessageRole::Tool);
        assert_eq!(
            request.messages[2].tool_call_id.as_deref(),
            Some("call_read")
        );
        assert!(
            request.messages[2]
                .content
                .as_deref()
                .is_some_and(|content| content.contains("ok: true") && content.contains("body"))
        );
        assert_eq!(request.tools.len(), registry.len());
        assert_eq!(
            request.tool_choice,
            Some(ToolChoice::Mode(ToolChoiceMode::Auto))
        );
    }

    #[test]
    fn deepseek_error_mapping_covers_http_network_disconnect_and_json_failures() {
        let mut stream = new_test_stream();
        let error = single_failed_error(stream.map_error(chat_error_from_http_status(
            401,
            r#"{"error":{"message":"bad api key","type":"invalid_request_error","code":"invalid_api_key","param":null}}"#,
        )));
        assert_eq!(error.kind, ChatErrorKind::Api);
        assert!(error.message.contains("DEEPSEEK_API_KEY"));
        let detail = error.detail.as_deref().expect("detail should be present");
        assert!(detail.contains("HTTP status: 401"));
        assert!(detail.contains("invalid_api_key"));

        let mut stream = new_test_stream();
        let error = single_failed_error(
            stream.map_error(chat_error_from_http_status(429, "rate limit body")),
        );
        assert_eq!(error.kind, ChatErrorKind::RateLimit);
        assert!(error.message.contains("429"));
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("rate limit body"))
        );

        let mut stream = new_test_stream();
        let error =
            single_failed_error(stream.map_error(chat_error_from_http_status(502, "gateway down")));
        assert_eq!(error.kind, ChatErrorKind::Api);
        assert!(error.message.contains("502"));
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("gateway down"))
        );

        let mut stream = new_test_stream();
        let error = single_failed_error(
            stream.map_error(chat_error_from_network_failure("request timed out")),
        );
        assert_eq!(error.kind, ChatErrorKind::Network);
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("request timed out"))
        );

        let mut stream = new_test_stream();
        let error = single_failed_error(stream.map_error(chat_error_from_stream_disconnect()));
        assert_eq!(error.kind, ChatErrorKind::Network);
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("[DONE]"))
        );

        let json_error = parse_chat_completion_sse_data("{not json").unwrap_err();
        let mut stream = new_test_stream();
        let error = single_failed_error(
            stream.map_error(chat_error_from_json_error(json_error, "{not json")),
        );
        assert_eq!(error.kind, ChatErrorKind::Api);
        assert!(error.message.contains("parse DeepSeek stream JSON"));
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("{not json"))
        );
    }

    #[test]
    fn deepseek_stream_error_event_fails_turn_with_structured_detail() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let text_block_id = assistant.blocks[0].id();
        store.push(assistant);
        let branch = store.branch_token();
        let _cancel = mock_turns.start(assistant_id);
        let mut stream =
            DeepSeekUiStream::new(branch, assistant_id, text_block_id, "deepseek-chat");
        let events = parse_chat_completion_sse(
            "data: {\"error\":{\"message\":\"bad api key\",\"type\":\"invalid_request_error\",\"code\":\"invalid_api_key\",\"param\":null}}\n\n",
        )
        .unwrap();

        for event in events {
            for action in stream.map_event(event) {
                assert!(apply_test_app_action(
                    &store,
                    &input_handle,
                    &mock_turns,
                    &status_state,
                    action,
                ));
            }
        }

        let messages = store.messages();
        let ChatTurnStatus::Failed(error) = &messages[0].status else {
            panic!("expected failed turn, got {:?}", messages[0].status);
        };
        assert_eq!(error.kind, ChatErrorKind::Api);
        assert!(error.message.contains("DEEPSEEK_API_KEY"));
        let detail = error.detail.as_deref().expect("detail should be present");
        assert!(detail.contains("bad api key"));
        assert!(detail.contains("invalid_request_error"));
        assert!(detail.contains("invalid_api_key"));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert!(!mock_turns.cancel(assistant_id));
        assert!(!apply_test_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            AppAction::TextDelta {
                branch,
                block_id: text_block_id,
                delta: "late".to_string(),
            },
        ));
        assert_eq!(message_text(&store.messages()[0]), "");
    }

    #[test]
    fn deepseek_stream_events_aggregate_tool_calls_by_index() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let text_block_id = assistant.blocks[0].id();
        store.push(assistant);
        let branch = store.branch_token();
        let _cancel = mock_turns.start(assistant_id);
        let mut stream =
            DeepSeekUiStream::new(branch, assistant_id, text_block_id, "deepseek-chat");
        let events = vec![
            ChatCompletionSseEvent::Chunk(ChatCompletionChunk {
                id: None,
                object: None,
                created: None,
                model: Some("deepseek-chat".to_string()),
                choices: vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: ChatCompletionDelta {
                        tool_calls: vec![
                            tool_call_delta(
                                1,
                                Some("call_2"),
                                Some("search_text"),
                                Some(r#"{"query":"hel"#),
                            ),
                            tool_call_delta(0, Some("call_1"), Some("read_"), Some(r#"{"path":"#)),
                        ],
                        ..ChatCompletionDelta::default()
                    },
                    finish_reason: None,
                }],
                usage: None,
            }),
            ChatCompletionSseEvent::Chunk(ChatCompletionChunk {
                id: None,
                object: None,
                created: None,
                model: None,
                choices: vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: ChatCompletionDelta {
                        tool_calls: vec![
                            tool_call_delta(0, None, Some("file"), Some(r#""src/lib.rs"}"#)),
                            tool_call_delta(1, None, None, Some(r#"lo"}"#)),
                        ],
                        ..ChatCompletionDelta::default()
                    },
                    finish_reason: Some(crate::deepseek::FinishReason::ToolCalls),
                }],
                usage: None,
            }),
            ChatCompletionSseEvent::Done,
        ];

        for event in events {
            for action in stream.map_event(event) {
                assert!(apply_test_app_action(
                    &store,
                    &input_handle,
                    &mock_turns,
                    &status_state,
                    action,
                ));
            }
        }

        let messages = store.messages();
        let assistant = &messages[0];
        assert_eq!(assistant.status, ChatTurnStatus::Complete);
        assert_eq!(assistant.meta.stop_reason, Some(StopReason::ToolUse));
        assert_eq!(assistant.blocks.len(), 3);
        assert!(
            matches!(&assistant.blocks[0], ChatBlock::Text(block) if block.markdown.is_empty() && !block.streaming)
        );
        match &assistant.blocks[1] {
            ChatBlock::ToolUse(block) => {
                assert_eq!(block.call_id, "call_1");
                assert_eq!(block.name, "read_file");
                assert_eq!(block.status, ToolStatus::Running);
                assert!(block.approval.is_none());
                match &block.input {
                    ToolInput::Json(ComponentValue::Map(input)) => assert_eq!(
                        input.get("path"),
                        Some(&ComponentValue::String("src/lib.rs".to_string()))
                    ),
                    other => panic!("expected JSON object tool input, got {other:?}"),
                }
            }
            other => panic!("expected first tool call block, got {other:?}"),
        }
        match &assistant.blocks[2] {
            ChatBlock::ToolUse(block) => {
                assert_eq!(block.call_id, "call_2");
                assert_eq!(block.name, "search_text");
                assert_eq!(block.status, ToolStatus::Running);
                match &block.input {
                    ToolInput::Json(ComponentValue::Map(input)) => assert_eq!(
                        input.get("query"),
                        Some(&ComponentValue::String("hello".to_string()))
                    ),
                    other => panic!("expected JSON object tool input, got {other:?}"),
                }
            }
            other => panic!("expected second tool call block, got {other:?}"),
        }
        assert!(!mock_turns.cancel(assistant_id));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }

    #[test]
    fn deepseek_stream_tool_call_invalid_arguments_fails_turn() {
        let mut stream = new_test_stream();
        let actions = stream.map_event(ChatCompletionSseEvent::Chunk(ChatCompletionChunk {
            id: None,
            object: None,
            created: None,
            model: None,
            choices: vec![ChatCompletionChunkChoice {
                index: 0,
                delta: ChatCompletionDelta {
                    tool_calls: vec![tool_call_delta(
                        0,
                        Some("call_1"),
                        Some("read_file"),
                        Some("{not json"),
                    )],
                    ..ChatCompletionDelta::default()
                },
                finish_reason: Some(crate::deepseek::FinishReason::ToolCalls),
            }],
            usage: None,
        }));

        let error = single_failed_error(actions);

        assert_eq!(error.kind, ChatErrorKind::Tool);
        assert!(error.message.contains("invalid tool call arguments"));
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("{not json"))
        );
        assert!(stream.map_event(ChatCompletionSseEvent::Done).is_empty());
    }

    #[test]
    fn deepseek_stream_events_map_reasoning_content_and_completion_meta() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let text_block_id = assistant.blocks[0].id();
        store.push(assistant);
        let branch = store.branch_token();
        let _cancel = mock_turns.start(assistant_id);
        let mut stream =
            DeepSeekUiStream::new(branch, assistant_id, text_block_id, "deepseek-chat");
        let events = parse_chat_completion_sse(concat!(
            "data: {\"model\":\"deepseek-reasoner\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"think \"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"more\",\"content\":\"hel\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":5,\"total_tokens\":12}}\n\n",
            "data: [DONE]\n\n",
        ))
        .unwrap();

        for event in events {
            for action in stream.map_event(event) {
                assert!(apply_test_app_action(
                    &store,
                    &input_handle,
                    &mock_turns,
                    &status_state,
                    action,
                ));
            }
        }

        let messages = store.messages();
        let assistant = &messages[0];
        assert_eq!(assistant.status, ChatTurnStatus::Complete);
        assert_eq!(assistant.meta.model.as_deref(), Some("deepseek-reasoner"));
        assert_eq!(
            assistant.meta.usage,
            Some(TokenUsage {
                input: 7,
                output: 5,
            })
        );
        assert_eq!(assistant.meta.stop_reason, Some(StopReason::EndTurn));
        assert_eq!(assistant.blocks.len(), 2);
        match &assistant.blocks[0] {
            ChatBlock::Thinking(block) => {
                assert_eq!(block.markdown, "think more");
                assert!(block.collapsed);
                assert!(!block.streaming);
            }
            other => panic!("expected thinking block, got {other:?}"),
        }
        match &assistant.blocks[1] {
            ChatBlock::Text(block) => {
                assert_eq!(block.markdown, "hello");
                assert!(!block.streaming);
            }
            other => panic!("expected text block, got {other:?}"),
        }
        assert!(!mock_turns.cancel(assistant_id));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }
}
