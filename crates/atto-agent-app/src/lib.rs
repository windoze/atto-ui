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
use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, MenuItem, MenuSpec,
    StatusSegment, StatusSegmentAlign, run_crossterm_desktop_with_actions,
};
use atto_ui::reactive::{Binding, EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowId, WindowKind};
use atto_ui_chat::{
    ChatBlock, ChatBlockId, ChatBranchToken, ChatError, ChatInputHandle, ChatInputResponse,
    ChatMessage, ChatMessageId, ChatMessageList, ChatMessageMeta, ChatMessageStore, ChatPanel,
    ChatRole, ChatSlashCommand, ChatTurnStatus, ThinkingBlock, ToolUseBlock,
};
use ratatui::layout::Rect;

pub mod config;
pub mod deepseek;
pub mod deepseek_client;
mod stream_ui;
pub mod tool;

use crate::config::{AgentConfig, PlanMode};
use crate::deepseek::{
    ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionDelta, ChatCompletionSseEvent,
    FinishReason,
};
use crate::stream_ui::DeepSeekUiStream;

pub const APP_TITLE: &str = "Atto Agent";
const CHAT_WINDOW_TAG: &str = "atto-agent:chat";
const STATUS_READY: &str = "ready";
const STATUS_STREAMING: &str = "streaming";
const MOCK_TOKEN_DELAY: Duration = Duration::from_millis(24);
const SNAPSHOT_MOCK_TOKEN_DELAY: Duration = Duration::from_millis(96);

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

#[derive(Clone, Debug)]
struct AgentTurnLauncher {
    model: String,
    action_sender: mpsc::Sender<AppAction>,
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
    message_store: ChatMessageStore,
    input_handle: ChatInputHandle,
    status_state: Property<String>,
    model_state: Property<String>,
    plan_mode_state: Property<String>,
}

