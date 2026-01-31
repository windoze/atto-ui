use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{cursor, style};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use chatty::app::{Desktop, DesktopAction, MenuBar, MenuItem, MenuSpec};
use chatty::theme::{Theme, ThemeConfig, ThemeConfigFormat};
use chatty::view::{EventOutcome, View, ViewContext, ViewEventResult};
use chatty::views::{
    Align, Anchor, AnchorPlacement, ControlView, EdgeInsets, Grid, HBox, LayoutParams,
    ScrollContent, ScrollContentContext, ScrollView, ScrollViewHost, Size, VBox,
};
use chatty::widgets::{
    Button, Checkbox, ControlOutcome, Form, Label, ListBox, RadioGroup, TableView, TextBox,
};
use chatty::wm::{Window, WindowId, WindowKind, WindowState};

#[derive(Default)]
struct TextView {
    lines: Vec<String>,
}

impl TextView {
    fn new(lines: Vec<String>) -> Self {
        Self { lines }
    }
}

impl View for TextView {
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            return ViewEventResult::close_window();
        }
        ViewEventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        let style = if ctx.is_focused {
            ctx.theme.widget.focused
        } else {
            ctx.theme.widget.normal
        };
        let text = self.lines.to_vec().join("\n");
        frame.render_widget(
            Paragraph::new(text).style(style).wrap(Wrap { trim: false }),
            area,
        );
    }
}

struct DialogView {
    form: Form,
}

impl DialogView {
    fn about() -> Self {
        let controls: Vec<Box<dyn chatty::widgets::Control>> = vec![
            Box::new(Label::new("Chatty demo (Turbo Vision-inspired).")),
            Box::new(Label::new("")),
            Box::new(Label::new("Keys:")),
            Box::new(Label::new(
                "  F10 menu   Ctrl+W window mode   F2 cycle theme",
            )),
            Box::new(Label::new("  n new win  a about/modal        t tooltip")),
            Box::new(Label::new("  d widget states demo (disabled controls)")),
            Box::new(Label::new("  v layout demo (view hierarchy)")),
            Box::new(Label::new("  s scroll demo (viewport + scrollbars)")),
            Box::new(Label::new(
                "  z virtual scroll demo (delegate-driven content)",
            )),
            Box::new(Label::new("")),
            Box::new(Button::new("Close (Enter)")),
        ];
        Self {
            form: Form::new(controls),
        }
    }
}

impl View for DialogView {
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        let (outcome, action) = self.form.handle_event(event);
        if action == chatty::widgets::FormAction::Submitted {
            return ViewEventResult::close_window();
        }
        match outcome {
            ControlOutcome::Consumed => ViewEventResult::consumed(),
            ControlOutcome::Ignored => ViewEventResult::ignored(),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.form.draw(frame, area, ctx.theme, ctx.is_focused);
    }
}

struct WidgetsView {
    form: Form,
    last_msg: String,
}

impl WidgetsView {
    fn new() -> Self {
        let controls: Vec<Box<dyn chatty::widgets::Control>> = vec![
            Box::new(Label::new("Try mouse drag on title bar; click × to close.")),
            Box::new(Label::new("Try bracketed paste into textbox: 你好👋 / 👩‍💻")),
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
                .with_height(4),
            ),
            Box::new(
                TableView::new(
                    "Table",
                    vec!["Key".into(), "Value".into()],
                    vec![
                        vec!["lang".into(), "Rust".into()],
                        vec!["jp".into(), "こんにちは".into()],
                        vec!["cn".into(), "你好👋".into()],
                    ],
                )
                .with_height(4),
            ),
            Box::new(Button::new("OK")),
        ];
        Self {
            form: Form::new(controls),
            last_msg: String::new(),
        }
    }
}

impl View for WidgetsView {
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        if let Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
            && modifiers.contains(KeyModifiers::CONTROL)
        {
            return ViewEventResult::close_window();
        }

        let (outcome, action) = self.form.handle_event(event);
        if action == chatty::widgets::FormAction::Submitted {
            self.last_msg = "Submitted!".to_string();
        }
        match outcome {
            ControlOutcome::Consumed => ViewEventResult::consumed(),
            ControlOutcome::Ignored => ViewEventResult::ignored(),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.form.draw(frame, area, ctx.theme, ctx.is_focused);
        if !self.last_msg.is_empty() && area.height >= 1 {
            let y = area.y + area.height - 1;
            frame.render_widget(
                Paragraph::new(Line::styled(self.last_msg.clone(), ctx.theme.widget.accent)),
                Rect {
                    x: area.x,
                    y,
                    width: area.width,
                    height: 1,
                },
            );
        }
    }
}

