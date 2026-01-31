use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{cursor, style};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use chatty::app::{Desktop, DesktopAction, MenuBar, MenuItem, MenuSpec};
use chatty::theme::Theme;
use chatty::view::{EventOutcome, View, ViewContext, ViewEventResult};
use chatty::views::{
    Align, Anchor, AnchorPlacement, ControlView, EdgeInsets, Grid, HBox, LayoutParams, Size, VBox,
};
use chatty::widgets::{
    Button, Checkbox, ControlOutcome, Form, Label, ListBox, RadioGroup, TableView, TextBox,
};
use chatty::wm::{Window, WindowId, WindowKind, WindowState};

#[derive(Default)]
struct TextView {
    lines: Vec<String>,
}

impl TextView {
    fn new(lines: Vec<String>) -> Self {
        Self { lines }
    }
}

impl View for TextView {
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            return ViewEventResult::close_window();
        }
        ViewEventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        let style = if ctx.is_focused {
            ctx.theme.widget.focused
        } else {
            ctx.theme.widget.normal
        };
        let text = self.lines.to_vec().join("\n");
        frame.render_widget(
            Paragraph::new(text).style(style).wrap(Wrap { trim: false }),
            area,
        );
    }
}

struct DialogView {
    form: Form,
}

impl DialogView {
    fn about() -> Self {
        let controls: Vec<Box<dyn chatty::widgets::Control>> = vec![
            Box::new(Label::new("Chatty demo (Turbo Vision-inspired).")),
            Box::new(Label::new("")),
            Box::new(Label::new("Keys:")),
            Box::new(Label::new("  F10 menu   Ctrl+W window mode   F2 theme")),
            Box::new(Label::new("  n new win  a about/modal        t tooltip")),
            Box::new(Label::new("  v layout demo (view hierarchy)")),
            Box::new(Label::new("  s scroll demo (viewport + scrollbars)")),
            Box::new(Label::new("")),
            Box::new(Button::new("Close (Enter)")),
        ];
        Self {
            form: Form::new(controls),
        }
    }
}

impl View for DialogView {
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        let (outcome, action) = self.form.handle_event(event);
        if action == chatty::widgets::FormAction::Submitted {
            return ViewEventResult::close_window();
        }
        match outcome {
            ControlOutcome::Consumed => ViewEventResult::consumed(),
            ControlOutcome::Ignored => ViewEventResult::ignored(),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.form.draw(frame, area, ctx.theme);
    }
}

struct WidgetsView {
    form: Form,
    last_msg: String,
}

impl WidgetsView {
    fn new() -> Self {
        let controls: Vec<Box<dyn chatty::widgets::Control>> = vec![
            Box::new(Label::new("Try mouse drag on title bar; click × to close.")),
            Box::new(Label::new("Try bracketed paste into textbox: 你好👋 / 👩‍💻")),
            Box::new(TextBox::new("Text").with_text("hello")),
            Box::new(Checkbox::new("Enable feature", true)),
            Box::new(RadioGroup::new(
                "Mode",
                vec!["Normal".into(), "Insert".into(), "Visual".into()],
                0,
            )),
            Box::new(
                ListBox::new(
                    "List",
                    vec![
                        "Alpha".into(),
                        "Beta".into(),
                        "Gamma".into(),
                        "Delta".into(),
                    ],
                )
                .with_height(6),
            ),
            Box::new(
                TableView::new(
                    "Table",
                    vec!["Key".into(), "Value".into()],
                    vec![
                        vec!["lang".into(), "Rust".into()],
                        vec!["jp".into(), "こんにちは".into()],
                        vec!["cn".into(), "你好👋".into()],
                    ],
                )
                .with_height(6),
            ),
            Box::new(Button::new("OK")),
        ];
        Self {
            form: Form::new(controls),
            last_msg: String::new(),
        }
    }
}

impl View for WidgetsView {
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        if let Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
            && modifiers.contains(KeyModifiers::CONTROL)
        {
            return ViewEventResult::close_window();
        }

