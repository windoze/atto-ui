use std::sync::{Arc, Mutex};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use crate::theme::Theme;
use crate::wm::WindowId;

use super::geom::{
    TabDirection, align_within, contains, mouse_coords_local_to_area, position_anchored,
    tab_direction_for_event,
};
use super::{
    Align, Anchor, AnchorPlacement, Component, ComponentAction, ComponentContext, DynamicTree,
    EdgeInsets, EventHandling, EventOutcome, EventResult, FocusNav, Grid, HStack, Layout,
    LayoutParams, MouseCoordinateSpace, ScrollConfig, Scrollable, ScrollbarHost,
    ScrollbarVisibility, Size, Splitter, SplitterOrientation, TabMode, VStack,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecordedEvent {
    Key(KeyCode),
    Mouse { column: u16, row: u16 },
}

#[derive(Clone)]
struct RecordingView {
    focusable: bool,
    desired_width: Option<u16>,
    desired_height: Option<u16>,
    outcome: EventOutcome,
    events: Arc<Mutex<Vec<RecordedEvent>>>,
}

#[derive(Clone, Copy, Debug)]
struct SizedView {
    min_w: u16,
    min_h: u16,
}

impl SizedView {
    fn new(min_w: u16, min_h: u16) -> Self {
        Self { min_w, min_h }
    }
}

impl Component for SizedView {
    fn draw(&mut self, _frame: &mut ratatui::Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
}

impl Layout for SizedView {
    fn min_width(&self) -> u16 {
        self.min_w
    }

    fn min_height(&self) -> u16 {
        self.min_h
    }
}

crate::impl_component_default_traits!(SizedView => Scrollable, FocusNav, DynamicTree, EventHandling);

impl RecordingView {
    fn new(events: Arc<Mutex<Vec<RecordedEvent>>>) -> Self {
        Self {
            focusable: true,
            desired_width: None,
            desired_height: None,
            outcome: EventOutcome::Consumed,
            events,
        }
    }

    fn with_focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
        self
    }

    fn with_desired_width(mut self, width: Option<u16>) -> Self {
        self.desired_width = width;
        self
    }

    fn with_desired_height(mut self, height: Option<u16>) -> Self {
        self.desired_height = height;
        self
    }

    fn with_outcome(mut self, outcome: EventOutcome) -> Self {
        self.outcome = outcome;
        self
    }
}

impl Component for RecordingView {
    fn draw(&mut self, _frame: &mut ratatui::Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
}

impl Layout for RecordingView {
    fn desired_width(&self) -> Option<u16> {
        self.desired_width
    }

    fn desired_height(&self) -> Option<u16> {
        self.desired_height
    }
}

impl FocusNav for RecordingView {
    fn is_focusable(&self) -> bool {
        self.focusable
    }
}

impl EventHandling for RecordingView {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        match event {
            Event::Key(KeyEvent { code, .. }) => {
                self.events
                    .lock()
                    .expect("events lock")
                    .push(RecordedEvent::Key(*code));
            }
            Event::Mouse(m) => {
                self.events
                    .lock()
                    .expect("events lock")
                    .push(RecordedEvent::Mouse {
                        column: m.column,
                        row: m.row,
                    });
            }
            _ => {}
        }

        EventResult {
            outcome: self.outcome,
            action: ComponentAction::None,
            capture: crate::composable::Capture::None,
        }
    }
}

crate::impl_component_default_traits!(RecordingView => Scrollable, DynamicTree);

/// Models a real Checkbox/Button: requests pointer capture on mouse-down and,
/// on release, only toggles if the pointer is **inside its own bounds** — using
/// the coordinate space the parent forwarded (exactly like `Checkbox::hit`).
/// This bounds check is what breaks if the capture route fails to translate
/// coordinates into the child's local space.
#[derive(Clone)]
struct CapturingView {
    events: Arc<Mutex<Vec<RecordedEvent>>>,
    toggled: Arc<Mutex<bool>>,
    last_area: Arc<Mutex<Option<Rect>>>,
}

impl CapturingView {
    fn new(events: Arc<Mutex<Vec<RecordedEvent>>>, toggled: Arc<Mutex<bool>>) -> Self {
        Self {
            events,
            toggled,
            last_area: Arc::new(Mutex::new(None)),
        }
    }

    fn hit(&self, m: &MouseEvent, space: MouseCoordinateSpace) -> bool {
        let area = self.last_area.lock().expect("area lock");
        area.is_some_and(|area| mouse_coords_local_to_area(area, *m, space).is_some())
    }
}

impl Component for CapturingView {
    fn draw(&mut self, _frame: &mut ratatui::Frame<'_>, area: Rect, _ctx: ComponentContext<'_>) {
        *self.last_area.lock().expect("area lock") = Some(area);
    }
}

impl Layout for CapturingView {
    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }
}

impl FocusNav for CapturingView {
    fn is_focusable(&self) -> bool {
        true
    }
}

impl EventHandling for CapturingView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        use crate::composable::Capture;
        if let Event::Mouse(m) = event {
            self.events
                .lock()
                .expect("events lock")
                .push(RecordedEvent::Mouse {
                    column: m.column,
                    row: m.row,
                });
            match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    return EventResult::consumed().with_capture(Capture::Request);
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    if self.hit(m, ctx.mouse_coordinate_space) {
                        *self.toggled.lock().expect("toggle lock") ^= true;
                        return EventResult::changed().with_capture(Capture::Release);
                    }
                    return EventResult::consumed().with_capture(Capture::Release);
                }
                _ => return EventResult::consumed(),
            }
        }
        EventResult::ignored()
    }
}

crate::impl_component_default_traits!(CapturingView => Scrollable, DynamicTree);

