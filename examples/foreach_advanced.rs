use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;

use atto_ui::app::{Desktop, MenuBar};
use atto_ui::declarative::{
    DeclarativeView, Divider, ForEach, Identifiable, LayoutParams, Size, Text, VStack,
};
use atto_ui::reactive::{EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::view::View;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_macros::view_builder;

/// 待办事项数据结构（带独立状态）
#[derive(Clone)]
struct TodoItem {
    id: usize,
    text: String,
    completed: Property<bool>, // 每个项目有自己的状态
}

impl TodoItem {
    fn new(id: usize, text: impl Into<String>) -> Self {
        Self {
            id,
            text: text.into(),
            completed: Property::new(false),
        }
    }
}

// 手动实现 PartialEq，忽略 Property 字段
impl PartialEq for TodoItem {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.text == other.text
    }
}

// 实现 Identifiable trait 以支持增量更新优化
impl Identifiable for TodoItem {
    type Id = usize;

    fn id(&self) -> Self::Id {
        self.id
    }
}

/// 用户数据结构
#[derive(Clone, PartialEq)]
struct User {
    id: usize,
    name: String,
    email: String,
}

impl User {
    fn new(id: usize, name: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            email: email.into(),
        }
    }
}

// 实现 Identifiable trait 以支持增量更新优化
impl Identifiable for User {
    type Id = usize;

    fn id(&self) -> Self::Id {
        self.id
    }
}

/// 构建 TodoList 窗口（展示状态绑定）
fn build_todo_window(todos: Property<Vec<TodoItem>>) -> Box<dyn View> {
    let header = view_builder! {
        VStack {
            Text("Todo List - State Binding Demo")
            Text("Press 't' to add task, 'x' to remove first")
        }
        .spacing(0)
    };

    let todo_list = ForEach::new(todos.binding(), |todo, _idx| {
        view_builder! {
            HStack {
                Checkbox("", todo.completed.binding())
                Text(&todo.text)
            }
            .spacing(1)
        }
    })
    .spacing(0)
    .with_id(); // 启用基于 ID 的增量更新优化

    VStack::new()
        .padding(1)
        .spacing(0)
        .child_with_layout(
            header,
            LayoutParams {
                height: Size::Content,
                ..Default::default()
            },
        )
        .child_with_layout(
            Divider::horizontal(),
            LayoutParams {
                height: Size::Content,
                ..Default::default()
            },
        )
        .child(todo_list)
        .build_view()
}

/// 构建用户列表窗口（展示回调模式）
fn build_user_window(users: Property<Vec<User>>, click_log: Property<String>) -> Box<dyn View> {
    let log_for_foreach = click_log.clone();
    let log_for_text = click_log.clone();

    let header = view_builder! {
        VStack {
            Text("User List - Callback Demo")
            Text("Click buttons to select user")
        }
        .spacing(0)
    };

    let user_list = ForEach::new(users.binding(), move |user, _idx| {
        let log = log_for_foreach.clone();
        let user_id = user.id;
        let user_name = user.name.clone();
        let user_email = user.email.clone();

        view_builder! {
            HStack {
                Button(format!("#{id}", id = user.id))
                    .on_click(move || {
                        log.set(format!(
                            "Selected: {} ({}) - ID: {}",
                            user_name, user_email, user_id
                        ));
                    })
                Text(&user.name)
                Text(format!("<{}>", user.email))
            }
            .spacing(1)
        }
    })
    .spacing(0)
    .with_id(); // 启用基于 ID 的增量更新优化

    let footer = view_builder! {
        TextFn(move || log_for_text.get())
    };

    VStack::new()
        .padding(1)
        .spacing(0)
        .child_with_layout(
            header,
            LayoutParams {
                height: Size::Content,
                ..Default::default()
            },
        )
        .child_with_layout(
            Divider::horizontal(),
            LayoutParams {
                height: Size::Content,
                ..Default::default()
            },
        )
        .child(user_list)
        .child_with_layout(
            Divider::horizontal(),
            LayoutParams {
                height: Size::Content,
                ..Default::default()
            },
        )
        .child_with_layout(
            footer,
            LayoutParams {
                height: Size::Content,
                ..Default::default()
            },
        )
        .build_view()
}

