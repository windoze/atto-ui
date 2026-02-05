use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use std::cmp::Ordering;

use crate::reactive::Binding;
use super::component::{Component, ComponentContext, EventResult, ScrollbarHost, TabMode};
use super::layout::{Align, Anchor, EdgeInsets, LayoutParams, Size, add_signed, apply_padding};
use super::node::{ComponentId, ComponentNode};
use super::scroll::{
    ScrollConfig, ScrollOffset, ScrollbarDrag, ScrollbarHit, Scrollbars, clamp_scroll_offset,
    max_scroll_offset, scroll_offset_from_thumb_start, scrollbar_hit_test, scrollbar_layout_1d,
    should_show_scrollbar,
};

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

fn clamp_u16(v: u16, min: u16, max: u16) -> u16 {
    if v < min {
        min
    } else if v > max {
        max
    } else {
        v
    }
}

fn mouse_coords_local_to_area(area: Rect, m: MouseEvent) -> Option<(u16, u16)> {
    if contains(area, m.column, m.row) {
        return Some((
            m.column.saturating_sub(area.x),
            m.row.saturating_sub(area.y),
        ));
    }

    // Nested containers receive mouse coordinates already relative to their own origin.
    if m.column < area.width && m.row < area.height {
        return Some((m.column, m.row));
    }

    None
}

fn position_anchored(
    content_size: (u16, u16),
    size: (u16, u16),
    anchor: Anchor,
    offset_x: i16,
    offset_y: i16,
) -> Rect {
    let (content_w, content_h) = content_size;
    let (w, h) = size;

    let base_x = match anchor {
        Anchor::TopLeft | Anchor::Left | Anchor::BottomLeft => 0,
        Anchor::TopRight | Anchor::Right | Anchor::BottomRight => content_w.saturating_sub(w),
        Anchor::Top | Anchor::Bottom | Anchor::Center => content_w.saturating_sub(w) / 2,
    };
    let base_y = match anchor {
        Anchor::TopLeft | Anchor::Top | Anchor::TopRight => 0,
        Anchor::BottomLeft | Anchor::Bottom | Anchor::BottomRight => content_h.saturating_sub(h),
        Anchor::Left | Anchor::Right | Anchor::Center => content_h.saturating_sub(h) / 2,
    };

    let x = add_signed(base_x, offset_x);
    let y = add_signed(base_y, offset_y);

    let max_x = content_w.saturating_sub(w);
    let max_y = content_h.saturating_sub(h);

    Rect {
        x: clamp_u16(x, 0, max_x),
        y: clamp_u16(y, 0, max_y),
        width: w,
        height: h,
    }
}

fn align_within(slot: Rect, desired: (u16, u16), align_x: Align, align_y: Align) -> Rect {
    let (desired_w, desired_h) = desired;

    let w = match align_x {
        Align::Stretch => slot.width,
        _ => desired_w.min(slot.width),
    };
    let h = match align_y {
        Align::Stretch => slot.height,
        _ => desired_h.min(slot.height),
    };

    let dx = slot.width.saturating_sub(w);
    let dy = slot.height.saturating_sub(h);

    let off_x = match align_x {
        Align::Start | Align::Stretch => 0,
        Align::Center => dx / 2,
        Align::End => dx,
    };
    let off_y = match align_y {
        Align::Start | Align::Stretch => 0,
        Align::Center => dy / 2,
        Align::End => dy,
    };

    Rect {
        x: slot.x.saturating_add(off_x),
        y: slot.y.saturating_add(off_y),
        width: w,
        height: h,
    }
}

fn desired_size_for_slot(view: &dyn Component, slot: Rect, layout: LayoutParams) -> (u16, u16) {
    let min_w = view.min_width();
    let min_h = view.min_height();
    let w = match layout.width {
        Size::Fixed(w) => w,
        Size::Content => view.desired_width().unwrap_or(slot.width),
        Size::Fill | Size::Weight(_) => view.desired_width().unwrap_or(slot.width),
    };
    let h = match layout.height {
        Size::Fixed(h) => h,
        Size::Content => view.desired_height().unwrap_or(slot.height),
        Size::Fill | Size::Weight(_) => view.desired_height().unwrap_or(slot.height),
    };
    (w.max(min_w), h.max(min_h))
}

pub struct VStack {
    id: ComponentId,
    children: Vec<ComponentNode>,
    padding: Binding<EdgeInsets>,
    spacing: Binding<u16>,
    focused: Option<ComponentId>,
    last_area: Option<Rect>,
    scrollable: Binding<bool>,
    scroll: Binding<ScrollOffset>,
    content_size: (u16, u16),
    viewport_size: (u16, u16),
    scroll_config: Binding<ScrollConfig>,
    scrollbars: Option<Scrollbars>,
    scrollbar_drag: Option<ScrollbarDrag>,
}

impl Default for VStack {
    fn default() -> Self {
        Self::new()
    }
}

impl VStack {
    pub fn new() -> Self {
        Self {
            id: ComponentId::next(),
            children: Vec::new(),
            padding: EdgeInsets::ZERO.into(),
            spacing: 0u16.into(),
            focused: None,
            last_area: None,
            scrollable: false.into(),
            scroll: ScrollOffset::ZERO.into(),
            content_size: (0, 0),
            viewport_size: (0, 0),
            scroll_config: ScrollConfig::default().into(),
            scrollbars: None,
            scrollbar_drag: None,
        }
    }

    pub fn with_padding(mut self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.padding = padding.into();
        self
    }

    pub fn with_spacing(mut self, spacing: impl Into<Binding<u16>>) -> Self {
        self.spacing = spacing.into();
        self
    }

    pub fn with_scrollable(mut self, scrollable: impl Into<Binding<bool>>) -> Self {
        self.scrollable = scrollable.into();
        if !self.scrollable.get() {
            self.scroll.set(ScrollOffset::ZERO);
        }
        self
    }

    pub fn with_scroll_config(mut self, config: impl Into<Binding<ScrollConfig>>) -> Self {
        self.scroll_config = config.into();
        self
    }

    pub fn spacing(self, spacing: impl Into<Binding<u16>>) -> Self {
        self.with_spacing(spacing)
    }

    pub fn padding(self, padding: u16) -> Self {
        self.with_padding(EdgeInsets::all(padding))
    }

    pub fn padding_insets(self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.with_padding(padding)
    }

    pub fn scrollable(self, scrollable: impl Into<Binding<bool>>) -> Self {
        self.with_scrollable(scrollable)
    }

    pub fn scroll_config(self, config: impl Into<Binding<ScrollConfig>>) -> Self {
        self.with_scroll_config(config)
    }

    pub fn child(mut self, view: impl Component + 'static) -> Self {
        self.add_child_with_layout(Box::new(view), LayoutParams::default());
        self
    }

    pub fn child_with_layout(
        mut self,
        view: impl Component + 'static,
        layout: LayoutParams,
    ) -> Self {
        self.add_child_with_layout(Box::new(view), layout);
        self
    }

    pub fn add_child_with_layout(&mut self, view: Box<dyn Component>, layout: LayoutParams) -> ComponentId {
        let mut node = ComponentNode::new(view).with_layout(layout);
        node.parent = Some(self.id);
        let id = node.id;
        if self.focused.is_none() && node.view.is_focusable() {
            self.focused = Some(id);
        }
        self.children.push(node);
        id
    }

    pub fn replace_children(&mut self, mut children: Vec<ComponentNode>) {
        for child in children.iter_mut() {
            child.parent = Some(self.id);
        }

        self.children = children;

        let focused_valid = self.focused.is_some_and(|id| {
            self.children
                .iter()
                .any(|child| child.id == id && child.view.is_focusable())
        });

        if !focused_valid {
            self.focused = self.first_focusable_child();
        }

        // Any in-progress scrollbar drag is no longer valid after restructuring children.
        self.scrollbar_drag = None;
    }

    fn first_focusable_child(&self) -> Option<ComponentId> {
        focusable_children_in_tab_order(&self.children)
            .first()
            .copied()
    }

    fn move_focus(&mut self, direction: TabDirection, wrap: bool) -> bool {
        let focusable = focusable_children_in_tab_order(&self.children);
        if focusable.is_empty() {
            self.focused = None;
            return false;
        }

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
            return false;
        };

        self.focused = Some(id);
        true
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

        // If we don't have a focused child yet, initialize focus and stop.
        let focusable = focusable_children_in_tab_order(&self.children);
        if focusable.is_empty() {
            self.focused = None;
            return EventResult::ignored();
        }

        let focused = match self.focused {
            Some(id) if focusable.contains(&id) => id,
            _ => {
                let id = match direction {
                    TabDirection::Next => focusable[0],
                    TabDirection::Prev => focusable[focusable.len() - 1],
                };
                self.focused = Some(id);
                self.focus_focused_child_edge(direction);
                return EventResult::consumed();
            }
        };

        // Give the currently focused child a chance to advance focus within its subtree.
        if let Some(child_idx) = self.children.iter().position(|c| c.id == focused) {
            let child_focused = ctx.is_focused && self.focused == Some(focused);
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
            };

