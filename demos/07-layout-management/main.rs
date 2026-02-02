use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;

use chatty::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, run_crossterm_desktop,
};
use chatty::declarative::{
    Align, Anchor, AnchorPlacement, DeclarativeView, Divider, EdgeInsets, Grid, HStack,
    LayoutParams, Size, Spacer, Text, VStack,
};
use chatty::reactive::Property;
use chatty::theme::Theme;
use chatty::view::{EventOutcome, View};
use chatty::widgets::{Button, Checkbox, Label};
use chatty::wm::{Window, WindowKind};

fn content_height() -> LayoutParams {
    LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    }
}

/// 构建 VStack 演示窗口
fn build_vstack_demo_view() -> Box<dyn View> {
    let items = VStack::new()
        .spacing(1)
        .padding(1)
        .child_with_layout(Text::new("Item 1 - Fixed height"), content_height())
        .child_with_layout(Text::new("Item 2 - Fixed height"), content_height())
        .child_with_layout(Text::new("Item 3 - Fixed height"), content_height());

    VStack::new()
        .spacing(1)
        .padding(1)
        .child_with_layout(
            Text::new("VStack Demo - Vertical Stack Layout"),
            content_height(),
        )
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(
            Text::new("VStack arranges children vertically with configurable spacing"),
            content_height(),
        )
        .child_with_layout(items, content_height())
        .child(Spacer::new())
        .child_with_layout(
            Text::new("Press 'h' for HStack, 'g' for Grid, 'a' for Alignment demo"),
            content_height(),
        )
        .build_view()
}