/// Reproduces the settings-dialog checkbox bug at the framework level. A
/// capturing widget lives inside a section VStack that is tall enough to be
/// clipped by a scrollable root VStack. Once scrolled, the section renders
/// through the offscreen (`draw_component_region`) path, so the widget's
/// `last_area` is offscreen-local, not screen-absolute.
///
/// A mouse-down reaches the widget (hit-test route translates coordinates) and
/// the widget requests capture. The mouse-up then travels the *capture* route,
/// which must apply the same per-level coordinate translation — otherwise the
/// widget never sees a usable up, never toggles, and never releases capture.
#[test]
fn clipped_captured_child_receives_translated_mouse_up() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let toggled = Arc::new(Mutex::new(false));

    // Section taller than the viewport so it is always drawn clipped/offscreen.
    // Fillers precede the capturing child so the child sits at a non-zero row
    // within the section — making the per-level coordinate translation matter.
    let mut section = VStack::new();
    for _ in 0..4 {
        section.add_child_with_layout(
            Box::new(SizedView::new(1, 1)),
            LayoutParams {
                height: Size::Fixed(1),
                ..LayoutParams::default()
            },
        );
    }
    section.add_child_with_layout(
        Box::new(CapturingView::new(
            Arc::clone(&events),
            Arc::clone(&toggled),
        )),
        LayoutParams {
            height: Size::Fixed(1),
            ..LayoutParams::default()
        },
    );
    for _ in 0..4 {
        section.add_child_with_layout(
            Box::new(SizedView::new(1, 1)),
            LayoutParams {
                height: Size::Fixed(1),
                ..LayoutParams::default()
            },
        );
    }

    let mut root = VStack::new().with_scrollable(true);
    for _ in 0..6 {
        root.add_child_with_layout(
            Box::new(SizedView::new(1, 1)),
            LayoutParams {
                height: Size::Fixed(1),
                ..LayoutParams::default()
            },
        );
    }
    root.add_child_with_layout(Box::new(section), LayoutParams::default());

    let area = Rect::new(0, 0, 20, 6);
    draw_view(&mut root, area);
    // Scroll so the section's top is clipped above the viewport: the section is
    // drawn through the offscreen path with a non-zero source offset, so its
    // descendants' `last_area` no longer aligns with absolute screen coords.
    root.set_scroll_offset(0, 8);
    draw_view(&mut root, area);

    // Find the viewport row where a mouse-down reaches the capturing child.
    let mut hit_row = None;
    for row in 0..area.height {
        events.lock().expect("lock").clear();
        let _ = root.handle_event(
            &Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 1,
                row,
                modifiers: KeyModifiers::NONE,
            }),
            test_context(),
        );
        if !events.lock().expect("lock").is_empty() {
            hit_row = Some(row);
            break;
        }
        // No down landed here; nothing captured, safe to continue.
    }
    let row = hit_row.expect("capturing child reachable by mouse-down after scroll");

    // Now deliver the mouse-up. Without translation on the capture route, this
    // up misses the offscreen-local child and never toggles it.
    *toggled.lock().expect("lock") = false;
    let _ = root.handle_event(
        &Event::Mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: 1,
            row,
            modifiers: KeyModifiers::NONE,
        }),
        test_context(),
    );
    assert!(
        *toggled.lock().expect("toggle lock"),
        "captured child in a clipped section must receive its mouse-up and toggle"
    );
}

fn draw_view(view: &mut dyn Component, area: Rect) {
    let theme = Theme::dark();
    let ctx = ComponentContext {
        theme: &theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };

    let backend = TestBackend::new(80, 40);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
}

fn test_context() -> ComponentContext<'static> {
    let theme = Box::leak(Box::new(Theme::dark()));
    ComponentContext {
        theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    }
}

#[test]
fn geom_contains_checks_bounds() {
    let rect = Rect {
        x: 2,
        y: 3,
        width: 4,
        height: 2,
    };
    assert!(contains(rect, 2, 3));
    assert!(contains(rect, 5, 4));
    assert!(!contains(rect, 6, 4));
    assert!(!contains(rect, 5, 5));
}

#[test]
fn geom_mouse_coords_local_to_area_accepts_local_coords() {
    let area = Rect {
        x: 10,
        y: 20,
        width: 5,
        height: 4,
    };
    let abs = MouseEvent {
        kind: MouseEventKind::Moved,
        column: 12,
        row: 22,
        modifiers: KeyModifiers::empty(),
    };
    assert_eq!(
        mouse_coords_local_to_area(area, abs, MouseCoordinateSpace::Absolute),
        Some((2, 2))
    );

    let local = MouseEvent {
        kind: MouseEventKind::Moved,
        column: 3,
        row: 1,
        modifiers: KeyModifiers::empty(),
    };
    assert_eq!(
        mouse_coords_local_to_area(Rect::new(0, 0, 4, 3), local, MouseCoordinateSpace::Local),
        Some((3, 1))
    );
}

#[test]
fn geom_mouse_coords_local_to_area_rejects_ambiguous_absolute_coords() {
    let area = Rect::new(10, 20, 5, 4);
    let event = MouseEvent {
        kind: MouseEventKind::Moved,
        column: 3,
        row: 1,
        modifiers: KeyModifiers::empty(),
    };

    assert_eq!(
        mouse_coords_local_to_area(area, event, MouseCoordinateSpace::Absolute),
        None
    );
    assert_eq!(
        mouse_coords_local_to_area(area, event, MouseCoordinateSpace::Local),
        Some((3, 1))
    );
}

#[test]
fn geom_position_anchored_clamps_to_content() {
    let rect = position_anchored((10, 6), (4, 4), Anchor::BottomRight, 5, 5);
    assert_eq!(rect.x, 6);
    assert_eq!(rect.y, 2);
}

