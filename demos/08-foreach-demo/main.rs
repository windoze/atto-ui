// Demo: 08-foreach-demo
// 演示 ForEach：动态增删、稳定 ID 复用、滚动/滚动条、以及与 TextBox 的配合。

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
    Align, DeclarativeView, Divider, EdgeInsets, ForEach, HStack, Identifiable, LayoutParams, Size,
    Spacer, Text, TextFn, VStack, ViewAdapter,
};
use chatty::reactive::Property;
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
struct TodoItem {
    id: usize,
    text: Property<String>,
    done: Property<bool>,
}

impl TodoItem {
    fn new(id: usize, text: impl Into<String>) -> Self {
        Self {
            id,
            text: Property::new(text.into()),
            done: Property::new(false),
        }
    }
}

// We keep equality intentionally shallow (stable identity only) so ForEachIdentifiable can reuse
// existing views and preserve view-local state (e.g. TextBox cursor position) during reorders.
impl PartialEq for TodoItem {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Identifiable for TodoItem {
    type Id = usize;

    fn id(&self) -> Self::Id {
        self.id
    }
}

#[derive(Clone)]
struct TodoModel {
    next_id: Property<usize>,
    new_text: Property<String>,
    todos: Property<Vec<TodoItem>>,
}

impl TodoModel {
    fn new() -> Self {
        let todos = (0..200usize)
            .map(|i| TodoItem::new(i, format!("Task #{i:03}")))
            .collect::<Vec<_>>();

        Self {
            next_id: Property::new(200),
            new_text: Property::new(String::new()),
            todos: Property::new(todos),
        }
    }

    fn add_one(&self) {
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));

        let text = self.new_text.get();
        let text = if text.trim().is_empty() {
            format!("Task #{id:03}")
        } else {
            text
        };

        self.todos.update(|v| v.push(TodoItem::new(id, text)));
        self.new_text.set(String::new());
    }

    fn add_many(&self, n: usize) {
        if n == 0 {
            return;
        }

        self.todos.update(|v| {
            for _ in 0..n {
                let id = self.next_id.get();
                self.next_id.set(id.saturating_add(1));
                v.push(TodoItem::new(id, format!("Task #{id:03}")));
            }
        });
    }

    fn rotate(&self) {
        self.todos.update(|v| {
            if v.len() <= 1 {
                return;
            }
            let first = v.remove(0);
            v.push(first);
        });
    }

    fn reverse(&self) {
        self.todos.update(|v| v.reverse());
    }

    fn clear_completed(&self) {
        self.todos.update(|v| v.retain(|t| !t.done.get()));
    }

    fn clear_all(&self) {
        self.todos.set(Vec::new());
    }
}

struct ControlsView {
    model: TodoModel,
}

impl ControlsView {
    fn new(model: TodoModel) -> Self {
        Self { model }
    }
}