struct DisabledWidgetsView {
    form: Form,
}

impl DisabledWidgetsView {
    fn new() -> Self {
        let controls: Vec<Box<dyn chatty::widgets::Control>> = vec![
            Box::new(Label::new(
                "Disabled widgets (not focusable/clickable; Esc closes)",
            )),
            Box::new(
                TextBox::new("Text (disabled)")
                    .with_text("read-only")
                    .with_enabled(false),
            ),
            Box::new(Checkbox::new("Enable feature (disabled)", true).with_enabled(false)),
            Box::new(
                RadioGroup::new(
                    "Mode (disabled)",
                    vec!["Normal".into(), "Insert".into(), "Visual".into()],
                    1,
                )
                .with_enabled(false),
            ),
            Box::new(
                ListBox::new(
                    "List (disabled)",
                    vec![
                        "Alpha".into(),
                        "Beta".into(),
                        "Gamma".into(),
                        "Delta".into(),
                    ],
                )
                .with_height(4)
                .with_enabled(false),
            ),
            Box::new(
                TableView::new(
                    "Table (disabled)",
                    vec!["Key".into(), "Value".into()],
                    vec![
                        vec!["lang".into(), "Rust".into()],
                        vec!["jp".into(), "こんにちは".into()],
                        vec!["cn".into(), "你好👋".into()],
                    ],
                )
                .with_height(4)
                .with_enabled(false),
            ),
            Box::new(Button::new("OK (disabled)").with_enabled(false)),
            Box::new(Label::new(
                "Tip: focus another window to see inactive state.",
            )),
        ];
        Self {
            form: Form::new(controls),
        }
    }
}

impl View for DisabledWidgetsView {
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            return ViewEventResult::close_window();
        }

        let (outcome, _) = self.form.handle_event(event);
        match outcome {
            ControlOutcome::Consumed => ViewEventResult::consumed(),
            ControlOutcome::Ignored => ViewEventResult::ignored(),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.form.draw(frame, area, ctx.theme, ctx.is_focused);
    }
}

#[derive(Default)]
struct TooltipView {
    text: String,
}

impl TooltipView {
    fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl View for TooltipView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        frame.render_widget(
            Paragraph::new(self.text.clone()).style(ctx.theme.widget.normal),
            area,
        );
    }
}

#[derive(Clone, Debug)]
struct IntrinsicLabelView {
    text: String,
}

impl IntrinsicLabelView {
    fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl View for IntrinsicLabelView {
    fn desired_width(&self) -> Option<u16> {
        Some(self.text.len().min(u16::MAX as usize) as u16)
    }

    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let style = ctx.theme.widget.accent;
        frame.render_widget(Paragraph::new(Line::styled(self.text.clone(), style)), area);
    }
}

