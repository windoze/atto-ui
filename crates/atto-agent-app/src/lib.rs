#![forbid(unsafe_code)]

//! Application crate for the Atto TUI agent.
//!
//! The crate is intentionally thin at this stage: later milestones will compose
//! `atto-ui`, `atto-ui-chat`, and `atto-ui-async` here without adding network
//! dependencies to the reusable UI crates.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::Result;
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

pub const APP_TITLE: &str = "Atto Agent";
const CHAT_WINDOW_TAG: &str = "atto-agent:chat";
const STATUS_READY: &str = "ready";
const STATUS_STREAMING: &str = "streaming";
const PLAN_MODE_OFF: &str = "plan: off";
const PLAN_MODE_ON: &str = "plan: on";
const PLAN_MODE_AUTO: &str = "plan: auto";
const MOCK_TOKEN_DELAY: Duration = Duration::from_millis(24);

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

/// Runtime state for the single-window agent UI.
pub struct AgentApp {
    desktop: Desktop,
    message_store: ChatMessageStore,
    input_handle: ChatInputHandle,
    status_state: Property<String>,
    plan_mode_state: Property<String>,
    chat_window_id: WindowId,
}

impl AgentApp {
    /// Builds the initial desktop, status bar, chat panel, and chat state handles.
    pub fn new(screen: Rect) -> Self {
        let (action_sender, _action_receiver) = EventQueue::<AppAction>::channel();
        Self::with_runtime_state(
            screen,
            EventQueue::new(),
            action_sender,
            ChatMessageStore::new(),
            ChatInputHandle::new(),
            Property::new(STATUS_READY.to_string()),
            Property::new(PLAN_MODE_OFF.to_string()),
        )
    }

