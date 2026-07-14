use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::component::{
    Capture, Component, ComponentContext, EventHandling, EventResult, FocusNav, Layout,
    MouseCoordinateSpace,
};
use crate::reactive::Binding;
use atto_ui_macros::{ComponentProperties, component_properties};

/// Text view with optional rendered-text selection and OSC52 copy support.
#[derive(Clone, ComponentProperties)]
pub struct Text {
    #[component(rename = "text")]
    text: Option<Binding<String>>,
    selectable: Binding<bool>,
    clipboard: Binding<String>,
    content: TextContent,
    style: Option<Style>,
    selection: TextSelectionState,
    last_area: Option<Rect>,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        let text = content.into();
        Self {
            text: Some(Binding::new(text.clone())),
            selectable: false.into(),
            clipboard: String::new().into(),
            content: TextContent::Static(text),
            style: None,
            selection: TextSelectionState::default(),
            last_area: None,
        }
    }

    pub fn from_fn<F>(f: F) -> Self
    where
        F: Fn() -> String + Send + Sync + 'static,
    {
        Self {
            text: None,
            selectable: false.into(),
            clipboard: String::new().into(),
            content: TextContent::Dynamic(Arc::new(f)),
            style: None,
            selection: TextSelectionState::default(),
            last_area: None,
        }
    }

    pub fn text(mut self, text: impl Into<Binding<String>>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn selectable(mut self, selectable: impl Into<Binding<bool>>) -> Self {
        self.selectable = selectable.into();
        self
    }

    pub fn clipboard(mut self, clipboard: impl Into<Binding<String>>) -> Self {
        self.clipboard = clipboard.into();
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn fg(mut self, color: Color) -> Self {
        self.style = Some(self.style.unwrap_or_default().fg(color));
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.style = Some(self.style.unwrap_or_default().bg(color));
        self
    }

    fn resolve(&self) -> String {
        match &self.content {
            TextContent::Static(s) => s.clone(),
            TextContent::Dynamic(f) => (f)(),
        }
    }

    fn text_value(&self) -> String {
        if let Some(text) = &self.text {
            text.get()
        } else {
            self.resolve()
        }
    }
}

#[component_properties]
impl Component for Text {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            return;
        }
        let style = self.style.unwrap_or(ctx.theme.widget.normal);
        let text = self.text_value();
        if self.selectable.get() {
            draw_selectable_text(
                frame.buffer_mut(),
                area,
                &text,
                style,
                ctx.theme.selection.add_modifier(Modifier::REVERSED),
                self.selection.range(),
            );
        } else {
            frame.render_widget(Paragraph::new(text).style(style), area);
        }
    }
}

impl Layout for Text {
    fn desired_height(&self) -> Option<u16> {
        Some(line_count(&self.text_value()).min(u16::MAX as usize) as u16)
    }

    fn desired_width(&self) -> Option<u16> {
        Some(max_line_width(&self.text_value()))
    }
}

impl FocusNav for Text {
    fn is_focusable(&self) -> bool {
        self.selectable.get()
    }
}

impl EventHandling for Text {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.selectable.get() {
            return EventResult::ignored();
        }

