use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};

use atto_ui::app::{
    AppControl, AppHost, CrosstermAppConfig, CursorMode, Desktop, MenuBar, MenuItem, MenuSpec,
    StatusBar,
};
use atto_ui::composable::{
    Component, ComponentContext, EdgeInsets, EventResult, HStack, LayoutParams, Size, VStack,
};
use atto_ui::reactive::{EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::widgets::{Button, Checkbox, Label, ListBox, RadioGroup, TableView, TextBox};
use atto_ui::wm::{Window, WindowKind};

#[derive(Clone, Debug)]
enum SnapshotAppAction {
    Quit,
    OpenAbout,
    SetThemeDark,
    SetThemeLight,
}

#[derive(Clone, Copy, Debug)]
enum StatusFixture {
    Unicode,
    LongCjk,
}

impl StatusFixture {
    fn from_arg(arg: &str) -> Option<Self> {
        match arg {
            "--status-unicode" => Some(Self::Unicode),
            "--status-long-cjk" => Some(Self::LongCjk),
            _ => None,
        }
    }

    fn parts(self) -> (&'static str, &'static str) {
        match self {
            Self::Unicode => ("状态栏", "🦀"),
            Self::LongCjk => ("你好你好你好你好", ""),
        }
    }
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

impl ::atto_ui::composable::Component for LogView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
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

impl ::atto_ui::composable::Layout for LogView {}

impl ::atto_ui::composable::Scrollable for LogView {}

impl ::atto_ui::composable::FocusNav for LogView {}

impl ::atto_ui::composable::DynamicTree for LogView {}

impl ::atto_ui::composable::EventHandling for LogView {
    fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }
}

type TraceLog = Arc<Mutex<Vec<&'static str>>>;

fn is_left_mouse_down(event: &Event) -> bool {
    matches!(event, Event::Mouse(m) if matches!(m.kind, MouseEventKind::Down(MouseButton::Left)))
}

fn push_trace(log: &TraceLog, entry: &'static str) {
    if let Ok(mut events) = log.lock() {
        events.push(entry);
    }
}

struct TraceContainer {
    capture_entry: &'static str,
    bubble_entry: &'static str,
    inner: Box<dyn Component>,
    log: TraceLog,
}

impl TraceContainer {
    fn new(
        capture_entry: &'static str,
        bubble_entry: &'static str,
        inner: Box<dyn Component>,
        log: TraceLog,
    ) -> Self {
        Self {
            capture_entry,
            bubble_entry,
            inner,
            log,
        }
    }
}

impl ::atto_ui::composable::Component for TraceContainer {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.inner.draw(frame, area, ctx);
    }
}

impl ::atto_ui::composable::Layout for TraceContainer {
    fn min_width(&self) -> u16 {
        self.inner.min_width()
    }

    fn min_height(&self) -> u16 {
        self.inner.min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.inner.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.inner.desired_height()
    }
}

impl ::atto_ui::composable::Scrollable for TraceContainer {}

impl ::atto_ui::composable::FocusNav for TraceContainer {}

impl ::atto_ui::composable::DynamicTree for TraceContainer {}

impl ::atto_ui::composable::EventHandling for TraceContainer {
    fn handle_event_capture(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if is_left_mouse_down(event) {
            push_trace(&self.log, self.capture_entry);
        }
        EventResult::ignored()
    }

    fn handle_event_bubble(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if is_left_mouse_down(event) {
            push_trace(&self.log, self.bubble_entry);
        }
        EventResult::ignored()
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let capture = self.handle_event_capture(event, ctx);
        if capture.is_consumed() {
            return capture;
        }

        let target = self.inner.handle_event(event, ctx);
        if target.is_consumed() {
            return target;
        }

        self.handle_event_bubble(event, ctx)
    }
}

struct TraceTarget {
    log: TraceLog,
}

impl TraceTarget {
    fn new(log: TraceLog) -> Self {
        Self { log }
    }
}

impl ::atto_ui::composable::Component for TraceTarget {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        frame.render_widget(
            Paragraph::new("Target leaf").style(ctx.theme.widget.normal),
            area,
        );
    }
}

impl ::atto_ui::composable::Layout for TraceTarget {
    fn min_width(&self) -> u16 {
        11
    }

    fn min_height(&self) -> u16 {
        1
    }
}

impl ::atto_ui::composable::Scrollable for TraceTarget {}