        let (outcome, action) = self.form.handle_event(event);
        if action == chatty::widgets::FormAction::Submitted {
            self.last_msg = "Submitted!".to_string();
        }
        match outcome {
            ControlOutcome::Consumed => ViewEventResult::consumed(),
            ControlOutcome::Ignored => ViewEventResult::ignored(),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.form.draw(frame, area, ctx.theme);
        if !self.last_msg.is_empty() && area.height >= 1 {
            let y = area.y + area.height - 1;
            frame.render_widget(
                Paragraph::new(Line::styled(self.last_msg.clone(), ctx.theme.widget.accent)),
                Rect {
                    x: area.x,
                    y,
                    width: area.width,
                    height: 1,
                },
            );
        }
    }
}

#[derive(Default)]
struct TooltipView {
    text: String,
}

impl TooltipView {
    fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl View for TooltipView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        frame.render_widget(
            Paragraph::new(self.text.clone()).style(ctx.theme.widget.normal),
            area,
        );
    }
}

#[derive(Clone, Debug)]
struct IntrinsicLabelView {
    text: String,
}

impl IntrinsicLabelView {
    fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl View for IntrinsicLabelView {
    fn desired_width(&self) -> Option<u16> {
        Some(self.text.len().min(u16::MAX as usize) as u16)
    }

    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let style = ctx.theme.widget.accent;
        frame.render_widget(Paragraph::new(Line::styled(self.text.clone(), style)), area);
    }
}

fn build_layout_demo_view() -> VBox {
    let mut root = VBox::new().with_padding(EdgeInsets::all(1)).with_spacing(1);

    // Anchor demo: this badge sticks to the top-right of the view content area.
    root.add_child_with_layout(
        Box::new(IntrinsicLabelView::new("[ANCHOR]")),
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
    );

    // Margin demo: reserve some space on the right so the anchored badge doesn't overlap.
    root.add_child_with_layout(
        Box::new(ControlView::new(Box::new(Label::new(
            "M6 layout demo (resize window)",
        )))),
        LayoutParams {
            height: Size::Content,
            margin: EdgeInsets {
                right: 10,
                ..EdgeInsets::ZERO
            },
            ..LayoutParams::default()
        },
    );

    // HBox demo:
    // - left: content-based sizing (intrinsic width)
    // - middle/right: weighted sizing (1:2 split)
    // - vertical alignment: the badge is centered within the 3-row toolbar
    let mut toolbar = HBox::new().with_spacing(1);
    toolbar.add_child_with_layout(
        Box::new(IntrinsicLabelView::new("Content")),
        LayoutParams {
            width: Size::Content,
            height: Size::Content,
            align_y: Align::Center,
            margin: EdgeInsets {
                left: 1,
                right: 1,
                ..EdgeInsets::ZERO
            },
            ..LayoutParams::default()
        },
    );
    toolbar.add_child_with_layout(
        Box::new(ControlView::new(Box::new(Button::new("W1")))),
        LayoutParams {
            width: Size::Weight(1),
            ..LayoutParams::default()
        },
    );
    toolbar.add_child_with_layout(
        Box::new(ControlView::new(Box::new(Button::new("W2")))),
        LayoutParams {
            width: Size::Weight(2),
            ..LayoutParams::default()
        },
    );

    root.add_child_with_layout(
        Box::new(toolbar),
        LayoutParams {
            height: Size::Fixed(3),
            ..LayoutParams::default()
        },
    );

    // Grid demo:
    // - 2 columns with equal widths
    // - row height is tallest child in the row (Button is 3 rows tall)
    // - checkbox is vertically centered in that tall row
    let mut grid = Grid::new(2).with_row_gap(1).with_column_gap(2);
    grid.add_child_with_layout(
        Box::new(ControlView::new(Box::new(Button::new("Tall")))),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );
    grid.add_child_with_layout(
        Box::new(ControlView::new(Box::new(Checkbox::new("Centered", false)))),
        LayoutParams {
            align_y: Align::Center,
            ..LayoutParams::default()
        },
    );
    grid.add_child(Box::new(ControlView::new(Box::new(Checkbox::new(
        "Row 2A", false,
    )))));
    grid.add_child(Box::new(ControlView::new(Box::new(Checkbox::new(
        "Row 2B", false,
    )))));

    root.add_child_with_layout(
        Box::new(grid),
        LayoutParams {
            height: Size::Fixed(5),
            margin: EdgeInsets {
                left: 2,
                ..EdgeInsets::ZERO
            },
            ..LayoutParams::default()
        },
    );

    root
}

