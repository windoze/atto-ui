use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;

use crate::app::{Desktop, DesktopAction, DesktopEventResult, WindowInfo};
use crate::app::{Toast, ToastLevel};
use crate::composable::EventOutcome;
use crate::inspect::{DesktopInspector, DesktopSnapshot};
use crate::ipc::{IpcServer, IpcServerConfig};
use crate::reactive::{global_tick_rate_nanos, set_global_tick_rate, tick_global_timers};

/// Cap on how many timer ticks a single `step` may dispatch, so a long pause
/// (e.g. the process was suspended) doesn't trigger a burst of catch-up ticks.
const MAX_TIMER_CATCHUP_TICKS: u32 = 8;
use crate::runtime::{ComponentValue, TreeError};
use crate::task::TaskRegistry;
use crate::{ComponentError, WindowId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorMode {
    Show,
    #[default]
    Hide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrosstermAppConfig {
    pub tick_rate: Duration,
    pub enable_mouse_capture: bool,
    pub enable_bracketed_paste: bool,
    pub enable_keyboard_enhancement: bool,
    pub cursor: CursorMode,
}

impl Default for CrosstermAppConfig {
    fn default() -> Self {
        Self {
            tick_rate: Duration::from_millis(16),
            enable_mouse_capture: true,
            enable_bracketed_paste: false,
            enable_keyboard_enhancement: true,
            cursor: CursorMode::Hide,
        }
    }
}

impl CrosstermAppConfig {
    pub fn tick_rate(self, tick_rate: Duration) -> Self {
        Self { tick_rate, ..self }
    }

    pub fn mouse_capture(self, enable_mouse_capture: bool) -> Self {
        Self {
            enable_mouse_capture,
            ..self
        }
    }

    pub fn bracketed_paste(self, enable_bracketed_paste: bool) -> Self {
        Self {
            enable_bracketed_paste,
            ..self
        }
    }

    pub fn keyboard_enhancement(self, enable_keyboard_enhancement: bool) -> Self {
        Self {
            enable_keyboard_enhancement,
            ..self
        }
    }

    pub fn cursor(self, cursor: CursorMode) -> Self {
        Self { cursor, ..self }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppControl {
    Continue,
    Exit,
}

pub fn should_quit_default(event: &Event, outcome: EventOutcome) -> bool {
    match event {
        // Ctrl+Q always quits.
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => true,
        // 'q' quits only when the event was not consumed by the UI.
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            ..
        }) => outcome == EventOutcome::Ignored,
        _ => false,
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    keyboard_enhancement_active: bool,
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
        let keyboard_enhancement_active = if config.enable_keyboard_enhancement {
            execute!(
                stdout,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )
            .is_ok()
        } else {
            false
        };
        match config.cursor {
            CursorMode::Show => execute!(stdout, cursor::Show)?,
            CursorMode::Hide => execute!(stdout, cursor::Hide)?,
        }

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        Ok(Self {
            terminal,
            keyboard_enhancement_active,
        })
    }
}

enum HostSession {
    Terminal(TerminalSession),
    Headless { screen: Rect },
}

impl HostSession {
    fn screen(&self) -> Result<Rect> {
        match self {
            Self::Terminal(session) => Ok(session.terminal.size()?.into()),
            Self::Headless { screen } => Ok(*screen),
        }
    }

    fn draw(&mut self, desktop: &mut Desktop) -> Result<()> {
        match self {
            Self::Terminal(session) => {
                session.terminal.draw(|f| desktop.draw(f))?;
                Ok(())
            }
            Self::Headless { screen } => {
                DesktopInspector::new(desktop)
                    .export_snapshot(*screen)
                    .map_err(|err| anyhow::anyhow!("{err:?}"))?;
                Ok(())
            }
        }
    }

    fn is_headless(&self) -> bool {
        matches!(self, Self::Headless { .. })
    }

    fn restore_terminal(&mut self) {
        if let Self::Terminal(_) = self {
            let screen = self.screen().unwrap_or_else(|_| Rect::new(0, 0, 80, 24));
            *self = Self::Headless { screen };
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();

        let backend = self.terminal.backend_mut();
        if self.keyboard_enhancement_active {
            let _ = execute!(backend, PopKeyboardEnhancementFlags);
        }
        let _ = execute!(backend, LeaveAlternateScreen);
        let _ = execute!(backend, event::DisableMouseCapture);
        let _ = execute!(backend, event::DisableBracketedPaste);
        let _ = execute!(backend, cursor::Show);

        let _ = self.terminal.show_cursor();
    }
}

fn handle_desktop_action(desktop: &mut Desktop, action: &DesktopAction) {
    match *action {
        DesktopAction::None => {}
        DesktopAction::CloseWindow(id) => {
            desktop.wm.close(id);
        }
    }
}

fn drain_ipc_server(ipc_server: &mut Option<IpcServer>, desktop: &mut Desktop, screen: Rect) {
    if let Some(server) = ipc_server.as_mut() {
        server.drain_pending(desktop, screen);
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

pub type TickCallBack = dyn FnMut(&mut Desktop, Rect) -> Result<AppControl>;
pub type EventCallBack =
    dyn FnMut(&mut Desktop, &Event, Rect, &DesktopEventResult) -> Result<AppControl>;

/// Advance global timers by the real elapsed time since `*last_timer_instant`, dispatching one tick
/// per `tick_rate` of wall-clock time (capped by [`MAX_TIMER_CATCHUP_TICKS`]).
///
/// This keeps duration-based timers running at real speed regardless of how often the host driver
/// iterates: the React tick loop calls `step` far more often than the tick rate, and a fixed
/// one-tick-per-iteration would run timers (e.g. a spinner) at the loop frequency instead of real
/// time. Shared by [`AppHost::step`] and the free-function run loops so both advance timers
/// identically.
fn advance_global_timers(config: &CrosstermAppConfig, last_timer_instant: &mut Option<Instant>) {
    let rate_nanos = if config.tick_rate.is_zero() {
        global_tick_rate_nanos()
    } else {
        config.tick_rate.as_nanos().min(u64::MAX as u128) as u64
    };
    let rate = Duration::from_nanos(rate_nanos.max(1));
    let now = Instant::now();
    let prev = *last_timer_instant.get_or_insert(now);
    let mut cursor = prev;
    let mut ticks = 0u32;
    while now.saturating_duration_since(cursor) >= rate && ticks < MAX_TIMER_CATCHUP_TICKS {
        cursor += rate;
        ticks += 1;
    }
    if ticks > 0 {
        *last_timer_instant = Some(cursor);
        for _ in 0..ticks {
            tick_global_timers();
        }
    }
}

/// One iteration of the main loop, shared by [`AppHost::step`] and the free-function run loops so
/// the tick/draw/poll/dispatch sequence exists exactly once.
///
/// The sequence is: advance timers (wall-clock) → `on_tick` → `drain_actions` → drain IPC → draw →
/// (headless: stop here) → poll/read one event → dispatch → Esc-cancel → quit check → `on_event`.
/// `drain_actions` is a no-op for run loops without an action channel.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn step_once(
    config: &CrosstermAppConfig,
    session: &mut HostSession,
    desktop: &mut Desktop,
    task_registry: &TaskRegistry,
    ipc_server: &mut Option<IpcServer>,
    last_timer_instant: &mut Option<Instant>,
    on_tick: Option<&mut dyn FnMut(&mut Desktop, Rect) -> Result<AppControl>>,
    on_event: Option<
        &mut dyn FnMut(&mut Desktop, &Event, Rect, &DesktopEventResult) -> Result<AppControl>,
    >,
    drain_actions: &mut dyn FnMut(&mut Desktop, Rect) -> Result<AppControl>,
) -> Result<AppControl> {
    let screen = session.screen()?;

    advance_global_timers(config, last_timer_instant);
    if let Some(handler) = on_tick
        && handler(desktop, screen)? == AppControl::Exit
    {
        return Ok(AppControl::Exit);
    }
    if drain_actions(desktop, screen)? == AppControl::Exit {
        return Ok(AppControl::Exit);
    }
    drain_ipc_server(ipc_server, desktop, screen);

    session.draw(desktop)?;

    if session.is_headless() {
        return Ok(AppControl::Continue);
    }

    if !event::poll(config.tick_rate)? {
        return Ok(AppControl::Continue);
    }

    let ev = event::read()?;
    let screen = session.screen()?;
    let mut result = desktop.handle_event(&ev, screen);
    handle_desktop_action(desktop, &result.action);
    mark_consumed_if_escape_cancelled(&ev, &mut result, task_registry);

    if should_quit_default(&ev, result.outcome) {
        return Ok(AppControl::Exit);
    }

    if let Some(handler) = on_event
        && handler(desktop, &ev, screen, &result)? == AppControl::Exit
    {
        return Ok(AppControl::Exit);
    }

    Ok(AppControl::Continue)
}

pub struct AppHost {
    config: CrosstermAppConfig,
    session: HostSession,
    desktop: Desktop,
    task_registry: TaskRegistry,
    ipc_server: Option<IpcServer>,
    on_tick: Option<Box<TickCallBack>>,
    on_event: Option<Box<EventCallBack>>,
    /// Wall-clock anchor for advancing global timers by elapsed time rather than
    /// once per `step`. Drivers like the React tick loop call `step` far more
    /// often than the tick rate, so a fixed one-tick-per-step would run timers
    /// (e.g. a spinner) at the loop frequency instead of real time.
    last_timer_instant: Option<Instant>,
}

impl AppHost {
    pub fn new<B>(config: CrosstermAppConfig, build: B) -> Result<Self>
    where
        B: FnOnce(Rect) -> Result<Desktop>,
    {
        let session = TerminalSession::new(config)?;
        let screen: Rect = session.terminal.size()?.into();
        let desktop = build(screen)?;
        set_global_tick_rate(config.tick_rate);
        Ok(Self {
            config,
            session: HostSession::Terminal(session),
            desktop,
            task_registry: TaskRegistry::new(),
            ipc_server: None,
            on_tick: None,
            on_event: None,
            last_timer_instant: None,
        })
    }

    pub fn new_headless<B>(screen: Rect, build: B) -> Result<Self>
    where
        B: FnOnce(Rect) -> Result<Desktop>,
    {
        let config = CrosstermAppConfig::default();
        let desktop = build(screen)?;
        set_global_tick_rate(config.tick_rate);
        Ok(Self {
            config,
            session: HostSession::Headless { screen },
            desktop,
            task_registry: TaskRegistry::new(),
            ipc_server: None,
            on_tick: None,
            on_event: None,
            last_timer_instant: None,
        })
    }

    pub fn desktop(&mut self) -> &mut Desktop {
        &mut self.desktop
    }

    pub fn desktop_ref(&self) -> &Desktop {
        &self.desktop
    }

    pub fn task_registry(&self) -> TaskRegistry {
        self.task_registry.clone()
    }

    pub fn screen(&self) -> Result<Rect> {
        self.session.screen()
    }

    pub fn enable_ipc(&mut self, socket_path: impl Into<PathBuf>) -> Result<()> {
        self.ipc_server = Some(IpcServer::bind(socket_path.into())?);
        Ok(())
    }

    pub fn enable_ipc_from_env(&mut self) -> Result<Option<PathBuf>> {
        let Some(config) = IpcServerConfig::from_env() else {
            return Ok(None);
        };
        let socket_path = config.socket_path().to_path_buf();
        self.ipc_server = Some(IpcServer::from_config(config)?);
        Ok(Some(socket_path))
    }

    pub fn disable_ipc(&mut self) {
        self.ipc_server = None;
    }

    pub fn ipc_socket_path(&self) -> Option<&Path> {
        self.ipc_server.as_ref().map(IpcServer::socket_path)
    }

    pub fn restore_terminal(&mut self) {
        self.session.restore_terminal();
    }

    pub fn send_event(&mut self, window_id: WindowId, event: Event) -> Result<DesktopEventResult> {
        let screen = self.screen()?;
        let mut result = self
            .desktop
            .send_event_to_window(window_id, event.clone(), screen);
        handle_desktop_action(&mut self.desktop, &result.action);
        mark_consumed_if_escape_cancelled(&event, &mut result, &self.task_registry);
        Ok(result)
    }

    pub fn close_window(&mut self, id: WindowId) -> bool {
        self.desktop.close_window(id)
    }

    pub fn focus_window(&mut self, id: WindowId) -> bool {
        self.desktop.focus_window(id)
    }

    pub fn move_window(&mut self, id: WindowId, x: u16, y: u16) -> Result<bool> {
        let screen = self.screen()?;
        Ok(self.desktop.move_window(id, x, y, screen))
    }

    pub fn resize_window(&mut self, id: WindowId, width: u16, height: u16) -> Result<bool> {
        let screen = self.screen()?;
        Ok(self.desktop.resize_window(id, width, height, screen))
    }

    pub fn minimize_window(&mut self, id: WindowId) -> bool {
        self.desktop.minimize_window(id)
    }

    pub fn restore_window(&mut self, id: WindowId) -> bool {
        self.desktop.restore_window(id)
    }

    pub fn maximize_window(&mut self, id: WindowId) -> Result<bool> {
        let screen = self.screen()?;
        Ok(self.desktop.maximize_window(id, screen))
    }

    pub fn cascade_windows(&mut self) -> Result<()> {
        let screen = self.screen()?;
        self.desktop.cascade_windows(screen);
        Ok(())
    }

    pub fn tile_windows(&mut self) -> Result<()> {
        let screen = self.screen()?;
        self.desktop.tile_windows(screen);
        Ok(())
    }

    pub fn minimize_all_windows(&mut self) {
        self.desktop.minimize_all_windows();
    }

    pub fn restore_all_windows(&mut self) {
        self.desktop.restore_all_windows();
    }

    pub fn close_all_windows(&mut self) {
        self.desktop.close_all_windows();
    }

    pub fn focus_next_window(&mut self) {
        self.desktop.focus_next_window();
    }

    pub fn focus_previous_window(&mut self) {
        self.desktop.focus_previous_window();
    }

    pub fn list_windows(&self) -> Vec<WindowInfo> {
        self.desktop.list_windows()
    }

    pub fn push_toast(&mut self, toast: Toast) {
        self.desktop.push_toast(toast);
    }

    pub fn push_toast_message(
        &mut self,
        level: ToastLevel,
        message: impl Into<String>,
        duration: Duration,
    ) {
        self.desktop
            .toasts
            .push_message(level, message.into(), duration);
    }

    pub fn notify_background_complete(&mut self, message: impl Into<String>) {
        self.desktop.notify_background_complete(message);
    }

    pub fn set_title(&mut self, id: WindowId, title: impl Into<String>) -> bool {
        self.desktop.set_title(id, title)
    }

    pub fn set_property(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        value: ComponentValue,
    ) -> std::result::Result<(), TreeError> {
        self.desktop.set_property(id, name, value)
    }

    pub fn get_property(
        &mut self,
        id: &str,
        name: &str,
    ) -> std::result::Result<ComponentValue, ComponentError> {
        DesktopInspector::new(&mut self.desktop).get_property(id, name)
    }

    pub fn snapshot(&mut self) -> Result<DesktopSnapshot> {
        let screen = self.screen()?;
        DesktopInspector::new(&mut self.desktop)
            .export_snapshot(screen)
            .map_err(|err| anyhow::anyhow!("{err:?}"))
    }

    pub fn set_on_tick<F>(&mut self, handler: F)
    where
        F: FnMut(&mut Desktop, Rect) -> Result<AppControl> + 'static,
    {
        self.on_tick = Some(Box::new(handler));
    }

    pub fn set_on_event<F>(&mut self, handler: F)
    where
        F: FnMut(&mut Desktop, &Event, Rect, &DesktopEventResult) -> Result<AppControl> + 'static,
    {
        self.on_event = Some(Box::new(handler));
    }

    pub fn step(&mut self) -> Result<AppControl> {
        // Disjoint field borrows so `on_tick`/`on_event` can be passed as `&mut dyn FnMut` while the
        // rest of `self` is borrowed by `step_once`.
        let Self {
            config,
            session,
            desktop,
            task_registry,
            ipc_server,
            on_tick,
            on_event,
            last_timer_instant,
        } = self;
        let on_tick = on_tick
            .as_mut()
            .map(|h| h.as_mut() as &mut dyn FnMut(&mut Desktop, Rect) -> Result<AppControl>);
        let on_event = on_event.as_mut().map(|h| {
            h.as_mut()
                as &mut dyn FnMut(
                    &mut Desktop,
                    &Event,
                    Rect,
                    &DesktopEventResult,
                ) -> Result<AppControl>
        });
        let mut drain_actions = |_desktop: &mut Desktop, _screen: Rect| Ok(AppControl::Continue);
        step_once(
            config,
            session,
            desktop,
            task_registry,
            ipc_server,
            last_timer_instant,
            on_tick,
            on_event,
            &mut drain_actions,
        )
    }

    pub fn run(&mut self) -> Result<()> {
        loop {
            if self.step()? == AppControl::Exit {
                break;
            }
        }
        Ok(())
    }
}

pub fn run_crossterm_desktop_simple<B>(config: CrosstermAppConfig, build: B) -> Result<()>
where
    B: FnOnce(Rect) -> Result<Desktop>,
{
    run_crossterm_desktop(
        config,
        build,
        |_desktop, _screen| Ok(AppControl::Continue),
        |_, _, _, _| Ok(AppControl::Continue),
    )
}

pub fn run_crossterm_desktop<B, TTick, TEvent>(
    config: CrosstermAppConfig,
    build: B,
    mut on_tick: TTick,
    mut on_event: TEvent,
) -> Result<()>
where
    B: FnOnce(Rect) -> Result<Desktop>,
    TTick: FnMut(&mut Desktop, Rect) -> Result<AppControl>,
    TEvent: FnMut(&mut Desktop, &Event, Rect, &DesktopEventResult) -> Result<AppControl>,
{
    let session = TerminalSession::new(config)?;
    let screen: Rect = session.terminal.size()?.into();
    let mut session = HostSession::Terminal(session);
    let mut desktop = build(screen)?;
    let mut ipc_server = IpcServer::from_env()?;
    let task_registry = TaskRegistry::new();
    let mut last_timer_instant = None;
    set_global_tick_rate(config.tick_rate);

    let mut drain_actions = |_desktop: &mut Desktop, _screen: Rect| Ok(AppControl::Continue);
    loop {
        if step_once(
            &config,
            &mut session,
            &mut desktop,
            &task_registry,
            &mut ipc_server,
            &mut last_timer_instant,
            Some(&mut on_tick),
            Some(&mut on_event),
            &mut drain_actions,
        )? == AppControl::Exit
        {
            break;
        }
    }

    Ok(())
}

/// Runs a crossterm-backed desktop UI, draining background actions from a channel each frame.
///
/// This is the standard-library-only integration point for receiving async events (network,
/// timers, worker threads, etc.) and dispatching them onto the main UI thread.
///
/// The channel is drained before each draw, so actions can update UI state immediately.
pub fn run_crossterm_desktop_with_actions<B, TTick, TEvent, TAction, A>(
    config: CrosstermAppConfig,
    build: B,
    action_receiver: mpsc::Receiver<A>,
    on_action: TAction,
    on_tick: TTick,
    on_event: TEvent,
) -> Result<()>
where
    B: FnOnce(Rect) -> Result<Desktop>,
    TAction: FnMut(&mut Desktop, A, Rect) -> Result<AppControl>,
    TTick: FnMut(&mut Desktop, Rect) -> Result<AppControl>,
    TEvent: FnMut(&mut Desktop, &Event, Rect, &DesktopEventResult) -> Result<AppControl>,
{
    run_crossterm_desktop_with_actions_and_tasks(
        config,
        build,
        action_receiver,
        TaskRegistry::new(),
        on_action,
        on_tick,
        on_event,
    )
}

/// Runs a crossterm-backed desktop UI with a shared task registry for Esc cancellation.
pub fn run_crossterm_desktop_with_actions_and_tasks<B, TTick, TEvent, TAction, A>(
    config: CrosstermAppConfig,
    build: B,
    action_receiver: mpsc::Receiver<A>,
    task_registry: TaskRegistry,
    mut on_action: TAction,
    mut on_tick: TTick,
    mut on_event: TEvent,
) -> Result<()>
where
    B: FnOnce(Rect) -> Result<Desktop>,
    TAction: FnMut(&mut Desktop, A, Rect) -> Result<AppControl>,
    TTick: FnMut(&mut Desktop, Rect) -> Result<AppControl>,
    TEvent: FnMut(&mut Desktop, &Event, Rect, &DesktopEventResult) -> Result<AppControl>,
{
    let session = TerminalSession::new(config)?;
    let screen: Rect = session.terminal.size()?.into();
    let mut session = HostSession::Terminal(session);
    let mut desktop = build(screen)?;
    let mut ipc_server = IpcServer::from_env()?;
    let mut last_timer_instant = None;
    set_global_tick_rate(config.tick_rate);

    // Drain background actions before drawing so their effects are visible immediately. Runs in the
    // post-`on_tick`, pre-IPC/draw slot of `step_once`, matching the original ordering.
    let mut drain_actions = |desktop: &mut Desktop, screen: Rect| {
        while let Ok(action) = action_receiver.try_recv() {
            if on_action(desktop, action, screen)? == AppControl::Exit {
                return Ok(AppControl::Exit);
            }
        }
        Ok(AppControl::Continue)
    };

    loop {
        if step_once(
            &config,
            &mut session,
            &mut desktop,
            &task_registry,
            &mut ipc_server,
            &mut last_timer_instant,
            Some(&mut on_tick),
            Some(&mut on_event),
            &mut drain_actions,
        )? == AppControl::Exit
        {
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::MenuBar;
    use crate::composable::{
        Component, ComponentContext, ComponentTagExt, EventHandling, EventOutcome, EventResult,
        Label,
    };
    use crate::reactive::GLOBAL_TIMER_TEST_GUARD;
    use crate::theme::Theme;

    /// Serialize with every other test touching the process-global timer wheel / tick rate. Every
    /// `AppHost` here ticks the global wheel and sets the global tick rate, so parallel runs would
    /// otherwise perturb the timing-sensitive `step_advances_timers_by_real_elapsed_time`.
    fn timer_test_guard() -> parking_lot::MutexGuard<'static, ()> {
        GLOBAL_TIMER_TEST_GUARD.lock()
    }
    use crate::wm::{Window, WindowKind};

    struct ConsumeEscView;

    impl Component for ConsumeEscView {
        fn draw(
            &mut self,
            _frame: &mut ratatui::Frame<'_>,
            _area: Rect,
            _ctx: ComponentContext<'_>,
        ) {
        }
    }

    impl EventHandling for ConsumeEscView {
        fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
            if is_escape_press(event) {
                EventResult::consumed()
            } else {
                EventResult::ignored()
            }
        }
    }

    crate::impl_component_default_traits!(ConsumeEscView => Layout, Scrollable, FocusNav, DynamicTree);

    #[test]
    fn headless_apphost_snapshot_uses_in_memory_layout() {
        let _guard = timer_test_guard();
        let screen = Rect::new(0, 0, 80, 24);
        let mut host = AppHost::new_headless(screen, |screen| {
            let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
            let window = Window::new(
                WindowKind::Normal,
                "Headless",
                Rect::new(2, 2, 24, 6),
                Box::new(Label::new("Hello").tag("message")),
            )
            .with_tag("win");
            desktop.add_window(window, screen);
            Ok(desktop)
        })
        .expect("headless host");

        assert_eq!(host.screen().expect("screen"), screen);
        assert_eq!(host.step().expect("headless step"), AppControl::Continue);

        let snapshot = host.snapshot().expect("snapshot");
        let label = snapshot.tree.find_by_id("message").expect("message node");
        assert_eq!(label.text.as_deref(), Some("Hello"));
        assert_eq!(
            label.bounds,
            Some(crate::runtime::Rect {
                x: 3,
                y: 3,
                width: 22,
                height: 4,
            })
        );
    }

    #[test]
    fn pointer_capture_routes_release_outside_button_without_click() {
        let _guard = timer_test_guard();
        use crate::widgets::Button;
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_button = Arc::clone(&calls);
        let screen = Rect::new(0, 0, 80, 24);
        let mut window_id = None;
        let mut host = AppHost::new_headless(screen, |screen| {
            let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
            let id = desktop.add_window(
                Window::new(
                    WindowKind::Normal,
                    "Capture",
                    Rect::new(2, 2, 28, 7),
                    Box::new(Button::new("Fire").on_click(move || {
                        calls_for_button.fetch_add(1, Ordering::SeqCst);
                    })),
                ),
                screen,
            );
            window_id = Some(id);
            Ok(desktop)
        })
        .expect("headless host");
        let window_id = window_id.expect("window id");

        // Lay out and draw so the button records its on-screen area.
        host.step().expect("step");

        let down = |col, row| {
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: col,
                row,
                modifiers: KeyModifiers::NONE,
            })
        };
        let up = |col, row| {
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                column: col,
                row,
                modifiers: KeyModifiers::NONE,
            })
        };

        // Press inside the button (window-relative coords): no click yet, capture armed.
        let res = host.send_event(window_id, down(2, 2)).expect("down");
        assert_eq!(res.outcome, EventOutcome::Consumed);
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        // Release well outside the button's area (below and past the window). This
        // only reaches the button if pointer capture routes the out-of-bounds event
        // back to it; releasing outside the button must NOT count as a click.
        let res = host.send_event(window_id, up(5, 20)).expect("up outside");
        assert_eq!(res.outcome, EventOutcome::Consumed);
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        // A clean press+release at the same point is a click, and capture is cleared
        // afterwards (the next stray move is ignored by the button).
        host.send_event(window_id, down(2, 2)).expect("down again");
        host.send_event(window_id, up(2, 2)).expect("up inside");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn step_advances_timers_by_real_elapsed_time() {
        let _guard = timer_test_guard();
        use crate::reactive::{cancel_timer, register_timer_with_duration};
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let fired = Arc::new(AtomicUsize::new(0));
        let counter = fired.clone();
        let handle = register_timer_with_duration(Duration::from_millis(20), move || {
            counter.fetch_add(1, Ordering::SeqCst);
            true
        });

        let screen = Rect::new(0, 0, 20, 5);
        let mut host = AppHost::new_headless(screen, |_screen| {
            Ok(Desktop::new(Theme::dark(), MenuBar::new(vec![])))
        })
        .expect("headless host");

        // The first step only anchors the timer clock (no catch-up burst).
        host.step().expect("step");
        let baseline = fired.load(Ordering::SeqCst);

        // After real time elapses, a single step dispatches the elapsed ticks, so
        // the 20ms timer fires regardless of how often step is called. This is what
        // keeps a spinner animating under the React tick loop (which calls step far
        // more often than the tick rate).
        std::thread::sleep(Duration::from_millis(120));
        host.step().expect("step");
        let after = fired.load(Ordering::SeqCst);

        cancel_timer(handle);
        assert!(
            after > baseline,
            "timer should fire after elapsed wall-clock time (baseline {baseline}, after {after})"
        );
    }

    #[test]
    fn apphost_escape_cancels_current_task_when_event_is_ignored() {
        let _guard = timer_test_guard();
        let screen = Rect::new(0, 0, 80, 24);
        let mut window_id = None;
        let mut host = AppHost::new_headless(screen, |screen| {
            let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
            let id = desktop.add_window(
                Window::new(
                    WindowKind::Normal,
                    "Task",
                    Rect::new(2, 2, 24, 6),
                    Box::new(Label::new("Task")),
                ),
                screen,
            );
            window_id = Some(id);
            Ok(desktop)
        })
        .expect("headless host");
        let window_id = window_id.expect("window id");
        let registry = host.task_registry();
        let running = registry.running_property();
        let handle = registry.register("background");

        let result = host
            .send_event(
                window_id,
                Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            )
            .expect("send Esc");

        assert_eq!(result.outcome, EventOutcome::Consumed);
        assert!(handle.is_cancelled());
        assert!(running.get());

        assert!(registry.unregister(handle.id()));
        assert!(!running.get());
    }

    #[test]
    fn apphost_escape_does_not_cancel_task_when_view_consumes_event() {
        let _guard = timer_test_guard();
        let screen = Rect::new(0, 0, 80, 24);
        let mut window_id = None;
        let mut host = AppHost::new_headless(screen, |screen| {
            let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
            let id = desktop.add_window(
                Window::new(
                    WindowKind::Normal,
                    "Task",
                    Rect::new(2, 2, 24, 6),
                    Box::new(ConsumeEscView),
                ),
                screen,
            );
            window_id = Some(id);
            Ok(desktop)
        })
        .expect("headless host");
        let window_id = window_id.expect("window id");
        let registry = host.task_registry();
        let handle = registry.register("background");

        let result = host
            .send_event(
                window_id,
                Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            )
            .expect("send Esc");

        assert_eq!(result.outcome, EventOutcome::Consumed);
        assert!(!handle.is_cancelled());
    }
}
