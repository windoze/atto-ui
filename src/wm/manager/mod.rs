use ratatui::layout::Rect;

use crate::composable::scroll::ScrollbarDrag;

use super::{
    Window, WindowBorderStyle, WindowButtons, WindowId, WindowKind, WindowMinSizeMode, WindowState,
};

mod chrome;
mod draw;
mod events;
mod focus;
mod placement;
mod z_order;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowManagerInputMode {
    Normal,
    WindowManagement,
}

#[derive(Debug, Default)]
pub struct WindowManagerAction {
    pub consumed: bool,
    pub close: Option<WindowId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResizeCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug)]
enum DragKind {
    Move {
        offset_x: u16,
        offset_y: u16,
    },
    Resize {
        start_rect: Rect,
        corner: ResizeCorner,
    },
    Scrollbar {
        drag: ScrollbarDrag,
    },
}

#[derive(Clone, Copy, Debug)]
struct DragState {
    window_id: WindowId,
    kind: DragKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HitRegion {
    TitleBar,
    MinimizeButton,
    MaximizeButton,
    CloseButton,
    ResizeHandle(ResizeCorner),
    VScrollbar,
    HScrollbar,
    Body,
}

#[derive(Clone, Copy, Debug)]
struct HitTest {
    window_id: WindowId,
    region: HitRegion,
}

#[derive(Default)]
pub struct WindowManager {
    next_id: u64,
    windows: Vec<Window>,
    focused: Option<WindowId>,
    drag: Option<DragState>,
    mouse_capture: bool,
}

impl WindowManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    pub fn windows_mut(&mut self) -> &mut [Window] {
        &mut self.windows
    }

    pub fn add_window(&mut self, mut window: Window, bounds: Rect) -> WindowId {
        self.next_id += 1;
        let id = WindowId(self.next_id);
        window.id = id;

        let enforced_min_size = placement::window_enforced_min_size(&window);

        let rect = placement::normalize_rect(window.rect.get(), bounds, enforced_min_size);
        window.rect.set(rect);

        if window.kind == WindowKind::Modal {
            // Ensure modals are always on top and focused.
            self.focused = Some(id);
        } else if window.kind.is_focusable() {
            self.focused = Some(id);
        }

        self.windows.push(window);
        self.bring_to_front(id);
        id
    }

    pub fn close(&mut self, id: WindowId) {
        self.drag = match self.drag {
            Some(d) if d.window_id == id => None,
            other => other,
        };
        self.mouse_capture = false;
        let was_focused = self.focused == Some(id);
        self.windows.retain(|w| w.id != id);
        if was_focused {
            self.focused = self.topmost_focusable_id();
        }
    }

    pub fn request_close(&mut self, id: WindowId) -> bool {
        let allow = {
            let Some(w) = self.window_mut(id) else {
                return false;
            };
            w.allow_close()
        };
        if allow {
            self.close(id);
            true
        } else {
            false
        }
    }

    pub fn set_view(&mut self, id: WindowId, view: Box<dyn crate::composable::Component>) -> bool {
        let Some(window) = self.window_mut(id) else {
            return false;
        };
        window.set_view(view);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::WindowManager;
    use super::draw::draw_shadow;
    use crate::automation::AutomationValue;
    use crate::composable::{
        Component, ComponentContext, EventResult, Label, ScrollConfig, ScrollbarVisibility,
    };
    use crate::theme::Theme;
    use crate::wm::{Window, WindowBorderStyle, WindowKind, WindowMinSizeMode};
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton,
        MouseEvent, MouseEventKind,
    };
    use ratatui::Frame;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::{Color, Style};

    #[derive(Default)]
    struct DummyView;