fn build_scroll_demo_view() -> VBox {
    let mut root = VBox::new()
        .with_padding(EdgeInsets::all(1))
        .with_spacing(1)
        .with_scrollable(true);

    root.add_child_with_layout(
        Box::new(IntrinsicLabelView::new(
            "M7/M8 scrolling demo: ↑↓ PgUp/PgDn Home/End, wheel, drag scrollbar thumb",
        )),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );

    // Horizontal scrolling demo (inside its own scrollable HBox).
    let mut wide_row = HBox::new().with_spacing(1).with_scrollable(true);
    for i in 0..24 {
        wide_row.add_child_with_layout(
            Box::new(IntrinsicLabelView::new(format!("[col-{i:02}]"))),
            LayoutParams {
                width: Size::Content,
                height: Size::Content,
                ..LayoutParams::default()
            },
        );
    }
    root.add_child_with_layout(
        Box::new(wide_row),
        LayoutParams {
            height: Size::Fixed(3),
            ..LayoutParams::default()
        },
    );

    for i in 0..120u16 {
        root.add_child_with_layout(
            Box::new(IntrinsicLabelView::new(format!(
                "{i:03}: The quick brown fox jumps over the lazy dog."
            ))),
            LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            },
        );
    }

    root
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        event::EnableMouseCapture,
        event::EnableBracketedPaste,
        cursor::Show,
        style::Print("")
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut is_dark = true;
    let menu = build_menu();
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;

    let widgets_id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Widgets",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(1),
                width: 40,
                height: work.height.saturating_sub(1),
            },
            Box::new(WidgetsView::new()),
        ),
        screen,
    );
    desktop.wm.focus(widgets_id);

    let mut layout_demo_window_id = Some(desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Layout",
            Rect {
                x: work.x.saturating_add(43),
                y: work.y.saturating_add(1),
                width: 36,
                height: 15,
            },
            Box::new(build_layout_demo_view()),
        ),
        screen,
    ));
    let mut scroll_demo_window_id: Option<WindowId> = None;

    let log_id = desktop.add_window(
        Window::new(
            WindowKind::Floating,
            "Notes",
            Rect {
                x: work.x.saturating_add(43),
                y: work.y.saturating_add(16),
                width: 36,
                height: 6,
            },
            Box::new(TextView::new(vec![
                "Mouse: click to focus; drag title bar; drag corners to resize".into(),
                "Ctrl+W: window mode (move/resize/min/max/close)".into(),
                "F2: toggle theme".into(),
                "Paste: bracketed paste into textbox".into(),
                "V: focus/open layout demo".into(),
                "S: focus/open scroll demo".into(),
            ])),
        ),
        screen,
    );

    let mut tooltip: Option<(WindowId, Instant)> = None;
    let res = run(
        &mut terminal,
        &mut desktop,
        &mut is_dark,
        log_id,
        &mut layout_demo_window_id,
        &mut scroll_demo_window_id,
        &mut tooltip,
    );

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        event::DisableMouseCapture,
        event::DisableBracketedPaste
    )?;
    terminal.show_cursor()?;

    res
}

