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
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use atto_ui::app::{Desktop, MenuBar, MenuItem, MenuSpec};
use atto_ui::composable::{
    Align, Anchor, AnchorPlacement, Component, ComponentContext, EdgeInsets, EventOutcome,
    EventResult, Grid, HStack, LayoutParams, ScrollContainer, ScrollContainerHost, ScrollContent,
    ScrollContentContext, Size, VStack,
};
use atto_ui::reactive::{EventQueue, Property};
use atto_ui::theme::{Theme, ThemeConfig, ThemeConfigFormat};
use atto_ui::widgets::{Button, Checkbox, Label, ListBox, RadioGroup, TableView, TextBox};
use atto_ui::wm::{Window, WindowId, WindowKind, WindowState};
use atto_ui_macros::{Reactive, view_builder};

#[derive(Default)]
struct TextView {
    lines: Vec<String>,
}

impl TextView {
    fn new(lines: Vec<String>) -> Self {
        Self { lines }
    }
}

impl Component for TextView {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            return EventResult::close_window();
        }
        EventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
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
    root: VStack,
}

impl DialogView {
    fn about() -> Self {
        let row_layout = LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        };

        let root = VStack::new()
            .spacing(0)
            .child_with_layout(
                Label::new("Chatty demo (Turbo Vision-inspired)."),
                row_layout,
            )
            .child_with_layout(Label::new(""), row_layout)
            .child_with_layout(Label::new("Keys:"), row_layout)
            .child_with_layout(
                Label::new("  F10 menu   Ctrl+W window mode   F2 cycle theme"),
                row_layout,
            )
            .child_with_layout(
                Label::new("  n new win  a about/modal        t tooltip"),
                row_layout,
            )
            .child_with_layout(
                Label::new("  d widget states demo (disabled controls)"),
                row_layout,
            )
            .child_with_layout(Label::new("  v layout demo (view hierarchy)"), row_layout)
            .child_with_layout(
                Label::new("  s scroll demo (viewport + scrollbars)"),
                row_layout,
            )
            .child_with_layout(
                Label::new("  z virtual scroll demo (delegate-driven content)"),
                row_layout,
            )
            .child_with_layout(Label::new(""), row_layout)
            .child_with_layout(Button::new("Close (Enter)"), row_layout);

        Self { root }
    }
}

impl Component for DialogView {
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

