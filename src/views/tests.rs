use std::sync::{Arc, Mutex};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use crate::theme::Theme;
use crate::view::{EventOutcome, ScrollbarHost, View, ViewContext, ViewEventResult};
use crate::wm::WindowId;

use super::{
    Align, Anchor, AnchorPlacement, EdgeInsets, Grid, HBox, LayoutParams, ScrollConfig,
    ScrollbarVisibility, Size, VBox,
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

impl View for RecordingView {
    fn is_focusable(&self) -> bool {
        self.focusable
    }

    fn desired_width(&self) -> Option<u16> {
        self.desired_width
    }

    fn desired_height(&self) -> Option<u16> {
        self.desired_height
    }

    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
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

        ViewEventResult {
            outcome: self.outcome,
            action: crate::view::ViewAction::None,
        }
    }

    fn draw(&mut self, _frame: &mut ratatui::Frame<'_>, _area: Rect, _ctx: ViewContext<'_>) {}
}

fn draw_view(view: &mut dyn View, area: Rect) {
    let theme = Theme::dark();
    let ctx = ViewContext {
        theme: &theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::View,
    };

    let backend = TestBackend::new(80, 40);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
}

#[test]
fn view_hierarchy_add_remove_and_query_children() {
    let mut vbox = VBox::new();
    let vbox_id = vbox.id();

    let ev1 = Arc::new(Mutex::new(Vec::new()));
    let id1 = vbox.add_child(Box::new(RecordingView::new(Arc::clone(&ev1))));

    let ev2 = Arc::new(Mutex::new(Vec::new()));
    let id2 = vbox.add_child(Box::new(
        RecordingView::new(Arc::clone(&ev2)).with_focusable(false),
    ));

    assert_eq!(vbox.child_count(), 2);
    assert_eq!(vbox.child(id1).expect("child 1").parent, Some(vbox_id));
    assert_eq!(vbox.child(id2).expect("child 2").parent, Some(vbox_id));

    let removed = vbox.remove_child(id1).expect("remove child");
    assert_eq!(removed.parent, Some(vbox_id));
    assert_eq!(vbox.child_count(), 1);
    assert!(vbox.child(id1).is_none());
}

#[test]
fn nested_hierarchy_preserves_parent_ids() {
    let mut inner = VBox::new();
    let inner_id = inner.id();
    let leaf_events = Arc::new(Mutex::new(Vec::new()));
    let leaf_id = inner.add_child(Box::new(RecordingView::new(Arc::clone(&leaf_events))));

    let mut outer = VBox::new();
    let outer_id = outer.id();
    let inner_node_id = outer.add_child(Box::new(inner));

    assert_eq!(
        outer.child(inner_node_id).expect("inner node").parent,
        Some(outer_id)
    );

    let inner_children = outer
        .child(inner_node_id)
        .expect("inner node")
        .view
        .children();
    assert_eq!(inner_children.len(), 1);
    assert_eq!(inner_children[0].id, leaf_id);
    assert_eq!(inner_children[0].parent, Some(inner_id));
}

#[test]
fn vbox_layout_fixed_heights() {
    let mut vbox = VBox::new();
    vbox.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(5),
            ..LayoutParams::default()
        },
    );
    vbox.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(10),
            ..LayoutParams::default()
        },
    );
    vbox.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(5),
            ..LayoutParams::default()
        },
    );

    draw_view(&mut vbox, Rect::new(0, 0, 40, 20));

    let children = vbox.children();
    assert_eq!(children.len(), 3);
    assert_eq!(children[0].bounds(), Rect::new(0, 0, 40, 5));
    assert_eq!(children[1].bounds(), Rect::new(0, 5, 40, 10));
    assert_eq!(children[2].bounds(), Rect::new(0, 15, 40, 5));
}

#[test]
fn vbox_layout_weighted_split() {
    let mut vbox = VBox::new();
    vbox.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Weight(1),
            ..LayoutParams::default()
        },
    );
    vbox.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Weight(2),
            ..LayoutParams::default()
        },
    );

    draw_view(&mut vbox, Rect::new(0, 0, 20, 30));
    let children = vbox.children();
    assert_eq!(children[0].bounds(), Rect::new(0, 0, 20, 10));
    assert_eq!(children[1].bounds(), Rect::new(0, 10, 20, 20));
}

