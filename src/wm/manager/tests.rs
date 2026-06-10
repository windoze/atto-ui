use super::{DragKind, WindowManager};
use crate::composable::{
    Component, ComponentContext, DragAndDrop, DragOffer, DragOperation, DragPayload, DragSource,
    DropEffect, DropFeedback, DynamicTree, EventHandling, EventResult, FocusNav, Label, Layout,
    ScrollConfig, Scrollable, ScrollbarVisibility,
};
use crate::drawing::draw_shadow;
use crate::theme::Theme;
use crate::wm::{
    DockAutoHide, DockSide, Window, WindowBorderStyle, WindowDock, WindowKind, WindowMinSizeMode,
    WindowState,
};
use crate::{CallbackRegistry, ComponentSpec, ComponentValue, TreeOp};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct DummyView;

impl Component for DummyView {
    fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
}

impl EventHandling for DummyView {
    fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }
}

crate::impl_component_default_traits!(DummyView => Layout, Scrollable, FocusNav, DynamicTree);

fn assert_window_index_matches_order(wm: &WindowManager) {
    assert_eq!(wm.window_index.len(), wm.windows().len());
    for (idx, window) in wm.windows().iter().enumerate() {
        assert_eq!(wm.window_index_of(window.id), Some(idx));
    }
}

struct MinSizeView {
    min: (u16, u16),
}

impl Component for MinSizeView {
    fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
}

impl Layout for MinSizeView {
    fn min_width(&self) -> u16 {
        self.min.0
    }

    fn min_height(&self) -> u16 {
        self.min.1
    }
}

crate::impl_component_default_traits!(MinSizeView => Scrollable, FocusNav, DynamicTree, EventHandling);

struct DragSourceProbe {
    threshold: u16,
    source_requests: Arc<AtomicUsize>,
    cancel_count: Arc<AtomicUsize>,
}

impl Component for DragSourceProbe {
    fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
}

impl DragAndDrop for DragSourceProbe {
    fn drag_source_at(
        &self,
        _screen_x: u16,
        _screen_y: u16,
        _ctx: ComponentContext<'_>,
    ) -> Option<DragSource> {
        self.source_requests.fetch_add(1, Ordering::SeqCst);
        Some(DragSource {
            payload: DragPayload::Text("probe".to_string()),
            operation: DragOperation::Copy,
            threshold: self.threshold,
            ghost: Some("probe".to_string()),
        })
    }

    fn drag_cancelled(&mut self, _ctx: ComponentContext<'_>) {
        self.cancel_count.fetch_add(1, Ordering::SeqCst);
    }
}

impl Layout for DragSourceProbe {}
impl Scrollable for DragSourceProbe {}
impl FocusNav for DragSourceProbe {}
impl DynamicTree for DragSourceProbe {}
impl EventHandling for DragSourceProbe {}

struct DropTargetProbe {
    effect: DropEffect,
    drag_over_count: Arc<AtomicUsize>,
    drop_count: Arc<AtomicUsize>,
}

impl Component for DropTargetProbe {
    fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
}

impl DragAndDrop for DropTargetProbe {
    fn drag_over(&mut self, offer: DragOffer<'_>, ctx: ComponentContext<'_>) -> DropFeedback {
        assert!(
            ctx.drag.is_some(),
            "active drag context should reach target"
        );
        assert_eq!(offer.payload, &DragPayload::Text("probe".to_string()));
        self.drag_over_count.fetch_add(1, Ordering::SeqCst);
        DropFeedback {
            effect: self.effect,
            rect: Some(Rect::new(30, 3, 10, 2)),
            label: Some("target".to_string()),
        }
    }

    fn drop(&mut self, offer: DragOffer<'_>, ctx: ComponentContext<'_>) -> EventResult {
        assert!(ctx.drag.is_some(), "active drag context should reach drop");
        assert_eq!(offer.payload, &DragPayload::Text("probe".to_string()));
        self.drop_count.fetch_add(1, Ordering::SeqCst);
        EventResult::consumed()
    }
}

impl Layout for DropTargetProbe {}
impl Scrollable for DropTargetProbe {}
impl FocusNav for DropTargetProbe {}
impl DynamicTree for DropTargetProbe {}
impl EventHandling for DropTargetProbe {}

struct DrawCountView {
    count: Arc<AtomicUsize>,
}

impl Component for DrawCountView {
    fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

crate::impl_component_default_traits!(DrawCountView => Layout, Scrollable, FocusNav, DynamicTree, EventHandling);

struct MouseCountView {
    count: Arc<AtomicUsize>,
}

impl Component for MouseCountView {
    fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
}

impl EventHandling for MouseCountView {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if matches!(event, Event::Mouse(_)) {
            self.count.fetch_add(1, Ordering::SeqCst);
            EventResult::consumed()
        } else {
            EventResult::ignored()
        }
    }
}

crate::impl_component_default_traits!(MouseCountView => Layout, Scrollable, FocusNav, DynamicTree);