impl DeclarativeView for ControlsView {
    fn body(&self) -> Box<dyn DeclarativeView> {
        let model = self.model.clone();

        let stats = {
            let todos = model.todos.clone();
            TextFn::new(move || {
                let list = todos.get();
                let total = list.len();
                let done = list.iter().filter(|t| t.done.get()).count();
                format!("Stats: total={total}  done={done}  (uses stable ids: .with_id())")
            })
        };

        let add_row = HStack::new()
            .spacing(1)
            .child_with_layout(
                TextBox::new("New task", model.new_text.binding()),
                LayoutParams {
                    width: Size::Weight(1),
                    ..LayoutParams::default()
                },
            )
            .child_with_layout(
                Button::new("Add").on_click({
                    let model = model.clone();
                    move || model.add_one()
                }),
                LayoutParams {
                    width: Size::Fixed(9),
                    ..LayoutParams::default()
                },
            )
            .child_with_layout(
                Button::new("Add +50").on_click({
                    let model = model.clone();
                    move || model.add_many(50)
                }),
                LayoutParams {
                    width: Size::Fixed(9),
                    ..LayoutParams::default()
                },
            );

        let ops_row = {
            let model_rotate = model.clone();
            let model_reverse = model.clone();
            let model_clear_done = model.clone();
            let model_clear_all = model.clone();

            HStack::new()
                .spacing(1)
                .child(Button::new("Rotate").on_click(move || model_rotate.rotate()))
                .child(Button::new("Reverse").on_click(move || model_reverse.reverse()))
                .child(Button::new("Clear done").on_click(move || {
                    model_clear_done.clear_completed();
                }))
                .child(Button::new("Clear all").on_click(move || model_clear_all.clear_all()))
                .child(Spacer::new())
        };

        Box::new(
            VStack::new()
                .spacing(1)
                .padding_insets(EdgeInsets::all(1))
                .scrollable(true)
                .child_with_layout(Text::new("ForEach Demo (Controls)"), content_height())
                .child_with_layout(
                    Text::new(
                        "Keyboard: Tab/Shift+Tab to move focus, Enter/Space to activate. Mouse works too.",
                    ),
                    content_height(),
                )
                .child_with_layout(
                    Text::new(
                        "Quit: Ctrl+Q always; 'q' only when the focused widget did not consume it (e.g. not typing in a TextBox).",
                    ),
                    content_height(),
                )
                .child_with_layout(Divider::horizontal(), content_height())
                .child_with_layout(add_row, content_height())
                .child_with_layout(ops_row, content_height())
                .child_with_layout(Divider::horizontal(), content_height())
                .child_with_layout(stats, content_height()),
        )
    }
}

struct ListView {
    model: TodoModel,
}

impl ListView {
    fn new(model: TodoModel) -> Self {
        Self { model }
    }
}

impl DeclarativeView for ListView {
    fn body(&self) -> Box<dyn DeclarativeView> {
        let todos = self.model.todos.clone();

        let list = ForEach::new(todos.binding(), move |todo, _idx| {
            let id = todo.id;
            let todos_for_delete = todos.clone();

            HStack::new()
                .spacing(1)
                .child_with_layout(
                    Checkbox::new("", todo.done.binding()),
                    LayoutParams {
                        width: Size::Fixed(4),
                        height: Size::Content,
                        align_y: Align::Center,
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    TextBox::new(format!("#{id:03}"), todo.text.binding()),
                    LayoutParams {
                        width: Size::Weight(1),
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    Button::new("Del").on_click(move || {
                        todos_for_delete.update(|v| v.retain(|t| t.id != id));
                    }),
                    LayoutParams {
                        width: Size::Fixed(7),
                        ..LayoutParams::default()
                    },
                )
        })
        .spacing(0)
        .padding(1)
        .scrollable(true)
        .with_id();

        Box::new(
            VStack::new()
                .padding_insets(EdgeInsets::all(1))
                .spacing(1)
                .child_with_layout(Text::new("ForEach Demo (List)"), content_height())
                .child_with_layout(
                    Text::new(
                        "Focus a TextBox, move cursor, then click Rotate/Reverse: state stays with the item id.",
                    ),
                    content_height(),
                )
                .child_with_layout(Divider::horizontal(), content_height())
                .child_with_layout(
                    list,
                    LayoutParams {
                        height: Size::Fill,
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

    let model = TodoModel::new();

    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;

    let controls_h = 12u16.min(work.height.saturating_sub(4).max(8));
    let gutter = 1u16;

    let controls_rect = Rect {
        x: work.x.saturating_add(2),
        y: work.y.saturating_add(1),
        width: work.width.saturating_sub(4).max(20),
        height: controls_h,
    };

    let list_rect = Rect {
        x: work.x.saturating_add(2),
        y: controls_rect
            .y
            .saturating_add(controls_rect.height)
            .saturating_add(gutter),
        width: work.width.saturating_sub(4).max(20),
        height: work
            .height
            .saturating_sub(controls_rect.height)
            .saturating_sub(gutter)
            .saturating_sub(2)
            .max(8),
    };

    let controls_id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "ForEach - Controls",
            controls_rect,
            Box::new(ViewAdapter::new(ControlsView::new(model.clone()))),
        ),
        screen,
    );
    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "ForEach - List (scrollable)",
            list_rect,
            Box::new(ViewAdapter::new(ListView::new(model.clone()))),
        ),
        screen,
    );
    desktop.wm.focus(controls_id);

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
