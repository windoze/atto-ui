use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use atto_ui::app::{
    AppControl, CrosstermAppConfig, Desktop, MenuBar, run_crossterm_desktop_with_actions,
};
use atto_ui::composable::EventOutcome;
use atto_ui::reactive::{EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::widgets::Label;
use atto_ui::wm::{Window, WindowKind};

#[derive(Clone, Debug)]
enum SnapshotAsyncAction {
    SetStatus(String),
}

fn main() -> Result<()> {
    // Deterministic app used by PTY tests to validate async-action integration.
    let cfg = CrosstermAppConfig::default()
        // PTY tests do their own waiting; keep draw ticks responsive.
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(false);

    let status = Property::new("Async: idle".to_string());
    let status_for_view = status.clone();
    let status_for_actions = status;

    let (action_sender, action_receiver) = EventQueue::<SnapshotAsyncAction>::channel();

    // Background thread waits for a deterministic trigger from the main thread.
    let (start_tx, start_rx) = mpsc::channel::<()>();
    let sender_for_thread = action_sender.clone();
    thread::spawn(move || {
        while start_rx.recv().is_ok() {
            let _ =
                sender_for_thread.send(SnapshotAsyncAction::SetStatus("Async: done".to_string()));
        }
    });

    let start_tx_for_events = start_tx;

    run_crossterm_desktop_with_actions(
        cfg,
        move |screen| {
            let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
            let work = Desktop::layout(screen).work_area;

            desktop.add_window(
                Window::new(
                    WindowKind::Normal,
                    "Async",
                    work,
                    Box::new(Label::new(status_for_view.binding())),
                ),
                screen,
            );

            Ok(desktop)
        },
        action_receiver,
        move |_desktop, action, _screen| {
            match action {
                SnapshotAsyncAction::SetStatus(next) => {
                    status_for_actions.set(next);
                }
            }
            Ok(AppControl::Continue)
        },
        |_desktop, _screen| Ok(AppControl::Continue),
        move |_desktop, event, _screen, result| {
            // App-level shortcut: only if the UI didn't handle the key.
            if result.outcome != EventOutcome::Ignored {
                return Ok(AppControl::Continue);
            }

            let Event::Key(KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                ..
            }) = event
            else {
                return Ok(AppControl::Continue);
            };

            if matches!(*code, KeyCode::Char('s')) {
                start_tx_for_events.send(()).ok();
            }

            Ok(AppControl::Continue)
        },
    )
}
