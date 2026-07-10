#![forbid(unsafe_code)]

//! Application crate for the Atto TUI agent.
//!
//! The crate is intentionally thin at this stage: later milestones will compose
//! `atto-ui`, `atto-ui-chat`, and `atto-ui-async` here without adding network
//! dependencies to the reusable UI crates.

use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use atto_ui::CancellationToken;
use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, MenuItem, MenuSpec,
    StatusSegment, StatusSegmentAlign, run_crossterm_desktop_with_actions,
};
use atto_ui::reactive::{Binding, DirtyObserver, EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowId, WindowKind};
use atto_ui_chat::{
    ApprovalAction, ApprovalDecision, ApprovalLevel, ApprovalOption, ApprovalRequest, ChatBlock,
    ChatBlockId, ChatBranchToken, ChatError, ChatErrorKind, ChatInputHandle, ChatInputResponse,
    ChatMessage, ChatMessageId, ChatMessageList, ChatMessageMeta, ChatMessageStore, ChatPanel,
    ChatRole, ChatSlashCommand, ChatTurnStatus, DiffData, EditAndResubmitEvent, MessageAction,
    MessageActionKind, PlanBlock, PlanDecision, PlanDecisionEvent, PlanItem, TextBlock,
    ThinkingBlock, ToolInput, ToolOutput, ToolResultBlock, ToolStatus, ToolUseBlock,
};
use futures_util::future::{AbortHandle, AbortRegistration, Abortable};
use ratatui::layout::Rect;
use serde_json::Value;

mod compact;
pub mod config;
pub mod context;
pub mod deepseek;
pub mod deepseek_client;
mod limits;
pub mod plan;
pub mod skill;
mod stream_ui;
pub mod tool;
pub mod transcript;

use crate::compact::{CompactPolicy, compact_store_if_needed, estimate_transcript_tokens};
use crate::config::{AgentConfig, AgentProvider, PlanMode};
use crate::context::{ContextBuilder, component_value_to_json};
use crate::deepseek::{
    ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionDelta, ChatCompletionMessage,
    ChatCompletionRequest, ChatCompletionSseEvent, ChatFunctionCallDelta, ChatToolCallDelta,
    ChatToolKind, FinishReason, ToolChoice, ToolChoiceMode,
};
use crate::deepseek_client::DeepSeekClient;
use crate::limits::{AgentTurnLimits, TurnBudgetTracker};
use crate::plan::{
    PLAN_MODE_SYSTEM_PROMPT, PlanTurnDecision, decide_plan_for_turn, submit_plan_chat_tool,
    submit_plan_tool_choice,
};
use crate::skill::{DEFAULT_MAX_AUTO_LOADED_SKILLS, LoadedSkillSet, SkillRegistry};
use crate::stream_ui::DeepSeekUiStream;
use crate::tool::{
    ToolContext, ToolOutputKind, ToolPermissionDecision, ToolPermissionPolicy, ToolRegistry,
    ToolResult,
};
use crate::transcript::{load_transcript_jsonl, save_transcript_jsonl};

pub const APP_TITLE: &str = "Atto Agent";
const CHAT_WINDOW_TAG: &str = "atto-agent:chat";
const STATUS_READY: &str = "ready";
const STATUS_STREAMING: &str = "streaming";
const MOCK_TOKEN_DELAY: Duration = Duration::from_millis(24);
const SNAPSHOT_MOCK_TOKEN_DELAY: Duration = Duration::from_millis(96);
const TRANSCRIPT_SAVE_DEBOUNCE: Duration = Duration::from_millis(500);
const SNAPSHOT_COMPACT_POLICY: CompactPolicy = CompactPolicy {
    threshold_tokens: 40,
    recent_message_limit: 2,
    summary_max_bytes: 2048,
};
const MOCK_READ_FILE_PROMPT: &str = "agent-pty-read-file";
const MOCK_RUN_COMMAND_PROMPT: &str = "agent-pty-run-command";
const MOCK_CONTEXT_PROBE_PREFIX: &str = "agent-pty-context-probe";
const MOCK_RETRY_EDIT_PROMPT: &str = "agent-pty-retry-edit-seed";
const ACCEPTED_PLAN_EXECUTION_INSTRUCTION: &str = "The user accepted the plan. Execute the accepted plan now. Use tools only when needed and obey approval policy.";
const PLAN_MODE_MUTATING_TOOL_BLOCKED_RESULT: &str =
    "Plan mode blocks mutating tools until the plan is accepted.";

static MOCK_RETRY_EDIT_TURN_COUNT: AtomicUsize = AtomicUsize::new(0);

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
        mutating_tools_allowed: bool,
        continue_after_tools: bool,
    },
    PlanReady {
        branch: ChatBranchToken,
        message_id: ChatMessageId,
        items: Vec<PlanItem>,
    },
    ToolResultReady {
        branch: ChatBranchToken,
        message_id: ChatMessageId,
        tool_block_id: ChatBlockId,
        call_id: String,
        result: ToolResultBlock,
        mutating_tools_allowed: bool,
        continue_after_tools: bool,
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
    plan_decision: PlanTurnDecision,
    mutating_tools_allowed: bool,
}

#[derive(Clone, Debug)]
struct DeepSeekAgentTurnRequest {
    branch: ChatBranchToken,
    message_id: ChatMessageId,
    block_id: ChatBlockId,
    cancel: CancellationToken,
    config: AgentConfig,
    request: ChatCompletionRequest,
    plan_decision: PlanTurnDecision,
    mutating_tools_allowed: bool,
}

#[derive(Clone, Debug)]
struct AgentTurnStartRequest {
    prompt: String,
    plan_decision: PlanTurnDecision,
    mutating_tools_allowed: bool,
    skill_registry: SkillRegistry,
    loaded_skills: LoadedSkillSet,
}

#[derive(Clone)]
struct ToolExecutionRequest {
    branch: ChatBranchToken,
    message_id: ChatMessageId,
    tool_use: ToolUseBlock,
    config: AgentConfig,
    registry: ToolRegistry,
    limits: AgentTurnLimits,
    action_sender: mpsc::Sender<AppAction>,
    mutating_tools_allowed: bool,
    continue_after_tools: bool,
}

#[derive(Clone, Debug)]
struct TranscriptStatusState {
    token_estimate_state: Property<String>,
    error_summary_state: Property<String>,
}

struct StatusSegmentBindings {
    model: Binding<String>,
    provider: Binding<String>,
    state: Binding<String>,
    plan_mode: Binding<String>,
    tools: Binding<String>,
    skills: Binding<String>,
    tokens: Binding<String>,
    error: Binding<String>,
}

impl TranscriptStatusState {
    fn new() -> Self {
        Self {
            token_estimate_state: Property::new(format_token_estimate_status(0)),
            error_summary_state: Property::new(error_summary_status(&[])),
        }
    }

    fn sync(&self, store: &ChatMessageStore) {
        sync_transcript_status(store, &self.token_estimate_state, &self.error_summary_state);
    }
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
    transcript_status: TranscriptStatusState,
    turn_budgets: TurnBudgetTracker,
}

#[derive(Clone)]
struct AgentTurnLauncher {
    config: AgentConfig,
    action_sender: mpsc::Sender<AppAction>,
    tool_registry: ToolRegistry,
    turn_budgets: TurnBudgetTracker,
    limits: AgentTurnLimits,
    compact_policy: CompactPolicy,
}

#[derive(Clone)]
struct PlanDecisionRuntime {
    input_handle: ChatInputHandle,
    mock_turns: MockTurnRegistry,
    status_state: Property<String>,
    skill_registry: SkillRegistry,
    loaded_skills: LoadedSkillSet,
    transcript_status: TranscriptStatusState,
    turn_launcher: AgentTurnLauncher,
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
    abort_handle: Option<AbortHandle>,
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
    provider_state: Property<String>,
    model_state: Property<String>,
    plan_mode_state: Property<String>,
    tool_count_state: Property<String>,
    skill_count_state: Property<String>,
    transcript_status: TranscriptStatusState,
}

#[derive(Clone)]
struct ToolRuntime {
    config: AgentConfig,
    action_sender: mpsc::Sender<AppAction>,
    registry: ToolRegistry,
    permissions: Arc<Mutex<ToolPermissionPolicy>>,
    turn_budgets: TurnBudgetTracker,
    limits: AgentTurnLimits,
    input_handle: ChatInputHandle,
    mock_turns: MockTurnRegistry,
    status_state: Property<String>,
    skill_registry: SkillRegistry,
    loaded_skills: LoadedSkillSet,
    transcript_status: TranscriptStatusState,
}

struct TranscriptPersistence {
    path: Option<PathBuf>,
    messages: Binding<Vec<ChatMessage>>,
    observer: DirtyObserver,
    pending_dirty: bool,
    last_save: Option<Instant>,
}

impl TranscriptPersistence {
    fn new(path: Option<PathBuf>, store: &ChatMessageStore) -> Self {
        let messages = store.binding();
        let observer = messages.dirty_observer();
        Self {
            path,
            messages,
            observer,
            pending_dirty: false,
            last_save: None,
        }
    }

    fn save_if_dirty(&mut self) -> Result<()> {
        let Some(path) = self.path.as_deref() else {
            return Ok(());
        };
        if self.messages.check_dirty(&mut self.observer) {
            self.pending_dirty = true;
        }
        if !self.pending_dirty {
            return Ok(());
        }
        if self
            .last_save
            .is_some_and(|last_save| last_save.elapsed() < TRANSCRIPT_SAVE_DEBOUNCE)
        {
            return Ok(());
        }
        save_transcript_jsonl(path, &self.messages.get())?;
        self.pending_dirty = false;
        self.last_save = Some(Instant::now());
        Ok(())
    }

    fn save_now(&mut self) -> Result<()> {
        let Some(path) = self.path.as_deref() else {
            return Ok(());
        };
        save_transcript_jsonl(path, &self.messages.get())?;
        self.pending_dirty = false;
        self.last_save = Some(Instant::now());
        Ok(())
    }
}

impl AgentRuntime {
    fn new(
        config: AgentConfig,
        action_sender: mpsc::Sender<AppAction>,
        mock_turns: MockTurnRegistry,
    ) -> Self {
        let model_state = Property::new(format!("model: {}", config.model));
        let provider_state = Property::new(config.provider.status());
        let plan_mode_state = Property::new(config.plan_mode.status());
        let tool_registry =
            crate::tool::builtin_tool_registry().expect("built-in tool registry must be valid");
        let tool_count_state = Property::new(format_tool_count_status(tool_registry.len()));
        let skill_registry = SkillRegistry::discover(&config.workspace, config.home_dir.as_deref());
        let loaded_skills = LoadedSkillSet::default();
        let limits = AgentTurnLimits::default();
        let turn_budgets = TurnBudgetTracker::default();
        let skill_count_state = Property::new(loaded_skills.status());
        let transcript_status = TranscriptStatusState::new();
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
            provider_state,
            model_state,
            plan_mode_state,
            tool_count_state,
            skill_count_state,
            transcript_status,
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
            input_handle: self.input_handle.clone(),
            mock_turns: self.mock_turns.clone(),
            status_state: self.status_state.clone(),
            skill_registry: self.skill_registry.clone(),
            loaded_skills: self.loaded_skills.clone(),
            transcript_status: self.transcript_status.clone(),
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
            transcript_status: self.transcript_status.clone(),
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
        self.start_with_abort_handle(message_id, None)
    }