fn left_mouse_event(kind: MouseEventKind, column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn dock_auto_hide_visible(wm: &WindowManager, id: crate::wm::WindowId) -> bool {
    wm.window(id)
        .and_then(|window| window.dock.get())
        .is_some_and(|dock| matches!(dock.auto_hide, DockAutoHide::Enabled { visible: true }))
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
        .and_then(|w| w.view.get_property("text"))
        .expect("before text");
    assert_eq!(before, ComponentValue::String("A".into()));

    assert!(wm.set_view(id, Box::new(Label::new("B"))));
    let after = wm
        .window_mut(id)
        .and_then(|w| w.view.get_property("text"))
        .expect("after text");
    assert_eq!(after, ComponentValue::String("B".into()));
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
fn window_index_stays_synced_after_reorder_focus_and_close() {
    let bounds = Rect::new(0, 0, 80, 24);
    let mut wm = WindowManager::new();
    let id1 = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "One",
            Rect::new(1, 1, 20, 6),
            Box::new(DummyView),
        ),
        bounds,
    );
    let id2 = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Two",
            Rect::new(3, 3, 20, 6),
            Box::new(DummyView),
        ),
        bounds,
    );
    let id3 = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Three",
            Rect::new(5, 5, 20, 6),
            Box::new(DummyView),
        ),
        bounds,
    );

    assert_window_index_matches_order(&wm);
    assert_eq!(wm.window(id2).expect("id2").title.get(), "Two");

    wm.bring_to_front(id1);
    assert_window_index_matches_order(&wm);
    assert_eq!(wm.windows().last().map(|w| w.id), Some(id1));

    wm.focus(id3);
    assert_window_index_matches_order(&wm);
    assert_eq!(wm.focused(), Some(id3));
    assert_eq!(wm.windows().last().map(|w| w.id), Some(id3));

    wm.close(id2);
    assert_window_index_matches_order(&wm);
    assert!(wm.window(id2).is_none());
    assert_eq!(wm.window(id1).expect("id1").title.get(), "One");
    assert_eq!(wm.window(id3).expect("id3").title.get(), "Three");
}

#[test]
fn window_index_lookup_recovers_from_internal_slice_reorder() {
    let bounds = Rect::new(0, 0, 80, 24);
    let mut wm = WindowManager::new();
    let id1 = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "One",
            Rect::new(1, 1, 20, 6),
            Box::new(DummyView),
        ),
        bounds,
    );
    let id2 = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Two",
            Rect::new(3, 3, 20, 6),
            Box::new(DummyView),
        ),
        bounds,
    );

    wm.windows_mut().swap(0, 1);

    assert_eq!(wm.window(id1).expect("id1").title.get(), "One");
    assert_eq!(wm.window(id2).expect("id2").title.get(), "Two");

    wm.bring_to_front(id1);
    assert_window_index_matches_order(&wm);
    assert_eq!(wm.windows().last().map(|w| w.id), Some(id1));
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
fn normal_window_maximize_and_restore_preserves_rect() {
    let bounds = Rect::new(0, 0, 80, 24);
    let original = Rect::new(4, 3, 24, 8);
    let mut wm = WindowManager::new();
    let id = wm.add_window(
        Window::new(WindowKind::Normal, "Main", original, Box::new(DummyView)),
        bounds,
    );

    wm.toggle_maximize_focused(bounds);
    let window = wm.window(id).expect("window");
    assert_eq!(window.state.get(), WindowState::Maximized);
    assert_eq!(window.rect.get(), bounds);

    wm.toggle_maximize_focused(bounds);
    let window = wm.window(id).expect("window");
    assert_eq!(window.state.get(), WindowState::Normal);
    assert_eq!(window.rect.get(), original);
}

#[test]
fn relocated_titlebar_buttons_handle_mouse_at_drawn_positions() {
    let bounds = Rect::new(0, 0, 80, 24);
    let theme = Theme::dark();
    let original = Rect::new(10, 3, 24, 8);
    let mut wm = WindowManager::new();
    let id = wm.add_window(
        Window::new(WindowKind::Normal, "Main", original, Box::new(DummyView)),
        bounds,
    );

    let close = wm.handle_event(
        &left_mouse_event(MouseEventKind::Down(MouseButton::Left), 12, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    assert_eq!(close.close, Some(id));
    assert!(
        wm.drag.is_none(),
        "clicking the left close button must not start a move drag"
    );

    let maximize = wm.handle_event(
        &left_mouse_event(MouseEventKind::Down(MouseButton::Left), 31, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    assert_eq!(maximize.close, None);
    assert_eq!(
        wm.window(id).expect("window").state.get(),
        WindowState::Maximized
    );
    assert_eq!(wm.window(id).expect("window").rect.get(), bounds);

    let restore = wm.handle_event(
        &left_mouse_event(MouseEventKind::Down(MouseButton::Left), 77, 0),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    assert_eq!(restore.close, None);
    assert_eq!(
        wm.window(id).expect("window").state.get(),
        WindowState::Normal
    );
    assert_eq!(wm.window(id).expect("window").rect.get(), original);
}

#[test]
fn titlebar_drag_still_starts_outside_relocated_buttons() {
    let bounds = Rect::new(0, 0, 80, 24);
    let theme = Theme::dark();
    let original = Rect::new(10, 3, 24, 8);
    let mut wm = WindowManager::new();
    let id = wm.add_window(
        Window::new(WindowKind::Normal, "Main", original, Box::new(DummyView)),
        bounds,
    );

    wm.handle_event(
        &left_mouse_event(MouseEventKind::Down(MouseButton::Left), 15, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );

    assert!(
        matches!(
            wm.drag,
            Some(super::DragState {
                window_id,
                kind: DragKind::Move {
                    offset_x: 5,
                    offset_y: 0
                },
            }) if window_id == id
        ),
        "clicking the titlebar text area after the left close button should start a move drag"
    );

    wm.handle_event(
        &left_mouse_event(MouseEventKind::Drag(MouseButton::Left), 18, 5),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    wm.handle_event(
        &left_mouse_event(MouseEventKind::Up(MouseButton::Left), 18, 5),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );

    assert_eq!(
        wm.window(id).expect("window").rect.get(),
        Rect::new(13, 5, 24, 8)
    );
}

#[test]
fn left_dock_reserves_maximized_normal_window() {
    let bounds = Rect::new(0, 0, 80, 24);
    let mut wm = WindowManager::new();
    let dock_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Dock",
            Rect::new(30, 8, 5, 5),
            Box::new(DummyView),
        )
        .with_dock(Some(WindowDock::docked(DockSide::Left, 18))),
        bounds,
    );
    let normal_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Main",
            Rect::new(25, 3, 20, 8),
            Box::new(DummyView),
        ),
        bounds,
    );

    wm.focus(normal_id);
    wm.toggle_maximize_focused(bounds);

    assert_eq!(
        wm.window(dock_id).expect("dock").rect.get(),
        Rect::new(0, 0, 18, 24)
    );
    assert_eq!(
        wm.window(normal_id).expect("normal").rect.get(),
        Rect::new(18, 0, 62, 24)
    );
}

#[test]
fn right_and_bottom_docks_reserve_work_area_in_order() {
    let bounds = Rect::new(0, 0, 80, 24);
    let mut wm = WindowManager::new();
    let right_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Right",
            Rect::new(1, 1, 5, 5),
            Box::new(DummyView),
        )
        .with_dock(Some(WindowDock::docked(DockSide::Right, 15))),
        bounds,
    );
    let bottom_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Bottom",
            Rect::new(1, 1, 5, 5),
            Box::new(DummyView),
        )
        .with_dock(Some(WindowDock::docked(DockSide::Bottom, 6))),
        bounds,
    );
    let normal_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Main",
            Rect::new(1, 1, 20, 8),
            Box::new(DummyView),
        ),
        bounds,
    );

    wm.focus(normal_id);
    wm.toggle_maximize_focused(bounds);

    assert_eq!(wm.effective_work_area(bounds), Rect::new(0, 0, 65, 18));
    assert_eq!(
        wm.window(right_id).expect("right").rect.get(),
        Rect::new(65, 0, 15, 24)
    );
    assert_eq!(
        wm.window(bottom_id).expect("bottom").rect.get(),
        Rect::new(0, 18, 65, 6)
    );
    assert_eq!(
        wm.window(normal_id).expect("normal").rect.get(),
        Rect::new(0, 0, 65, 18)
    );
}