            let res = self.children[child_idx]
                .view
                .handle_event_capture(event, child_ctx);
            if res.is_consumed() {
                return res;
            }
        }

        let wrap = matches!(ctx.tab_mode, TabMode::Cycle);
        if self.move_focus(direction, wrap) {
            self.focus_focused_child_edge(direction);
            return EventResult::consumed();
        }

        EventResult::ignored()
    }

    fn scroll_by(&mut self, dx: i16, dy: i16) -> bool {
        if !self.scrollable.get() {
            return false;
        }

        let scroll = self.scroll.get();
        let desired = ScrollOffset {
            x: add_signed(scroll.x, dx),
            y: add_signed(scroll.y, dy),
        };
        let clamped = clamp_scroll_offset(self.content_size, self.viewport_size, desired);
        let changed = clamped != scroll;
        self.scroll.set(clamped);
        changed
    }

    fn scroll_to_clamped(&mut self, x: u16, y: u16) -> bool {
        if !self.scrollable.get() {
            return false;
        }
        let scroll = self.scroll.get();
        let desired = ScrollOffset { x, y };
        let clamped = clamp_scroll_offset(self.content_size, self.viewport_size, desired);
        let changed = clamped != scroll;
        self.scroll.set(clamped);
        changed
    }

    fn bounds_fully_visible(bounds: Rect, scroll: ScrollOffset, viewport: (u16, u16)) -> bool {
        if viewport.0 == 0 || viewport.1 == 0 {
            return false;
        }
        let x0 = bounds.x;
        let y0 = bounds.y;
        let x1 = bounds.x.saturating_add(bounds.width);
        let y1 = bounds.y.saturating_add(bounds.height);

        let vx0 = scroll.x;
        let vy0 = scroll.y;
        let vx1 = scroll.x.saturating_add(viewport.0);
        let vy1 = scroll.y.saturating_add(viewport.1);

        x0 >= vx0 && y0 >= vy0 && x1 <= vx1 && y1 <= vy1
    }

    fn hit_test_child_scrolled(
        &self,
        viewport_x: u16,
        viewport_y: u16,
        viewport: (u16, u16),
    ) -> Option<ComponentId> {
        // Anchored children are treated as overlays and do not scroll.
        for child in self
            .children
            .iter()
            .rev()
            .filter(|c| c.layout.anchor.is_some())
        {
            if contains(child.bounds(), viewport_x, viewport_y) {
                return Some(child.id);
            }
        }

        let scroll = self.scroll.get();
        let content_x = viewport_x.saturating_add(scroll.x);
        let content_y = viewport_y.saturating_add(scroll.y);

        for child in self
            .children
            .iter()
            .rev()
            .filter(|c| c.layout.anchor.is_none())
        {
            if !Self::bounds_fully_visible(child.bounds(), scroll, viewport) {
                continue;
            }
            if contains(child.bounds(), content_x, content_y) {
                return Some(child.id);
            }
        }
        None
    }

    fn desired_height_flow(&self) -> u16 {
        let spacing = self.spacing.get();
        let padding = self.padding.get();

        let mut total: u16 = padding.top.saturating_add(padding.bottom);
        let mut first_flow = true;

        for child in self.children.iter().filter(|c| c.layout.anchor.is_none()) {
            if !first_flow {
                total = total.saturating_add(spacing);
            }
            first_flow = false;

            let margin = child.layout.margin;
            total = total
                .saturating_add(margin.top)
                .saturating_add(margin.bottom);

            let min_h = child.view.min_height();
            let h = match child.layout.height {
                Size::Fixed(h) => h,
                Size::Content => child.view.desired_height().unwrap_or(1),
                Size::Fill | Size::Weight(_) => min_h,
            }
            .max(min_h);

            total = total.saturating_add(h);
        }

        total
    }

    fn min_width_flow(&self) -> u16 {
        let padding = self.padding.get();

        let mut max_child: u16 = 0;
        for child in self.children.iter().filter(|c| c.layout.anchor.is_none()) {
            let margin = child.layout.margin;
            let min_w = child.view.min_width();
            let required_w = match child.layout.width {
                Size::Fixed(w) => w.max(min_w),
                Size::Content | Size::Fill | Size::Weight(_) => min_w,
            };

            let outer_w = margin
                .left
                .saturating_add(required_w)
                .saturating_add(margin.right);
            max_child = max_child.max(outer_w);
        }

        padding
            .left
            .saturating_add(padding.right)
            .saturating_add(max_child)
    }

    fn min_height_flow(&self) -> u16 {
        let spacing = self.spacing.get();
        let padding = self.padding.get();
        let scrollable = self.scrollable.get();

        if scrollable {
            let mut max_child: u16 = 0;
            for child in self.children.iter().filter(|c| c.layout.anchor.is_none()) {
                let margin = child.layout.margin;
                let min_h = child.view.min_height();
                let required_h = match child.layout.height {
                    Size::Fixed(h) => h.max(min_h),
                    Size::Content | Size::Fill | Size::Weight(_) => min_h,
                };

                let outer_h = margin
                    .top
                    .saturating_add(required_h)
                    .saturating_add(margin.bottom);
                max_child = max_child.max(outer_h);
            }

            return padding
                .top
                .saturating_add(padding.bottom)
                .saturating_add(max_child);
        }

        let mut total: u16 = padding.top.saturating_add(padding.bottom);
        let mut first_flow = true;

        for child in self.children.iter().filter(|c| c.layout.anchor.is_none()) {
            if !first_flow {
                total = total.saturating_add(spacing);
            }
            first_flow = false;

            let margin = child.layout.margin;
            total = total
                .saturating_add(margin.top)
                .saturating_add(margin.bottom);

            let min_h = child.view.min_height();
            let required_h = match child.layout.height {
                Size::Fixed(h) => h.max(min_h),
                Size::Content | Size::Fill | Size::Weight(_) => min_h,
            };

            total = total.saturating_add(required_h);
        }

        total
    }

    fn layout_children(&mut self, viewport_size: (u16, u16)) -> (u16, u16) {
        let (content_w, content_h) = viewport_size;
        let spacing = self.spacing.get();
        let scrollable = self.scrollable.get();

        #[derive(Clone, Copy, Debug)]
        enum HeightSpec {
            /// Fixed height (cannot shrink).
            Fixed(u16),
            /// Content-sized height (can shrink down to `min` when constrained).
            Content { min: u16, desired: u16 },
            /// Flexible height (takes remaining space), with an enforced minimum.
            Weight { weight: u16, min: u16 },
        }

        let mut specs: Vec<Option<HeightSpec>> = vec![None; self.children.len()];
        let mut margin_total: u16 = 0;
        let mut flow_count = 0usize;

        for (idx, child) in self.children.iter().enumerate() {
            if child.layout.anchor.is_some() {
                continue;
            }
            flow_count += 1;

            let margin = child.layout.margin;
            margin_total = margin_total
                .saturating_add(margin.top)
                .saturating_add(margin.bottom);

            let min_h = child.view.min_height();
            let spec = match child.layout.height {
                Size::Fixed(h) => HeightSpec::Fixed(h.max(min_h)),
                Size::Content => {
                    let desired = child.view.desired_height().unwrap_or(1).max(min_h);
                    HeightSpec::Content {
                        min: min_h,
                        desired,
                    }
                }
                Size::Weight(w) => HeightSpec::Weight {
                    weight: w.max(1),
                    min: min_h,
                },
                Size::Fill => HeightSpec::Weight {
                    weight: 1,
                    min: min_h,
                },
            };

            specs[idx] = Some(spec);
        }

        if flow_count >= 2 && spacing > 0 {
            margin_total =
                margin_total.saturating_add(spacing.saturating_mul(flow_count as u16 - 1));
        }

        let mut allocations: Vec<u16> = vec![0; self.children.len()];

        if scrollable {
            let mut fixed_total: u16 = 0;
            let mut weight_total: u16 = 0;

            for (idx, spec) in specs.iter().enumerate() {
                let Some(spec) = spec else {
                    continue;
                };

                match *spec {
                    HeightSpec::Fixed(h) => {
                        allocations[idx] = h;
                        fixed_total = fixed_total.saturating_add(h);
                    }
                    HeightSpec::Content { desired, .. } => {
                        allocations[idx] = desired;
                        fixed_total = fixed_total.saturating_add(desired);
                    }
                    HeightSpec::Weight { weight, min } => {
                        allocations[idx] = min;
                        fixed_total = fixed_total.saturating_add(min);
                        weight_total = weight_total.saturating_add(weight);
                    }
                }
            }

            let available = content_h
                .saturating_sub(margin_total)
                .saturating_sub(fixed_total);
            let mut remaining = available;

            if weight_total > 0 && remaining > 0 {
                let mut used: u16 = 0;
                for (idx, spec) in specs.iter().enumerate() {
                    let Some(HeightSpec::Weight { weight: w, .. }) = spec else {
                        continue;
                    };
                    let share = ((remaining as u32) * (*w as u32) / (weight_total as u32))
                        .min(u16::MAX as u32) as u16;
                    allocations[idx] = allocations[idx].saturating_add(share);
                    used = used.saturating_add(share);
                }
                remaining = remaining.saturating_sub(used);

                // Distribute any leftover 1-row remainders deterministically.
                if remaining > 0 {
                    for (idx, spec) in specs.iter().enumerate() {
                        if remaining == 0 {
                            break;
                        }
                        if matches!(spec, Some(HeightSpec::Weight { .. })) {
                            allocations[idx] = allocations[idx].saturating_add(1);
                            remaining = remaining.saturating_sub(1);
                        }
                    }
                }
            }
        } else {
            let mut min_total: u16 = 0;
            let mut weight_total: u16 = 0;
            let mut content_extras: Vec<(usize, u16)> = Vec::new();

            for (idx, spec) in specs.iter().enumerate() {
                let Some(spec) = spec else {
                    continue;
                };

                match *spec {
                    HeightSpec::Fixed(h) => {
                        allocations[idx] = h;
                        min_total = min_total.saturating_add(h);
                    }
                    HeightSpec::Content { min, desired } => {
                        allocations[idx] = min;
                        min_total = min_total.saturating_add(min);
                        content_extras.push((idx, desired.saturating_sub(min)));
                    }
                    HeightSpec::Weight { weight, min } => {
                        allocations[idx] = min;
                        min_total = min_total.saturating_add(min);
                        weight_total = weight_total.saturating_add(weight);
                    }
                }
            }

            let available_for_children = content_h.saturating_sub(margin_total);
            let mut remaining = available_for_children.saturating_sub(min_total);

            // First, satisfy content views up to their desired size.
            for (idx, needed) in content_extras {
                if remaining == 0 {
                    break;
                }
                let extra = needed.min(remaining);
                allocations[idx] = allocations[idx].saturating_add(extra);
                remaining = remaining.saturating_sub(extra);
            }

            // Then distribute any leftover space across weight/fill children.
            if weight_total > 0 && remaining > 0 {
                let mut used: u16 = 0;
                for (idx, spec) in specs.iter().enumerate() {
                    let Some(HeightSpec::Weight { weight: w, .. }) = spec else {
                        continue;
                    };
                    let share = ((remaining as u32) * (*w as u32) / (weight_total as u32))
                        .min(u16::MAX as u32) as u16;
                    allocations[idx] = allocations[idx].saturating_add(share);
                    used = used.saturating_add(share);
                }

                let mut leftover = remaining.saturating_sub(used);
                if leftover > 0 {
                    for (idx, spec) in specs.iter().enumerate() {
                        if leftover == 0 {
                            break;
                        }
                        if matches!(spec, Some(HeightSpec::Weight { .. })) {
                            allocations[idx] = allocations[idx].saturating_add(1);
                            leftover = leftover.saturating_sub(1);
                        }
                    }
                }
            }
        }

        let mut cursor_y: u16 = 0;
        let mut first_flow = true;
        let mut out_of_space = false;

        for (idx, child) in self.children.iter_mut().enumerate() {
            if let Some(anchor) = child.layout.anchor {
                let desired_w = match child.layout.width {
                    Size::Fixed(w) => w,
                    Size::Content => child.view.desired_width().unwrap_or(content_w),
                    Size::Fill | Size::Weight(_) => child.view.desired_width().unwrap_or(content_w),
                }
                .min(content_w);
                let desired_h = match child.layout.height {
                    Size::Fixed(h) => h,
                    Size::Content => child.view.desired_height().unwrap_or(1),
                    Size::Fill | Size::Weight(_) => child.view.desired_height().unwrap_or(1),
                }
                .min(content_h);

                let (min_w, min_h) = child.view.min_size();
                if content_w < min_w || content_h < min_h {
                    child.set_bounds(Rect::default());
                    continue;
                }

                child.set_bounds(position_anchored(
                    viewport_size,
                    (desired_w.max(min_w), desired_h.max(min_h)),
                    anchor.anchor,
                    anchor.offset_x,
                    anchor.offset_y,
                ));
                continue;
            }

            if !first_flow && spacing > 0 {
                cursor_y = cursor_y.saturating_add(spacing);
            }
            first_flow = false;

            let margin = child.layout.margin;
            cursor_y = cursor_y.saturating_add(margin.top);

            if !scrollable && cursor_y >= content_h {
                child.set_bounds(Rect::default());
                continue;
            }

            if out_of_space {
                child.set_bounds(Rect::default());
                continue;
            }

            let slot_h = allocations[idx];

            let max_h = content_h.saturating_sub(cursor_y);
            let available_h = max_h.saturating_sub(margin.bottom);
            let available_w = content_w.saturating_sub(margin.left.saturating_add(margin.right));
            let min_w = child.view.min_width();
            if available_w < min_w {
                child.set_bounds(Rect::default());
                continue;
            }

            let required_h = child.view.min_height();
            if !scrollable && available_h < required_h {
                child.set_bounds(Rect::default());
                out_of_space = true;
                continue;
            }

            let h = if scrollable {
                slot_h
            } else {
                slot_h.min(available_h)
            };
            if h == 0 {
                child.set_bounds(Rect::default());
                continue;
            }

            let slot = Rect {
                x: margin.left,
                y: cursor_y,
                width: available_w,
                height: h,
            };

            let desired = desired_size_for_slot(child.view.as_ref(), slot, child.layout);
            let aligned = align_within(slot, desired, child.layout.align_x, child.layout.align_y);
            child.set_bounds(aligned);

            cursor_y = cursor_y.saturating_add(h).saturating_add(margin.bottom);
        }

        (content_w, cursor_y)
    }
}