/// 构建统计信息窗口（展示反应式计算）
fn build_stats_window(todos: Property<Vec<TodoItem>>) -> Box<dyn View> {
    let stats = view_builder! {
        VStack {
            Text("Statistics - Reactive Computed")
        }
        .spacing(0)
    };

    let total_text = Text::from_fn({
        let todos = todos.clone();
        move || {
            let items = todos.get();
            format!("Total tasks: {}", items.len())
        }
    });

    let completed_text = Text::from_fn({
        let todos = todos.clone();
        move || {
            let items = todos.get();
            let completed = items.iter().filter(|t| t.completed.get()).count();
            format!("Completed: {}", completed)
        }
    });

    let pending_text = Text::from_fn({
        let todos = todos.clone();
        move || {
            let items = todos.get();
            let pending = items.iter().filter(|t| !t.completed.get()).count();
            format!("Pending: {}", pending)
        }
    });

    let progress_text = Text::from_fn({
        move || {
            let items = todos.get();
            if items.is_empty() {
                "Progress: N/A".to_string()
            } else {
                let completed = items.iter().filter(|t| t.completed.get()).count();
                let percentage = (completed as f64 / items.len() as f64 * 100.0) as usize;
                format!("Progress: {}%", percentage)
            }
        }
    });

    VStack::new()
        .padding(1)
        .spacing(0)
        .child_with_layout(
            stats,
            LayoutParams {
                height: Size::Content,
                ..Default::default()
            },
        )
        .child_with_layout(
            Divider::horizontal(),
            LayoutParams {
                height: Size::Content,
                ..Default::default()
            },
        )
        .child_with_layout(
            total_text,
            LayoutParams {
                height: Size::Content,
                ..Default::default()
            },
        )
        .child_with_layout(
            completed_text,
            LayoutParams {
                height: Size::Content,
                ..Default::default()
            },
        )
        .child_with_layout(
            pending_text,
            LayoutParams {
                height: Size::Content,
                ..Default::default()
            },
        )
        .child_with_layout(
            progress_text,
            LayoutParams {
                height: Size::Content,
                ..Default::default()
            },
        )
        .build_view()
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        event::EnableMouseCapture,
        cursor::Show
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // 创建数据源
    let todos = Property::new(vec![
        TodoItem::new(1, "Buy groceries"),
        TodoItem::new(2, "Write documentation"),
        TodoItem::new(3, "Review pull requests"),
        TodoItem::new(4, "Fix bug #123"),
    ]);

    let users = Property::new(vec![
        User::new(1, "Alice Johnson", "alice@example.com"),
        User::new(2, "Bob Smith", "bob@example.com"),
        User::new(3, "Charlie Brown", "charlie@example.com"),
        User::new(4, "Diana Prince", "diana@example.com"),
    ]);

    let click_log = Property::new("No user selected yet".to_string());

    let actions: EventQueue<()> = EventQueue::new();
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;

    // 创建三个窗口
    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Todos",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(1),
                width: 35.min(work.width.saturating_sub(4)),
                height: 14.min(work.height.saturating_sub(2)),
            },
            build_todo_window(todos.clone()),
        ),
        screen,
    );

    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Users",
            Rect {
                x: work.x.saturating_add(39),
                y: work.y.saturating_add(1),
                width: 38.min(work.width.saturating_sub(41)),
                height: 14.min(work.height.saturating_sub(2)),
            },
            build_user_window(users.clone(), click_log.clone()),
        ),
        screen,
    );

    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Stats",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(16),
                width: 35.min(work.width.saturating_sub(4)),
                height: 9.min(work.height.saturating_sub(17)),
            },
            build_stats_window(todos.clone()),
        ),
        screen,
    );

    let mut next_todo_id = 5;

    loop {
        terminal.draw(|f| desktop.draw(f))?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        let screen: Rect = terminal.size()?.into();
        let _res = desktop.handle_event(&ev, screen);

        match ev {
            Event::Key(KeyEvent {
                code: KeyCode::Char('q'),
                kind: KeyEventKind::Press,
                ..
            }) => {
                break;
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('t'),
                kind: KeyEventKind::Press,
                ..
            }) => {
                // 添加新任务
                let mut current_todos = todos.get();
                current_todos.push(TodoItem::new(
                    next_todo_id,
                    format!("Task {}", next_todo_id),
                ));
                next_todo_id += 1;
                todos.set(current_todos);
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('x'),
                kind: KeyEventKind::Press,
                ..
            }) => {
                // 删除第一个任务
                let mut current_todos = todos.get();
                if !current_todos.is_empty() {
                    current_todos.remove(0);
                    todos.set(current_todos);
                }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                kind: KeyEventKind::Press,
                ..
            }) => {
                // 清空所有任务
                todos.set(Vec::new());
                click_log.set("No user selected yet".to_string());
            }
            _ => {}
        }

        if actions.pop().is_some() {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}
