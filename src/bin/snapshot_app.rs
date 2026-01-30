use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
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
use chatty::view::{View, ViewAction, ViewContext};
use chatty::widgets::{Button, Checkbox, Form, Label, ListBox, RadioGroup, TableView, TextBox};
use chatty::wm::{Window, WindowKind};

#[derive(Default)]
struct LogView;

impl View for LogView {
    fn handle_event(&mut self, _event: &Event, _ctx: ViewContext<'_>) -> ViewAction {
        ViewAction::None
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        let style = if ctx.is_focused {
            ctx.theme.widget.focused
        } else {
            ctx.theme.widget.normal
        };
        frame.render_widget(
            Paragraph::new(Line::styled("Log window (click to focus).", style)),
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
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewAction {
        let _ = self.form.handle_event(event);
        ViewAction::None
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.form.draw(frame, area, ctx.theme);
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

    let mut is_dark = true;
    let menu = MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![MenuItem::command("Quit", "app.quit").shortcut("q")],
        ),
        MenuSpec::new(
            "Help",
            vec![MenuItem::command("About", "help.about").shortcut("F1")],
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
            Box::new(LogView),
        ),
        screen,
    );
    desktop.wm.focus(widgets_id);

    let res = run(&mut terminal, &mut desktop, &mut is_dark);

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
) -> Result<()> {
    loop {
        terminal.draw(|f| desktop.draw(f))?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        if let Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            kind: KeyEventKind::Press,
            ..
        }) = ev
        {
            break;
        }

        if let Event::Key(KeyEvent {
            code: KeyCode::F(2),
            kind: KeyEventKind::Press,
            ..
        }) = ev
        {
            *is_dark = !*is_dark;
            desktop.theme = if *is_dark {
                Theme::dark()
            } else {
                Theme::light()
            };
            continue;
        }

        match desktop.handle_event(&ev, terminal.size()?.into()) {
            chatty::app::DesktopAction::MenuCommand(cmd) if cmd == "app.quit" => break,
            _ => {}
        }
    }
    Ok(())
}
