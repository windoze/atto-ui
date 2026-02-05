use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};

use atto_ui::app::{Desktop, MenuBar, MenuItem, MenuSpec};
use atto_ui::declarative::{DeclarativeView, EdgeInsets, HStack, LayoutParams, Size, VStack};
use atto_ui::reactive::{EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::view::{View, ViewContext, ViewEventResult};
use atto_ui::widgets::{
    Button, Checkbox, ControlOutcome, Form, Label, ListBox, RadioGroup, TableView, TextBox,
};
use atto_ui::wm::{Window, WindowKind};

#[derive(Clone, Debug)]
enum SnapshotAppAction {
    Quit,
    OpenAbout,
    SetThemeDark,
    SetThemeLight,
}

#[derive(Default)]
struct LogView {
    is_dark: Arc<AtomicBool>,
}

impl LogView {
    fn new(is_dark: Arc<AtomicBool>) -> Self {
        Self { is_dark }
    }
}

impl View for LogView {
    fn handle_event(&mut self, _event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        ViewEventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        let style = if ctx.is_focused {
            ctx.theme.widget.focused
        } else {
            ctx.theme.widget.normal
        };
        let theme_label = if self.is_dark.load(Ordering::SeqCst) {
            "Dark"
        } else {
            "Light"
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled("Log window (click to focus).", style),
                Line::styled(format!("Theme: {theme_label}"), style),
            ]),
            area,
        );
    }
}

struct WidgetsView {
    form: Form,
}

impl WidgetsView {
    fn new() -> Self {
        let text = Property::new("hello".to_string());
        let enable_feature = Property::new(true);
        let mode = Property::new(0usize);
        let list_selection = Property::new(0usize);
        let table_selection = Property::new(0usize);

        let controls: Vec<Box<dyn atto_ui::widgets::Control>> = vec![
            Box::new(Label::new(
                "Paste Unicode into the textbox (bracketed paste).",
            )),
            Box::new(TextBox::new("Text", text.binding())),
            Box::new(Checkbox::new("Enable feature", enable_feature.binding())),
            Box::new(RadioGroup::new(
                "Mode",
                vec!["Normal".into(), "Insert".into(), "Visual".into()],
                mode.binding(),
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
                    list_selection.binding(),
                )
                .height(5u16),
            ),
            Box::new(
                TableView::new(
                    "Table",
                    vec!["Key".into(), "Value".into()],
                    vec![
                        vec!["lang".into(), "Rust".into()],
                        vec!["hello".into(), "こんにちは".into()],
                        vec!["wide".into(), "你好👋".into()],
                    ],
                    table_selection.binding(),
                )
                .height(6u16),
            ),
            Box::new(Button::new("OK")),
        ];
        Self {
            form: Form::new(controls),
        }
    }
}

impl View for WidgetsView {
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        let (outcome, _action) = self.form.handle_event(event);
        match outcome {
            ControlOutcome::Consumed => ViewEventResult::consumed(),
            ControlOutcome::Ignored => ViewEventResult::ignored(),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.form.draw(frame, area, ctx.theme, ctx.is_focused);
    }
}