fn build_layout_demo_view() -> VBox {
    let mut root = VBox::new().with_padding(EdgeInsets::all(1)).with_spacing(1);

    // Anchor demo: this badge sticks to the top-right of the view content area.
    root.add_child_with_layout(
        Box::new(IntrinsicLabelView::new("[ANCHOR]")),
        LayoutParams {
            width: Size::Content,
            height: Size::Content,
            anchor: Some(AnchorPlacement {
                anchor: Anchor::TopRight,
                offset_x: 0,
                offset_y: 0,
            }),
            ..LayoutParams::default()
        },
    );

    // Margin demo: reserve some space on the right so the anchored badge doesn't overlap.
    root.add_child_with_layout(
        Box::new(ControlView::new(Box::new(Label::new(
            "M6 layout demo (resize window)",
        )))),
        LayoutParams {
            height: Size::Content,
            margin: EdgeInsets {
                right: 10,
                ..EdgeInsets::ZERO
            },
            ..LayoutParams::default()
        },
    );

    // HBox demo:
    // - left: content-based sizing (intrinsic width)
    // - middle/right: weighted sizing (1:2 split)
    // - vertical alignment: the badge is centered within the 3-row toolbar
    let mut toolbar = HBox::new().with_spacing(1);
    toolbar.add_child_with_layout(
        Box::new(IntrinsicLabelView::new("Content")),
        LayoutParams {
            width: Size::Content,
            height: Size::Content,
            align_y: Align::Center,
            margin: EdgeInsets {
                left: 1,
                right: 1,
                ..EdgeInsets::ZERO
            },
            ..LayoutParams::default()
        },
    );
    toolbar.add_child_with_layout(
        Box::new(ControlView::new(Box::new(Button::new("W1")))),
        LayoutParams {
            width: Size::Weight(1),
            ..LayoutParams::default()
        },
    );
    toolbar.add_child_with_layout(
        Box::new(ControlView::new(Box::new(Button::new("W2")))),
        LayoutParams {
            width: Size::Weight(2),
            ..LayoutParams::default()
        },
    );

    root.add_child_with_layout(
        Box::new(toolbar),
        LayoutParams {
            height: Size::Fixed(3),
            ..LayoutParams::default()
        },
    );

    // Grid demo:
    // - 2 columns with equal widths
    // - row height is tallest child in the row (Button is 3 rows tall)
    // - checkbox is vertically centered in that tall row
    let mut grid = Grid::new(2).with_row_gap(1).with_column_gap(2);
    grid.add_child_with_layout(
        Box::new(ControlView::new(Box::new(Button::new("Tall")))),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );
    grid.add_child_with_layout(
        Box::new(ControlView::new(Box::new(Checkbox::new("Centered", false)))),
        LayoutParams {
            align_y: Align::Center,
            ..LayoutParams::default()
        },
    );
    grid.add_child(Box::new(ControlView::new(Box::new(Checkbox::new(
        "Row 2A", false,
    )))));
    grid.add_child(Box::new(ControlView::new(Box::new(Checkbox::new(
        "Row 2B", false,
    )))));

    root.add_child_with_layout(
        Box::new(grid),
        LayoutParams {
            height: Size::Fixed(5),
            margin: EdgeInsets {
                left: 2,
                ..EdgeInsets::ZERO
            },
            ..LayoutParams::default()
        },
    );

    root
}

fn build_scroll_demo_view() -> VBox {
    let mut root = VBox::new()
        .with_padding(EdgeInsets::all(1))
        .with_spacing(1)
        .with_scrollable(true);

    root.add_child_with_layout(
        Box::new(IntrinsicLabelView::new(
            "M7/M8 scrolling demo: ↑↓ PgUp/PgDn Home/End, wheel, drag scrollbar thumb",
        )),
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );

    // Horizontal scrolling demo (inside its own scrollable HBox).
    let mut wide_row = HBox::new().with_spacing(1).with_scrollable(true);
    for i in 0..24 {
        wide_row.add_child_with_layout(
            Box::new(IntrinsicLabelView::new(format!("[col-{i:02}]"))),
            LayoutParams {
                width: Size::Content,
                height: Size::Content,
                ..LayoutParams::default()
            },
        );
    }
    root.add_child_with_layout(
        Box::new(wide_row),
        LayoutParams {
            height: Size::Fixed(3),
            ..LayoutParams::default()
        },
    );

    for i in 0..120u16 {
        root.add_child_with_layout(
            Box::new(IntrinsicLabelView::new(format!(
                "{i:03}: The quick brown fox jumps over the lazy dog."
            ))),
            LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            },
        );
    }

    root
}

#[derive(Clone, Debug)]
struct VirtualScrollContentView {
    rows: u16,
    cols: u16,
}

impl VirtualScrollContentView {
    const HEADER: &'static str = "Virtual scrolling demo: wheel/drag/arrow buttons, Esc closes";

    fn new(rows: u16, cols: u16) -> Self {
        Self { rows, cols }
    }

    fn content_height(&self) -> u16 {
        // Row 0 is a header.
        1u16.saturating_add(self.rows)
    }

    fn content_width(&self) -> u16 {
        // Row string format: "0000:" + cols * (" " + "[col-00]") = 5 + cols * 9.
        let row_width = 5u16.saturating_add(self.cols.saturating_mul(9));
        let header_width = Self::HEADER.len().min(u16::MAX as usize) as u16;
        row_width.max(header_width)
    }