#[test]
fn geom_align_within_centers_child() {
    let slot = Rect::new(0, 0, 10, 6);
    let aligned = align_within(slot, (4, 2), Align::Center, Align::Center);
    assert_eq!(aligned.x, 3);
    assert_eq!(aligned.y, 2);
    assert_eq!(aligned.width, 4);
    assert_eq!(aligned.height, 2);
}

#[test]
fn geom_tab_direction_detects_shift_tab() {
    let event = Event::Key(KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::empty(),
    });
    assert_eq!(tab_direction_for_event(&event), Some(TabDirection::Prev));
}

#[test]
fn view_hierarchy_sets_parent_ids_for_children() {
    let mut vstack = VStack::new();

    let ev1 = Arc::new(Mutex::new(Vec::new()));
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::clone(&ev1))),
        LayoutParams::default(),
    );

    let ev2 = Arc::new(Mutex::new(Vec::new()));
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::clone(&ev2)).with_focusable(false)),
        LayoutParams::default(),
    );

    let children = vstack.children();
    assert_eq!(children.len(), 2);

    let vstack_id = children[0].parent.expect("parent id");
    assert_ne!(children[0].id, vstack_id);
    assert_eq!(children[1].parent, Some(vstack_id));
}

#[test]
fn nested_hierarchy_preserves_parent_ids() {
    let mut inner = VStack::new();
    let leaf_events = Arc::new(Mutex::new(Vec::new()));
    inner.add_child_with_layout(
        Box::new(RecordingView::new(Arc::clone(&leaf_events))),
        LayoutParams::default(),
    );
    let leaf_id = inner.children()[0].id;
    let inner_id = inner.children()[0].parent.expect("inner id");

    let mut outer = VStack::new();
    outer.add_child_with_layout(Box::new(inner), LayoutParams::default());
    let outer_children = outer.children();
    assert_eq!(outer_children.len(), 1);

    let outer_id = outer_children[0].parent.expect("outer id");
    assert_ne!(
        outer_id, inner_id,
        "expected nested containers to have distinct ids"
    );

    let inner_children = outer_children[0].view.children();
    assert_eq!(inner_children.len(), 1);
    assert_eq!(inner_children[0].id, leaf_id);
    assert_eq!(inner_children[0].parent, Some(inner_id));
}

#[test]
fn vstack_layout_fixed_heights() {
    let mut vstack = VStack::new();
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(5),
            ..LayoutParams::default()
        },
    );
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(10),
            ..LayoutParams::default()
        },
    );
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(5),
            ..LayoutParams::default()
        },
    );

    draw_view(&mut vstack, Rect::new(0, 0, 40, 20));

    let children = vstack.children();
    assert_eq!(children.len(), 3);
    assert_eq!(children[0].bounds(), Rect::new(0, 0, 40, 5));
    assert_eq!(children[1].bounds(), Rect::new(0, 5, 40, 10));
    assert_eq!(children[2].bounds(), Rect::new(0, 15, 40, 5));
}

#[test]
fn splitter_vertical_layout_respects_split_position() {
    let mut splitter = Splitter::vertical(
        RecordingView::new(Arc::new(Mutex::new(Vec::new()))),
        RecordingView::new(Arc::new(Mutex::new(Vec::new()))),
    )
    .split_position(12u16);

    draw_view(&mut splitter, Rect::new(0, 0, 40, 10));

    let children = splitter.children();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].bounds(), Rect::new(0, 0, 12, 10));
    assert_eq!(children[1].bounds(), Rect::new(13, 0, 27, 10));
}

#[test]
fn splitter_horizontal_layout_respects_split_position() {
    let mut splitter = Splitter::horizontal(
        RecordingView::new(Arc::new(Mutex::new(Vec::new()))),
        RecordingView::new(Arc::new(Mutex::new(Vec::new()))),
    )
    .split_position(4u16);

    draw_view(&mut splitter, Rect::new(0, 0, 20, 12));

    let children = splitter.children();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].bounds(), Rect::new(0, 0, 20, 4));
    assert_eq!(children[1].bounds(), Rect::new(0, 5, 20, 7));
}

#[test]
fn splitter_drag_clamps_to_min_sizes() {
    let mut splitter = Splitter::new(
        SplitterOrientation::Vertical,
        SizedView::new(8, 1),
        SizedView::new(8, 1),
    )
    .min_sizes(8u16, 8u16)
    .split_position(10u16);

    let area = Rect::new(0, 0, 30, 5);
    draw_view(&mut splitter, area);

    let ctx = test_context();
    let down = Event::Mouse(MouseEvent {
        column: 10,
        row: 0,
        kind: MouseEventKind::Down(MouseButton::Left),
        modifiers: KeyModifiers::empty(),
    });
    splitter.handle_event(&down, ctx);

    let drag = Event::Mouse(MouseEvent {
        column: 1,
        row: 0,
        kind: MouseEventKind::Drag(MouseButton::Left),
        modifiers: KeyModifiers::empty(),
    });
    splitter.handle_event(&drag, ctx);

    draw_view(&mut splitter, area);

    let children = splitter.children();
    assert_eq!(children[0].bounds(), Rect::new(0, 0, 8, 5));
    assert_eq!(children[1].bounds(), Rect::new(9, 0, 21, 5));
}

#[test]
fn vstack_layout_weighted_split() {
    let mut vstack = VStack::new();
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Weight(1),
            ..LayoutParams::default()
        },
    );
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Weight(2),
            ..LayoutParams::default()
        },
    );

    draw_view(&mut vstack, Rect::new(0, 0, 20, 30));
    let children = vstack.children();
    assert_eq!(children[0].bounds(), Rect::new(0, 0, 20, 10));
    assert_eq!(children[1].bounds(), Rect::new(0, 10, 20, 20));
}

