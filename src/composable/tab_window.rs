use crossterm::event::{Event, MouseButton, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::{
    Component, ComponentContext, DynamicTree, EventHandling, EventResult, FocusNav, Layout,
    Scrollable, TitleBarContent, TitleBarContext, TitleBarSpan,
};
use crate::{CallbackRegistry, ComponentSpec, TreeError, TreeOp};
use atto_ui_macros::{ComponentProperties, component_properties};

#[derive(ComponentProperties)]
pub struct TabWindowTab {
    pub title: String,
    pub view: Box<dyn Component>,
}

#[derive(ComponentProperties)]
pub struct TabWindow {
    tabs: Vec<TabWindowTab>,
    active: usize,
    title_scroll: u16,
}

impl TabWindow {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            title_scroll: 0,
        }
    }

    pub fn with_tab(title: impl Into<String>, view: Box<dyn Component>) -> Self {
        let mut out = Self::new();
        out.add_tab(title, view);
        out
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn active_tab(&self) -> Option<usize> {
        if self.tabs.is_empty() {
            None
        } else {
            Some(self.active.min(self.tabs.len().saturating_sub(1)))
        }
    }

    pub fn add_tab(&mut self, title: impl Into<String>, view: Box<dyn Component>) -> usize {
        let index = self.tabs.len();
        self.tabs.push(TabWindowTab {
            title: title.into(),
            view,
        });
        if self.tabs.len() == 1 {
            self.active = 0;
            let _ = self.tabs[0].view.focus_first();
        }
        index
    }

    pub fn insert_tab(
        &mut self,
        index: usize,
        title: impl Into<String>,
        view: Box<dyn Component>,
    ) -> usize {
        let idx = index.min(self.tabs.len());
        self.tabs.insert(
            idx,
            TabWindowTab {
                title: title.into(),
                view,
            },
        );
        if self.tabs.len() == 1 {
            self.active = 0;
            let _ = self.tabs[0].view.focus_first();
        } else if idx <= self.active {
            self.active += 1;
        }
        idx
    }

    pub fn remove_tab(&mut self, index: usize) -> Option<TabWindowTab> {
        if index >= self.tabs.len() {
            return None;
        }
        let removed = self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active = 0;
            return Some(removed);
        }
        if self.active == index {
            self.active = self.active.min(self.tabs.len().saturating_sub(1));
            let _ = self.tabs[self.active].view.focus_first();
        } else if index < self.active {
            self.active = self.active.saturating_sub(1);
        }
        Some(removed)
    }

    pub fn set_tab_title(&mut self, index: usize, title: impl Into<String>) -> bool {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.title = title.into();
            true
        } else {
            false
        }
    }

    pub fn select_tab(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() || self.active == index {
            return false;
        }
        self.active = index;
        let _ = self.tabs[self.active].view.focus_first();
        true
    }

    fn clamp_title_scroll(&mut self, total_width: u16, view_width: u16) {
        let max_scroll = total_width.saturating_sub(view_width);
        if self.title_scroll > max_scroll {
            self.title_scroll = max_scroll;
        }
    }

    fn scroll_title_left(
        &mut self,
        ranges: &[(u16, u16)],
        view_width: u16,
        total_width: u16,
    ) -> bool {
        let prev = self.title_scroll;
        if let Some(first) = first_visible_tab(ranges, self.title_scroll) {
            if ranges[first].0 < self.title_scroll {
                self.title_scroll = ranges[first].0;
            } else if first > 0 {
                self.title_scroll = ranges[first - 1].0;
            } else {
                self.title_scroll = 0;
            }
        }
        self.clamp_title_scroll(total_width, view_width);
        self.title_scroll != prev
    }

    fn scroll_title_right(
        &mut self,
        ranges: &[(u16, u16)],
        view_width: u16,
        total_width: u16,
    ) -> bool {
        let prev = self.title_scroll;
        let view_right = self.title_scroll.saturating_add(view_width);
        if let Some(last) = last_visible_tab(ranges, self.title_scroll, view_width) {
            if ranges[last].1 > view_right {
                self.title_scroll = ranges[last].1.saturating_sub(view_width);
            } else if last + 1 < ranges.len() {
                self.title_scroll = ranges[last + 1].1.saturating_sub(view_width);
            }
        }
        self.clamp_title_scroll(total_width, view_width);
        self.title_scroll != prev
    }

    fn active_view(&self) -> Option<&(dyn Component + '_)> {
        if let Some(tab) = self.tabs.get(self.active) {
            Some(tab.view.as_ref())
        } else {
            None
        }
    }

    fn active_view_mut(&mut self) -> Option<&mut (dyn Component + '_)> {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            Some(tab.view.as_mut())
        } else {
            None
        }
    }

    fn active_dynamic_view(&mut self) -> Result<&mut dyn Component, TreeError> {
        let Some(active) = self.tabs.get_mut(self.active) else {
            return Err(TreeError::InvalidTreeOp(
                "component does not support tree operations".to_string(),
            ));
        };
        if active.view.dynamic_root_spec().is_none() {
            return Err(TreeError::InvalidTreeOp(
                "component does not support tree operations".to_string(),
            ));
        }
        Ok(active.view.as_mut())
    }

    fn titlebar_layout(&mut self, ctx: &TitleBarContext<'_>) -> Option<TabTitlebarLayout> {
        if self.tabs.is_empty() || ctx.area.width == 0 {
            return None;
        }

        let theme = ctx.theme;
        let sep = theme.glyph("tab-separator").unwrap_or("|");
        let active_left = theme.glyph("tab-active-left").unwrap_or(">");
        let active_right = theme.glyph("tab-active-right").unwrap_or("<");
        let overflow_left_marker = theme.glyph("scrollbar-left-arrow").unwrap_or("\u{25C4}");
        let overflow_right_marker = theme.glyph("scrollbar-right-arrow").unwrap_or("\u{25BA}");

        let fallback_active = if ctx.is_focused {
            theme.window_title_focused
        } else {
            theme.window_title
        };
        let fallback_inactive = theme.window_title;

        let active_style = theme
            .named_style("tab-title-active")
            .unwrap_or(fallback_active);
        let inactive_style = theme
            .named_style("tab-title-inactive")
            .unwrap_or(fallback_inactive);
        let sep_style = theme
            .named_style("tab-title-separator")
            .unwrap_or(fallback_inactive);
        let marker_style = theme
            .named_style("tab-title-marker")
            .unwrap_or(fallback_active);

        let active_style = theme.window_bg.patch(active_style);
        let inactive_style = theme.window_bg.patch(inactive_style);
        let sep_style = theme.window_bg.patch(sep_style);
        let marker_style = theme.window_bg.patch(marker_style);

        let (full_spans, tab_ranges, total_width) = self.build_titlebar_line(
            sep,
            active_left,
            active_right,
            active_style,
            inactive_style,
            sep_style,
            marker_style,
        );
        self.clamp_title_scroll(total_width, ctx.area.width);

        let view_width = ctx.area.width;
        let mut visible_spans = if total_width > view_width {
            slice_titlebar_spans(&full_spans, self.title_scroll, view_width)
        } else {
            full_spans
        };

        let overflow_left = self.title_scroll > 0;
        let overflow_right = self.title_scroll.saturating_add(view_width) < total_width;
        if overflow_left || overflow_right {
            visible_spans = apply_titlebar_markers(
                visible_spans,
                view_width,
                overflow_left,
                overflow_right,
                overflow_left_marker,
                overflow_right_marker,
                marker_style,
            );
        }

        Some(TabTitlebarLayout {
            content: TitleBarContent {
                spans: visible_spans,
            },
            tab_ranges,
            total_width,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn build_titlebar_line(
        &self,
        separator: &str,
        active_left: &str,
        active_right: &str,
        active_style: Style,
        inactive_style: Style,
        separator_style: Style,
        marker_style: Style,
    ) -> (Vec<TitleBarSpan>, Vec<(u16, u16)>, u16) {
        let mut spans = Vec::new();
        let mut ranges = Vec::with_capacity(self.tabs.len());
        let mut cursor: u16 = 0;

        fn push_text(spans: &mut Vec<TitleBarSpan>, cursor: &mut u16, text: &str, style: Style) {
            spans.push(TitleBarSpan::styled(text.to_string(), style));
            *cursor = cursor.saturating_add(UnicodeWidthStr::width(text) as u16);
        }

        fn push_sep(spans: &mut Vec<TitleBarSpan>, cursor: &mut u16, glyph: &str, style: Style) {
            let text = format!(" {} ", glyph);
            spans.push(TitleBarSpan::styled(text.clone(), style));
            *cursor = cursor.saturating_add(UnicodeWidthStr::width(text.as_str()) as u16);
        }

        if self.tabs.is_empty() {
            push_sep(&mut spans, &mut cursor, separator, separator_style);
            return (spans, ranges, cursor);
        }

        push_sep(&mut spans, &mut cursor, separator, separator_style);

        for (idx, tab) in self.tabs.iter().enumerate() {
            if idx > 0 {
                let (glyph, style) = if idx == self.active {
                    (active_left, marker_style)
                } else if idx == self.active.saturating_add(1) {
                    (active_right, marker_style)
                } else {
                    (separator, separator_style)
                };
                push_sep(&mut spans, &mut cursor, glyph, style);
            }

            let label_style = if idx == self.active {
                active_style
            } else {
                inactive_style
            };
            let start = cursor;
            push_text(&mut spans, &mut cursor, &tab.title, label_style);
            let end = cursor;
            ranges.push((start, end));
        }

        push_sep(&mut spans, &mut cursor, separator, separator_style);

        (spans, ranges, cursor)
    }
}

impl Default for TabWindow {
    fn default() -> Self {
        Self::new()
    }
}

#[component_properties]
impl Component for TabWindow {
    fn titlebar(&mut self, ctx: TitleBarContext<'_>) -> Option<TitleBarContent> {
        self.titlebar_layout(&ctx).map(|layout| layout.content)
    }

    fn handle_titlebar_event(&mut self, event: &Event, ctx: TitleBarContext<'_>) -> EventResult {
        let Event::Mouse(m) = event else {
            return EventResult::ignored();
        };
        if !matches!(m.kind, MouseEventKind::Down(MouseButton::Left)) {
            return EventResult::ignored();
        }
        if ctx.area.width == 0 {
            return EventResult::ignored();
        }
        if m.row != ctx.area.y
            || m.column < ctx.area.x
            || m.column >= ctx.area.x.saturating_add(ctx.area.width)
        {
            return EventResult::ignored();
        }

        let Some(layout) = self.titlebar_layout(&ctx) else {
            return EventResult::ignored();
        };
        let local_x = m.column.saturating_sub(ctx.area.x);
        let header_width = ctx.area.width;
        let overflow_left = self.title_scroll > 0;
        let overflow_right = self.title_scroll.saturating_add(header_width) < layout.total_width;

        if overflow_left && local_x == 0 {
            let _ = self.scroll_title_left(&layout.tab_ranges, header_width, layout.total_width);
            return EventResult::consumed();
        }
        if overflow_right && local_x == header_width.saturating_sub(1) {
            let _ = self.scroll_title_right(&layout.tab_ranges, header_width, layout.total_width);
            return EventResult::consumed();
        }

        let full_x = local_x.saturating_add(self.title_scroll);
        if let Some((idx, _)) = layout
            .tab_ranges
            .iter()
            .enumerate()
            .find(|(_, (start, end))| full_x >= *start && full_x < *end)
        {
            let changed = self.select_tab(idx);
            return if changed {
                EventResult::changed()
            } else {
                EventResult::consumed()
            };
        }
        EventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let Some(view) = self.active_view_mut() else {
            return;
        };
        view.draw(frame, area, ctx);
    }
}

impl Layout for TabWindow {
    fn min_width(&self) -> u16 {
        self.tabs
            .iter()
            .map(|t| t.view.min_width())
            .max()
            .unwrap_or(0)
    }

    fn min_height(&self) -> u16 {
        self.tabs
            .iter()
            .map(|t| t.view.min_height())
            .max()
            .unwrap_or(0)
    }
}

impl Scrollable for TabWindow {
    fn is_scrollable(&self) -> bool {
        self.active_view().is_some_and(|v| v.is_scrollable())
    }

    fn content_size(&self) -> (u16, u16) {
        self.active_view().map_or((0, 0), |v| v.content_size())
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.active_view().map_or((0, 0), |v| v.scroll_offset())
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.active_view().map_or((0, 0), |v| v.viewport_size())
    }

    fn scroll_config(&self) -> super::ScrollConfig {
        self.active_view()
            .map_or(super::ScrollConfig::default(), |v| v.scroll_config())
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        if let Some(view) = self.active_view_mut() {
            view.set_scroll_offset(x, y);
        }
    }

    fn scroll_to_child(&mut self, child_id: super::ComponentId) {
        if let Some(view) = self.active_view_mut() {
            view.scroll_to_child(child_id);
        }
    }
}

impl FocusNav for TabWindow {
    fn is_focusable(&self) -> bool {
        self.active_view().is_some_and(|v| v.is_focusable())
    }

    fn focus_first(&mut self) -> bool {
        self.active_view_mut().is_some_and(|v| v.focus_first())
    }

    fn focus_last(&mut self) -> bool {
        self.active_view_mut().is_some_and(|v| v.focus_last())
    }
}

impl DynamicTree for TabWindow {
    fn apply_tree_ops(&mut self, ops: &[TreeOp]) -> Result<bool, TreeError> {
        self.active_dynamic_view()?.apply_tree_ops(ops)
    }

    fn rebuild_tree(&mut self) -> Result<(), TreeError> {
        self.active_dynamic_view()?.rebuild_tree()
    }

    fn dynamic_root_spec(&self) -> Option<&ComponentSpec> {
        self.tabs
            .get(self.active)
            .and_then(|tab| tab.view.dynamic_root_spec())
    }

    fn dynamic_callbacks(&self) -> Option<&CallbackRegistry> {
        self.tabs
            .get(self.active)
            .and_then(|tab| tab.view.dynamic_callbacks())
    }
}

impl EventHandling for TabWindow {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Some(view) = self.active_view_mut() else {
            return EventResult::ignored();
        };
        view.handle_event(event, ctx)
    }

    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Some(view) = self.active_view_mut() else {
            return EventResult::ignored();
        };
        view.handle_event_capture(event, ctx)
    }

    fn handle_event_bubble(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Some(view) = self.active_view_mut() else {
            return EventResult::ignored();
        };
        view.handle_event_bubble(event, ctx)
    }
}