impl Component for VStack {
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
        self.min_width_flow()
    }

    fn min_height(&self) -> u16 {
        self.min_height_flow()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.desired_height_flow())
    }

    fn children(&self) -> &[ComponentNode] {
        &self.children
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        Some(&mut self.children)
    }

    fn is_scrollable(&self) -> bool {
        self.scrollable.get()
    }

    fn content_size(&self) -> (u16, u16) {
        self.content_size
    }

    fn scroll_offset(&self) -> (u16, u16) {
        let scroll = self.scroll.get();
        (scroll.x, scroll.y)
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.viewport_size
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.scroll_config.get()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        let _ = self.scroll_to_clamped(x, y);
    }

    fn scroll_to_child(&mut self, child_id: ComponentId) {
        let Some(node) = self.children.iter().find(|c| c.id == child_id) else {
            return;
        };
        if node.layout.anchor.is_some() {
            return;
        }

        let viewport = self.viewport_size;
        if viewport.0 == 0 || viewport.1 == 0 {
            return;
        }

        let bounds = node.bounds();
        let center_x = (bounds.x as u32).saturating_add((bounds.width as u32) / 2);
        let center_y = (bounds.y as u32).saturating_add((bounds.height as u32) / 2);

        let half_vw = (viewport.0 as u32) / 2;
        let half_vh = (viewport.1 as u32) / 2;

        let target_x = center_x.saturating_sub(half_vw).min(u16::MAX as u32) as u16;
        let target_y = center_y.saturating_sub(half_vh).min(u16::MAX as u32) as u16;
        let _ = self.scroll_to_clamped(target_x, target_y);
    }

    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let tab = self.handle_tab_navigation(event, ctx);
        if tab.is_consumed() {
            return tab;
        }

        EventResult::ignored()
    }

    fn handle_event_bubble(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if !self.scrollable.get() {
            return EventResult::ignored();
        }

        let cfg = self.scroll_config.get();
        match event {
            Event::Key(KeyEvent { code, kind, .. }) => {
                if matches!(kind, KeyEventKind::Release) {
                    return EventResult::ignored();
                }

                let viewport_h = self.viewport_size.1;
                let max = max_scroll_offset(self.content_size, self.viewport_size);

                let changed = match code {
                    KeyCode::Up => self.scroll_by(0, -1),
                    KeyCode::Down => self.scroll_by(0, 1),
                    KeyCode::Left => self.scroll_by(-1, 0),
                    KeyCode::Right => self.scroll_by(1, 0),
                    KeyCode::PageUp => self.scroll_by(0, -(viewport_h as i16)),
                    KeyCode::PageDown => self.scroll_by(0, viewport_h as i16),
                    KeyCode::Home => self.scroll_to_clamped(0, 0),
                    KeyCode::End => self.scroll_to_clamped(max.x, max.y),
                    _ => false,
                };

                if changed {
                    EventResult::consumed()
                } else {
                    EventResult::ignored()
                }
            }
            Event::Mouse(m) => {
                let Some(area) = self.last_area else {
                    return EventResult::ignored();
                };
                if mouse_coords_local_to_area(area, *m).is_none() {
                    return EventResult::ignored();
                }

                let step = cfg.wheel_step as i16;
                let changed = match m.kind {
                    MouseEventKind::ScrollUp => self.scroll_by(0, -step),
                    MouseEventKind::ScrollDown => self.scroll_by(0, step),
                    MouseEventKind::ScrollLeft => self.scroll_by(-step, 0),
                    MouseEventKind::ScrollRight => self.scroll_by(step, 0),
                    _ => false,
                };

                if changed {
                    EventResult::consumed()
                } else {
                    EventResult::ignored()
                }
            }
            _ => EventResult::ignored(),
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let capture = self.handle_event_capture(event, ctx);
        if capture.is_consumed() {
            return capture;
        }

        if let Event::Mouse(m) = event {
            let Some(area) = self.last_area else {
                return self.handle_event_bubble(event, ctx);
            };
            let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
                return self.handle_event_bubble(event, ctx);
            };

            let cfg = self.scroll_config.get();
            let padding = self.padding.get();
            let scrollbars = if let Some(scrollbars) = self.scrollbars {
                scrollbars
            } else {
                let viewport = Rect {
                    x: 0,
                    y: 0,
                    width: area.width,
                    height: area.height,
                };
                Scrollbars {
                    viewport,
                    content: apply_padding(viewport, padding),
                    vbar: None,
                    hbar: None,
                    thickness: cfg.scrollbar_thickness.max(1),
                }
            };

            if self.scrollable.get() {
                // If we started a thumb drag, keep consuming drag/up events.
                if let Some(drag) = self.scrollbar_drag {
                    let scroll = self.scroll.get();
                    match m.kind {
                        MouseEventKind::Drag(MouseButton::Left) => match drag {
                            ScrollbarDrag::Vertical { grab_offset } => {
                                let Some(vbar) = scrollbars.vbar else {
                                    self.scrollbar_drag = None;
                                    return EventResult::consumed();
                                };
                                if vbar.height == 0 {
                                    return EventResult::consumed();
                                }
                                let layout = scrollbar_layout_1d(
                                    vbar.height,
                                    scrollbars.content.height,
                                    self.content_size.1,
                                    scroll.y,
                                    cfg.arrows,
                                );
                                if layout.track_len == 0 {
                                    return EventResult::consumed();
                                }

                                let pos = local_y
                                    .saturating_sub(vbar.y)
                                    .min(vbar.height.saturating_sub(1));
                                let pos_in_track = pos
                                    .saturating_sub(layout.track_start)
                                    .min(layout.track_len.saturating_sub(1));

                                let max_start = layout.track_len.saturating_sub(layout.thumb_len);
                                let new_thumb_start =
                                    pos_in_track.saturating_sub(grab_offset).min(max_start);
                                let new_off = scroll_offset_from_thumb_start(
                                    layout.track_len,
                                    scrollbars.content.height,
                                    self.content_size.1,
                                    new_thumb_start,
                                );
                                let _ = self.scroll_to_clamped(scroll.x, new_off);
                                return EventResult::consumed();
                            }
                            ScrollbarDrag::Horizontal { grab_offset } => {
                                let Some(hbar) = scrollbars.hbar else {
                                    self.scrollbar_drag = None;
                                    return EventResult::consumed();
                                };
                                if hbar.width == 0 {
                                    return EventResult::consumed();
                                }
                                let layout = scrollbar_layout_1d(
                                    hbar.width,
                                    scrollbars.content.width,
                                    self.content_size.0,
                                    scroll.x,
                                    cfg.arrows,
                                );
                                if layout.track_len == 0 {
                                    return EventResult::consumed();
                                }

                                let pos = local_x
                                    .saturating_sub(hbar.x)
                                    .min(hbar.width.saturating_sub(1));
                                let pos_in_track = pos
                                    .saturating_sub(layout.track_start)
                                    .min(layout.track_len.saturating_sub(1));

                                let max_start = layout.track_len.saturating_sub(layout.thumb_len);
                                let new_thumb_start =
                                    pos_in_track.saturating_sub(grab_offset).min(max_start);
                                let new_off = scroll_offset_from_thumb_start(
                                    layout.track_len,
                                    scrollbars.content.width,
                                    self.content_size.0,
                                    new_thumb_start,
                                );
                                let _ = self.scroll_to_clamped(new_off, scroll.y);
                                return EventResult::consumed();
                            }
                        },
                        MouseEventKind::Up(MouseButton::Left) => {
                            self.scrollbar_drag = None;
                            return EventResult::consumed();
                        }
                        _ => {}
                    }
                }

                if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                    let scroll = self.scroll.get();
                    if let Some(vbar) = scrollbars.vbar
                        && contains(vbar, local_x, local_y)
                        && vbar.height > 0
                    {
                        let pos = local_y.saturating_sub(vbar.y);
                        let layout = scrollbar_layout_1d(
                            vbar.height,
                            scrollbars.content.height,
                            self.content_size.1,
                            scroll.y,
                            cfg.arrows,
                        );
                        match scrollbar_hit_test(layout, pos) {
                            ScrollbarHit::ArrowDec => {
                                let _ = self.scroll_by(0, -1);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::ArrowInc => {
                                let _ = self.scroll_by(0, 1);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::Thumb { grab_offset } => {
                                self.scrollbar_drag = Some(ScrollbarDrag::Vertical { grab_offset });
                                return EventResult::consumed();
                            }
                            ScrollbarHit::TrackDec => {
                                let page = scrollbars.content.height as i16;
                                let _ = self.scroll_by(0, -(page));
                                return EventResult::consumed();
                            }
                            ScrollbarHit::TrackInc => {
                                let page = scrollbars.content.height as i16;
                                let _ = self.scroll_by(0, page);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::None => {}
                        }
                    }

                    if let Some(hbar) = scrollbars.hbar
                        && contains(hbar, local_x, local_y)
                        && hbar.width > 0
                    {
                        let pos = local_x.saturating_sub(hbar.x);
                        let layout = scrollbar_layout_1d(
                            hbar.width,
                            scrollbars.content.width,
                            self.content_size.0,
                            scroll.x,
                            cfg.arrows,
                        );
                        match scrollbar_hit_test(layout, pos) {
                            ScrollbarHit::ArrowDec => {
                                let _ = self.scroll_by(-1, 0);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::ArrowInc => {
                                let _ = self.scroll_by(1, 0);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::Thumb { grab_offset } => {
                                self.scrollbar_drag =
                                    Some(ScrollbarDrag::Horizontal { grab_offset });
                                return EventResult::consumed();
                            }
                            ScrollbarHit::TrackDec => {
                                let page = scrollbars.content.width as i16;
                                let _ = self.scroll_by(-(page), 0);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::TrackInc => {
                                let page = scrollbars.content.width as i16;
                                let _ = self.scroll_by(page, 0);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::None => {}
                        }
                    }
                }
            }

            let content = scrollbars.content;
            if !contains(content, local_x, local_y) {
                return self.handle_event_bubble(event, ctx);
            }

            let content_x = local_x.saturating_sub(content.x);
            let content_y = local_y.saturating_sub(content.y);
            let content_size = (content.width, content.height);

            let Some(child_id) = self.hit_test_child_scrolled(content_x, content_y, content_size)
            else {
                return self.handle_event_bubble(event, ctx);
            };
            let Some(child_idx) = self.children.iter().position(|c| c.id == child_id) else {
                return self.handle_event_bubble(event, ctx);
            };

            let child_bounds = self.children[child_idx].bounds();
            let is_anchored = self.children[child_idx].layout.anchor.is_some();
            let scroll = self.scroll.get();
            let point_x = if is_anchored {
                content_x
            } else {
                content_x.saturating_add(scroll.x)
            };
            let point_y = if is_anchored {
                content_y
            } else {
                content_y.saturating_add(scroll.y)
            };
            let child_x = point_x.saturating_sub(child_bounds.x);
            let child_y = point_y.saturating_sub(child_bounds.y);

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

            return self.handle_event_bubble(event, ctx);
        }

        // Keyboard/paste/etc: send to focused child first.
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

        self.handle_event_bubble(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);

        let cfg = self.scroll_config.get();
        let padding = self.padding.get();
        let thickness = cfg.scrollbar_thickness.max(1);

        let mut viewport_outer = area;
        let mut inner = apply_padding(viewport_outer, padding);
        let mut show_v = false;
        let mut show_h = false;

        if self.scrollable.get() {
            if matches!(ctx.scrollbar_host, ScrollbarHost::Component) {
                for _ in 0..2 {
                    inner = apply_padding(viewport_outer, padding);
                    self.viewport_size = (inner.width, inner.height);
                    self.content_size = self.layout_children((inner.width, inner.height));

                    let new_show_v = should_show_scrollbar(
                        cfg.vertical_scrollbar,
                        self.content_size.1,
                        self.viewport_size.1,
                    );
                    let new_show_h = should_show_scrollbar(
                        cfg.horizontal_scrollbar,
                        self.content_size.0,
                        self.viewport_size.0,
                    );

                    if new_show_v == show_v && new_show_h == show_h {
                        break;
                    }

                    show_v = new_show_v;
                    show_h = new_show_h;

                    let v_thick = if show_v { thickness } else { 0 };
                    let h_thick = if show_h { thickness } else { 0 };
                    viewport_outer = Rect {
                        x: area.x,
                        y: area.y,
                        width: area.width.saturating_sub(v_thick),
                        height: area.height.saturating_sub(h_thick),
                    };
                }
            } else {
                viewport_outer = area;
                inner = apply_padding(area, padding);
                self.viewport_size = (inner.width, inner.height);
                self.content_size = self.layout_children((inner.width, inner.height));
                show_v = should_show_scrollbar(
                    cfg.vertical_scrollbar,
                    self.content_size.1,
                    self.viewport_size.1,
                );
                show_h = should_show_scrollbar(
                    cfg.horizontal_scrollbar,
                    self.content_size.0,
                    self.viewport_size.0,
                );
            }
        } else {
            inner = apply_padding(area, padding);
            self.viewport_size = (inner.width, inner.height);
            self.content_size = self.layout_children((inner.width, inner.height));
        }

        let scroll = self.scroll.get();
        self.scroll.set(clamp_scroll_offset(
            self.content_size,
            self.viewport_size,
            scroll,
        ));

        if self.scrollable.get() && matches!(ctx.scrollbar_host, ScrollbarHost::Component) {
            let viewport_local = Rect {
                x: viewport_outer.x.saturating_sub(area.x),
                y: viewport_outer.y.saturating_sub(area.y),
                width: viewport_outer.width,
                height: viewport_outer.height,
            };
            let content_local = apply_padding(viewport_local, padding);
            let vbar = show_v.then_some(Rect {
                x: viewport_local.x.saturating_add(viewport_local.width),
                y: viewport_local.y,
                width: thickness,
                height: viewport_local.height,
            });
            let hbar = show_h.then_some(Rect {
                x: viewport_local.x,
                y: viewport_local.y.saturating_add(viewport_local.height),
                width: viewport_local.width,
                height: thickness,
            });
            self.scrollbars = Some(Scrollbars {
                viewport: viewport_local,
                content: content_local,
                vbar,
                hbar,
                thickness,
            });
            if !show_v && !show_h {
                self.scrollbar_drag = None;
            }
        } else {
            self.scrollbars = None;
            self.scrollbar_drag = None;
        }

        let scrollable = self.scrollable.get();
        let scroll = self.scroll.get();
        let viewport = self.viewport_size;

        for child in self
            .children
            .iter_mut()
            .filter(|c| c.layout.anchor.is_none())
        {
            let r = child.bounds();
            if r.width == 0 || r.height == 0 {
                continue;
            }
            if scrollable && !Self::bounds_fully_visible(r, scroll, viewport) {
                continue;
            }
            let abs = Rect {
                x: inner.x.saturating_add(r.x.saturating_sub(scroll.x)),
                y: inner.y.saturating_add(r.y.saturating_sub(scroll.y)),
                width: r.width,
                height: r.height,
            };

            let child_focused = ctx.is_focused && self.focused == Some(child.id);
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
            };
            child.view.draw(frame, abs, child_ctx);
        }

        for child in self
            .children
            .iter_mut()
            .filter(|c| c.layout.anchor.is_some())
        {
            let r = child.bounds();
            if r.width == 0 || r.height == 0 {
                continue;
            }
            let abs = Rect {
                x: inner.x.saturating_add(r.x),
                y: inner.y.saturating_add(r.y),
                width: r.width,
                height: r.height,
            };

            let child_focused = ctx.is_focused && self.focused == Some(child.id);
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
            };
            child.view.draw(frame, abs, child_ctx);
        }

        let Some(scrollbars) = self.scrollbars else {
            return;
        };

        let track_style = ctx.theme.scrollbar_track;
        let thumb_style = ctx.theme.scrollbar_thumb;
        let arrow_style = ctx.theme.scrollbar_arrow;
        let buf = frame.buffer_mut();

        let track = ctx.theme.glyph("scrollbar-track").unwrap_or("░");
        let thumb = ctx.theme.glyph("scrollbar-thumb").unwrap_or("█");
        let arrow_up = ctx.theme.glyph("scrollbar-up-arrow").unwrap_or("▲");
        let arrow_down = ctx.theme.glyph("scrollbar-down-arrow").unwrap_or("▼");
        let arrow_left = ctx.theme.glyph("scrollbar-left-arrow").unwrap_or("◄");
        let arrow_right = ctx.theme.glyph("scrollbar-right-arrow").unwrap_or("►");

        if let Some(vbar) = scrollbars.vbar {
            let layout = scrollbar_layout_1d(
                vbar.height,
                viewport.1,
                self.content_size.1,
                scroll.y,
                cfg.arrows,
            );

            for dy in 0..vbar.height {
                let (symbol, style) = if layout.has_arrows && dy == 0 {
                    (arrow_up, arrow_style)
                } else if layout.has_arrows && dy == layout.bar_len.saturating_sub(1) {
                    (arrow_down, arrow_style)
                } else if dy >= layout.thumb_start
                    && dy < layout.thumb_start.saturating_add(layout.thumb_len)
                {
                    (thumb, thumb_style)
                } else {
                    (track, track_style)
                };
                for dx in 0..vbar.width {
                    buf[(
                        area.x.saturating_add(vbar.x).saturating_add(dx),
                        area.y.saturating_add(vbar.y).saturating_add(dy),
                    )]
                        .set_symbol(symbol)
                        .set_style(style);
                }
            }
        }

        if let Some(hbar) = scrollbars.hbar {
            let layout = scrollbar_layout_1d(
                hbar.width,
                viewport.0,
                self.content_size.0,
                scroll.x,
                cfg.arrows,
            );

            for dx in 0..hbar.width {
                let (symbol, style) = if layout.has_arrows && dx == 0 {
                    (arrow_left, arrow_style)
                } else if layout.has_arrows && dx == layout.bar_len.saturating_sub(1) {
                    (arrow_right, arrow_style)
                } else if dx >= layout.thumb_start
                    && dx < layout.thumb_start.saturating_add(layout.thumb_len)
                {
                    (thumb, thumb_style)
                } else {
                    (track, track_style)
                };
                for dy in 0..hbar.height {
                    buf[(
                        area.x.saturating_add(hbar.x).saturating_add(dx),
                        area.y.saturating_add(hbar.y).saturating_add(dy),
                    )]
                        .set_symbol(symbol)
                        .set_style(style);
                }
            }
        }

        if let (Some(vbar), Some(hbar)) = (scrollbars.vbar, scrollbars.hbar) {
            let corner = Rect {
                x: vbar.x,
                y: hbar.y,
                width: vbar.width,
                height: hbar.height,
            };
            for dy in 0..corner.height {
                for dx in 0..corner.width {
                    buf[(
                        area.x.saturating_add(corner.x).saturating_add(dx),
                        area.y.saturating_add(corner.y).saturating_add(dy),
                    )]
                        .set_symbol(track)
                        .set_style(track_style);
                }
            }
        }
    }
}

pub struct HStack {
    id: ComponentId,
    children: Vec<ComponentNode>,
    padding: Binding<EdgeInsets>,
    spacing: Binding<u16>,
    focused: Option<ComponentId>,
    last_area: Option<Rect>,
    scrollable: Binding<bool>,
    scroll: Binding<ScrollOffset>,
    content_size: (u16, u16),
    viewport_size: (u16, u16),
    scroll_config: Binding<ScrollConfig>,
    scrollbars: Option<Scrollbars>,
    scrollbar_drag: Option<ScrollbarDrag>,
}

impl Default for HStack {
    fn default() -> Self {
        Self::new()
    }
}

impl HStack {
    pub fn new() -> Self {
        Self {
            id: ComponentId::next(),
            children: Vec::new(),
            padding: EdgeInsets::ZERO.into(),
            spacing: 0u16.into(),
            focused: None,
            last_area: None,
            scrollable: false.into(),
            scroll: ScrollOffset::ZERO.into(),
            content_size: (0, 0),
            viewport_size: (0, 0),
            scroll_config: ScrollConfig::default().into(),
            scrollbars: None,
            scrollbar_drag: None,
        }
    }

    pub fn with_padding(mut self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.padding = padding.into();
        self
    }

    pub fn with_spacing(mut self, spacing: impl Into<Binding<u16>>) -> Self {
        self.spacing = spacing.into();
        self
    }

    pub fn with_scrollable(mut self, scrollable: impl Into<Binding<bool>>) -> Self {
        self.scrollable = scrollable.into();
        if !self.scrollable.get() {
            self.scroll.set(ScrollOffset::ZERO);
        }
        self
    }

    pub fn with_scroll_config(mut self, config: impl Into<Binding<ScrollConfig>>) -> Self {
        self.scroll_config = config.into();
        self
    }

    pub fn spacing(self, spacing: impl Into<Binding<u16>>) -> Self {
        self.with_spacing(spacing)
    }

    pub fn padding(self, padding: u16) -> Self {
        self.with_padding(EdgeInsets::all(padding))
    }

    pub fn padding_insets(self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.with_padding(padding)
    }

    pub fn scrollable(self, scrollable: impl Into<Binding<bool>>) -> Self {
        self.with_scrollable(scrollable)
    }

    pub fn scroll_config(self, config: impl Into<Binding<ScrollConfig>>) -> Self {
        self.with_scroll_config(config)
    }

    pub fn child(mut self, view: impl Component + 'static) -> Self {
        self.add_child_with_layout(Box::new(view), LayoutParams::default());
        self
    }

    pub fn child_with_layout(
        mut self,
        view: impl Component + 'static,
        layout: LayoutParams,
    ) -> Self {
        self.add_child_with_layout(Box::new(view), layout);
        self
    }

    pub fn add_child_with_layout(&mut self, view: Box<dyn Component>, layout: LayoutParams) -> ComponentId {
        let mut node = ComponentNode::new(view).with_layout(layout);
        node.parent = Some(self.id);
        let id = node.id;
        if self.focused.is_none() && node.view.is_focusable() {
            self.focused = Some(id);
        }
        self.children.push(node);
        id
    }

    fn first_focusable_child(&self) -> Option<ComponentId> {
        focusable_children_in_tab_order(&self.children)
            .first()
            .copied()
    }

    fn move_focus(&mut self, direction: TabDirection, wrap: bool) -> bool {
        let focusable = focusable_children_in_tab_order(&self.children);
        if focusable.is_empty() {
            self.focused = None;
            return false;
        }

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
            return false;
        };

        self.focused = Some(id);
        true
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

        // If we don't have a focused child yet, initialize focus and stop.
        let focusable = focusable_children_in_tab_order(&self.children);
        if focusable.is_empty() {
            self.focused = None;
            return EventResult::ignored();
        }

        let focused = match self.focused {
            Some(id) if focusable.contains(&id) => id,
            _ => {
                let id = match direction {
                    TabDirection::Next => focusable[0],
                    TabDirection::Prev => focusable[focusable.len() - 1],
                };
                self.focused = Some(id);
                self.focus_focused_child_edge(direction);
                return EventResult::consumed();
            }
        };

        // Give the currently focused child a chance to advance focus within its subtree.
        if let Some(child_idx) = self.children.iter().position(|c| c.id == focused) {
            let child_focused = ctx.is_focused && self.focused == Some(focused);
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
            };

            let res = self.children[child_idx]
                .view
                .handle_event_capture(event, child_ctx);
            if res.is_consumed() {
                return res;
            }
        }

        let wrap = matches!(ctx.tab_mode, TabMode::Cycle);
        if self.move_focus(direction, wrap) {
            self.focus_focused_child_edge(direction);
            return EventResult::consumed();
        }

        EventResult::ignored()
    }

    fn scroll_by(&mut self, dx: i16, dy: i16) -> bool {
        if !self.scrollable.get() {
            return false;
        }

        let scroll = self.scroll.get();
        let desired = ScrollOffset {
            x: add_signed(scroll.x, dx),
            y: add_signed(scroll.y, dy),
        };
        let clamped = clamp_scroll_offset(self.content_size, self.viewport_size, desired);
        let changed = clamped != scroll;
        self.scroll.set(clamped);
        changed
    }

    fn scroll_to_clamped(&mut self, x: u16, y: u16) -> bool {
        if !self.scrollable.get() {
            return false;
        }
        let scroll = self.scroll.get();
        let desired = ScrollOffset { x, y };
        let clamped = clamp_scroll_offset(self.content_size, self.viewport_size, desired);
        let changed = clamped != scroll;
        self.scroll.set(clamped);
        changed
    }

    fn bounds_fully_visible(bounds: Rect, scroll: ScrollOffset, viewport: (u16, u16)) -> bool {
        if viewport.0 == 0 || viewport.1 == 0 {
            return false;
        }
        let x0 = bounds.x;
        let y0 = bounds.y;
        let x1 = bounds.x.saturating_add(bounds.width);
        let y1 = bounds.y.saturating_add(bounds.height);

        let vx0 = scroll.x;
        let vy0 = scroll.y;
        let vx1 = scroll.x.saturating_add(viewport.0);
        let vy1 = scroll.y.saturating_add(viewport.1);

        x0 >= vx0 && y0 >= vy0 && x1 <= vx1 && y1 <= vy1
    }

    fn hit_test_child_scrolled(
        &self,
        viewport_x: u16,
        viewport_y: u16,
        viewport: (u16, u16),
    ) -> Option<ComponentId> {
        for child in self
            .children
            .iter()
            .rev()
            .filter(|c| c.layout.anchor.is_some())
        {
            if contains(child.bounds(), viewport_x, viewport_y) {
                return Some(child.id);
            }
        }

        let scroll = self.scroll.get();
        let content_x = viewport_x.saturating_add(scroll.x);
        let content_y = viewport_y.saturating_add(scroll.y);

        for child in self
            .children
            .iter()
            .rev()
            .filter(|c| c.layout.anchor.is_none())
        {
            if !Self::bounds_fully_visible(child.bounds(), scroll, viewport) {
                continue;
            }
            if contains(child.bounds(), content_x, content_y) {
                return Some(child.id);
            }
        }
        None
    }

    fn desired_height_flow(&self) -> u16 {
        let padding = self.padding.get();

        let mut max_child: u16 = 0;
        for child in self.children.iter().filter(|c| c.layout.anchor.is_none()) {
            let margin = child.layout.margin;

            let min_h = child.view.min_height();
            let h = match child.layout.height {
                Size::Fixed(h) => h,
                Size::Content => child.view.desired_height().unwrap_or(1),
                Size::Fill | Size::Weight(_) => min_h,
            }
            .max(min_h);

            let h = h.saturating_add(margin.top).saturating_add(margin.bottom);
            max_child = max_child.max(h);
        }

        padding
            .top
            .saturating_add(padding.bottom)
            .saturating_add(max_child)
    }

    fn min_width_flow(&self) -> u16 {
        let padding = self.padding.get();
        let spacing = self.spacing.get();
        let scrollable = self.scrollable.get();

        if scrollable {
            let mut max_child: u16 = 0;
            for child in self.children.iter().filter(|c| c.layout.anchor.is_none()) {
                let margin = child.layout.margin;
                let min_w = child.view.min_width();
                let required_w = match child.layout.width {
                    Size::Fixed(w) => w.max(min_w),
                    Size::Content | Size::Fill | Size::Weight(_) => min_w,
                };
                let outer_w = margin
                    .left
                    .saturating_add(required_w)
                    .saturating_add(margin.right);
                max_child = max_child.max(outer_w);
            }

            return padding
                .left
                .saturating_add(padding.right)
                .saturating_add(max_child);
        }

        let mut total: u16 = padding.left.saturating_add(padding.right);
        let mut first_flow = true;

        for child in self.children.iter().filter(|c| c.layout.anchor.is_none()) {
            if !first_flow {
                total = total.saturating_add(spacing);
            }
            first_flow = false;

            let margin = child.layout.margin;
            total = total
                .saturating_add(margin.left)
                .saturating_add(margin.right);

            let min_w = child.view.min_width();
            let required_w = match child.layout.width {
                Size::Fixed(w) => w.max(min_w),
                Size::Content | Size::Fill | Size::Weight(_) => min_w,
            };

            total = total.saturating_add(required_w);
        }

        total
    }

    fn min_height_flow(&self) -> u16 {
        let padding = self.padding.get();

        let mut max_child: u16 = 0;
        for child in self.children.iter().filter(|c| c.layout.anchor.is_none()) {
            let margin = child.layout.margin;

            let min_h = child.view.min_height();
            let required_h = match child.layout.height {
                Size::Fixed(h) => h.max(min_h),
                Size::Content | Size::Fill | Size::Weight(_) => min_h,
            };

            let outer_h = margin
                .top
                .saturating_add(required_h)
                .saturating_add(margin.bottom);
            max_child = max_child.max(outer_h);
        }

        padding
            .top
            .saturating_add(padding.bottom)
            .saturating_add(max_child)
    }

    fn layout_children(&mut self, viewport_size: (u16, u16)) -> (u16, u16) {
        let (content_w, content_h) = viewport_size;
        let spacing = self.spacing.get();
        let scrollable = self.scrollable.get();

        #[derive(Clone, Copy, Debug)]
        enum WidthSpec {
            /// Fixed width (cannot shrink).
            Fixed(u16),
            /// Content-sized width (can shrink down to `min` when constrained).
            Content { min: u16, desired: u16 },
            /// Flexible width (takes remaining space), with an enforced minimum.
            Weight { weight: u16, min: u16 },
        }

        let mut specs: Vec<Option<WidthSpec>> = vec![None; self.children.len()];
        let mut margin_total: u16 = 0;
        let mut flow_count = 0usize;

        for (idx, child) in self.children.iter().enumerate() {
            if child.layout.anchor.is_some() {
                continue;
            }
            flow_count += 1;

            let margin = child.layout.margin;
            margin_total = margin_total
                .saturating_add(margin.left)
                .saturating_add(margin.right);

            let min_w = child.view.min_width();
            let spec = match child.layout.width {
                Size::Fixed(w) => WidthSpec::Fixed(w.max(min_w)),
                Size::Content => {
                    let desired = child.view.desired_width().unwrap_or(1).max(min_w);
                    WidthSpec::Content {
                        min: min_w,
                        desired,
                    }
                }
                Size::Weight(w) => WidthSpec::Weight {
                    weight: w.max(1),
                    min: min_w,
                },
                Size::Fill => WidthSpec::Weight {
                    weight: 1,
                    min: min_w,
                },
            };

            specs[idx] = Some(spec);
        }

        if flow_count >= 2 && spacing > 0 {
            margin_total =
                margin_total.saturating_add(spacing.saturating_mul(flow_count as u16 - 1));
        }

        let mut allocations: Vec<u16> = vec![0; self.children.len()];

        if scrollable {
            let mut fixed_total: u16 = 0;
            let mut weight_total: u16 = 0;

            for (idx, spec) in specs.iter().enumerate() {
                let Some(spec) = spec else {
                    continue;
                };

                match *spec {
                    WidthSpec::Fixed(w) => {
                        allocations[idx] = w;
                        fixed_total = fixed_total.saturating_add(w);
                    }
                    WidthSpec::Content { desired, .. } => {
                        allocations[idx] = desired;
                        fixed_total = fixed_total.saturating_add(desired);
                    }
                    WidthSpec::Weight { weight, min } => {
                        allocations[idx] = min;
                        fixed_total = fixed_total.saturating_add(min);
                        weight_total = weight_total.saturating_add(weight);
                    }
                }
            }

            let available = content_w
                .saturating_sub(margin_total)
                .saturating_sub(fixed_total);
            let mut remaining = available;

            if weight_total > 0 && remaining > 0 {
                let mut used: u16 = 0;
                for (idx, spec) in specs.iter().enumerate() {
                    let Some(WidthSpec::Weight { weight: w, .. }) = spec else {
                        continue;
                    };
                    let share = ((remaining as u32) * (*w as u32) / (weight_total as u32))
                        .min(u16::MAX as u32) as u16;
                    allocations[idx] = allocations[idx].saturating_add(share);
                    used = used.saturating_add(share);
                }
                remaining = remaining.saturating_sub(used);

                if remaining > 0 {
                    for (idx, spec) in specs.iter().enumerate() {
                        if remaining == 0 {
                            break;
                        }
                        if matches!(spec, Some(WidthSpec::Weight { .. })) {
                            allocations[idx] = allocations[idx].saturating_add(1);
                            remaining = remaining.saturating_sub(1);
                        }
                    }
                }
            }
        } else {
            let mut min_total: u16 = 0;
            let mut weight_total: u16 = 0;
            let mut content_extras: Vec<(usize, u16)> = Vec::new();

            for (idx, spec) in specs.iter().enumerate() {
                let Some(spec) = spec else {
                    continue;
                };

                match *spec {
                    WidthSpec::Fixed(w) => {
                        allocations[idx] = w;
                        min_total = min_total.saturating_add(w);
                    }
                    WidthSpec::Content { min, desired } => {
                        allocations[idx] = min;
                        min_total = min_total.saturating_add(min);
                        content_extras.push((idx, desired.saturating_sub(min)));
                    }
                    WidthSpec::Weight { weight, min } => {
                        allocations[idx] = min;
                        min_total = min_total.saturating_add(min);
                        weight_total = weight_total.saturating_add(weight);
                    }
                }
            }

            let available_for_children = content_w.saturating_sub(margin_total);
            let mut remaining = available_for_children.saturating_sub(min_total);

            for (idx, needed) in content_extras {
                if remaining == 0 {
                    break;
                }
                let extra = needed.min(remaining);
                allocations[idx] = allocations[idx].saturating_add(extra);
                remaining = remaining.saturating_sub(extra);
            }

            if weight_total > 0 && remaining > 0 {
                let mut used: u16 = 0;
                for (idx, spec) in specs.iter().enumerate() {
                    let Some(WidthSpec::Weight { weight: w, .. }) = spec else {
                        continue;
                    };
                    let share = ((remaining as u32) * (*w as u32) / (weight_total as u32))
                        .min(u16::MAX as u32) as u16;
                    allocations[idx] = allocations[idx].saturating_add(share);
                    used = used.saturating_add(share);
                }

                let mut leftover = remaining.saturating_sub(used);
                if leftover > 0 {
                    for (idx, spec) in specs.iter().enumerate() {
                        if leftover == 0 {
                            break;
                        }
                        if matches!(spec, Some(WidthSpec::Weight { .. })) {
                            allocations[idx] = allocations[idx].saturating_add(1);
                            leftover = leftover.saturating_sub(1);
                        }
                    }
                }
            }
        }

        let mut cursor_x: u16 = 0;
        let mut first_flow = true;
        let mut out_of_space = false;

        for (idx, child) in self.children.iter_mut().enumerate() {
            if let Some(anchor) = child.layout.anchor {
                let desired_w = match child.layout.width {
                    Size::Fixed(w) => w,
                    Size::Content => child.view.desired_width().unwrap_or(1),
                    Size::Fill | Size::Weight(_) => child.view.desired_width().unwrap_or(1),
                }
                .min(content_w);
                let desired_h = match child.layout.height {
                    Size::Fixed(h) => h,
                    Size::Content => child.view.desired_height().unwrap_or(content_h),
                    Size::Fill | Size::Weight(_) => {
                        child.view.desired_height().unwrap_or(content_h)
                    }
                }
                .min(content_h);

                let (min_w, min_h) = child.view.min_size();
                if content_w < min_w || content_h < min_h {
                    child.set_bounds(Rect::default());
                    continue;
                }

                child.set_bounds(position_anchored(
                    viewport_size,
                    (desired_w.max(min_w), desired_h.max(min_h)),
                    anchor.anchor,
                    anchor.offset_x,
                    anchor.offset_y,
                ));
                continue;
            }

            if !first_flow && spacing > 0 {
                cursor_x = cursor_x.saturating_add(spacing);
            }
            first_flow = false;

            let margin = child.layout.margin;
            cursor_x = cursor_x.saturating_add(margin.left);

            if !scrollable && cursor_x >= content_w {
                child.set_bounds(Rect::default());
                continue;
            }

            if out_of_space {
                child.set_bounds(Rect::default());
                continue;
            }

            let slot_w = allocations[idx];

            let max_w = content_w.saturating_sub(cursor_x);
            let available_w = max_w.saturating_sub(margin.right);
            let available_h = content_h.saturating_sub(margin.top.saturating_add(margin.bottom));

            let required_w = child.view.min_width();
            if !scrollable && available_w < required_w {
                child.set_bounds(Rect::default());
                out_of_space = true;
                continue;
            }

            let required_h = child.view.min_height();

            let w = if scrollable {
                slot_w
            } else {
                slot_w.min(available_w)
            };
            if w == 0 && required_w > 0 {
                child.set_bounds(Rect::default());
                out_of_space = true;
                continue;
            }

            if available_h < required_h {
                // Reserve horizontal space, but don't render an unusable child.
                child.set_bounds(Rect::default());
                cursor_x = cursor_x.saturating_add(w).saturating_add(margin.right);
                continue;
            }

            let slot = Rect {
                x: cursor_x,
                y: margin.top,
                width: w,
                height: available_h,
            };

            let desired = desired_size_for_slot(child.view.as_ref(), slot, child.layout);
            let aligned = align_within(slot, desired, child.layout.align_x, child.layout.align_y);
            child.set_bounds(aligned);

            cursor_x = cursor_x.saturating_add(w).saturating_add(margin.right);
        }

        (cursor_x, content_h)
    }
}

impl Component for HStack {
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
        self.min_width_flow()
    }

    fn min_height(&self) -> u16 {
        self.min_height_flow()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.desired_height_flow())
    }

    fn children(&self) -> &[ComponentNode] {
        &self.children
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        Some(&mut self.children)
    }

    fn is_scrollable(&self) -> bool {
        self.scrollable.get()
    }

    fn content_size(&self) -> (u16, u16) {
        self.content_size
    }

    fn scroll_offset(&self) -> (u16, u16) {
        let scroll = self.scroll.get();
        (scroll.x, scroll.y)
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.viewport_size
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.scroll_config.get()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        let _ = self.scroll_to_clamped(x, y);
    }

    fn scroll_to_child(&mut self, child_id: ComponentId) {
        let Some(node) = self.children.iter().find(|c| c.id == child_id) else {
            return;
        };
        if node.layout.anchor.is_some() {
            return;
        }

        let viewport = self.viewport_size;
        if viewport.0 == 0 || viewport.1 == 0 {
            return;
        }

        let bounds = node.bounds();
        let center_x = (bounds.x as u32).saturating_add((bounds.width as u32) / 2);
        let center_y = (bounds.y as u32).saturating_add((bounds.height as u32) / 2);

        let half_vw = (viewport.0 as u32) / 2;
        let half_vh = (viewport.1 as u32) / 2;

        let target_x = center_x.saturating_sub(half_vw).min(u16::MAX as u32) as u16;
        let target_y = center_y.saturating_sub(half_vh).min(u16::MAX as u32) as u16;
        let _ = self.scroll_to_clamped(target_x, target_y);
    }

    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let tab = self.handle_tab_navigation(event, ctx);
        if tab.is_consumed() {
            return tab;
        }

        EventResult::ignored()
    }

    fn handle_event_bubble(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if !self.scrollable.get() {
            return EventResult::ignored();
        }

        let cfg = self.scroll_config.get();
        match event {
            Event::Key(KeyEvent { code, kind, .. }) => {
                if matches!(kind, KeyEventKind::Release) {
                    return EventResult::ignored();
                }

                let viewport_w = self.viewport_size.0;
                let max = max_scroll_offset(self.content_size, self.viewport_size);

                let changed = match code {
                    KeyCode::Up => self.scroll_by(0, -1),
                    KeyCode::Down => self.scroll_by(0, 1),
                    KeyCode::Left => self.scroll_by(-1, 0),
                    KeyCode::Right => self.scroll_by(1, 0),
                    KeyCode::PageUp => self.scroll_by(-(viewport_w as i16), 0),
                    KeyCode::PageDown => self.scroll_by(viewport_w as i16, 0),
                    KeyCode::Home => self.scroll_to_clamped(0, 0),
                    KeyCode::End => self.scroll_to_clamped(max.x, max.y),
                    _ => false,
                };

                if changed {
                    EventResult::consumed()
                } else {
                    EventResult::ignored()
                }
            }
            Event::Mouse(m) => {
                let Some(area) = self.last_area else {
                    return EventResult::ignored();
                };
                if mouse_coords_local_to_area(area, *m).is_none() {
                    return EventResult::ignored();
                }

                let step = cfg.wheel_step as i16;
                let changed = match m.kind {
                    MouseEventKind::ScrollUp => self.scroll_by(0, -step),
                    MouseEventKind::ScrollDown => self.scroll_by(0, step),
                    MouseEventKind::ScrollLeft => self.scroll_by(-step, 0),
                    MouseEventKind::ScrollRight => self.scroll_by(step, 0),
                    _ => false,
                };

                if changed {
                    EventResult::consumed()
                } else {
                    EventResult::ignored()
                }
            }
            _ => EventResult::ignored(),
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let capture = self.handle_event_capture(event, ctx);
        if capture.is_consumed() {
            return capture;
        }

        if let Event::Mouse(m) = event {
            let Some(area) = self.last_area else {
                return self.handle_event_bubble(event, ctx);
            };
            let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
                return self.handle_event_bubble(event, ctx);
            };

            let cfg = self.scroll_config.get();
            let padding = self.padding.get();
            let scrollbars = if let Some(scrollbars) = self.scrollbars {
                scrollbars
            } else {
                let viewport = Rect {
                    x: 0,
                    y: 0,
                    width: area.width,
                    height: area.height,
                };
                Scrollbars {
                    viewport,
                    content: apply_padding(viewport, padding),
                    vbar: None,
                    hbar: None,
                    thickness: cfg.scrollbar_thickness.max(1),
                }
            };

            if self.scrollable.get() {
                if let Some(drag) = self.scrollbar_drag {
                    let scroll = self.scroll.get();
                    match m.kind {
                        MouseEventKind::Drag(MouseButton::Left) => match drag {
                            ScrollbarDrag::Vertical { grab_offset } => {
                                let Some(vbar) = scrollbars.vbar else {
                                    self.scrollbar_drag = None;
                                    return EventResult::consumed();
                                };
                                if vbar.height == 0 {
                                    return EventResult::consumed();
                                }
                                let layout = scrollbar_layout_1d(
                                    vbar.height,
                                    scrollbars.content.height,
                                    self.content_size.1,
                                    scroll.y,
                                    cfg.arrows,
                                );
                                if layout.track_len == 0 {
                                    return EventResult::consumed();
                                }

                                let pos = local_y
                                    .saturating_sub(vbar.y)
                                    .min(vbar.height.saturating_sub(1));
                                let pos_in_track = pos
                                    .saturating_sub(layout.track_start)
                                    .min(layout.track_len.saturating_sub(1));

                                let max_start = layout.track_len.saturating_sub(layout.thumb_len);
                                let new_thumb_start =
                                    pos_in_track.saturating_sub(grab_offset).min(max_start);
                                let new_off = scroll_offset_from_thumb_start(
                                    layout.track_len,
                                    scrollbars.content.height,
                                    self.content_size.1,
                                    new_thumb_start,
                                );
                                let _ = self.scroll_to_clamped(scroll.x, new_off);
                                return EventResult::consumed();
                            }
                            ScrollbarDrag::Horizontal { grab_offset } => {
                                let Some(hbar) = scrollbars.hbar else {
                                    self.scrollbar_drag = None;
                                    return EventResult::consumed();
                                };
                                if hbar.width == 0 {
                                    return EventResult::consumed();
                                }
                                let layout = scrollbar_layout_1d(
                                    hbar.width,
                                    scrollbars.content.width,
                                    self.content_size.0,
                                    scroll.x,
                                    cfg.arrows,
                                );
                                if layout.track_len == 0 {
                                    return EventResult::consumed();
                                }

                                let pos = local_x
                                    .saturating_sub(hbar.x)
                                    .min(hbar.width.saturating_sub(1));
                                let pos_in_track = pos
                                    .saturating_sub(layout.track_start)
                                    .min(layout.track_len.saturating_sub(1));

                                let max_start = layout.track_len.saturating_sub(layout.thumb_len);
                                let new_thumb_start =
                                    pos_in_track.saturating_sub(grab_offset).min(max_start);
                                let new_off = scroll_offset_from_thumb_start(
                                    layout.track_len,
                                    scrollbars.content.width,
                                    self.content_size.0,
                                    new_thumb_start,
                                );
                                let _ = self.scroll_to_clamped(new_off, scroll.y);
                                return EventResult::consumed();
                            }
                        },
                        MouseEventKind::Up(MouseButton::Left) => {
                            self.scrollbar_drag = None;
                            return EventResult::consumed();
                        }
                        _ => {}
                    }
                }

                if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                    let scroll = self.scroll.get();
                    if let Some(vbar) = scrollbars.vbar
                        && contains(vbar, local_x, local_y)
                        && vbar.height > 0
                    {
                        let pos = local_y.saturating_sub(vbar.y);
                        let layout = scrollbar_layout_1d(
                            vbar.height,
                            scrollbars.content.height,
                            self.content_size.1,
                            scroll.y,
                            cfg.arrows,
                        );
                        match scrollbar_hit_test(layout, pos) {
                            ScrollbarHit::ArrowDec => {
                                let _ = self.scroll_by(0, -1);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::ArrowInc => {
                                let _ = self.scroll_by(0, 1);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::Thumb { grab_offset } => {
                                self.scrollbar_drag = Some(ScrollbarDrag::Vertical { grab_offset });
                                return EventResult::consumed();
                            }
                            ScrollbarHit::TrackDec => {
                                let page = scrollbars.content.height as i16;
                                let _ = self.scroll_by(0, -(page));
                                return EventResult::consumed();
                            }
                            ScrollbarHit::TrackInc => {
                                let page = scrollbars.content.height as i16;
                                let _ = self.scroll_by(0, page);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::None => {}
                        }
                    }

                    if let Some(hbar) = scrollbars.hbar
                        && contains(hbar, local_x, local_y)
                        && hbar.width > 0
                    {
                        let pos = local_x.saturating_sub(hbar.x);
                        let layout = scrollbar_layout_1d(
                            hbar.width,
                            scrollbars.content.width,
                            self.content_size.0,
                            scroll.x,
                            cfg.arrows,
                        );
                        match scrollbar_hit_test(layout, pos) {
                            ScrollbarHit::ArrowDec => {
                                let _ = self.scroll_by(-1, 0);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::ArrowInc => {
                                let _ = self.scroll_by(1, 0);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::Thumb { grab_offset } => {
                                self.scrollbar_drag =
                                    Some(ScrollbarDrag::Horizontal { grab_offset });
                                return EventResult::consumed();
                            }
                            ScrollbarHit::TrackDec => {
                                let page = scrollbars.content.width as i16;
                                let _ = self.scroll_by(-(page), 0);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::TrackInc => {
                                let page = scrollbars.content.width as i16;
                                let _ = self.scroll_by(page, 0);
                                return EventResult::consumed();
                            }
                            ScrollbarHit::None => {}
                        }
                    }
                }
            }

            let content = scrollbars.content;
            if !contains(content, local_x, local_y) {
                return self.handle_event_bubble(event, ctx);
            }

            let content_x = local_x.saturating_sub(content.x);
            let content_y = local_y.saturating_sub(content.y);
            let content_size = (content.width, content.height);

            let Some(child_id) = self.hit_test_child_scrolled(content_x, content_y, content_size)
            else {
                return self.handle_event_bubble(event, ctx);
            };
            let Some(child_idx) = self.children.iter().position(|c| c.id == child_id) else {
                return self.handle_event_bubble(event, ctx);
            };

            let child_bounds = self.children[child_idx].bounds();
            let is_anchored = self.children[child_idx].layout.anchor.is_some();
            let scroll = self.scroll.get();
            let point_x = if is_anchored {
                content_x
            } else {
                content_x.saturating_add(scroll.x)
            };
            let point_y = if is_anchored {
                content_y
            } else {
                content_y.saturating_add(scroll.y)
            };
            let child_x = point_x.saturating_sub(child_bounds.x);
            let child_y = point_y.saturating_sub(child_bounds.y);

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

            return self.handle_event_bubble(event, ctx);
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

        self.handle_event_bubble(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);

        let cfg = self.scroll_config.get();
        let padding = self.padding.get();
        let thickness = cfg.scrollbar_thickness.max(1);

        let mut viewport_outer = area;
        let mut inner = apply_padding(viewport_outer, padding);
        let mut show_v = false;
        let mut show_h = false;

        if self.scrollable.get() {
            if matches!(ctx.scrollbar_host, ScrollbarHost::Component) {
                for _ in 0..2 {
                    inner = apply_padding(viewport_outer, padding);
                    self.viewport_size = (inner.width, inner.height);
                    self.content_size = self.layout_children((inner.width, inner.height));

                    let new_show_v = should_show_scrollbar(
                        cfg.vertical_scrollbar,
                        self.content_size.1,
                        self.viewport_size.1,
                    );
                    let new_show_h = should_show_scrollbar(
                        cfg.horizontal_scrollbar,
                        self.content_size.0,
                        self.viewport_size.0,
                    );

                    if new_show_v == show_v && new_show_h == show_h {
                        break;
                    }

                    show_v = new_show_v;
                    show_h = new_show_h;

                    let v_thick = if show_v { thickness } else { 0 };
                    let h_thick = if show_h { thickness } else { 0 };
                    viewport_outer = Rect {
                        x: area.x,
                        y: area.y,
                        width: area.width.saturating_sub(v_thick),
                        height: area.height.saturating_sub(h_thick),
                    };
                }
            } else {
                viewport_outer = area;
                inner = apply_padding(area, padding);
                self.viewport_size = (inner.width, inner.height);
                self.content_size = self.layout_children((inner.width, inner.height));
                show_v = should_show_scrollbar(
                    cfg.vertical_scrollbar,
                    self.content_size.1,
                    self.viewport_size.1,
                );
                show_h = should_show_scrollbar(
                    cfg.horizontal_scrollbar,
                    self.content_size.0,
                    self.viewport_size.0,
                );
            }
        } else {
            inner = apply_padding(area, padding);
            self.viewport_size = (inner.width, inner.height);
            self.content_size = self.layout_children((inner.width, inner.height));
        }

        let scroll = self.scroll.get();
        self.scroll.set(clamp_scroll_offset(
            self.content_size,
            self.viewport_size,
            scroll,
        ));

        if self.scrollable.get() && matches!(ctx.scrollbar_host, ScrollbarHost::Component) {
            let viewport_local = Rect {
                x: viewport_outer.x.saturating_sub(area.x),
                y: viewport_outer.y.saturating_sub(area.y),
                width: viewport_outer.width,
                height: viewport_outer.height,
            };
            let content_local = apply_padding(viewport_local, padding);
            let vbar = show_v.then_some(Rect {
                x: viewport_local.x.saturating_add(viewport_local.width),
                y: viewport_local.y,
                width: thickness,
                height: viewport_local.height,
            });
            let hbar = show_h.then_some(Rect {
                x: viewport_local.x,
                y: viewport_local.y.saturating_add(viewport_local.height),
                width: viewport_local.width,
                height: thickness,
            });
            self.scrollbars = Some(Scrollbars {
                viewport: viewport_local,
                content: content_local,
                vbar,
                hbar,
                thickness,
            });
            if !show_v && !show_h {
                self.scrollbar_drag = None;
            }
        } else {
            self.scrollbars = None;
            self.scrollbar_drag = None;
        }

        let scrollable = self.scrollable.get();
        let scroll = self.scroll.get();
        let viewport = self.viewport_size;

        for child in self
            .children
            .iter_mut()
            .filter(|c| c.layout.anchor.is_none())
        {
            let r = child.bounds();
            if r.width == 0 || r.height == 0 {
                continue;
            }
            if scrollable && !Self::bounds_fully_visible(r, scroll, viewport) {
                continue;
            }
            let abs = Rect {
                x: inner.x.saturating_add(r.x.saturating_sub(scroll.x)),
                y: inner.y.saturating_add(r.y.saturating_sub(scroll.y)),
                width: r.width,
                height: r.height,
            };

            let child_focused = ctx.is_focused && self.focused == Some(child.id);
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
            };
            child.view.draw(frame, abs, child_ctx);
        }

        for child in self
            .children
            .iter_mut()
            .filter(|c| c.layout.anchor.is_some())
        {
            let r = child.bounds();
            if r.width == 0 || r.height == 0 {
                continue;
            }
            let abs = Rect {
                x: inner.x.saturating_add(r.x),
                y: inner.y.saturating_add(r.y),
                width: r.width,
                height: r.height,
            };

            let child_focused = ctx.is_focused && self.focused == Some(child.id);
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
            };
            child.view.draw(frame, abs, child_ctx);
        }

        let Some(scrollbars) = self.scrollbars else {
            return;
        };

        let track_style = ctx.theme.scrollbar_track;
        let thumb_style = ctx.theme.scrollbar_thumb;
        let arrow_style = ctx.theme.scrollbar_arrow;
        let buf = frame.buffer_mut();

        let track = ctx.theme.glyph("scrollbar-track").unwrap_or("░");
        let thumb = ctx.theme.glyph("scrollbar-thumb").unwrap_or("█");
        let arrow_up = ctx.theme.glyph("scrollbar-up-arrow").unwrap_or("▲");
        let arrow_down = ctx.theme.glyph("scrollbar-down-arrow").unwrap_or("▼");
        let arrow_left = ctx.theme.glyph("scrollbar-left-arrow").unwrap_or("◄");
        let arrow_right = ctx.theme.glyph("scrollbar-right-arrow").unwrap_or("►");

        if let Some(vbar) = scrollbars.vbar {
            let layout = scrollbar_layout_1d(
                vbar.height,
                viewport.1,
                self.content_size.1,
                scroll.y,
                cfg.arrows,
            );

            for dy in 0..vbar.height {
                let (symbol, style) = if layout.has_arrows && dy == 0 {
                    (arrow_up, arrow_style)
                } else if layout.has_arrows && dy == layout.bar_len.saturating_sub(1) {
                    (arrow_down, arrow_style)
                } else if dy >= layout.thumb_start
                    && dy < layout.thumb_start.saturating_add(layout.thumb_len)
                {
                    (thumb, thumb_style)
                } else {
                    (track, track_style)
                };
                for dx in 0..vbar.width {
                    buf[(
                        area.x.saturating_add(vbar.x).saturating_add(dx),
                        area.y.saturating_add(vbar.y).saturating_add(dy),
                    )]
                        .set_symbol(symbol)
                        .set_style(style);
                }
            }
        }

        if let Some(hbar) = scrollbars.hbar {
            let layout = scrollbar_layout_1d(
                hbar.width,
                viewport.0,
                self.content_size.0,
                scroll.x,
                cfg.arrows,
            );

            for dx in 0..hbar.width {
                let (symbol, style) = if layout.has_arrows && dx == 0 {
                    (arrow_left, arrow_style)
                } else if layout.has_arrows && dx == layout.bar_len.saturating_sub(1) {
                    (arrow_right, arrow_style)
                } else if dx >= layout.thumb_start
                    && dx < layout.thumb_start.saturating_add(layout.thumb_len)
                {
                    (thumb, thumb_style)
                } else {
                    (track, track_style)
                };
                for dy in 0..hbar.height {
                    buf[(
                        area.x.saturating_add(hbar.x).saturating_add(dx),
                        area.y.saturating_add(hbar.y).saturating_add(dy),
                    )]
                        .set_symbol(symbol)
                        .set_style(style);
                }
            }
        }

        if let (Some(vbar), Some(hbar)) = (scrollbars.vbar, scrollbars.hbar) {
            let corner = Rect {
                x: vbar.x,
                y: hbar.y,
                width: vbar.width,
                height: hbar.height,
            };
            for dy in 0..corner.height {
                for dx in 0..corner.width {
                    buf[(
                        area.x.saturating_add(corner.x).saturating_add(dx),
                        area.y.saturating_add(corner.y).saturating_add(dy),
                    )]
                        .set_symbol(track)
                        .set_style(track_style);
                }
            }
        }
    }
}
