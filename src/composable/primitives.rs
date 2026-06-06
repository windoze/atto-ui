use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::component::{
    Component, ComponentContext, EventHandling, EventResult, FocusNav, Layout, MouseCoordinateSpace,
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
            let lines =
                selectable_text_lines(&text, style, ctx.theme.selection, self.selection.range());
            frame.render_widget(Paragraph::new(lines), area);
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
                    return EventResult::ignored();
                };
                let Some((local_x, local_y)) =
                    text_mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
                else {
                    return EventResult::ignored();
                };
                let text = self.text_value();
                let pos = position_for_point(&text, local_x, local_y);

                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        self.selection.start(pos);
                        EventResult::consumed()
                    }
                    MouseEventKind::Drag(MouseButton::Left) => {
                        if self.selection.is_active() {
                            self.selection.update(pos);
                            EventResult::consumed()
                        } else {
                            EventResult::ignored()
                        }
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        if self.selection.is_active() {
                            EventResult::consumed()
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

fn selectable_text_lines(
    text: &str,
    base_style: Style,
    selection_style: Style,
    selection: Option<TextSelectionRange>,
) -> Vec<Line<'static>> {
    split_text_lines(text)
        .into_iter()
        .enumerate()
        .map(|(row, line)| {
            let selected = selection.and_then(|range| selection_cols_for_line(range, row, line));
            selectable_line(line, base_style, selection_style, selected)
        })
        .collect()
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

fn selectable_line(
    line: &str,
    base_style: Style,
    selection_style: Style,
    selection_cols: Option<(u16, u16)>,
) -> Line<'static> {
    if line.is_empty() {
        return Line::styled(String::new(), base_style);
    }

    let mut spans = Vec::new();
    let mut col: u16 = 0;
    for g in line.graphemes(true) {
        let w = (UnicodeWidthStr::width(g) as u16).max(1);
        let next = col.saturating_add(w);
        let selected = selection_cols.is_some_and(|(start, end)| start < next && end > col);
        let style = if selected {
            selection_style
        } else {
            base_style
        };
        spans.push(Span::styled(g.to_string(), style));
        col = next;
    }
    Line::from(spans)
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
    use super::{TextPosition, TextSelectionRange, selected_text_for_range};

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