    fn children(&self) -> &[atto_ui::composable::ComponentNode] {
        self.root.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<atto_ui::composable::ComponentNode>> {
        self.root.children_mut()
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let res = self.root.handle_event(event, ctx);
        if res.action == atto_ui::composable::ComponentAction::Submitted {
            return EventResult::close_window();
        }
        res
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.root.draw(frame, area, ctx);
    }
}

#[derive(Clone, Reactive)]
struct WidgetsModel {
    #[reactive]
    text: Property<String>,
    #[reactive]
    enable_feature: Property<bool>,
    #[reactive]
    mode: Property<usize>,
    #[reactive]
    list_selection: Property<usize>,
    #[reactive]
    table_selection: Property<usize>,
    #[reactive]
    click_count: Property<u32>,
    #[reactive]
    last_msg: Property<String>,
}

impl WidgetsModel {
    fn new() -> Self {
        let text = Property::new("hello".to_string());
        let enable_feature = Property::new(true);
        let mode = Property::new(0usize);
        let list_selection = Property::new(0usize);
        let table_selection = Property::new(0usize);
        let click_count = Property::new(0u32);
        Self {
            text,
            enable_feature,
            mode,
            list_selection,
            table_selection,
            click_count,
            last_msg: Property::new(String::new()),
        }
    }
}

struct WidgetsView {
    _model: WidgetsModel,
    root: VStack,
}

impl WidgetsView {
    fn new() -> Self {
        let model = WidgetsModel::new();
        let click_count = model.click_count.clone();
        let last_msg = model.last_msg.clone();
        let model_for_state = model.clone();

        let labels = view_builder! {
            VStack {
                Text("Try mouse drag on title bar; click × to close.")
                Text("Try bracketed paste into textbox: 你好👋 / 👩‍💻")
            }
            .spacing(0)
        };

        let widgets = {
            let left_column = VStack::new()
                .spacing(1)
                .child_with_layout(
                    TextBox::new("Text", model.text_binding()),
                    LayoutParams {
                        height: Size::Content,
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    Checkbox::new("Enable feature", model.enable_feature_binding()),
                    LayoutParams {
                        height: Size::Content,
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    RadioGroup::new(
                        "Mode",
                        vec!["Normal".into(), "Insert".into(), "Visual".into()],
                        model.mode_binding(),
                    ),
                    LayoutParams {
                        height: Size::Content,
                        ..LayoutParams::default()
                    },
                );

            HStack::new()
                .spacing(1)
                .child_with_layout(
                    left_column,
                    LayoutParams {
                        width: Size::Weight(1),
                        height: Size::Fill,
                        align_x: Align::Stretch,
                        align_y: Align::Stretch,
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    ListBox::new(
                        "List",
                        vec![
                            "Alpha".into(),
                            "Beta".into(),
                            "Gamma".into(),
                            "Delta".into(),
                        ],
                        model.list_selection_binding(),
                    )
                    .with_min_height(5),
                    LayoutParams {
                        width: Size::Weight(1),
                        height: Size::Fill,
                        align_x: Align::Stretch,
                        align_y: Align::Stretch,
                        ..LayoutParams::default()
                    },
                )
                .child_with_layout(
                    TableView::new(
                        "Table",
                        vec!["Key".into(), "Value".into()],
                        vec![
                            vec!["lang".into(), "Rust".into()],
                            vec!["jp".into(), "こんにちは".into()],
                            vec!["cn".into(), "你好👋".into()],
                        ],
                        model.table_selection_binding(),
                    )
                    .with_min_height(5),
                    LayoutParams {
                        width: Size::Weight(1),
                        height: Size::Fill,
                        align_x: Align::Stretch,
                        align_y: Align::Stretch,
                        ..LayoutParams::default()
                    },
                )
        };

        let buttons = view_builder! {
            HStack {
                Button("Count").on_click(move || {
                    click_count.update(|c| *c = c.saturating_add(1));
                })
                Spacer()
                Button("OK").on_click(move || {
                    last_msg.set("Submitted!".to_string());
                })
            }
            .spacing(1)
        };

        let states = view_builder! { TextFn(move || {
            let status = format!(
                "States: count={}  checked={}  mode={}  list={}  table={}  text={}",
                model_for_state.get_click_count(),
                if model_for_state.get_enable_feature() { "on" } else { "off" },
                model_for_state.get_mode(),
                model_for_state.get_list_selection(),
                model_for_state.get_table_selection(),
                model_for_state.get_text(),
            );
            let msg = model_for_state.get_last_msg();
            if msg.is_empty() {
                status
            } else {
                format!("{status}  |  {msg}")
            }
        }) };

        let root = VStack::new()
            .child_with_layout(
                labels,
                LayoutParams {
                    height: Size::Fixed(2),
                    ..LayoutParams::default()
                },
            )
            .child_with_layout(
                widgets,
                LayoutParams {
                    height: Size::Fill,
                    align_y: Align::Stretch,
                    ..LayoutParams::default()
                },
            )
            .child_with_layout(
                buttons,
                LayoutParams {
                    height: Size::Fixed(3),
                    ..LayoutParams::default()
                },
            )
            .child_with_layout(
                states,
                LayoutParams {
                    height: Size::Fixed(1),
                    ..LayoutParams::default()
                },
            )
            .spacing(1)
            .padding(1);

        Self {
            _model: model,
            root,
        }
    }
}

impl Component for WidgetsView {
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

    fn children(&self) -> &[atto_ui::composable::ComponentNode] {
        self.root.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<atto_ui::composable::ComponentNode>> {
        self.root.children_mut()
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.root.handle_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.root.draw(frame, area, ctx);
    }
}

struct DisabledWidgetsView {
    root: VStack,
}

impl DisabledWidgetsView {
    fn new() -> Self {
        let text = Property::new("read-only".to_string());
        let enable_feature = Property::new(true);
        let mode = Property::new(1usize);
        let list_selection = Property::new(0usize);
        let table_selection = Property::new(0usize);

        let row_layout = LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        };

        let root = VStack::new()
            .spacing(0)
            .child_with_layout(
                Label::new("Disabled widgets (not focusable/clickable; Esc closes)"),
                row_layout,
            )
            .child_with_layout(
                TextBox::new("Text (disabled)", text.binding()).enabled(false),
                row_layout,
            )
            .child_with_layout(
                Checkbox::new("Enable feature (disabled)", enable_feature.binding()).enabled(false),
                row_layout,
            )
            .child_with_layout(
                RadioGroup::new(
                    "Mode (disabled)",
                    vec!["Normal".into(), "Insert".into(), "Visual".into()],
                    mode.binding(),
                )
                .enabled(false),
                row_layout,
            )
            .child_with_layout(
                ListBox::new(
                    "List (disabled)",
                    vec![
                        "Alpha".into(),
                        "Beta".into(),
                        "Gamma".into(),
                        "Delta".into(),
                    ],
                    list_selection.binding(),
                )
                .height(4u16)
                .enabled(false),
                row_layout,
            )
            .child_with_layout(
                TableView::new(
                    "Table (disabled)",
                    vec!["Key".into(), "Value".into()],
                    vec![
                        vec!["lang".into(), "Rust".into()],
                        vec!["jp".into(), "こんにちは".into()],
                        vec!["cn".into(), "你好👋".into()],
                    ],
                    table_selection.binding(),
                )
                .height(4u16)
                .enabled(false),
                row_layout,
            )
            .child_with_layout(Button::new("OK (disabled)").enabled(false), row_layout)
            .child_with_layout(
                Label::new("Tip: focus another window to see inactive state."),
                row_layout,
            );

        Self { root }
    }
}

impl Component for DisabledWidgetsView {
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

    fn children(&self) -> &[atto_ui::composable::ComponentNode] {
        self.root.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<atto_ui::composable::ComponentNode>> {
        self.root.children_mut()
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            return EventResult::close_window();
        }

        self.root.handle_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.root.draw(frame, area, ctx);
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

impl Component for TooltipView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        frame.render_widget(
            Paragraph::new(self.text.clone()).style(ctx.theme.widget.normal),
            area,
        );
    }
}

fn build_layout_demo_view() -> Box<dyn Component> {
    let toolbar = HStack::new()
        .spacing(1)
        .child_with_layout(
            Label::new("Content"),
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
        )
        .child_with_layout(
            Button::new("W1"),
            LayoutParams {
                width: Size::Weight(1),
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            Button::new("W2"),
            LayoutParams {
                width: Size::Weight(2),
                ..LayoutParams::default()
            },
        );

    let grid = Grid::new()
        .columns(2usize)
        .row_gap(1u16)
        .column_gap(2u16)
        .child_with_layout(
            Button::new("Tall"),
            LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            Checkbox::new("Centered", Property::new(false).binding()),
            LayoutParams {
                align_y: Align::Center,
                ..LayoutParams::default()
            },
        )
        .child(Checkbox::new("Row 2A", Property::new(false).binding()))
        .child(Checkbox::new("Row 2B", Property::new(false).binding()));

    let root = VStack::new()
        .padding_insets(EdgeInsets::all(1))
        .spacing(1)
        .child_with_layout(
            Label::new("[ANCHOR]"),
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
        )
        .child_with_layout(
            Label::new("M6 layout demo (resize window)"),
            LayoutParams {
                height: Size::Content,
                margin: EdgeInsets {
                    right: 10,
                    ..EdgeInsets::ZERO
                },
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            toolbar,
            LayoutParams {
                height: Size::Fixed(3),
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            grid,
            LayoutParams {
                height: Size::Fixed(5),
                margin: EdgeInsets {
                    left: 2,
                    ..EdgeInsets::ZERO
                },
                ..LayoutParams::default()
            },
        );

    Box::new(root)
}

fn build_scroll_demo_view() -> Box<dyn Component> {
    let wide_row = (0..24).fold(HStack::new().spacing(1).scrollable(true), |row, i| {
        row.child_with_layout(
            Label::new(format!("[col-{i:02}]")),
            LayoutParams {
                width: Size::Content,
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
    });

    let root = (0..120u16).fold(
        VStack::new()
            .padding_insets(EdgeInsets::all(1))
            .spacing(1)
            .scrollable(true)
            .child_with_layout(
                Label::new(
                    "M7/M8 scrolling demo: ↑↓ PgUp/PgDn Home/End, wheel, drag scrollbar thumb",
                ),
                LayoutParams {
                    height: Size::Content,
                    ..LayoutParams::default()
                },
            )
            .child_with_layout(
                wide_row,
                LayoutParams {
                    height: Size::Fixed(3),
                    ..LayoutParams::default()
                },
            ),
        |v, i| {
            v.child_with_layout(
                Label::new(format!(
                    "{i:03}: The quick brown fox jumps over the lazy dog."
                )),
                LayoutParams {
                    height: Size::Content,
                    ..LayoutParams::default()
                },
            )
        },
    );

    Box::new(root)
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
        _host: &mut ScrollContainerHost,
    ) -> EventResult {
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            return EventResult::close_window();
        }
        EventResult::ignored()
    }

    fn draw(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ScrollContentContext<'_>,
        _host: &mut ScrollContainerHost,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let style = if ctx.component.is_focused {
            ctx.component.theme.widget.focused
        } else {
            ctx.component.theme.widget.normal
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

fn build_virtual_scroll_demo_view() -> ScrollContainer {
    ScrollContainer::new(Box::new(VirtualScrollContentView::new(10_000, 40)))
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

#[derive(Clone, Debug)]
enum DemoAction {
    Quit,
    NewWindow,
    FocusNext,
    OpenLayoutDemo,
    OpenScrollDemo,
    OpenVirtualScrollDemo,
    OpenWidgetStatesDemo,
    MinimizeFocused,
    ToggleMaximizeFocused,
    CloseFocused,
    OpenAboutModal,
    SetTheme(DemoTheme),
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
        w.title.set(format!("Notes (Theme: {})", theme.label()));
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
    let actions: EventQueue<DemoAction> = EventQueue::new();
    let menu = build_menu(actions.clone());
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
            build_layout_demo_view(),
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
        &actions,
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

fn build_menu(actions: EventQueue<DemoAction>) -> MenuBar {
    MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![
                MenuItem::action("New window", {
                    let actions = actions.clone();
                    move || actions.push(DemoAction::NewWindow)
                })
                .shortcut("n"),
                MenuItem::submenu(
                    "Theme",
                    vec![
                        MenuItem::action("Dark", {
                            let actions = actions.clone();
                            move || actions.push(DemoAction::SetTheme(DemoTheme::Dark))
                        }),
                        MenuItem::action("Dark + Unicode", {
                            let actions = actions.clone();
                            move || actions.push(DemoAction::SetTheme(DemoTheme::DarkUnicode))
                        }),
                        MenuItem::action("Dark + ASCII", {
                            let actions = actions.clone();
                            move || actions.push(DemoAction::SetTheme(DemoTheme::DarkAscii))
                        }),
                        MenuItem::action("Dark + High Contrast", {
                            let actions = actions.clone();
                            move || actions.push(DemoAction::SetTheme(DemoTheme::DarkHighContrast))
                        }),
                        MenuItem::action("Light", {
                            let actions = actions.clone();
                            move || actions.push(DemoAction::SetTheme(DemoTheme::Light))
                        }),
                    ],
                ),
                MenuItem::action("Quit", {
                    let actions = actions.clone();
                    move || actions.push(DemoAction::Quit)
                })
                .shortcut("q"),
            ],
        ),
        MenuSpec::new(
            "Window",
            vec![
                MenuItem::action("Next", {
                    let actions = actions.clone();
                    move || actions.push(DemoAction::FocusNext)
                })
                .shortcut("F6"),
                MenuItem::action("Widget states demo", {
                    let actions = actions.clone();
                    move || actions.push(DemoAction::OpenWidgetStatesDemo)
                })
                .shortcut("d"),
                MenuItem::action("Layout demo", {
                    let actions = actions.clone();
                    move || actions.push(DemoAction::OpenLayoutDemo)
                })
                .shortcut("v"),
                MenuItem::action("Scroll demo", {
                    let actions = actions.clone();
                    move || actions.push(DemoAction::OpenScrollDemo)
                })
                .shortcut("s"),
                MenuItem::action("Virtual scroll demo", {
                    let actions = actions.clone();
                    move || actions.push(DemoAction::OpenVirtualScrollDemo)
                })
                .shortcut("z"),
                MenuItem::action("Minimize", {
                    let actions = actions.clone();
                    move || actions.push(DemoAction::MinimizeFocused)
                })
                .shortcut("m"),
                MenuItem::action("Maximize", {
                    let actions = actions.clone();
                    move || actions.push(DemoAction::ToggleMaximizeFocused)
                })
                .shortcut("x"),
                MenuItem::action("Close", {
                    let actions = actions.clone();
                    move || actions.push(DemoAction::CloseFocused)
                })
                .shortcut("c"),
            ],
        ),
        MenuSpec::new(
            "Help",
            vec![
                MenuItem::action("About", {
                    let actions = actions.clone();
                    move || actions.push(DemoAction::OpenAboutModal)
                })
                .shortcut("a"),
            ],
        ),
    ])
}

#[allow(clippy::too_many_arguments)]
fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    desktop: &mut Desktop,
    demo_theme: &mut DemoTheme,
    actions: &EventQueue<DemoAction>,
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

        for action in actions.drain() {
            match action {
                DemoAction::Quit => break,
                DemoAction::NewWindow => {
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
                DemoAction::FocusNext => desktop.wm.focus_next(),
                DemoAction::OpenLayoutDemo => {
                    open_layout_demo(desktop, screen, layout_demo_window_id)?;
                }
                DemoAction::OpenScrollDemo => {
                    open_scroll_demo(desktop, screen, scroll_demo_window_id)?;
                }
                DemoAction::OpenVirtualScrollDemo => {
                    open_virtual_scroll_demo(desktop, screen, virtual_scroll_demo_window_id)?;
                }
                DemoAction::OpenWidgetStatesDemo => {
                    open_widget_states_demo(desktop, screen, widget_states_demo_window_id)?;
                }
                DemoAction::MinimizeFocused => desktop.wm.minimize_focused(),
                DemoAction::ToggleMaximizeFocused => {
                    let screen: Rect = terminal.size()?.into();
                    let work = Desktop::layout(screen).work_area;
                    desktop.wm.toggle_maximize_focused(work);
                }
                DemoAction::CloseFocused => {
                    if let Some(id) = desktop.wm.focused() {
                        desktop.wm.request_close(id);
                    }
                }
                DemoAction::OpenAboutModal => open_about_modal(desktop, screen)?,
                DemoAction::SetTheme(theme) => {
                    *demo_theme = theme;
                    apply_demo_theme(desktop, *demo_theme)?;
                    update_notes_title(desktop, notes_window_id, *demo_theme);
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
            && w.state.get() == WindowState::Minimized
        {
            w.state.set(WindowState::Normal);
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
            build_layout_demo_view(),
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
            build_scroll_demo_view(),
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
            let rect = w.rect.get();
            (rect.x.saturating_add(2), rect.y.saturating_add(2))
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
        w.decorations.update(|d| d.shadow = false);
        w.closable.set(false);
    }
    *tooltip = Some((id, Instant::now() + Duration::from_millis(1200)));
    Ok(())
}
