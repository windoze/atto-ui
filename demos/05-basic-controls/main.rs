// Demo: 05-basic-controls
// 演示所有基础控件的使用方法

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

use chatty::app::{Desktop, MenuBar};
use chatty::declarative::{
    DeclarativeView, Divider, EdgeInsets, HStack, LayoutParams, Size, Text, TextFn, VStack,
    ViewAdapter,
};
use chatty::reactive::Property;
use chatty::theme::Theme;
use chatty::view::EventOutcome;
use chatty::widgets::{Button, Checkbox, ListBox, RadioGroup, TableView, TextBox};
use chatty::wm::{Window, WindowKind};

/// 应用状态模型
#[derive(Clone)]
struct AppState {
    // Button 相关
    click_count: Property<u32>,

    // TextBox 相关
    username: Property<String>,
    password: Property<String>,

    // Checkbox 相关
    accept_terms: Property<bool>,
    enable_notifications: Property<bool>,
    remember_me: Property<bool>,

    // RadioGroup 相关
    theme_selection: Property<usize>,
    language_selection: Property<usize>,

    // ListBox 相关
    fruit_selection: Property<usize>,

    // TableView 相关
    table_selection: Property<usize>,

    // 状态信息
    status_message: Property<String>,
}

impl AppState {
    fn new() -> Self {
        Self {
            click_count: Property::new(0),
            username: Property::new(String::new()),
            password: Property::new(String::new()),
            accept_terms: Property::new(false),
            enable_notifications: Property::new(true),
            remember_me: Property::new(false),
            theme_selection: Property::new(0),
            language_selection: Property::new(0),
            fruit_selection: Property::new(0),
            table_selection: Property::new(0),
            status_message: Property::new("欢迎使用基础控件演示！".to_string()),
        }
    }
}

/// 左侧窗口：Button、TextBox、Checkbox、RadioGroup 演示
struct LeftPanelView {
    state: AppState,
}

