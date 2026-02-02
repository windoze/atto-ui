// Demo: 06-data-binding
// 演示 Chatty 的反应式数据绑定：Property / Binding + 双向同步 + 禁用状态。

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;

use chatty::app::{Desktop, DesktopAction, MenuBar};
use chatty::declarative::{
    DeclarativeView, Divider, EdgeInsets, HStack, LayoutParams, Size, Spacer, Text, TextFn, VStack,
    ViewAdapter,
};
use chatty::reactive::Property;
use chatty::theme::Theme;
use chatty::view::EventOutcome;
use chatty::widgets::{Button, Checkbox, Label, RadioGroup, TextBox};
use chatty::wm::{Window, WindowKind};

fn content_height() -> LayoutParams {
    LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    }
}

#[derive(Clone)]
struct AppModel {
    editor_enabled: Property<bool>,
    name: Property<String>,
    email: Property<String>,
    notes: Property<String>,
    subscribed: Property<bool>,
    role: Property<usize>,
    counter: Property<u32>,
    status: Property<String>,
    sample_idx: Property<usize>,
}

impl AppModel {
    fn new() -> Self {
        Self {
            editor_enabled: Property::new(true),
            name: Property::new("Alice".to_string()),
            email: Property::new("alice@example.com".to_string()),
            notes: Property::new(String::new()),
            subscribed: Property::new(true),
            role: Property::new(0),
            counter: Property::new(0),
            status: Property::new("Ready. Try editing fields or click 'Load sample'.".to_string()),
            sample_idx: Property::new(0),
        }
    }

    fn load_sample(&self) {
        const SAMPLES: &[(&str, &str, bool, usize)] = &[
            ("Alice", "alice@example.com", true, 0),
            ("Bob", "bob@example.com", false, 1),
            ("Chen", "chen@example.com", true, 2),
            ("Dora", "dora@example.com", true, 0),
        ];

        let idx = self.sample_idx.get() % SAMPLES.len();
        let (name, email, subscribed, role) = SAMPLES[idx];

        self.name.set(name.to_string());
        self.email.set(email.to_string());
        self.subscribed.set(subscribed);
        self.role.set(role);
        self.status
            .set(format!("Loaded sample #{idx}: {name} ({email})"));

        self.sample_idx.update(|i| *i = i.saturating_add(1));
    }

    fn clear(&self) {
        self.name.set(String::new());
        self.email.set(String::new());
        self.notes.set(String::new());
        self.subscribed.set(false);
        self.role.set(0);
        self.status.set("Cleared fields.".to_string());
    }
}

struct EditorView {
    model: AppModel,
}

impl EditorView {
    fn new(model: AppModel) -> Self {
        Self { model }
    }
}