#[test]
fn vstack_layout_clamps_overflow() {
    let mut vstack = VStack::new();
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(6),
            ..LayoutParams::default()
        },
    );
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(6),
            ..LayoutParams::default()
        },
    );

    draw_view(&mut vstack, Rect::new(0, 0, 10, 10));
    let children = vstack.children();
    assert_eq!(children[0].bounds(), Rect::new(0, 0, 10, 6));
    assert_eq!(children[1].bounds(), Rect::new(0, 6, 10, 4));
}

#[test]
fn vstack_padding_reduces_content_area() {
    let mut vstack = VStack::new().with_padding(EdgeInsets::all(2));
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams::default(),
    );

    draw_view(&mut vstack, Rect::new(0, 0, 20, 10));
    let child = &vstack.children()[0];
    assert_eq!(child.bounds(), Rect::new(0, 0, 16, 6));
}

#[test]
fn vstack_margins_reserve_space_around_child() {
    let mut vstack = VStack::new();
    let margin = EdgeInsets {
        top: 1,
        right: 1,
        bottom: 1,
        left: 1,
    };
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(2),
            margin,
            ..LayoutParams::default()
        },
    );
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(2),
            margin,
            ..LayoutParams::default()
        },
    );

    draw_view(&mut vstack, Rect::new(0, 0, 20, 10));

    let children = vstack.children();
    assert_eq!(children[0].bounds(), Rect::new(1, 1, 18, 2));
    assert_eq!(children[1].bounds(), Rect::new(1, 5, 18, 2));
}

#[test]
fn vstack_alignment_centers_child_in_slot() {
    let mut vstack = VStack::new();
    vstack.add_child_with_layout(
        Box::new(
            RecordingView::new(Arc::new(Mutex::new(Vec::new())))
                .with_desired_width(Some(4))
                .with_desired_height(Some(1)),
        ),
        LayoutParams {
            width: Size::Content,
            height: Size::Fixed(1),
            align_x: Align::Center,
            ..LayoutParams::default()
        },
    );

    draw_view(&mut vstack, Rect::new(0, 0, 20, 5));

    let child = &vstack.children()[0];
    assert_eq!(child.bounds(), Rect::new(8, 0, 4, 1));
}

#[test]
fn vstack_anchor_positions_overlay_and_does_not_affect_flow() {
    let mut vstack = VStack::new();
    vstack.add_child_with_layout(
        Box::new(
            RecordingView::new(Arc::new(Mutex::new(Vec::new())))
                .with_desired_width(Some(3))
                .with_desired_height(Some(2)),
        ),
        LayoutParams {
            width: Size::Fixed(3),
            height: Size::Fixed(2),
            anchor: Some(AnchorPlacement {
                anchor: Anchor::TopRight,
                offset_x: 0,
                offset_y: 0,
            }),
            ..LayoutParams::default()
        },
    );
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(5),
            ..LayoutParams::default()
        },
    );

    draw_view(&mut vstack, Rect::new(0, 0, 20, 10));

    let children = vstack.children();
    assert_eq!(children.len(), 2);

    // Anchored overlays do not affect the flow child layout.
    assert_eq!(children[1].bounds(), Rect::new(0, 0, 20, 5));

    // Anchored child is positioned relative to the parent's content size.
    assert_eq!(children[0].bounds(), Rect::new(17, 0, 3, 2));
}

#[test]
fn vstack_anchor_repositions_on_resize() {
    let mut vstack = VStack::new();
    vstack.add_child_with_layout(
        Box::new(
            RecordingView::new(Arc::new(Mutex::new(Vec::new())))
                .with_desired_width(Some(3))
                .with_desired_height(Some(2)),
        ),
        LayoutParams {
            width: Size::Fixed(3),
            height: Size::Fixed(2),
            anchor: Some(AnchorPlacement {
                anchor: Anchor::TopRight,
                offset_x: 0,
                offset_y: 0,
            }),
            ..LayoutParams::default()
        },
    );

    draw_view(&mut vstack, Rect::new(0, 0, 20, 5));
    assert_eq!(vstack.children()[0].bounds(), Rect::new(17, 0, 3, 2));

    draw_view(&mut vstack, Rect::new(0, 0, 30, 5));
    assert_eq!(vstack.children()[0].bounds(), Rect::new(27, 0, 3, 2));
}

#[test]
fn event_routing_translates_absolute_mouse_coords_to_child_local() {
    let leaf_events = Arc::new(Mutex::new(Vec::new()));
    let leaf = RecordingView::new(Arc::clone(&leaf_events)).with_outcome(EventOutcome::Consumed);

    let mut inner = VStack::new();
    inner.add_child_with_layout(Box::new(leaf), LayoutParams::default());

    let mut outer = VStack::new().with_padding(EdgeInsets::all(1));
    outer.add_child_with_layout(
        Box::new(inner),
        LayoutParams {
            height: Size::Fixed(4),
            ..LayoutParams::default()
        },
    );

    // Draw at a non-zero origin so the test exercises absolute-to-local translation.
    draw_view(&mut outer, Rect::new(10, 5, 20, 10));

    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 10 + 1 + 2, // area.x + padding.left + 2
        row: 5 + 1 + 1,     // area.y + padding.top + 1
        modifiers: KeyModifiers::NONE,
    });

    let theme = Theme::dark();
    let ctx = ComponentContext {
        theme: &theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let res = outer.handle_event(&click, ctx);
    assert!(res.is_consumed());

    let recorded = leaf_events.lock().expect("events lock").clone();
    assert_eq!(recorded, vec![RecordedEvent::Mouse { column: 2, row: 1 }]);
}