#[test]
fn dock_window_rect_ignores_original_builder_rect_and_draws_at_edge() {
    let theme = Theme::dark();
    let bounds = Rect::new(0, 0, 80, 24);
    let original = Rect::new(20, 7, 5, 5);
    let mut wm = WindowManager::new();
    let id = wm.add_window(
        Window::new(WindowKind::Normal, "Top", original, Box::new(DummyView))
            .with_dock(Some(WindowDock::docked(DockSide::Top, 4))),
        bounds,
    );

    assert_ne!(wm.window(id).expect("dock").rect.get(), original);
    assert_eq!(
        wm.window(id).expect("dock").rect.get(),
        Rect::new(0, 0, 80, 4)
    );

    let backend = TestBackend::new(bounds.width, bounds.height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

    assert_ne!(
        terminal
            .backend()
            .buffer()
            .cell((0, 0))
            .expect("dock edge")
            .symbol(),
        " "
    );
}

#[test]
fn normal_move_and_resize_are_clamped_to_dock_reserve() {
    let bounds = Rect::new(0, 0, 80, 24);
    let mut wm = WindowManager::new();
    let dock_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Dock",
            Rect::new(2, 2, 5, 5),
            Box::new(DummyView),
        )
        .with_dock(Some(WindowDock::docked(DockSide::Left, 20))),
        bounds,
    );
    let normal_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Main",
            Rect::new(30, 4, 20, 8),
            Box::new(DummyView),
        ),
        bounds,
    );

    assert!(wm.move_window_to(normal_id, 0, 0, bounds));
    assert_eq!(wm.window(normal_id).expect("normal").rect.get().x, 20);

    assert!(wm.resize_window_to(normal_id, 100, 30, bounds));
    assert_eq!(
        wm.window(normal_id).expect("normal").rect.get(),
        Rect::new(20, 0, 60, 24)
    );

    let dock_rect = wm.window(dock_id).expect("dock").rect.get();
    assert!(!wm.move_window_to(dock_id, 10, 0, bounds));
    assert_eq!(wm.window(dock_id).expect("dock").rect.get(), dock_rect);
}

#[test]
fn auto_hidden_dock_reserves_one_cell_handle() {
    let bounds = Rect::new(0, 0, 80, 24);
    let mut dock = WindowDock::docked(DockSide::Left, 20);
    dock.auto_hide = DockAutoHide::Enabled { visible: false };
    let mut wm = WindowManager::new();
    let dock_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Dock",
            Rect::new(2, 2, 5, 5),
            Box::new(DummyView),
        )
        .with_dock(Some(dock)),
        bounds,
    );
    let normal_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Main",
            Rect::new(3, 3, 20, 8),
            Box::new(DummyView),
        ),
        bounds,
    );

    wm.focus(normal_id);
    wm.toggle_maximize_focused(bounds);

    assert_eq!(
        wm.window(dock_id).expect("dock").rect.get(),
        Rect::new(0, 0, 1, 24)
    );
    assert_eq!(
        wm.window(normal_id).expect("normal").rect.get(),
        Rect::new(1, 0, 79, 24)
    );
}

#[test]
fn dock_resize_edge_rect_uses_inner_edge_for_every_side() {
    let rect = Rect::new(10, 5, 8, 6);

    assert_eq!(
        super::docking::dock_resize_edge_rect(rect, DockSide::Left),
        Rect::new(17, 5, 1, 6)
    );
    assert_eq!(
        super::docking::dock_resize_edge_rect(rect, DockSide::Right),
        Rect::new(10, 5, 1, 6)
    );
    assert_eq!(
        super::docking::dock_resize_edge_rect(rect, DockSide::Bottom),
        Rect::new(10, 5, 8, 1)
    );
    assert_eq!(
        super::docking::dock_resize_edge_rect(rect, DockSide::Top),
        Rect::new(10, 10, 8, 1)
    );
}