    fn start_with_abort_handle(
        &self,
        message_id: ChatMessageId,
        abort_handle: Option<AbortHandle>,
    ) -> CancellationToken {
        let cancel = CancellationToken::new();
        *self.current.lock().expect("active turn lock poisoned") = Some(ActiveMockTurn {
            message_id,
            cancel: cancel.clone(),
            abort_handle,
        });
        cancel
    }

    fn cancel(&self, message_id: ChatMessageId) -> bool {
        let mut current = self.current.lock().expect("active turn lock poisoned");
        if !current
            .as_ref()
            .is_some_and(|turn| turn.message_id == message_id)
        {
            return false;
        }
        let Some(turn) = current.take() else {
            return false;
        };
        Self::cancel_active_turn(turn);
        true
    }

    fn cancel_current(&self) -> Option<ChatMessageId> {
        let mut current = self.current.lock().expect("active turn lock poisoned");
        let turn = current.take()?;
        let message_id = turn.message_id;
        Self::cancel_active_turn(turn);
        Some(message_id)
    }

    fn clear(&self, message_id: ChatMessageId) {
        let mut current = self.current.lock().expect("active turn lock poisoned");
        if current
            .as_ref()
            .is_some_and(|turn| turn.message_id == message_id)
        {
            *current = None;
        }
    }