#[test]
fn capture_phase_consumes_tab_before_children_receive_it() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut vstack = VStack::new();
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::clone(&events))),
        LayoutParams::default(),
    );

    draw_view(&mut vstack, Rect::new(0, 0, 10, 5));

    let tab = Event::Key(KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });

    let theme = Theme::dark();
    let ctx = ComponentContext {
        theme: &theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };
    let res = vstack.handle_event(&tab, ctx);
    assert!(res.is_consumed());
    assert!(events.lock().expect("events lock").is_empty());
}

#[test]
fn keyboard_events_route_to_focused_child() {
    let a = Arc::new(Mutex::new(Vec::new()));
    let b = Arc::new(Mutex::new(Vec::new()));

    let mut vstack = VStack::new();
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::clone(&a))),
        LayoutParams::default(),
    );
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::clone(&b))),
        LayoutParams::default(),
    );

    draw_view(&mut vstack, Rect::new(0, 0, 10, 5));

    let theme = Theme::dark();
    let ctx = ComponentContext {
        theme: &theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };

    let key_a = Event::Key(KeyEvent {
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    assert!(vstack.handle_event(&key_a, ctx).is_consumed());

    let tab = Event::Key(KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    assert!(vstack.handle_event(&tab, ctx).is_consumed());

    let key_b = Event::Key(KeyEvent {
        code: KeyCode::Char('b'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    assert!(vstack.handle_event(&key_b, ctx).is_consumed());

    let rec_a = a.lock().expect("events lock").clone();
    let rec_b = b.lock().expect("events lock").clone();

    assert_eq!(rec_a, vec![RecordedEvent::Key(KeyCode::Char('a'))]);
    assert_eq!(rec_b, vec![RecordedEvent::Key(KeyCode::Char('b'))]);
}

#[test]
fn tab_traversal_enters_nested_container_before_advancing_to_next_sibling() {
    let a = Arc::new(Mutex::new(Vec::new()));
    let b = Arc::new(Mutex::new(Vec::new()));
    let c = Arc::new(Mutex::new(Vec::new()));

    let mut inner = HStack::new();
    inner.add_child_with_layout(
        Box::new(RecordingView::new(Arc::clone(&a))),
        LayoutParams::default(),
    );
    inner.add_child_with_layout(
        Box::new(RecordingView::new(Arc::clone(&b))),
        LayoutParams::default(),
    );

    let mut root = VStack::new();
    root.add_child_with_layout(Box::new(inner), LayoutParams::default());
    root.add_child_with_layout(
        Box::new(RecordingView::new(Arc::clone(&c))),
        LayoutParams::default(),
    );

    draw_view(&mut root, Rect::new(0, 0, 20, 5));

    let theme = Theme::dark();
    let ctx = ComponentContext {
        theme: &theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };

    let key_1 = Event::Key(KeyEvent {
        code: KeyCode::Char('1'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    assert!(root.handle_event(&key_1, ctx).is_consumed());

    let tab = Event::Key(KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    assert!(root.handle_event(&tab, ctx).is_consumed());

    let key_2 = Event::Key(KeyEvent {
        code: KeyCode::Char('2'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    assert!(root.handle_event(&key_2, ctx).is_consumed());

    assert!(root.handle_event(&tab, ctx).is_consumed());

    let key_3 = Event::Key(KeyEvent {
        code: KeyCode::Char('3'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    assert!(root.handle_event(&key_3, ctx).is_consumed());

    let rec_a = a.lock().expect("events lock").clone();
    let rec_b = b.lock().expect("events lock").clone();
    let rec_c = c.lock().expect("events lock").clone();

    assert_eq!(rec_a, vec![RecordedEvent::Key(KeyCode::Char('1'))]);
    assert_eq!(rec_b, vec![RecordedEvent::Key(KeyCode::Char('2'))]);
    assert_eq!(rec_c, vec![RecordedEvent::Key(KeyCode::Char('3'))]);
}

#[test]
fn tab_traversal_respects_tab_index_over_insertion_order() {
    let a = Arc::new(Mutex::new(Vec::new()));
    let b = Arc::new(Mutex::new(Vec::new()));
    let c = Arc::new(Mutex::new(Vec::new()));

    let mut vstack = VStack::new();
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::clone(&a))),
        LayoutParams {
            tab_index: Some(0),
            ..LayoutParams::default()
        },
    );
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::clone(&b))),
        LayoutParams {
            tab_index: Some(2),
            ..LayoutParams::default()
        },
    );
    vstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::clone(&c))),
        LayoutParams {
            tab_index: Some(1),
            ..LayoutParams::default()
        },
    );

    draw_view(&mut vstack, Rect::new(0, 0, 10, 5));

    let theme = Theme::dark();
    let ctx = ComponentContext {
        theme: &theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };

    let tab = Event::Key(KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    assert!(vstack.handle_event(&tab, ctx).is_consumed());

    let key_c = Event::Key(KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    assert!(vstack.handle_event(&key_c, ctx).is_consumed());

    assert!(a.lock().expect("events lock").is_empty());
    assert!(b.lock().expect("events lock").is_empty());
    assert_eq!(
        c.lock().expect("events lock").clone(),
        vec![RecordedEvent::Key(KeyCode::Char('c'))]
    );
}

#[test]
fn hstack_layout_fixed_widths() {
    let mut hstack = HStack::new();
    hstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            width: Size::Fixed(10),
            ..LayoutParams::default()
        },
    );
    hstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            width: Size::Fixed(20),
            ..LayoutParams::default()
        },
    );
    hstack.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            width: Size::Fixed(10),
            ..LayoutParams::default()
        },
    );

    draw_view(&mut hstack, Rect::new(0, 0, 40, 5));
    let children = hstack.children();
    assert_eq!(children.len(), 3);
    assert_eq!(children[0].bounds(), Rect::new(0, 0, 10, 5));
    assert_eq!(children[1].bounds(), Rect::new(10, 0, 20, 5));
    assert_eq!(children[2].bounds(), Rect::new(30, 0, 10, 5));
}

#[test]
fn grid_layout_columns_and_row_heights() {
    let mut grid = Grid::new().with_columns(3usize);
    grid.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new()))).with_desired_height(Some(1))),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );
    grid.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new()))).with_desired_height(Some(3))),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );
    grid.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new()))).with_desired_height(Some(2))),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );
    grid.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new()))).with_desired_height(Some(1))),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );
    grid.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new()))).with_desired_height(Some(4))),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );

    draw_view(&mut grid, Rect::new(0, 0, 60, 20));
    let children = grid.children();
    assert_eq!(children.len(), 5);

    // 60 columns / 3 columns = 20 columns per cell.
    assert_eq!(children[0].bounds().width, 20);
    assert_eq!(children[1].bounds().x, 20);
    assert_eq!(children[2].bounds().x, 40);

    // Row height should be the tallest child in the row (3 for first row, 4 for second row).
    assert_eq!(children[0].bounds().height, 1);
    assert_eq!(children[1].bounds().height, 3);
    assert_eq!(children[2].bounds().height, 2);
    assert_eq!(children[3].bounds().y, 3);
    assert_eq!(children[4].bounds().y, 3);
    assert_eq!(children[4].bounds().height, 4);
}