impl AgentRuntime {
    fn new(
        config: AgentConfig,
        action_sender: mpsc::Sender<AppAction>,
        mock_turns: MockTurnRegistry,
    ) -> Self {
        let model_state = Property::new(format!("model: {}", config.model));
        let plan_mode_state = Property::new(config.plan_mode.status());
        Self {
            config,
            action_sender,
            mock_turns,
            message_store: ChatMessageStore::new(),
            input_handle: ChatInputHandle::new(),
            status_state: Property::new(STATUS_READY.to_string()),
            model_state,
            plan_mode_state,
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
            &runtime.input_handle,
            runtime.config.model.clone(),
            runtime.status_state.clone(),
            runtime.plan_mode_state.clone(),
            runtime.action_sender.clone(),
            runtime.mock_turns.clone(),
        );

        let mut desktop = Desktop::new(Theme::dark(), agent_menu(quit_events));
        desktop.status.set_segments(status_segments(
            runtime.model_state.binding(),
            runtime.status_state.binding(),
            runtime.plan_mode_state.binding(),
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
    run_with_config_and_mock_token_delay(AgentConfig::defaults("."), SNAPSHOT_MOCK_TOKEN_DELAY)
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
            apply_app_action(
                &runtime_for_actions.message_store,
                &runtime_for_actions.input_handle,
                &runtime_for_actions.mock_turns,
                &runtime_for_actions.status_state,
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
    input_handle: &ChatInputHandle,
    model: String,
    status_state: Property<String>,
    plan_mode_state: Property<String>,
    action_sender: mpsc::Sender<AppAction>,
    mock_turns: MockTurnRegistry,
) -> ChatPanel {
    // Compose the reusable chat list and input controls around shared state handles.
    let input_handle_for_cancel = input_handle.clone();
    let status_for_cancel = status_state.clone();
    let mock_turns_for_cancel = mock_turns.clone();
    let list = ChatMessageList::new(store.clone())
        .show_timestamps(false)
        .on_cancel(move |message_id| {
            finish_canceled_turn(
                &input_handle_for_cancel,
                &mock_turns_for_cancel,
                &status_for_cancel,
                message_id,
            );
        });
    let store_for_submit = store.clone();
    let input_handle_for_submit = input_handle.clone();
    let mock_turns_for_submit = mock_turns.clone();
    let status_for_submit = status_state.clone();
    let plan_mode_for_submit = plan_mode_state.clone();
    let turn_launcher_for_submit = AgentTurnLauncher {
        model,
        action_sender,
    };
    let store_for_slash = store.clone();
    let input_handle_for_slash = input_handle.clone();
    let mock_turns_for_slash = mock_turns.clone();
    let status_for_slash = status_state.clone();
    let plan_mode_for_slash = plan_mode_state.clone();
    input_handle.set_slash_commands(agent_slash_commands());
    let input = input_handle
        .panel()
        .on_submit(move |response| {
            submit_input_response(
                &store_for_submit,
                &input_handle_for_submit,
                &mock_turns_for_submit,
                &status_for_submit,
                &plan_mode_for_submit,
                &turn_launcher_for_submit,
                response,
            );
        })
        .on_slash_command(move |command| {
            let _ = submit_slash_command_text(
                &store_for_slash,
                &input_handle_for_slash,
                &mock_turns_for_slash,
                &status_for_slash,
                &plan_mode_for_slash,
                &command.replacement,
            );
        });
    ChatPanel::new(list, input)
}

fn submit_input_response(
    store: &ChatMessageStore,
    input_handle: &ChatInputHandle,
    mock_turns: &MockTurnRegistry,
    status_state: &Property<String>,
    plan_mode_state: &Property<String>,
    turn_launcher: &AgentTurnLauncher,
    response: ChatInputResponse,
) {
    let text = input_response_text(response);
    if text.trim().is_empty() {
        return;
    }

    if submit_slash_command_text(
        store,
        input_handle,
        mock_turns,
        status_state,
        plan_mode_state,
        &text,
    ) {
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
    let cancel = mock_turns.start(assistant_id);

    input_handle.streaming_binding().set(true);
    status_state.set(STATUS_STREAMING.to_string());
    spawn_mock_agent_turn(
        turn_launcher.action_sender.clone(),
        MockAgentTurnRequest {
            branch,
            message_id: assistant_id,
            block_id: text_block_id,
            cancel,
            token_delay: mock_turns.token_delay(),
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
        ChatSlashCommand::new("/tools")
            .detail("List available tools")
            .submit_on_accept(),
        ChatSlashCommand::new("/abort")
            .detail("Cancel the active mock turn")
            .submit_on_accept(),
    ]
}

fn submit_slash_command_text(
    store: &ChatMessageStore,
    input_handle: &ChatInputHandle,
    mock_turns: &MockTurnRegistry,
    status_state: &Property<String>,
    plan_mode_state: &Property<String>,
    text: &str,
) -> bool {
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
        "clear" => clear_session(store, input_handle, mock_turns, status_state),
        "plan" => apply_plan_command(store, plan_mode_state, &args),
        "skills" => append_system_message(store, skills_text()),
        "tools" => append_system_message(store, tools_text()),
        "abort" => apply_abort_command(store, input_handle, mock_turns, status_state),
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
) {
    if cancel_latest_streaming_turn(store, input_handle, mock_turns, status_state) {
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
    finish_canceled_turn(input_handle, mock_turns, status_state, message_id);
    true
}

fn finish_canceled_turn(
    input_handle: &ChatInputHandle,
    mock_turns: &MockTurnRegistry,
    status_state: &Property<String>,
    message_id: ChatMessageId,
) {
    let _ = mock_turns.cancel(message_id);
    input_handle.streaming_binding().set(false);
    status_state.set(STATUS_READY.to_string());
}

fn help_text() -> &'static str {
    "Available commands:\n\
- /help: Show this help.\n\
- /clear: Clear the current conversation and keep app configuration.\n\
- /plan [on|off|auto]: Cycle or set the basic plan mode state.\n\
- /skills: List available skills.\n\
- /tools: List available tools and approval policy.\n\
- /abort: Cancel the active mock turn."
}

fn skills_text() -> &'static str {
    "Skills: none registered yet. Skill registry integration is scheduled for M4."
}

fn tools_text() -> &'static str {
    "Tools: none registered yet. Tool registry abstractions are available; built-in tools and approvals are scheduled for M3.3-M3.5."
}

fn spawn_mock_agent_turn(action_sender: mpsc::Sender<AppAction>, request: MockAgentTurnRequest) {
    thread::spawn(move || {
        let mut stream = DeepSeekUiStream::new(
            request.branch,
            request.message_id,
            request.block_id,
            request.model,
        );
        for delta in mock_agent_deltas(&request.prompt) {
            if request.cancel.is_cancelled() {
                return;
            }
            thread::sleep(request.token_delay);
            if request.cancel.is_cancelled() {
                return;
            }
            if !send_stream_actions(
                &action_sender,
                stream.map_event(mock_stream_content_event(delta)),
            ) {
                return;
            }
        }
        if request.cancel.is_cancelled() {
            return;
        }
        thread::sleep(request.token_delay);
        if request.cancel.is_cancelled() {
            return;
        }
        if !send_stream_actions(&action_sender, stream.map_event(mock_stream_finish_event())) {
            return;
        }
        let _ = send_stream_actions(
            &action_sender,
            stream.map_event(ChatCompletionSseEvent::Done),
        );
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
            for tool_call in tool_calls {
                if store
                    .append_block(message_id, ChatBlock::ToolUse(tool_call))
                    .is_none()
                {
                    return false;
                }
            }
            true
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
                input_handle.streaming_binding().set(false);
                status_state.set(STATUS_READY.to_string());
            }
            found
        }
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
    use atto_ui::ComponentValue;
    use atto_ui::composable::{
        ComponentContext, EventHandling, MouseCoordinateSpace, ScrollbarHost, TabMode,
    };
    use atto_ui::theme::Theme;
    use atto_ui::wm::WindowId;
    use atto_ui_chat::{
        ChatBlock, ChatError, ChatErrorKind, ChatInputMode, ChatInputResponse, ChatMessage,
        ChatMessageStore, ChatRole, ChatSlashCommandAction, ChatTurnStatus, StopReason, TokenUsage,
        ToolInput, ToolStatus,
    };
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::layout::Rect;

    use crate::config::{AgentConfig, PlanMode};
    use crate::deepseek::{
        ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionDelta,
        ChatCompletionSseEvent, ChatFunctionCallDelta, ChatToolCallDelta, ChatToolKind,
        chat_error_from_http_status, chat_error_from_json_error, chat_error_from_network_failure,
        chat_error_from_stream_disconnect, parse_chat_completion_sse,
        parse_chat_completion_sse_data,
    };
    use crate::stream_ui::DeepSeekUiStream;

    use super::{
        APP_TITLE, AgentApp, AgentTurnLauncher, AppAction, MockTurnRegistry, STATUS_READY,
        STATUS_STREAMING, apply_app_action, build_chat_panel, submit_input_response,
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
            vec!["/help", "/clear", "/plan", "/skills", "/tools", "/abort"]
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

        assert!(submit_slash_command_text(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            "/help",
        ));

        let messages = store.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, ChatRole::System);
        assert!(message_text(&messages[0]).contains("/clear"));
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
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
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

        assert!(submit_slash_command_text(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            "/plan on",
        ));
        assert_eq!(plan_mode_state.get(), PlanMode::On.status());

        assert!(submit_slash_command_text(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            "/plan auto",
        ));
        assert_eq!(plan_mode_state.get(), PlanMode::Auto.status());

        assert!(submit_slash_command_text(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            "/plan",
        ));
        assert_eq!(plan_mode_state.get(), PlanMode::Off.status());

        let messages = store.messages();
        assert!(message_text(&messages[0]).contains("Plan mode set to on."));
        assert!(message_text(&messages[1]).contains("Plan mode set to auto."));
        assert!(message_text(&messages[2]).contains("Plan mode set to off."));
    }

    #[test]
    fn skills_and_tools_slash_commands_report_empty_m1_registries() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());

        assert!(submit_slash_command_text(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            "/skills",
        ));
        assert!(submit_slash_command_text(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            "/tools",
        ));

        let messages = store.messages();
        assert!(message_text(&messages[0]).contains("Skills: none registered"));
        assert!(message_text(&messages[1]).contains("Tools: none registered"));
    }

    #[test]
    fn abort_slash_command_cancels_latest_streaming_turn_and_rejects_late_tokens() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
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
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            "/abort",
        ));

        let messages = store.messages();
        assert_eq!(messages[0].status, ChatTurnStatus::Canceled);
        assert_eq!(messages[1].role, ChatRole::System);
        assert!(message_text(&messages[1]).contains("Aborted active turn."));
        assert!(cancel.is_cancelled());
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert!(!apply_app_action(
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
            &input_handle,
            "deepseek-chat".to_string(),
            status_state.clone(),
            plan_mode_state,
            sender,
            mock_turns.clone(),
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
        assert!(!apply_app_action(
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
        let turn_launcher = AgentTurnLauncher {
            model: "deepseek-chat".to_string(),
            action_sender: sender,
        };

        submit_input_response(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
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

        assert!(apply_app_action(
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
        assert!(apply_app_action(
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
        assert!(apply_app_action(
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
                assert!(apply_app_action(
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
        assert!(!apply_app_action(
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
                assert!(apply_app_action(
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
                assert_eq!(block.status, ToolStatus::Pending);
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
                assert_eq!(block.status, ToolStatus::Pending);
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
                assert!(apply_app_action(
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
