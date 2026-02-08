use crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use atto_ui_macros::{ComponentProperties, component_properties};
use crate::ComponentCommand;
use crate::composable::{Component, ComponentContext, ComponentId, ComponentNode, EventResult};
use crate::runtime::CallbackHandle;
use crate::reactive::Binding;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabHeaderPosition {
    Top,
    Bottom,
}

impl TabHeaderPosition {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "Top" | "top" => Some(Self::Top),
            "Bottom" | "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }
}

struct HeaderLayout {
    tab_ranges: Vec<(u16, u16)>,
}

#[derive(ComponentProperties)]
pub struct TabView {
    id: ComponentId,
    children: Vec<ComponentNode>,
    titles: Vec<Binding<String>>,
    selection: Binding<usize>,
    header_position: Binding<TabHeaderPosition>,
    last_area: Option<Rect>,
    last_header: Option<HeaderLayout>,
    focused: Option<ComponentId>,
    on_change_callback: Option<CallbackHandle>,
}

impl Default for TabView {
    fn default() -> Self {
        Self::new()
    }
}

impl TabView {
    pub fn new() -> Self {
        Self {
            id: ComponentId::next(),
            children: Vec::new(),
            titles: Vec::new(),
            selection: 0usize.into(),
            header_position: TabHeaderPosition::Top.into(),
            last_area: None,
            last_header: None,
            focused: None,
            on_change_callback: None,
        }
    }

    pub fn selection(mut self, selection: impl Into<Binding<usize>>) -> Self {
        self.selection = selection.into();
        self
    }

    pub fn with_header_position(mut self, position: impl Into<Binding<TabHeaderPosition>>) -> Self {
        self.header_position = position.into();
        self
    }

    pub fn header_position(self, position: impl Into<Binding<TabHeaderPosition>>) -> Self {
        self.with_header_position(position)
    }

    pub fn tab(
        mut self,
        title: impl Into<Binding<String>>,
        view: impl Component + 'static,
    ) -> Self {
        self.add_tab(title, view);
        self
    }

    pub fn len(&self) -> usize {
        self.children.len()
    }

    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    pub fn child(mut self, view: impl Component + 'static) -> Self {
        let title = format!("Tab{}", self.children.len());
        self.add_tab(title, view);
        self
    }

    pub fn add_tab(
        &mut self,
        title: impl Into<Binding<String>>,
        view: impl Component + 'static,
    ) -> ComponentId {
        let mut node = ComponentNode::new(Box::new(view));
        node.parent = Some(self.id);
        let id = node.id;
        self.children.push(node);
        self.titles.push(title.into());

        self.normalize_selection();
        if self.children.len() == 1 && self.children[0].view.is_focusable() {
            self.focused = Some(self.children[0].id);
        }

        id
    }

    pub fn remove_tab(&mut self, index: usize) -> bool {
        if index >= self.children.len() {
            return false;
        }
        let removed = self.children.remove(index);
        self.titles.remove(index);

        if self.focused == Some(removed.id) {
            self.focused = None;
        }
        self.normalize_selection();
        if let Some(active_id) = self.active_child_id()
            && self
                .children
                .iter()
                .any(|c| c.id == active_id && c.view.is_focusable())
        {
            self.focused = Some(active_id);
        }
        true
    }

    pub fn set_selected(&mut self, index: usize) {
        self.selection.set(index);
        self.normalize_selection();
    }

    pub fn selected(&self) -> Option<usize> {
        let len = self.children.len();
        if len == 0 {
            None
        } else {
            Some(self.selection.get().min(len - 1))
        }
    }

    fn active_child_id(&self) -> Option<ComponentId> {
        self.selected()
            .and_then(|idx| self.children.get(idx).map(|c| c.id))
    }

    fn normalize_selection(&mut self) -> Option<usize> {
        let len = self.children.len();
        if len == 0 {
            self.selection.set(0);
            self.focused = None;
            return None;
        }
        let mut selected = self.selection.get();
        if selected >= len {
            selected = len - 1;
            self.selection.set(selected);
        }
        Some(selected)
    }