#[test]
fn vstack_desired_height_includes_padding_spacing_margins_and_intrinsic_children() {
    #[derive(Default)]
    struct MinHeightView {
        min_h: u16,
        desired_h: Option<u16>,
    }

    impl Component for MinHeightView {
        fn draw(
            &mut self,
            _frame: &mut ratatui::Frame<'_>,
            _area: Rect,
            _ctx: ComponentContext<'_>,
        ) {
        }
    }

    impl Layout for MinHeightView {
        fn min_height(&self) -> u16 {
            self.min_h
        }

        fn desired_height(&self) -> Option<u16> {
            self.desired_h
        }
    }

    crate::impl_component_default_traits!(MinHeightView => Scrollable, FocusNav, DynamicTree, EventHandling);

    let mut vstack = VStack::new()
        .with_padding(EdgeInsets {
            top: 2,
            right: 0,
            bottom: 3,
            left: 0,
        })
        .with_spacing(1u16);

    vstack.add_child_with_layout(
        Box::new(MinHeightView {
            min_h: 0,
            desired_h: Some(1),
        }),
        LayoutParams {
            height: Size::Content,
            margin: EdgeInsets {
                top: 1,
                bottom: 2,
                ..EdgeInsets::ZERO
            },
            ..LayoutParams::default()
        },
    );

    // Fixed height < min height should contribute min height.
    vstack.add_child_with_layout(
        Box::new(MinHeightView {
            min_h: 6,
            desired_h: Some(10),
        }),
        LayoutParams {
            height: Size::Fixed(4),
            ..LayoutParams::default()
        },
    );

    // Fill flexes, so it contributes only its min height to the desired size.
    vstack.add_child_with_layout(
        Box::new(MinHeightView {
            min_h: 2,
            desired_h: Some(999),
        }),
        LayoutParams {
            height: Size::Fill,
            margin: EdgeInsets {
                top: 1,
                bottom: 1,
                ..EdgeInsets::ZERO
            },
            ..LayoutParams::default()
        },
    );

    // Anchored children should not affect the flow desired size.
    vstack.add_child_with_layout(
        Box::new(MinHeightView {
            min_h: 50,
            desired_h: Some(50),
        }),
        LayoutParams {
            height: Size::Fixed(50),
            anchor: Some(AnchorPlacement {
                anchor: Anchor::TopLeft,
                offset_x: 0,
                offset_y: 0,
            }),
            ..LayoutParams::default()
        },
    );

    // Expected:
    // padding: top+bottom = 5
    // child1: margin 3 + height 1 = 4
    // spacing: 1
    // child2: fixed 4 -> min 6 = 6
    // spacing: 1
    // child3: margin 2 + min 2 = 4  (Fill flexes, contributes only min)
    // total = 5 + 4 + 1 + 6 + 1 + 4 = 21
    assert_eq!(vstack.desired_height(), Some(21));
}

#[test]
fn vstack_min_height_uses_children_min_heights_not_desired_heights() {
    #[derive(Default)]
    struct SizedView {
        min_h: u16,
        desired_h: Option<u16>,
    }

    impl Component for SizedView {
        fn draw(
            &mut self,
            _frame: &mut ratatui::Frame<'_>,
            _area: Rect,
            _ctx: ComponentContext<'_>,
        ) {
        }
    }

    impl Layout for SizedView {
        fn min_height(&self) -> u16 {
            self.min_h
        }

        fn desired_height(&self) -> Option<u16> {
            self.desired_h
        }
    }

    crate::impl_component_default_traits!(SizedView => Scrollable, FocusNav, DynamicTree, EventHandling);

    let mut vstack = VStack::new().with_spacing(1u16);
    vstack.add_child_with_layout(
        Box::new(SizedView {
            min_h: 2,
            desired_h: Some(10),
        }),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );
    vstack.add_child_with_layout(
        Box::new(SizedView {
            min_h: 2,
            desired_h: Some(10),
        }),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );

    assert_eq!(
        vstack.min_height(),
        5,
        "min height should be sum(mins)+spacing (2 + 1 + 2)"
    );
}

