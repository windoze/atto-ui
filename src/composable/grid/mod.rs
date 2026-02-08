mod events;
mod layout;
mod scrollbars;

use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::component::{Component, ComponentContext, EventResult};
use super::geom::{align_within, focusable_children_in_tab_order, position_anchored};
use super::layout::{EdgeInsets, LayoutParams, Size};
use super::node::{ComponentId, ComponentNode};
use super::scroll::{ScrollConfig, ScrollOffset, ScrollbarDrag, Scrollbars};
use crate::reactive::Binding;
use atto_ui_macros::{ComponentProperties, component_properties};

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

#[derive(ComponentProperties)]
pub struct Grid {
    id: ComponentId,
    children: Vec<ComponentNode>,
    columns: Binding<usize>,
    padding: Binding<EdgeInsets>,
    row_gap: Binding<u16>,
    column_gap: Binding<u16>,
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

impl Default for Grid {
    fn default() -> Self {
        Self {
            id: ComponentId::next(),
            children: Vec::new(),
            columns: 1usize.into(),
            padding: EdgeInsets::ZERO.into(),
            row_gap: 0u16.into(),
            column_gap: 0u16.into(),
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
}

impl Grid {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_columns(mut self, columns: impl Into<Binding<usize>>) -> Self {
        self.columns = columns.into();
        self
    }

    pub fn with_padding(mut self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.padding = padding.into();
        self
    }

    pub fn with_row_gap(mut self, gap: impl Into<Binding<u16>>) -> Self {
        self.row_gap = gap.into();
        self
    }

    pub fn with_column_gap(mut self, gap: impl Into<Binding<u16>>) -> Self {
        self.column_gap = gap.into();
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

    pub fn columns(self, columns: impl Into<Binding<usize>>) -> Self {
        self.with_columns(columns)
    }

    pub fn padding(self, padding: u16) -> Self {
        self.with_padding(EdgeInsets::all(padding))
    }

    pub fn padding_insets(self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.with_padding(padding)
    }

    pub fn row_gap(self, gap: impl Into<Binding<u16>>) -> Self {
        self.with_row_gap(gap)
    }

    pub fn column_gap(self, gap: impl Into<Binding<u16>>) -> Self {
        self.with_column_gap(gap)
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

    pub fn add_child_with_layout(
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

    fn layout_children(&mut self, viewport_size: (u16, u16)) -> (u16, u16) {
        let (viewport_w, viewport_h) = viewport_size;

        let columns = self.columns.get().max(1);
        let col_gap = self.column_gap.get();
        let row_gap = self.row_gap.get();
        let scrollable = self.scrollable.get();

        let mut col_mins: Vec<u16> = vec![0; columns];
        let mut row_mins: Vec<u16> = Vec::new();
        let mut row_desired: Vec<u16> = Vec::new();
        let mut flow_rows: Vec<Vec<usize>> = Vec::new();

        for (idx, child) in self.children.iter().enumerate() {
            if child.layout.anchor.is_some() {
                continue;
            }
            let row = idx / columns;
            let col = idx % columns;
            while flow_rows.len() <= row {
                flow_rows.push(Vec::new());
                row_mins.push(0);
                row_desired.push(0);
            }
            flow_rows[row].push(idx);

            let margin = child.layout.margin;

            let min_w = child.view.min_width();
            let required_w = match child.layout.width {
                Size::Fixed(w) => w.max(min_w),
                Size::Content | Size::Fill | Size::Weight(_) => min_w,
            };
            let outer_min_w = margin
                .left
                .saturating_add(required_w)
                .saturating_add(margin.right);
            if let Some(slot) = col_mins.get_mut(col) {
                *slot = (*slot).max(outer_min_w);
            }

            let min_h = child.view.min_height();
            let required_min_h = match child.layout.height {
                Size::Fixed(h) => h.max(min_h),
                Size::Content | Size::Fill | Size::Weight(_) => min_h,
            };
            let outer_min_h = margin
                .top
                .saturating_add(required_min_h)
                .saturating_add(margin.bottom);
            row_mins[row] = row_mins[row].max(outer_min_h);

            let desired_h = match child.layout.height {
                Size::Fixed(h) => h.max(min_h),
                Size::Content => child.view.desired_height().unwrap_or(1).max(min_h),
                Size::Fill | Size::Weight(_) => child.view.desired_height().unwrap_or(1).max(min_h),
            };
            let outer_desired_h = margin
                .top
                .saturating_add(desired_h)
                .saturating_add(margin.bottom);
            row_desired[row] = row_desired[row].max(outer_desired_h);
        }

        let gap_total_w = if columns >= 2 {
            col_gap.saturating_mul(columns as u16 - 1)
        } else {
            0
        };
        let cols_min_sum: u16 = col_mins
            .iter()
            .copied()
            .fold(0, |acc, w| acc.saturating_add(w));
        let min_content_w = cols_min_sum.saturating_add(gap_total_w);
        let content_w = if scrollable {
            viewport_w.max(min_content_w)
        } else {
            viewport_w
        };

        let col_widths: Vec<u16> = if !scrollable && viewport_w < min_content_w {
            // When not scrollable, never emit x coordinates outside the viewport.
            // This fallback keeps the grid inside the viewport even if constraints are violated.
            let usable_w = viewport_w.saturating_sub(gap_total_w);
            let base = usable_w / columns as u16;
            let remainder = usable_w % columns as u16;

            let mut widths = vec![base; columns];
            for w in widths.iter_mut().take(remainder as usize) {
                *w = w.saturating_add(1);
            }
            widths
        } else {
            let mut widths = col_mins;
            if columns > 0 {
                let extra = content_w.saturating_sub(min_content_w);
                if extra > 0 {
                    let share = extra / columns as u16;
                    let remainder = extra % columns as u16;
                    for (idx, w) in widths.iter_mut().enumerate() {
                        *w = w.saturating_add(share);
                        if idx < remainder as usize {
                            *w = w.saturating_add(1);
                        }
                    }
                }
            }
            widths
        };

        let mut col_xs = vec![0u16; columns];
        let mut x = 0u16;
        for (i, w) in col_widths.iter().enumerate() {
            col_xs[i] = x;
            x = x.saturating_add(*w).saturating_add(col_gap);
        }

        let rows = row_desired.len();
        let gap_total_h = if rows >= 2 {
            row_gap.saturating_mul(rows as u16 - 1)
        } else {
            0
        };

        let row_heights: Vec<u16> = if scrollable {
            row_desired
        } else {
            let mut heights = row_mins;
            let available_for_rows = viewport_h.saturating_sub(gap_total_h);
            let min_sum: u16 = heights
                .iter()
                .copied()
                .fold(0, |acc, h| acc.saturating_add(h));
            let mut remaining = available_for_rows.saturating_sub(min_sum);

            for row in 0..rows {
                if remaining == 0 {
                    break;
                }
                let need = row_desired[row].saturating_sub(heights[row]);
                let extra = need.min(remaining);
                heights[row] = heights[row].saturating_add(extra);
                remaining = remaining.saturating_sub(extra);
            }

            heights
        };

        let mut row_ys: Vec<u16> = vec![0; rows];
        let mut y = 0u16;
        for (row, h) in row_heights.iter().enumerate() {
            row_ys[row] = y;
            y = y.saturating_add(*h).saturating_add(row_gap);
        }

        for child in self.children.iter_mut() {
            child.set_bounds(Rect::default());
        }

        for (row, indices) in flow_rows.iter().enumerate() {
            let y0 = row_ys[row];
            if !scrollable && y0 >= viewport_h {
                continue;
            }
            let row_h = if scrollable {
                row_heights[row]
            } else {
                row_heights[row].min(viewport_h.saturating_sub(y0))
            };

            for &idx in indices {
                let col = idx % columns;
                let cell_x = col_xs[col];
                let cell_w = col_widths[col];

                let child = &mut self.children[idx];
                let margin = child.layout.margin;

                let slot_x = cell_x.saturating_add(margin.left);
                let slot_y = y0.saturating_add(margin.top);
                let slot_w = cell_w.saturating_sub(margin.left.saturating_add(margin.right));
                let slot_h = row_h.saturating_sub(margin.top.saturating_add(margin.bottom));

                let min_w = child.view.min_width();
                let min_h = child.view.min_height();
                let required_w = match child.layout.width {
                    Size::Fixed(w) => w.max(min_w),
                    Size::Content | Size::Fill | Size::Weight(_) => min_w,
                };
                let required_h = match child.layout.height {
                    Size::Fixed(h) => h.max(min_h),
                    Size::Content | Size::Fill | Size::Weight(_) => min_h,
                };

                if slot_w < required_w || slot_h < required_h {
                    child.set_bounds(Rect::default());
                    continue;
                }

                let slot = Rect {
                    x: slot_x,
                    y: slot_y,
                    width: slot_w,
                    height: slot_h,
                };

                let desired = desired_size_for_slot(child.view.as_ref(), slot, child.layout);
                let aligned =
                    align_within(slot, desired, child.layout.align_x, child.layout.align_y);
                child.set_bounds(aligned);
            }
        }

        for child in self.children.iter_mut() {
            let Some(anchor) = child.layout.anchor else {
                continue;
            };
            let desired_w = match child.layout.width {
                Size::Fixed(w) => w,
                Size::Content => child.view.desired_width().unwrap_or(viewport_w),
                Size::Fill | Size::Weight(_) => child.view.desired_width().unwrap_or(viewport_w),
            }
            .min(viewport_w);
            let desired_h = match child.layout.height {
                Size::Fixed(h) => h,
                Size::Content => child.view.desired_height().unwrap_or(1),
                Size::Fill | Size::Weight(_) => child.view.desired_height().unwrap_or(1),
            }
            .min(viewport_h);

            let (min_w, min_h) = child.view.min_size();
            if viewport_w < min_w || viewport_h < min_h {
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
        }

        let total_h = match rows {
            0 => 0,
            n => row_ys[n - 1].saturating_add(row_heights[n - 1]),
        };

        (content_w, total_h)
    }
}

#[component_properties]
impl Component for Grid {
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

    fn min_width(&self) -> u16 {
        let columns = self.columns.get().max(1);
        let padding = self.padding.get();
        let col_gap = self.column_gap.get();

        let mut col_mins: Vec<u16> = vec![0; columns];
        for (idx, child) in self.children.iter().enumerate() {
            if child.layout.anchor.is_some() {
                continue;
            }
            let col = idx % columns;
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
            if let Some(slot) = col_mins.get_mut(col) {
                *slot = (*slot).max(outer_w);
            }
        }

        let mut total: u16 = padding.left.saturating_add(padding.right);
        if columns >= 2 {
            total = total.saturating_add(col_gap.saturating_mul(columns as u16 - 1));
        }
        for w in col_mins {
            total = total.saturating_add(w);
        }
        total
    }

    fn min_height(&self) -> u16 {
        let columns = self.columns.get().max(1);
        let padding = self.padding.get();
        let row_gap = self.row_gap.get();
        let scrollable = self.scrollable.get();

        let mut row_mins: Vec<u16> = Vec::new();
        for (idx, child) in self.children.iter().enumerate() {
            if child.layout.anchor.is_some() {
                continue;
            }
            let row = idx / columns;
            if row_mins.len() <= row {
                row_mins.resize(row.saturating_add(1), 0);
            }

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
            row_mins[row] = row_mins[row].max(outer_h);
        }

        let Some(first) = row_mins.first().copied() else {
            return padding.top.saturating_add(padding.bottom);
        };

        let rows = row_mins.len();
        let mut rows_total: u16 = if scrollable {
            row_mins.into_iter().max().unwrap_or(first)
        } else {
            row_mins.into_iter().fold(0, |acc, h| acc.saturating_add(h))
        };

        if !scrollable && rows >= 2 {
            rows_total = rows_total.saturating_add(row_gap.saturating_mul(rows as u16 - 1));
        }

        padding
            .top
            .saturating_add(padding.bottom)
            .saturating_add(rows_total)
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
        self.handle_event_capture_impl(event, ctx)
    }

    fn handle_event_bubble(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        self.handle_event_bubble_impl(event)
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.handle_event_impl(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.draw_impl(frame, area, ctx)
    }
}
