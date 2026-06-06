//! Async terminal event loop built on crossterm `EventStream`.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use atto_ui::app::{
    AppControl, CrosstermAppConfig, Desktop, DesktopAction, DesktopEventResult, should_quit_default,
};
use atto_ui::composable::EventOutcome;
use atto_ui::reactive::{set_global_tick_rate, tick_global_timers};
use atto_ui::task::TaskRegistry;
use crossterm::cursor;
use crossterm::event::{self, Event, EventStream, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures_util::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use tokio::sync::mpsc as tokio_mpsc;
use tokio::time::{self, MissedTickBehavior};

/// A terminal event or application action received by the async input helper.
#[derive(Debug)]
pub enum AsyncInput<A> {
    Terminal(Event),
    Action(A),
}

/// Creates a crossterm `EventStream` for async terminal input.
pub fn terminal_event_stream() -> EventStream {
    EventStream::new()
}

/// Waits for either the next terminal event or the next async action.
pub async fn next_terminal_event_or_action<A>(
    events: &mut EventStream,
    actions: &mut tokio_mpsc::UnboundedReceiver<A>,
) -> Result<Option<AsyncInput<A>>> {
    tokio::select! {
        maybe_event = events.next() => match maybe_event {
            Some(Ok(event)) => Ok(Some(AsyncInput::Terminal(event))),
            Some(Err(err)) => Err(err.into()),
            None => Ok(None),
        },
        action = actions.recv() => Ok(action.map(AsyncInput::Action)),
    }
}

/// Runs a crossterm-backed desktop UI with async terminal input and a core action channel.
pub async fn run_crossterm_desktop_with_async_actions<B, TTick, TEvent, TAction, A>(
    config: CrosstermAppConfig,
    build: B,
    action_receiver: mpsc::Receiver<A>,
    on_action: TAction,
    on_tick: TTick,
    on_event: TEvent,
) -> Result<()>
where
    A: Send + 'static,
    B: FnOnce(Rect) -> Result<Desktop>,
    TAction: FnMut(&mut Desktop, A, Rect) -> Result<AppControl>,
    TTick: FnMut(&mut Desktop, Rect) -> Result<AppControl>,
    TEvent: FnMut(&mut Desktop, &Event, Rect, &DesktopEventResult) -> Result<AppControl>,
{
    run_crossterm_desktop_with_async_actions_and_tasks(
        config,
        build,
        action_receiver,
        TaskRegistry::new(),
        on_action,
        on_tick,
        on_event,
    )
    .await
}

/// Runs an async crossterm UI with a shared `TaskRegistry` for Esc cancellation.
pub async fn run_crossterm_desktop_with_async_actions_and_tasks<B, TTick, TEvent, TAction, A>(
    config: CrosstermAppConfig,
    build: B,
    action_receiver: mpsc::Receiver<A>,
    task_registry: TaskRegistry,
    mut on_action: TAction,
    mut on_tick: TTick,
    mut on_event: TEvent,
) -> Result<()>
where
    A: Send + 'static,
    B: FnOnce(Rect) -> Result<Desktop>,
    TAction: FnMut(&mut Desktop, A, Rect) -> Result<AppControl>,
    TTick: FnMut(&mut Desktop, Rect) -> Result<AppControl>,
    TEvent: FnMut(&mut Desktop, &Event, Rect, &DesktopEventResult) -> Result<AppControl>,
{
    let mut session = TerminalSession::new(config)?;
    let screen = session.screen()?;
    let mut desktop = build(screen)?;
    set_global_tick_rate(config.tick_rate);

    let mut events = terminal_event_stream();
    let mut actions = ActionBridge::new(action_receiver);
    let mut actions_open = true;
    let mut ticks = time::interval(config.tick_rate);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        if actions_open {
            tokio::select! {
                _ = ticks.tick() => {
                    let screen = session.screen()?;
                    tick_global_timers();
                    if on_tick(&mut desktop, screen)? == AppControl::Exit {
                        break;
                    }
                    session.draw(&mut desktop)?;
                }
                action = actions.recv() => {
                    let Some(action) = action else {
                        actions_open = false;
                        continue;
                    };
                    let screen = session.screen()?;
                    if on_action(&mut desktop, action, screen)? == AppControl::Exit {
                        break;
                    }
                    while let Some(action) = actions.try_recv() {
                        if on_action(&mut desktop, action, screen)? == AppControl::Exit {
                            return Ok(());
                        }
                    }
                    session.draw(&mut desktop)?;
                }
                maybe_event = events.next() => {
                    if !handle_terminal_event(
                        maybe_event,
                        &mut session,
                        &mut desktop,
                        &task_registry,
                        &mut on_event,
                    )? {
                        break;
                    }
                }
            }
        } else {
            tokio::select! {
                _ = ticks.tick() => {
                    let screen = session.screen()?;
                    tick_global_timers();
                    if on_tick(&mut desktop, screen)? == AppControl::Exit {
                        break;
                    }
                    session.draw(&mut desktop)?;
                }
                maybe_event = events.next() => {
                    if !handle_terminal_event(
                        maybe_event,
                        &mut session,
                        &mut desktop,
                        &task_registry,
                        &mut on_event,
                    )? {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

fn handle_terminal_event<TEvent>(
    maybe_event: Option<io::Result<Event>>,
    session: &mut TerminalSession,
    desktop: &mut Desktop,
    task_registry: &TaskRegistry,
    on_event: &mut TEvent,
) -> Result<bool>
where
    TEvent: FnMut(&mut Desktop, &Event, Rect, &DesktopEventResult) -> Result<AppControl>,
{
    let Some(event) = maybe_event else {
        return Ok(false);
    };
    let event = event?;
    let screen = session.screen()?;
    let mut result = desktop.handle_event(&event, screen);
    handle_desktop_action(desktop, &result.action);
    mark_consumed_if_escape_cancelled(&event, &mut result, task_registry);

    if should_quit_default(&event, result.outcome) {
        return Ok(false);
    }
    if on_event(desktop, &event, screen, &result)? == AppControl::Exit {
        return Ok(false);
    }

    session.draw(desktop)?;
    Ok(true)
}

fn handle_desktop_action(desktop: &mut Desktop, action: &DesktopAction) {
    match action {
        DesktopAction::None => {}
        DesktopAction::CloseWindow(id) => {
            desktop.close_window(*id);
        }
    }
}

fn is_escape_press(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(KeyEvent {
            code: KeyCode::Esc,
            kind: KeyEventKind::Press,
            ..
        })
    )
}

fn mark_consumed_if_escape_cancelled(
    event: &Event,
    result: &mut DesktopEventResult,
    task_registry: &TaskRegistry,
) -> bool {
    if result.outcome != EventOutcome::Ignored || !is_escape_press(event) {
        return false;
    }

    if task_registry.cancel_current() {
        result.outcome = EventOutcome::Consumed;
        true
    } else {
        false
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalSession {
    fn new(config: CrosstermAppConfig) -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();

        execute!(stdout, EnterAlternateScreen)?;
        if config.enable_mouse_capture {
            execute!(stdout, event::EnableMouseCapture)?;
        }
        if config.enable_bracketed_paste {
            execute!(stdout, event::EnableBracketedPaste)?;
        }
        match config.cursor {
            atto_ui::app::CursorMode::Show => execute!(stdout, cursor::Show)?,
            atto_ui::app::CursorMode::Hide => execute!(stdout, cursor::Hide)?,
        }

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        Ok(Self { terminal })
    }

    fn screen(&self) -> Result<Rect> {
        Ok(self.terminal.size()?.into())
    }

    fn draw(&mut self, desktop: &mut Desktop) -> Result<()> {
        self.terminal.draw(|frame| desktop.draw(frame))?;
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();

        let backend = self.terminal.backend_mut();
        let _ = execute!(backend, LeaveAlternateScreen);
        let _ = execute!(backend, event::DisableMouseCapture);
        let _ = execute!(backend, event::DisableBracketedPaste);
        let _ = execute!(backend, cursor::Show);

        let _ = self.terminal.show_cursor();
    }
}

struct ActionBridge<A> {
    shutdown: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
    receiver: tokio_mpsc::UnboundedReceiver<A>,
}

impl<A> ActionBridge<A>
where
    A: Send + 'static,
{
    fn new(source: mpsc::Receiver<A>) -> Self {
        let (sender, receiver) = tokio_mpsc::unbounded_channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_for_thread = Arc::clone(&shutdown);
        let join = thread::spawn(move || {
            while !shutdown_for_thread.load(Ordering::Relaxed) {
                match source.recv_timeout(Duration::from_millis(10)) {
                    Ok(action) => {
                        if sender.send(action).is_err() {
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        Self {
            shutdown,
            join: Some(join),
            receiver,
        }
    }

    async fn recv(&mut self) -> Option<A> {
        self.receiver.recv().await
    }

    fn try_recv(&mut self) -> Option<A> {
        self.receiver.try_recv().ok()
    }
}

impl<A> Drop for ActionBridge<A> {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
