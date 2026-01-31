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

use chatty::app::{Desktop, MenuBar, MenuItem, MenuSpec};
use chatty::theme::Theme;
use chatty::view::{View, ViewContext, ViewEventResult};
use chatty::views::{ControlView, EdgeInsets, HBox, LayoutParams, Size, VBox};
use chatty::widgets::{
    Button, Checkbox, ControlOutcome, Form, Label, ListBox, RadioGroup, TableView, TextBox,
};
use chatty::wm::{Window, WindowKind};

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
        let controls: Vec<Box<dyn chatty::widgets::Control>> = vec![
            Box::new(Label::new(
                "Paste Unicode into the textbox (bracketed paste).",
            )),
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
                .with_height(5),
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
                )
                .with_height(6),
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

fn view_hierarchy_demo() -> VBox {
    let mut root = VBox::new().with_padding(EdgeInsets::all(1)).with_spacing(1);

    root.add_child_with_layout(
        Box::new(ControlView::new(Box::new(Label::new(
            "View hierarchy demo (VBox + HBox + ControlView)",
        )))),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );

    let mut row = HBox::new().with_spacing(2);
    row.add_child_with_layout(
        Box::new(ControlView::new(Box::new(Checkbox::new(
            "Nested checkbox",
            false,
        )))),
        LayoutParams {
            width: Size::Fixed(20),
            ..LayoutParams::default()
        },
    );
    row.add_child(Box::new(ControlView::new(Box::new(Label::new(
        "click to toggle",
    )))));

    root.add_child_with_layout(
        Box::new(row),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );

    root
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

    let theme_state = Arc::new(AtomicBool::new(true));
    let mut is_dark = true;
    let menu = MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![
                MenuItem::submenu(
                    "Theme",
                    vec![
                        MenuItem::command("Dark", "theme.dark").shortcut("d"),
                        MenuItem::command("Light", "theme.light").shortcut("l"),
                    ],
                ),
                MenuItem::command("Quit", "app.quit").shortcut("q"),
            ],
        ),
        MenuSpec::new(
            "Help",
            vec![MenuItem::command("About", "help.about").shortcut("a")],
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
            Box::new(view_hierarchy_demo()),
        ),
        screen,
    );
    desktop.wm.focus(widgets_id);

    let res = run(&mut terminal, &mut desktop, &mut is_dark, &theme_state);

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

        match result.action {
            chatty::app::DesktopAction::MenuCommand(cmd) if cmd == "app.quit" => break,
            chatty::app::DesktopAction::MenuCommand(cmd) if cmd == "help.about" => {
                open_about_modal(desktop, screen);
            }
            chatty::app::DesktopAction::MenuCommand(cmd) if cmd == "theme.dark" => {
                *is_dark = true;
                theme_state.store(true, Ordering::SeqCst);
                desktop.theme = Theme::dark();
            }
            chatty::app::DesktopAction::MenuCommand(cmd) if cmd == "theme.light" => {
                *is_dark = false;
                theme_state.store(false, Ordering::SeqCst);
                desktop.theme = Theme::light();
            }
            _ => {}
        }

        // Application-level shortcuts: only run if the event was not handled by the view/window/desktop.
        if result.outcome == chatty::view::EventOutcome::Ignored
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