impl LeftPanelView {
    fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl DeclarativeView for LeftPanelView {
    fn body(&self) -> Box<dyn DeclarativeView> {
        let state = self.state.clone();
        let click_count = state.click_count.clone();
        let status_message = state.status_message.clone();

        // 标题和说明
        let header = VStack::new()
            .spacing(0)
            .child(Text::new("基础控件演示 - 左侧面板"))
            .child(Text::new("使用 Tab/Shift+Tab 切换焦点"))
            .child(Divider::horizontal());

        // Button 演示区域
        let click_count_display = state.click_count.clone();
        let state_clone1 = state.clone();
        let state_clone2 = state.clone();
        let button_section = VStack::new()
            .spacing(1)
            .child(Text::new("【按钮演示】"))
            .child(
                HStack::new()
                    .spacing(1)
                    .child(Button::new("计数 +1").on_click(move || {
                        click_count.update(|c| *c = c.saturating_add(1));
                    }))
                    .child(Button::new("重置计数").on_click(move || {
                        state_clone1.click_count.set(0);
                        state_clone1.status_message.set("计数已重置".to_string());
                    }))
                    .child(Button::new("显示消息").on_click(move || {
                        status_message.set("按钮被点击了！".to_string());
                    })),
            )
            .child(TextFn::new(move || {
                format!("点击次数: {}", click_count_display.get())
            }));

        // TextBox 演示区域
        let textbox_section = VStack::new()
            .spacing(1)
            .child(Text::new("【文本输入框演示】"))
            .child(TextBox::new("用户名", state_clone2.username.binding()))
            .child(TextBox::new("密码", state.password.binding()))
            .child(Text::new("支持: Unicode 输入、光标移动、鼠标定位、粘贴"));

        // Checkbox 演示区域
        let checkbox_section = VStack::new()
            .spacing(1)
            .child(Text::new("【复选框演示】"))
            .child(Checkbox::new("接受服务条款", state.accept_terms.binding()))
            .child(Checkbox::new(
                "启用通知",
                state.enable_notifications.binding(),
            ))
            .child(Checkbox::new("记住我", state.remember_me.binding()));

        // RadioGroup 演示区域
        let radio_section = VStack::new()
            .spacing(1)
            .child(Text::new("【单选按钮演示】"))
            .child(RadioGroup::new(
                "主题",
                vec!["深色".into(), "浅色".into(), "自动".into()],
                state.theme_selection.binding(),
            ))
            .child(RadioGroup::new(
                "语言",
                vec!["中文".into(), "English".into(), "日本語".into()],
                state.language_selection.binding(),
            ));

        // 组合所有区域
        Box::new(
            VStack::new()
                .child_with_layout(
                    header,
                    LayoutParams {
                        height: Size::Fixed(3),
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    button_section,
                    LayoutParams {
                        height: Size::Fixed(6),
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    textbox_section,
                    LayoutParams {
                        height: Size::Fixed(7),
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    checkbox_section,
                    LayoutParams {
                        height: Size::Fixed(5),
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    radio_section,
                    LayoutParams {
                        height: Size::Fill,
                        ..LayoutParams::default()
                    },
                )
                .spacing(1)
                .padding(1),
        )
    }
}

/// 右侧窗口：ListBox、TableView、Label 演示
struct RightPanelView {
    state: AppState,
}

impl RightPanelView {
    fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl DeclarativeView for RightPanelView {
    fn body(&self) -> Box<dyn DeclarativeView> {
        let state = self.state.clone();

        // 标题
        let header = VStack::new()
            .spacing(0)
            .child(Text::new("基础控件演示 - 右侧面板"))
            .child(Divider::horizontal());

        // ListBox 演示
        let list_section = VStack::new()
            .spacing(1)
            .child(Text::new("【列表框演示】"))
            .child(
                ListBox::new(
                    "选择水果",
                    vec![
                        "苹果 🍎".into(),
                        "香蕉 🍌".into(),
                        "橙子 🍊".into(),
                        "葡萄 🍇".into(),
                        "西瓜 🍉".into(),
                        "草莓 🍓".into(),
                    ],
                    state.fruit_selection.binding(),
                )
                .height(8u16),
            );

        // TableView 演示
        let table_section = VStack::new()
            .spacing(1)
            .child(Text::new("【表格视图演示】"))
            .child(
                TableView::new(
                    "编程语言对比",
                    vec!["语言".into(), "类型".into(), "特性".into()],
                    vec![
                        vec!["Rust".into(), "系统".into(), "内存安全".into()],
                        vec!["Python".into(), "脚本".into(), "简单易学".into()],
                        vec!["JavaScript".into(), "Web".into(), "无处不在".into()],
                        vec!["Go".into(), "系统".into(), "并发优秀".into()],
                        vec!["C++".into(), "系统".into(), "性能极致".into()],
                    ],
                    state.table_selection.binding(),
                )
                .height(9u16),
            );

        // Label 和提示信息
        let info_section = VStack::new()
            .spacing(0)
            .child(Text::new("【提示信息】"))
            .child(Text::new("↑↓ - 在列表/表格中导航"))
            .child(Text::new("鼠标点击 - 直接选择"))
            .child(Text::new("Enter/Space - 激活按钮"));

        Box::new(
            VStack::new()
                .child_with_layout(
                    header,
                    LayoutParams {
                        height: Size::Fixed(2),
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    list_section,
                    LayoutParams {
                        height: Size::Fixed(11),
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    table_section,
                    LayoutParams {
                        height: Size::Fixed(12),
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    info_section,
                    LayoutParams {
                        height: Size::Fill,
                        ..LayoutParams::default()
                    },
                )
                .spacing(1)
                .padding(1),
        )
    }
}

/// 状态栏窗口：显示当前状态信息
struct StatusView {
    state: AppState,
}

impl StatusView {
    fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl DeclarativeView for StatusView {
    fn body(&self) -> Box<dyn DeclarativeView> {
        let state = self.state.clone();

        Box::new(
            VStack::new()
                .padding_insets(EdgeInsets {
                    left: 1,
                    right: 1,
                    top: 0,
                    bottom: 0,
                })
                .child(TextFn::new(move || {
                    format!(
                        "状态: {} | 用户: {} | 复选: [{}{}{}] | 主题: {} | 语言: {} | 水果: #{} | 表格: #{}",
                        state.status_message.get(),
                        if state.username.get().is_empty() {
                            "<未输入>".to_string()
                        } else {
                            state.username.get()
                        },
                        if state.accept_terms.get() { "✓" } else { " " },
                        if state.enable_notifications.get() { "✓" } else { " " },
                        if state.remember_me.get() { "✓" } else { " " },
                        state.theme_selection.get(),
                        state.language_selection.get(),
                        state.fruit_selection.get(),
                        state.table_selection.get(),
                    )
                })),
        )
    }
}

fn main() -> Result<()> {
    // 初始化终端
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

    // 创建应用状态
    let state = AppState::new();

    // 创建桌面
    let menu = MenuBar::new(vec![]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;

    // 添加左侧窗口
    let left_window = Window::new(
        WindowKind::Normal,
        "控件演示 - 左侧",
        Rect {
            x: work.x.saturating_add(2),
            y: work.y.saturating_add(1),
            width: 45,
            height: work.height.saturating_sub(5),
        },
        Box::new(ViewAdapter::new(LeftPanelView::new(state.clone()))),
    );
    let left_id = desktop.add_window(left_window, screen);

    // 添加右侧窗口
    let right_window = Window::new(
        WindowKind::Normal,
        "控件演示 - 右侧",
        Rect {
            x: work.x.saturating_add(48),
            y: work.y.saturating_add(1),
            width: 45,
            height: work.height.saturating_sub(5),
        },
        Box::new(ViewAdapter::new(RightPanelView::new(state.clone()))),
    );
    desktop.add_window(right_window, screen);

    // 添加状态栏窗口
    let status_window = Window::new(
        WindowKind::Floating,
        "状态栏",
        Rect {
            x: work.x.saturating_add(2),
            y: work.y.saturating_add(work.height).saturating_sub(3),
            width: work.width.saturating_sub(4),
            height: 3,
        },
        Box::new(ViewAdapter::new(StatusView::new(state.clone()))),
    );
    desktop.add_window(status_window, screen);

    // 聚焦左侧窗口
    desktop.wm.focus(left_id);

    // 运行主循环
    run(&mut terminal, &mut desktop)?;

    // 清理终端
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        event::DisableMouseCapture,
        event::DisableBracketedPaste,
    )?;
    terminal.show_cursor()?;

    Ok(())
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

        // 检查退出键
        if result.outcome == EventOutcome::Ignored {
            if let Event::Key(KeyEvent {
                code,
                modifiers,
                kind: KeyEventKind::Press,
                ..
            }) = ev
            {
                match (code, modifiers) {
                    (KeyCode::Char('q'), KeyModifiers::NONE)
                    | (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