fn view_hierarchy_demo() -> Box<dyn View> {
    let row = HStack::new()
        .spacing(2)
        .child_with_layout(
            Checkbox::new("Nested checkbox", Property::new(false).binding()),
            LayoutParams {
                width: Size::Fixed(20),
                ..LayoutParams::default()
            },
        )
        .child(Label::new("click to toggle"));

    VStack::new()
        .padding_insets(EdgeInsets::all(1))
        .spacing(1)
        .child_with_layout(
            Label::new("View hierarchy demo (VStack + HStack)"),
            LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            row,
            LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .build_view()
}

#[derive(Default)]
struct AboutView;

impl View for AboutView {
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc | KeyCode::Enter,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            return ViewEventResult::close_window();
        }
        ViewEventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        frame.render_widget(
            Paragraph::new(Line::styled(
                "About modal (Esc to close).",
                ctx.theme.widget.normal,
            )),
            area,
        );
    }
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        event::EnableMouseCapture,
        event::EnableBracketedPaste,
        cursor::Show
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let actions: EventQueue<SnapshotAppAction> = EventQueue::new();

    let theme_state = Arc::new(AtomicBool::new(true));
    let mut is_dark = true;
    let menu = MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![
                MenuItem::submenu(
                    "Theme",
                    vec![
                        MenuItem::action("Dark", {
                            let actions = actions.clone();
                            move || actions.push(SnapshotAppAction::SetThemeDark)
                        })
                        .shortcut("d"),
                        MenuItem::action("Light", {
                            let actions = actions.clone();
                            move || actions.push(SnapshotAppAction::SetThemeLight)
                        })
                        .shortcut("l"),
                    ],
                ),
                MenuItem::action("Quit", {
                    let actions = actions.clone();
                    move || actions.push(SnapshotAppAction::Quit)
                })
                .shortcut("q"),
            ],
        ),
        MenuSpec::new(
            "Help",
            vec![
                MenuItem::action("About", {
                    let actions = actions.clone();
                    move || actions.push(SnapshotAppAction::OpenAbout)
                })
                .shortcut("a"),
            ],
        ),
    ]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    // Create windows after we know the screen bounds.
    let screen: Rect = terminal.size()?.into();
    let widgets_id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Widgets",
            Rect {
                x: 2,
                y: 2,
                width: 42,
                height: 20,
            },
            Box::new(WidgetsView::new()),
        ),
        screen,
    );
    let _log_id = desktop.add_window(
        Window::new(
            WindowKind::Floating,
            "Log",
            Rect {
                x: 46,
                y: 4,
                width: 30,
                height: 10,
            },
            Box::new(LogView::new(Arc::clone(&theme_state))),
        ),
        screen,
    );

    let _views_id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Views",
            Rect {
                x: 46,
                y: 14,
                width: 30,
                height: 8,
            },
            view_hierarchy_demo(),
        ),
        screen,
    );
    desktop.wm.focus(widgets_id);

    let res = run(
        &mut terminal,
        &mut desktop,
        &actions,
        &mut is_dark,
        &theme_state,
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

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    desktop: &mut Desktop,
    actions: &EventQueue<SnapshotAppAction>,
    is_dark: &mut bool,
    theme_state: &Arc<AtomicBool>,
) -> Result<()> {
    loop {
        terminal.draw(|f| desktop.draw(f))?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        // Deterministic quit key for PTY tests: accept both Ctrl+Q and raw DC1 (0x11). This is
        // checked before dispatch so focused text inputs can't consume it.
        if let Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = ev
            && modifiers.contains(KeyModifiers::CONTROL)
        {
            break;
        }
        if let Event::Key(KeyEvent {
            code: KeyCode::Char('\u{11}'),
            kind: KeyEventKind::Press,
            ..
        }) = ev
        {
            break;
        }

        let screen: Rect = terminal.size()?.into();
        let result = desktop.handle_event(&ev, screen);

        for action in actions.drain() {
            match action {
                SnapshotAppAction::Quit => return Ok(()),
                SnapshotAppAction::OpenAbout => {
                    open_about_modal(desktop, screen);
                }
                SnapshotAppAction::SetThemeDark => {
                    *is_dark = true;
                    theme_state.store(true, Ordering::SeqCst);
                    desktop.theme = Theme::dark();
                }
                SnapshotAppAction::SetThemeLight => {
                    *is_dark = false;
                    theme_state.store(false, Ordering::SeqCst);
                    desktop.theme = Theme::light();
                }
            }
        }

        // Application-level shortcuts: only run if the event was not handled by the view/window/desktop.
        if result.outcome == atto_ui::view::EventOutcome::Ignored
            && let Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Press,
                ..
            }) = ev
        {
            match code {
                KeyCode::Char('q') => break,
                KeyCode::F(2) => {
                    *is_dark = !*is_dark;
                    theme_state.store(*is_dark, Ordering::SeqCst);
                    desktop.theme = if *is_dark {
                        Theme::dark()
                    } else {
                        Theme::light()
                    };
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn open_about_modal(desktop: &mut Desktop, screen: Rect) {
    if desktop.wm.has_active_modal() {
        return;
    }

    let work = Desktop::layout(screen).work_area;
    let w = 36.min(work.width.saturating_sub(2)).max(20);
    let h = 7.min(work.height.saturating_sub(2)).max(5);
    let rect = Rect {
        x: work.x + (work.width.saturating_sub(w)) / 2,
        y: work.y + (work.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };
    desktop.add_window(
        Window::new(WindowKind::Modal, "About", rect, Box::new(AboutView)),
        screen,
    );
}