#[test]
fn dock_resize_clamp_respects_min_max_and_available_for_every_side() {
    for side in [
        DockSide::Left,
        DockSide::Right,
        DockSide::Bottom,
        DockSide::Top,
    ] {
        let mut dock = WindowDock::docked(side, 12);
        dock.min_size = 5;
        dock.max_size = Some(12);

        assert_eq!(
            super::docking::clamp_dock_size(&dock, Rect::new(0, 0, 40, 20), 1),
            5
        );
        assert_eq!(
            super::docking::clamp_dock_size(&dock, Rect::new(0, 0, 40, 20), 50),
            12
        );

        dock.max_size = None;
        let available_area = Rect::new(0, 0, 9, 7);
        let available = match side {
            DockSide::Left | DockSide::Right => available_area.width,
            DockSide::Bottom | DockSide::Top => available_area.height,
        };
        assert_eq!(
            super::docking::clamp_dock_size(&dock, available_area, 50),
            available
        );
    }
}

#[test]
fn left_dock_resize_drag_updates_dock_size_without_touching_normal_rect() {
    let bounds = Rect::new(0, 0, 80, 24);
    let theme = Theme::dark();
    let mut dock = WindowDock::docked(DockSide::Left, 18);
    dock.min_size = 10;
    dock.max_size = Some(30);
    let mut wm = WindowManager::new();
    let dock_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Dock",
            Rect::new(2, 2, 5, 5),
            Box::new(DummyView),
        )
        .with_dock(Some(dock)),
        bounds,
    );
    let normal_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Main",
            Rect::new(35, 4, 20, 8),
            Box::new(DummyView),
        ),
        bounds,
    );
    let normal_before = wm.window(normal_id).expect("normal").rect.get();

    wm.handle_event(
        &left_mouse_event(MouseEventKind::Down(MouseButton::Left), 17, 5),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    wm.handle_event(
        &left_mouse_event(MouseEventKind::Drag(MouseButton::Left), 24, 5),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    wm.handle_event(
        &left_mouse_event(MouseEventKind::Up(MouseButton::Left), 24, 5),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );

    let dock = wm
        .window(dock_id)
        .expect("dock")
        .dock
        .get()
        .expect("dock config");
    assert_eq!(dock.size, 25);
    assert_eq!(wm.window(dock_id).expect("dock").rect.get().width, 25);
    assert_eq!(
        wm.window(normal_id).expect("normal").rect.get(),
        normal_before
    );
}

#[test]
fn auto_hide_handle_click_shows_overlay_and_outside_click_hides_it() {
    let bounds = Rect::new(0, 0, 80, 24);
    let theme = Theme::dark();
    let mut dock = WindowDock::docked(DockSide::Left, 20);
    dock.auto_hide = DockAutoHide::Enabled { visible: false };
    dock.handle_label = Some("D".to_string());
    let mut wm = WindowManager::new();
    let dock_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Dock",
            Rect::new(2, 2, 5, 5),
            Box::new(DummyView),
        )
        .with_dock(Some(dock)),
        bounds,
    );
    let _normal_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Main",
            Rect::new(3, 3, 20, 8),
            Box::new(DummyView),
        ),
        bounds,
    );
    wm.toggle_maximize_focused(bounds);

    assert_eq!(wm.effective_work_area(bounds), Rect::new(1, 0, 79, 24));
    assert_eq!(wm.window(dock_id).expect("dock").rect.get().width, 1);

    wm.handle_event(
        &left_mouse_event(MouseEventKind::Down(MouseButton::Left), 0, 5),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );

    assert!(dock_auto_hide_visible(&wm, dock_id));
    assert_eq!(wm.effective_work_area(bounds), Rect::new(1, 0, 79, 24));
    assert_eq!(wm.window(dock_id).expect("dock").rect.get().width, 20);

    wm.handle_event(
        &left_mouse_event(MouseEventKind::Down(MouseButton::Left), 30, 5),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );

    assert!(!dock_auto_hide_visible(&wm, dock_id));
    assert_eq!(wm.window(dock_id).expect("dock").rect.get().width, 1);
}

#[test]
fn auto_hidden_dock_draws_handle_without_drawing_view() {
    let bounds = Rect::new(0, 0, 80, 24);
    let theme = Theme::dark();
    let mut dock = WindowDock::docked(DockSide::Left, 20);
    dock.auto_hide = DockAutoHide::Enabled { visible: false };
    dock.handle_label = Some("D".to_string());
    let draw_count = Arc::new(AtomicUsize::new(0));
    let mut wm = WindowManager::new();
    let dock_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Dock",
            Rect::new(2, 2, 5, 5),
            Box::new(DrawCountView {
                count: Arc::clone(&draw_count),
            }),
        )
        .with_dock(Some(dock)),
        bounds,
    );

    let backend = TestBackend::new(bounds.width, bounds.height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

    assert_eq!(draw_count.load(Ordering::SeqCst), 0);
    assert_eq!(wm.window(dock_id).expect("dock").rect.get().width, 1);
    assert_eq!(
        terminal
            .backend()
            .buffer()
            .cell((0, 0))
            .expect("handle cell")
            .symbol(),
        "D"
    );
}

#[test]
fn visible_auto_hide_dock_overlays_normal_windows_for_hit_test() {
    let bounds = Rect::new(0, 0, 80, 24);
    let mut dock = WindowDock::docked(DockSide::Left, 20);
    dock.auto_hide = DockAutoHide::Enabled { visible: true };
    let mut wm = WindowManager::new();
    let dock_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Dock",
            Rect::new(2, 2, 5, 5),
            Box::new(DummyView),
        )
        .with_dock(Some(dock)),
        bounds,
    );
    let normal_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Main",
            Rect::new(3, 3, 20, 8),
            Box::new(DummyView),
        ),
        bounds,
    );

    wm.toggle_maximize_focused(bounds);

    assert_eq!(wm.effective_work_area(bounds), Rect::new(1, 0, 79, 24));
    assert_eq!(wm.window(dock_id).expect("dock").rect.get().width, 20);
    assert_eq!(wm.window(normal_id).expect("normal").rect.get().x, 1);
    assert_eq!(wm.window_at(5, 5), Some(dock_id));
}

