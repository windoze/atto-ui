use std::cmp::Ordering;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::component::{Component, ComponentContext, EventResult, TabMode};
use super::node::{ComponentId, ComponentNode};
use crate::reactive::Binding;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitterOrientation {
    /// Panels are left/right with a vertical divider.
    Vertical,
    /// Panels are top/bottom with a horizontal divider.
    Horizontal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabDirection {
    Next,
    Prev,
}

fn tab_direction_for_event(event: &Event) -> Option<TabDirection> {
    match event {
        Event::Key(KeyEvent {
            code: KeyCode::Tab,
            modifiers,
            ..
        }) => Some(if modifiers.contains(KeyModifiers::SHIFT) {
            TabDirection::Prev
        } else {
            TabDirection::Next
        }),
        Event::Key(KeyEvent {
            code: KeyCode::BackTab,
            ..
        }) => Some(TabDirection::Prev),
        _ => None,
    }
}

fn focusable_children_in_tab_order(children: &[ComponentNode]) -> Vec<ComponentId> {
    let mut focusable: Vec<(Option<i32>, usize, ComponentId)> = children
        .iter()
        .enumerate()
        .filter(|(_, c)| c.view.is_focusable())
        .map(|(idx, c)| (c.layout.tab_index, idx, c.id))
        .collect();

    focusable.sort_by(|a, b| match (a.0, b.0) {
        (Some(a_idx), Some(b_idx)) => a_idx.cmp(&b_idx).then_with(|| a.1.cmp(&b.1)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.1.cmp(&b.1),
    });

    focusable.into_iter().map(|(_, _, id)| id).collect()
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

#[derive(Clone, Copy, Debug, Default)]
struct SplitLayout {
    area: Rect,
    first: Rect,
    divider: Rect,
    second: Rect,
}

#[derive(Clone, Copy, Debug)]
struct DragState {
    grab_offset: u16,
}

pub struct Splitter {
    children: Vec<ComponentNode>,
    orientation: SplitterOrientation,
    split_pos: Binding<u16>,
    min_first: Binding<u16>,
    min_second: Binding<u16>,
    border: Binding<bool>,
    border_style: Option<Style>,
    last_layout: Option<SplitLayout>,
    drag: Option<DragState>,
    split_auto: bool,
    initial_sizes: Option<(u16, u16)>,
    focused: Option<ComponentId>,
}

impl Splitter {
    pub fn new(
        orientation: SplitterOrientation,
        first: impl Component + 'static,
        second: impl Component + 'static,
    ) -> Self {
        let id = ComponentId::next();
        let mut children = Vec::with_capacity(2);

        let mut first_node = ComponentNode::new(Box::new(first));
        first_node.parent = Some(id);
        children.push(first_node);

        let mut second_node = ComponentNode::new(Box::new(second));
        second_node.parent = Some(id);
        children.push(second_node);

        let focused = children
            .iter()
            .find(|c| c.view.is_focusable())
            .map(|c| c.id);

        Self {
            children,
            orientation,
            split_pos: 0u16.into(),
            min_first: 0u16.into(),
            min_second: 0u16.into(),
            border: true.into(),
            border_style: None,
            last_layout: None,
            drag: None,
            split_auto: true,
            initial_sizes: None,
            focused,
        }
    }

    pub fn vertical(first: impl Component + 'static, second: impl Component + 'static) -> Self {
        Self::new(SplitterOrientation::Vertical, first, second)
    }

    pub fn horizontal(first: impl Component + 'static, second: impl Component + 'static) -> Self {
        Self::new(SplitterOrientation::Horizontal, first, second)
    }

    /// Set initial sizes for the panels (used the first time layout runs).
    pub fn with_initial_sizes(mut self, first: u16, second: u16) -> Self {
        self.initial_sizes = Some((first, second));
        self.split_auto = true;
        self
    }

    /// Bind the split position (size of the first panel in cells).
    pub fn split_position(mut self, split_pos: impl Into<Binding<u16>>) -> Self {
        self.split_pos = split_pos.into();
        self.split_auto = false;
        self
    }

    pub fn set_split_position(&mut self, split_pos: u16) {
        self.split_pos.set(split_pos);
        self.split_auto = false;
    }

    pub fn min_first(mut self, min: impl Into<Binding<u16>>) -> Self {
        self.min_first = min.into();
        self
    }

    pub fn min_second(mut self, min: impl Into<Binding<u16>>) -> Self {
        self.min_second = min.into();
        self
    }

    pub fn min_sizes(
        mut self,
        first: impl Into<Binding<u16>>,
        second: impl Into<Binding<u16>>,
    ) -> Self {
        self.min_first = first.into();
        self.min_second = second.into();
        self
    }

    /// Toggle divider border visibility.
    pub fn with_border(mut self, border: impl Into<Binding<bool>>) -> Self {
        self.border = border.into();
        self
    }

    /// Style for the divider border.
    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = Some(style);
        self
    }

    fn divider_thickness(&self) -> u16 {
        1
    }

    fn axis_min_first(&self) -> u16 {
        let configured = self.min_first.get();
        let child = self.children.get(0);
        let child_min = match (self.orientation, child) {
            (SplitterOrientation::Vertical, Some(node)) => node.view.min_width(),
            (SplitterOrientation::Horizontal, Some(node)) => node.view.min_height(),
            _ => 0,
        };
        configured.max(child_min)
    }

    fn axis_min_second(&self) -> u16 {
        let configured = self.min_second.get();
        let child = self.children.get(1);
        let child_min = match (self.orientation, child) {
            (SplitterOrientation::Vertical, Some(node)) => node.view.min_width(),
            (SplitterOrientation::Horizontal, Some(node)) => node.view.min_height(),
            _ => 0,
        };
        configured.max(child_min)
    }

    fn cross_min_size(&self) -> u16 {
        let first = self.children.get(0);
        let second = self.children.get(1);
        match self.orientation {
            SplitterOrientation::Vertical => {
                let a = first.map(|c| c.view.min_height()).unwrap_or(0);
                let b = second.map(|c| c.view.min_height()).unwrap_or(0);
                a.max(b)
            }
            SplitterOrientation::Horizontal => {
                let a = first.map(|c| c.view.min_width()).unwrap_or(0);
                let b = second.map(|c| c.view.min_width()).unwrap_or(0);
                a.max(b)
            }
        }
    }

    fn initial_split(&self, available: u16) -> u16 {
        if available == 0 {
            return 0;
        }

        if let Some((first, second)) = self.initial_sizes {
            let total = first.saturating_add(second) as u32;
            if total > 0 {
                return ((available as u32) * (first as u32) / total).min(u16::MAX as u32) as u16;
            }
        }

        available / 2
    }

    fn clamp_split(&self, desired: u16, span: u16, min_first: u16, min_second: u16) -> u16 {
        let divider = self.divider_thickness();
        if span <= divider {
            return 0;
        }

        let available = span.saturating_sub(divider);
        let min_first = min_first.min(available);
        let max_first = available.saturating_sub(min_second);
        if max_first < min_first {
            return min_first;
        }

        desired.clamp(min_first, max_first)
    }

    fn layout_for_area(&mut self, area: Rect) -> SplitLayout {
        let divider = self.divider_thickness();
        let span = match self.orientation {
            SplitterOrientation::Vertical => area.width,
            SplitterOrientation::Horizontal => area.height,
        };

        let min_first = self.axis_min_first();
        let min_second = self.axis_min_second();

        if self.split_auto {
            let available = span.saturating_sub(divider);
            let desired = self.initial_split(available);
            self.split_pos.set(desired);
            self.split_auto = false;
        }

        let desired = self.split_pos.get();
        let clamped = self.clamp_split(desired, span, min_first, min_second);
        if clamped != desired {
            self.split_pos.set(clamped);
        }

        let available = span.saturating_sub(divider);
        let first_len = clamped.min(available);
        let second_len = available.saturating_sub(first_len);

        let (first, divider_rect, second) = match self.orientation {
            SplitterOrientation::Vertical => {
                let first = Rect {
                    x: 0,
                    y: 0,
                    width: first_len,
                    height: area.height,
                };
                let divider_rect = Rect {
                    x: first_len,
                    y: 0,
                    width: divider.min(area.width.saturating_sub(first_len)),
                    height: area.height,
                };
                let second = Rect {
                    x: first_len.saturating_add(divider),
                    y: 0,
                    width: second_len,
                    height: area.height,
                };
                (first, divider_rect, second)
            }
            SplitterOrientation::Horizontal => {
                let first = Rect {
                    x: 0,
                    y: 0,
                    width: area.width,
                    height: first_len,
                };
                let divider_rect = Rect {
                    x: 0,
                    y: first_len,
                    width: area.width,
                    height: divider.min(area.height.saturating_sub(first_len)),
                };
                let second = Rect {
                    x: 0,
                    y: first_len.saturating_add(divider),
                    width: area.width,
                    height: second_len,
                };
                (first, divider_rect, second)
            }
        };

        SplitLayout {
            area,
            first,
            divider: divider_rect,
            second,
        }
    }

    fn first_focusable_child(&self) -> Option<ComponentId> {
        focusable_children_in_tab_order(&self.children)
            .first()
            .copied()
    }

    fn focus_focused_child_edge(&mut self, direction: TabDirection) {
        let Some(child_id) = self.focused else {
            return;
        };
        let Some(child_idx) = self.children.iter().position(|c| c.id == child_id) else {
            return;
        };

        match direction {
            TabDirection::Next => {
                let _ = self.children[child_idx].view.focus_first();
            }
            TabDirection::Prev => {
                let _ = self.children[child_idx].view.focus_last();
            }
        }
    }

    fn handle_tab_navigation(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Some(direction) = tab_direction_for_event(event) else {
            return EventResult::ignored();
        };

        if !ctx.is_focused {
            return EventResult::ignored();
        }

        let focusable = focusable_children_in_tab_order(&self.children);
        if focusable.is_empty() {
            self.focused = None;
            return EventResult::ignored();
        }

        let wrap = matches!(ctx.tab_mode, TabMode::Cycle);
        let desired = match self
            .focused
            .and_then(|id| focusable.iter().position(|x| *x == id))
        {
            Some(idx) => match direction {
                TabDirection::Next => {
                    if idx + 1 < focusable.len() {
                        Some(focusable[idx + 1])
                    } else if wrap {
                        Some(focusable[0])
                    } else {
                        None
                    }
                }
                TabDirection::Prev => {
                    if idx > 0 {
                        Some(focusable[idx - 1])
                    } else if wrap {
                        Some(focusable[focusable.len() - 1])
                    } else {
                        None
                    }
                }
            },
            None => Some(match direction {
                TabDirection::Next => focusable[0],
                TabDirection::Prev => focusable[focusable.len() - 1],
            }),
        };

        let Some(id) = desired else {
            return EventResult::ignored();
        };

        self.focused = Some(id);
        self.focus_focused_child_edge(direction);
        EventResult::consumed()
    }

    fn update_dragged_split(&mut self, local_x: u16, local_y: u16) -> bool {
        let Some(layout) = self.last_layout else {
            return false;
        };
        let Some(drag) = self.drag else {
            return false;
        };

        let axis_pos = match self.orientation {
            SplitterOrientation::Vertical => local_x.saturating_sub(drag.grab_offset),
            SplitterOrientation::Horizontal => local_y.saturating_sub(drag.grab_offset),
        };

        let span = match self.orientation {
            SplitterOrientation::Vertical => layout.area.width,
            SplitterOrientation::Horizontal => layout.area.height,
        };

        let min_first = self.axis_min_first();
        let min_second = self.axis_min_second();
        let clamped = self.clamp_split(axis_pos, span, min_first, min_second);
        let current = self.split_pos.get();
        if clamped != current {
            self.split_pos.set(clamped);
            return true;
        }
        false
    }
}

impl Component for Splitter {
    fn is_focusable(&self) -> bool {
        self.children.iter().any(|c| c.view.is_focusable())
    }

    fn focus_first(&mut self) -> bool {
        let Some(child_id) = focusable_children_in_tab_order(&self.children)
            .first()
            .copied()
        else {
            self.focused = None;
            return false;
        };

        self.focused = Some(child_id);
        if let Some(child_idx) = self.children.iter().position(|c| c.id == child_id) {
            let _ = self.children[child_idx].view.focus_first();
        }
        true
    }

    fn focus_last(&mut self) -> bool {
        let focusable = focusable_children_in_tab_order(&self.children);
        let Some(&child_id) = focusable.last() else {
            self.focused = None;
            return false;
        };

        self.focused = Some(child_id);
        if let Some(child_idx) = self.children.iter().position(|c| c.id == child_id) {
            let _ = self.children[child_idx].view.focus_last();
        }
        true
    }

    fn min_width(&self) -> u16 {
        match self.orientation {
            SplitterOrientation::Vertical => self
                .axis_min_first()
                .saturating_add(self.axis_min_second())
                .saturating_add(self.divider_thickness()),
            SplitterOrientation::Horizontal => self.cross_min_size(),
        }
    }

    fn min_height(&self) -> u16 {
        match self.orientation {
            SplitterOrientation::Horizontal => self
                .axis_min_first()
                .saturating_add(self.axis_min_second())
                .saturating_add(self.divider_thickness()),
            SplitterOrientation::Vertical => self.cross_min_size(),
        }
    }

    fn desired_width(&self) -> Option<u16> {
        let first = self.children.get(0);
        let second = self.children.get(1);
        match self.orientation {
            SplitterOrientation::Vertical => {
                let w1 = first
                    .map(|c| c.view.desired_width().unwrap_or(c.view.min_width()))
                    .unwrap_or(0);
                let w2 = second
                    .map(|c| c.view.desired_width().unwrap_or(c.view.min_width()))
                    .unwrap_or(0);
                Some(
                    w1.saturating_add(w2)
                        .saturating_add(self.divider_thickness()),
                )
            }
            SplitterOrientation::Horizontal => {
                let w1 = first
                    .and_then(|c| c.view.desired_width())
                    .unwrap_or_else(|| first.map(|c| c.view.min_width()).unwrap_or(0));
                let w2 = second
                    .and_then(|c| c.view.desired_width())
                    .unwrap_or_else(|| second.map(|c| c.view.min_width()).unwrap_or(0));
                Some(w1.max(w2))
            }
        }
    }

    fn desired_height(&self) -> Option<u16> {
        let first = self.children.get(0);
        let second = self.children.get(1);
        match self.orientation {
            SplitterOrientation::Horizontal => {
                let h1 = first
                    .map(|c| c.view.desired_height().unwrap_or(c.view.min_height()))
                    .unwrap_or(0);
                let h2 = second
                    .map(|c| c.view.desired_height().unwrap_or(c.view.min_height()))
                    .unwrap_or(0);
                Some(
                    h1.saturating_add(h2)
                        .saturating_add(self.divider_thickness()),
                )
            }
            SplitterOrientation::Vertical => {
                let h1 = first
                    .and_then(|c| c.view.desired_height())
                    .unwrap_or_else(|| first.map(|c| c.view.min_height()).unwrap_or(0));
                let h2 = second
                    .and_then(|c| c.view.desired_height())
                    .unwrap_or_else(|| second.map(|c| c.view.min_height()).unwrap_or(0));
                Some(h1.max(h2))
            }
        }
    }

    fn children(&self) -> &[ComponentNode] {
        &self.children
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        Some(&mut self.children)
    }

    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let tab = self.handle_tab_navigation(event, ctx);
        if tab.is_consumed() {
            return tab;
        }
        EventResult::ignored()
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let capture = self.handle_event_capture(event, ctx);
        if capture.is_consumed() {
            return capture;
        }

        if let Event::Mouse(m) = event {
            let Some(layout) = self.last_layout else {
                return EventResult::ignored();
            };
            let Some((local_x, local_y)) = mouse_coords_local_to_area(layout.area, *m) else {
                return EventResult::ignored();
            };

            if self.drag.is_some() {
                match m.kind {
                    MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
                        let changed = self.update_dragged_split(local_x, local_y);
                        return if changed {
                            EventResult::consumed()
                        } else {
                            EventResult::ignored()
                        };
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        self.drag = None;
                        return EventResult::consumed();
                    }
                    _ => {}
                }
            }

            if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                if contains(layout.divider, local_x, local_y) {
                    let grab_offset = match self.orientation {
                        SplitterOrientation::Vertical => local_x.saturating_sub(layout.divider.x),
                        SplitterOrientation::Horizontal => local_y.saturating_sub(layout.divider.y),
                    };
                    self.drag = Some(DragState { grab_offset });
                    self.split_auto = false;
                    return EventResult::consumed();
                }
            }

            let (child_idx, child_bounds) = if contains(layout.first, local_x, local_y) {
                (Some(0usize), layout.first)
            } else if contains(layout.second, local_x, local_y) {
                (Some(1usize), layout.second)
            } else {
                (None, Rect::default())
            };

            let Some(child_idx) = child_idx else {
                return EventResult::ignored();
            };
            if child_idx >= self.children.len() {
                return EventResult::ignored();
            }

            let child_id = self.children[child_idx].id;
            let child_x = local_x.saturating_sub(child_bounds.x);
            let child_y = local_y.saturating_sub(child_bounds.y);

            let focus_changed = matches!(m.kind, MouseEventKind::Down(_))
                && self.children[child_idx].view.is_focusable()
                && self.focused != Some(child_id);
            if focus_changed {
                self.focused = Some(child_id);
            }

            let child_event = Event::Mouse(MouseEvent {
                column: child_x,
                row: child_y,
                ..*m
            });
            let child_focused = ctx.is_focused && self.focused == Some(child_id);
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
            };

            let res = self.children[child_idx]
                .view
                .handle_event(&child_event, child_ctx);
            if res.is_consumed() {
                return res;
            }

            if focus_changed {
                return EventResult::consumed();
            }

            return EventResult::ignored();
        }

        if let Some(child_id) = self.focused.or_else(|| self.first_focusable_child())
            && let Some(child_idx) = self.children.iter().position(|c| c.id == child_id)
        {
            self.focused = Some(child_id);
            let child_focused = ctx.is_focused && self.focused == Some(child_id);
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
            };
            let res = self.children[child_idx].view.handle_event(event, child_ctx);
            if res.is_consumed() {
                return res;
            }
        }

        EventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if area.width == 0 || area.height == 0 {
            self.last_layout = Some(SplitLayout {
                area,
                ..SplitLayout::default()
            });
            return;
        }

        let layout = self.layout_for_area(area);
        self.last_layout = Some(layout);

        if let Some(first) = self.children.get_mut(0) {
            first.set_bounds(layout.first);
        }
        if let Some(second) = self.children.get_mut(1) {
            second.set_bounds(layout.second);
        }

        if let Some(first) = self.children.get_mut(0) {
            if layout.first.width > 0 && layout.first.height > 0 {
                let abs = Rect {
                    x: area.x.saturating_add(layout.first.x),
                    y: area.y.saturating_add(layout.first.y),
                    width: layout.first.width,
                    height: layout.first.height,
                };
                let child_focused = ctx.is_focused && self.focused == Some(first.id);
                let child_ctx = ComponentContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: child_focused,
                    scrollbar_host: ctx.scrollbar_host.for_child(),
                    tab_mode: ctx.tab_mode.for_child(),
                };
                first.view.draw(frame, abs, child_ctx);
            }
        }

        if let Some(second) = self.children.get_mut(1) {
            if layout.second.width > 0 && layout.second.height > 0 {
                let abs = Rect {
                    x: area.x.saturating_add(layout.second.x),
                    y: area.y.saturating_add(layout.second.y),
                    width: layout.second.width,
                    height: layout.second.height,
                };
                let child_focused = ctx.is_focused && self.focused == Some(second.id);
                let child_ctx = ComponentContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: child_focused,
                    scrollbar_host: ctx.scrollbar_host.for_child(),
                    tab_mode: ctx.tab_mode.for_child(),
                };
                second.view.draw(frame, abs, child_ctx);
            }
        }

        if self.border.get() && layout.divider.width > 0 && layout.divider.height > 0 {
            let style = self.border_style.unwrap_or(ctx.theme.widget.dim);
            let border_set = ctx.theme.border_set(false);
            let symbol = match self.orientation {
                SplitterOrientation::Vertical => border_set.vertical_left,
                SplitterOrientation::Horizontal => border_set.horizontal_top,
            };

            let buf = frame.buffer_mut();
            for dy in 0..layout.divider.height {
                for dx in 0..layout.divider.width {
                    let x = area.x.saturating_add(layout.divider.x).saturating_add(dx);
                    let y = area.y.saturating_add(layout.divider.y).saturating_add(dy);
                    buf[(x, y)].set_symbol(symbol).set_style(style);
                }
            }
        }
    }
}