impl ::atto_ui::composable::FocusNav for TraceTarget {}

impl ::atto_ui::composable::DynamicTree for TraceTarget {}

impl ::atto_ui::composable::EventHandling for TraceTarget {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if is_left_mouse_down(event) {
            push_trace(&self.log, "target-handle");
        }
        EventResult::ignored()
    }
}

struct TraceLogView {
    log: TraceLog,
}

impl TraceLogView {
    fn new(log: TraceLog) -> Self {
        Self { log }
    }
}

impl ::atto_ui::composable::Component for TraceLogView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let trace = match self.log.lock() {
            Ok(events) if events.is_empty() => "TRACE: none".to_string(),
            Ok(events) => format!("TRACE: {}", events.join(">")),
            Err(_) => "TRACE: <poisoned>".to_string(),
        };
        frame.render_widget(Paragraph::new(trace).style(ctx.theme.widget.normal), area);
    }
}

impl ::atto_ui::composable::Layout for TraceLogView {
    fn min_width(&self) -> u16 {
        72
    }

    fn min_height(&self) -> u16 {
        1
    }
}

impl ::atto_ui::composable::Scrollable for TraceLogView {}

impl ::atto_ui::composable::FocusNav for TraceLogView {}

impl ::atto_ui::composable::DynamicTree for TraceLogView {}

impl ::atto_ui::composable::EventHandling for TraceLogView {}

fn event_order_demo() -> Box<dyn Component> {
    let log = Arc::new(Mutex::new(Vec::new()));
    let row_layout = LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    };

    let child = TraceContainer::new(
        "child-capture",
        "child-bubble",
        Box::new(HStack::new().child_with_layout(
            TraceTarget::new(Arc::clone(&log)),
            LayoutParams {
                width: Size::Fixed(14),
                height: Size::Content,
                ..LayoutParams::default()
            },
        )),
        Arc::clone(&log),
    );
    let root = VStack::new()
        .spacing(0)
        .child_with_layout(Label::new("Event order fixture"), row_layout)
        .child_with_layout(child, row_layout)
        .child_with_layout(TraceLogView::new(Arc::clone(&log)), row_layout);

    Box::new(TraceContainer::new(
        "root-capture",
        "root-bubble",
        Box::new(root),
        log,
    ))
}

struct WidgetsView {
    root: VStack,
}

impl WidgetsView {
    fn new(initial_text: impl Into<String>) -> Self {
        let text = Property::new(initial_text.into());
        let enable_feature = Property::new(true);
        let mode = Property::new(0usize);
        let list_selection = Property::new(0usize);
        let table_selection = Property::new(0usize);

        let row_layout = LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        };

        let root = VStack::new()
            .spacing(0)
            .child_with_layout(
                Label::new("Paste Unicode into the textbox (bracketed paste)."),
                row_layout,
            )
            .child_with_layout(TextBox::new("Text", text.binding()), row_layout)
            .child_with_layout(
                Checkbox::new("Enable feature", enable_feature.binding()),
                row_layout,
            )
            .child_with_layout(
                RadioGroup::new(
                    "Mode",
                    vec!["Normal".into(), "Insert".into(), "Visual".into()],
                    mode.binding(),
                ),
                row_layout,
            )
            .child_with_layout(
                ListBox::new(
                    "List",
                    vec![
                        "**Alpha**".into(),
                        "*Beta*".into(),
                        "__Gamma__".into(),
                        "Delta [link](https://example.com)".into(),
                    ],
                    list_selection.binding(),
                )
                .height(5u16),
                row_layout,
            )
            .child_with_layout(
                TableView::new(
                    "Table",
                    vec!["Key".into(), "Value (__styled__)".into()],
                    vec![
                        vec!["lang".into(), "**Rust**".into()],
                        vec!["hello".into(), "こんにちは *world*".into()],
                        vec!["wide".into(), "你好👋 [link](https://example.com)".into()],
                    ],
                    table_selection.binding(),
                )
                .height(6u16),
                row_layout,
            )
            .child_with_layout(Button::new("OK"), row_layout);

        Self { root }
    }
}

impl ::atto_ui::composable::Component for WidgetsView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.root.draw(frame, area, ctx);
    }
}

impl ::atto_ui::composable::Layout for WidgetsView {
    fn min_width(&self) -> u16 {
        self.root.min_width()
    }

