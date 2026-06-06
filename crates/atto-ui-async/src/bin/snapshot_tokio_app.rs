use std::time::Duration;

use anyhow::Result;
use atto_ui::app::{AppControl, CrosstermAppConfig, Desktop, MenuBar};
use atto_ui::composable::{EventOutcome, VStack};
use atto_ui::reactive::{EventQueue, Property};
use atto_ui::task::TaskRegistry;
use atto_ui::theme::Theme;
use atto_ui::widgets::{Label, Spinner};
use atto_ui::wm::{Window, WindowKind};
use atto_ui_async::{
    build_current_thread_runtime, run_crossterm_desktop_with_async_actions_and_tasks, spawn_async,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Clone, Debug)]
enum TokioAction {
    SetStatus(String),
    SetRunning(bool),
}

async fn run_dispatch_fixture() -> Result<()> {
    let cfg = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(false);
    let tasks = TaskRegistry::new();
    let status = Property::new("Tokio: idle".to_string());
    let status_for_view = status.clone();
    let status_for_actions = status.clone();
    let status_for_events = status;
    let (action_sender, action_receiver) = EventQueue::<TokioAction>::channel();
    let sender_for_events = action_sender;
    let tasks_for_events = tasks.clone();

    run_crossterm_desktop_with_async_actions_and_tasks(
        cfg,
        move |screen| {
            let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
            let work = Desktop::layout(screen).work_area;
            desktop.add_window(
                Window::new(
                    WindowKind::Normal,
                    "Tokio",
                    work,
                    Box::new(Label::new(status_for_view.binding())),
                ),
                screen,
            );
            Ok(desktop)
        },
        action_receiver,
        tasks,
        move |_desktop, action, _screen| {
            if let TokioAction::SetStatus(next) = action {
                status_for_actions.set(next);
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

            if matches!(*code, KeyCode::Char('s')) && !tasks_for_events.is_running() {
                status_for_events.set("Tokio: scheduled".to_string());
                let sender_for_task = sender_for_events.clone();
                let (_handle, _join) = spawn_async(
                    &tasks_for_events,
                    "tokio-dispatch",
                    sender_for_task,
                    |token, actions| async move {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        if !token.is_cancelled() {
                            actions
                                .send(TokioAction::SetStatus("Tokio: done".to_string()))
                                .ok();
                        }
                    },
                );
            }

            Ok(AppControl::Continue)
        },
    )
    .await
}

async fn run_cancellable_fixture() -> Result<()> {
    let cfg = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(false);
    let tasks = TaskRegistry::new();
    let running_for_view = tasks.running_property();
    let status = Property::new("Tokio: idle".to_string());
    let running_label = Property::new("Running: false".to_string());
    let status_for_view = status.clone();
    let running_label_for_view = running_label.clone();
    let status_for_actions = status.clone();
    let running_label_for_actions = running_label.clone();
    let status_for_events = status;
    let running_label_for_events = running_label;
    let (action_sender, action_receiver) = EventQueue::<TokioAction>::channel();
    let sender_for_events = action_sender;
    let tasks_for_events = tasks.clone();
    let mut ping_count = 0usize;

    run_crossterm_desktop_with_async_actions_and_tasks(
        cfg,
        move |screen| {
            let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
            let work = Desktop::layout(screen).work_area;
            let root = VStack::new()
                .child(Label::new("Tokio async fixture"))
                .child(Spinner::new("Worker running").running(running_for_view.binding()))
                .child(Label::new(status_for_view.binding()))
                .child(Label::new(running_label_for_view.binding()))
                .child(Label::new("Keys: c start, Esc cancel, p ping, Ctrl+Q quit"));

            desktop.add_window(
                Window::new(WindowKind::Normal, "Tokio", work, Box::new(root)),
                screen,
            );
            Ok(desktop)
        },
        action_receiver,
        tasks,
        move |_desktop, action, _screen| {
            match action {
                TokioAction::SetStatus(next) => status_for_actions.set(next),
                TokioAction::SetRunning(is_running) => {
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
                KeyCode::Char('c') => {
                    if tasks_for_events.is_running() {
                        status_for_events.set("Tokio: already running".to_string());
                    } else {
                        status_for_events.set("Tokio: running".to_string());
                        running_label_for_events.set("Running: true".to_string());
                        let sender_for_task = sender_for_events.clone();
                        let (_handle, _join) = spawn_async(
                            &tasks_for_events,
                            "tokio-cancellable",
                            sender_for_task,
                            |token, actions| async move {
                                while !token.is_cancelled() {
                                    tokio::time::sleep(Duration::from_millis(10)).await;
                                }
                                actions.send(TokioAction::SetRunning(false)).ok();
                                actions
                                    .send(TokioAction::SetStatus("Tokio: cancelled".to_string()))
                                    .ok();
                            },
                        );
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
    .await
}

fn main() -> Result<()> {
    let runtime = build_current_thread_runtime()?;
    if std::env::args().any(|arg| arg == "--cancellable") {
        runtime.block_on(run_cancellable_fixture())
    } else {
        runtime.block_on(run_dispatch_fixture())
    }
}
