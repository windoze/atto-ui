// Demo: 09-custom-components
// 演示如何用声明式 + 反应式 API 封装可复用组件（含：bindings / callbacks / disabled state）。

use std::io;
use std::sync::Arc;
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
    Align, DeclarativeView, Divider, EdgeInsets, HStack, LayoutParams, Size, Spacer, Text, TextFn,
    VStack, ViewAdapter,
};
use chatty::reactive::{Binding, Property};
use chatty::theme::Theme;
use chatty::view::EventOutcome;
use chatty::widgets::{Button, Checkbox, TextBox};
use chatty::wm::{Window, WindowKind};

fn content_height() -> LayoutParams {
    LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    }
}

#[derive(Clone)]
struct DemoModel {
    enabled: Property<bool>,
    name: Property<String>,
    email: Property<String>,
    subscribed: Property<bool>,
    volume: Property<i32>,
    brightness: Property<i32>,
    last_search: Property<String>,
    status: Property<String>,
}

impl DemoModel {
    fn new() -> Self {
        Self {
            enabled: Property::new(true),
            name: Property::new("Alice".to_string()),
            email: Property::new("alice@example.com".to_string()),
            subscribed: Property::new(true),
            volume: Property::new(5),
            brightness: Property::new(7),
            last_search: Property::new(String::new()),
            status: Property::new("Ready.".to_string()),
        }
    }

    fn reset(&self) {
        self.enabled.set(true);
        self.name.set("Alice".to_string());
        self.email.set("alice@example.com".to_string());
        self.subscribed.set(true);
        self.volume.set(5);
        self.brightness.set(7);
        self.last_search.set(String::new());
        self.status.set("Reset to defaults.".to_string());
    }
}

/// A simple wrapper around TextBox to demonstrate "component = reusable building block".
#[derive(Clone)]
struct LabeledField {
    title: String,
    value: Binding<String>,
    enabled: Binding<bool>,
}

impl LabeledField {
    fn new(title: impl Into<String>, value: Binding<String>) -> Self {
        Self {
            title: title.into(),
            value,
            enabled: true.into(),
        }
    }

    fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }
}

impl DeclarativeView for LabeledField {
    fn body(&self) -> Box<dyn DeclarativeView> {
        Box::new(TextBox::new(self.title.clone(), self.value.clone()).enabled(self.enabled.clone()))
    }
}

/// A small counter component (value is fully owned by parent via binding).
#[derive(Clone)]
struct CounterRow {
    title: String,
    value: Binding<i32>,
    enabled: Binding<bool>,
}

impl CounterRow {
    fn new(title: impl Into<String>, value: Binding<i32>) -> Self {
        Self {
            title: title.into(),
            value,
            enabled: true.into(),
        }
    }

    fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }
}