#[test]
fn visible_auto_hide_overlay_mouse_does_not_pass_through_to_normal_window() {
    let bounds = Rect::new(0, 0, 80, 24);
    let theme = Theme::dark();
    let mut dock = WindowDock::docked(DockSide::Left, 20);
    dock.auto_hide = DockAutoHide::Enabled { visible: true };
    let dock_events = Arc::new(AtomicUsize::new(0));
    let normal_events = Arc::new(AtomicUsize::new(0));
    let mut wm = WindowManager::new();
    let dock_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Dock",
            Rect::new(2, 2, 5, 5),
            Box::new(MouseCountView {
                count: Arc::clone(&dock_events),
            }),
        )
        .with_dock(Some(dock)),
        bounds,
    );
    let normal_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Main",
            Rect::new(1, 0, 79, 24),
            Box::new(MouseCountView {
                count: Arc::clone(&normal_events),
            }),
        ),
        bounds,
    );
    wm.toggle_maximize_focused(bounds);
    assert_eq!(wm.focused(), Some(normal_id));

    let event = left_mouse_event(MouseEventKind::Down(MouseButton::Left), 5, 5);
    wm.handle_event(
        &event,
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    wm.dispatch_to_focused_view(&event, bounds, &theme);

    assert_eq!(wm.focused(), Some(dock_id));
    assert_eq!(dock_events.load(Ordering::SeqCst), 1);
    assert_eq!(normal_events.load(Ordering::SeqCst), 0);
}

#[test]
fn maximized_modal_uses_dock_reserved_work_area() {
    let bounds = Rect::new(0, 0, 80, 24);
    let mut wm = WindowManager::new();
    let dock_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Dock",
            Rect::new(2, 2, 5, 5),
            Box::new(DummyView),
        )
        .with_dock(Some(WindowDock::docked(DockSide::Left, 20))),
        bounds,
    );
    let modal_id = wm.add_window(
        Window::new(
            WindowKind::Modal,
            "Modal",
            Rect::new(0, 0, 30, 10),
            Box::new(DummyView),
        ),
        bounds,
    );

    wm.toggle_maximize_focused(bounds);

    assert_eq!(wm.focused(), Some(modal_id));
    assert_eq!(
        wm.window(dock_id).expect("dock").rect.get(),
        Rect::new(0, 0, 20, 24)
    );
    assert_eq!(
        wm.window(modal_id).expect("modal").rect.get(),
        Rect::new(20, 0, 60, 24),
        "modals stay inside the dock-reserved work area instead of covering docked windows"
    );
    assert_eq!(
        wm.window_at(5, 5),
        None,
        "active modals keep docked windows visible but block dock hit-testing outside the modal"
    );
}

#[test]
fn normal_window_minimize_updates_focus_and_restore_refocuses() {
    let bounds = Rect::new(0, 0, 80, 24);
    let mut wm = WindowManager::new();
    let first = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "First",
            Rect::new(1, 1, 20, 6),
            Box::new(DummyView),
        ),
        bounds,
    );
    let second = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Second",
            Rect::new(4, 4, 20, 6),
            Box::new(DummyView),
        ),
        bounds,
    );

    assert_eq!(wm.focused(), Some(second));
    wm.minimize_focused();
    assert_eq!(
        wm.window(second).expect("second").state.get(),
        WindowState::Minimized
    );
    assert_eq!(wm.focused(), Some(first));

    assert!(wm.restore_window(second));
    assert_eq!(
        wm.window(second).expect("second").state.get(),
        WindowState::Normal
    );
    assert_eq!(wm.focused(), Some(second));
}

#[test]
fn tooltip_windows_do_not_steal_focus_or_accept_focus() {
    let bounds = Rect::new(0, 0, 80, 24);
    let mut wm = WindowManager::new();
    let normal = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Normal",
            Rect::new(1, 1, 20, 6),
            Box::new(DummyView),
        ),
        bounds,
    );
    let tooltip = wm.add_window(
        Window::new(
            WindowKind::Tooltip,
            "Tip",
            Rect::new(6, 3, 18, 4),
            Box::new(DummyView),
        ),
        bounds,
    );

    assert_eq!(wm.focused(), Some(normal));
    assert_eq!(wm.windows().last().map(|w| w.id), Some(tooltip));

    wm.focus(tooltip);
    assert_eq!(wm.focused(), Some(normal));
}

