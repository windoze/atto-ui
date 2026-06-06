use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use atto_ui::app::{
    AppControl, CrosstermAppConfig, Desktop, MenuBar, run_crossterm_desktop_with_actions,
    run_crossterm_desktop_with_actions_and_tasks,
};
use atto_ui::composable::{EventOutcome, VStack};
use atto_ui::reactive::{EventQueue, Property};
use atto_ui::task::TaskRegistry;
use atto_ui::theme::Theme;
use atto_ui::widgets::{Label, Spinner};
use atto_ui::wm::{Window, WindowKind};

#[derive(Clone, Debug)]
enum SnapshotAsyncAction {
    SetStatus(String),
}

#[derive(Clone, Debug)]
enum CancellableAction {
    SetStatus(String),
    SetRunning(bool),
}

fn run_cancellable_fixture() -> Result<()> {
    let cfg = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(false);

    let tasks = TaskRegistry::new();
    let running_for_view = tasks.running_property();
    let status = Property::new("Task: idle".to_string());
    let running_label = Property::new("Running: false".to_string());
    let status_for_view = status.clone();
    let running_label_for_view = running_label.clone();
    let status_for_actions = status.clone();
    let running_label_for_actions = running_label.clone();
    let status_for_events = status;
    let running_label_for_events = running_label;
    let (action_sender, action_receiver) = EventQueue::<CancellableAction>::channel();
    let sender_for_events = action_sender;
    let tasks_for_events = tasks.clone();
    let mut ping_count = 0usize;

    run_crossterm_desktop_with_actions_and_tasks(
        cfg,
        move |screen| {
            let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
            let work = Desktop::layout(screen).work_area;
            let root = VStack::new()
                .child(Label::new("Task cancellation fixture"))
                .child(Spinner::new("Worker running").running(running_for_view.binding()))
                .child(Label::new(status_for_view.binding()))
                .child(Label::new(running_label_for_view.binding()))
                .child(Label::new("Keys: s start, Esc cancel, p ping, Ctrl+Q quit"));

            desktop.add_window(
                Window::new(WindowKind::Normal, "Cancellable", work, Box::new(root)),
                screen,
            );

            Ok(desktop)
        },
        action_receiver,
        tasks,
        move |_desktop, action, _screen| {
            match action {
                CancellableAction::SetStatus(next) => status_for_actions.set(next),
                CancellableAction::SetRunning(is_running) => {
                    running_label_for_actions.set(format!("Running: {is_running}"));
                }
            }
            Ok(AppControl::Continue)
        },
        |_desktop, _screen| Ok(AppControl::Continue),
        move |_desktop, event, _screen, result| {
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

            match *code {
                KeyCode::Char('s') => {
                    if tasks_for_events.is_running() {
                        status_for_events.set("Task: already running".to_string());
                    } else {
                        status_for_events.set("Task: running".to_string());
                        running_label_for_events.set("Running: true".to_string());
                        let sender_for_worker = sender_for_events.clone();
                        let handle = tasks_for_events.register("snapshot-worker");
                        let task_id = handle.id();
                        let token = handle.token();
                        let registry_for_worker = tasks_for_events.clone();
                        let _join = thread::spawn(move || {
                            while !token.is_cancelled() {
                                thread::sleep(Duration::from_millis(10));
                            }
                            registry_for_worker.unregister(task_id);
                            sender_for_worker
                                .send(CancellableAction::SetRunning(false))
                                .ok();
                            sender_for_worker
                                .send(CancellableAction::SetStatus("Task: cancelled".to_string()))
                                .ok();
                        });
                    }
                }
                KeyCode::Char('p') => {
                    ping_count += 1;
                    status_for_events.set(format!("Ping: {ping_count}"));
                }
                _ => {}
            }

            Ok(AppControl::Continue)
        },
    )
}

fn main() -> Result<()> {
    if std::env::args().any(|arg| arg == "--cancellable") {
        return run_cancellable_fixture();
    }

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
