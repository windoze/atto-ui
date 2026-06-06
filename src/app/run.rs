use std::io;
use std::sync::mpsc;
use std::time::Duration;

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
use crate::composable::EventOutcome;
use crate::inspect::{DesktopInspector, DesktopSnapshot};
use crate::reactive::{set_global_tick_rate, tick_global_timers};
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

pub struct AppHost {
    config: CrosstermAppConfig,
    session: HostSession,
    desktop: Desktop,
    task_registry: TaskRegistry,
    on_tick: Option<Box<TickCallBack>>,
    on_event: Option<Box<EventCallBack>>,
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
            on_tick: None,
            on_event: None,
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
            on_tick: None,
            on_event: None,
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

    pub fn list_windows(&self) -> Vec<WindowInfo> {
        self.desktop.list_windows()
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
        let screen = self.screen()?;

        tick_global_timers();
        if let Some(handler) = self.on_tick.as_mut()
            && handler(&mut self.desktop, screen)? == AppControl::Exit
        {
            return Ok(AppControl::Exit);
        }

        self.session.draw(&mut self.desktop)?;

        if self.session.is_headless() {
            return Ok(AppControl::Continue);
        }

        if !event::poll(self.config.tick_rate)? {
            return Ok(AppControl::Continue);
        }

        let ev = event::read()?;
        let screen = self.screen()?;
        let mut result = self.desktop.handle_event(&ev, screen);
        handle_desktop_action(&mut self.desktop, &result.action);
        mark_consumed_if_escape_cancelled(&ev, &mut result, &self.task_registry);

        if should_quit_default(&ev, result.outcome) {
            return Ok(AppControl::Exit);
        }

        if let Some(handler) = self.on_event.as_mut()
            && handler(&mut self.desktop, &ev, screen, &result)? == AppControl::Exit
        {
            return Ok(AppControl::Exit);
        }

        Ok(AppControl::Continue)
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
    let mut session = TerminalSession::new(config)?;
    let screen: Rect = session.terminal.size()?.into();
    let mut desktop = build(screen)?;
    set_global_tick_rate(config.tick_rate);

    loop {
        let screen: Rect = session.terminal.size()?.into();

        tick_global_timers();
        if on_tick(&mut desktop, screen)? == AppControl::Exit {
            break;
        }

        session.terminal.draw(|f| desktop.draw(f))?;

        if !event::poll(config.tick_rate)? {
            continue;
        }

        let ev = event::read()?;
        let screen: Rect = session.terminal.size()?.into();
        let result = desktop.handle_event(&ev, screen);
        handle_desktop_action(&mut desktop, &result.action);

        if should_quit_default(&ev, result.outcome) {
            break;
        }
        if on_event(&mut desktop, &ev, screen, &result)? == AppControl::Exit {
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
    let mut session = TerminalSession::new(config)?;
    let screen: Rect = session.terminal.size()?.into();
    let mut desktop = build(screen)?;
    set_global_tick_rate(config.tick_rate);

    loop {
        let screen: Rect = session.terminal.size()?.into();

        tick_global_timers();
        if on_tick(&mut desktop, screen)? == AppControl::Exit {
            break;
        }

        // Drain background actions before drawing so their effects are visible immediately.
        while let Ok(action) = action_receiver.try_recv() {
            if on_action(&mut desktop, action, screen)? == AppControl::Exit {
                return Ok(());
            }
        }

        session.terminal.draw(|f| desktop.draw(f))?;

        if !event::poll(config.tick_rate)? {
            continue;
        }

        let ev = event::read()?;
        let screen: Rect = session.terminal.size()?.into();
        let mut result = desktop.handle_event(&ev, screen);
        handle_desktop_action(&mut desktop, &result.action);
        mark_consumed_if_escape_cancelled(&ev, &mut result, &task_registry);

        if should_quit_default(&ev, result.outcome) {
            break;
        }
        if on_event(&mut desktop, &ev, screen, &result)? == AppControl::Exit {
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
    use crate::theme::Theme;
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
    fn apphost_escape_cancels_current_task_when_event_is_ignored() {
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
