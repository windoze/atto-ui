use std::time::Duration;

use anyhow::Result;
use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, MenuItem, MenuSpec,
    run_crossterm_desktop,
};
use atto_ui::composable::{ComponentContext, EventResult};
use atto_ui::reactive::{Binding, EventQueue};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_macros::{ComponentProperties, component_properties};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// 应用动作枚举 - 定义所有菜单可以触发的动作
#[derive(Clone, Debug)]
enum AppAction {
    // File 菜单
    New,
    Open,
    Save,
    Quit,

    // Edit 菜单
    Copy,
    Paste,
    Preferences,

    // View 菜单
    ZoomIn,
    ZoomOut,
    ThemeDark,
    ThemeLight,
    ThemeHighContrast,
}

/// 状态视图 - 显示应用状态和最后执行的菜单项
#[derive(Clone, ComponentProperties)]
struct StatusView {
    zoom_level: Binding<f32>,
    theme: Binding<String>,
    last_action: Binding<String>,
    action_history: Binding<Vec<String>>,
}

impl StatusView {
    fn new() -> Self {
        Self {
            zoom_level: Binding::new(100.0),
            theme: Binding::new("Dark".to_string()),
            last_action: Binding::new("Welcome! Press F10 to activate menu".to_string()),
            action_history: Binding::new(Vec::new()),
        }
    }

    fn handle_action(&self, action: AppAction) {
        let action_str = match &action {
            AppAction::New => {
                self.action_history
                    .update(|h| h.push("Created new file".to_string()));
                "File → New"
            }
            AppAction::Open => {
                self.action_history
                    .update(|h| h.push("Opened file".to_string()));
                "File → Open"
            }
            AppAction::Save => {
                self.action_history
                    .update(|h| h.push("Saved file".to_string()));
                "File → Save"
            }
            AppAction::Quit => {
                self.action_history
                    .update(|h| h.push("Quit requested".to_string()));
                "File → Quit"
            }
            AppAction::Copy => {
                self.action_history
                    .update(|h| h.push("Copied text".to_string()));
                "Edit → Copy"
            }
            AppAction::Paste => {
                self.action_history
                    .update(|h| h.push("Pasted text".to_string()));
                "Edit → Paste"
            }
            AppAction::Preferences => {
                self.action_history
                    .update(|h| h.push("Opened preferences".to_string()));
                "Edit → Preferences"
            }
            AppAction::ZoomIn => {
                self.zoom_level.update(|z| *z = (*z + 10.0).min(200.0));
                let zoom = self.zoom_level.get();
                self.action_history
                    .update(|h| h.push(format!("Zoomed in to {}%", zoom)));
                "View → Zoom In"
            }
            AppAction::ZoomOut => {
                self.zoom_level.update(|z| *z = (*z - 10.0).max(50.0));
                let zoom = self.zoom_level.get();
                self.action_history
                    .update(|h| h.push(format!("Zoomed out to {}%", zoom)));
                "View → Zoom Out"
            }
            AppAction::ThemeDark => {
                self.theme.set("Dark".to_string());
                self.action_history
                    .update(|h| h.push("Switched to Dark theme".to_string()));
                "View → Theme → Dark"
            }
            AppAction::ThemeLight => {
                self.theme.set("Light".to_string());
                self.action_history
                    .update(|h| h.push("Switched to Light theme".to_string()));
                "View → Theme → Light"
            }
            AppAction::ThemeHighContrast => {
                self.theme.set("High Contrast".to_string());
                self.action_history
                    .update(|h| h.push("Switched to High Contrast theme".to_string()));
                "View → Theme → High Contrast"
            }
        };

        self.last_action.set(action_str.to_string());

        // 只保留最近 10 条历史
        self.action_history.update(|h| {
            if h.len() > 10 {
                h.remove(0);
            }
        });
    }
}