/// 构建 HStack 演示窗口
fn build_hstack_demo_view() -> Box<dyn View> {
    let row1 = HStack::new()
        .spacing(1)
        .child(Text::new("Left"))
        .child(Spacer::new())
        .child(Text::new("Center"))
        .child(Spacer::new())
        .child(Text::new("Right"));

    let row2 = HStack::new()
        .spacing(2)
        .child(Text::new("[A]"))
        .child(Text::new("[B]"))
        .child(Text::new("[C]"))
        .child(Text::new("[D]"));

    let row3 = HStack::new()
        .child_with_layout(
            Label::new("Fixed 10"),
            LayoutParams {
                width: Size::Fixed(10),
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            Label::new("Weight 1"),
            LayoutParams {
                width: Size::Weight(1),
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            Label::new("Weight 2"),
            LayoutParams {
                width: Size::Weight(2),
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .spacing(1);

    VStack::new()
        .padding_insets(EdgeInsets::all(1))
        .spacing(1)
        .child_with_layout(
            Text::new("HStack Demo - Horizontal Stack Layout"),
            content_height(),
        )
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(Text::new("Spacer distribution:"), content_height())
        .child_with_layout(
            row1,
            LayoutParams {
                height: Size::Fixed(1),
                ..LayoutParams::default()
            },
        )
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(Text::new("Equal spacing:"), content_height())
        .child_with_layout(
            row2,
            LayoutParams {
                height: Size::Fixed(1),
                ..LayoutParams::default()
            },
        )
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(Text::new("Weight-based distribution:"), content_height())
        .child_with_layout(
            row3,
            LayoutParams {
                height: Size::Fixed(3),
                ..LayoutParams::default()
            },
        )
        .build_view()
}

/// 构建 Grid 演示窗口
fn build_grid_demo_view() -> Box<dyn View> {
    let grid = Grid::new()
        .columns(3)
        .row_gap(1)
        .column_gap(2)
        .child(Label::new("Row 1, Col 1"))
        .child(Label::new("Row 1, Col 2"))
        .child(Label::new("Row 1, Col 3"))
        .child(Label::new("Row 2, Col 1"))
        .child(Label::new("Row 2, Col 2"))
        .child(Label::new("Row 2, Col 3"))
        .child_with_layout(
            Button::new("Tall Button"),
            LayoutParams {
                height: Size::Fixed(5),
                align_y: Align::Center,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            Checkbox::new("Centered", Property::new(false).binding()),
            LayoutParams {
                align_y: Align::Center,
                ..LayoutParams::default()
            },
        )
        .child(Label::new("Row 3, Col 3"));

    VStack::new()
        .padding_insets(EdgeInsets::all(1))
        .spacing(1)
        .child_with_layout(Text::new("Grid Demo - Grid Layout"), content_height())
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(
            Text::new("3-column grid with row/column gaps:"),
            content_height(),
        )
        .child_with_layout(
            grid,
            LayoutParams {
                height: Size::Fixed(15),
                ..LayoutParams::default()
            },
        )
        .build_view()
}

/// 构建对齐和锚点演示窗口
fn build_alignment_demo_view() -> Box<dyn View> {
    // 锚点演示
    let anchor_demo = VStack::new()
        .child_with_layout(
            Label::new("[TL]"),
            LayoutParams {
                width: Size::Content,
                height: Size::Content,
                anchor: Some(AnchorPlacement {
                    anchor: Anchor::TopLeft,
                    offset_x: 0,
                    offset_y: 0,
                }),
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            Label::new("[TR]"),
            LayoutParams {
                width: Size::Content,
                height: Size::Content,
                anchor: Some(AnchorPlacement {
                    anchor: Anchor::TopRight,
                    offset_x: 0,
                    offset_y: 0,
                }),
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            Label::new("[CENTER]"),
            LayoutParams {
                width: Size::Content,
                height: Size::Content,
                anchor: Some(AnchorPlacement {
                    anchor: Anchor::Center,
                    offset_x: 0,
                    offset_y: 0,
                }),
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            Label::new("[BL]"),
            LayoutParams {
                width: Size::Content,
                height: Size::Content,
                anchor: Some(AnchorPlacement {
                    anchor: Anchor::BottomLeft,
                    offset_x: 0,
                    offset_y: 0,
                }),
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            Label::new("[BR]"),
            LayoutParams {
                width: Size::Content,
                height: Size::Content,
                anchor: Some(AnchorPlacement {
                    anchor: Anchor::BottomRight,
                    offset_x: 0,
                    offset_y: 0,
                }),
                ..LayoutParams::default()
            },
        );

    // 对齐演示
    let align_demo = VStack::new()
        .spacing(1)
        .child_with_layout(Label::new("Align::Start (default):"), content_height())
        .child_with_layout(
            Label::new("Start aligned"),
            LayoutParams {
                align_x: Align::Start,
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(Label::new("Align::Center:"), content_height())
        .child_with_layout(
            Label::new("Center aligned"),
            LayoutParams {
                align_x: Align::Center,
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(Label::new("Align::End:"), content_height())
        .child_with_layout(
            Label::new("End aligned"),
            LayoutParams {
                align_x: Align::End,
                height: Size::Content,
                ..LayoutParams::default()
            },
        );

    VStack::new()
        .padding_insets(EdgeInsets::all(1))
        .spacing(1)
        .child_with_layout(Text::new("Alignment & Anchor Demo"), content_height())
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(
            Text::new("Anchors (absolute positioning):"),
            content_height(),
        )
        .child_with_layout(
            anchor_demo,
            LayoutParams {
                height: Size::Fixed(10),
                ..LayoutParams::default()
            },
        )
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(
            align_demo,
            LayoutParams {
                height: Size::Fill,
                ..LayoutParams::default()
            },
        )
        .build_view()
}

/// 构建尺寸约束演示窗口
fn build_size_demo_view() -> Box<dyn View> {
    let size_demo = VStack::new()
        .spacing(1)
        .child_with_layout(Text::new("Size::Content (auto size):"), content_height())
        .child_with_layout(
            Label::new("Short"),
            LayoutParams {
                width: Size::Content,
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            Label::new("Much longer content here"),
            LayoutParams {
                width: Size::Content,
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(Text::new("Size::Fixed (fixed width):"), content_height())
        .child_with_layout(
            Label::new("Fixed 20"),
            LayoutParams {
                width: Size::Fixed(20),
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(Text::new("Size::Weight (proportional):"), content_height())
        .child_with_layout(
            HStack::new()
                .spacing(1)
                .child_with_layout(
                    Button::new("W=1"),
                    LayoutParams {
                        width: Size::Weight(1),
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    Button::new("W=2"),
                    LayoutParams {
                        width: Size::Weight(2),
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    Button::new("W=3"),
                    LayoutParams {
                        width: Size::Weight(3),
                        ..LayoutParams::default()
                    },
                ),
            LayoutParams {
                height: Size::Fixed(3),
                ..LayoutParams::default()
            },
        )
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(Text::new("Size::Fill (expand to fill):"), content_height())
        .child_with_layout(
            Label::new("This fills available space"),
            LayoutParams {
                width: Size::Fill,
                height: Size::Content,
                ..LayoutParams::default()
            },
        );

    VStack::new()
        .padding_insets(EdgeInsets::all(1))
        .spacing(1)
        .child_with_layout(Text::new("Size Constraints Demo"), content_height())
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(size_demo, content_height())
        .build_view()
}

/// 构建 Padding/Margin 演示窗口
fn build_spacing_demo_view() -> Box<dyn View> {
    let padding_demo = VStack::new()
        .padding_insets(EdgeInsets {
            top: 2,
            right: 3,
            bottom: 2,
            left: 3,
        })
        .child_with_layout(
            Text::new("Content with padding: top=2, right=3, bottom=2, left=3"),
            content_height(),
        );

    let margin_demo = VStack::new()
        .spacing(1)
        .child_with_layout(
            Label::new("No margin"),
            LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            Label::new("Margin left=5"),
            LayoutParams {
                height: Size::Content,
                margin: EdgeInsets {
                    left: 5,
                    ..EdgeInsets::ZERO
                },
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            Label::new("Margin left=10"),
            LayoutParams {
                height: Size::Content,
                margin: EdgeInsets {
                    left: 10,
                    ..EdgeInsets::ZERO
                },
                ..LayoutParams::default()
            },
        );

    VStack::new()
        .padding_insets(EdgeInsets::all(1))
        .spacing(1)
        .child_with_layout(Text::new("Padding & Margin Demo"), content_height())
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(Text::new("Padding (inside container):"), content_height())
        .child_with_layout(
            padding_demo,
            LayoutParams {
                height: Size::Fixed(5),
                ..LayoutParams::default()
            },
        )
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(Text::new("Margin (outside element):"), content_height())
        .child_with_layout(margin_demo, content_height())
        .build_view()
}

fn main() -> Result<()> {
    let config = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(true)
        .cursor(CursorMode::Hide);

    run_crossterm_desktop(
        config,
        |screen| {
            let theme = Theme::dark();
            let menu = MenuBar::new(vec![]);
            let mut desktop = Desktop::new(theme, menu);

            let work = Desktop::layout(screen).work_area;

            let vstack_window = Window::new(
                WindowKind::Normal,
                "VStack Demo",
                Rect {
                    x: work.x + 2,
                    y: work.y + 1,
                    width: 55,
                    height: 18,
                },
                build_vstack_demo_view(),
            );
            desktop.add_window(vstack_window, screen);

            let hstack_window = Window::new(
                WindowKind::Normal,
                "HStack Demo",
                Rect {
                    x: work.x + 59,
                    y: work.y + 1,
                    width: 55,
                    height: 18,
                },
                build_hstack_demo_view(),
            );
            desktop.add_window(hstack_window, screen);

            let grid_window = Window::new(
                WindowKind::Normal,
                "Grid Demo",
                Rect {
                    x: work.x + 2,
                    y: work.y + 20,
                    width: 55,
                    height: 20,
                },
                build_grid_demo_view(),
            );
            desktop.add_window(grid_window, screen);

            let alignment_window = Window::new(
                WindowKind::Normal,
                "Alignment & Anchor",
                Rect {
                    x: work.x + 59,
                    y: work.y + 20,
                    width: 55,
                    height: 20,
                },
                build_alignment_demo_view(),
            );
            desktop.add_window(alignment_window, screen);

            Ok(desktop)
        },
        |_desktop, _screen| Ok(AppControl::Continue),
        |desktop, ev, screen, result| {
            if result.outcome != EventOutcome::Ignored {
                return Ok(AppControl::Continue);
            }

            let Event::Key(KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                ..
            }) = ev
            else {
                return Ok(AppControl::Continue);
            };

            let work = Desktop::layout(screen).work_area;
            match *code {
                KeyCode::Char('v') => {
                    let window = Window::new(
                        WindowKind::Normal,
                        "VStack Demo",
                        Rect {
                            x: work.x + 2,
                            y: work.y + 1,
                            width: 55,
                            height: 18,
                        },
                        build_vstack_demo_view(),
                    );
                    desktop.add_window(window, screen);
                }
                KeyCode::Char('h') => {
                    let window = Window::new(
                        WindowKind::Normal,
                        "HStack Demo",
                        Rect {
                            x: work.x + 59,
                            y: work.y + 1,
                            width: 55,
                            height: 18,
                        },
                        build_hstack_demo_view(),
                    );
                    desktop.add_window(window, screen);
                }
                KeyCode::Char('g') => {
                    let window = Window::new(
                        WindowKind::Normal,
                        "Grid Demo",
                        Rect {
                            x: work.x + 2,
                            y: work.y + 20,
                            width: 55,
                            height: 20,
                        },
                        build_grid_demo_view(),
                    );
                    desktop.add_window(window, screen);
                }
                KeyCode::Char('a') => {
                    let window = Window::new(
                        WindowKind::Normal,
                        "Alignment & Anchor",
                        Rect {
                            x: work.x + 59,
                            y: work.y + 20,
                            width: 55,
                            height: 20,
                        },
                        build_alignment_demo_view(),
                    );
                    desktop.add_window(window, screen);
                }
                KeyCode::Char('s') => {
                    let window = Window::new(
                        WindowKind::Normal,
                        "Size Constraints",
                        Rect {
                            x: work.x + 10,
                            y: work.y + 5,
                            width: 60,
                            height: 25,
                        },
                        build_size_demo_view(),
                    );
                    desktop.add_window(window, screen);
                }
                KeyCode::Char('p') => {
                    let window = Window::new(
                        WindowKind::Normal,
                        "Padding & Margin",
                        Rect {
                            x: work.x + 15,
                            y: work.y + 7,
                            width: 55,
                            height: 20,
                        },
                        build_spacing_demo_view(),
                    );
                    desktop.add_window(window, screen);
                }
                KeyCode::Char('c') => {
                    if let Some(id) = desktop.wm.focused() {
                        desktop.wm.request_close(id);
                    }
                }
                KeyCode::Tab => {
                    desktop.wm.focus_next();
                }
                _ => {}
            }

            Ok(AppControl::Continue)
        },
    )
}