struct TabTitlebarLayout {
    content: TitleBarContent,
    tab_ranges: Vec<(u16, u16)>,
    total_width: u16,
}

#[derive(Clone)]
struct StyledGrapheme {
    text: String,
    style: Option<Style>,
    width: u16,
}

fn first_visible_tab(ranges: &[(u16, u16)], scroll: u16) -> Option<usize> {
    ranges.iter().position(|(_, end)| *end > scroll)
}

fn last_visible_tab(ranges: &[(u16, u16)], scroll: u16, width: u16) -> Option<usize> {
    let right = scroll.saturating_add(width);
    ranges.iter().rposition(|(start, _)| *start < right)
}

fn slice_titlebar_spans(spans: &[TitleBarSpan], start: u16, width: u16) -> Vec<TitleBarSpan> {
    if width == 0 {
        return Vec::new();
    }
    let end = start.saturating_add(width);
    let mut col: u16 = 0;
    let mut out: Vec<TitleBarSpan> = Vec::new();

    for span in spans {
        let style = span.style;
        let mut buf = String::new();
        for g in span.text.graphemes(true) {
            let w = (UnicodeWidthStr::width(g) as u16).max(1);
            let next = col.saturating_add(w);
            if next <= start {
                col = next;
                continue;
            }
            if col >= end {
                break;
            }
            buf.push_str(g);
            col = next;
            if col >= end {
                break;
            }
        }
        if !buf.is_empty() {
            out.push(TitleBarSpan { text: buf, style });
        }
        if col >= end {
            break;
        }
    }
    out
}