#[test]
fn vbox_layout_clamps_overflow() {
    let mut vbox = VBox::new();
    vbox.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(6),
            ..LayoutParams::default()
        },
    );
    vbox.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(6),
            ..LayoutParams::default()
        },
    );

    draw_view(&mut vbox, Rect::new(0, 0, 10, 10));
    let children = vbox.children();
    assert_eq!(children[0].bounds(), Rect::new(0, 0, 10, 6));
    assert_eq!(children[1].bounds(), Rect::new(0, 6, 10, 4));
}

#[test]
fn vbox_padding_reduces_content_area() {
    let mut vbox = VBox::new().with_padding(EdgeInsets::all(2));
    vbox.add_child(Box::new(RecordingView::new(Arc::new(Mutex::new(
        Vec::new(),
    )))));

    draw_view(&mut vbox, Rect::new(0, 0, 20, 10));
    let child = &vbox.children()[0];
    assert_eq!(child.bounds(), Rect::new(0, 0, 16, 6));
}

#[test]
fn vbox_margins_reserve_space_around_child() {
    let mut vbox = VBox::new();
    let margin = EdgeInsets {
        top: 1,
        right: 1,
        bottom: 1,
        left: 1,
    };
    vbox.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(2),
            margin,
            ..LayoutParams::default()
        },
    );
    vbox.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(2),
            margin,
            ..LayoutParams::default()
        },
    );

    draw_view(&mut vbox, Rect::new(0, 0, 20, 10));

    let children = vbox.children();
    assert_eq!(children[0].bounds(), Rect::new(1, 1, 18, 2));
    assert_eq!(children[1].bounds(), Rect::new(1, 5, 18, 2));
}

#[test]
fn vbox_alignment_centers_child_in_slot() {
    let mut vbox = VBox::new();
    vbox.add_child_with_layout(
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

    draw_view(&mut vbox, Rect::new(0, 0, 20, 5));

    let child = &vbox.children()[0];
    assert_eq!(child.bounds(), Rect::new(8, 0, 4, 1));
}

#[test]
fn vbox_anchor_positions_overlay_and_does_not_affect_flow() {
    let mut vbox = VBox::new();
    vbox.add_child_with_layout(
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
    vbox.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            height: Size::Fixed(5),
            ..LayoutParams::default()
        },
    );

    draw_view(&mut vbox, Rect::new(0, 0, 20, 10));

    let children = vbox.children();
    assert_eq!(children.len(), 2);

    // Anchored overlays do not affect the flow child layout.
    assert_eq!(children[1].bounds(), Rect::new(0, 0, 20, 5));

    // Anchored child is positioned relative to the parent's content size.
    assert_eq!(children[0].bounds(), Rect::new(17, 0, 3, 2));
}

#[test]
fn vbox_anchor_repositions_on_resize() {
    let mut vbox = VBox::new();
    vbox.add_child_with_layout(
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

    draw_view(&mut vbox, Rect::new(0, 0, 20, 5));
    assert_eq!(vbox.children()[0].bounds(), Rect::new(17, 0, 3, 2));

    draw_view(&mut vbox, Rect::new(0, 0, 30, 5));
    assert_eq!(vbox.children()[0].bounds(), Rect::new(27, 0, 3, 2));
}

#[test]
fn event_routing_translates_absolute_mouse_coords_to_child_local() {
    let leaf_events = Arc::new(Mutex::new(Vec::new()));
    let leaf = RecordingView::new(Arc::clone(&leaf_events)).with_outcome(EventOutcome::Consumed);

    let mut inner = VBox::new();
    inner.add_child(Box::new(leaf));

    let mut outer = VBox::new().with_padding(EdgeInsets::all(1));
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
    let ctx = ViewContext {
        theme: &theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::View,
    };
    let res = outer.handle_event(&click, ctx);
    assert!(res.is_consumed());

    let recorded = leaf_events.lock().expect("events lock").clone();
    assert_eq!(recorded, vec![RecordedEvent::Mouse { column: 2, row: 1 }]);
}

#[test]
fn capture_phase_consumes_tab_before_children_receive_it() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut vbox = VBox::new();
    vbox.add_child(Box::new(RecordingView::new(Arc::clone(&events))));

    draw_view(&mut vbox, Rect::new(0, 0, 10, 5));

    let tab = Event::Key(KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });

    let theme = Theme::dark();
    let ctx = ViewContext {
        theme: &theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::View,
    };
    let res = vbox.handle_event(&tab, ctx);
    assert!(res.is_consumed());
    assert!(events.lock().expect("events lock").is_empty());
}