    pub fn on_change_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_change_callback = Some(callback);
        self
    }

    fn emit_change(&self) {
        if let Some(cb) = &self.on_change_callback {
            cb.emit();
        }
    }

    fn header_and_content(area: Rect, position: TabHeaderPosition) -> (Rect, Rect) {
        if area.width == 0 || area.height == 0 {
            return (Rect::default(), Rect::default());
        }

        let header_height = 1;
        if area.height == 1 {
            let header = Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 1,
            };
            return (header, Rect::default());
        }

        match position {
            TabHeaderPosition::Top => {
                let header = Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: header_height,
                };
                let content = Rect {
                    x: area.x,
                    y: area.y + header_height,
                    width: area.width,
                    height: area.height.saturating_sub(header_height),
                };
                (header, content)
            }
            TabHeaderPosition::Bottom => {
                let header = Rect {
                    x: area.x,
                    y: area.y + area.height.saturating_sub(header_height),
                    width: area.width,
                    height: header_height,
                };
                let content = Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: area.height.saturating_sub(header_height),
                };
                (header, content)
            }
        }
    }

    fn build_header_line(
        &self,
        ctx: ComponentContext<'_>,
        selected: Option<usize>,
    ) -> (Line<'static>, Vec<(u16, u16)>) {
        let separator = ctx.theme.glyph("tab-separator").unwrap_or("|");
        let active_left = ctx.theme.glyph("tab-active-left").unwrap_or(">");
        let active_right = ctx.theme.glyph("tab-active-right").unwrap_or("<");

        let inactive_style = ctx
            .theme
            .named_style("tab-inactive")
            .unwrap_or(ctx.theme.widget.normal);
        let active_style = ctx
            .theme
            .named_style("tab-active")
            .unwrap_or(ctx.theme.widget.focused);
        let separator_style = ctx
            .theme
            .named_style("tab-separator")
            .unwrap_or(ctx.theme.widget.dim);

        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut ranges = Vec::with_capacity(self.children.len());
        let mut cursor: u16 = 0;

        fn push_text(spans: &mut Vec<Span<'static>>, cursor: &mut u16, text: &str, style: Style) {
            spans.push(Span::styled(text.to_string(), style));
            *cursor = cursor.saturating_add(UnicodeWidthStr::width(text) as u16);
        }

        fn push_sep(spans: &mut Vec<Span<'static>>, cursor: &mut u16, glyph: &str, style: Style) {
            let text = format!(" {} ", glyph);
            spans.push(Span::styled(text.clone(), style));
            *cursor = cursor.saturating_add(UnicodeWidthStr::width(text.as_str()) as u16);
        }

        if self.children.is_empty() {
            push_sep(&mut spans, &mut cursor, separator, separator_style);
            return (Line::from(spans), ranges);
        }

        push_sep(&mut spans, &mut cursor, separator, separator_style);

        for idx in 0..self.children.len() {
            let title = self.titles.get(idx).map(|t| t.get()).unwrap_or_default();
            let start = cursor;
            let style = if Some(idx) == selected {
                active_style
            } else {
                inactive_style
            };
            push_text(&mut spans, &mut cursor, &title, style);
            let end = cursor;
            ranges.push((start, end));

            if idx + 1 < self.children.len() {
                let glyph = if Some(idx + 1) == selected {
                    active_left
                } else if Some(idx) == selected {
                    active_right
                } else {
                    separator
                };
                push_sep(&mut spans, &mut cursor, glyph, separator_style);
            }
        }

        push_sep(&mut spans, &mut cursor, separator, separator_style);

        (Line::from(spans), ranges)
    }

    fn set_selection(&mut self, idx: usize) -> EventResult {
        if self.children.is_empty() {
            self.selection.set(0);
            self.focused = None;
            return EventResult::ignored();
        }
        let idx = idx.min(self.children.len().saturating_sub(1));
        let prev = self.selection.get();
        if prev == idx {
            return EventResult::ignored();
        }
        self.selection.set(idx);
        self.normalize_selection();
        if let Some(child) = self.children.get_mut(idx) {
            if child.view.is_focusable() {
                self.focused = Some(child.id);
                let _ = child.view.focus_first();
            } else {
                self.focused = None;
            }
        }
        EventResult::changed()
    }
}

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn mouse_coords_local_to_area(area: Rect, m: MouseEvent) -> Option<(u16, u16)> {
    if contains(area, m.column, m.row) {
        return Some((
            m.column.saturating_sub(area.x),
            m.row.saturating_sub(area.y),
        ));
    }

    // Nested containers may forward mouse coordinates already relative to this view.
    if m.column < area.width && m.row < area.height {
        return Some((m.column, m.row));
    }

    None
}