fn apply_titlebar_markers(
    spans: Vec<TitleBarSpan>,
    width: u16,
    overflow_left: bool,
    overflow_right: bool,
    left_marker: &str,
    right_marker: &str,
    marker_style: Style,
) -> Vec<TitleBarSpan> {
    if spans.is_empty() || width == 0 {
        return spans;
    }

    let mut cells: Vec<StyledGrapheme> = Vec::new();
    for span in &spans {
        for g in span.text.graphemes(true) {
            let w = (UnicodeWidthStr::width(g) as u16).max(1);
            cells.push(StyledGrapheme {
                text: g.to_string(),
                style: span.style,
                width: w,
            });
        }
    }

    let current_width: u16 = cells.iter().map(|c| c.width).sum();
    if current_width < width {
        let pad = width - current_width;
        for _ in 0..pad {
            cells.push(StyledGrapheme {
                text: " ".to_string(),
                style: None,
                width: 1,
            });
        }
    }

    if overflow_left && !cells.is_empty() {
        let w = cells[0].width;
        let mut text = left_marker.to_string();
        if w > 1 {
            text.push_str(&" ".repeat((w - 1) as usize));
        }
        cells[0].text = text;
        cells[0].style = Some(marker_style);
    }

    if overflow_right && !cells.is_empty() {
        let idx = cells.len().saturating_sub(1);
        let w = cells[idx].width;
        let mut text = String::new();
        if w > 1 {
            text.push_str(&" ".repeat((w - 1) as usize));
        }
        text.push_str(right_marker);
        cells[idx].text = text;
        cells[idx].style = Some(marker_style);
    }

    let mut out: Vec<TitleBarSpan> = Vec::new();
    let mut current_style = cells[0].style;
    let mut current_text = String::new();
    for cell in cells {
        if cell.style == current_style {
            current_text.push_str(&cell.text);
        } else {
            if !current_text.is_empty() {
                out.push(TitleBarSpan {
                    text: current_text,
                    style: current_style,
                });
            }
            current_style = cell.style;
            current_text = cell.text;
        }
    }
    if !current_text.is_empty() {
        out.push(TitleBarSpan {
            text: current_text,
            style: current_style,
        });
    }

    out
}