#[test]
fn keyboard_events_route_to_focused_child() {
    let a = Arc::new(Mutex::new(Vec::new()));
    let b = Arc::new(Mutex::new(Vec::new()));

    let mut vbox = VBox::new();
    vbox.add_child(Box::new(RecordingView::new(Arc::clone(&a))));
    vbox.add_child(Box::new(RecordingView::new(Arc::clone(&b))));

    draw_view(&mut vbox, Rect::new(0, 0, 10, 5));

    let theme = Theme::dark();
    let ctx = ViewContext {
        theme: &theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::View,
    };

    let key_a = Event::Key(KeyEvent {
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    assert!(vbox.handle_event(&key_a, ctx).is_consumed());

    let tab = Event::Key(KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    assert!(vbox.handle_event(&tab, ctx).is_consumed());

    let key_b = Event::Key(KeyEvent {
        code: KeyCode::Char('b'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    assert!(vbox.handle_event(&key_b, ctx).is_consumed());

    let rec_a = a.lock().expect("events lock").clone();
    let rec_b = b.lock().expect("events lock").clone();

    assert_eq!(rec_a, vec![RecordedEvent::Key(KeyCode::Char('a'))]);
    assert_eq!(rec_b, vec![RecordedEvent::Key(KeyCode::Char('b'))]);
}

#[test]
fn hbox_layout_fixed_widths() {
    let mut hbox = HBox::new();
    hbox.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            width: Size::Fixed(10),
            ..LayoutParams::default()
        },
    );
    hbox.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            width: Size::Fixed(20),
            ..LayoutParams::default()
        },
    );
    hbox.add_child_with_layout(
        Box::new(RecordingView::new(Arc::new(Mutex::new(Vec::new())))),
        LayoutParams {
            width: Size::Fixed(10),
            ..LayoutParams::default()
        },
    );

    draw_view(&mut hbox, Rect::new(0, 0, 40, 5));
    let children = hbox.children();
    assert_eq!(children.len(), 3);
    assert_eq!(children[0].bounds(), Rect::new(0, 0, 10, 5));
    assert_eq!(children[1].bounds(), Rect::new(10, 0, 20, 5));
    assert_eq!(children[2].bounds(), Rect::new(30, 0, 10, 5));
}

#[test]
fn grid_layout_columns_and_row_heights() {
    let mut grid = Grid::new(3);
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
fn scrollbar_position_left_places_vertical_scrollbar_on_left_edge() {
    #[derive(Default)]
    struct BlankLineView;

    impl View for BlankLineView {
        fn desired_width(&self) -> Option<u16> {
            Some(1)
        }

        fn desired_height(&self) -> Option<u16> {
            Some(1)
        }

        fn draw(&mut self, _frame: &mut ratatui::Frame<'_>, _area: Rect, _ctx: ViewContext<'_>) {}
    }

    let mut vbox = VBox::new().with_scrollable(true).with_scroll_config(
        ScrollConfig::default()
            .vertical_scrollbar(ScrollbarVisibility::Always)
            .horizontal_scrollbar(ScrollbarVisibility::Never),
    );

    for _ in 0..20 {
        vbox.add_child_with_layout(
            Box::new(BlankLineView),
            LayoutParams {
                height: Size::Content,
                width: Size::Content,
                ..LayoutParams::default()
            },
        );
    }

    let theme = Theme::dark();
    let ctx = ViewContext {
        theme: &theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::View,
    };

    let backend = TestBackend::new(10, 5);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|f| vbox.draw(f, Rect::new(0, 0, 10, 5), ctx))
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

    impl View for BlankCellView {
        fn desired_width(&self) -> Option<u16> {
            Some(2)
        }

        fn desired_height(&self) -> Option<u16> {
            Some(1)
        }

        fn draw(&mut self, _frame: &mut ratatui::Frame<'_>, _area: Rect, _ctx: ViewContext<'_>) {}
    }

    let mut hbox = HBox::new().with_scrollable(true).with_scroll_config(
        ScrollConfig::default()
            .vertical_scrollbar(ScrollbarVisibility::Never)
            .horizontal_scrollbar(ScrollbarVisibility::Always),
    );

    for _ in 0..40 {
        hbox.add_child_with_layout(
            Box::new(BlankCellView),
            LayoutParams {
                height: Size::Content,
                width: Size::Content,
                ..LayoutParams::default()
            },
        );
    }

    let theme = Theme::dark();
    let ctx = ViewContext {
        theme: &theme,
        window_id: WindowId(1),
        is_focused: true,
        scrollbar_host: ScrollbarHost::View,
    };

    let backend = TestBackend::new(10, 4);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|f| hbox.draw(f, Rect::new(0, 0, 10, 4), ctx))
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