#[test]
fn window_scrollbars_do_not_overwrite_resize_corners() {
    #[derive(Default)]
    struct ScrollableDummyView {
        viewport: (u16, u16),
    }

    impl Component for ScrollableDummyView {
        fn draw(&mut self, _frame: &mut Frame<'_>, area: Rect, _ctx: ComponentContext<'_>) {
            self.viewport = (area.width, area.height);
        }
    }

    impl Scrollable for ScrollableDummyView {
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
    }

    crate::impl_component_default_traits!(ScrollableDummyView => Layout, FocusNav, DynamicTree, EventHandling);

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
fn component_drag_stays_pending_until_threshold_is_reached() {
    let bounds = Rect::new(0, 0, 80, 24);
    let theme = Theme::dark();
    let source_requests = Arc::new(AtomicUsize::new(0));
    let cancel_count = Arc::new(AtomicUsize::new(0));
    let target_over = Arc::new(AtomicUsize::new(0));
    let target_drop = Arc::new(AtomicUsize::new(0));

    let mut wm = WindowManager::new();
    wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Source",
            Rect::new(2, 2, 20, 6),
            Box::new(DragSourceProbe {
                threshold: 4,
                source_requests: Arc::clone(&source_requests),
                cancel_count: Arc::clone(&cancel_count),
            }),
        ),
        bounds,
    );
    wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Target",
            Rect::new(30, 2, 20, 6),
            Box::new(DropTargetProbe {
                effect: DropEffect::Copy,
                drag_over_count: Arc::clone(&target_over),
                drop_count: Arc::clone(&target_drop),
            }),
        ),
        bounds,
    );

    wm.handle_event(
        &left_mouse_event(MouseEventKind::Down(MouseButton::Left), 3, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    wm.handle_event(
        &left_mouse_event(MouseEventKind::Drag(MouseButton::Left), 5, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );

    let drag = wm.global_drag.as_ref().expect("pending drag");
    assert!(!drag.active);
    assert_eq!(source_requests.load(Ordering::SeqCst), 1);
    assert_eq!(target_over.load(Ordering::SeqCst), 0);

    let up = wm.handle_event(
        &left_mouse_event(MouseEventKind::Up(MouseButton::Left), 5, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    assert!(
        !up.consumed,
        "pending click release should still reach views"
    );
    assert!(wm.global_drag.is_none());
    assert_eq!(cancel_count.load(Ordering::SeqCst), 0);
    assert_eq!(target_drop.load(Ordering::SeqCst), 0);
}

#[test]
fn component_drag_over_reaches_target_after_threshold() {
    let bounds = Rect::new(0, 0, 80, 24);
    let theme = Theme::dark();
    let source_requests = Arc::new(AtomicUsize::new(0));
    let cancel_count = Arc::new(AtomicUsize::new(0));
    let target_over = Arc::new(AtomicUsize::new(0));
    let target_drop = Arc::new(AtomicUsize::new(0));

    let mut wm = WindowManager::new();
    wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Source",
            Rect::new(2, 2, 20, 6),
            Box::new(DragSourceProbe {
                threshold: 1,
                source_requests: Arc::clone(&source_requests),
                cancel_count: Arc::clone(&cancel_count),
            }),
        ),
        bounds,
    );
    let target_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Target",
            Rect::new(30, 2, 20, 6),
            Box::new(DropTargetProbe {
                effect: DropEffect::Copy,
                drag_over_count: Arc::clone(&target_over),
                drop_count: Arc::clone(&target_drop),
            }),
        ),
        bounds,
    );

    wm.handle_event(
        &left_mouse_event(MouseEventKind::Down(MouseButton::Left), 3, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    wm.handle_event(
        &left_mouse_event(MouseEventKind::Drag(MouseButton::Left), 32, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );

    let drag = wm.global_drag.as_ref().expect("active drag");
    assert!(drag.active);
    assert_eq!(drag.target_window, Some(target_id));
    assert_eq!(target_over.load(Ordering::SeqCst), 1);
    assert_eq!(target_drop.load(Ordering::SeqCst), 0);
    assert_eq!(cancel_count.load(Ordering::SeqCst), 0);
}

#[test]
fn component_drag_rejected_drop_cancels_source() {
    let bounds = Rect::new(0, 0, 80, 24);
    let theme = Theme::dark();
    let source_requests = Arc::new(AtomicUsize::new(0));
    let cancel_count = Arc::new(AtomicUsize::new(0));
    let target_over = Arc::new(AtomicUsize::new(0));
    let target_drop = Arc::new(AtomicUsize::new(0));

    let mut wm = WindowManager::new();
    wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Source",
            Rect::new(2, 2, 20, 6),
            Box::new(DragSourceProbe {
                threshold: 1,
                source_requests: Arc::clone(&source_requests),
                cancel_count: Arc::clone(&cancel_count),
            }),
        ),
        bounds,
    );
    wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Target",
            Rect::new(30, 2, 20, 6),
            Box::new(DropTargetProbe {
                effect: DropEffect::None,
                drag_over_count: Arc::clone(&target_over),
                drop_count: Arc::clone(&target_drop),
            }),
        ),
        bounds,
    );

    wm.handle_event(
        &left_mouse_event(MouseEventKind::Down(MouseButton::Left), 3, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    wm.handle_event(
        &left_mouse_event(MouseEventKind::Drag(MouseButton::Left), 32, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    let up = wm.handle_event(
        &left_mouse_event(MouseEventKind::Up(MouseButton::Left), 32, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );

    assert!(up.consumed);
    assert!(wm.global_drag.is_none());
    assert_eq!(target_over.load(Ordering::SeqCst), 1);
    assert_eq!(target_drop.load(Ordering::SeqCst), 0);
    assert_eq!(cancel_count.load(Ordering::SeqCst), 1);
}

#[test]
fn component_drag_closes_source_window_clears_drag_state() {
    let bounds = Rect::new(0, 0, 80, 24);
    let theme = Theme::dark();
    let source_requests = Arc::new(AtomicUsize::new(0));
    let cancel_count = Arc::new(AtomicUsize::new(0));
    let target_over = Arc::new(AtomicUsize::new(0));
    let target_drop = Arc::new(AtomicUsize::new(0));

    let mut wm = WindowManager::new();
    let source_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Source",
            Rect::new(2, 2, 20, 6),
            Box::new(DragSourceProbe {
                threshold: 1,
                source_requests: Arc::clone(&source_requests),
                cancel_count: Arc::clone(&cancel_count),
            }),
        ),
        bounds,
    );
    wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Target",
            Rect::new(30, 2, 20, 6),
            Box::new(DropTargetProbe {
                effect: DropEffect::Copy,
                drag_over_count: Arc::clone(&target_over),
                drop_count: Arc::clone(&target_drop),
            }),
        ),
        bounds,
    );

    wm.handle_event(
        &left_mouse_event(MouseEventKind::Down(MouseButton::Left), 3, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    wm.handle_event(
        &left_mouse_event(MouseEventKind::Drag(MouseButton::Left), 32, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    assert!(wm.has_global_drag());

    wm.close(source_id);

    assert!(!wm.has_global_drag());
}

#[test]
fn component_drag_closes_target_window_clears_drag_state() {
    let bounds = Rect::new(0, 0, 80, 24);
    let theme = Theme::dark();
    let source_requests = Arc::new(AtomicUsize::new(0));
    let cancel_count = Arc::new(AtomicUsize::new(0));
    let target_over = Arc::new(AtomicUsize::new(0));
    let target_drop = Arc::new(AtomicUsize::new(0));

    let mut wm = WindowManager::new();
    wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Source",
            Rect::new(2, 2, 20, 6),
            Box::new(DragSourceProbe {
                threshold: 1,
                source_requests: Arc::clone(&source_requests),
                cancel_count: Arc::clone(&cancel_count),
            }),
        ),
        bounds,
    );
    let target_id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Target",
            Rect::new(30, 2, 20, 6),
            Box::new(DropTargetProbe {
                effect: DropEffect::Copy,
                drag_over_count: Arc::clone(&target_over),
                drop_count: Arc::clone(&target_drop),
            }),
        ),
        bounds,
    );

    wm.handle_event(
        &left_mouse_event(MouseEventKind::Down(MouseButton::Left), 3, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    wm.handle_event(
        &left_mouse_event(MouseEventKind::Drag(MouseButton::Left), 32, 3),
        bounds,
        super::WindowManagerInputMode::Normal,
        &theme,
    );
    assert!(wm.has_global_drag());

    wm.close(target_id);

    assert!(!wm.has_global_drag());
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

    impl EventHandling for UnderlayView {
        fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
            EventResult::ignored()
        }
    }

    crate::impl_component_default_traits!(UnderlayView => Layout, Scrollable, FocusNav, DynamicTree);

    #[derive(Default)]
    struct OverlayView;

    impl Component for OverlayView {
        fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
    }

    impl EventHandling for OverlayView {
        fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
            EventResult::ignored()
        }
    }

    crate::impl_component_default_traits!(OverlayView => Layout, Scrollable, FocusNav, DynamicTree);

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
    let inner_left = rect.x.saturating_add(1);
    let inner_right = rect.x.saturating_add(rect.width).saturating_sub(2);

    assert_eq!(
        buf.cell((inner_left, rect.y)).expect("close left").symbol(),
        "[",
        "expected close button to start at the left titlebar edge"
    );
    assert_eq!(
        buf.cell((inner_left.saturating_add(1), rect.y))
            .expect("close glyph")
            .symbol(),
        "■",
        "expected close button to still be visible"
    );
    assert_eq!(
        buf.cell((inner_left.saturating_add(2), rect.y))
            .expect("close right")
            .symbol(),
        "]",
        "expected close button to be bracketed"
    );
    assert_eq!(
        buf.cell((inner_right, rect.y))
            .expect("maximize slot")
            .symbol(),
        "═",
        "expected maximize button to be hidden for fixed-size windows"
    );
    assert_eq!(
        buf.cell((inner_right.saturating_sub(2), rect.y))
            .expect("minimize slot")
            .symbol(),
        "═",
        "expected minimize button to be hidden for fixed-size windows"
    );
}