        match event {
            Event::Mouse(m) => {
                let Some(area) = self.last_area else {
                    if matches!(m.kind, MouseEventKind::Up(MouseButton::Left))
                        && self.selection.is_active()
                    {
                        return EventResult::consumed().with_capture(Capture::Release);
                    }
                    return EventResult::ignored();
                };
                let text = self.text_value();

                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let Some((local_x, local_y)) =
                            text_mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
                        else {
                            return EventResult::ignored();
                        };
                        let pos = position_for_point(&text, local_x, local_y);
                        self.selection.start(pos);
                        EventResult::consumed().with_capture(Capture::Request)
                    }
                    MouseEventKind::Drag(MouseButton::Left) => {
                        if self.selection.is_active() {
                            let Some((local_x, local_y)) = text_mouse_coords_clamped_to_area(
                                area,
                                *m,
                                ctx.mouse_coordinate_space,
                            ) else {
                                return EventResult::ignored();
                            };
                            let pos = position_for_point(&text, local_x, local_y);
                            self.selection.update(pos);
                            EventResult::consumed()
                        } else {
                            EventResult::ignored()
                        }
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        if self.selection.is_active() {
                            if let Some((local_x, local_y)) = text_mouse_coords_clamped_to_area(
                                area,
                                *m,
                                ctx.mouse_coordinate_space,
                            ) {
                                let pos = position_for_point(&text, local_x, local_y);
                                self.selection.update(pos);
                            }
                            EventResult::consumed().with_capture(Capture::Release)
                        } else {
                            EventResult::ignored()
                        }
                    }
                    _ => EventResult::ignored(),
                }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::CONTROL) => {
                let Some(text) = self.selected_text() else {
                    return EventResult::ignored();
                };
                self.clipboard.set(text.clone());
                let _ = crate::clipboard::copy_to_system_clipboard(&text);
                EventResult::consumed()
            }
            Event::Key(KeyEvent {
                code: KeyCode::Esc, ..
            }) => {
                if self.selection.clear() {
                    EventResult::consumed()
                } else {
                    EventResult::ignored()
                }
            }
            _ => EventResult::ignored(),
        }
    }
}

impl Text {
    fn selected_text(&self) -> Option<String> {
        selected_text_for_range(&self.text_value(), self.selection.range()?)
    }
}

crate::impl_component_default_traits!(Text => Scrollable, DynamicTree);

/// Dynamic text view (constructed from a closure).
///
/// This exists primarily to make `Text::from_fn` usable from the `view_builder!` macro, which
/// expects a `Type::new(...)` constructor form.
#[derive(Clone, ComponentProperties)]
pub struct TextFn {
    inner: Text,
}

impl TextFn {
    pub fn new<F>(f: F) -> Self
    where
        F: Fn() -> String + Send + Sync + 'static,
    {
        Self {
            inner: Text::from_fn(f),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.inner = self.inner.style(style);
        self
    }

    pub fn selectable(mut self, selectable: impl Into<Binding<bool>>) -> Self {
        self.inner = self.inner.selectable(selectable);
        self
    }

    pub fn clipboard(mut self, clipboard: impl Into<Binding<String>>) -> Self {
        self.inner = self.inner.clipboard(clipboard);
        self
    }

    pub fn fg(mut self, color: Color) -> Self {
        self.inner = self.inner.fg(color);
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.inner = self.inner.bg(color);
        self
    }
}

#[component_properties]
impl Component for TextFn {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.inner.draw(frame, area, ctx);
    }
}

impl Layout for TextFn {
    fn desired_height(&self) -> Option<u16> {
        self.inner.desired_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.inner.desired_width()
    }
}

impl FocusNav for TextFn {
    fn is_focusable(&self) -> bool {
        self.inner.is_focusable()
    }
}

impl EventHandling for TextFn {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.inner.handle_event(event, ctx)
    }
}

crate::impl_component_default_traits!(TextFn => Scrollable, DynamicTree);