    fn line_for_row(&self, row: u16) -> String {
        if row == 0 {
            return Self::HEADER.to_string();
        }

        let idx = row - 1;
        let mut s = format!("{idx:04}:");
        for c in 0..self.cols {
            s.push(' ');
            s.push_str(&format!("[col-{c:02}]"));
        }
        s
    }

    fn draw_line(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        style: Style,
        dy: u16,
        row: Option<u16>,
        scroll_x: u16,
    ) {
        let buf = frame.buffer_mut();
        let y = area.y.saturating_add(dy);

        for dx in 0..area.width {
            buf[(area.x.saturating_add(dx), y)]
                .set_symbol(" ")
                .set_style(style);
        }

        let Some(row) = row else {
            return;
        };

        let line = self.line_for_row(row);
        let start = scroll_x as usize;
        let visible = if start < line.len() {
            &line[start..]
        } else {
            ""
        };
        buf.set_stringn(area.x, y, visible, area.width as usize, style);
    }
}

impl ScrollContent for VirtualScrollContentView {
    fn content_size(
        &mut self,
        _viewport: (u16, u16),
        _ctx: ScrollContentContext<'_>,
    ) -> (u16, u16) {
        (self.content_width(), self.content_height())
    }

    fn handle_event(
        &mut self,
        event: &Event,
        _ctx: ScrollContentContext<'_>,
        _host: &mut ScrollViewHost<'_>,
    ) -> ViewEventResult {
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            return ViewEventResult::close_window();
        }
        ViewEventResult::ignored()
    }

    fn draw(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ScrollContentContext<'_>,
        _host: &mut ScrollViewHost<'_>,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let style = if ctx.view.is_focused {
            ctx.view.theme.widget.focused
        } else {
            ctx.view.theme.widget.normal
        };

        let scroll = ctx.info.scroll_offset;
        let content_h = self.content_height();

        for dy in 0..area.height {
            let row = scroll.y.saturating_add(dy);
            let row = (row < content_h).then_some(row);
            self.draw_line(frame, area, style, dy, row, scroll.x);
        }
    }
}

fn build_virtual_scroll_demo_view() -> ScrollView {
    ScrollView::new(Box::new(VirtualScrollContentView::new(10_000, 40)))
        .with_padding(EdgeInsets::all(1))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DemoTheme {
    Dark,
    DarkUnicode,
    DarkAscii,
    DarkHighContrast,
    Light,
}

impl DemoTheme {
    const fn label(self) -> &'static str {
        match self {
            DemoTheme::Dark => "Dark",
            DemoTheme::DarkUnicode => "Dark + Unicode",
            DemoTheme::DarkAscii => "Dark + ASCII",
            DemoTheme::DarkHighContrast => "Dark + High Contrast",
            DemoTheme::Light => "Light",
        }
    }

    const fn next(self) -> Self {
        match self {
            DemoTheme::Dark => DemoTheme::DarkUnicode,
            DemoTheme::DarkUnicode => DemoTheme::DarkAscii,
            DemoTheme::DarkAscii => DemoTheme::DarkHighContrast,
            DemoTheme::DarkHighContrast => DemoTheme::Light,
            DemoTheme::Light => DemoTheme::Dark,
        }
    }
}

fn apply_demo_theme(desktop: &mut Desktop, theme: DemoTheme) -> Result<()> {
    let (base, overlay) = match theme {
        DemoTheme::Dark => (Theme::dark(), None),
        DemoTheme::Light => (Theme::light(), None),
        DemoTheme::DarkUnicode => (Theme::dark(), Some(THEME_OVERLAY_UNICODE)),
        DemoTheme::DarkAscii => (Theme::dark(), Some(THEME_OVERLAY_ASCII)),
        DemoTheme::DarkHighContrast => (Theme::dark(), Some(THEME_OVERLAY_HIGH_CONTRAST)),
    };

    desktop.theme = base;
    if let Some(overlay) = overlay {
        let cfg = ThemeConfig::from_str(overlay, ThemeConfigFormat::Yaml)?;
        desktop.theme.apply_config_overlay(&cfg)?;
    }

    Ok(())
}

fn update_notes_title(desktop: &mut Desktop, notes_window_id: WindowId, theme: DemoTheme) {
    if let Some(w) = desktop.wm.window_mut(notes_window_id) {
        w.title = format!("Notes (Theme: {})", theme.label());
    }
}