impl DeclarativeView for CounterRow {
    fn body(&self) -> Box<dyn DeclarativeView> {
        let enabled = self.enabled.clone();
        let value_for_minus = self.value.clone();
        let value_for_plus = self.value.clone();
        let value_for_text = self.value.clone();
        let title = self.title.clone();

        Box::new(
            HStack::new()
                .spacing(1)
                .child_with_layout(
                    Text::new(title),
                    LayoutParams {
                        width: Size::Weight(1),
                        height: Size::Content,
                        align_y: Align::Center,
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    Button::new("−")
                        .enabled(enabled.clone())
                        .on_click(move || value_for_minus.update(|v| *v = v.saturating_sub(1))),
                    LayoutParams {
                        width: Size::Fixed(5),
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    Button::new("+")
                        .enabled(enabled.clone())
                        .on_click(move || value_for_plus.update(|v| *v = v.saturating_add(1))),
                    LayoutParams {
                        width: Size::Fixed(5),
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    TextFn::new(move || format!("{}", value_for_text.get())),
                    LayoutParams {
                        width: Size::Fixed(12),
                        height: Size::Content,
                        align_y: Align::Center,
                        ..LayoutParams::default()
                    },
                ),
        )
    }
}

/// A component with local state + a callback back to the parent.
#[derive(Clone)]
struct SearchBar {
    query: Property<String>,
    enabled: Binding<bool>,
    on_search: Arc<dyn Fn(String) + Send + Sync>,
}

impl SearchBar {
    fn new(on_search: Arc<dyn Fn(String) + Send + Sync>) -> Self {
        Self {
            query: Property::new(String::new()),
            enabled: true.into(),
            on_search,
        }
    }

    fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }
}

impl DeclarativeView for SearchBar {
    fn body(&self) -> Box<dyn DeclarativeView> {
        let enabled = self.enabled.clone();
        let query = self.query.clone();
        let on_search = self.on_search.clone();

        Box::new(
            HStack::new()
                .spacing(1)
                .child_with_layout(
                    TextBox::new("Query", query.binding()).enabled(enabled.clone()),
                    LayoutParams {
                        width: Size::Weight(1),
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    Button::new("Search")
                        .enabled(enabled.clone())
                        .on_click(move || {
                            let q = query.get();
                            if !q.trim().is_empty() {
                                on_search(q.clone());
                            }
                            query.set(String::new());
                        }),
                    LayoutParams {
                        width: Size::Fixed(10),
                        ..LayoutParams::default()
                    },
                ),
        )
    }
}

struct ComponentsView {
    model: DemoModel,
}

impl ComponentsView {
    fn new(model: DemoModel) -> Self {
        Self { model }
    }
}

impl DeclarativeView for ComponentsView {
    fn body(&self) -> Box<dyn DeclarativeView> {
        let model = self.model.clone();
        let enabled = model.enabled.binding();

        let search_callback = {
            let model = model.clone();
            Arc::new(move |q: String| {
                model.last_search.set(q.clone());
                model.status.set(format!("Searched for: {q}"));
            })
        };

        let reset_button = {
            let model = model.clone();
            Button::new("Reset").on_click(move || model.reset())
        };

        let status_line = {
            let status = model.status.clone();
            TextFn::new(move || format!("Status: {}", status.get()))
        };

        Box::new(
            VStack::new()
                .spacing(1)
                .padding(1)
                .scrollable(true)
                .child_with_layout(Text::new("Custom Components Demo"), content_height())
                .child_with_layout(
                    Text::new(
                        "Components are plain Rust structs that compose views + pass bindings/callbacks.",
                    ),
                    content_height(),
                )
                .child_with_layout(
                    Text::new(
                        "Quit: Ctrl+Q always; 'q' only when the focused widget did not consume it.",
                    ),
                    content_height(),
                )
                .child_with_layout(Divider::horizontal(), content_height())
                .child_with_layout(
                    Checkbox::new("Enable controls", model.enabled.binding()),
                    content_height(),
                )
                .child_with_layout(
                    VStack::new()
                        .spacing(1)
                        .child_with_layout(Text::new("Profile"), content_height())
                        .child_with_layout(
                            LabeledField::new("Name", model.name.binding()).enabled(enabled.clone()),
                            content_height(),
                        )
                        .child_with_layout(
                            LabeledField::new("Email", model.email.binding()).enabled(enabled.clone()),
                            content_height(),
                        )
                        .child_with_layout(
                            Checkbox::new("Subscribed", model.subscribed.binding())
                                .enabled(enabled.clone()),
                            content_height(),
                        ),
                    LayoutParams {
                        height: Size::Content,
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(Divider::horizontal(), content_height())
                .child_with_layout(
                    VStack::new()
                        .spacing(1)
                        .child_with_layout(Text::new("Tuning"), content_height())
                        .child_with_layout(
                            CounterRow::new("Volume", model.volume.binding()).enabled(enabled.clone()),
                            content_height(),
                        )
                        .child_with_layout(
                            CounterRow::new("Brightness", model.brightness.binding()).enabled(enabled.clone()),
                            content_height(),
                        ),
                    LayoutParams {
                        height: Size::Content,
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(Divider::horizontal(), content_height())
                .child_with_layout(
                    VStack::new()
                        .spacing(1)
                        .child_with_layout(
                            Text::new("Search (local state + callback)"),
                            content_height(),
                        )
                        .child_with_layout(
                            SearchBar::new(search_callback).enabled(enabled.clone()),
                            content_height(),
                        )
                        .child_with_layout(status_line, content_height()),
                    LayoutParams {
                        height: Size::Content,
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(Divider::horizontal(), content_height())
                .child_with_layout(
                    HStack::new()
                        .spacing(1)
                        .child(reset_button.enabled(enabled.clone()))
                        .child(Spacer::new()),
                    content_height(),
                ),
        )
    }
}

struct PreviewView {
    model: DemoModel,
}

impl PreviewView {
    fn new(model: DemoModel) -> Self {
        Self { model }
    }
}

impl DeclarativeView for PreviewView {
    fn body(&self) -> Box<dyn DeclarativeView> {
        let model = self.model.clone();
        let name = model.name.clone();
        let email = model.email.clone();
        let subscribed = model.subscribed.clone();
        let volume = model.volume.clone();
        let brightness = model.brightness.clone();
        let last_search = model.last_search.clone();
        let enabled = model.enabled.clone();

        let summary = TextFn::new(move || {
            format!(
                "User: {} <{}>  subscribed={}  volume={}  brightness={}  last_search=\"{}\"  enabled={}",
                name.get(),
                email.get(),
                subscribed.get(),
                volume.get(),
                brightness.get(),
                last_search.get(),
                enabled.get(),
            )
        });

        Box::new(
            VStack::new()
                .spacing(1)
                .padding_insets(EdgeInsets::all(1))
                .child_with_layout(Text::new("Preview (shared bindings)"), content_height())
                .child_with_layout(
                    Text::new(
                        "This window is read-only; it updates live as you interact with the components.",
                    ),
                    content_height(),
                )
                .child_with_layout(Divider::horizontal(), content_height())
                .child_with_layout(summary, content_height()),
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

    let model = DemoModel::new();

    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;
    let gutter = 2;
    let half = work.width / 2;

    let left = Rect {
        x: work.x.saturating_add(gutter),
        y: work.y.saturating_add(1),
        width: half.saturating_sub(gutter.saturating_add(1)).max(30),
        height: work.height.saturating_sub(2).max(12),
    };
    let right = Rect {
        x: work.x.saturating_add(half).saturating_add(1),
        y: work.y.saturating_add(1),
        width: work
            .width
            .saturating_sub(half)
            .saturating_sub(gutter.saturating_add(1))
            .max(30),
        height: work.height.saturating_sub(2).max(12),
    };

    let components_id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Custom Components",
            left,
            Box::new(ViewAdapter::new(ComponentsView::new(model.clone()))),
        ),
        screen,
    );
    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Preview",
            right,
            Box::new(ViewAdapter::new(PreviewView::new(model.clone()))),
        ),
        screen,
    );
    desktop.wm.focus(components_id);

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
