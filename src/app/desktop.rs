use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::app::status::Fill;
use crate::theme::Theme;
use crate::view::EventOutcome;
use crate::view::ViewAction;
use crate::wm::{Window, WindowId, WindowManager, WindowManagerInputMode};

use super::menu::{MenuAction, MenuBar};
use super::status::StatusBar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopMode {
    Normal,
    Menu,
    WindowManagement,
}

#[derive(Clone, Debug)]
pub enum DesktopAction {
    None,
    MenuCommand(String),
    CloseWindow(WindowId),
}

#[derive(Clone, Debug)]
pub struct DesktopEventResult {
    pub outcome: EventOutcome,
    pub action: DesktopAction,
}

impl DesktopEventResult {
    pub const fn ignored() -> Self {
        Self {
            outcome: EventOutcome::Ignored,
            action: DesktopAction::None,
        }
    }

    pub const fn consumed() -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: DesktopAction::None,
        }
    }

    pub fn menu_command(cmd: String) -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: DesktopAction::MenuCommand(cmd),
        }
    }

    pub const fn close_window(id: WindowId) -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: DesktopAction::CloseWindow(id),
        }
    }

    pub const fn is_consumed(&self) -> bool {
        matches!(self.outcome, EventOutcome::Consumed)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DesktopLayout {
    pub menu_bar: Rect,
    pub work_area: Rect,
    pub status_bar: Rect,
}

pub struct Desktop {
    pub theme: Theme,
    pub wm: WindowManager,
    pub menu: MenuBar,
    pub status: StatusBar,
    pub mode: DesktopMode,
}

impl Desktop {
    pub fn new(theme: Theme, menu: MenuBar) -> Self {
        Self {
            theme,
            wm: WindowManager::new(),
            menu,
            status: StatusBar::default(),
            mode: DesktopMode::Normal,
        }
    }

    pub fn layout(area: Rect) -> DesktopLayout {
        let menu_bar = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1.min(area.height),
        };
        let status_bar = if area.height >= 2 {
            Rect {
                x: area.x,
                y: area.y + area.height - 1,
                width: area.width,
                height: 1,
            }
        } else {
            Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 0,
            }
        };
        let work_area = if area.height >= 3 {
            Rect {
                x: area.x,
                y: area.y + 1,
                width: area.width,
                height: area.height - 2,
            }
        } else {
            Rect {
                x: area.x,
                y: area.y.saturating_add(1),
                width: area.width,
                height: area.height.saturating_sub(1),
            }
        };
        DesktopLayout {
            menu_bar,
            work_area,
            status_bar,
        }
    }

    pub fn add_window(&mut self, window: Window, screen: Rect) -> WindowId {
        let layout = Self::layout(screen);
        self.wm.add_window(window, layout.work_area)
    }

    pub fn handle_event(&mut self, event: &Event, screen: Rect) -> DesktopEventResult {
        let layout = Self::layout(screen);

        // Menu captures all input while active.
        if self.mode == DesktopMode::Menu || self.menu.is_active() {
            if self.mode != DesktopMode::Menu {
                self.mode = DesktopMode::Menu;
            }
            let action = match self.menu.handle_event(event) {
                MenuAction::None => DesktopAction::None,
                MenuAction::Closed => {
                    self.mode = DesktopMode::Normal;
                    self.menu.deactivate();
                    DesktopAction::None
                }
                MenuAction::Command(cmd) => {
                    self.mode = DesktopMode::Normal;
                    self.menu.deactivate();
                    DesktopAction::MenuCommand(cmd)
                }
            };
            return DesktopEventResult {
                outcome: EventOutcome::Consumed,
                action,
            };
        }

        let modal_active = self.wm.has_active_modal();

        let input_mode = if self.mode == DesktopMode::WindowManagement {
            WindowManagerInputMode::WindowManagement
        } else {
            WindowManagerInputMode::Normal
        };

        let mut view_dispatched = false;

        // Layered input:
        //  1. Focused view receives the event (normal mode only; keys/paste/etc).
        //  2. Focused window (WindowManager) receives the event.
        //  3. Desktop receives the event (global shortcuts), unless a modal is open.
        if input_mode == WindowManagerInputMode::Normal && !matches!(event, Event::Mouse(_)) {
            view_dispatched = true;
            if let Some((id, res)) =
                self.wm
                    .dispatch_to_focused_view(event, layout.work_area, &self.theme)
            {
                if res.action == ViewAction::CloseWindow {
                    self.wm.close(id);
                    return DesktopEventResult::close_window(id);
                }
                if res.is_consumed() {
                    return DesktopEventResult::consumed();
                }
            }
        }

        let wm_action = self.wm.handle_event(event, layout.work_area, input_mode);
        if let Some(id) = wm_action.close {
            self.wm.close(id);
            return DesktopEventResult::close_window(id);
        }
        if wm_action.consumed {
            return DesktopEventResult::consumed();
        }

        // Mouse events need to hit-test and potentially change focus before dispatching to the view,
        // so we dispatch them after the WindowManager.
        if input_mode == WindowManagerInputMode::Normal
            && !view_dispatched
            && let Some((id, res)) =
                self.wm
                    .dispatch_to_focused_view(event, layout.work_area, &self.theme)
        {
            if res.action == ViewAction::CloseWindow {
                self.wm.close(id);
                return DesktopEventResult::close_window(id);
            }
            if res.is_consumed() {
                return DesktopEventResult::consumed();
            }
        }

        // Modals act as an event sink: even if the modal view ignores an event, it should not
        // propagate to desktop-level shortcuts.
        if modal_active {
            return DesktopEventResult::consumed();
        }

        // Desktop-level shortcuts (press only).
        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            if *code == KeyCode::F(10) {
                self.mode = DesktopMode::Menu;
                self.menu.activate();
                return DesktopEventResult::consumed();
            }

            if *code == KeyCode::Char('w') && modifiers.contains(KeyModifiers::CONTROL) {
                self.menu.deactivate();
                self.mode = if self.mode == DesktopMode::WindowManagement {
                    DesktopMode::Normal
                } else {
                    DesktopMode::WindowManagement
                };
                return DesktopEventResult::consumed();
            }

            if *code == KeyCode::Esc && self.mode != DesktopMode::Normal {
                self.mode = DesktopMode::Normal;
                self.menu.deactivate();
                return DesktopEventResult::consumed();
            }
        }

        DesktopEventResult::ignored()
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let layout = Self::layout(area);

        frame.render_widget(
            Fill {
                style: self.theme.desktop,
                ch: ' ',
            },
            area,
        );

        // Draw windows before chrome overlays so dropdown menus/tooltips can render on top.
        self.wm.draw(frame, layout.work_area, &self.theme);

        self.menu.draw(frame, layout.menu_bar, &self.theme);

        let status_left = match self.mode {
            DesktopMode::Normal => "F10 Menu  Ctrl+W Window  F6 Next",
            DesktopMode::Menu => "Menu: ←/→/↑/↓ Enter  Esc Close",
            DesktopMode::WindowManagement => {
                "Window: arrows move  Shift+arrows resize  c close  x max  m min  Esc exit"
            }
        };
        self.status.set_left(status_left);
        let focused = self
            .wm
            .focused()
            .map(|id| format!("Focus: {:?}", id.0))
            .unwrap_or_else(|| "Focus: none".to_string());
        self.status.set_right(focused);
        self.status.draw(frame, layout.status_bar, &self.theme);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use crate::view::{View, ViewContext, ViewEventResult};
    use crate::wm::{Window, WindowKind};
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
    struct ConsumeF6View;

    impl View for ConsumeF6View {
        fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
            if let Event::Key(KeyEvent { code, .. }) = event
                && *code == KeyCode::F(6)
            {
                return ViewEventResult::consumed();
            }
            ViewEventResult::ignored()
        }

        fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ViewContext<'_>) {}
    }

    #[derive(Clone)]
    struct CountingMouseView {
        downs: Arc<AtomicUsize>,
    }

    impl CountingMouseView {
        fn new(downs: Arc<AtomicUsize>) -> Self {
            Self { downs }
        }
    }

    impl View for CountingMouseView {
        fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
            if let Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                ..
            }) = event
            {
                self.downs.fetch_add(1, Ordering::SeqCst);
                return ViewEventResult::consumed();
            }
            ViewEventResult::ignored()
        }

        fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ViewContext<'_>) {}
    }

    #[test]
    fn focused_view_can_consume_event_before_window_manager() {
        let screen = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));

        let _id1 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "One",
                Rect {
                    x: 2,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(ConsumeF6View),
            ),
            screen,
        );
        let id2 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Two",
                Rect {
                    x: 25,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(ConsumeF6View),
            ),
            screen,
        );

        assert_eq!(desktop.wm.focused(), Some(id2));
        let result = desktop.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)),
            screen,
        );
        assert!(result.is_consumed());
        assert_eq!(
            desktop.wm.focused(),
            Some(id2),
            "expected view consumption to prevent WindowManager focus cycling"
        );
    }

    #[test]
    fn ignored_view_event_bubbles_to_window_manager() {
        struct IgnoreAllView;

        impl View for IgnoreAllView {
            fn handle_event(&mut self, _event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
                ViewEventResult::ignored()
            }

            fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ViewContext<'_>) {}
        }

        let screen = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));

        let id1 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "One",
                Rect {
                    x: 2,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(IgnoreAllView),
            ),
            screen,
        );
        let id2 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Two",
                Rect {
                    x: 25,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(IgnoreAllView),
            ),
            screen,
        );

        assert_eq!(desktop.wm.focused(), Some(id2));
        let result = desktop.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)),
            screen,
        );
        assert!(result.is_consumed());
        assert_eq!(
            desktop.wm.focused(),
            Some(id1),
            "expected unhandled F6 to bubble to WindowManager focus_next"
        );
    }

    #[test]
    fn modal_window_blocks_desktop_shortcuts() {
        struct IgnoreAllView;

        impl View for IgnoreAllView {
            fn handle_event(&mut self, _event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
                ViewEventResult::ignored()
            }

            fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ViewContext<'_>) {}
        }

        let screen = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));

        let _normal_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Normal",
                Rect {
                    x: 2,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(IgnoreAllView),
            ),
            screen,
        );
        let modal_id = desktop.add_window(
            Window::new(
                WindowKind::Modal,
                "Modal",
                Rect {
                    x: 10,
                    y: 8,
                    width: 30,
                    height: 8,
                },
                Box::new(IgnoreAllView),
            ),
            screen,
        );

        assert!(desktop.wm.has_active_modal());
        assert_eq!(desktop.wm.focused(), Some(modal_id));

        let result = desktop.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            screen,
        );
        assert!(result.is_consumed());
        assert_eq!(
            desktop.mode,
            DesktopMode::Normal,
            "expected Ctrl+W to be blocked while a modal is open"
        );

        let result = desktop.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE)),
            screen,
        );
        assert!(result.is_consumed());
        assert_eq!(
            desktop.mode,
            DesktopMode::Normal,
            "expected F10 to be blocked while a modal is open"
        );
        assert!(
            !desktop.menu.is_active(),
            "expected menu to remain inactive"
        );
    }

    #[test]
    fn mouse_body_click_dispatches_to_focused_view() {
        let screen = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));

        let clicks_one = Arc::new(AtomicUsize::new(0));
        let clicks_two = Arc::new(AtomicUsize::new(0));

        let id1 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "One",
                Rect {
                    x: 2,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(CountingMouseView::new(Arc::clone(&clicks_one))),
            ),
            screen,
        );
        let id2 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Two",
                Rect {
                    x: 25,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(CountingMouseView::new(Arc::clone(&clicks_two))),
            ),
            screen,
        );

        assert_eq!(desktop.wm.focused(), Some(id2));

        // Click inside window "One" body (not the title bar).
        let result = desktop.handle_event(
            &Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 3,
                row: 3,
                modifiers: KeyModifiers::NONE,
            }),
            screen,
        );
        assert!(result.is_consumed());

        assert_eq!(desktop.wm.focused(), Some(id1));
        assert_eq!(clicks_one.load(Ordering::SeqCst), 1);
        assert_eq!(clicks_two.load(Ordering::SeqCst), 0);
    }
}