const THEME_OVERLAY_UNICODE: &str = r##"
glyphs:
  checkbox-unchecked: "☐"
  checkbox-checked: "☑"
  radio-unselected: "◯"
  radio-selected: "◉"
  close-button: "✕"
  minimize-button: "−"
"##;

const THEME_OVERLAY_ASCII: &str = r##"
glyphs:
  h-border: "-"
  v-border: "|"
  top-left-corner: "+"
  top-right-corner: "+"
  bottom-left-corner: "+"
  bottom-right-corner: "+"
  active-h-border: "="
  active-v-border: "!"
  active-top-left-corner: "*"
  active-top-right-corner: "*"
  active-bottom-left-corner: "*"
  active-bottom-right-corner: "*"
  scrollbar-track: "."
  scrollbar-thumb: "#"
  close-button: "X"
  minimize-button: "−"
  maximize-button: "O"
"##;

const THEME_OVERLAY_HIGH_CONTRAST: &str = r##"
colors:
  desktop: { bg: "#000000", fg: "#FFFFFF" }
  desktop-dim: { bg: "#000000", fg: "#808080" }

  window-bg: { bg: "#000000", fg: "#FFFFFF" }
  inactive-window-border: { fg: "#808080" }
  active-window-border: { fg: "#FFFF00" }
  inactive-window-title: { fg: "#FFFFFF" }
  active-window-title: { fg: "#FFFF00" }

  menu-bar: { bg: "#000000", fg: "#FFFFFF" }
  menu-bar-active: { bg: "#FFFF00", fg: "#000000" }
  menu-item: { bg: "#000000", fg: "#FFFFFF" }
  menu-item-selected: { bg: "#FFFF00", fg: "#000000" }
  selection: { bg: "#00FFFF", fg: "#000000" }

  widget-normal: { fg: "#FFFFFF" }
  widget-focused: { bg: "#FFFF00", fg: "#000000" }
  widget-disabled: { fg: "#666666" }
  widget-accent: { fg: "#00FFFF" }

  scrollbar-thumb: { fg: "#FFFF00" }
  scrollbar-arrow: { fg: "#FFFF00" }
styles:
  active-window-border: ["bold"]
  widget-focused: ["bold"]
  selection: ["bold"]