#[test]
fn vstack_layout_at_min_height_keeps_all_children_visible() {
    #[derive(Default)]
    struct SizedView {
        min_h: u16,
        desired_h: Option<u16>,
    }

    impl Component for SizedView {
        fn draw(
            &mut self,
            _frame: &mut ratatui::Frame<'_>,
            _area: Rect,
            _ctx: ComponentContext<'_>,
        ) {
        }
    }

    impl Layout for SizedView {
        fn min_height(&self) -> u16 {
            self.min_h
        }

        fn desired_height(&self) -> Option<u16> {
            self.desired_h
        }
    }

    crate::impl_component_default_traits!(SizedView => Scrollable, FocusNav, DynamicTree, EventHandling);

    let mut vstack = VStack::new().with_spacing(1u16);
    vstack.add_child_with_layout(
        Box::new(SizedView {
            min_h: 2,
            desired_h: Some(10),
        }),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );
    vstack.add_child_with_layout(
        Box::new(SizedView {
            min_h: 2,
            desired_h: Some(10),
        }),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );

    // Height equals min_height() so both children should still be laid out at min height.
    draw_view(&mut vstack, Rect::new(0, 0, 20, 5));
    let children = vstack.children();
    assert_eq!(children.len(), 2);

    assert_eq!(children[0].bounds().y, 0);
    assert_eq!(children[0].bounds().height, 2);
    assert_eq!(
        children[1].bounds().y,
        3,
        "expected spacing row between children"
    );
    assert_eq!(children[1].bounds().height, 2);
}

#[test]
fn hstack_layout_at_min_width_keeps_all_children_visible() {
    #[derive(Default)]
    struct SizedView {
        min_w: u16,
        desired_w: Option<u16>,
    }

    impl Component for SizedView {
        fn draw(
            &mut self,
            _frame: &mut ratatui::Frame<'_>,
            _area: Rect,
            _ctx: ComponentContext<'_>,
        ) {
        }
    }

    impl Layout for SizedView {
        fn min_width(&self) -> u16 {
            self.min_w
        }

        fn desired_width(&self) -> Option<u16> {
            self.desired_w
        }
    }

    crate::impl_component_default_traits!(SizedView => Scrollable, FocusNav, DynamicTree, EventHandling);

    let mut hstack = HStack::new().with_spacing(1u16);
    hstack.add_child_with_layout(
        Box::new(SizedView {
            min_w: 2,
            desired_w: Some(10),
        }),
        LayoutParams {
            width: Size::Content,
            ..LayoutParams::default()
        },
    );
    hstack.add_child_with_layout(
        Box::new(SizedView {
            min_w: 2,
            desired_w: Some(10),
        }),
        LayoutParams {
            width: Size::Content,
            ..LayoutParams::default()
        },
    );

    // Width equals sum(mins)+spacing => 2 + 1 + 2 = 5.
    draw_view(&mut hstack, Rect::new(0, 0, 5, 3));
    let children = hstack.children();
    assert_eq!(children.len(), 2);

    assert_eq!(children[0].bounds().x, 0);
    assert_eq!(children[0].bounds().width, 2);
    assert_eq!(children[1].bounds().x, 3);
    assert_eq!(children[1].bounds().width, 2);
}

#[test]
fn grid_layout_at_min_height_keeps_all_rows_visible() {
    #[derive(Default)]
    struct SizedView {
        min_h: u16,
        desired_h: Option<u16>,
    }

    impl Component for SizedView {
        fn draw(
            &mut self,
            _frame: &mut ratatui::Frame<'_>,
            _area: Rect,
            _ctx: ComponentContext<'_>,
        ) {
        }
    }

    impl Layout for SizedView {
        fn min_height(&self) -> u16 {
            self.min_h
        }

        fn desired_height(&self) -> Option<u16> {
            self.desired_h
        }
    }

    crate::impl_component_default_traits!(SizedView => Scrollable, FocusNav, DynamicTree, EventHandling);

    let mut grid = Grid::new().with_columns(1usize).with_row_gap(1u16);
    grid.add_child_with_layout(
        Box::new(SizedView {
            min_h: 2,
            desired_h: Some(10),
        }),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );
    grid.add_child_with_layout(
        Box::new(SizedView {
            min_h: 2,
            desired_h: Some(10),
        }),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );

    // Height equals sum(row mins)+gap => 2 + 1 + 2 = 5.
    draw_view(&mut grid, Rect::new(0, 0, 20, 5));
    let children = grid.children();
    assert_eq!(children.len(), 2);

    assert_eq!(children[0].bounds().y, 0);
    assert_eq!(children[0].bounds().height, 2);
    assert_eq!(children[1].bounds().y, 3);
    assert_eq!(children[1].bounds().height, 2);
}

#[test]
fn grid_mouse_hit_routes_to_child_and_wheel_scrolls() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut grid = Grid::new().with_columns(1usize).with_scrollable(true);
    for _ in 0..6 {
        grid.add_child_with_layout(
            Box::new(RecordingView::new(Arc::clone(&events)).with_outcome(EventOutcome::Ignored)),
            LayoutParams {
                height: Size::Fixed(1),
                ..LayoutParams::default()
            },
        );
    }

    draw_view(&mut grid, Rect::new(0, 0, 20, 3));
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 1,
        row: 1,
        modifiers: KeyModifiers::NONE,
    });
    let result = grid.handle_event(&click, test_context());
    assert!(result.is_consumed());
    assert_eq!(
        events.lock().expect("events lock").as_slice(),
        &[RecordedEvent::Mouse { column: 1, row: 0 }]
    );

    let wheel = Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 1,
        row: 1,
        modifiers: KeyModifiers::NONE,
    });
    let result = grid.handle_event(&wheel, test_context());
    assert!(result.is_consumed());
    assert!(grid.scroll_offset().1 > 0);
}