impl TabView {
    fn header_min_width(&self) -> u16 {
        if self.children.is_empty() {
            return UnicodeWidthStr::width(" | ") as u16;
        }

        let sep = UnicodeWidthStr::width(" | ") as u16;
        let left = UnicodeWidthStr::width(" > ") as u16;
        let right = UnicodeWidthStr::width(" < ") as u16;
        let gap = sep.max(left).max(right);

        let mut width = sep;
        for idx in 0..self.children.len() {
            let title = self.titles.get(idx).map(|t| t.get()).unwrap_or_default();
            width = width.saturating_add(UnicodeWidthStr::width(title.as_str()) as u16);
            if idx + 1 < self.children.len() {
                width = width.saturating_add(gap);
            }
        }
        width.saturating_add(sep)
    }
}

#[component_properties]
impl Component for TabView {
    fn focused_child(&self) -> Option<ComponentId> {
        self.focused
    }

    fn apply_command(&mut self, command: ComponentCommand) -> EventResult {
        match command {
            ComponentCommand::SelectIndex(idx) => self.set_selection(idx),
            _ => EventResult::ignored(),
        }
    }

    fn is_focusable(&self) -> bool {
        self.selected()
            .and_then(|idx| self.children.get(idx))
            .is_some_and(|c| c.view.is_focusable())
    }

    fn focus_first(&mut self) -> bool {
        let Some(idx) = self.selected() else {
            self.focused = None;
            return false;
        };
        let child = &mut self.children[idx];
        if !child.view.is_focusable() {
            self.focused = None;
            return false;
        }
        self.focused = Some(child.id);
        let _ = child.view.focus_first();
        true
    }

    fn focus_last(&mut self) -> bool {
        let Some(idx) = self.selected() else {
            self.focused = None;
            return false;
        };
        let child = &mut self.children[idx];
        if !child.view.is_focusable() {
            self.focused = None;
            return false;
        }
        self.focused = Some(child.id);
        let _ = child.view.focus_last();
        true
    }

    fn min_width(&self) -> u16 {
        let header = self.header_min_width();
        let child = self
            .selected()
            .and_then(|idx| self.children.get(idx))
            .map(|c| c.view.min_width())
            .unwrap_or(0);
        header.max(child)
    }

    fn min_height(&self) -> u16 {
        let header: u16 = 1;
        let child = self
            .selected()
            .and_then(|idx| self.children.get(idx))
            .map(|c| c.view.min_height())
            .unwrap_or(0);
        header.saturating_add(child)
    }

    fn desired_width(&self) -> Option<u16> {
        let header = self.header_min_width();
        let child = self
            .selected()
            .and_then(|idx| self.children.get(idx))
            .and_then(|c| c.view.desired_width())
            .unwrap_or(0);
        Some(header.max(child))
    }

    fn desired_height(&self) -> Option<u16> {
        let header: u16 = 1;
        let child = self
            .selected()
            .and_then(|idx| self.children.get(idx))
            .and_then(|c| c.view.desired_height())?;
        Some(header.saturating_add(child))
    }