"##;

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        event::EnableMouseCapture,
        event::EnableBracketedPaste,
        cursor::Show,
        style::Print("")
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut demo_theme = DemoTheme::Dark;
    let menu = build_menu();
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;

    let widgets_id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Widgets",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(1),
                width: 40,
                height: work.height.saturating_sub(1),
            },
            Box::new(WidgetsView::new()),
        ),
        screen,
    );
    desktop.wm.focus(widgets_id);

    let mut layout_demo_window_id = Some(desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Layout",
            Rect {
                x: work.x.saturating_add(43),
                y: work.y.saturating_add(1),
                width: 36,
                height: 15,
            },
            Box::new(build_layout_demo_view()),
        ),
        screen,
    ));
    let mut scroll_demo_window_id: Option<WindowId> = None;
    let mut virtual_scroll_demo_window_id: Option<WindowId> = None;
    let mut widget_states_demo_window_id: Option<WindowId> = None;

    let log_id = desktop.add_window(
        Window::new(
            WindowKind::Floating,
            format!("Notes (Theme: {})", demo_theme.label()),
            Rect {
                x: work.x.saturating_add(43),
                y: work.y.saturating_add(16),
                width: 36,
                height: 7,
            },
            Box::new(TextView::new(vec![
                "Mouse: click to focus; drag title bar; drag corners to resize".into(),
                "Ctrl+W: window mode (move/resize/min/max/close)".into(),
                "F2: cycle theme (built-in + overlays)".into(),
                "Paste: bracketed paste into textbox".into(),
                "D: widget states demo (disabled)".into(),
                "V: layout demo (view hierarchy)".into(),
                "S/Z: scroll / virtual scroll demos".into(),
            ])),
        ),
        screen,
    );

    let mut tooltip: Option<(WindowId, Instant)> = None;
    let res = run(
        &mut terminal,
        &mut desktop,
        &mut demo_theme,
        log_id,
        &mut layout_demo_window_id,
        &mut scroll_demo_window_id,
        &mut virtual_scroll_demo_window_id,
        &mut widget_states_demo_window_id,
        &mut tooltip,
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

fn build_menu() -> MenuBar {
    MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![
                MenuItem::command("New window", "window.new").shortcut("n"),
                MenuItem::submenu(
                    "Theme",
                    vec![
                        MenuItem::command("Dark", "theme.dark"),
                        MenuItem::command("Dark + Unicode", "theme.dark_unicode"),
                        MenuItem::command("Dark + ASCII", "theme.dark_ascii"),
                        MenuItem::command("Dark + High Contrast", "theme.dark_high_contrast"),
                        MenuItem::command("Light", "theme.light"),
                    ],
                ),
                MenuItem::command("Quit", "app.quit").shortcut("q"),
            ],
        ),
        MenuSpec::new(
            "Window",
            vec![
                MenuItem::command("Next", "window.next").shortcut("F6"),
                MenuItem::command("Widget states demo", "window.states_demo").shortcut("d"),
                MenuItem::command("Layout demo", "window.layout_demo").shortcut("v"),
                MenuItem::command("Scroll demo", "window.scroll_demo").shortcut("s"),
                MenuItem::command("Virtual scroll demo", "window.virtual_scroll_demo")
                    .shortcut("z"),
                MenuItem::command("Minimize", "window.min").shortcut("m"),
                MenuItem::command("Maximize", "window.max").shortcut("x"),
                MenuItem::command("Close", "window.close").shortcut("c"),
            ],
        ),
        MenuSpec::new(
            "Help",
            vec![MenuItem::command("About", "help.about").shortcut("a")],
        ),
    ])
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    desktop: &mut Desktop,
    demo_theme: &mut DemoTheme,
    notes_window_id: WindowId,
    layout_demo_window_id: &mut Option<WindowId>,
    scroll_demo_window_id: &mut Option<WindowId>,
    virtual_scroll_demo_window_id: &mut Option<WindowId>,
    widget_states_demo_window_id: &mut Option<WindowId>,
    tooltip: &mut Option<(WindowId, Instant)>,
) -> Result<()> {
    let mut next_float = 0u32;

    loop {
        // Auto-close tooltip after a short time.
        if let Some((id, until)) = tooltip.take() {
            if Instant::now() < until {
                *tooltip = Some((id, until));
            } else {
                desktop.wm.close(id);
            }
        }

        terminal.draw(|f| desktop.draw(f))?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        let screen: Rect = terminal.size()?.into();
        let result = desktop.handle_event(&ev, screen);

        if let DesktopAction::MenuCommand(cmd) = result.action {
            match cmd.as_str() {
                "app.quit" => break,
                "window.new" => {
                    next_float += 1;
                    let screen: Rect = terminal.size()?.into();
                    let title = format!("Floating {next_float}");
                    desktop.add_window(
                        Window::new(
                            WindowKind::Floating,
                            title,
                            Rect {
                                x: 10 + (next_float as u16 % 15),
                                y: 5 + (next_float as u16 % 6),
                                width: 30,
                                height: 8,
                            },
                            Box::new(TextView::new(vec!["Hello from a new window.".into()])),
                        ),
                        screen,
                    );
                }
                "window.next" => desktop.wm.focus_next(),
                "window.layout_demo" => {
                    open_layout_demo(desktop, screen, layout_demo_window_id)?;
                }
                "window.scroll_demo" => {
                    open_scroll_demo(desktop, screen, scroll_demo_window_id)?;
                }
                "window.virtual_scroll_demo" => {
                    open_virtual_scroll_demo(desktop, screen, virtual_scroll_demo_window_id)?;
                }
                "window.states_demo" => {
                    open_widget_states_demo(desktop, screen, widget_states_demo_window_id)?;
                }
                "window.min" => desktop.wm.minimize_focused(),
                "window.max" => {
                    let screen: Rect = terminal.size()?.into();
                    let work = Desktop::layout(screen).work_area;
                    desktop.wm.toggle_maximize_focused(work);
                }
                "window.close" => {
                    if let Some(id) = desktop.wm.focused() {
                        desktop.wm.close(id);
                    }
                }
                "help.about" => open_about_modal(desktop, screen)?,
                "theme.dark" => {
                    *demo_theme = DemoTheme::Dark;
                    apply_demo_theme(desktop, *demo_theme)?;
                    update_notes_title(desktop, notes_window_id, *demo_theme);
                }
                "theme.light" => {
                    *demo_theme = DemoTheme::Light;
                    apply_demo_theme(desktop, *demo_theme)?;
                    update_notes_title(desktop, notes_window_id, *demo_theme);
                }
                "theme.dark_unicode" => {
                    *demo_theme = DemoTheme::DarkUnicode;
                    apply_demo_theme(desktop, *demo_theme)?;
                    update_notes_title(desktop, notes_window_id, *demo_theme);
                }
                "theme.dark_ascii" => {
                    *demo_theme = DemoTheme::DarkAscii;
                    apply_demo_theme(desktop, *demo_theme)?;
                    update_notes_title(desktop, notes_window_id, *demo_theme);
                }
                "theme.dark_high_contrast" => {
                    *demo_theme = DemoTheme::DarkHighContrast;
                    apply_demo_theme(desktop, *demo_theme)?;
                    update_notes_title(desktop, notes_window_id, *demo_theme);
                }
                _ => {
                    // Unknown commands are ignored.
                }
            }
        }

        // Application-level shortcuts: only run if the event was not handled by the view/window/desktop.
        if result.outcome == EventOutcome::Ignored
            && let Event::Key(KeyEvent {
                code,
                modifiers,
                kind: KeyEventKind::Press,
                ..
            }) = ev
        {
            match (code, modifiers) {
                (KeyCode::Char('q'), KeyModifiers::NONE) => break,
                (KeyCode::F(2), _) => {
                    *demo_theme = demo_theme.next();
                    apply_demo_theme(desktop, *demo_theme)?;
                    update_notes_title(desktop, notes_window_id, *demo_theme);
                }
                (KeyCode::Char('n'), KeyModifiers::NONE) => {
                    next_float += 1;
                    let title = format!("Floating {next_float}");
                    desktop.add_window(
                        Window::new(
                            WindowKind::Floating,
                            title,
                            Rect {
                                x: 8 + (next_float as u16 % 20),
                                y: 4 + (next_float as u16 % 8),
                                width: 30,
                                height: 8,
                            },
                            Box::new(TextView::new(vec![
                                "Floating window".into(),
                                "Press Esc to close.".into(),
                            ])),
                        ),
                        screen,
                    );
                }
                (KeyCode::Char('a'), KeyModifiers::NONE) => {
                    open_about_modal(desktop, screen)?;
                }
                (KeyCode::Char('t'), KeyModifiers::NONE) => {
                    open_tooltip(desktop, screen, tooltip)?;
                }
                (KeyCode::Char('d'), KeyModifiers::NONE) => {
                    open_widget_states_demo(desktop, screen, widget_states_demo_window_id)?;
                }
                (KeyCode::Char('v'), KeyModifiers::NONE) => {
                    open_layout_demo(desktop, screen, layout_demo_window_id)?;
                }
                (KeyCode::Char('s'), KeyModifiers::NONE) => {
                    open_scroll_demo(desktop, screen, scroll_demo_window_id)?;
                }
                (KeyCode::Char('z'), KeyModifiers::NONE) => {
                    open_virtual_scroll_demo(desktop, screen, virtual_scroll_demo_window_id)?;
                }
                _ => {}
            }
        }

        // Keep the Notes window from being minimized as a small demo nicety.
        if let Some(w) = desktop.wm.window_mut(notes_window_id)
            && w.state == WindowState::Minimized
        {
            w.state = WindowState::Normal;
        }
    }
    Ok(())
}