fn build_menu() -> MenuBar {
    MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![
                MenuItem::command("New window", "window.new").shortcut("n"),
                MenuItem::submenu(
                    "Theme",
                    vec![
                        MenuItem::command("Dark", "theme.dark"),
                        MenuItem::command("Light", "theme.light"),
                    ],
                ),
                MenuItem::command("Quit", "app.quit").shortcut("q"),
            ],
        ),
        MenuSpec::new(
            "Window",
            vec![
                MenuItem::command("Next", "window.next").shortcut("F6"),
                MenuItem::command("Layout demo", "window.layout_demo").shortcut("v"),
                MenuItem::command("Scroll demo", "window.scroll_demo").shortcut("s"),
                MenuItem::command("Minimize", "window.min").shortcut("m"),
                MenuItem::command("Maximize", "window.max").shortcut("x"),
                MenuItem::command("Close", "window.close").shortcut("c"),
            ],
        ),
        MenuSpec::new(
            "Help",
            vec![MenuItem::command("About", "help.about").shortcut("a")],
        ),
    ])
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    desktop: &mut Desktop,
    is_dark: &mut bool,
    notes_window_id: WindowId,
    layout_demo_window_id: &mut Option<WindowId>,
    scroll_demo_window_id: &mut Option<WindowId>,
    tooltip: &mut Option<(WindowId, Instant)>,
) -> Result<()> {
    let mut next_float = 0u32;

    loop {
        // Auto-close tooltip after a short time.
        if let Some((id, until)) = tooltip.take() {
            if Instant::now() < until {
                *tooltip = Some((id, until));
            } else {
                desktop.wm.close(id);
            }
        }

        terminal.draw(|f| desktop.draw(f))?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        let screen: Rect = terminal.size()?.into();
        let result = desktop.handle_event(&ev, screen);

        if let DesktopAction::MenuCommand(cmd) = result.action {
            match cmd.as_str() {
                "app.quit" => break,
                "window.new" => {
                    next_float += 1;
                    let screen: Rect = terminal.size()?.into();
                    let title = format!("Floating {next_float}");
                    desktop.add_window(
                        Window::new(
                            WindowKind::Floating,
                            title,
                            Rect {
                                x: 10 + (next_float as u16 % 15),
                                y: 5 + (next_float as u16 % 6),
                                width: 30,
                                height: 8,
                            },
                            Box::new(TextView::new(vec!["Hello from a new window.".into()])),
                        ),
                        screen,
                    );
                }
                "window.next" => desktop.wm.focus_next(),
                "window.layout_demo" => {
                    open_layout_demo(desktop, screen, layout_demo_window_id)?;
                }
                "window.scroll_demo" => {
                    open_scroll_demo(desktop, screen, scroll_demo_window_id)?;
                }
                "window.min" => desktop.wm.minimize_focused(),
                "window.max" => {
                    let screen: Rect = terminal.size()?.into();
                    let work = Desktop::layout(screen).work_area;
                    desktop.wm.toggle_maximize_focused(work);
                }
                "window.close" => {
                    if let Some(id) = desktop.wm.focused() {
                        desktop.wm.close(id);
                    }
                }
                "help.about" => open_about_modal(desktop, screen)?,
                "theme.dark" => {
                    *is_dark = true;
                    desktop.theme = Theme::dark();
                }
                "theme.light" => {
                    *is_dark = false;
                    desktop.theme = Theme::light();
                }
                _ => {
                    // Unknown commands are ignored.
                }
            }
        }

        // Application-level shortcuts: only run if the event was not handled by the view/window/desktop.
        if result.outcome == EventOutcome::Ignored
            && let Event::Key(KeyEvent {
                code,
                modifiers,
                kind: KeyEventKind::Press,
                ..
            }) = ev
        {
            match (code, modifiers) {
                (KeyCode::Char('q'), KeyModifiers::NONE) => break,
                (KeyCode::F(2), _) => {
                    *is_dark = !*is_dark;
                    desktop.theme = if *is_dark {
                        Theme::dark()
                    } else {
                        Theme::light()
                    };
                }
                (KeyCode::Char('n'), KeyModifiers::NONE) => {
                    next_float += 1;
                    let title = format!("Floating {next_float}");
                    desktop.add_window(
                        Window::new(
                            WindowKind::Floating,
                            title,
                            Rect {
                                x: 8 + (next_float as u16 % 20),
                                y: 4 + (next_float as u16 % 8),
                                width: 30,
                                height: 8,
                            },
                            Box::new(TextView::new(vec![
                                "Floating window".into(),
                                "Press Esc to close.".into(),
                            ])),
                        ),
                        screen,
                    );
                }
                (KeyCode::Char('a'), KeyModifiers::NONE) => {
                    open_about_modal(desktop, screen)?;
                }
                (KeyCode::Char('t'), KeyModifiers::NONE) => {
                    open_tooltip(desktop, screen, tooltip)?;
                }
                (KeyCode::Char('v'), KeyModifiers::NONE) => {
                    open_layout_demo(desktop, screen, layout_demo_window_id)?;
                }
                (KeyCode::Char('s'), KeyModifiers::NONE) => {
                    open_scroll_demo(desktop, screen, scroll_demo_window_id)?;
                }
                _ => {}
            }
        }

        // Keep the Notes window from being minimized as a small demo nicety.
        if let Some(w) = desktop.wm.window_mut(notes_window_id)
            && w.state == WindowState::Minimized
        {
            w.state = WindowState::Normal;
        }
    }
    Ok(())
}

