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
    StatusSegment, StatusSegmentAlign, popup_menu_window, run_crossterm_desktop_with_actions,
};
use atto_ui::reactive::{Binding, DirtyObserver, EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowId, WindowKind};
use atto_ui_chat::{
    ApprovalAction, ApprovalDecision, ApprovalLevel, ApprovalOption, ApprovalRequest, ChatBlock,
    ChatBlockId, ChatBranchToken, ChatContextMenuRequest, ChatError, ChatErrorKind,
    ChatInputHandle, ChatInputResponse, ChatMessage, ChatMessageId, ChatMessageList,
    ChatMessageMeta, ChatMessageStore, ChatPanel, ChatRole, ChatSlashCommand, ChatTurnStatus,
    DiffData, EditAndResubmitEvent, MessageAction, MessageActionKind, PlanBlock, PlanDecision,
    PlanDecisionEvent, PlanItem, TextBlock, ThinkingBlock, ToolInput, ToolOutput, ToolResultBlock,
    ToolStatus, ToolUseBlock,
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

mod chat;
mod dispatch;
mod menu_status;
mod runtime;
mod slash;
mod tools;
mod transcript_sync;
mod turns;

// Flatten the submodule items into the crate root so cross-module references
// (e.g. AgentRuntime, apply_app_action, spawn_mock_agent_turn) resolve
// exactly as before the split. All moved items are pub(crate); the public
// deepseek_* request builders and AgentApp stay defined here.
pub(crate) use chat::*;
pub(crate) use dispatch::*;
pub(crate) use menu_status::*;
pub(crate) use runtime::*;
pub(crate) use slash::*;
pub(crate) use tools::*;
pub(crate) use transcript_sync::*;
pub(crate) use turns::*;

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
const MISSING_DEEPSEEK_API_KEY_NOTICE: &str = "DeepSeek API key is not configured; using the mock provider. Set DEEPSEEK_API_KEY or pass --api-key to use live DeepSeek, or pass --mock to explicitly stay on mock.";
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
            runtime.context_menus.clone(),
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
    let mut config = AgentConfig::defaults(env!("CARGO_MANIFEST_DIR"));
    config.force_mock = true;
    run_with_config_mock_token_delay_and_compact_policy(
        config,
        SNAPSHOT_MOCK_TOKEN_DELAY,
        SNAPSHOT_COMPACT_POLICY,
    )
}

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

#[cfg(test)]
mod tests;