fn open_about_modal(desktop: &mut Desktop, screen: Rect) -> Result<()> {
    let work = Desktop::layout(screen).work_area;
    let w = 46.min(work.width.saturating_sub(2)).max(20);
    let h = 12.min(work.height.saturating_sub(2)).max(7);
    let rect = Rect {
        x: work.x + (work.width.saturating_sub(w)) / 2,
        y: work.y + (work.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };
    desktop.add_window(
        Window::new(
            WindowKind::Modal,
            "About",
            rect,
            Box::new(DialogView::about()),
        ),
        screen,
    );
    Ok(())
}

fn open_layout_demo(
    desktop: &mut Desktop,
    screen: Rect,
    layout_demo_window_id: &mut Option<WindowId>,
) -> Result<()> {
    if let Some(id) = *layout_demo_window_id
        && desktop.wm.window_mut(id).is_some()
    {
        desktop.wm.focus(id);
        desktop.wm.bring_to_front(id);
        return Ok(());
    }

    let work = Desktop::layout(screen).work_area;
    let id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Layout",
            Rect {
                x: work.x.saturating_add(43),
                y: work.y.saturating_add(1),
                width: 36,
                height: 15,
            },
            Box::new(build_layout_demo_view()),
        ),
        screen,
    );
    *layout_demo_window_id = Some(id);
    Ok(())
}