    fn cancel_active_turn(turn: ActiveMockTurn) {
        turn.cancel.cancel();
        if let Some(abort_handle) = turn.abort_handle {
            abort_handle.abort();
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
    provider_state: Property<String>,
    model_state: Property<String>,
    plan_mode_state: Property<String>,
    tool_count_state: Property<String>,
    skill_count_state: Property<String>,
    transcript_status: TranscriptStatusState,
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
        Self::with_runtime_state(screen, EventQueue::new(), runtime, CompactPolicy::default())
    }

    /// Builds the initial app state from a resolved configuration.
    pub fn with_config(screen: Rect, config: AgentConfig) -> Self {
        let (action_sender, _action_receiver) = EventQueue::<AppAction>::channel();
        let runtime = AgentRuntime::new(config, action_sender, MockTurnRegistry::new());
        Self::with_runtime_state(screen, EventQueue::new(), runtime, CompactPolicy::default())
    }

    fn with_runtime_state(
        screen: Rect,
        quit_events: EventQueue<()>,
        runtime: AgentRuntime,
        compact_policy: CompactPolicy,
    ) -> Self {
        runtime.transcript_status.sync(&runtime.message_store);
        let chat_panel = build_chat_panel(
            &runtime.message_store,
            AgentTurnLauncher {
                config: runtime.config.clone(),
                action_sender: runtime.action_sender.clone(),
                tool_registry: runtime.tool_registry.clone(),
                turn_budgets: runtime.turn_budgets.clone(),
                limits: runtime.limits,
                compact_policy,
            },
            runtime.slash_runtime(),
            runtime.tool_runtime(),
        );

        let mut desktop = Desktop::new(Theme::dark(), agent_menu(quit_events));
        desktop
            .status
            .set_segments(status_segments(StatusSegmentBindings {
                model: runtime.model_state.binding(),
                provider: runtime.provider_state.binding(),
                state: runtime.status_state.binding(),
                plan_mode: runtime.plan_mode_state.binding(),
                tools: runtime.tool_count_state.binding(),
                skills: runtime.skill_count_state.binding(),
                tokens: runtime.transcript_status.token_estimate_state.binding(),
                error: runtime.transcript_status.error_summary_state.binding(),
            }));

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
            provider_state: runtime.provider_state,
            model_state: runtime.model_state,
            plan_mode_state: runtime.plan_mode_state,
            tool_count_state: runtime.tool_count_state,
            skill_count_state: runtime.skill_count_state,
            transcript_status: runtime.transcript_status,
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

    pub fn provider_state(&self) -> Property<String> {
        self.provider_state.clone()
    }

    pub fn model_state(&self) -> Property<String> {
        self.model_state.clone()
    }

    pub fn plan_mode_state(&self) -> Property<String> {
        self.plan_mode_state.clone()
    }

    pub fn tool_count_state(&self) -> Property<String> {
        self.tool_count_state.clone()
    }

    pub fn skill_count_state(&self) -> Property<String> {
        self.skill_count_state.clone()
    }

    pub fn token_estimate_state(&self) -> Property<String> {
        self.transcript_status.token_estimate_state.clone()
    }

    pub fn error_summary_state(&self) -> Property<String> {
        self.transcript_status.error_summary_state.clone()
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
    run_with_config_mock_token_delay_and_compact_policy(
        AgentConfig::defaults(env!("CARGO_MANIFEST_DIR")),
        SNAPSHOT_MOCK_TOKEN_DELAY,
        SNAPSHOT_COMPACT_POLICY,
    )
}

fn run_with_config_and_mock_token_delay(
    config: AgentConfig,
    mock_token_delay: Duration,
) -> Result<()> {
    run_with_config_mock_token_delay_and_compact_policy(
        config,
        mock_token_delay,
        CompactPolicy::default(),
    )
}

fn run_with_config_mock_token_delay_and_compact_policy(
    config: AgentConfig,
    mock_token_delay: Duration,
    compact_policy: CompactPolicy,
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
    restore_transcript_if_configured(
        &runtime.message_store,
        runtime.config.transcript_path.as_deref(),
    )?;
    let transcript_persistence = Arc::new(Mutex::new(TranscriptPersistence::new(
        runtime.config.transcript_path.clone(),
        &runtime.message_store,
    )));
    let runtime_for_build = runtime.clone();
    let runtime_for_actions = runtime.clone();
    let persistence_for_actions = transcript_persistence.clone();
    let persistence_for_loop = transcript_persistence.clone();

    let run_result = run_crossterm_desktop_with_actions(
        CrosstermAppConfig::default()
            .bracketed_paste(true)
            .cursor(CursorMode::Show),
        move |screen| {
            Ok(AgentApp::with_runtime_state(
                screen,
                quit_events_for_menu,
                runtime_for_build.clone(),
                compact_policy,
            )
            .into_desktop())
        },
        action_receiver,
        move |_desktop, action, _screen| {
            let tool_runtime = runtime_for_actions.tool_runtime();
            let changed = apply_app_action(
                &runtime_for_actions.message_store,
                &runtime_for_actions.input_handle,
                &runtime_for_actions.mock_turns,
                &runtime_for_actions.status_state,
                &runtime_for_actions.transcript_status,
                &tool_runtime,
                action,
            );
            if changed {
                persistence_for_actions
                    .lock()
                    .expect("transcript persistence lock poisoned")
                    .save_if_dirty()?;
            }
            Ok(AppControl::Continue)
        },
        move |_desktop, _screen| {
            persistence_for_loop
                .lock()
                .expect("transcript persistence lock poisoned")
                .save_if_dirty()?;
            if quit_events_for_loop.pop().is_some() {
                Ok(AppControl::Exit)
            } else {
                Ok(AppControl::Continue)
            }
        },
        |_desktop, _event, _screen, _result| Ok(AppControl::Continue),
    );
    let save_result = transcript_persistence
        .lock()
        .expect("transcript persistence lock poisoned")
        .save_now();
    run_result.and(save_result)
}

fn restore_transcript_if_configured(store: &ChatMessageStore, path: Option<&Path>) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    let messages = load_transcript_jsonl(path)?;
    if !messages.is_empty() {
        store.replace_all(messages);
    }
    Ok(())
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

    let _ = start_agent_turn_from_user_prompt(store, slash_runtime, turn_launcher, text, true);
}

fn handle_edit_and_resubmit(
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

fn handle_message_action(
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

fn start_agent_turn_from_user_prompt(
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

fn cancel_active_turn_after_transcript_truncation(
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

fn latest_user_prompt(store: &ChatMessageStore) -> Option<String> {
    store.messages().iter().rev().find_map(user_prompt_text)
}

fn user_prompt_text(message: &ChatMessage) -> Option<String> {
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

fn mutating_tools_allowed_for_turn(plan_mode: PlanMode, plan_decision: &PlanTurnDecision) -> bool {
    plan_mode == PlanMode::Off && !plan_decision.requires_plan()
}

fn start_agent_turn_for_request(
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

fn deepseek_live_request_for_turn(
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

fn auto_load_matching_skills(
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
            .detail("Cancel the active turn")
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

fn cancel_latest_streaming_turn(
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

fn finish_canceled_turn(
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

fn help_text() -> &'static str {
    "Available commands:\n\
- /help: Show this help.\n\
- /clear: Clear the current conversation and keep app configuration.\n\
- /plan [on|off|auto]: Cycle or set the basic plan mode state.\n\
- /skills: List available skills.\n\
- `/skill <name>`: Activate a skill for this session.\n\
- /tools: List available tools and approval policy.\n\
- /abort: Cancel the active turn."
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

fn spawn_deepseek_agent_turn(
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

async fn run_deepseek_agent_turn(
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

fn send_stream_actions(action_sender: &mpsc::Sender<AppAction>, actions: Vec<AppAction>) -> bool {
    for action in actions {
        if action_sender.send(action).is_err() {
            return false;
        }
    }
    true
}

fn deepseek_runtime_error(error: std::io::Error) -> ChatError {
    ChatError::new(
        ChatErrorKind::Other,
        "Failed to start DeepSeek async runtime.",
    )
    .with_detail(error.to_string())
}

fn deepseek_turn_cancelled_error() -> ChatError {
    ChatError::new(ChatErrorKind::Other, "DeepSeek turn was canceled.")
}

fn ui_action_channel_closed_error() -> ChatError {
    ChatError::new(
        ChatErrorKind::Other,
        "UI action channel closed before DeepSeek turn finished.",
    )
}

fn mock_agent_events(
    prompt: &str,
    plan_decision: &PlanTurnDecision,
) -> Vec<ChatCompletionSseEvent> {
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

fn mock_context_probe_events(prompt: &str) -> Vec<ChatCompletionSseEvent> {
    vec![
        mock_stream_content_event("Mock context probe:\n".to_string()),
        mock_stream_content_event(mock_context_probe_text(prompt)),
        mock_stream_finish_event(),
        ChatCompletionSseEvent::Done,
    ]
}

fn mock_context_probe_text(prompt: &str) -> String {
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

fn apply_app_action(
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

fn maybe_continue_deepseek_tool_loop(
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

fn tool_loop_ready_for_continuation(messages: &[ChatMessage], message_id: ChatMessageId) -> bool {
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

#[derive(Clone, Debug)]
struct PreparedToolCall {
    tool_use: ToolUseBlock,
    result: Option<ToolResultBlock>,
}

fn prepare_tool_call(
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

fn handle_plan_decision(
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

fn continue_after_accepted_plan(
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

fn finish_plan_decision_turn(
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
            message_id: request.message_id,
            tool_block_id,
            call_id,
            result,
            mutating_tools_allowed: request.mutating_tools_allowed,
            continue_after_tools: request.continue_after_tools,
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
    ChatCompletionRequest::from_config(
        config,
        ContextBuilder::new()
            .with_file_mentions(&config.workspace)
            .build_messages(messages),
    )
    .with_tools(registry.chat_tools())
    .with_tool_choice(ToolChoice::Mode(ToolChoiceMode::Auto))
}

/// Builds the OpenAI-compatible request body and prepends active skills to the system prompt.
pub fn deepseek_request_from_transcript_with_skills(
    config: &AgentConfig,
    registry: &ToolRegistry,
    skill_registry: &SkillRegistry,
    loaded_skills: &LoadedSkillSet,
    messages: &[ChatMessage],
) -> ChatCompletionRequest {
    ChatCompletionRequest::from_config(
        config,
        ContextBuilder::new()
            .with_skills(skill_registry, loaded_skills)
            .with_file_mentions(&config.workspace)
            .build_messages(messages),
    )
    .with_tools(registry.chat_tools())
    .with_tool_choice(ToolChoice::Mode(ToolChoiceMode::Auto))
}

/// Builds the plan-draft request body and exposes only the virtual `submit_plan` tool.
pub fn deepseek_plan_request_from_transcript(
    config: &AgentConfig,
    messages: &[ChatMessage],
) -> ChatCompletionRequest {
    deepseek_plan_request_from_messages(
        config,
        ContextBuilder::new()
            .with_file_mentions(&config.workspace)
            .build_messages(messages),
    )
}

/// Builds a plan-draft request while preserving active skill prompt injection.
pub fn deepseek_plan_request_from_transcript_with_skills(
    config: &AgentConfig,
    skill_registry: &SkillRegistry,
    loaded_skills: &LoadedSkillSet,
    messages: &[ChatMessage],
) -> ChatCompletionRequest {
    deepseek_plan_request_from_messages(
        config,
        ContextBuilder::new()
            .with_skills(skill_registry, loaded_skills)
            .with_file_mentions(&config.workspace)
            .build_messages(messages),
    )
}

fn deepseek_plan_request_from_messages(
    config: &AgentConfig,
    mut messages: Vec<ChatCompletionMessage>,
) -> ChatCompletionRequest {
    messages.insert(0, ChatCompletionMessage::system(PLAN_MODE_SYSTEM_PROMPT));
    ChatCompletionRequest::from_config(config, messages)
        .with_tools(vec![submit_plan_chat_tool()])
        .with_tool_choice(submit_plan_tool_choice())
}

/// Converts the UI transcript into DeepSeek/OpenAI-compatible chat messages.
pub fn deepseek_messages_from_transcript(messages: &[ChatMessage]) -> Vec<ChatCompletionMessage> {
    ContextBuilder::new().build_messages(messages)
}

/// Converts the UI transcript and active skills into DeepSeek/OpenAI-compatible messages.
pub fn deepseek_messages_from_transcript_with_skills(
    skill_registry: &SkillRegistry,
    loaded_skills: &LoadedSkillSet,
    messages: &[ChatMessage],
) -> Vec<ChatCompletionMessage> {
    ContextBuilder::new()
        .with_skills(skill_registry, loaded_skills)
        .build_messages(messages)
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

fn sync_transcript_status(
    store: &ChatMessageStore,
    token_estimate_state: &Property<String>,
    error_summary_state: &Property<String>,
) {
    let messages = store.messages();
    token_estimate_state.set(format_token_estimate_status(estimate_transcript_tokens(
        &messages,
    )));
    error_summary_state.set(error_summary_status(&messages));
}

fn format_tool_count_status(count: usize) -> String {
    format!("tools: {count}")
}

fn format_token_estimate_status(tokens: u64) -> String {
    format!("tokens~{tokens}")
}

fn error_summary_status(messages: &[ChatMessage]) -> String {
    latest_error_summary(messages).unwrap_or_else(|| "err:ok".to_string())
}

fn latest_error_summary(messages: &[ChatMessage]) -> Option<String> {
    for message in messages.iter().rev() {
        if let ChatTurnStatus::Failed(error) = &message.status {
            return Some(format_status_error_summary(
                chat_error_kind_label(&error.kind),
                &error.message,
            ));
        }
        for block in message.blocks.iter().rev() {
            if let ChatBlock::ToolResult(result) = block
                && !result.ok
            {
                return Some(format_status_error_summary("tool", result.output.as_text()));
            }
        }
    }
    None
}

fn format_status_error_summary(kind: &str, message: &str) -> String {
    truncate_status_text(
        &format!("err:{kind} {}", normalize_status_text(message)),
        36,
    )
}

fn normalize_status_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_status_text(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let prefix = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_none() {
        return prefix;
    }
    let keep = max_chars.saturating_sub(3);
    format!("{}...", text.chars().take(keep).collect::<String>())
}

fn chat_error_kind_label(kind: &ChatErrorKind) -> &'static str {
    match kind {
        ChatErrorKind::Api => "api",
        ChatErrorKind::Tool => "tool",
        ChatErrorKind::RateLimit => "rate",
        ChatErrorKind::Refusal => "refusal",
        ChatErrorKind::Network => "network",
        ChatErrorKind::Other => "other",
    }
}

fn agent_menu(quit_events: EventQueue<()>) -> MenuBar {
    // Keep the initial app shell minimal while still offering a discoverable quit action.
    MenuBar::new(vec![MenuSpec::new(
        "File",
        vec![MenuItem::action("Quit", move || quit_events.push(())).shortcut("q")],
    )])
}

fn status_segments(bindings: StatusSegmentBindings) -> Vec<StatusSegment> {
    vec![
        StatusSegment::new("app", APP_TITLE)
            .priority(40)
            .min_width(10),
        StatusSegment::new("provider", bindings.provider)
            .priority(86)
            .min_width(18),
        StatusSegment::new("model", bindings.model)
            .priority(95)
            .min_width(18),
        StatusSegment::new("plan", bindings.plan_mode)
            .priority(94)
            .min_width(9),
        StatusSegment::new("tools", bindings.tools)
            .priority(93)
            .min_width(8),
        StatusSegment::new("skills", bindings.skills)
            .priority(92)
            .min_width(9),
        StatusSegment::new("tokens", bindings.tokens)
            .priority(91)
            .min_width(8),
        StatusSegment::new("error", bindings.error)
            .align(StatusSegmentAlign::Right)
            .priority(89)
            .min_width(6),
        StatusSegment::new("streaming", bindings.state)
            .align(StatusSegmentAlign::Right)
            .priority(90)
            .min_width(9),
        StatusSegment::new("keys", "Esc cancel | Ctrl+Q quit | /help")
            .align(StatusSegmentAlign::Right)
            .priority(30)
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
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use atto_ui::ComponentValue;
    use atto_ui::composable::{
        ComponentContext, EventHandling, MouseCoordinateSpace, ScrollbarHost, TabMode,
    };
    use atto_ui::theme::Theme;
    use atto_ui::wm::WindowId;
    use atto_ui_chat::{
        ApprovalAction, ApprovalDecision, ApprovalLevel, ChatBlock, ChatBlockId, ChatError,
        ChatErrorKind, ChatInputMode, ChatInputResponse, ChatMessage, ChatMessageStore, ChatRole,
        ChatSlashCommandAction, ChatTurnStatus, CompactBlock, CompactStatus, EditAndResubmitEvent,
        MessageAction, MessageActionKind, PlanBlock, PlanDecision, PlanDecisionEvent, PlanItem,
        StopReason, TokenUsage, ToolInput, ToolOutput, ToolResultBlock, ToolStatus, ToolUseBlock,
    };
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::layout::Rect;

    use crate::compact::CompactPolicy;
    use crate::config::{AgentConfig, AgentProvider, PlanMode};
    use crate::deepseek::{
        ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionDelta,
        ChatCompletionSseEvent, ChatFunctionCallDelta, ChatMessageRole, ChatToolCallDelta,
        ChatToolKind, ToolChoice, ToolChoiceMode, chat_error_from_http_status,
        chat_error_from_json_error, chat_error_from_network_failure,
        chat_error_from_stream_disconnect, parse_chat_completion_sse,
        parse_chat_completion_sse_data,
    };
    use crate::skill::{LoadedSkillSet, SkillMode, SkillRegistry, SkillSearchPath};
    use crate::stream_ui::DeepSeekUiStream;
    use crate::tool::{
        ToolContext, ToolExecutor, ToolOutputKind, ToolPermission, ToolPermissionPolicy,
        ToolRegistry, ToolResult, ToolSpec,
    };

    use super::{
        ACCEPTED_PLAN_EXECUTION_INSTRUCTION, APP_TITLE, AgentApp, AgentTurnLauncher,
        AgentTurnLimits, AppAction, MockTurnRegistry, PLAN_MODE_MUTATING_TOOL_BLOCKED_RESULT,
        PlanDecisionRuntime, STATUS_READY, STATUS_STREAMING, SlashRuntime, StatusSegmentBindings,
        ToolRuntime, TranscriptPersistence, TranscriptStatusState, TurnBudgetTracker,
        apply_app_action, build_chat_panel, deepseek_plan_request_from_transcript,
        deepseek_request_from_transcript, deepseek_request_from_transcript_with_skills,
        error_summary_status, execute_tool_use_to_result_block, format_token_estimate_status,
        handle_edit_and_resubmit, handle_message_action, handle_plan_decision,
        handle_tool_approval, status_segments, submit_input_response, submit_slash_command_text,
        sync_transcript_status,
    };

    fn message_text(message: &ChatMessage) -> &str {
        match &message.blocks[0] {
            ChatBlock::Text(block) => &block.markdown,
            other => panic!("expected text block, got {other:?}"),
        }
    }

    fn plan_decision(store: &ChatMessageStore, block_id: ChatBlockId) -> PlanDecision {
        store
            .with_block(block_id, |block| match block {
                ChatBlock::Plan(plan) => plan.decision,
                other => panic!("expected plan block, got {other:?}"),
            })
            .expect("plan block should exist")
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
            transcript_status: TranscriptStatusState::new(),
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
        write_test_skill_with_mode(
            workspace,
            dir_name,
            name,
            description,
            SkillMode::Manual,
            &[],
        );
    }

    fn write_test_skill_with_mode(
        workspace: &Path,
        dir_name: &str,
        name: &str,
        description: &str,
        mode: SkillMode,
        triggers: &[&str],
    ) {
        write_test_skill_with_mode_and_tools(
            workspace,
            dir_name,
            name,
            description,
            mode,
            triggers,
            &[],
        );
    }

    fn write_test_skill_with_mode_and_tools(
        workspace: &Path,
        dir_name: &str,
        name: &str,
        description: &str,
        mode: SkillMode,
        triggers: &[&str],
        tools: &[&str],
    ) {
        let dir = workspace.join(".atto/skills").join(dir_name);
        fs::create_dir_all(&dir).expect("test skill directory should be created");
        let triggers = triggers
            .iter()
            .map(|trigger| format!("\"{trigger}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let tools = tools
            .iter()
            .map(|tool| format!("\"{tool}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            dir.join("SKILL.md"),
            format!(
                r#"---
name: {name}
description: {description}
triggers: [{triggers}]
tools: [{tools}]
mode: {mode}
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
        test_tool_runtime_with_limits_and_budgets(
            config,
            action_sender,
            registry,
            permissions,
            limits,
            TurnBudgetTracker::default(),
        )
    }

    fn test_tool_runtime_with_limits_and_budgets(
        config: AgentConfig,
        action_sender: std::sync::mpsc::Sender<AppAction>,
        registry: ToolRegistry,
        permissions: Arc<Mutex<ToolPermissionPolicy>>,
        limits: AgentTurnLimits,
        turn_budgets: TurnBudgetTracker,
    ) -> ToolRuntime {
        ToolRuntime {
            config,
            action_sender,
            registry,
            permissions,
            turn_budgets,
            limits,
            input_handle: atto_ui_chat::ChatInputHandle::new(),
            mock_turns: MockTurnRegistry::new(),
            status_state: atto_ui::reactive::Property::new(STATUS_READY.to_string()),
            skill_registry: SkillRegistry::default(),
            loaded_skills: LoadedSkillSet::default(),
            transcript_status: TranscriptStatusState::new(),
        }
    }

    struct LiveToolRuntimeParts<'a> {
        config: AgentConfig,
        action_sender: std::sync::mpsc::Sender<AppAction>,
        registry: ToolRegistry,
        turn_budgets: TurnBudgetTracker,
        limits: AgentTurnLimits,
        input_handle: &'a atto_ui_chat::ChatInputHandle,
        mock_turns: &'a MockTurnRegistry,
        status_state: &'a atto_ui::reactive::Property<String>,
        transcript_status: &'a TranscriptStatusState,
    }

    fn live_tool_runtime(parts: LiveToolRuntimeParts<'_>) -> ToolRuntime {
        ToolRuntime {
            config: parts.config,
            action_sender: parts.action_sender,
            registry: parts.registry,
            permissions: test_tool_permissions(),
            turn_budgets: parts.turn_budgets,
            limits: parts.limits,
            input_handle: parts.input_handle.clone(),
            mock_turns: parts.mock_turns.clone(),
            status_state: parts.status_state.clone(),
            skill_registry: SkillRegistry::default(),
            loaded_skills: LoadedSkillSet::default(),
            transcript_status: parts.transcript_status.clone(),
        }
    }

    fn apply_live_actions_until_idle(
        receiver: &std::sync::mpsc::Receiver<AppAction>,
        store: &ChatMessageStore,
        input_handle: &atto_ui_chat::ChatInputHandle,
        mock_turns: &MockTurnRegistry,
        status_state: &atto_ui::reactive::Property<String>,
        transcript_status: &TranscriptStatusState,
        tool_runtime: &ToolRuntime,
    ) {
        for _ in 0..16 {
            let action = receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("live DeepSeek tool loop should emit an app action");
            apply_app_action(
                store,
                input_handle,
                mock_turns,
                status_state,
                transcript_status,
                tool_runtime,
                action,
            );
            if !input_handle.streaming_binding().get() {
                return;
            }
        }
        panic!("live DeepSeek tool loop did not become idle");
    }

    fn test_plan_decision_runtime(
        input_handle: &atto_ui_chat::ChatInputHandle,
        mock_turns: &MockTurnRegistry,
        status_state: &atto_ui::reactive::Property<String>,
        turn_budgets: &TurnBudgetTracker,
    ) -> (PlanDecisionRuntime, std::sync::mpsc::Receiver<AppAction>) {
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        (
            PlanDecisionRuntime {
                input_handle: input_handle.clone(),
                mock_turns: mock_turns.clone(),
                status_state: status_state.clone(),
                skill_registry: SkillRegistry::default(),
                loaded_skills: LoadedSkillSet::default(),
                transcript_status: TranscriptStatusState::new(),
                turn_launcher: AgentTurnLauncher {
                    config: AgentConfig::defaults("."),
                    action_sender: sender,
                    tool_registry: test_tool_registry(),
                    turn_budgets: turn_budgets.clone(),
                    limits: AgentTurnLimits::default(),
                    compact_policy: CompactPolicy::default(),
                },
            },
            receiver,
        )
    }

    fn test_turn_launcher(
        action_sender: std::sync::mpsc::Sender<AppAction>,
        turn_budgets: &TurnBudgetTracker,
        compact_policy: CompactPolicy,
    ) -> AgentTurnLauncher {
        AgentTurnLauncher {
            config: AgentConfig::defaults("."),
            action_sender,
            tool_registry: test_tool_registry(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy,
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
        let transcript_status = TranscriptStatusState::new();
        apply_app_action(
            store,
            input_handle,
            mock_turns,
            status_state,
            &transcript_status,
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

    struct TestSseServer {
        address: String,
        handle: thread::JoinHandle<String>,
    }

    struct TestSseSequenceServer {
        address: String,
        handle: thread::JoinHandle<Vec<String>>,
    }

    struct TestAbortableSseServer {
        address: String,
        first_event_sent: mpsc::Receiver<()>,
        handle: thread::JoinHandle<(String, bool)>,
    }

    impl TestSseServer {
        fn spawn(body: impl Into<String>) -> Self {
            Self::spawn_response(200, "OK", "text/event-stream", body)
        }
        fn spawn_response(
            status: u16,
            reason: &'static str,
            content_type: &'static str,
            body: impl Into<String>,
        ) -> Self {
            let body = body.into();
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock SSE server");
            listener
                .set_nonblocking(true)
                .expect("configure mock SSE listener");
            let address = listener
                .local_addr()
                .expect("mock SSE server address")
                .to_string();
            let handle = thread::spawn(move || {
                let (mut stream, _) = accept_with_timeout(&listener, Duration::from_secs(5));
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("configure mock SSE read timeout");
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .expect("configure mock SSE write timeout");
                let request = read_http_request(&mut stream);
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write mock SSE response");
                request
            });
            Self { address, handle }
        }

        fn base_url(&self) -> String {
            format!("http://{}/v1", self.address)
        }

        fn join(self) -> String {
            self.handle.join().expect("mock SSE server should join")
        }
    }

    impl TestAbortableSseServer {
        fn spawn(first_event: impl Into<String>) -> Self {
            let first_event = first_event.into();
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock SSE server");
            listener
                .set_nonblocking(true)
                .expect("configure mock SSE listener");
            let address = listener
                .local_addr()
                .expect("mock SSE server address")
                .to_string();
            let (sent_tx, first_event_sent) = mpsc::channel();
            let handle = thread::spawn(move || {
                let (mut stream, _) = accept_with_timeout(&listener, Duration::from_secs(5));
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("configure mock SSE read timeout");
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .expect("configure mock SSE write timeout");
                let request = read_http_request(&mut stream);
                let response_headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                stream
                    .write_all(response_headers.as_bytes())
                    .expect("write mock SSE response headers");
                stream
                    .write_all(first_event.as_bytes())
                    .expect("write first mock SSE event");
                stream.flush().expect("flush first mock SSE event");
                sent_tx.send(()).ok();
                let closed = wait_for_client_close(&mut stream, Duration::from_secs(3));
                (request, closed)
            });
            Self {
                address,
                first_event_sent,
                handle,
            }
        }

        fn base_url(&self) -> String {
            format!("http://{}/v1", self.address)
        }

        fn wait_for_first_event(&self) {
            self.first_event_sent
                .recv_timeout(Duration::from_secs(2))
                .expect("mock SSE server should send the first event");
        }

        fn join(self) -> (String, bool) {
            self.handle.join().expect("mock SSE server should join")
        }
    }

    impl TestSseSequenceServer {
        fn spawn(bodies: Vec<String>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock SSE server");
            listener
                .set_nonblocking(true)
                .expect("configure mock SSE listener");
            let address = listener
                .local_addr()
                .expect("mock SSE server address")
                .to_string();
            let handle = thread::spawn(move || {
                let mut requests = Vec::new();
                for body in bodies {
                    let (mut stream, _) = accept_with_timeout(&listener, Duration::from_secs(5));
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .expect("configure mock SSE read timeout");
                    stream
                        .set_write_timeout(Some(Duration::from_secs(5)))
                        .expect("configure mock SSE write timeout");
                    requests.push(read_http_request(&mut stream));
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream
                        .write_all(response.as_bytes())
                        .expect("write mock SSE response");
                }
                requests
            });
            Self { address, handle }
        }

        fn base_url(&self) -> String {
            format!("http://{}/v1", self.address)
        }

        fn join(self) -> Vec<String> {
            self.handle.join().expect("mock SSE server should join")
        }
    }

    fn sse_tool_call_body(call_id: &str, name: &str, arguments: serde_json::Value) -> String {
        let chunk = serde_json::json!({
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": arguments.to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });
        format!("data: {chunk}\n\ndata: [DONE]\n\n")
    }

    fn sse_final_text_body(text: &str) -> String {
        let chunk = serde_json::json!({
            "model": "mock-deepseek",
            "choices": [{
                "index": 0,
                "delta": { "content": text },
                "finish_reason": "stop"
            }]
        });
        format!("data: {chunk}\n\ndata: [DONE]\n\n")
    }

    fn accept_with_timeout(
        listener: &TcpListener,
        timeout: Duration,
    ) -> (TcpStream, std::net::SocketAddr) {
        let start = Instant::now();
        loop {
            match listener.accept() {
                Ok(stream) => return stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && start.elapsed() < timeout =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept mock SSE request: {error}"),
            }
        }
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let read = stream.read(&mut buffer).expect("read mock HTTP request");
            if read == 0 {
                break;
            }

            bytes.extend_from_slice(&buffer[..read]);
            if http_request_body_complete(&bytes) {
                break;
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn wait_for_client_close(stream: &mut TcpStream, timeout: Duration) -> bool {
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("configure mock SSE close probe timeout");
        let start = Instant::now();
        let mut buffer = [0_u8; 1];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => return true,
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    if start.elapsed() >= timeout {
                        return false;
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionAborted
                            | std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::BrokenPipe
                    ) =>
                {
                    return true;
                }
                Err(error) => panic!("probe mock SSE client close: {error}"),
            }
        }
    }

    fn http_request_body_complete(bytes: &[u8]) -> bool {
        let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            return false;
        };
        let headers = String::from_utf8_lossy(&bytes[..header_end]);
        let content_length = headers.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        });
        let body_start = header_end + 4;
        bytes.len() >= body_start + content_length.unwrap_or(0)
    }

    fn http_request_json(request: &str) -> serde_json::Value {
        let (_, body) = request
            .split_once("\r\n\r\n")
            .expect("mock HTTP request should contain a header/body separator");
        serde_json::from_str(body).expect("mock HTTP request body should be JSON")
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
        let transcript_status = TranscriptStatusState::new();

        assert!(apply_app_action(
            store,
            input_handle,
            mock_turns,
            status_state,
            &transcript_status,
            &tool_runtime,
            AppAction::ToolCallsReady {
                branch,
                message_id: assistant_id,
                tool_calls: vec![tool_call],
                mutating_tools_allowed: true,
                continue_after_tools: false,
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
        assert_eq!(app.provider_state().get(), AgentProvider::Mock.status());
        assert_eq!(app.model_state().get(), "model: deepseek-chat");
        assert_eq!(app.plan_mode_state().get(), PlanMode::Auto.status());
        assert_eq!(app.tool_count_state().get(), "tools: 5");
        assert_eq!(app.skill_count_state().get(), "skills: 0");
        assert_eq!(app.token_estimate_state().get(), "tokens~0");
        assert_eq!(app.error_summary_state().get(), "err:ok");
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
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.model = "deepseek-reasoner".to_string();
        config.plan_mode = PlanMode::On;

        let app = AgentApp::with_config(Rect::new(0, 0, 80, 24), config);

        assert_eq!(app.config().provider, AgentProvider::DeepSeek);
        assert_eq!(app.provider_state().get(), AgentProvider::DeepSeek.status());
        assert_eq!(app.config().model, "deepseek-reasoner");
        assert_eq!(app.model_state().get(), "model: deepseek-reasoner");
        assert_eq!(app.plan_mode_state().get(), PlanMode::On.status());
    }

    #[test]
    fn status_bar_segments_include_agent_runtime_fields() {
        let model = atto_ui::reactive::Property::new("model: deepseek-chat".to_string());
        let provider = atto_ui::reactive::Property::new(AgentProvider::DeepSeek.status());
        let state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan = atto_ui::reactive::Property::new(PlanMode::Auto.status());
        let tools = atto_ui::reactive::Property::new("tools: 5".to_string());
        let skills = atto_ui::reactive::Property::new("skills: 0".to_string());
        let tokens = atto_ui::reactive::Property::new("tokens~0".to_string());
        let error = atto_ui::reactive::Property::new("err:ok".to_string());

        let segments = status_segments(StatusSegmentBindings {
            model: model.binding(),
            provider: provider.binding(),
            state: state.binding(),
            plan_mode: plan.binding(),
            tools: tools.binding(),
            skills: skills.binding(),
            tokens: tokens.binding(),
            error: error.binding(),
        });
        let pairs = segments
            .iter()
            .map(|segment| (segment.id.as_str(), segment.text.get()))
            .collect::<Vec<_>>();

        assert!(pairs.contains(&("model", "model: deepseek-chat".to_string())));
        assert!(pairs.contains(&("provider", "provider: deepseek".to_string())));
        assert!(pairs.contains(&("plan", "plan: auto".to_string())));
        assert!(pairs.contains(&("tools", "tools: 5".to_string())));
        assert!(pairs.contains(&("skills", "skills: 0".to_string())));
        assert!(pairs.contains(&("tokens", "tokens~0".to_string())));
        assert!(pairs.contains(&("error", "err:ok".to_string())));
        assert!(pairs.contains(&("streaming", STATUS_READY.to_string())));
    }

    #[test]
    fn transcript_status_summarizes_tokens_and_latest_error() {
        let store = ChatMessageStore::new();
        let token_estimate_state =
            atto_ui::reactive::Property::new(format_token_estimate_status(0));
        let error_summary_state = atto_ui::reactive::Property::new(error_summary_status(&[]));

        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "hello status tokens",
        ));
        sync_transcript_status(&store, &token_estimate_state, &error_summary_state);

        assert_ne!(token_estimate_state.get(), "tokens~0");
        assert_eq!(error_summary_state.get(), "err:ok");

        store.push(
            ChatMessage::text(store.next_message_id(), ChatRole::Assistant, "").with_status(
                ChatTurnStatus::Failed(ChatError::new(
                    ChatErrorKind::Network,
                    "Network stream disconnected while reading SSE",
                )),
            ),
        );
        sync_transcript_status(&store, &token_estimate_state, &error_summary_state);

        assert!(error_summary_state.get().starts_with("err:network"));
    }

    #[test]
    fn transcript_persistence_debounces_dirty_saves_and_flushes_on_save_now() {
        let workspace = unique_temp_dir("transcript-debounce");
        fs::create_dir_all(&workspace).expect("create workspace");
        let transcript_path = workspace.join("session.jsonl");
        let store = ChatMessageStore::new();
        let mut persistence = TranscriptPersistence::new(Some(transcript_path.clone()), &store);

        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "first persisted message",
        ));
        persistence
            .save_if_dirty()
            .expect("initial save should pass");
        let saved = fs::read_to_string(&transcript_path).expect("read initial transcript");
        assert!(saved.contains("first persisted message"));

        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "second pending message",
        ));
        persistence
            .save_if_dirty()
            .expect("debounced save should pass");
        let debounced = fs::read_to_string(&transcript_path).expect("read debounced transcript");
        assert!(!debounced.contains("second pending message"));

        persistence.save_now().expect("final flush should pass");
        let flushed = fs::read_to_string(&transcript_path).expect("read flushed transcript");
        assert!(flushed.contains("second pending message"));

        let _ = fs::remove_dir_all(workspace);
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
                config: AgentConfig::defaults("."),
                action_sender: sender.clone(),
                tool_registry: test_tool_registry(),
                turn_budgets: turn_budgets.clone(),
                limits: AgentTurnLimits::default(),
                compact_policy: CompactPolicy::default(),
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
            config: AgentConfig::defaults("."),
            action_sender: sender,
            tool_registry: test_tool_registry(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy: CompactPolicy::default(),
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
    fn deepseek_provider_streams_live_events_through_app_actions() {
        let server = TestSseServer::spawn(concat!(
            "data: {\"model\":\"mock-deepseek\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello from live\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        ));
        let mut config = AgentConfig::defaults(".");
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::Off;

        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let turn_launcher = AgentTurnLauncher {
            config: config.clone(),
            action_sender: sender.clone(),
            tool_registry: test_tool_registry(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy: CompactPolicy::default(),
        };
        let tool_runtime = test_tool_runtime(
            config.clone(),
            sender,
            test_tool_registry(),
            test_tool_permissions(),
        );
        let transcript_status = TranscriptStatusState::new();

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
            ChatInputResponse::Text("live prompt".to_string()),
        );

        for _ in 0..4 {
            let action = receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("live DeepSeek turn should emit an app action");
            apply_app_action(
                &store,
                &input_handle,
                &mock_turns,
                &status_state,
                &transcript_status,
                &tool_runtime,
                action,
            );
            if !input_handle.streaming_binding().get() {
                break;
            }
        }

        let request = server.join();
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(request.contains(r#""content":"live prompt""#));
        let messages = store.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(messages[1].role, ChatRole::Assistant);
        assert_eq!(message_text(&messages[1]), "hello from live");
        assert_eq!(messages[1].status, ChatTurnStatus::Complete);
        assert_eq!(messages[1].meta.model.as_deref(), Some("mock-deepseek"));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }

    #[test]
    fn deepseek_provider_abort_slash_cancels_in_flight_http_request_and_rejects_late_events() {
        let server = TestAbortableSseServer::spawn(
            "data: {\"model\":\"mock-deepseek\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"first live token\"},\"finish_reason\":null}]}\n\n",
        );
        let mut config = AgentConfig::defaults(".");
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::Off;

        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let turn_launcher = AgentTurnLauncher {
            config: config.clone(),
            action_sender: sender.clone(),
            tool_registry: test_tool_registry(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy: CompactPolicy::default(),
        };
        let tool_runtime = test_tool_runtime(
            config,
            sender,
            test_tool_registry(),
            test_tool_permissions(),
        );
        let transcript_status = TranscriptStatusState::new();

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
            ChatInputResponse::Text("live cancel prompt".to_string()),
        );
        server.wait_for_first_event();

        let action = receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("live DeepSeek turn should emit the first token");
        assert!(apply_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
            action,
        ));
        let messages = store.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(message_text(&messages[1]), "first live token");
        assert_eq!(messages[1].status, ChatTurnStatus::Streaming);
        let assistant_id = messages[1].id;
        let block_id = messages[1].blocks[0].id();
        let stale_branch = store.branch_token();

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
        assert_eq!(messages[1].id, assistant_id);
        assert_eq!(messages[1].status, ChatTurnStatus::Canceled);
        assert_eq!(messages[2].role, ChatRole::System);
        assert!(message_text(&messages[2]).contains("Aborted active turn."));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);

        let (request, client_closed) = server.join();
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(
            client_closed,
            "aborting a live turn should close the in-flight SSE connection"
        );
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
        assert_eq!(message_text(&store.messages()[1]), "first live token");
    }

    #[test]
    fn deepseek_provider_posts_context_builder_request_with_tools() {
        let workspace = unique_temp_dir("live-context-request");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::write(workspace.join("note.txt"), "workspace note context\n").expect("write note");
        write_test_skill(
            &workspace,
            "rust",
            "rust-review",
            "Review Rust code before responding.",
        );
        let skill_registry =
            SkillRegistry::discover_from_paths(&[SkillSearchPath::workspace(&workspace)]);
        let skills = TestSkillState::new(skill_registry);
        assert!(skills.loaded.insert("rust-review"));

        let server = TestSseServer::spawn("data: [DONE]\n\n");
        let mut config = AgentConfig::defaults(workspace.clone());
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::Off;
        let registry = test_tool_registry();

        let store = ChatMessageStore::new();
        store.push(ChatMessage::new(
            store.next_message_id(),
            ChatRole::System,
            vec![ChatBlock::Compact(CompactBlock {
                id: ChatBlockId::new(70_001),
                status: CompactStatus::Complete,
                before_tokens: Some(2048),
                after_tokens: Some(256),
                summary: "summarized earlier conversation".to_string(),
            })],
        ));
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "Earlier request",
        ));
        store.push(ChatMessage::new(
            store.next_message_id(),
            ChatRole::Assistant,
            vec![
                ChatBlock::ToolUse(read_file_tool_call("call_read", "prior.txt")),
                ChatBlock::ToolResult(ToolResultBlock {
                    id: ChatBlockId::new(70_002),
                    call_id: "call_read".to_string(),
                    ok: true,
                    exit_code: None,
                    output: ToolOutput::Markdown("prior tool output".to_string()),
                    collapsed: false,
                }),
            ],
        ));

        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let slash_runtime = test_slash_runtime_with_skills(
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            &skills,
            &turn_budgets,
        );
        let turn_launcher = AgentTurnLauncher {
            config: config.clone(),
            action_sender: sender,
            tool_registry: registry.clone(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy: CompactPolicy::default(),
        };

        submit_input_response(
            &store,
            &slash_runtime,
            &turn_launcher,
            ChatInputResponse::Text("Use @note.txt with rust-review context".to_string()),
        );

        let request = http_request_json(&server.join());
        let messages = request["messages"]
            .as_array()
            .expect("request should contain messages");
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("system")
                && message["content"].as_str().is_some_and(|content| {
                    content.contains("<skills>")
                        && content.contains("rust-review")
                        && content.contains("Use this skill for rust-review tasks.")
                })
        }));
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("system")
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("<compact status=\"complete\""))
        }));
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("user")
                && message["content"].as_str().is_some_and(|content| {
                    content.contains("Use @note.txt")
                        && content.contains("<context_files>")
                        && content.contains("workspace note context")
                })
        }));
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("assistant")
                && message["tool_calls"]
                    .as_array()
                    .is_some_and(|calls| calls[0]["function"]["name"].as_str() == Some("read_file"))
        }));
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("tool")
                && message["tool_call_id"].as_str() == Some("call_read")
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("prior tool output"))
        }));

        let tool_names = request["tools"]
            .as_array()
            .expect("request should include tool schema")
            .iter()
            .filter_map(|tool| tool["function"]["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(tool_names.len(), registry.len());
        assert!(tool_names.contains(&"read_file"));
        assert!(tool_names.contains(&"run_command"));
        assert_eq!(request["tool_choice"].as_str(), Some("auto"));

        drop(receiver);
        fs::remove_dir_all(workspace).expect("remove workspace");
    }

    #[test]
    fn deepseek_provider_continues_after_live_tool_result() {
        let workspace = unique_temp_dir("live-tool-loop");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::write(workspace.join("note.txt"), "tool loop file context\n").expect("write note");
        let server = TestSseSequenceServer::spawn(vec![
            sse_tool_call_body(
                "call_read_note",
                "read_file",
                serde_json::json!({ "path": "note.txt" }),
            ),
            sse_final_text_body("Final answer after reading the file."),
        ]);
        let mut config = AgentConfig::defaults(workspace.clone());
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::Off;
        let registry = test_tool_registry();
        let limits = AgentTurnLimits::default();
        let turn_budgets = TurnBudgetTracker::default();
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let transcript_status = TranscriptStatusState::new();
        let turn_launcher = AgentTurnLauncher {
            config: config.clone(),
            action_sender: sender.clone(),
            tool_registry: registry.clone(),
            turn_budgets: turn_budgets.clone(),
            limits,
            compact_policy: CompactPolicy::default(),
        };
        let tool_runtime = live_tool_runtime(LiveToolRuntimeParts {
            config,
            action_sender: sender,
            registry,
            turn_budgets: turn_budgets.clone(),
            limits,
            input_handle: &input_handle,
            mock_turns: &mock_turns,
            status_state: &status_state,
            transcript_status: &transcript_status,
        });

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
            ChatInputResponse::Text("Read the note before answering.".to_string()),
        );
        apply_live_actions_until_idle(
            &receiver,
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
        );

        let requests = server.join();
        assert_eq!(requests.len(), 2);
        let second_request = http_request_json(&requests[1]);
        let second_messages = second_request["messages"]
            .as_array()
            .expect("follow-up request should contain messages");
        assert!(second_messages.iter().any(|message| {
            message["role"].as_str() == Some("assistant")
                && message["tool_calls"]
                    .as_array()
                    .is_some_and(|calls| calls[0]["id"].as_str() == Some("call_read_note"))
        }));
        assert!(second_messages.iter().any(|message| {
            message["role"].as_str() == Some("tool")
                && message["tool_call_id"].as_str() == Some("call_read_note")
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("tool loop file context"))
        }));

        let messages = store.messages();
        assert_eq!(messages.len(), 2);
        let assistant = &messages[1];
        assert_eq!(assistant.status, ChatTurnStatus::Complete);
        assert!(assistant.blocks.iter().any(|block| {
            matches!(block, ChatBlock::Text(text) if text.markdown.contains("Final answer after reading the file."))
        }));
        let tool = assistant
            .blocks
            .iter()
            .find_map(|block| match block {
                ChatBlock::ToolUse(tool) if tool.call_id == "call_read_note" => Some(tool),
                _ => None,
            })
            .expect("tool use should remain in the transcript");
        assert_eq!(tool.status, ToolStatus::Done);
        assert!(tool_result_for_call(&store, "call_read_note").ok);
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        fs::remove_dir_all(workspace).expect("remove workspace");
    }

    #[test]
    fn deepseek_provider_continues_after_denied_live_tool_call() {
        let workspace = unique_temp_dir("live-tool-loop-deny");
        fs::create_dir_all(&workspace).expect("create workspace");
        let server = TestSseSequenceServer::spawn(vec![
            sse_tool_call_body(
                "call_denied_command",
                "run_command",
                serde_json::json!({ "argv": ["/bin/echo", "should-not-run"], "cwd": "." }),
            ),
            sse_final_text_body("Final answer after the denied tool."),
        ]);
        let mut config = AgentConfig::defaults(workspace.clone());
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::Off;
        let registry = test_tool_registry();
        let limits = AgentTurnLimits::default();
        let turn_budgets = TurnBudgetTracker::default();
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let transcript_status = TranscriptStatusState::new();
        let turn_launcher = AgentTurnLauncher {
            config: config.clone(),
            action_sender: sender.clone(),
            tool_registry: registry.clone(),
            turn_budgets: turn_budgets.clone(),
            limits,
            compact_policy: CompactPolicy::default(),
        };
        let tool_runtime = live_tool_runtime(LiveToolRuntimeParts {
            config,
            action_sender: sender,
            registry,
            turn_budgets: turn_budgets.clone(),
            limits,
            input_handle: &input_handle,
            mock_turns: &mock_turns,
            status_state: &status_state,
            transcript_status: &transcript_status,
        });

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
            ChatInputResponse::Text("Try a command, but wait for approval.".to_string()),
        );
        let action = receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("tool call should be streamed before approval");
        assert!(apply_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
            action,
        ));
        assert!(input_handle.streaming_binding().get());
        let assistant_id = store.messages()[1].id;
        let tool = store
            .messages()
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                ChatBlock::ToolUse(tool) if tool.call_id == "call_denied_command" => {
                    Some(tool.clone())
                }
                _ => None,
            })
            .expect("tool approval should be pending");
        assert_eq!(tool.status, ToolStatus::Pending);
        let approval_id = tool
            .approval
            .as_ref()
            .expect("run_command should request approval")
            .id
            .clone();

        handle_tool_approval(
            &store,
            &tool_runtime,
            ApprovalDecision {
                message_id: assistant_id,
                block_id: tool.id,
                approval_id,
                option_id: "deny".to_string(),
                action: ApprovalAction::Deny,
                level: ApprovalLevel::Once,
            },
        );
        apply_live_actions_until_idle(
            &receiver,
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
        );

        let requests = server.join();
        assert_eq!(requests.len(), 2);
        let second_request = http_request_json(&requests[1]);
        let second_messages = second_request["messages"]
            .as_array()
            .expect("follow-up request should contain messages");
        assert!(second_messages.iter().any(|message| {
            message["role"].as_str() == Some("tool")
                && message["tool_call_id"].as_str() == Some("call_denied_command")
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("User denied tool call"))
        }));
        let result = tool_result_for_call(&store, "call_denied_command");
        assert!(!result.ok);
        let messages = store.messages();
        let assistant = &messages[1];
        assert_eq!(assistant.status, ChatTurnStatus::Complete);
        assert!(assistant.blocks.iter().any(|block| {
            matches!(block, ChatBlock::Text(text) if text.markdown.contains("Final answer after the denied tool."))
        }));
        fs::remove_dir_all(workspace).expect("remove workspace");
    }

    #[test]
    fn deepseek_provider_stops_tool_loop_at_model_request_budget() {
        let workspace = unique_temp_dir("live-tool-loop-budget");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::write(workspace.join("note.txt"), "budget file context\n").expect("write note");
        let server = TestSseServer::spawn(sse_tool_call_body(
            "call_read_budget",
            "read_file",
            serde_json::json!({ "path": "note.txt" }),
        ));
        let mut config = AgentConfig::defaults(workspace.clone());
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::Off;
        let registry = test_tool_registry();
        let limits = AgentTurnLimits::new(1, 16, Duration::from_secs(30));
        let turn_budgets = TurnBudgetTracker::default();
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let transcript_status = TranscriptStatusState::new();
        let turn_launcher = AgentTurnLauncher {
            config: config.clone(),
            action_sender: sender.clone(),
            tool_registry: registry.clone(),
            turn_budgets: turn_budgets.clone(),
            limits,
            compact_policy: CompactPolicy::default(),
        };
        let tool_runtime = live_tool_runtime(LiveToolRuntimeParts {
            config,
            action_sender: sender,
            registry,
            turn_budgets: turn_budgets.clone(),
            limits,
            input_handle: &input_handle,
            mock_turns: &mock_turns,
            status_state: &status_state,
            transcript_status: &transcript_status,
        });

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
            ChatInputResponse::Text("Read the note within a tiny budget.".to_string()),
        );
        apply_live_actions_until_idle(
            &receiver,
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
        );

        let first_request = server.join();
        assert!(first_request.contains("Read the note within a tiny budget."));
        let messages = store.messages();
        let assistant = &messages[1];
        let ChatTurnStatus::Failed(error) = &assistant.status else {
            panic!("expected failed assistant turn, got {:?}", assistant.status);
        };
        assert_eq!(error.kind, ChatErrorKind::Other);
        assert!(error.message.contains("model request limit"));
        assert!(tool_result_for_call(&store, "call_read_budget").ok);
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        fs::remove_dir_all(workspace).expect("remove workspace");
    }

    #[test]
    fn deepseek_provider_plan_turn_posts_submit_plan_context_request() {
        let workspace = unique_temp_dir("live-plan-request");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::write(workspace.join("plan.txt"), "plan context\n").expect("write note");
        let server = TestSseServer::spawn("data: [DONE]\n\n");
        let mut config = AgentConfig::defaults(workspace.clone());
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::On;

        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::On.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let turn_launcher = AgentTurnLauncher {
            config,
            action_sender: sender,
            tool_registry: test_tool_registry(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy: CompactPolicy::default(),
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
            ChatInputResponse::Text("Update @plan.txt and run tests".to_string()),
        );

        let request = http_request_json(&server.join());
        let messages = request["messages"]
            .as_array()
            .expect("request should contain messages");
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("system")
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("You are in plan mode"))
        }));
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("user")
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("plan context"))
        }));
        let tools = request["tools"]
            .as_array()
            .expect("plan request should include virtual submit_plan tool");
        assert_eq!(tools.len(), 1);
        assert_eq!(
            tools[0]["function"]["name"].as_str(),
            Some(crate::plan::SUBMIT_PLAN_TOOL_NAME)
        );
        assert_eq!(
            request["tool_choice"]["function"]["name"].as_str(),
            Some(crate::plan::SUBMIT_PLAN_TOOL_NAME)
        );
        drop(receiver);
        fs::remove_dir_all(workspace).expect("remove workspace");
    }