#[test]
fn hstack_desired_height_is_max_child_height_plus_padding() {
    #[derive(Default)]
    struct MinHeightView {
        min_h: u16,
        desired_h: Option<u16>,
    }

    impl Component for MinHeightView {
        fn draw(
            &mut self,
            _frame: &mut ratatui::Frame<'_>,
            _area: Rect,
            _ctx: ComponentContext<'_>,
        ) {
        }
    }

    impl Layout for MinHeightView {
        fn min_height(&self) -> u16 {
            self.min_h
        }

        fn desired_height(&self) -> Option<u16> {
            self.desired_h
        }
    }

    crate::impl_component_default_traits!(MinHeightView => Scrollable, FocusNav, DynamicTree, EventHandling);

    let mut hstack = HStack::new().with_padding(EdgeInsets {
        top: 1,
        right: 0,
        bottom: 1,
        left: 0,
    });

    hstack.add_child_with_layout(
        Box::new(MinHeightView {
            min_h: 3,
            desired_h: Some(2),
        }),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );

    hstack.add_child_with_layout(
        Box::new(MinHeightView {
            min_h: 1,
            desired_h: Some(1),
        }),
        LayoutParams {
            height: Size::Fixed(5),
            margin: EdgeInsets {
                top: 1,
                ..EdgeInsets::ZERO
            },
            ..LayoutParams::default()
        },
    );

    // child1: max(desired=2, min=3) = 3
    // child2: fixed 5 + margin top 1 = 6
    // max = 6
    // padding = 2
    assert_eq!(hstack.desired_height(), Some(8));
}

#[test]
fn scrollbar_position_left_places_vertical_scrollbar_on_left_edge() {
    #[derive(Default)]
    struct BlankLineView;

    impl Component for BlankLineView {
        fn draw(
            &mut self,
            _frame: &mut ratatui::Frame<'_>,
            _area: Rect,
            _ctx: ComponentContext<'_>,
        ) {
        }
    }

    impl Layout for BlankLineView {
        fn desired_width(&self) -> Option<u16> {
            Some(1)
        }

        fn desired_height(&self) -> Option<u16> {
            Some(1)
        }
    }

    crate::impl_component_default_traits!(BlankLineView => Scrollable, FocusNav, DynamicTree, EventHandling);

    let mut vstack = VStack::new().with_scrollable(true).with_scroll_config(
        ScrollConfig::default()
            .vertical_scrollbar(ScrollbarVisibility::Always)
            .horizontal_scrollbar(ScrollbarVisibility::Never),
    );

    for _ in 0..20 {
        vstack.add_child_with_layout(
            Box::new(BlankLineView),
            LayoutParams {
                height: Size::Content,
                width: Size::Content,
                ..LayoutParams::default()
            },
        );
    }

    let theme = Theme::dark();
    let ctx = ComponentContext {
        theme: &theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };

    let backend = TestBackend::new(10, 5);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|f| vstack.draw(f, Rect::new(0, 0, 10, 5), ctx))
        .expect("draw");

    let buf = terminal.backend().buffer();

    let right = buf.cell((9, 0)).expect("cell").symbol();
    assert!(
        matches!(right, "░" | "█" | "▲" | "▼"),
        "expected vertical scrollbar on the right edge; got {right:?} at (9,0)"
    );

    let viewport_col0 = buf.cell((0, 0)).expect("cell").symbol();
    assert!(
        !matches!(viewport_col0, "░" | "█"),
        "expected viewport to start at x=0; got {viewport_col0:?} at (0,0)"
    );
}

#[test]
fn scrollbar_position_top_places_horizontal_scrollbar_on_top_edge() {
    #[derive(Default)]
    struct BlankCellView;

    impl Component for BlankCellView {
        fn draw(
            &mut self,
            _frame: &mut ratatui::Frame<'_>,
            _area: Rect,
            _ctx: ComponentContext<'_>,
        ) {
        }
    }

    impl Layout for BlankCellView {
        fn desired_width(&self) -> Option<u16> {
            Some(2)
        }

        fn desired_height(&self) -> Option<u16> {
            Some(1)
        }
    }

    crate::impl_component_default_traits!(BlankCellView => Scrollable, FocusNav, DynamicTree, EventHandling);

    let mut hstack = HStack::new().with_scrollable(true).with_scroll_config(
        ScrollConfig::default()
            .vertical_scrollbar(ScrollbarVisibility::Never)
            .horizontal_scrollbar(ScrollbarVisibility::Always),
    );

    for _ in 0..40 {
        hstack.add_child_with_layout(
            Box::new(BlankCellView),
            LayoutParams {
                height: Size::Content,
                width: Size::Content,
                ..LayoutParams::default()
            },
        );
    }

    let theme = Theme::dark();
    let ctx = ComponentContext {
        theme: &theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::Component,
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag: None,
    };

    let backend = TestBackend::new(10, 4);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|f| hstack.draw(f, Rect::new(0, 0, 10, 4), ctx))
        .expect("draw");

    let buf = terminal.backend().buffer();

    let bottom = buf.cell((0, 3)).expect("cell").symbol();
    assert!(
        matches!(bottom, "░" | "█" | "◄" | "►"),
        "expected horizontal scrollbar on the bottom edge; got {bottom:?} at (0,3)"
    );

    let viewport_row0 = buf.cell((0, 0)).expect("cell").symbol();
    assert!(
        !matches!(viewport_row0, "░" | "█"),
        "expected viewport to start at y=0; got {viewport_row0:?} at (0,0)"
    );
}