#[derive(Clone)]
enum TextContent {
    Static(String),
    Dynamic(Arc<dyn Fn() -> String + Send + Sync>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct TextPosition {
    row: usize,
    col: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TextSelectionRange {
    start: TextPosition,
    end: TextPosition,
}

#[derive(Clone, Debug, Default)]
struct TextSelectionState {
    anchor: Option<TextPosition>,
    focus: Option<TextPosition>,
}

impl TextSelectionState {
    fn start(&mut self, pos: TextPosition) {
        self.anchor = Some(pos);
        self.focus = Some(pos);
    }

    fn update(&mut self, pos: TextPosition) {
        if self.anchor.is_some() {
            self.focus = Some(pos);
        }
    }

    fn is_active(&self) -> bool {
        self.anchor.is_some()
    }

    fn range(&self) -> Option<TextSelectionRange> {
        let anchor = self.anchor?;
        let focus = self.focus?;
        if anchor == focus {
            return None;
        }
        let (start, end) = if anchor <= focus {
            (anchor, focus)
        } else {
            (focus, anchor)
        };
        Some(TextSelectionRange { start, end })
    }

    fn clear(&mut self) -> bool {
        let had_selection = self.anchor.is_some() || self.focus.is_some();
        self.anchor = None;
        self.focus = None;
        had_selection
    }
}

fn text_mouse_coords_local_to_area(
    area: Rect,
    m: crossterm::event::MouseEvent,
    coordinate_space: MouseCoordinateSpace,
) -> Option<(u16, u16)> {
    match coordinate_space {
        MouseCoordinateSpace::Absolute => (area.width > 0
            && area.height > 0
            && m.column >= area.x
            && m.column < area.x.saturating_add(area.width)
            && m.row >= area.y
            && m.row < area.y.saturating_add(area.height))
        .then(|| {
            (
                m.column.saturating_sub(area.x),
                m.row.saturating_sub(area.y),
            )
        }),
        MouseCoordinateSpace::Local => {
            (area.width > 0 && area.height > 0 && m.column < area.width && m.row < area.height)
                .then_some((m.column, m.row))
        }
    }
}

fn text_mouse_coords_clamped_to_area(
    area: Rect,
    m: crossterm::event::MouseEvent,
    coordinate_space: MouseCoordinateSpace,
) -> Option<(u16, u16)> {
    if area.width == 0 || area.height == 0 {
        return None;
    }

    let max_x = area.width;
    let max_y = area.height.saturating_sub(1);
    match coordinate_space {
        MouseCoordinateSpace::Absolute => Some((
            m.column.saturating_sub(area.x).min(max_x),
            m.row.saturating_sub(area.y).min(max_y),
        )),
        MouseCoordinateSpace::Local => Some((m.column.min(max_x), m.row.min(max_y))),
    }
}

fn split_text_lines(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

fn line_count(text: &str) -> usize {
    split_text_lines(text).len()
}

fn max_line_width(text: &str) -> u16 {
    split_text_lines(text)
        .into_iter()
        .map(display_width)
        .max()
        .unwrap_or(0)
}

fn position_for_point(text: &str, x: u16, y: u16) -> TextPosition {
    let lines = split_text_lines(text);
    let row = usize::from(y).min(lines.len().saturating_sub(1));
    let width = lines.get(row).map_or(0, |line| display_width(line));
    TextPosition {
        row,
        col: x.min(width),
    }
}

fn draw_selectable_text(
    buf: &mut Buffer,
    area: Rect,
    text: &str,
    base_style: Style,
    selection_style: Style,
    selection: Option<TextSelectionRange>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    for (row, line) in split_text_lines(text)
        .into_iter()
        .take(usize::from(area.height))
        .enumerate()
    {
        let selected = selection.and_then(|range| selection_cols_for_line(range, row, line));
        draw_selectable_line(
            buf,
            area,
            row as u16,
            line,
            SelectableTextStyles {
                base: base_style,
                selection: selection_style,
            },
            selected,
        );
    }
}

#[derive(Clone, Copy)]
struct SelectableTextStyles {
    base: Style,
    selection: Style,
}

fn selection_cols_for_line(
    range: TextSelectionRange,
    row: usize,
    line: &str,
) -> Option<(u16, u16)> {
    if row < range.start.row || row > range.end.row {
        return None;
    }

    let line_width = display_width(line);
    let (start, end) = if range.start.row == range.end.row {
        (
            range.start.col.min(line_width),
            range.end.col.min(line_width),
        )
    } else if row == range.start.row {
        (range.start.col.min(line_width), line_width)
    } else if row == range.end.row {
        (0, range.end.col.min(line_width))
    } else {
        (0, line_width)
    };

    (start < end).then_some((start, end))
}

fn draw_selectable_line(
    buf: &mut Buffer,
    area: Rect,
    row: u16,
    line: &str,
    styles: SelectableTextStyles,
    selection_cols: Option<(u16, u16)>,
) {
    let width = area.width;
    if width == 0 {
        return;
    }

    let mut col: u16 = 0;
    for g in line.graphemes(true) {
        let w = (UnicodeWidthStr::width(g) as u16).max(1);
        let next = col.saturating_add(w);
        if next > width {
            break;
        }
        let selected = selection_cols.is_some_and(|(start, end)| start < next && end > col);
        let style = if selected {
            styles.selection
        } else {
            styles.base
        };
        buf.set_stringn(
            area.x.saturating_add(col),
            area.y.saturating_add(row),
            g,
            usize::from(width.saturating_sub(col)),
            style,
        );
        col = next;
    }
}

fn selected_text_for_range(text: &str, range: TextSelectionRange) -> Option<String> {
    let lines = split_text_lines(text);
    if range.start.row >= lines.len() || range.end.row >= lines.len() {
        return None;
    }

    if range.start.row == range.end.row {
        let line = lines[range.start.row];
        return slice_line_cols(line, range.start.col, range.end.col).filter(|s| !s.is_empty());
    }

    let mut out = String::new();
    let first =
        slice_line_cols(lines[range.start.row], range.start.col, u16::MAX).unwrap_or_default();
    out.push_str(&first);
    for line in lines
        .iter()
        .take(range.end.row)
        .skip(range.start.row.saturating_add(1))
    {
        out.push('\n');
        out.push_str(line);
    }
    out.push('\n');
    let last = slice_line_cols(lines[range.end.row], 0, range.end.col).unwrap_or_default();
    out.push_str(&last);

    (!out.is_empty()).then_some(out)
}

fn slice_line_cols(line: &str, start_col: u16, end_col: u16) -> Option<String> {
    let start_col = start_col.min(end_col);
    let start = byte_index_at_display_col(line, start_col);
    let end = byte_index_at_display_col(line, end_col).max(start);
    (start < end).then(|| line[start..end].to_string())
}

fn byte_index_at_display_col(text: &str, target_col: u16) -> usize {
    let mut col: u16 = 0;
    for (byte_idx, g) in text.grapheme_indices(true) {
        let w = (UnicodeWidthStr::width(g) as u16).max(1);
        let next = col.saturating_add(w);
        if target_col < next {
            return byte_idx;
        }
        col = next;
    }
    text.len()
}

fn display_width(text: &str) -> u16 {
    UnicodeWidthStr::width(text).min(u16::MAX as usize) as u16
}

#[cfg(test)]
mod text_selection_tests {
    use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    use super::{Text, TextPosition, TextSelectionRange, selected_text_for_range};
    use crate::composable::{
        Capture, Component, ComponentContext, EventHandling, EventOutcome, MouseCoordinateSpace,
        ScrollbarHost, TabMode,
    };
    use crate::theme::Theme;
    use crate::wm::WindowId;

    fn context<'a>(
        theme: &'a Theme,
        mouse_coordinate_space: MouseCoordinateSpace,
    ) -> ComponentContext<'a> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space,
            drag: None,
        }
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn selected_text_for_range_spans_lines() {
        let text = "alpha beta\ngamma delta\nomega";
        let selected = selected_text_for_range(
            text,
            TextSelectionRange {
                start: TextPosition { row: 0, col: 6 },
                end: TextPosition { row: 1, col: 5 },
            },
        );

        assert_eq!(selected.as_deref(), Some("beta\ngamma"));
    }

    #[test]
    fn selected_text_for_range_respects_grapheme_display_columns() {
        let selected = selected_text_for_range(
            "a你b",
            TextSelectionRange {
                start: TextPosition { row: 0, col: 1 },
                end: TextPosition { row: 0, col: 3 },
            },
        );

        assert_eq!(selected.as_deref(), Some("你"));
    }

    #[test]
    fn selectable_text_drag_requests_and_releases_pointer_capture() {
        let theme = Theme::dark();
        let mut text = Text::new("alpha beta\ngamma delta\nomega").selectable(true);
        text.last_area = Some(Rect::new(10, 5, 20, 3));

        let down = text.handle_event(
            &mouse(MouseEventKind::Down(MouseButton::Left), 16, 5),
            context(&theme, MouseCoordinateSpace::Absolute),
        );
        assert_eq!(down.outcome, EventOutcome::Consumed);
        assert_eq!(down.capture, Capture::Request);

        let up = text.handle_event(
            &mouse(MouseEventKind::Up(MouseButton::Left), 15, 6),
            context(&theme, MouseCoordinateSpace::Absolute),
        );
        assert_eq!(up.outcome, EventOutcome::Consumed);
        assert_eq!(up.capture, Capture::Release);
        assert_eq!(text.selected_text().as_deref(), Some("beta\ngamma"));
    }

    #[test]
    fn selectable_text_mouse_up_outside_area_still_releases_capture() {
        let theme = Theme::dark();
        let mut text = Text::new("alpha beta\ngamma delta\nomega").selectable(true);
        text.last_area = Some(Rect::new(10, 5, 20, 3));

        let down = text.handle_event(
            &mouse(MouseEventKind::Down(MouseButton::Left), 16, 5),
            context(&theme, MouseCoordinateSpace::Absolute),
        );
        assert_eq!(down.capture, Capture::Request);

        let up = text.handle_event(
            &mouse(MouseEventKind::Up(MouseButton::Left), 80, 20),
            context(&theme, MouseCoordinateSpace::Absolute),
        );
        assert_eq!(up.outcome, EventOutcome::Consumed);
        assert_eq!(up.capture, Capture::Release);
    }

    #[test]
    fn selectable_text_draws_selected_cells_with_distinct_style() {
        let theme = Theme::dark();
        let mut text = Text::new("alpha beta\ngamma delta\nomega").selectable(true);
        text.last_area = Some(Rect::new(0, 0, 20, 3));
        text.handle_event(
            &mouse(MouseEventKind::Down(MouseButton::Left), 6, 0),
            context(&theme, MouseCoordinateSpace::Absolute),
        );
        text.handle_event(
            &mouse(MouseEventKind::Up(MouseButton::Left), 5, 1),
            context(&theme, MouseCoordinateSpace::Absolute),
        );

        let mut terminal = Terminal::new(TestBackend::new(20, 3)).expect("terminal");
        terminal
            .draw(|frame| {
                text.draw(
                    frame,
                    Rect::new(0, 0, 20, 3),
                    context(&theme, MouseCoordinateSpace::Absolute),
                )
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();

        assert_ne!(buffer[(6, 0)].style(), buffer[(0, 0)].style());
    }
}

/// Spacer view (takes space, renders nothing).
#[derive(Clone, Debug, Default, ComponentProperties)]
pub struct Spacer;

impl Spacer {
    pub fn new() -> Self {
        Self
    }
}

#[component_properties]
impl Component for Spacer {
    fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
}

crate::impl_component_default_traits!(Spacer => Layout, Scrollable, FocusNav, DynamicTree, EventHandling);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DividerOrientation {
    Horizontal,
    Vertical,
}

impl DividerOrientation {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "Horizontal" | "horizontal" => Some(Self::Horizontal),
            "Vertical" | "vertical" => Some(Self::Vertical),
            _ => None,
        }
    }
}

/// Divider view (horizontal or vertical line).
#[derive(Clone, Debug, ComponentProperties)]
pub struct Divider {
    #[component(rename = "orientation")]
    orientation: Binding<DividerOrientation>,
}

impl Divider {
    pub fn new(orientation: DividerOrientation) -> Self {
        Self {
            orientation: orientation.into(),
        }
    }

    pub fn horizontal() -> Self {
        Self::new(DividerOrientation::Horizontal)
    }

    pub fn vertical() -> Self {
        Self::new(DividerOrientation::Vertical)
    }

    pub fn orientation(mut self, orientation: impl Into<Binding<DividerOrientation>>) -> Self {
        self.orientation = orientation.into();
        self
    }
}

#[component_properties]
impl Component for Divider {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let style = ctx.theme.widget.normal;
        if matches!(self.orientation.get(), DividerOrientation::Horizontal) {
            let line = "─".repeat(area.width as usize);
            frame.render_widget(Paragraph::new(Line::styled(line, style)), area);
            return;
        }

        let buf = frame.buffer_mut();
        for dy in 0..area.height {
            buf[(area.x, area.y.saturating_add(dy))]
                .set_symbol("│")
                .set_style(style);
        }
    }
}

impl Layout for Divider {
    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }

    fn desired_width(&self) -> Option<u16> {
        Some(1)
    }
}

crate::impl_component_default_traits!(Divider => Scrollable, FocusNav, DynamicTree, EventHandling);
