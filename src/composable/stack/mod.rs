mod events;
mod layout;
mod scrollbars;

use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::component::{
    Component, ComponentContext, DragAndDrop, DynamicTree, EventHandling, EventResult, FocusNav,
    Layout, Scrollable,
};
use super::geom::{align_within, focusable_children_in_tab_order, position_anchored};
use super::layout::{EdgeInsets, LayoutParams, Size};
use super::node::{ComponentId, ComponentNode};
use super::scroll::{ScrollConfig, ScrollOffset, ScrollbarDrag, Scrollbars};
use crate::reactive::Binding;
use atto_ui_macros::{ComponentProperties, component_properties};
use layout::desired_size_for_slot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StackAxis {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingScrollAdjustment {
    ToBottom,
    PreserveYAfterContentHeightChange {
        previous_content_height: u16,
        previous_scroll_y: u16,
    },
}

#[derive(ComponentProperties)]
struct StackCore {
    axis: StackAxis,
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
    pending_scroll_adjustment: Option<PendingScrollAdjustment>,
    scroll_config: Binding<ScrollConfig>,
    scrollbars: Option<Scrollbars>,
    scrollbar_drag: Option<ScrollbarDrag>,
    /// Child that has captured the pointer (set when a descendant requests
    /// capture on mouse down). While set, mouse events are routed straight to it,
    /// bypassing hit-testing, until it releases on mouse up.
    captured_child: Option<ComponentId>,
}

impl Default for StackCore {
    fn default() -> Self {
        Self::new(StackAxis::Vertical)
    }
}

impl StackCore {
    fn new(axis: StackAxis) -> Self {
        Self {
            axis,
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
            pending_scroll_adjustment: None,
            scroll_config: ScrollConfig::default().into(),
            scrollbars: None,
            scrollbar_drag: None,
            captured_child: None,
        }
    }

    fn with_padding(mut self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.padding = padding.into();
        self
    }

    fn with_spacing(mut self, spacing: impl Into<Binding<u16>>) -> Self {
        self.spacing = spacing.into();
        self
    }

    fn with_scrollable(mut self, scrollable: impl Into<Binding<bool>>) -> Self {
        self.scrollable = scrollable.into();
        if !self.scrollable.get() {
            self.scroll.set(ScrollOffset::ZERO);
        }
        self
    }

    fn with_scroll_config(mut self, config: impl Into<Binding<ScrollConfig>>) -> Self {
        self.scroll_config = config.into();
        self
    }

    fn scroll_to_bottom_on_next_layout(&mut self) {
        if self.scrollable.get() {
            self.pending_scroll_adjustment = Some(PendingScrollAdjustment::ToBottom);
        }
    }

    fn preserve_scroll_y_after_next_layout(
        &mut self,
        previous_content_height: u16,
        previous_scroll_y: u16,
    ) {
        if self.scrollable.get() {
            self.pending_scroll_adjustment =
                Some(PendingScrollAdjustment::PreserveYAfterContentHeightChange {
                    previous_content_height,
                    previous_scroll_y,
                });
        }
    }

    fn apply_pending_scroll_adjustment(&mut self) {
        if !self.scrollable.get() {
            self.pending_scroll_adjustment = None;
            return;
        }
        if self.viewport_size.1 == 0 {
            return;
        }

        let Some(adjustment) = self.pending_scroll_adjustment.take() else {
            return;
        };
        let current = self.scroll.get();
        let desired_y = match adjustment {
            PendingScrollAdjustment::ToBottom => {
                self.content_size.1.saturating_sub(self.viewport_size.1)
            }
            PendingScrollAdjustment::PreserveYAfterContentHeightChange {
                previous_content_height,
                previous_scroll_y,
            } => {
                let inserted_height = self.content_size.1.saturating_sub(previous_content_height);
                previous_scroll_y.saturating_add(inserted_height)
            }
        };
        let _ = self.scroll_to_clamped(current.x, desired_y);
    }

    fn child(mut self, view: impl Component + 'static) -> Self {
        self.add_child_with_layout(Box::new(view), LayoutParams::default());
        self
    }

    fn child_with_layout(mut self, view: impl Component + 'static, layout: LayoutParams) -> Self {
        self.add_child_with_layout(Box::new(view), layout);
        self
    }

    fn add_child_with_layout(
        &mut self,
        view: Box<dyn Component>,
        layout: LayoutParams,
    ) -> ComponentId {
        let mut node = ComponentNode::new(view).with_layout(layout);
        node.parent = Some(self.id);
        let id = node.id;
        if self.focused.is_none() && node.view.is_focusable() {
            self.focused = Some(id);
        }
        self.children.push(node);
        id
    }

    fn replace_children(&mut self, mut children: Vec<ComponentNode>) {
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

        // Any in-progress scrollbar drag or pointer capture is no longer valid
        // after restructuring children.
        self.scrollbar_drag = None;
        self.captured_child = None;
    }

    fn desired_height_flow(&self) -> u16 {
        match self.axis {
            StackAxis::Vertical => {
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
            StackAxis::Horizontal => {
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
        }
    }

    fn min_width_flow(&self) -> u16 {
        match self.axis {
            StackAxis::Vertical => {
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
            StackAxis::Horizontal => {
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
        }
    }

    fn min_height_flow(&self) -> u16 {
        match self.axis {
            StackAxis::Vertical => {
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
            StackAxis::Horizontal => {
                let padding = self.padding.get();

                let mut max_child: u16 = 0;
                for child in self.children.iter().filter(|c| c.layout.anchor.is_none()) {
                    let margin = child.layout.margin;

                    let min_h = child.view.min_height();
                    let required_h = match child.layout.height {
                        Size::Fixed(h) => h.max(min_h),
                        Size::Content | Size::Fill | Size::Weight(_) => min_h,
                    };

                    let h = required_h
                        .saturating_add(margin.top)
                        .saturating_add(margin.bottom);
                    max_child = max_child.max(h);
                }

                padding
                    .top
                    .saturating_add(padding.bottom)
                    .saturating_add(max_child)
            }
        }
    }

    fn layout_children(&mut self, viewport_size: (u16, u16)) -> (u16, u16) {
        match self.axis {
            StackAxis::Vertical => self.layout_children_vertical(viewport_size),
            StackAxis::Horizontal => self.layout_children_horizontal(viewport_size),
        }
    }

    fn layout_children_vertical(&mut self, viewport_size: (u16, u16)) -> (u16, u16) {
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

    fn layout_children_horizontal(&mut self, viewport_size: (u16, u16)) -> (u16, u16) {
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

#[component_properties]
impl Component for StackCore {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.draw_impl(frame, area, ctx)
    }
}

impl DragAndDrop for StackCore {}

impl FocusNav for StackCore {
    fn focused_child(&self) -> Option<ComponentId> {
        self.focused
    }

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
}

impl Layout for StackCore {
    fn min_width(&self) -> u16 {
        self.min_width_flow()
    }

    fn min_height(&self) -> u16 {
        self.min_height_flow()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.desired_height_flow())
    }
}

impl DynamicTree for StackCore {
    fn children(&self) -> &[ComponentNode] {
        &self.children
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        Some(&mut self.children)
    }
}

impl Scrollable for StackCore {
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
}

impl EventHandling for StackCore {
    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.handle_event_capture_impl(event, ctx)
    }

    fn handle_event_bubble(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.handle_event_bubble_impl(event, ctx.mouse_coordinate_space)
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.handle_event_impl(event, ctx)
    }
}

macro_rules! define_stack {
    ($name:ident, $axis:expr) => {
        #[derive(ComponentProperties)]
        pub struct $name {
            #[component(delegate)]
            core: StackCore,
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl $name {
            pub fn new() -> Self {
                Self {
                    core: StackCore::new($axis),
                }
            }

            pub fn with_padding(mut self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
                self.core = self.core.with_padding(padding);
                self
            }

            pub fn with_spacing(mut self, spacing: impl Into<Binding<u16>>) -> Self {
                self.core = self.core.with_spacing(spacing);
                self
            }

            pub fn with_scrollable(mut self, scrollable: impl Into<Binding<bool>>) -> Self {
                self.core = self.core.with_scrollable(scrollable);
                self
            }

            pub fn with_scroll_config(mut self, config: impl Into<Binding<ScrollConfig>>) -> Self {
                self.core = self.core.with_scroll_config(config);
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

            pub fn scroll_to_bottom_on_next_layout(&mut self) {
                self.core.scroll_to_bottom_on_next_layout();
            }

            pub fn preserve_scroll_y_after_next_layout(
                &mut self,
                previous_content_height: u16,
                previous_scroll_y: u16,
            ) {
                self.core.preserve_scroll_y_after_next_layout(
                    previous_content_height,
                    previous_scroll_y,
                );
            }

            pub fn child(mut self, view: impl Component + 'static) -> Self {
                self.core = self.core.child(view);
                self
            }

            pub fn child_with_layout(
                mut self,
                view: impl Component + 'static,
                layout: LayoutParams,
            ) -> Self {
                self.core = self.core.child_with_layout(view, layout);
                self
            }

            pub fn add_child_with_layout(
                &mut self,
                view: Box<dyn Component>,
                layout: LayoutParams,
            ) {
                self.core.add_child_with_layout(view, layout);
            }

            pub fn replace_children(&mut self, children: Vec<ComponentNode>) {
                self.core.replace_children(children);
            }
        }

        #[component_properties]
        impl Component for $name {
            fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
                self.core.draw(frame, area, ctx)
            }
        }

        impl FocusNav for $name {
            fn focused_child(&self) -> Option<ComponentId> {
                self.core.focused_child()
            }

            fn is_focusable(&self) -> bool {
                self.core.is_focusable()
            }

            fn focus_first(&mut self) -> bool {
                self.core.focus_first()
            }

            fn focus_last(&mut self) -> bool {
                self.core.focus_last()
            }
        }

        impl Layout for $name {
            fn min_width(&self) -> u16 {
                self.core.min_width()
            }

            fn min_height(&self) -> u16 {
                self.core.min_height()
            }

            fn desired_height(&self) -> Option<u16> {
                self.core.desired_height()
            }
        }

        impl DynamicTree for $name {
            fn children(&self) -> &[ComponentNode] {
                self.core.children()
            }

            fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
                self.core.children_mut()
            }
        }

        impl Scrollable for $name {
            fn is_scrollable(&self) -> bool {
                self.core.is_scrollable()
            }

            fn content_size(&self) -> (u16, u16) {
                self.core.content_size()
            }

            fn scroll_offset(&self) -> (u16, u16) {
                self.core.scroll_offset()
            }

            fn viewport_size(&self) -> (u16, u16) {
                self.core.viewport_size()
            }

            fn scroll_config(&self) -> ScrollConfig {
                Scrollable::scroll_config(&self.core)
            }

            fn set_scroll_offset(&mut self, x: u16, y: u16) {
                self.core.set_scroll_offset(x, y);
            }

            fn scroll_to_child(&mut self, child_id: ComponentId) {
                self.core.scroll_to_child(child_id);
            }
        }

        impl EventHandling for $name {
            fn handle_event_capture(
                &mut self,
                event: &Event,
                ctx: ComponentContext<'_>,
            ) -> EventResult {
                self.core.handle_event_capture(event, ctx)
            }

            fn handle_event_bubble(
                &mut self,
                event: &Event,
                ctx: ComponentContext<'_>,
            ) -> EventResult {
                self.core.handle_event_bubble(event, ctx)
            }

            fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
                self.core.handle_event(event, ctx)
            }
        }

        impl DragAndDrop for $name {}
    };
}

define_stack!(VStack, StackAxis::Vertical);
define_stack!(HStack, StackAxis::Horizontal);