fn open_scroll_demo(
    desktop: &mut Desktop,
    screen: Rect,
    scroll_demo_window_id: &mut Option<WindowId>,
) -> Result<()> {
    if let Some(id) = *scroll_demo_window_id
        && desktop.wm.window_mut(id).is_some()
    {
        desktop.wm.focus(id);
        desktop.wm.bring_to_front(id);
        return Ok(());
    }

    let work = Desktop::layout(screen).work_area;
    let id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Scroll",
            Rect {
                x: work.x.saturating_add(3),
                y: work.y.saturating_add(4),
                width: 44,
                height: 14,
            },
            Box::new(build_scroll_demo_view()),
        ),
        screen,
    );
    *scroll_demo_window_id = Some(id);
    Ok(())
}

fn open_virtual_scroll_demo(
    desktop: &mut Desktop,
    screen: Rect,
    virtual_scroll_demo_window_id: &mut Option<WindowId>,
) -> Result<()> {
    if let Some(id) = *virtual_scroll_demo_window_id
        && desktop.wm.window_mut(id).is_some()
    {
        desktop.wm.focus(id);
        desktop.wm.bring_to_front(id);
        return Ok(());
    }

    let work = Desktop::layout(screen).work_area;
    let w = 56.min(work.width.saturating_sub(2)).max(20);
    let h = 16.min(work.height.saturating_sub(2)).max(8);
    let id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Virtual Scroll",
            Rect {
                x: work.x.saturating_add(6),
                y: work.y.saturating_add(3),
                width: w,
                height: h,
            },
            Box::new(build_virtual_scroll_demo_view()),
        ),
        screen,
    );
    *virtual_scroll_demo_window_id = Some(id);
    Ok(())
}

fn open_widget_states_demo(
    desktop: &mut Desktop,
    screen: Rect,
    widget_states_demo_window_id: &mut Option<WindowId>,
) -> Result<()> {
    if let Some(id) = *widget_states_demo_window_id
        && desktop.wm.window_mut(id).is_some()
    {
        desktop.wm.focus(id);
        desktop.wm.bring_to_front(id);
        return Ok(());
    }

    let work = Desktop::layout(screen).work_area;
    let w = 56.min(work.width.saturating_sub(2)).max(24);
    let h = 18.min(work.height.saturating_sub(2)).max(10);
    let rect = Rect {
        x: work.x + (work.width.saturating_sub(w)) / 2,
        y: work.y + (work.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };

    let id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Widget States",
            rect,
            Box::new(DisabledWidgetsView::new()),
        ),
        screen,
    );
    *widget_states_demo_window_id = Some(id);
    Ok(())
}

fn open_tooltip(
    desktop: &mut Desktop,
    screen: Rect,
    tooltip: &mut Option<(WindowId, Instant)>,
) -> Result<()> {
    if let Some((id, _)) = tooltip.take() {
        desktop.wm.close(id);
    }

    let work = Desktop::layout(screen).work_area;
    let (x, y) = if let Some(focused) = desktop.wm.focused() {
        if let Some(w) = desktop.wm.windows().iter().find(|w| w.id == focused) {
            (w.rect.x.saturating_add(2), w.rect.y.saturating_add(2))
        } else {
            (work.x + 2, work.y + 2)
        }
    } else {
        (work.x + 2, work.y + 2)
    };

    let rect = Rect {
        x,
        y,
        width: 26.min(work.width.saturating_sub(2)),
        height: 4,
    };
    let id = desktop.add_window(
        Window::new(
            WindowKind::Tooltip,
            "Tip",
            rect,
            Box::new(TooltipView::new("Tooltip: press 't' again to close.")),
        ),
        screen,
    );
    if let Some(w) = desktop.wm.window_mut(id) {
        w.decorations.shadow = false;
        w.closable = false;
    }
    *tooltip = Some((id, Instant::now() + Duration::from_millis(1200)));
    Ok(())
}