#[component_properties]
impl ::atto_ui::composable::Component for StatusView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let mut lines: Vec<Line> = Vec::new();

        // 标题
        lines.push(Line::from(vec![Span::styled(
            "Menu System Demo",
            Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::raw("═".repeat(area.width as usize)));
        lines.push(Line::raw(""));

        // 当前状态
        let theme = self.theme.get();
        let zoom = self.zoom_level.get();
        let last_action = self.last_action.get();
        let action_history = self.action_history.get();

        lines.push(Line::from(vec![
            Span::styled("Current Theme: ", Style::default().fg(Color::Gray)),
            Span::styled(
                theme,
                Style::default()
                    .fg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Zoom Level: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}%", zoom as i32),
                Style::default()
                    .fg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::raw(""));

        // 最后执行的动作
        lines.push(Line::from(vec![Span::styled(
            "Last Action: ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(last_action, Style::default().fg(Color::White)),
        ]));
        lines.push(Line::raw(""));

        // 操作历史
        if !action_history.is_empty() {
            lines.push(Line::styled(
                "Action History:",
                Style::default().fg(Color::Yellow),
            ));

            let start_index = if action_history.len() > 8 {
                action_history.len() - 8
            } else {
                0
            };

            for action in &action_history[start_index..] {
                lines.push(Line::from(vec![
                    Span::raw("  • "),
                    Span::styled(action.clone(), Style::default().fg(Color::DarkGray)),
                ]));
            }
            lines.push(Line::raw(""));
        }

        // 使用说明
        lines.push(Line::raw("─".repeat(area.width as usize)));
        lines.push(Line::styled(
            "Instructions:",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::raw("  F10 - Activate menu bar"));
        lines.push(Line::raw("  ↑↓  - Navigate menu items"));
        lines.push(Line::raw("  ←→  - Switch between menus"));
        lines.push(Line::raw("  Enter - Select menu item"));
        lines.push(Line::raw("  Esc - Close menu"));
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "Try these menu shortcuts:",
            Style::default().fg(Color::Cyan),
        ));
        lines.push(Line::raw("  n - New file      o - Open file"));
        lines.push(Line::raw("  s - Save file     + - Zoom in"));
        lines.push(Line::raw("  - - Zoom out      q - Quit"));

        let paragraph = Paragraph::new(lines).style(ctx.theme.window_bg);

        frame.render_widget(paragraph, area);
    }
}

impl ::atto_ui::composable::DragAndDrop for StatusView {}

impl ::atto_ui::composable::Layout for StatusView {}

impl ::atto_ui::composable::Scrollable for StatusView {}

impl ::atto_ui::composable::FocusNav for StatusView {}

impl ::atto_ui::composable::DynamicTree for StatusView {}

impl ::atto_ui::composable::EventHandling for StatusView {
    fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }
}

/// 构建菜单栏
fn build_menu(actions: EventQueue<AppAction>) -> MenuBar {
    MenuBar::new(vec![
        // File 菜单
        MenuSpec::new(
            "File",
            vec![
                MenuItem::action("New", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::New)
                })
                .shortcut("n"),
                MenuItem::action("Open", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::Open)
                })
                .shortcut("o"),
                MenuItem::action("Save", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::Save)
                })
                .shortcut("s"),
                // 分隔符通过空标签实现
                MenuItem::action("─────────────", {
                    || {} // 空操作
                })
                .enabled(false),
                MenuItem::action("Quit", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::Quit)
                })
                .shortcut("q"),
            ],
        ),
        // Edit 菜单
        MenuSpec::new(
            "Edit",
            vec![
                MenuItem::action("Copy", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::Copy)
                })
                .shortcut("c"),
                MenuItem::action("Paste", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::Paste)
                })
                .shortcut("p"),
                // 分隔符
                MenuItem::action("─────────────", || {}).enabled(false),
                MenuItem::action("Preferences", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::Preferences)
                })
                .shortcut("r"),
            ],
        ),
        // View 菜单 (包含子菜单)
        MenuSpec::new(
            "View",
            vec![
                MenuItem::action("Zoom In", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::ZoomIn)
                })
                .shortcut("+"),
                MenuItem::action("Zoom Out", {
                    let actions = actions.clone();
                    move || actions.push(AppAction::ZoomOut)
                })
                .shortcut("-"),
                // 分隔符
                MenuItem::action("─────────────", || {}).enabled(false),
                // Theme 子菜单
                MenuItem::submenu(
                    "Theme",
                    vec![
                        MenuItem::action("Dark", {
                            let actions = actions.clone();
                            move || actions.push(AppAction::ThemeDark)
                        })
                        .shortcut("d"),
                        MenuItem::action("Light", {
                            let actions = actions.clone();
                            move || actions.push(AppAction::ThemeLight)
                        })
                        .shortcut("l"),
                        MenuItem::action("High Contrast", {
                            let actions = actions.clone();
                            move || actions.push(AppAction::ThemeHighContrast)
                        })
                        .shortcut("h"),
                    ],
                ),
            ],
        ),
    ])
}

fn main() -> Result<()> {
    let config = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(true)
        .cursor(CursorMode::Hide);

    let actions: EventQueue<AppAction> = EventQueue::new();
    let menu = build_menu(actions.clone());
    let status_view = StatusView::new();
    let status_view_build = status_view.clone();
    let status_view_tick = status_view.clone();

    run_crossterm_desktop(
        config,
        move |screen| {
            let theme = Theme::dark();
            let mut desktop = Desktop::new(theme, menu);

            let window = Window::new(
                WindowKind::Normal,
                "Menu Creation Demo",
                Rect {
                    x: 5,
                    y: 3,
                    width: 65,
                    height: 28,
                },
                Box::new(status_view_build),
            );
            desktop.add_window(window, screen);

            Ok(desktop)
        },
        move |_desktop, _screen| {
            for action in actions.drain() {
                match action {
                    AppAction::Quit => return Ok(AppControl::Exit),
                    other => status_view_tick.handle_action(other),
                }
            }
            Ok(AppControl::Continue)
        },
        |_desktop, _event, _screen, _result| Ok(AppControl::Continue),
    )
}