#[test]
fn titlebar_buttons_use_bracketed_left_close_and_right_zoom() {
    let theme = Theme::dark();
    let bounds = Rect::new(0, 0, 40, 15);
    let rect = Rect::new(2, 2, 22, 7);

    let mut wm = WindowManager::new();
    let id = wm.add_window(
        Window::new(WindowKind::Normal, "Controls", rect, Box::new(DummyView)),
        bounds,
    );

    let backend = TestBackend::new(bounds.width, bounds.height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

    let buf = terminal.backend().buffer();
    let inner_left = rect.x.saturating_add(1);
    let inner_right = rect.x.saturating_add(rect.width).saturating_sub(2);
    let zoom_left = inner_right.saturating_sub(2);

    assert_eq!(
        buf.cell((inner_left, rect.y)).expect("close [").symbol(),
        "["
    );
    assert_eq!(
        buf.cell((inner_left.saturating_add(1), rect.y))
            .expect("close glyph")
            .symbol(),
        "■"
    );
    assert_eq!(
        buf.cell((inner_left.saturating_add(2), rect.y))
            .expect("close ]")
            .symbol(),
        "]"
    );
    assert_eq!(buf.cell((zoom_left, rect.y)).expect("zoom [").symbol(), "[");
    assert_eq!(
        buf.cell((zoom_left.saturating_add(1), rect.y))
            .expect("zoom glyph")
            .symbol(),
        "↑"
    );
    assert_eq!(
        buf.cell((zoom_left.saturating_add(2), rect.y))
            .expect("zoom ]")
            .symbol(),
        "]"
    );

    wm.toggle_maximize_focused(bounds);
    terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");
    let buf = terminal.backend().buffer();
    let maximized = wm.window_mut(id).expect("window").rect.get();
    let max_inner_right = maximized
        .x
        .saturating_add(maximized.width)
        .saturating_sub(2);
    let restore_glyph = max_inner_right.saturating_sub(1);
    assert_eq!(
        buf.cell((restore_glyph, maximized.y))
            .expect("restore glyph")
            .symbol(),
        "↕"
    );
}

#[test]
fn window_manager_apply_tree_ops_updates_dynamic_root() {
    let callbacks = CallbackRegistry::new();
    let root = ComponentSpec::new("Label")
        .with_id("root")
        .with_prop("text", ComponentValue::String("hello".into()));
    let window = Window::new_dynamic(
        WindowKind::Normal,
        "Dynamic",
        Rect::new(0, 0, 20, 5),
        root,
        callbacks,
    )
    .expect("dynamic window");

    let mut wm = WindowManager::new();
    let screen = Rect::new(0, 0, 80, 24);
    let id = wm.add_window(window, screen);

    wm.apply_tree_ops(
        id,
        &[TreeOp::SetProp {
            id: "root".into(),
            name: "text".into(),
            value: ComponentValue::String("bye".into()),
        }],
    )
    .expect("apply");

    let window = wm.window(id).expect("window");
    let root_spec = window.dynamic_root_spec().expect("dynamic root");
    assert_eq!(
        root_spec.props.get("text"),
        Some(&ComponentValue::String("bye".into()))
    );
}

fn normal_window(title: &str, rect: Rect) -> Window {
    Window::new(WindowKind::Normal, title, rect, Box::new(DummyView))
}

#[test]
fn cascade_offsets_windows_and_keeps_them_in_bounds() {
    let bounds = Rect::new(0, 1, 80, 22);
    let mut wm = WindowManager::new();
    let ids: Vec<_> = (0..3)
        .map(|i| wm.add_window(normal_window(&format!("W{i}"), Rect::new(40, 10, 10, 5)), bounds))
        .collect();

    wm.cascade(bounds);

    let rects: Vec<Rect> = ids
        .iter()
        .map(|id| wm.window(*id).expect("window").rect.get())
        .collect();
    // Each successive window is offset diagonally from the previous.
    assert_eq!(rects[0].x, bounds.x);
    assert_eq!(rects[0].y, bounds.y);
    assert!(rects[1].x > rects[0].x && rects[1].y > rects[0].y);
    assert!(rects[2].x > rects[1].x && rects[2].y > rects[1].y);
    // Everything stays inside the work area.
    for r in &rects {
        assert!(r.x >= bounds.x && r.y >= bounds.y);
        assert!(r.x + r.width <= bounds.x + bounds.width);
        assert!(r.y + r.height <= bounds.y + bounds.height);
    }
}

#[test]
fn cascade_un_maximizes_before_arranging() {
    let bounds = Rect::new(0, 1, 80, 22);
    let mut wm = WindowManager::new();
    let id = wm.add_window(normal_window("Max", Rect::new(5, 5, 20, 8)), bounds);
    wm.maximize_window(id, bounds);
    assert_eq!(wm.window(id).expect("window").state.get(), WindowState::Maximized);

    wm.cascade(bounds);
    assert_eq!(wm.window(id).expect("window").state.get(), WindowState::Normal);
}

#[test]
fn tile_partitions_work_area_into_a_grid() {
    let bounds = Rect::new(0, 1, 80, 22);
    let mut wm = WindowManager::new();
    let ids: Vec<_> = (0..4)
        .map(|i| wm.add_window(normal_window(&format!("W{i}"), Rect::new(0, 0, 10, 5)), bounds))
        .collect();

    wm.tile(bounds);

    let rects: Vec<Rect> = ids
        .iter()
        .map(|id| wm.window(*id).expect("window").rect.get())
        .collect();
    // 4 windows tile into a 2x2 grid: two distinct columns and two distinct rows.
    let xs: std::collections::BTreeSet<u16> = rects.iter().map(|r| r.x).collect();
    let ys: std::collections::BTreeSet<u16> = rects.iter().map(|r| r.y).collect();
    assert_eq!(xs.len(), 2);
    assert_eq!(ys.len(), 2);
    for r in &rects {
        assert!(r.x + r.width <= bounds.x + bounds.width);
        assert!(r.y + r.height <= bounds.y + bounds.height);
    }
}

#[test]
fn tile_respects_window_minimum_size_after_positioning() {
    // A single window with a large min size tiled into a small work area must keep
    // its minimum size (best-effort resize) while staying within bounds.
    let bounds = Rect::new(0, 1, 80, 22);
    let mut wm = WindowManager::new();
    let id = wm.add_window(
        Window::new(
            WindowKind::Normal,
            "Big",
            Rect::new(2, 2, 10, 5),
            Box::new(MinSizeView { min: (40, 14) }),
        ),
        bounds,
    );

    wm.tile(bounds);

    let rect = wm.window(id).expect("window").rect.get();
    // Border chrome adds 1 cell per side on top of the view minimum.
    assert!(rect.width >= 42, "width {} should not break min size", rect.width);
    assert!(rect.height >= 16, "height {} should not break min size", rect.height);
    assert!(rect.x + rect.width <= bounds.x + bounds.width);
    assert!(rect.y + rect.height <= bounds.y + bounds.height);
}

#[test]
fn minimize_all_then_restore_all_round_trips_state() {
    let bounds = Rect::new(0, 1, 80, 22);
    let mut wm = WindowManager::new();
    let ids: Vec<_> = (0..3)
        .map(|i| wm.add_window(normal_window(&format!("W{i}"), Rect::new(2, 2, 20, 6)), bounds))
        .collect();

    wm.minimize_all();
    for id in &ids {
        assert_eq!(wm.window(*id).expect("window").state.get(), WindowState::Minimized);
    }

    wm.restore_all();
    for id in &ids {
        assert_eq!(wm.window(*id).expect("window").state.get(), WindowState::Normal);
    }
}

#[test]
fn close_all_removes_every_plain_window() {
    let bounds = Rect::new(0, 1, 80, 22);
    let mut wm = WindowManager::new();
    for i in 0..3 {
        wm.add_window(normal_window(&format!("W{i}"), Rect::new(2, 2, 20, 6)), bounds);
    }
    assert_eq!(wm.windows().len(), 3);

    wm.close_all();
    assert_eq!(wm.windows().len(), 0);
}

#[test]
fn focus_previous_steps_back_and_inverts_focus_next() {
    let bounds = Rect::new(0, 1, 80, 22);
    let mut wm = WindowManager::new();
    let _id0 = wm.add_window(normal_window("W0", Rect::new(2, 2, 20, 6)), bounds);
    let id1 = wm.add_window(normal_window("W1", Rect::new(24, 2, 20, 6)), bounds);
    let id2 = wm.add_window(normal_window("W2", Rect::new(46, 2, 20, 6)), bounds);

    // Newest window is focused on add; stepping back selects the one beneath it.
    assert_eq!(wm.focused(), Some(id2));
    wm.focus_previous();
    assert_eq!(wm.focused(), Some(id1));

    // focus_previous undoes a focus_next (the pair round-trips focus).
    let before_pair = wm.focused();
    wm.focus_next();
    wm.focus_previous();
    assert_eq!(wm.focused(), before_pair);
}
