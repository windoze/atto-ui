#![forbid(unsafe_code)]

//! Application crate for the Atto TUI agent.
//!
//! The crate is intentionally thin at this stage: later milestones will compose
//! `atto-ui`, `atto-ui-chat`, and `atto-ui-async` here without adding network
//! dependencies to the reusable UI crates.

use anyhow::Result;
use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, MenuItem, MenuSpec,
    StatusSegment, StatusSegmentAlign, run_crossterm_desktop,
};
use atto_ui::reactive::EventQueue;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowId, WindowKind};
use atto_ui_chat::{ChatInputHandle, ChatMessageList, ChatMessageStore, ChatPanel};
use ratatui::layout::Rect;

pub const APP_TITLE: &str = "Atto Agent";
const CHAT_WINDOW_TAG: &str = "atto-agent:chat";

/// Runtime state for the single-window agent UI.
pub struct AgentApp {
    desktop: Desktop,
    message_store: ChatMessageStore,
    input_handle: ChatInputHandle,
    chat_window_id: WindowId,
}

impl AgentApp {
    /// Builds the initial desktop, status bar, chat panel, and chat state handles.
    pub fn new(screen: Rect) -> Self {
        Self::with_quit_events(screen, EventQueue::new())
    }

    fn with_quit_events(screen: Rect, quit_events: EventQueue<()>) -> Self {
        let message_store = ChatMessageStore::new();
        let input_handle = ChatInputHandle::new();
        let chat_panel = build_chat_panel(&message_store, &input_handle);

        let mut desktop = Desktop::new(Theme::dark(), agent_menu(quit_events));
        desktop.status.set_segments(status_segments());

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

    pub fn chat_window_id(&self) -> WindowId {
        self.chat_window_id
    }
}

/// Runs the TUI agent application.
pub fn run() -> Result<()> {
    let quit_events = EventQueue::new();
    let quit_events_for_menu = quit_events.clone();
    let quit_events_for_loop = quit_events.clone();

    run_crossterm_desktop(
        CrosstermAppConfig::default()
            .bracketed_paste(true)
            .cursor(CursorMode::Show),
        move |screen| Ok(AgentApp::with_quit_events(screen, quit_events_for_menu).into_desktop()),
        |_desktop, _screen| Ok(AppControl::Continue),
        move |_desktop, _event, _screen, _result| {
            if quit_events_for_loop.pop().is_some() {
                Ok(AppControl::Exit)
            } else {
                Ok(AppControl::Continue)
            }
        },
    )
}

fn build_chat_panel(store: &ChatMessageStore, input_handle: &ChatInputHandle) -> ChatPanel {
    // Compose the reusable chat list and input controls around shared state handles.
    let list = ChatMessageList::new(store.clone()).show_timestamps(false);
    ChatPanel::new(list, input_handle.panel())
}

fn agent_menu(quit_events: EventQueue<()>) -> MenuBar {
    // Keep the initial app shell minimal while still offering a discoverable quit action.
    MenuBar::new(vec![MenuSpec::new(
        "File",
        vec![MenuItem::action("Quit", move || quit_events.push(())).shortcut("q")],
    )])
}

fn status_segments() -> Vec<StatusSegment> {
    // Surface the static M1 shell state; later milestones will bind these to runtime state.
    vec![
        StatusSegment::new("app", APP_TITLE)
            .priority(100)
            .min_width(10),
        StatusSegment::new("provider", "provider: mock")
            .priority(80)
            .min_width(14),
        StatusSegment::new("state", "ready")
            .align(StatusSegmentAlign::Right)
            .priority(90)
            .min_width(5),
        StatusSegment::new("keys", "Ctrl+Q quit")
            .align(StatusSegmentAlign::Right)
            .priority(70)
            .min_width(11),
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
    use atto_ui_chat::ChatInputMode;
    use ratatui::layout::Rect;

    use super::{APP_TITLE, AgentApp};

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
        match app.input_handle().mode() {
            ChatInputMode::Text(config) => {
                assert_eq!(config.title, "Message");
                assert_eq!(config.placeholder.as_deref(), Some("Type a message..."));
            }
            other => panic!("expected text input mode, got {other:?}"),
        }
    }
}