fn open_about_modal(desktop: &mut Desktop, screen: Rect) -> Result<()> {
    let work = Desktop::layout(screen).work_area;
    let w = 46.min(work.width.saturating_sub(2)).max(20);
    let h = 12.min(work.height.saturating_sub(2)).max(7);
    let rect = Rect {
        x: work.x + (work.width.saturating_sub(w)) / 2,
        y: work.y + (work.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };
    desktop.add_window(
        Window::new(
            WindowKind::Modal,
            "About",
            rect,
            Box::new(DialogView::about()),
        ),
        screen,
    );
    Ok(())
}

fn open_layout_demo(
    desktop: &mut Desktop,
    screen: Rect,
    layout_demo_window_id: &mut Option<WindowId>,
) -> Result<()> {
    if let Some(id) = *layout_demo_window_id
        && desktop.wm.window_mut(id).is_some()
    {
        desktop.wm.focus(id);
        desktop.wm.bring_to_front(id);
        return Ok(());
    }

    let work = Desktop::layout(screen).work_area;
    let id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Layout",
            Rect {
                x: work.x.saturating_add(43),
                y: work.y.saturating_add(1),
                width: 36,
                height: 15,
            },
            Box::new(build_layout_demo_view()),
        ),
        screen,
    );
    *layout_demo_window_id = Some(id);
    Ok(())
}

fn open_scroll_demo(
    desktop: &mut Desktop,
    screen: Rect,
    scroll_demo_window_id: &mut Option<WindowId>,
) -> Result<()> {
    if let Some(id) = *scroll_demo_window_id
        && desktop.wm.window_mut(id).is_some()
    {
        desktop.wm.focus(id);
        desktop.wm.bring_to_front(id);
        return Ok(());
    }

    let work = Desktop::layout(screen).work_area;
    let id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Scroll",
            Rect {
                x: work.x.saturating_add(3),
                y: work.y.saturating_add(4),
                width: 44,
                height: 14,
            },
            Box::new(build_scroll_demo_view()),
        ),
        screen,
    );
    *scroll_demo_window_id = Some(id);
    Ok(())
}

fn open_tooltip(
    desktop: &mut Desktop,
    screen: Rect,
    tooltip: &mut Option<(WindowId, Instant)>,
) -> Result<()> {
    if let Some((id, _)) = tooltip.take() {
        desktop.wm.close(id);
    }

    let work = Desktop::layout(screen).work_area;
    let (x, y) = if let Some(focused) = desktop.wm.focused() {
        if let Some(w) = desktop.wm.windows().iter().find(|w| w.id == focused) {
            (w.rect.x.saturating_add(2), w.rect.y.saturating_add(2))
        } else {
            (work.x + 2, work.y + 2)
        }
    } else {
        (work.x + 2, work.y + 2)
    };

    let rect = Rect {
        x,
        y,
        width: 26.min(work.width.saturating_sub(2)),
        height: 4,
    };
    let id = desktop.add_window(
        Window::new(
            WindowKind::Tooltip,
            "Tip",
            rect,
            Box::new(TooltipView::new("Tooltip: press 't' again to close.")),
        ),
        screen,
    );
    if let Some(w) = desktop.wm.window_mut(id) {
        w.decorations.shadow = false;
        w.closable = false;
    }
    *tooltip = Some((id, Instant::now() + Duration::from_millis(1200)));
    Ok(())
}