    impl Component for DummyView {
        fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
        fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
            EventResult::ignored()
        }
    }

    struct MinSizeView {
        min: (u16, u16),
    }

    impl Component for MinSizeView {
        fn min_width(&self) -> u16 {
            self.min.0
        }

        fn min_height(&self) -> u16 {
            self.min.1
        }

        fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
    }

    #[test]
    fn window_manager_can_replace_view() {
        let bounds = Rect::new(0, 0, 80, 24);
        let mut wm = WindowManager::new();
        let id = wm.add_window(
            Window::new(
                WindowKind::Normal,
                "Win",
                Rect::new(1, 1, 10, 5),
                Box::new(Label::new("A")),
            ),
            bounds,
        );
        let before = wm
            .window_mut(id)
            .and_then(|w| w.view.automation_get_property("text"))
            .expect("before text");
        assert_eq!(before, AutomationValue::String("A".into()));

        assert!(wm.set_view(id, Box::new(Label::new("B"))));
        let after = wm
            .window_mut(id)
            .and_then(|w| w.view.automation_get_property("text"))
            .expect("after text");
        assert_eq!(after, AutomationValue::String("B".into()));
    }

    #[test]
    fn window_min_size_mode_enforce_clamps_to_content_min_size() {
        let bounds = Rect::new(0, 0, 80, 24);
        let mut wm = WindowManager::new();
        let id = wm.add_window(
            Window::new(
                WindowKind::Normal,
                "Enforce",
                Rect::new(1, 1, 6, 4),
                Box::new(MinSizeView { min: (30, 10) }),
            ),
            bounds,
        );

        let rect = wm.window_mut(id).expect("window").rect.get();
        // Border chrome consumes 1 cell on each side; the inner rect must still satisfy the view's
        // minimum size.
        assert_eq!(rect.width, 32);
        assert_eq!(rect.height, 12);
    }

    #[test]
    fn window_min_size_mode_clip_allows_shrinking_below_content_min_size() {
        let bounds = Rect::new(0, 0, 80, 24);
        let mut wm = WindowManager::new();
        let id = wm.add_window(
            Window::new(
                WindowKind::Normal,
                "Clip",
                Rect::new(1, 1, 6, 4),
                Box::new(MinSizeView { min: (30, 10) }),
            )
            .with_min_size_mode(WindowMinSizeMode::Clip),
            bounds,
        );

        let rect = wm.window_mut(id).expect("window").rect.get();
        assert_eq!(rect.width, 6);
        assert_eq!(rect.height, 4);
    }

    #[test]
    fn window_min_size_mode_scroll_allows_shrinking_below_content_min_size() {
        let bounds = Rect::new(0, 0, 80, 24);
        let mut wm = WindowManager::new();
        let id = wm.add_window(
            Window::new(
                WindowKind::Normal,
                "Scroll",
                Rect::new(1, 1, 6, 4),
                Box::new(MinSizeView { min: (30, 10) }),
            )
            .with_min_size_mode(WindowMinSizeMode::Scroll),
            bounds,
        );

        let rect = wm.window_mut(id).expect("window").rect.get();
        assert_eq!(rect.width, 6);
        assert_eq!(rect.height, 4);
    }

    #[test]
    fn window_min_size_mode_scroll_renders_window_border_scrollbars_on_overflow() {
        let bounds = Rect::new(0, 0, 80, 24);
        let rect = Rect::new(2, 2, 10, 6);
        let mut wm = WindowManager::new();
        wm.add_window(
            Window::new(
                WindowKind::Normal,
                "Scrollbars",
                rect,
                Box::new(MinSizeView { min: (30, 10) }),
            )
            .with_min_size_mode(WindowMinSizeMode::Scroll),
            bounds,
        );

        let theme = Theme::dark();
        let backend = TestBackend::new(bounds.width, bounds.height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

        let buf = terminal.backend().buffer();

        let left = rect.x;
        let top = rect.y;
        let right = rect.x.saturating_add(rect.width).saturating_sub(1);
        let bottom = rect.y.saturating_add(rect.height).saturating_sub(1);

        assert_eq!(buf.cell((right, top + 1)).expect("vbar up").symbol(), "▲");
        assert_eq!(
            buf.cell((right, bottom - 1)).expect("vbar down").symbol(),
            "▼"
        );
        assert_eq!(
            buf.cell((left + 1, bottom)).expect("hbar left").symbol(),
            "◄"
        );
        assert_eq!(
            buf.cell((right - 1, bottom)).expect("hbar right").symbol(),
            "►"
        );
    }

    #[test]
    fn window_min_size_mode_clip_does_not_render_window_border_scrollbars_on_overflow() {
        let bounds = Rect::new(0, 0, 80, 24);
        let rect = Rect::new(2, 2, 10, 6);
        let mut wm = WindowManager::new();
        wm.add_window(
            Window::new(
                WindowKind::Normal,
                "NoScrollbars",
                rect,
                Box::new(MinSizeView { min: (30, 10) }),
            )
            .with_min_size_mode(WindowMinSizeMode::Clip),
            bounds,
        );

        let theme = Theme::dark();
        let backend = TestBackend::new(bounds.width, bounds.height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

        let buf = terminal.backend().buffer();

        let left = rect.x;
        let top = rect.y;
        let right = rect.x.saturating_add(rect.width).saturating_sub(1);
        let bottom = rect.y.saturating_add(rect.height).saturating_sub(1);

        assert_eq!(
            buf.cell((right, top + 1)).expect("right border").symbol(),
            "║"
        );
        assert_eq!(
            buf.cell((right, bottom - 1))
                .expect("right border below")
                .symbol(),
            "║"
        );
        assert_eq!(
            buf.cell((left + 1, bottom))
                .expect("bottom border left")
                .symbol(),
            "═"
        );
        assert_eq!(
            buf.cell((right - 1, bottom))
                .expect("bottom border right")
                .symbol(),
            "═"
        );
    }

    #[test]
    fn window_min_size_mode_scroll_consumes_arrow_key_pans() {
        let bounds = Rect::new(0, 0, 80, 24);
        let rect = Rect::new(2, 2, 10, 6);
        let mut wm = WindowManager::new();
        wm.add_window(
            Window::new(
                WindowKind::Normal,
                "Pan",
                rect,
                Box::new(MinSizeView { min: (30, 10) }),
            )
            .with_min_size_mode(WindowMinSizeMode::Scroll),
            bounds,
        );

        let theme = Theme::dark();
        let backend = TestBackend::new(bounds.width, bounds.height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

        let down = Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });

        let (_id, res) = wm
            .dispatch_to_focused_view(&down, bounds, &theme)
            .expect("focused dispatch");
        assert!(res.is_consumed());
    }

    #[test]
    fn focus_cycles_between_windows() {
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut wm = WindowManager::new();
        let id1 = wm.add_window(
            Window::new(
                WindowKind::Normal,
                "One",
                Rect {
                    x: 1,
                    y: 1,
                    width: 20,
                    height: 6,
                },
                Box::new(DummyView),
            ),
            bounds,
        );
        let id2 = wm.add_window(
            Window::new(
                WindowKind::Normal,
                "Two",
                Rect {
                    x: 3,
                    y: 3,
                    width: 20,
                    height: 6,
                },
                Box::new(DummyView),
            ),
            bounds,
        );

        assert_eq!(wm.focused(), Some(id2));
        wm.focus_next();
        assert_eq!(wm.focused(), Some(id1));
    }

    #[test]
    fn modal_window_blocks_focus_changes() {
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut wm = WindowManager::new();
        let _id1 = wm.add_window(
            Window::new(
                WindowKind::Normal,
                "One",
                Rect {
                    x: 1,
                    y: 1,
                    width: 20,
                    height: 6,
                },
                Box::new(DummyView),
            ),
            bounds,
        );
        let modal_id = wm.add_window(
            Window::new(
                WindowKind::Modal,
                "Modal",
                Rect {
                    x: 10,
                    y: 8,
                    width: 30,
                    height: 8,
                },
                Box::new(DummyView),
            ),
            bounds,
        );

        assert_eq!(wm.focused(), Some(modal_id));
        wm.focus_next();
        assert_eq!(wm.focused(), Some(modal_id));
    }

    #[test]
    fn window_scrollbars_do_not_overwrite_resize_corners() {
        #[derive(Default)]
        struct ScrollableDummyView {
            viewport: (u16, u16),
        }

        impl Component for ScrollableDummyView {
            fn is_scrollable(&self) -> bool {
                true
            }

            fn content_size(&self) -> (u16, u16) {
                (200, 200)
            }

            fn scroll_offset(&self) -> (u16, u16) {
                (0, 0)
            }

            fn viewport_size(&self) -> (u16, u16) {
                self.viewport
            }

            fn scroll_config(&self) -> ScrollConfig {
                ScrollConfig::default()
                    .vertical_scrollbar(ScrollbarVisibility::Always)
                    .horizontal_scrollbar(ScrollbarVisibility::Always)
            }

            fn draw(&mut self, _frame: &mut Frame<'_>, area: Rect, _ctx: ComponentContext<'_>) {
                self.viewport = (area.width, area.height);
            }
        }

        let bounds = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let rect = Rect {
            x: 2,
            y: 2,
            width: 20,
            height: 8,
        };

        let mut wm = WindowManager::new();
        wm.add_window(
            Window::new(
                WindowKind::Normal,
                "Scroll",
                rect,
                Box::new(ScrollableDummyView::default()),
            ),
            bounds,
        );

        let theme = Theme::dark();
        let backend = TestBackend::new(bounds.width, bounds.height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

        let buf = terminal.backend().buffer();

        let left = rect.x;
        let top = rect.y;
        let right = rect.x.saturating_add(rect.width).saturating_sub(1);
        let bottom = rect.y.saturating_add(rect.height).saturating_sub(1);

        assert_eq!(
            buf.cell((left, top)).expect("top-left").symbol(),
            "╔",
            "top-left corner should remain a resize handle"
        );
        assert_eq!(
            buf.cell((right, top)).expect("top-right").symbol(),
            "╗",
            "top-right corner should remain a resize handle"
        );
        assert_eq!(
            buf.cell((left, bottom)).expect("bottom-left").symbol(),
            "╚",
            "bottom-left corner should remain a resize handle"
        );
        assert_eq!(
            buf.cell((right, bottom)).expect("bottom-right").symbol(),
            "╝",
            "bottom-right corner should remain a resize handle"
        );

        // Sanity: scrollbar arrows are drawn adjacent to the corners, not on them.
        assert_eq!(buf.cell((right, top + 1)).expect("vbar up").symbol(), "▲");
        assert_eq!(
            buf.cell((left + 1, bottom)).expect("hbar left").symbol(),
            "◄"
        );
        assert_eq!(
            buf.cell((right - 1, bottom)).expect("hbar right").symbol(),
            "►"
        );
    }

    #[test]
    fn mouse_drag_resize_handles_work_on_all_corners() {
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let cases = [
            (
                "top-left",
                (2, 2),
                (0, 0),
                Rect {
                    x: 0,
                    y: 0,
                    width: 22,
                    height: 8,
                },
            ),
            (
                "top-right",
                (21, 2),
                (25, 0),
                Rect {
                    x: 2,
                    y: 0,
                    width: 24,
                    height: 8,
                },
            ),
            (
                "bottom-left",
                (2, 7),
                (0, 9),
                Rect {
                    x: 0,
                    y: 2,
                    width: 22,
                    height: 8,
                },
            ),
            (
                "bottom-right",
                (21, 7),
                (25, 9),
                Rect {
                    x: 2,
                    y: 2,
                    width: 24,
                    height: 8,
                },
            ),
        ];

        for (label, down, drag, expected) in cases {
            let mut wm = WindowManager::new();
            let id = wm.add_window(
                Window::new(
                    WindowKind::Normal,
                    "Resizable",
                    Rect {
                        x: 2,
                        y: 2,
                        width: 20,
                        height: 6,
                    },
                    Box::new(DummyView),
                ),
                bounds,
            );

            let theme = Theme::dark();
            wm.handle_event(
                &Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: down.0,
                    row: down.1,
                    modifiers: KeyModifiers::NONE,
                }),
                bounds,
                super::WindowManagerInputMode::Normal,
                &theme,
            );
            wm.handle_event(
                &Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Drag(MouseButton::Left),
                    column: drag.0,
                    row: drag.1,
                    modifiers: KeyModifiers::NONE,
                }),
                bounds,
                super::WindowManagerInputMode::Normal,
                &theme,
            );

            let w = wm.window_mut(id).expect("window");
            assert_eq!(w.rect.get(), expected, "case {label}");
        }
    }

    #[test]
    fn close_hook_can_cancel_close_request() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let bounds = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let calls2 = Arc::clone(&calls);

        let mut wm = WindowManager::new();
        let id = wm.add_window(
            Window::new(
                WindowKind::Normal,
                "Hooked",
                Rect {
                    x: 2,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(DummyView),
            )
            .with_close_hook(move |_id| {
                calls2.fetch_add(1, Ordering::SeqCst);
                false
            }),
            bounds,
        );

        assert!(wm.window_mut(id).is_some());
        assert!(!wm.request_close(id), "expected close to be cancelled");
        assert!(wm.window_mut(id).is_some(), "window should remain");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn shadow_includes_bottom_right_corner() {
        let bounds = Rect::new(0, 0, 10, 10);
        let rect = Rect::new(1, 1, 3, 3);
        let style = Style::default().bg(Color::Red);

        let mut buf = Buffer::empty(bounds);
        assert_eq!(buf.cell((4, 4)).unwrap().bg, Color::Reset);

        draw_shadow(&mut buf, rect, bounds, style);

        assert_eq!(buf.cell((4, 4)).unwrap().bg, Color::Red);
    }

    #[test]
    fn window_background_is_opaque_by_default() {
        #[derive(Default)]
        struct UnderlayView {
            target: (u16, u16),
        }

        impl Component for UnderlayView {
            fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
                EventResult::ignored()
            }

            fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, _ctx: ComponentContext<'_>) {
                let (x, y) = self.target;
                if x >= area.x
                    && x < area.x.saturating_add(area.width)
                    && y >= area.y
                    && y < area.y.saturating_add(area.height)
                    && let Some(cell) = frame.buffer_mut().cell_mut((x, y))
                {
                    cell.set_symbol("X");
                }
            }
        }

        #[derive(Default)]
        struct OverlayView;

        impl Component for OverlayView {
            fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
                EventResult::ignored()
            }

            fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
        }

        let theme = Theme::dark();
        let bounds = Rect::new(0, 0, 30, 10);
        let target = (5, 3);

        let mut wm = WindowManager::new();
        let underlay = Window::new(
            WindowKind::Normal,
            "Underlay",
            Rect::new(1, 1, 20, 7),
            Box::new(UnderlayView { target }),
        );
        underlay.decorations.update(|d| d.shadow = false);
        wm.add_window(underlay, bounds);

        let overlay_rect = Rect::new(5, 3, 20, 6);
        let overlay = Window::new(
            WindowKind::Normal,
            "Overlay",
            overlay_rect,
            Box::new(OverlayView),
        );
        overlay.decorations.update(|d| d.shadow = false);
        wm.add_window(overlay, bounds);

        let backend = TestBackend::new(bounds.width, bounds.height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

        let cell = terminal.backend().buffer().cell(target).expect("cell");
        assert_ne!(
            cell.bg,
            Color::Reset,
            "expected window background fill to set a non-reset bg color (including border)"
        );
        assert_ne!(
            cell.symbol(),
            "X",
            "expected overlapping window to clear underlay content (including border)"
        );
    }

    #[test]
    fn thin_border_uses_single_line_glyphs_even_when_focused() {
        let theme = Theme::dark();
        let bounds = Rect::new(0, 0, 40, 15);
        let rect = Rect::new(2, 2, 16, 7);

        let mut wm = WindowManager::new();
        let w = Window::new(WindowKind::Normal, "Thin", rect, Box::new(DummyView));
        w.decorations.update(|d| d.border = WindowBorderStyle::Thin);
        wm.add_window(w, bounds);

        let backend = TestBackend::new(bounds.width, bounds.height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

        let buf = terminal.backend().buffer();

        let left = rect.x;
        let top = rect.y;
        let right = rect.x.saturating_add(rect.width).saturating_sub(1);
        let bottom = rect.y.saturating_add(rect.height).saturating_sub(1);

        assert_eq!(buf.cell((left, top)).expect("tl").symbol(), "┌");
        assert_eq!(buf.cell((right, top)).expect("tr").symbol(), "┐");
        assert_eq!(buf.cell((left, bottom)).expect("bl").symbol(), "└");
        assert_eq!(buf.cell((right, bottom)).expect("br").symbol(), "┘");
    }

    #[test]
    fn fixed_size_windows_hide_and_disable_minimize_maximize() {
        let theme = Theme::dark();
        let bounds = Rect::new(0, 0, 40, 15);
        let rect = Rect::new(2, 2, 18, 7);

        let mut wm = WindowManager::new();
        let w = Window::new(WindowKind::Normal, "Fixed", rect, Box::new(DummyView));
        w.resizable.set(false);
        let id = wm.add_window(w, bounds);

        wm.toggle_maximize_focused(bounds);
        wm.minimize_focused();
        let state = wm.window_mut(id).expect("window").state.get();
        assert_eq!(
            state,
            crate::wm::WindowState::Normal,
            "expected fixed-size window to ignore maximize/minimize actions"
        );

        let backend = TestBackend::new(bounds.width, bounds.height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

        let buf = terminal.backend().buffer();
        let inner_right = rect.x.saturating_add(rect.width).saturating_sub(2);

        assert_eq!(
            buf.cell((inner_right, rect.y)).expect("close btn").symbol(),
            "×",
            "expected close button to still be visible"
        );
        assert_eq!(
            buf.cell((inner_right.saturating_sub(2), rect.y))
                .expect("maximize slot")
                .symbol(),
            "═",
            "expected maximize button to be hidden for fixed-size windows"
        );
        assert_eq!(
            buf.cell((inner_right.saturating_sub(4), rect.y))
                .expect("minimize slot")
                .symbol(),
            "═",
            "expected minimize button to be hidden for fixed-size windows"
        );
    }
}