    #[test]
    fn deepseek_provider_accepted_plan_continue_posts_transcript_request_with_tools() {
        let server = TestSseServer::spawn("data: [DONE]\n\n");
        let mut config = AgentConfig::defaults(".");
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::On;

        let store = ChatMessageStore::new();
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "Please update the implementation.",
        ));
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::with_token_delay(Duration::from_millis(1));
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let turn_budgets = TurnBudgetTracker::default();
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let registry = test_tool_registry();
        let runtime = PlanDecisionRuntime {
            input_handle: input_handle.clone(),
            mock_turns: mock_turns.clone(),
            status_state: status_state.clone(),
            skill_registry: SkillRegistry::default(),
            loaded_skills: LoadedSkillSet::default(),
            transcript_status: TranscriptStatusState::new(),
            turn_launcher: AgentTurnLauncher {
                config,
                action_sender: sender,
                tool_registry: registry.clone(),
                turn_budgets: turn_budgets.clone(),
                limits: AgentTurnLimits::default(),
                compact_policy: CompactPolicy::default(),
            },
        };
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let plan_block_id = ChatBlockId::new(70_003);
        store.push(
            ChatMessage::new(
                assistant_id,
                ChatRole::Assistant,
                vec![ChatBlock::Plan(PlanBlock {
                    id: plan_block_id,
                    items: vec![PlanItem {
                        text: "Inspect and edit.".to_string(),
                    }],
                    decision: PlanDecision::Pending,
                })],
            )
            .with_status(ChatTurnStatus::Streaming),
        );
        turn_budgets.start_turn(assistant_id, AgentTurnLimits::default());
        let _plan_cancel = mock_turns.start(assistant_id);

        handle_plan_decision(
            &store,
            &runtime,
            PlanDecisionEvent {
                message_id: assistant_id,
                block_id: plan_block_id,
                decision: PlanDecision::Accepted,
            },
        );

        let request = http_request_json(&server.join());
        let messages = request["messages"]
            .as_array()
            .expect("request should contain messages");
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("user")
                && message["content"].as_str() == Some("Please update the implementation.")
        }));
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("system")
                && message["content"].as_str() == Some(ACCEPTED_PLAN_EXECUTION_INSTRUCTION)
        }));
        let tool_names = request["tools"]
            .as_array()
            .expect("accepted-plan execution should include registered tools")
            .iter()
            .filter_map(|tool| tool["function"]["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(tool_names.len(), registry.len());
        assert!(tool_names.contains(&"apply_patch"));
        assert!(tool_names.contains(&"run_command"));
        assert_eq!(request["tool_choice"].as_str(), Some("auto"));
        assert!(!tool_names.contains(&crate::plan::SUBMIT_PLAN_TOOL_NAME));

        drop(receiver);
    }

    #[test]
    fn text_submit_compacts_older_transcript_before_starting_turn() {
        let store = ChatMessageStore::new();
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "old user zero full body should be summarized",
        ));
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::Assistant,
            "old assistant zero full body should be summarized",
        ));
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "old user one full body should be summarized",
        ));
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::Assistant,
            "recent assistant keep",
        ));
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let turn_launcher = AgentTurnLauncher {
            config: AgentConfig::defaults("."),
            action_sender: sender,
            tool_registry: test_tool_registry(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy: CompactPolicy {
                threshold_tokens: 1,
                recent_message_limit: 2,
                summary_max_bytes: 4096,
            },
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
            ChatInputResponse::Text("current prompt keep".to_string()),
        );

        let messages = store.messages();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, ChatRole::System);
        match &messages[0].blocks[0] {
            ChatBlock::Compact(compact) => {
                assert_eq!(compact.status, CompactStatus::Complete);
                assert!(compact.before_tokens.is_some());
                assert!(compact.after_tokens.is_some());
                assert!(compact.summary.contains("old user zero full body"));
                assert!(compact.summary.contains("old assistant zero full body"));
                assert!(compact.summary.contains("old user one full body"));
            }
            other => panic!("expected compact block, got {other:?}"),
        }
        assert_eq!(message_text(&messages[1]), "recent assistant keep");
        assert_eq!(messages[2].role, ChatRole::User);
        assert_eq!(message_text(&messages[2]), "current prompt keep");
        assert_eq!(messages[3].role, ChatRole::Assistant);
        assert_eq!(messages[3].status, ChatTurnStatus::Streaming);

        let request = deepseek_request_from_transcript(
            &AgentConfig::defaults("."),
            &test_tool_registry(),
            &messages,
        );
        assert_eq!(request.messages.len(), 3);
        assert_eq!(request.messages[0].role, ChatMessageRole::System);
        assert!(
            request.messages[0]
                .content
                .as_deref()
                .is_some_and(|content| content.starts_with("<compact status=\"complete\""))
        );
        assert_eq!(request.messages[1].role, ChatMessageRole::Assistant);
        assert_eq!(
            request.messages[1].content.as_deref(),
            Some("recent assistant keep")
        );
        assert_eq!(request.messages[2].role, ChatMessageRole::User);
        assert_eq!(
            request.messages[2].content.as_deref(),
            Some("current prompt keep")
        );
        assert!(!request.messages.iter().any(|message| {
            message.role == ChatMessageRole::User
                && message
                    .content
                    .as_deref()
                    .is_some_and(|content| content.contains("old user zero full body"))
        }));
        drop(receiver);
    }

    #[test]
    fn text_submit_auto_loads_matching_auto_skills() {
        let workspace = unique_temp_dir("submit-auto-skills");
        write_test_skill_with_mode(
            &workspace,
            "rust",
            "rust-review",
            "Review Rust code.",
            SkillMode::Auto,
            &["clippy"],
        );
        write_test_skill_with_mode(
            &workspace,
            "docs",
            "docs",
            "Write documentation.",
            SkillMode::Manual,
            &["docs"],
        );
        let registry =
            SkillRegistry::discover_from_paths(&[SkillSearchPath::workspace(&workspace)]);
        let skills = TestSkillState::new(registry);
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let runtime = test_slash_runtime_with_skills(
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            &skills,
            &turn_budgets,
        );
        let turn_launcher = AgentTurnLauncher {
            config: AgentConfig::defaults("."),
            action_sender: sender,
            tool_registry: test_tool_registry(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy: CompactPolicy::default(),
        };

        submit_input_response(
            &store,
            &runtime,
            &turn_launcher,
            ChatInputResponse::Text("please run clippy on this rust code".to_string()),
        );

        assert_eq!(skills.loaded.names(), vec!["rust-review"]);
        assert_eq!(skills.count_state.get(), "skills: 1");
        let messages = store.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(messages[1].role, ChatRole::Assistant);
        drop(receiver);
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn edit_and_resubmit_appends_edited_user_and_restarts_turn() {
        let store = ChatMessageStore::new();
        let user_id = store.next_message_id();
        store.push(ChatMessage::text(user_id, ChatRole::User, "old prompt"));
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::Assistant,
            "old answer",
        ));
        let removed_messages = store
            .truncate_from(user_id)
            .expect("edit controller should have truncated from the user message");
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::with_token_delay(Duration::from_millis(1));
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let turn_budgets = TurnBudgetTracker::default();
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let runtime = test_slash_runtime(
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            &turn_budgets,
        );
        let turn_launcher = test_turn_launcher(sender, &turn_budgets, CompactPolicy::default());

        handle_edit_and_resubmit(
            &store,
            &runtime,
            &turn_launcher,
            EditAndResubmitEvent {
                message_id: user_id,
                original_text: "old prompt".to_string(),
                edited_text: "edited prompt".to_string(),
                removed_messages,
            },
        );

        let messages = store.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(message_text(&messages[0]), "edited prompt");
        assert_eq!(messages[1].role, ChatRole::Assistant);
        assert_eq!(messages[1].status, ChatTurnStatus::Streaming);
        assert!(input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_STREAMING);
        let action = receiver
            .recv_timeout(Duration::from_millis(250))
            .expect("resubmitted edit should start the mock turn");
        assert!(matches!(action, AppAction::TextDelta { .. }));
    }

    #[test]
    fn retry_and_regenerate_restart_from_retained_user_prompt_and_reject_late_tokens() {
        for kind in [MessageActionKind::Retry, MessageActionKind::Regenerate] {
            let store = ChatMessageStore::new();
            store.push(ChatMessage::text(
                store.next_message_id(),
                ChatRole::User,
                "retry prompt",
            ));
            let assistant_id = store.next_message_id();
            let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "old")
                .with_status(ChatTurnStatus::Streaming);
            let old_text_block_id = assistant.blocks[0].id();
            store.push(assistant);
            let stale_branch = store.branch_token();
            let input_handle = atto_ui_chat::ChatInputHandle::new();
            let mock_turns = MockTurnRegistry::with_token_delay(Duration::from_millis(1));
            let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
            let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
            let turn_budgets = TurnBudgetTracker::default();
            input_handle.streaming_binding().set(true);
            turn_budgets.start_turn(assistant_id, AgentTurnLimits::default());
            let old_cancel = mock_turns.start(assistant_id);
            let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
            let runtime = test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            );
            let turn_launcher = test_turn_launcher(sender, &turn_budgets, CompactPolicy::default());
            assert!(store.truncate_from(assistant_id).is_some());

            handle_message_action(
                &store,
                &runtime,
                &turn_launcher,
                MessageAction {
                    message_id: assistant_id,
                    kind,
                },
            );

            assert!(old_cancel.is_cancelled());
            let messages = store.messages();
            assert_eq!(messages.len(), 2);
            assert_eq!(messages[0].role, ChatRole::User);
            assert_eq!(message_text(&messages[0]), "retry prompt");
            assert_eq!(messages[1].role, ChatRole::Assistant);
            assert_eq!(messages[1].status, ChatTurnStatus::Streaming);
            assert!(input_handle.streaming_binding().get());
            assert_eq!(status_state.get(), STATUS_STREAMING);
            assert!(!apply_test_app_action(
                &store,
                &input_handle,
                &mock_turns,
                &status_state,
                AppAction::TextDelta {
                    branch: stale_branch,
                    block_id: old_text_block_id,
                    delta: "late".to_string(),
                },
            ));

            let _first = receiver
                .recv_timeout(Duration::from_millis(250))
                .expect("retry should start streaming");
            let second = receiver
                .recv_timeout(Duration::from_millis(250))
                .expect("retry should stream the retained prompt");
            match second {
                AppAction::TextDelta { delta, .. } => assert_eq!(delta, "retry prompt"),
                other => panic!("expected prompt text delta, got {other:?}"),
            }
        }
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
    fn plan_decision_callback_updates_pending_plan_block() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::with_token_delay(Duration::from_millis(1));
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let turn_budgets = TurnBudgetTracker::default();
        let (runtime, _receiver) =
            test_plan_decision_runtime(&input_handle, &mock_turns, &status_state, &turn_budgets);
        let assistant_id = store.next_message_id();
        let plan_block_id = ChatBlockId::new(30_001);
        store.push(ChatMessage::new(
            assistant_id,
            ChatRole::Assistant,
            vec![ChatBlock::Plan(PlanBlock {
                id: plan_block_id,
                items: vec![PlanItem {
                    text: "Inspect current implementation.".to_string(),
                }],
                decision: PlanDecision::Pending,
            })],
        ));

        handle_plan_decision(
            &store,
            &runtime,
            PlanDecisionEvent {
                message_id: assistant_id,
                block_id: plan_block_id,
                decision: PlanDecision::Accepted,
            },
        );

        assert_eq!(plan_decision(&store, plan_block_id), PlanDecision::Accepted);

        handle_plan_decision(
            &store,
            &runtime,
            PlanDecisionEvent {
                message_id: assistant_id,
                block_id: plan_block_id,
                decision: PlanDecision::Rejected,
            },
        );

        assert_eq!(plan_decision(&store, plan_block_id), PlanDecision::Accepted);
    }

    #[test]
    fn accepting_plan_appends_internal_instruction_and_starts_execution_turn() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::with_token_delay(Duration::from_millis(1));
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let turn_budgets = TurnBudgetTracker::default();
        input_handle.streaming_binding().set(true);
        let (runtime, receiver) =
            test_plan_decision_runtime(&input_handle, &mock_turns, &status_state, &turn_budgets);
        let assistant_id = store.next_message_id();
        let plan_block_id = ChatBlockId::new(30_002);
        store.push(
            ChatMessage::new(
                assistant_id,
                ChatRole::Assistant,
                vec![ChatBlock::Plan(PlanBlock {
                    id: plan_block_id,
                    items: vec![PlanItem {
                        text: "Inspect current implementation.".to_string(),
                    }],
                    decision: PlanDecision::Pending,
                })],
            )
            .with_status(ChatTurnStatus::Streaming),
        );
        turn_budgets.start_turn(assistant_id, AgentTurnLimits::default());
        let plan_cancel = mock_turns.start(assistant_id);

        handle_plan_decision(
            &store,
            &runtime,
            PlanDecisionEvent {
                message_id: assistant_id,
                block_id: plan_block_id,
                decision: PlanDecision::Accepted,
            },
        );

        let messages = store.messages();
        assert_eq!(plan_decision(&store, plan_block_id), PlanDecision::Accepted);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].status, ChatTurnStatus::Complete);
        assert!(plan_cancel.is_cancelled());
        assert_eq!(messages[1].role, ChatRole::System);
        assert_eq!(
            message_text(&messages[1]),
            ACCEPTED_PLAN_EXECUTION_INSTRUCTION
        );
        assert_eq!(messages[2].role, ChatRole::Assistant);
        assert_eq!(messages[2].status, ChatTurnStatus::Streaming);
        assert!(input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_STREAMING);
        let action = receiver
            .recv_timeout(Duration::from_millis(250))
            .expect("accepted plan should continue the mock execution loop");
        assert!(matches!(action, AppAction::TextDelta { .. }));
    }

    #[test]
    fn rejecting_plan_stops_turn_without_starting_execution() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::with_token_delay(Duration::from_millis(1));
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let turn_budgets = TurnBudgetTracker::default();
        input_handle.streaming_binding().set(true);
        let (runtime, receiver) =
            test_plan_decision_runtime(&input_handle, &mock_turns, &status_state, &turn_budgets);
        let assistant_id = store.next_message_id();
        let plan_block_id = ChatBlockId::new(30_003);
        store.push(
            ChatMessage::new(
                assistant_id,
                ChatRole::Assistant,
                vec![ChatBlock::Plan(PlanBlock {
                    id: plan_block_id,
                    items: vec![PlanItem {
                        text: "Inspect current implementation.".to_string(),
                    }],
                    decision: PlanDecision::Pending,
                })],
            )
            .with_status(ChatTurnStatus::Streaming),
        );
        turn_budgets.start_turn(assistant_id, AgentTurnLimits::default());
        let plan_cancel = mock_turns.start(assistant_id);

        handle_plan_decision(
            &store,
            &runtime,
            PlanDecisionEvent {
                message_id: assistant_id,
                block_id: plan_block_id,
                decision: PlanDecision::Rejected,
            },
        );

        let messages = store.messages();
        assert_eq!(plan_decision(&store, plan_block_id), PlanDecision::Rejected);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].status, ChatTurnStatus::Complete);
        assert!(plan_cancel.is_cancelled());
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert!(receiver.try_recv().is_err());
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
        let transcript_status = TranscriptStatusState::new();
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
            &transcript_status,
            &tool_runtime,
            AppAction::ToolCallsReady {
                branch,
                message_id: assistant_id,
                tool_calls: vec![
                    read_file_tool_call("call_1", "a.txt"),
                    read_file_tool_call("call_2", "b.txt"),
                ],
                mutating_tools_allowed: true,
                continue_after_tools: false,
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
        assert!(
            transcript_status
                .error_summary_state
                .get()
                .contains("err:tool")
        );
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
    fn plan_gate_blocks_mutating_tool_even_with_project_grant() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let transcript_status = TranscriptStatusState::new();
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();
        permissions
            .lock()
            .expect("tool permission policy lock poisoned")
            .allow_for_project("run_command");
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime = test_tool_runtime(
            AgentConfig::defaults("."),
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
            &transcript_status,
            &tool_runtime,
            AppAction::ToolCallsReady {
                branch,
                message_id: assistant_id,
                tool_calls: vec![run_command_tool_call("call_plan_gate")],
                mutating_tools_allowed: false,
                continue_after_tools: false,
            },
        ));
        let block_id = store
            .messages()
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                ChatBlock::ToolUse(tool) if tool.call_id == "call_plan_gate" => Some(tool.id),
                _ => None,
            })
            .expect("blocked tool use should be appended");

        let tool = tool_use_for_block(&store, block_id);
        let result = tool_result_for_call(&store, "call_plan_gate");
        assert_eq!(tool.status, ToolStatus::Canceled);
        assert!(tool.approval.is_none());
        assert!(!result.ok);
        assert_eq!(result.exit_code, None);
        match result.output {
            ToolOutput::Markdown(output) => {
                assert_eq!(output, PLAN_MODE_MUTATING_TOOL_BLOCKED_RESULT);
            }
            other => panic!("expected markdown plan-gate result, got {other:?}"),
        }
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
        let transcript_status = TranscriptStatusState::new();
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
            &transcript_status,
            &tool_runtime,
            AppAction::ToolCallsReady {
                branch,
                message_id: assistant_id,
                tool_calls: vec![read_file_tool_call("call_read", "fixture.txt")],
                mutating_tools_allowed: true,
                continue_after_tools: false,
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
            &transcript_status,
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
    fn deepseek_request_from_transcript_injects_file_mentions_from_config_workspace() {
        let workspace = unique_temp_dir("request-file-mentions");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::write(workspace.join("note.txt"), "workspace context\n").expect("write fixture");
        let config = AgentConfig::defaults(workspace.clone());
        let registry = test_tool_registry();

        let request = deepseek_request_from_transcript(
            &config,
            &registry,
            &[ChatMessage::text(1, ChatRole::User, "Use @note.txt")],
        );

        let content = request.messages[0]
            .content
            .as_deref()
            .expect("user message should contain text");
        assert!(content.contains("<context_files>"));
        assert!(content.contains("<file path=\"note.txt\""));
        assert!(content.contains("workspace context"));

        fs::remove_dir_all(workspace).expect("remove fixture workspace");
    }

    #[test]
    fn deepseek_plan_request_forces_submit_plan_virtual_tool() {
        let request = deepseek_plan_request_from_transcript(
            &AgentConfig::defaults("."),
            &[ChatMessage::text(
                1,
                ChatRole::User,
                "Please update README and run tests.",
            )],
        );

        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, ChatMessageRole::System);
        assert!(
            request.messages[0]
                .content
                .as_deref()
                .is_some_and(|content| content.contains("You are in plan mode"))
        );
        assert_eq!(request.messages[1].role, ChatMessageRole::User);
        assert_eq!(request.tools.len(), 1);
        assert_eq!(
            request.tools[0].function.name,
            crate::plan::SUBMIT_PLAN_TOOL_NAME
        );
        assert_eq!(
            request.tool_choice,
            Some(ToolChoice::Function(
                crate::deepseek::ToolChoiceFunction::named(crate::plan::SUBMIT_PLAN_TOOL_NAME,)
            ))
        );
    }

    #[test]
    fn deepseek_stream_submit_plan_tool_call_writes_pending_plan_block() {
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
        let mut stream = DeepSeekUiStream::new_with_plan_requirement(
            branch,
            assistant_id,
            text_block_id,
            "deepseek-chat",
            true,
        );
        let events = vec![
            ChatCompletionSseEvent::Chunk(ChatCompletionChunk {
                id: None,
                object: None,
                created: None,
                model: None,
                choices: vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: ChatCompletionDelta {
                        tool_calls: vec![tool_call_delta(
                            0,
                            Some("call_plan"),
                            Some(crate::plan::SUBMIT_PLAN_TOOL_NAME),
                            Some(
                                r#"{"items":["Inspect the current implementation.","Add submit_plan mapping.","Run validation."]}"#,
                            ),
                        )],
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
        assert_eq!(assistant.meta.stop_reason, Some(StopReason::EndTurn));
        assert_eq!(message_text(assistant), "");
        match &assistant.blocks[1] {
            ChatBlock::Plan(plan) => {
                assert_eq!(plan.decision, PlanDecision::Pending);
                assert_eq!(
                    plan.items
                        .iter()
                        .map(|item| item.text.as_str())
                        .collect::<Vec<_>>(),
                    vec![
                        "Inspect the current implementation.",
                        "Add submit_plan mapping.",
                        "Run validation."
                    ]
                );
            }
            other => panic!("expected plan block, got {other:?}"),
        }
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert!(!mock_turns.cancel(assistant_id));
    }

    #[test]
    fn deepseek_stream_plan_turn_mutating_tool_call_writes_blocked_tool_result() {
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
        let mut stream = DeepSeekUiStream::new_with_plan_requirement(
            branch,
            assistant_id,
            text_block_id,
            "deepseek-chat",
            true,
        );
        let events = vec![
            ChatCompletionSseEvent::Chunk(ChatCompletionChunk {
                id: None,
                object: None,
                created: None,
                model: None,
                choices: vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: ChatCompletionDelta {
                        tool_calls: vec![tool_call_delta(
                            0,
                            Some("call_blocked_run"),
                            Some("run_command"),
                            Some(r#"{"argv":["/bin/echo","blocked"],"cwd":"."}"#),
                        )],
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
        let tool = assistant
            .blocks
            .iter()
            .find_map(|block| match block {
                ChatBlock::ToolUse(tool) => Some(tool),
                _ => None,
            })
            .expect("blocked tool use should be appended");
        assert_eq!(tool.call_id, "call_blocked_run");
        assert_eq!(tool.name, "run_command");
        assert_eq!(tool.status, ToolStatus::Canceled);
        assert!(tool.approval.is_none());
        let result = tool_result_for_call(&store, "call_blocked_run");
        assert!(!result.ok);
        match result.output {
            ToolOutput::Markdown(output) => {
                assert_eq!(output, PLAN_MODE_MUTATING_TOOL_BLOCKED_RESULT);
            }
            other => panic!("expected markdown plan-gate result, got {other:?}"),
        }
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }

    #[test]
    fn deepseek_stream_markdown_plan_fallback_writes_pending_plan_block() {
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
        let mut stream = DeepSeekUiStream::new_with_plan_requirement(
            branch,
            assistant_id,
            text_block_id,
            "deepseek-chat",
            true,
        );
        let events = parse_chat_completion_sse(concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Plan:\\n1. Inspect current state.\\n\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"2. Implement the plan parser.\\n- [ ] Run validation.\\n\"},\"finish_reason\":\"stop\"}]}\n\n",
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
        assert_eq!(message_text(assistant), "");
        match &assistant.blocks[1] {
            ChatBlock::Plan(plan) => {
                assert_eq!(plan.decision, PlanDecision::Pending);
                assert_eq!(
                    plan.items
                        .iter()
                        .map(|item| item.text.as_str())
                        .collect::<Vec<_>>(),
                    vec![
                        "Inspect current state.",
                        "Implement the plan parser.",
                        "Run validation."
                    ]
                );
            }
            other => panic!("expected plan block, got {other:?}"),
        }
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }

    #[test]
    fn deepseek_request_from_transcript_injects_loaded_skills() {
        let (workspace, skill_registry) = test_skill_registry(&[
            ("rust", "rust-review", "Review Rust code."),
            ("docs", "docs", "Write documentation."),
        ]);
        let loaded = LoadedSkillSet::default();
        assert!(loaded.insert("rust-review"));
        let registry = test_tool_registry();

        let request = deepseek_request_from_transcript_with_skills(
            &AgentConfig::defaults("."),
            &registry,
            &skill_registry,
            &loaded,
            &[ChatMessage::text(1, ChatRole::User, "Please review this.")],
        );

        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, ChatMessageRole::System);
        let skill_prompt = request.messages[0]
            .content
            .as_deref()
            .expect("skill system prompt should have content");
        assert!(skill_prompt.starts_with("<skills>\n"));
        assert!(skill_prompt.contains("<skill name=\"rust-review\" source=\""));
        assert!(skill_prompt.contains("Use this skill for rust-review tasks."));
        assert!(!skill_prompt.contains("Use this skill for docs tasks."));
        assert!(skill_prompt.ends_with("</skills>"));
        assert_eq!(request.messages[1].role, ChatMessageRole::User);
        assert_eq!(request.tools.len(), registry.len());

        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn skill_tool_preferences_do_not_grant_mutating_tool_approval() {
        let workspace = unique_temp_dir("skill-tool-permissions");
        write_test_skill_with_mode_and_tools(
            &workspace,
            "shell",
            "shell-helper",
            "Prefer shell diagnostics.",
            SkillMode::Manual,
            &[],
            &["run_command"],
        );
        let skill_registry =
            SkillRegistry::discover_from_paths(&[SkillSearchPath::workspace(&workspace)]);
        let loaded = LoadedSkillSet::default();
        assert!(loaded.insert("shell-helper"));
        let registry = test_tool_registry();

        let request = deepseek_request_from_transcript_with_skills(
            &AgentConfig::defaults("."),
            &registry,
            &skill_registry,
            &loaded,
            &[ChatMessage::text(1, ChatRole::User, "Run diagnostics.")],
        );

        let skill_prompt = request.messages[0]
            .content
            .as_deref()
            .expect("skill prompt should be injected");
        assert!(skill_prompt.contains("tools=\"run_command\""));
        assert_eq!(request.tools.len(), registry.len());
        assert_eq!(
            registry
                .spec("run_command")
                .expect("run_command tool should be registered")
                .permission,
            ToolPermission::ApproveForProject
        );

        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let permissions = test_tool_permissions();
        let block_id = append_tool_call_with_runtime(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &registry,
            &permissions,
            run_command_tool_call("call_skill_run"),
        );

        let tool = tool_use_for_block(&store, block_id);
        assert_eq!(tool.status, ToolStatus::Pending);
        assert!(tool.approval.is_some());
        assert!(
            !permissions
                .lock()
                .expect("tool permission policy lock poisoned")
                .is_project_allowed("run_command")
        );

        let _ = fs::remove_dir_all(workspace);
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