    fn children(&self) -> &[ComponentNode] {
        &self.children
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        Some(&mut self.children)
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.normalize_selection();
        let Some(area) = self.last_area else {
            return EventResult::ignored();
        };

        let position = self.header_position.get();
        let local_area = Rect {
            x: 0,
            y: 0,
            width: area.width,
            height: area.height,
        };
        let (header_local, content_local) = Self::header_and_content(local_area, position);

        if let Event::Mouse(m) = event {
            let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
                return EventResult::ignored();
            };

            if contains(header_local, local_x, local_y) {
                if m.kind == MouseEventKind::Down(MouseButton::Left)
                    && let Some(layout) = &self.last_header
                    && let Some((idx, _)) = layout
                        .tab_ranges
                        .iter()
                        .enumerate()
                        .find(|(_, (start, end))| local_x >= *start && local_x < *end)
                {
                    let prev = self.selection.get();
                    if prev != idx {
                        self.selection.set(idx);
                        self.emit_change();
                        self.normalize_selection();
                        if let Some(child) = self.children.get_mut(idx) {
                            if child.view.is_focusable() {
                                self.focused = Some(child.id);
                                let _ = child.view.focus_first();
                            } else {
                                self.focused = None;
                            }
                        }
                        return EventResult::changed();
                    }
                    return EventResult::consumed();
                }
                return EventResult::ignored();
            }

            if contains(content_local, local_x, local_y) {
                let Some(idx) = self.selected() else {
                    return EventResult::ignored();
                };
                let child = &mut self.children[idx];
                if m.kind == MouseEventKind::Down(MouseButton::Left) && child.view.is_focusable() {
                    self.focused = Some(child.id);
                }

                let child_event = Event::Mouse(MouseEvent {
                    column: local_x.saturating_sub(content_local.x),
                    row: local_y.saturating_sub(content_local.y),
                    ..*m
                });
                let child_ctx = ComponentContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: ctx.is_focused && self.focused == Some(child.id),
                    scrollbar_host: ctx.scrollbar_host.for_child(),
                    tab_mode: ctx.tab_mode.for_child(),
                };
                return child.view.handle_event(&child_event, child_ctx);
            }

            return EventResult::ignored();
        }

        if let Some(idx) = self.selected() {
            let child = &mut self.children[idx];
            if child.view.is_focusable() && self.focused != Some(child.id) {
                self.focused = Some(child.id);
            }
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: ctx.is_focused && self.focused == Some(child.id),
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
            };
            let res = child.view.handle_event(event, child_ctx);
            if res.is_consumed() {
                return res;
            }
        }

        EventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        let selected = self.normalize_selection();

        let position = self.header_position.get();
        let (header_abs, content_abs) = Self::header_and_content(area, position);

        if header_abs.height > 0 && header_abs.width > 0 {
            let (line, ranges) = self.build_header_line(ctx, selected);
            self.last_header = Some(HeaderLayout { tab_ranges: ranges });

            let base_style = ctx
                .theme
                .named_style("tab-header")
                .unwrap_or(ctx.theme.widget.normal);
            let paragraph = Paragraph::new(line).style(base_style);
            frame.render_widget(paragraph, header_abs);
        } else {
            self.last_header = None;
        }

        if let Some(idx) = selected
            && let Some(child) = self.children.get_mut(idx)
        {
            child.set_bounds(content_abs);
            if child.view.is_focusable() {
                self.focused = Some(child.id);
            } else {
                self.focused = None;
            }

            if content_abs.width > 0 && content_abs.height > 0 {
                let child_ctx = ComponentContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: ctx.is_focused && self.focused == Some(child.id),
                    scrollbar_host: ctx.scrollbar_host.for_child(),
                    tab_mode: ctx.tab_mode.for_child(),
                };
                child.view.draw(frame, content_abs, child_ctx);
            }
        }
    }
}
