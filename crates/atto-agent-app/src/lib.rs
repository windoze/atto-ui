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
    ChatBlockId, ChatBranchToken, ChatInputHandle, ChatInputResponse, ChatMessage, ChatMessageId,
    ChatMessageList, ChatMessageStore, ChatPanel, ChatRole, ChatSlashCommand, ChatTurnStatus,
};
use ratatui::layout::Rect;

pub mod config;
pub mod deepseek;

use crate::config::{AgentConfig, PlanMode};

pub const APP_TITLE: &str = "Atto Agent";
const CHAT_WINDOW_TAG: &str = "atto-agent:chat";
const STATUS_READY: &str = "ready";
const STATUS_STREAMING: &str = "streaming";
const MOCK_TOKEN_DELAY: Duration = Duration::from_millis(24);
const SNAPSHOT_MOCK_TOKEN_DELAY: Duration = Duration::from_millis(96);

#[derive(Clone, Debug)]
enum AppAction {
    AssistantDelta {
        branch: ChatBranchToken,
        block_id: ChatBlockId,
        delta: String,
    },
    AssistantDone {
        branch: ChatBranchToken,
        message_id: ChatMessageId,
    },
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
                &action_sender,
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
    action_sender: &mpsc::Sender<AppAction>,
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
        action_sender.clone(),
        branch,
        assistant_id,
        text_block_id,
        cancel,
        mock_turns.token_delay(),
        text,
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
    "Tools: none registered in the M1 mock provider. Tool registry and approvals are scheduled for M3."
}

fn spawn_mock_agent_turn(
    action_sender: mpsc::Sender<AppAction>,
    branch: ChatBranchToken,
    message_id: ChatMessageId,
    block_id: ChatBlockId,
    cancel: CancellationToken,
    mock_token_delay: Duration,
    prompt: String,
) {
    thread::spawn(move || {
        for delta in mock_agent_deltas(&prompt) {
            if cancel.is_cancelled() {
                return;
            }
            thread::sleep(mock_token_delay);
            if cancel.is_cancelled() {
                return;
            }
            if action_sender
                .send(AppAction::AssistantDelta {
                    branch,
                    block_id,
                    delta,
                })
                .is_err()
            {
                return;
            }
        }
        if cancel.is_cancelled() {
            return;
        }
        thread::sleep(mock_token_delay);
        if cancel.is_cancelled() {
            return;
        }
        let _ = action_sender.send(AppAction::AssistantDone { branch, message_id });
    });
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
        AppAction::AssistantDelta {
            branch,
            block_id,
            delta,
        } => store.is_branch_current(branch) && store.append_text_delta(block_id, &delta),
        AppAction::AssistantDone { branch, message_id } => {
            if !store.is_branch_current(branch) {
                return false;
            }
            let found = store.set_turn_status(message_id, ChatTurnStatus::Complete);
            if found {
                mock_turns.clear(message_id);
                input_handle.streaming_binding().set(false);
                status_state.set(STATUS_READY.to_string());
            }
            found
        }
    }
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
    use atto_ui::composable::{
        ComponentContext, EventHandling, MouseCoordinateSpace, ScrollbarHost, TabMode,
    };
    use atto_ui::theme::Theme;
    use atto_ui::wm::WindowId;
    use atto_ui_chat::{
        ChatBlock, ChatInputMode, ChatInputResponse, ChatMessage, ChatMessageStore, ChatRole,
        ChatSlashCommandAction, ChatTurnStatus,
    };
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::layout::Rect;

    use crate::config::{AgentConfig, PlanMode};

    use super::{
        APP_TITLE, AgentApp, AppAction, MockTurnRegistry, STATUS_READY, STATUS_STREAMING,
        apply_app_action, build_chat_panel, submit_input_response, submit_slash_command_text,
    };

    fn message_text(message: &ChatMessage) -> &str {
        match &message.blocks[0] {
            ChatBlock::Text(block) => &block.markdown,
            other => panic!("expected text block, got {other:?}"),
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
            AppAction::AssistantDelta {
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
            AppAction::AssistantDelta {
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

        submit_input_response(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            &sender,
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
            AppAction::AssistantDelta {
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
            AppAction::AssistantDelta {
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
            AppAction::AssistantDone {
                branch,
                message_id: assistant_id,
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
}