    fn min_height(&self) -> u16 {
        self.root.min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.root.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.root.desired_height()
    }
}

impl ::atto_ui::composable::Scrollable for WidgetsView {}

impl ::atto_ui::composable::FocusNav for WidgetsView {}

impl ::atto_ui::composable::DynamicTree for WidgetsView {
    fn children(&self) -> &[atto_ui::composable::ComponentNode] {
        self.root.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<atto_ui::composable::ComponentNode>> {
        self.root.children_mut()
    }
}

impl ::atto_ui::composable::EventHandling for WidgetsView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.root.handle_event(event, ctx)
    }
}

fn view_hierarchy_demo() -> Box<dyn Component> {
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

    Box::new(
        VStack::new()
            .padding_insets(EdgeInsets::all(1))
            .spacing(1)
            .child_with_layout(
                Label::new("Component hierarchy demo (VStack + HStack)"),
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
            ),
    )
}

#[derive(Default)]
struct AboutView;

impl ::atto_ui::composable::Component for AboutView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        frame.render_widget(
            Paragraph::new(Line::styled(
                "About modal (Esc to close).",
                ctx.theme.widget.normal,
            )),
            area,
        );
    }
}

impl ::atto_ui::composable::Layout for AboutView {}

impl ::atto_ui::composable::Scrollable for AboutView {}

impl ::atto_ui::composable::FocusNav for AboutView {}

impl ::atto_ui::composable::DynamicTree for AboutView {}

impl ::atto_ui::composable::EventHandling for AboutView {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc | KeyCode::Enter,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            return EventResult::close_window();
        }
        EventResult::ignored()
    }
}

fn run_input_api_fixture() -> Result<()> {
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

    let res = input_api_loop(&mut terminal);

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

fn input_api_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut last_event = "none".to_string();
    loop {
        terminal.draw(|f| {
            let area = f.area();
            f.render_widget(
                Paragraph::new(vec![
                    Line::from("Input API fixture"),
                    Line::from(format!("size: {}x{}", area.width, area.height)),
                    Line::from(format!("last: {last_event}")),
                    Line::from("Ctrl+Q to quit"),
                ]),
                area,
            );
        })?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }

        let ev = event::read()?;
        if is_quit_event(&ev) {
            break;
        }
        last_event = describe_input_event(&ev);
    }
    Ok(())
}

fn is_quit_event(ev: &Event) -> bool {
    matches!(
        ev,
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL)
    ) || matches!(
        ev,
        Event::Key(KeyEvent {
            code: KeyCode::Char('\u{11}'),
            kind: KeyEventKind::Press,
            ..
        })
    )
}

fn describe_input_event(ev: &Event) -> String {
    match ev {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) => format!(
            "key:{} kind:{} mods={}",
            describe_key_code(*code),
            describe_key_kind(*kind),
            describe_modifiers(*modifiers)
        ),
        Event::Mouse(m) => format!(
            "mouse:{}@{},{} mods={}",
            describe_mouse_kind(m.kind),
            m.column,
            m.row,
            describe_modifiers(m.modifiers)
        ),
        Event::Resize(cols, rows) => format!("resize:{cols}x{rows}"),
        Event::Paste(text) => format!("paste:{text}"),
        Event::FocusGained => "focus:gained".to_string(),
        Event::FocusLost => "focus:lost".to_string(),
    }
}

fn describe_key_code(code: KeyCode) -> String {
    match code {
        KeyCode::Char(c) => format!("Char({c:?})"),
        KeyCode::F(n) => format!("F({n})"),
        other => format!("{other:?}"),
    }
}

fn describe_key_kind(kind: KeyEventKind) -> &'static str {
    match kind {
        KeyEventKind::Press => "press",
        KeyEventKind::Repeat => "repeat",
        KeyEventKind::Release => "release",
    }
}

fn describe_mouse_kind(kind: MouseEventKind) -> &'static str {
    match kind {
        MouseEventKind::Down(MouseButton::Left) => "down-left",
        MouseEventKind::Down(MouseButton::Middle) => "down-middle",
        MouseEventKind::Down(MouseButton::Right) => "down-right",
        MouseEventKind::Up(MouseButton::Left) => "up-left",
        MouseEventKind::Up(MouseButton::Middle) => "up-middle",
        MouseEventKind::Up(MouseButton::Right) => "up-right",
        MouseEventKind::Drag(MouseButton::Left) => "drag-left",
        MouseEventKind::Drag(MouseButton::Middle) => "drag-middle",
        MouseEventKind::Drag(MouseButton::Right) => "drag-right",
        MouseEventKind::Moved => "moved",
        MouseEventKind::ScrollUp => "scroll-up",
        MouseEventKind::ScrollDown => "scroll-down",
        MouseEventKind::ScrollLeft => "scroll-left",
        MouseEventKind::ScrollRight => "scroll-right",
    }
}