    fn with_runtime_state(
        screen: Rect,
        quit_events: EventQueue<()>,
        action_sender: mpsc::Sender<AppAction>,
        message_store: ChatMessageStore,
        input_handle: ChatInputHandle,
        status_state: Property<String>,
        plan_mode_state: Property<String>,
    ) -> Self {
        let chat_panel = build_chat_panel(
            &message_store,
            &input_handle,
            status_state.clone(),
            plan_mode_state.clone(),
            action_sender,
        );

        let mut desktop = Desktop::new(Theme::dark(), agent_menu(quit_events));
        desktop.status.set_segments(status_segments(
            status_state.binding(),
            plan_mode_state.binding(),
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
            desktop,
            message_store,
            input_handle,
            status_state,
            plan_mode_state,
            chat_window_id,
        }
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

    pub fn plan_mode_state(&self) -> Property<String> {
        self.plan_mode_state.clone()
    }

    pub fn chat_window_id(&self) -> WindowId {
        self.chat_window_id
    }
}

/// Runs the TUI agent application.
pub fn run() -> Result<()> {
    let quit_events = EventQueue::new();
    let quit_events_for_menu = quit_events.clone();
    let quit_events_for_loop = quit_events.clone();
    let (action_sender, action_receiver) = EventQueue::<AppAction>::channel();
    let message_store = ChatMessageStore::new();
    let input_handle = ChatInputHandle::new();
    let status_state = Property::new(STATUS_READY.to_string());
    let plan_mode_state = Property::new(PLAN_MODE_OFF.to_string());
    let message_store_for_build = message_store.clone();
    let input_handle_for_build = input_handle.clone();
    let status_state_for_build = status_state.clone();
    let plan_mode_state_for_build = plan_mode_state.clone();
    let action_sender_for_build = action_sender.clone();
    let message_store_for_actions = message_store.clone();
    let input_handle_for_actions = input_handle.clone();
    let status_state_for_actions = status_state.clone();

    run_crossterm_desktop_with_actions(
        CrosstermAppConfig::default()
            .bracketed_paste(true)
            .cursor(CursorMode::Show),
        move |screen| {
            Ok(AgentApp::with_runtime_state(
                screen,
                quit_events_for_menu,
                action_sender_for_build,
                message_store_for_build,
                input_handle_for_build,
                status_state_for_build,
                plan_mode_state_for_build,
            )
            .into_desktop())
        },
        action_receiver,
        move |_desktop, action, _screen| {
            apply_app_action(
                &message_store_for_actions,
                &input_handle_for_actions,
                &status_state_for_actions,
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
) -> ChatPanel {
    // Compose the reusable chat list and input controls around shared state handles.
    let list = ChatMessageList::new(store.clone()).show_timestamps(false);
    let store_for_submit = store.clone();
    let input_handle_for_submit = input_handle.clone();
    let status_for_submit = status_state.clone();
    let plan_mode_for_submit = plan_mode_state.clone();
    let store_for_slash = store.clone();
    let input_handle_for_slash = input_handle.clone();
    let status_for_slash = status_state.clone();
    let plan_mode_for_slash = plan_mode_state.clone();
    input_handle.set_slash_commands(agent_slash_commands());
    let input = input_handle
        .panel()
        .on_submit(move |response| {
            submit_input_response(
                &store_for_submit,
                &input_handle_for_submit,
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
    status_state: &Property<String>,
    plan_mode_state: &Property<String>,
    action_sender: &mpsc::Sender<AppAction>,
    response: ChatInputResponse,
) {
    let text = input_response_text(response);
    if text.trim().is_empty() {
        return;
    }

    if submit_slash_command_text(store, input_handle, status_state, plan_mode_state, &text) {
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

    input_handle.streaming_binding().set(true);
    status_state.set(STATUS_STREAMING.to_string());
    spawn_mock_agent_turn(
        action_sender.clone(),
        branch,
        assistant_id,
        text_block_id,
        text,
    );
}

fn input_response_text(response: ChatInputResponse) -> String {
    match response {
        ChatInputResponse::Text(text) | ChatInputResponse::Custom(text) => text,
        ChatInputResponse::Choice { label, .. } => label,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanMode {
    Off,
    On,
    Auto,
}

impl PlanMode {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "on" => Some(Self::On),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }

    fn from_status(status: &str) -> Option<Self> {
        status.strip_prefix("plan: ").and_then(Self::parse)
    }

    fn next(self) -> Self {
        match self {
            Self::Off => Self::On,
            Self::On => Self::Auto,
            Self::Auto => Self::Off,
        }
    }

    fn status(self) -> &'static str {
        match self {
            Self::Off => PLAN_MODE_OFF,
            Self::On => PLAN_MODE_ON,
            Self::Auto => PLAN_MODE_AUTO,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
            Self::Auto => "auto",
        }
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
        "clear" => clear_session(store, input_handle, status_state),
        "plan" => apply_plan_command(store, plan_mode_state, &args),
        "skills" => append_system_message(store, skills_text()),
        "tools" => append_system_message(store, tools_text()),
        "abort" => apply_abort_command(store, input_handle, status_state),
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
    status_state: &Property<String>,
) {
    store.replace_all(Vec::new());
    input_handle.streaming_binding().set(false);
    input_handle.clear_queued_responses();
    input_handle.clear_references();
    status_state.set(STATUS_READY.to_string());
}

fn apply_plan_command(store: &ChatMessageStore, plan_mode_state: &Property<String>, args: &[&str]) {
    let current = PlanMode::from_status(&plan_mode_state.get()).unwrap_or(PlanMode::Off);
    let next = match args {
        [] => Some(current.next()),
        [value] => PlanMode::parse(value),
        _ => None,
    };
    let Some(next) = next else {
        append_system_message(store, "Usage: /plan [on|off|auto]");
        return;
    };

    plan_mode_state.set(next.status().to_string());
    append_system_message(store, format!("Plan mode set to {}.", next.label()));
}

fn apply_abort_command(
    store: &ChatMessageStore,
    input_handle: &ChatInputHandle,
    status_state: &Property<String>,
) {
    if cancel_latest_streaming_turn(store, input_handle, status_state) {
        append_system_message(store, "Aborted active turn.");
    } else {
        append_system_message(store, "No active turn to abort.");
    }
}

fn cancel_latest_streaming_turn(
    store: &ChatMessageStore,
    input_handle: &ChatInputHandle,
    status_state: &Property<String>,
) -> bool {
    let mut messages = store.messages();
    let Some(message) = messages
        .iter_mut()
        .rev()
        .find(|message| message.status.is_streaming())
    else {
        return false;
    };

    // Replacing the transcript after marking the turn canceled advances the branch token.
    message.set_turn_status(ChatTurnStatus::Canceled);
    store.replace_all(messages);
    input_handle.streaming_binding().set(false);
    status_state.set(STATUS_READY.to_string());
    true
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
    prompt: String,
) {
    thread::spawn(move || {
        for delta in mock_agent_deltas(&prompt) {
            thread::sleep(MOCK_TOKEN_DELAY);
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
        thread::sleep(MOCK_TOKEN_DELAY);
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

fn status_segments(state: Binding<String>, plan_mode: Binding<String>) -> Vec<StatusSegment> {
    // Keep provider static in M1 while binding runtime state to the mock turn lifecycle.
    vec![
        StatusSegment::new("app", APP_TITLE)
            .priority(100)
            .min_width(10),
        StatusSegment::new("provider", "provider: mock")
            .priority(80)
            .min_width(14),
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
    use atto_ui_chat::{
        ChatBlock, ChatInputMode, ChatInputResponse, ChatMessage, ChatMessageStore, ChatRole,
        ChatSlashCommandAction, ChatTurnStatus,
    };
    use ratatui::layout::Rect;

    use super::{
        APP_TITLE, AgentApp, AppAction, PLAN_MODE_AUTO, PLAN_MODE_OFF, PLAN_MODE_ON, STATUS_READY,
        STATUS_STREAMING, apply_app_action, submit_input_response, submit_slash_command_text,
    };

    fn message_text(message: &ChatMessage) -> &str {
        match &message.blocks[0] {
            ChatBlock::Text(block) => &block.markdown,
            other => panic!("expected text block, got {other:?}"),
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
        assert_eq!(app.plan_mode_state().get(), PLAN_MODE_OFF);
        match app.input_handle().mode() {
            ChatInputMode::Text(config) => {
                assert_eq!(config.title, "Message");
                assert_eq!(config.placeholder.as_deref(), Some("Type a message..."));
            }
            other => panic!("expected text input mode, got {other:?}"),
        }
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
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PLAN_MODE_OFF.to_string());

        assert!(submit_slash_command_text(
            &store,
            &input_handle,
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
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PLAN_MODE_ON.to_string());
        input_handle.streaming_binding().set(true);
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "seed",
        ));

        assert!(submit_slash_command_text(
            &store,
            &input_handle,
            &status_state,
            &plan_mode_state,
            "/clear",
        ));

        assert!(store.messages().is_empty());
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert_eq!(plan_mode_state.get(), PLAN_MODE_ON);
    }

    #[test]
    fn plan_slash_command_sets_and_cycles_plan_mode() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PLAN_MODE_OFF.to_string());

        assert!(submit_slash_command_text(
            &store,
            &input_handle,
            &status_state,
            &plan_mode_state,
            "/plan on",
        ));
        assert_eq!(plan_mode_state.get(), PLAN_MODE_ON);

        assert!(submit_slash_command_text(
            &store,
            &input_handle,
            &status_state,
            &plan_mode_state,
            "/plan auto",
        ));
        assert_eq!(plan_mode_state.get(), PLAN_MODE_AUTO);

        assert!(submit_slash_command_text(
            &store,
            &input_handle,
            &status_state,
            &plan_mode_state,
            "/plan",
        ));
        assert_eq!(plan_mode_state.get(), PLAN_MODE_OFF);

        let messages = store.messages();
        assert!(message_text(&messages[0]).contains("Plan mode set to on."));
        assert!(message_text(&messages[1]).contains("Plan mode set to auto."));
        assert!(message_text(&messages[2]).contains("Plan mode set to off."));
    }

    #[test]
    fn skills_and_tools_slash_commands_report_empty_m1_registries() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PLAN_MODE_OFF.to_string());

        assert!(submit_slash_command_text(
            &store,
            &input_handle,
            &status_state,
            &plan_mode_state,
            "/skills",
        ));
        assert!(submit_slash_command_text(
            &store,
            &input_handle,
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
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PLAN_MODE_OFF.to_string());
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let block_id = assistant.blocks[0].id();
        store.push(assistant);
        let stale_branch = store.branch_token();

        assert!(submit_slash_command_text(
            &store,
            &input_handle,
            &status_state,
            &plan_mode_state,
            "/abort",
        ));

        let messages = store.messages();
        assert_eq!(messages[0].status, ChatTurnStatus::Canceled);
        assert_eq!(messages[1].role, ChatRole::System);
        assert!(message_text(&messages[1]).contains("Aborted active turn."));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert!(!apply_app_action(
            &store,
            &input_handle,
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
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PLAN_MODE_OFF.to_string());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();

        submit_input_response(
            &store,
            &input_handle,
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
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let block_id = assistant.blocks[0].id();
        store.push(assistant);
        let branch = store.branch_token();

        assert!(apply_app_action(
            &store,
            &input_handle,
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
            &status_state,
            AppAction::AssistantDone {
                branch,
                message_id: assistant_id,
            },
        ));

        let messages = store.messages();
        assert_eq!(message_text(&messages[0]), "Mock done");
        assert_eq!(messages[0].status, ChatTurnStatus::Complete);
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }
}