impl DeclarativeView for EditorView {
    fn body(&self) -> Box<dyn DeclarativeView> {
        let model = self.model.clone();
        let editor_enabled = model.editor_enabled.clone();

        let buttons = {
            let enabled = editor_enabled.binding();
            let model_load = model.clone();
            let model_clear = model.clone();
            let model_count = model.clone();

            HStack::new()
                .spacing(1)
                .child(
                    Button::new("Load sample")
                        .enabled(enabled.clone())
                        .on_click(move || model_load.load_sample()),
                )
                .child(
                    Button::new("Clear")
                        .enabled(enabled.clone())
                        .on_click(move || model_clear.clear()),
                )
                .child(Spacer::new())
                .child(
                    Button::new("Count +1")
                        .enabled(enabled.clone())
                        .on_click(move || {
                            model_count.counter.update(|c| *c = c.saturating_add(1));
                            model_count
                                .status
                                .set(format!("Counter = {}", model_count.counter.get()));
                        }),
                )
        };

        let status_line = {
            let status = model.status.clone();
            TextFn::new(move || format!("Status: {}", status.get()))
        };

        Box::new(
            VStack::new()
                .spacing(1)
                .padding(1)
                .child_with_layout(Text::new("Data Binding Demo (Editor)"), content_height())
                .child_with_layout(
                    Text::new("Tip: 'q' quits only when the focused widget did not consume the key; Ctrl+Q always quits."),
                    content_height(),
                )
                .child_with_layout(Divider::horizontal(), content_height())
                .child_with_layout(
                    Checkbox::new(
                        "Enable editor (disables inputs/buttons below)",
                        model.editor_enabled.binding(),
                    ),
                    content_height(),
                )
                .child_with_layout(
                    VStack::new()
                        .spacing(1)
                        .child(TextBox::new("Name", model.name.binding()).enabled(
                            editor_enabled.binding(),
                        ))
                        .child(TextBox::new("Email", model.email.binding()).enabled(
                            editor_enabled.binding(),
                        ))
                        .child(Checkbox::new("Subscribed", model.subscribed.binding()).enabled(
                            editor_enabled.binding(),
                        ))
                        .child(
                            RadioGroup::new(
                                "Role",
                                vec!["User".into(), "Admin".into(), "Guest".into()],
                                model.role.binding(),
                            )
                            .enabled(editor_enabled.binding()),
                        )
                        .child(TextBox::new("Notes (single-line)", model.notes.binding()).enabled(
                            editor_enabled.binding(),
                        )),
                    LayoutParams {
                        height: Size::Fill,
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(Divider::horizontal(), content_height())
                .child_with_layout(
                    buttons,
                    LayoutParams {
                        height: Size::Fixed(3),
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    status_line,
                    LayoutParams {
                        height: Size::Fixed(1),
                        ..LayoutParams::default()
                    },
                ),
        )
    }
}

struct MirrorView {
    model: AppModel,
}

impl MirrorView {
    fn new(model: AppModel) -> Self {
        Self { model }
    }
}

impl DeclarativeView for MirrorView {
    fn body(&self) -> Box<dyn DeclarativeView> {
        let model = self.model.clone();
        let name = model.name.clone();
        let email = model.email.clone();
        let notes = model.notes.clone();
        let subscribed = model.subscribed.clone();
        let role = model.role.clone();
        let counter = model.counter.clone();

        let summary = TextFn::new(move || {
            let role_label = match role.get() {
                0 => "User",
                1 => "Admin",
                2 => "Guest",
                _ => "Unknown",
            };
            format!(
                "Summary: name=\"{}\"  email=\"{}\"  notes=\"{}\"  subscribed={}  role={}  counter={}",
                name.get(),
                email.get(),
                notes.get(),
                subscribed.get(),
                role_label,
                counter.get(),
            )
        });

        Box::new(
            VStack::new()
                .spacing(1)
                .padding_insets(EdgeInsets::all(1))
                .child_with_layout(Text::new("Data Binding Demo (Mirror)"), content_height())
                .child_with_layout(
                    Text::new("These widgets share the same bindings as the Editor window."),
                    content_height(),
                )
                .child_with_layout(Divider::horizontal(), content_height())
                .child_with_layout(Label::new("Try editing on either side:"), content_height())
                .child(TextBox::new("Name (mirror)", model.name.binding()))
                .child(Checkbox::new(
                    "Subscribed (mirror)",
                    model.subscribed.binding(),
                ))
                .child_with_layout(Divider::horizontal(), content_height())
                .child_with_layout(
                    summary,
                    LayoutParams {
                        height: Size::Fixed(1),
                        ..LayoutParams::default()
                    },
                ),
        )
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
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let model = AppModel::new();

    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;
    let gutter = 2;
    let half = work.width / 2;

    let left = Rect {
        x: work.x.saturating_add(gutter),
        y: work.y.saturating_add(1),
        width: half.saturating_sub(gutter.saturating_add(1)).max(20),
        height: work.height.saturating_sub(2).max(10),
    };
    let right = Rect {
        x: work.x.saturating_add(half).saturating_add(1),
        y: work.y.saturating_add(1),
        width: work
            .width
            .saturating_sub(half)
            .saturating_sub(gutter.saturating_add(1))
            .max(20),
        height: work.height.saturating_sub(2).max(10),
    };

    let editor_id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Data Binding - Editor",
            left,
            Box::new(ViewAdapter::new(EditorView::new(model.clone()))),
        ),
        screen,
    );
    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Data Binding - Mirror",
            right,
            Box::new(ViewAdapter::new(MirrorView::new(model.clone()))),
        ),
        screen,
    );
    desktop.wm.focus(editor_id);

    let res = run(&mut terminal, &mut desktop);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        event::DisableMouseCapture,
        event::DisableBracketedPaste,
    )?;
    terminal.show_cursor()?;

    res
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, desktop: &mut Desktop) -> Result<()> {
    loop {
        terminal.draw(|f| desktop.draw(f))?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }

        let ev = event::read()?;
        let screen: Rect = terminal.size()?.into();
        let result = desktop.handle_event(&ev, screen);

        if let DesktopAction::CloseWindow(id) = result.action {
            desktop.wm.close(id);
        }

        if should_quit(&ev, result.outcome) {
            break;
        }
    }

    Ok(())
}

fn should_quit(event: &Event, outcome: EventOutcome) -> bool {
    match event {
        // Ctrl+Q always quits.
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) => true,
        // 'q' quits only when the event was not consumed by the UI.
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            ..
        }) => outcome == EventOutcome::Ignored,
        _ => false,
    }
}