fn describe_modifiers(modifiers: KeyModifiers) -> String {
    let mut parts = Vec::new();
    for (modifier, label) in [
        (KeyModifiers::SHIFT, "SHIFT"),
        (KeyModifiers::ALT, "ALT"),
        (KeyModifiers::CONTROL, "CONTROL"),
        (KeyModifiers::SUPER, "SUPER"),
        (KeyModifiers::HYPER, "HYPER"),
        (KeyModifiers::META, "META"),
    ] {
        if modifiers.contains(modifier) {
            parts.push(label);
        }
    }
    if parts.is_empty() {
        "NONE".to_string()
    } else {
        parts.join("|")
    }
}

fn run_apphost_api_fixture() -> Result<()> {
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_button = Arc::clone(&calls);
    let config = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(true)
        .cursor(CursorMode::Hide);

    let mut host = AppHost::new(config, move |screen| {
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
        desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "AppHost API",
                Rect::new(2, 2, 28, 7),
                Box::new(Button::new("Fire").on_click(move || {
                    calls_for_button.fetch_add(1, Ordering::SeqCst);
                })),
            ),
            screen,
        );
        Ok(desktop)
    })?;

    let Some(window_id) = host
        .list_windows()
        .into_iter()
        .find(|window| window.title == "AppHost API")
        .map(|window| window.id)
    else {
        anyhow::bail!("AppHost API fixture window missing");
    };

    host.step()?;
    host.focus_window(window_id);
    host.send_event(
        window_id,
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 2,
            modifiers: KeyModifiers::NONE,
        }),
    )?;
    host.send_event(
        window_id,
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    )?;
    host.move_window(window_id, 4, 4)?;
    host.resize_window(window_id, 34, 8)?;
    host.set_title(
        window_id,
        format!("AppHost API calls: {}", calls.load(Ordering::SeqCst)),
    );

    loop {
        if host.step()? == AppControl::Exit {
            break;
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--input-api") {
        return run_input_api_fixture();
    }
    if args.iter().any(|arg| arg == "--apphost-api") {
        return run_apphost_api_fixture();
    }

    let event_order_fixture = args.iter().any(|arg| arg == "--event-order");
    let mut status_fixture = args.iter().find_map(|arg| StatusFixture::from_arg(arg));
    let textbox_initial_text = if args.iter().any(|arg| arg == "--textbox-unicode") {
        "a你b好c"
    } else {
        "hello"
    };

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
    if event_order_fixture {
        let event_order_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Event Order",
                Rect {
                    x: 2,
                    y: 2,
                    width: 76,
                    height: 8,
                },
                event_order_demo(),
            ),
            screen,
        );
        desktop.wm.focus(event_order_id);
    } else {
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
                Box::new(WidgetsView::new(textbox_initial_text)),
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
    }

    let res = run(
        &mut terminal,
        &mut desktop,
        &actions,
        &mut is_dark,
        &theme_state,
        &mut status_fixture,
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
    status_fixture: &mut Option<StatusFixture>,
) -> Result<()> {
    loop {
        terminal.draw(|f| {
            desktop.draw(f);
            if let Some(fixture) = *status_fixture {
                let layout = Desktop::layout(f.area());
                let (left, right) = fixture.parts();
                let mut status = StatusBar::default();
                status.set_left(left);
                status.set_right(right);
                status.draw(f, layout.status_bar, &desktop.theme);
            }
        })?;

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

        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = ev
        {
            match code {
                KeyCode::F(3) => {
                    *status_fixture = Some(StatusFixture::Unicode);
                    continue;
                }
                KeyCode::F(4) => {
                    *status_fixture = Some(StatusFixture::LongCjk);
                    continue;
                }
                KeyCode::F(5) => {
                    *status_fixture = None;
                    continue;
                }
                _ => {}
            }
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
        if result.outcome == atto_ui::composable::EventOutcome::Ignored
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
