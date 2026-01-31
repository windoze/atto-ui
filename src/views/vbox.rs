use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::view::{View, ViewContext, ViewEventResult};

use super::layout::{add_signed, apply_padding};
use super::scroll::{
    ScrollbarDrag, ScrollbarHit, Scrollbars, clamp_scroll_offset, max_scroll_offset,
    scroll_offset_from_thumb_start, scrollbar_hit_test, scrollbar_layout_1d, should_show_scrollbar,
};
use super::{
    Align, Anchor, EdgeInsets, LayoutParams, ScrollConfig, ScrollOffset, Size, ViewId, ViewNode,
};

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

fn desired_size_for_slot(view: &dyn View, slot: Rect, layout: LayoutParams) -> (u16, u16) {
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
    (w, h)
}

pub struct VBox {
    id: ViewId,
    children: Vec<ViewNode>,
    padding: EdgeInsets,
    spacing: u16,
    focused: Option<ViewId>,
    last_area: Option<Rect>,
    scrollable: bool,
    scroll: ScrollOffset,
    content_size: (u16, u16),
    viewport_size: (u16, u16),
    scroll_config: ScrollConfig,
    scrollbars: Option<Scrollbars>,
    scrollbar_drag: Option<ScrollbarDrag>,
}

impl Default for VBox {
    fn default() -> Self {
        Self::new()
    }
}

impl VBox {
    pub fn new() -> Self {
        Self {
            id: ViewId::next(),
            children: Vec::new(),
            padding: EdgeInsets::ZERO,
            spacing: 0,
            focused: None,
            last_area: None,
            scrollable: false,
            scroll: ScrollOffset::ZERO,
            content_size: (0, 0),
            viewport_size: (0, 0),
            scroll_config: ScrollConfig::default(),
            scrollbars: None,
            scrollbar_drag: None,
        }
    }

    pub fn id(&self) -> ViewId {
        self.id
    }

    pub fn with_padding(mut self, padding: EdgeInsets) -> Self {
        self.padding = padding;
        self
    }

    pub fn with_spacing(mut self, spacing: u16) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn with_scrollable(mut self, scrollable: bool) -> Self {
        self.scrollable = scrollable;
        if !scrollable {
            self.scroll = ScrollOffset::ZERO;
        }
        self
    }

    pub fn with_scroll_config(mut self, config: ScrollConfig) -> Self {
        self.scroll_config = config;
        self
    }

    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    pub fn add_child(&mut self, view: Box<dyn View>) -> ViewId {
        self.add_child_with_layout(view, LayoutParams::default())
    }

    pub fn add_child_with_layout(&mut self, view: Box<dyn View>, layout: LayoutParams) -> ViewId {
        let mut node = ViewNode::new(view).with_layout(layout);
        node.parent = Some(self.id);
        let id = node.id;
        if self.focused.is_none() && node.view.is_focusable() {
            self.focused = Some(id);
        }
        self.children.push(node);
        id
    }

    pub fn remove_child(&mut self, id: ViewId) -> Option<ViewNode> {
        let idx = self.children.iter().position(|c| c.id == id)?;
        let removed = self.children.remove(idx);
        if self.focused == Some(id) {
            self.focused = self.first_focusable_child();
        }
        Some(removed)
    }

    pub fn child(&self, id: ViewId) -> Option<&ViewNode> {
        self.children.iter().find(|c| c.id == id)
    }

    pub fn child_mut(&mut self, id: ViewId) -> Option<&mut ViewNode> {
        self.children.iter_mut().find(|c| c.id == id)
    }

    fn first_focusable_child(&self) -> Option<ViewId> {
        self.children
            .iter()
            .find(|c| c.view.is_focusable())
            .map(|c| c.id)
    }

    fn focus_next(&mut self) {
        if self.children.is_empty() {
            self.focused = None;
            return;
        }

        let focusable: Vec<ViewId> = self
            .children
            .iter()
            .filter(|c| c.view.is_focusable())
            .map(|c| c.id)
            .collect();
        if focusable.is_empty() {
            self.focused = None;
            return;
        }

        let next = match self
            .focused
            .and_then(|id| focusable.iter().position(|x| *x == id))
        {
            Some(idx) => focusable[(idx + 1) % focusable.len()],
            None => focusable[0],
        };
        self.focused = Some(next);
    }

    fn focus_prev(&mut self) {
        if self.children.is_empty() {
            self.focused = None;
            return;
        }

        let focusable: Vec<ViewId> = self
            .children
            .iter()
            .filter(|c| c.view.is_focusable())
            .map(|c| c.id)
            .collect();
        if focusable.is_empty() {
            self.focused = None;
            return;
        }

        let prev = match self
            .focused
            .and_then(|id| focusable.iter().position(|x| *x == id))
        {
            Some(0) => focusable[focusable.len() - 1],
            Some(idx) => focusable[idx - 1],
            None => focusable[0],
        };
        self.focused = Some(prev);
    }

    fn scroll_by(&mut self, dx: i16, dy: i16) -> bool {
        if !self.scrollable {
            return false;
        }

        let desired = ScrollOffset {
            x: add_signed(self.scroll.x, dx),
            y: add_signed(self.scroll.y, dy),
        };
        let clamped = clamp_scroll_offset(self.content_size, self.viewport_size, desired);
        let changed = clamped != self.scroll;
        self.scroll = clamped;
        changed
    }

    fn scroll_to_clamped(&mut self, x: u16, y: u16) -> bool {
        if !self.scrollable {
            return false;
        }
        let desired = ScrollOffset { x, y };
        let clamped = clamp_scroll_offset(self.content_size, self.viewport_size, desired);
        let changed = clamped != self.scroll;
        self.scroll = clamped;
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
    ) -> Option<ViewId> {
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

        let content_x = viewport_x.saturating_add(self.scroll.x);
        let content_y = viewport_y.saturating_add(self.scroll.y);

        for child in self
            .children
            .iter()
            .rev()
            .filter(|c| c.layout.anchor.is_none())
        {
            if !Self::bounds_fully_visible(child.bounds(), self.scroll, viewport) {
                continue;
            }
            if contains(child.bounds(), content_x, content_y) {
                return Some(child.id);
            }
        }
        None
    }

    fn layout_children(&mut self, viewport_size: (u16, u16)) -> (u16, u16) {
        let (content_w, content_h) = viewport_size;
        let spacing = self.spacing;

        #[derive(Clone, Copy, Debug)]
        enum HeightSpec {
            Fixed(u16),
            Weight(u16),
        }

        let mut specs: Vec<Option<HeightSpec>> = vec![None; self.children.len()];
        let mut fixed_total: u16 = 0;
        let mut margin_total: u16 = 0;
        let mut weight_total: u16 = 0;
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

            let h = match child.layout.height {
                Size::Fixed(h) => HeightSpec::Fixed(h),
                Size::Content => HeightSpec::Fixed(child.view.desired_height().unwrap_or(1)),
                Size::Weight(w) => HeightSpec::Weight(w.max(1)),
                Size::Fill => HeightSpec::Weight(1),
            };
            match h {
                HeightSpec::Fixed(v) => fixed_total = fixed_total.saturating_add(v),
                HeightSpec::Weight(w) => weight_total = weight_total.saturating_add(w),
            }
            specs[idx] = Some(h);
        }

        if flow_count >= 2 && spacing > 0 {
            margin_total =
                margin_total.saturating_add(spacing.saturating_mul(flow_count as u16 - 1));
        }

        let available = content_h
            .saturating_sub(margin_total)
            .saturating_sub(fixed_total);
        let mut remaining = available;

        let mut allocated: Vec<u16> = vec![0; self.children.len()];
        if weight_total > 0 && remaining > 0 {
            let mut used: u16 = 0;
            for (idx, spec) in specs.iter().enumerate() {
                let Some(HeightSpec::Weight(w)) = spec else {
                    continue;
                };
                let share = ((remaining as u32) * (*w as u32) / (weight_total as u32))
                    .min(u16::MAX as u32) as u16;
                allocated[idx] = share;
                used = used.saturating_add(share);
            }
            remaining = remaining.saturating_sub(used);

            // Distribute any leftover 1-row remainders deterministically.
            if remaining > 0 {
                for (idx, spec) in specs.iter().enumerate() {
                    if remaining == 0 {
                        break;
                    }
                    if matches!(spec, Some(HeightSpec::Weight(_))) {
                        allocated[idx] = allocated[idx].saturating_add(1);
                        remaining = remaining.saturating_sub(1);
                    }
                }
            }
        }

        let mut cursor_y: u16 = 0;
        let mut first_flow = true;

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

                child.set_bounds(position_anchored(
                    viewport_size,
                    (desired_w, desired_h),
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

            if !self.scrollable && cursor_y >= content_h {
                child.set_bounds(Rect::default());
                continue;
            }

            let slot_h = match specs[idx] {
                Some(HeightSpec::Fixed(h)) => h,
                Some(HeightSpec::Weight(_)) => allocated[idx],
                None => 0,
            };

            let h = if self.scrollable {
                slot_h
            } else {
                let max_h = content_h.saturating_sub(cursor_y);
                slot_h.min(max_h)
            };

            let available_w = content_w.saturating_sub(margin.left.saturating_add(margin.right));
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

impl View for VBox {
    fn is_focusable(&self) -> bool {
        self.children.iter().any(|c| c.view.is_focusable())
    }

    fn children(&self) -> &[ViewNode] {
        &self.children
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ViewNode>> {
        Some(&mut self.children)
    }

    fn is_scrollable(&self) -> bool {
        self.scrollable
    }

    fn content_size(&self) -> (u16, u16) {
        self.content_size
    }

    fn scroll_offset(&self) -> (u16, u16) {
        (self.scroll.x, self.scroll.y)
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.viewport_size
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.scroll_config
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        let _ = self.scroll_to_clamped(x, y);
    }

    fn scroll_to_child(&mut self, child_id: ViewId) {
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

    fn handle_event_capture(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        if let Event::Key(KeyEvent {
            code: KeyCode::Tab,
            modifiers,
            ..
        }) = event
        {
            if modifiers.contains(KeyModifiers::SHIFT) {
                self.focus_prev();
            } else {
                self.focus_next();
            }
            return ViewEventResult::consumed();
        }
        if let Event::Key(KeyEvent {
            code: KeyCode::BackTab,
            ..
        }) = event
        {
            self.focus_prev();
            return ViewEventResult::consumed();
        }
        ViewEventResult::ignored()
    }

    fn handle_event_bubble(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        if !self.scrollable {
            return ViewEventResult::ignored();
        }

        match event {
            Event::Key(KeyEvent { code, kind, .. }) => {
                if matches!(kind, KeyEventKind::Release) {
                    return ViewEventResult::ignored();
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
                    ViewEventResult::consumed()
                } else {
                    ViewEventResult::ignored()
                }
            }
            Event::Mouse(m) => {
                let Some(area) = self.last_area else {
                    return ViewEventResult::ignored();
                };
                if mouse_coords_local_to_area(area, *m).is_none() {
                    return ViewEventResult::ignored();
                }

                let step = self.scroll_config.wheel_step as i16;
                let changed = match m.kind {
                    MouseEventKind::ScrollUp => self.scroll_by(0, -step),
                    MouseEventKind::ScrollDown => self.scroll_by(0, step),
                    MouseEventKind::ScrollLeft => self.scroll_by(-step, 0),
                    MouseEventKind::ScrollRight => self.scroll_by(step, 0),
                    _ => false,
                };

                if changed {
                    ViewEventResult::consumed()
                } else {
                    ViewEventResult::ignored()
                }
            }
            _ => ViewEventResult::ignored(),
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: ViewContext<'_>) -> ViewEventResult {
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

            let scrollbars = self.scrollbars.unwrap_or_else(|| {
                let viewport = Rect {
                    x: 0,
                    y: 0,
                    width: area.width,
                    height: area.height,
                };
                Scrollbars {
                    viewport,
                    content: apply_padding(viewport, self.padding),
                    vbar: None,
                    hbar: None,
                    thickness: self.scroll_config.scrollbar_thickness.max(1),
                }
            });

            if self.scrollable {
                // If we started a thumb drag, keep consuming drag/up events.
                if let Some(drag) = self.scrollbar_drag {
                    match m.kind {
                        MouseEventKind::Drag(MouseButton::Left) => match drag {
                            ScrollbarDrag::Vertical { grab_offset } => {
                                let Some(vbar) = scrollbars.vbar else {
                                    self.scrollbar_drag = None;
                                    return ViewEventResult::consumed();
                                };
                                if vbar.height == 0 {
                                    return ViewEventResult::consumed();
                                }
                                let layout = scrollbar_layout_1d(
                                    vbar.height,
                                    scrollbars.content.height,
                                    self.content_size.1,
                                    self.scroll.y,
                                    self.scroll_config.arrows,
                                );
                                if layout.track_len == 0 {
                                    return ViewEventResult::consumed();
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
                                let _ = self.scroll_to_clamped(self.scroll.x, new_off);
                                return ViewEventResult::consumed();
                            }
                            ScrollbarDrag::Horizontal { grab_offset } => {
                                let Some(hbar) = scrollbars.hbar else {
                                    self.scrollbar_drag = None;
                                    return ViewEventResult::consumed();
                                };
                                if hbar.width == 0 {
                                    return ViewEventResult::consumed();
                                }
                                let layout = scrollbar_layout_1d(
                                    hbar.width,
                                    scrollbars.content.width,
                                    self.content_size.0,
                                    self.scroll.x,
                                    self.scroll_config.arrows,
                                );
                                if layout.track_len == 0 {
                                    return ViewEventResult::consumed();
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
                                let _ = self.scroll_to_clamped(new_off, self.scroll.y);
                                return ViewEventResult::consumed();
                            }
                        },
                        MouseEventKind::Up(MouseButton::Left) => {
                            self.scrollbar_drag = None;
                            return ViewEventResult::consumed();
                        }
                        _ => {}
                    }
                }

                if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                    if let Some(vbar) = scrollbars.vbar
                        && contains(vbar, local_x, local_y)
                        && vbar.height > 0
                    {
                        let pos = local_y.saturating_sub(vbar.y);
                        let layout = scrollbar_layout_1d(
                            vbar.height,
                            scrollbars.content.height,
                            self.content_size.1,
                            self.scroll.y,
                            self.scroll_config.arrows,
                        );
                        match scrollbar_hit_test(layout, pos) {
                            ScrollbarHit::ArrowDec => {
                                let _ = self.scroll_by(0, -1);
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::ArrowInc => {
                                let _ = self.scroll_by(0, 1);
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::Thumb { grab_offset } => {
                                self.scrollbar_drag = Some(ScrollbarDrag::Vertical { grab_offset });
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::TrackDec => {
                                let page = scrollbars.content.height as i16;
                                let _ = self.scroll_by(0, -(page));
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::TrackInc => {
                                let page = scrollbars.content.height as i16;
                                let _ = self.scroll_by(0, page);
                                return ViewEventResult::consumed();
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
                            self.scroll.x,
                            self.scroll_config.arrows,
                        );
                        match scrollbar_hit_test(layout, pos) {
                            ScrollbarHit::ArrowDec => {
                                let _ = self.scroll_by(-1, 0);
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::ArrowInc => {
                                let _ = self.scroll_by(1, 0);
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::Thumb { grab_offset } => {
                                self.scrollbar_drag =
                                    Some(ScrollbarDrag::Horizontal { grab_offset });
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::TrackDec => {
                                let page = scrollbars.content.width as i16;
                                let _ = self.scroll_by(-(page), 0);
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::TrackInc => {
                                let page = scrollbars.content.width as i16;
                                let _ = self.scroll_by(page, 0);
                                return ViewEventResult::consumed();
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
            let point_x = if is_anchored {
                content_x
            } else {
                content_x.saturating_add(self.scroll.x)
            };
            let point_y = if is_anchored {
                content_y
            } else {
                content_y.saturating_add(self.scroll.y)
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
            let child_ctx = ViewContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
            };

            let res = self.children[child_idx]
                .view
                .handle_event(&child_event, child_ctx);
            if res.is_consumed() {
                return res;
            }

            if focus_changed {
                return ViewEventResult::consumed();
            }

            return self.handle_event_bubble(event, ctx);
        }

        // Keyboard/paste/etc: send to focused child first.
        if let Some(child_id) = self.focused.or_else(|| self.first_focusable_child())
            && let Some(child_idx) = self.children.iter().position(|c| c.id == child_id)
        {
            self.focused = Some(child_id);
            let child_focused = ctx.is_focused && self.focused == Some(child_id);
            let child_ctx = ViewContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
            };
            let res = self.children[child_idx].view.handle_event(event, child_ctx);
            if res.is_consumed() {
                return res;
            }
        }

        self.handle_event_bubble(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.last_area = Some(area);

        let thickness = self.scroll_config.scrollbar_thickness.max(1);

        let mut viewport_outer = area;
        let mut inner = apply_padding(viewport_outer, self.padding);
        let mut show_v = false;
        let mut show_h = false;

        if self.scrollable {
            if matches!(ctx.scrollbar_host, crate::view::ScrollbarHost::View) {
                for _ in 0..2 {
                    inner = apply_padding(viewport_outer, self.padding);
                    self.viewport_size = (inner.width, inner.height);
                    self.content_size = self.layout_children((inner.width, inner.height));

                    let new_show_v = should_show_scrollbar(
                        self.scroll_config.vertical_scrollbar,
                        self.content_size.1,
                        self.viewport_size.1,
                    );
                    let new_show_h = should_show_scrollbar(
                        self.scroll_config.horizontal_scrollbar,
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
                inner = apply_padding(area, self.padding);
                self.viewport_size = (inner.width, inner.height);
                self.content_size = self.layout_children((inner.width, inner.height));
                show_v = should_show_scrollbar(
                    self.scroll_config.vertical_scrollbar,
                    self.content_size.1,
                    self.viewport_size.1,
                );
                show_h = should_show_scrollbar(
                    self.scroll_config.horizontal_scrollbar,
                    self.content_size.0,
                    self.viewport_size.0,
                );
            }
        } else {
            inner = apply_padding(area, self.padding);
            self.viewport_size = (inner.width, inner.height);
            self.content_size = self.layout_children((inner.width, inner.height));
        }

        self.scroll = clamp_scroll_offset(self.content_size, self.viewport_size, self.scroll);

        if self.scrollable && matches!(ctx.scrollbar_host, crate::view::ScrollbarHost::View) {
            let viewport_local = Rect {
                x: viewport_outer.x.saturating_sub(area.x),
                y: viewport_outer.y.saturating_sub(area.y),
                width: viewport_outer.width,
                height: viewport_outer.height,
            };
            let content_local = apply_padding(viewport_local, self.padding);
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

        let scrollable = self.scrollable;
        let scroll = self.scroll;
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
            let child_ctx = ViewContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
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
            let child_ctx = ViewContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
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
                self.scroll_config.arrows,
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
                self.scroll_config.arrows,
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

pub struct HBox {
    id: ViewId,
    children: Vec<ViewNode>,
    padding: EdgeInsets,
    spacing: u16,
    focused: Option<ViewId>,
    last_area: Option<Rect>,
    scrollable: bool,
    scroll: ScrollOffset,
    content_size: (u16, u16),
    viewport_size: (u16, u16),
    scroll_config: ScrollConfig,
    scrollbars: Option<Scrollbars>,
    scrollbar_drag: Option<ScrollbarDrag>,
}

impl Default for HBox {
    fn default() -> Self {
        Self::new()
    }
}

impl HBox {
    pub fn new() -> Self {
        Self {
            id: ViewId::next(),
            children: Vec::new(),
            padding: EdgeInsets::ZERO,
            spacing: 0,
            focused: None,
            last_area: None,
            scrollable: false,
            scroll: ScrollOffset::ZERO,
            content_size: (0, 0),
            viewport_size: (0, 0),
            scroll_config: ScrollConfig::default(),
            scrollbars: None,
            scrollbar_drag: None,
        }
    }

    pub fn id(&self) -> ViewId {
        self.id
    }

    pub fn with_padding(mut self, padding: EdgeInsets) -> Self {
        self.padding = padding;
        self
    }

    pub fn with_spacing(mut self, spacing: u16) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn with_scrollable(mut self, scrollable: bool) -> Self {
        self.scrollable = scrollable;
        if !scrollable {
            self.scroll = ScrollOffset::ZERO;
        }
        self
    }

    pub fn with_scroll_config(mut self, config: ScrollConfig) -> Self {
        self.scroll_config = config;
        self
    }

    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    pub fn add_child(&mut self, view: Box<dyn View>) -> ViewId {
        self.add_child_with_layout(view, LayoutParams::default())
    }

    pub fn add_child_with_layout(&mut self, view: Box<dyn View>, layout: LayoutParams) -> ViewId {
        let mut node = ViewNode::new(view).with_layout(layout);
        node.parent = Some(self.id);
        let id = node.id;
        if self.focused.is_none() && node.view.is_focusable() {
            self.focused = Some(id);
        }
        self.children.push(node);
        id
    }

    pub fn remove_child(&mut self, id: ViewId) -> Option<ViewNode> {
        let idx = self.children.iter().position(|c| c.id == id)?;
        let removed = self.children.remove(idx);
        if self.focused == Some(id) {
            self.focused = self.first_focusable_child();
        }
        Some(removed)
    }

    pub fn child(&self, id: ViewId) -> Option<&ViewNode> {
        self.children.iter().find(|c| c.id == id)
    }

    pub fn child_mut(&mut self, id: ViewId) -> Option<&mut ViewNode> {
        self.children.iter_mut().find(|c| c.id == id)
    }

    fn first_focusable_child(&self) -> Option<ViewId> {
        self.children
            .iter()
            .find(|c| c.view.is_focusable())
            .map(|c| c.id)
    }

    fn focus_next(&mut self) {
        if self.children.is_empty() {
            self.focused = None;
            return;
        }

        let focusable: Vec<ViewId> = self
            .children
            .iter()
            .filter(|c| c.view.is_focusable())
            .map(|c| c.id)
            .collect();
        if focusable.is_empty() {
            self.focused = None;
            return;
        }

        let next = match self
            .focused
            .and_then(|id| focusable.iter().position(|x| *x == id))
        {
            Some(idx) => focusable[(idx + 1) % focusable.len()],
            None => focusable[0],
        };
        self.focused = Some(next)
    }

    fn focus_prev(&mut self) {
        if self.children.is_empty() {
            self.focused = None;
            return;
        }

        let focusable: Vec<ViewId> = self
            .children
            .iter()
            .filter(|c| c.view.is_focusable())
            .map(|c| c.id)
            .collect();
        if focusable.is_empty() {
            self.focused = None;
            return;
        }

        let prev = match self
            .focused
            .and_then(|id| focusable.iter().position(|x| *x == id))
        {
            Some(0) => focusable[focusable.len() - 1],
            Some(idx) => focusable[idx - 1],
            None => focusable[0],
        };
        self.focused = Some(prev);
    }

    fn scroll_by(&mut self, dx: i16, dy: i16) -> bool {
        if !self.scrollable {
            return false;
        }

        let desired = ScrollOffset {
            x: add_signed(self.scroll.x, dx),
            y: add_signed(self.scroll.y, dy),
        };
        let clamped = clamp_scroll_offset(self.content_size, self.viewport_size, desired);
        let changed = clamped != self.scroll;
        self.scroll = clamped;
        changed
    }

    fn scroll_to_clamped(&mut self, x: u16, y: u16) -> bool {
        if !self.scrollable {
            return false;
        }
        let desired = ScrollOffset { x, y };
        let clamped = clamp_scroll_offset(self.content_size, self.viewport_size, desired);
        let changed = clamped != self.scroll;
        self.scroll = clamped;
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
    ) -> Option<ViewId> {
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

        let content_x = viewport_x.saturating_add(self.scroll.x);
        let content_y = viewport_y.saturating_add(self.scroll.y);

        for child in self
            .children
            .iter()
            .rev()
            .filter(|c| c.layout.anchor.is_none())
        {
            if !Self::bounds_fully_visible(child.bounds(), self.scroll, viewport) {
                continue;
            }
            if contains(child.bounds(), content_x, content_y) {
                return Some(child.id);
            }
        }
        None
    }

    fn layout_children(&mut self, viewport_size: (u16, u16)) -> (u16, u16) {
        let (content_w, content_h) = viewport_size;
        let spacing = self.spacing;

        #[derive(Clone, Copy, Debug)]
        enum WidthSpec {
            Fixed(u16),
            Weight(u16),
        }

        let mut specs: Vec<Option<WidthSpec>> = vec![None; self.children.len()];
        let mut fixed_total: u16 = 0;
        let mut margin_total: u16 = 0;
        let mut weight_total: u16 = 0;
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

            let w = match child.layout.width {
                Size::Fixed(w) => WidthSpec::Fixed(w),
                Size::Content => WidthSpec::Fixed(child.view.desired_width().unwrap_or(1)),
                Size::Weight(w) => WidthSpec::Weight(w.max(1)),
                Size::Fill => WidthSpec::Weight(1),
            };
            match w {
                WidthSpec::Fixed(v) => fixed_total = fixed_total.saturating_add(v),
                WidthSpec::Weight(v) => weight_total = weight_total.saturating_add(v),
            }
            specs[idx] = Some(w);
        }

        if flow_count >= 2 && spacing > 0 {
            margin_total =
                margin_total.saturating_add(spacing.saturating_mul(flow_count as u16 - 1));
        }

        let available = content_w
            .saturating_sub(margin_total)
            .saturating_sub(fixed_total);
        let mut remaining = available;

        let mut allocated: Vec<u16> = vec![0; self.children.len()];
        if weight_total > 0 && remaining > 0 {
            let mut used: u16 = 0;
            for (idx, spec) in specs.iter().enumerate() {
                let Some(WidthSpec::Weight(w)) = spec else {
                    continue;
                };
                let share = ((remaining as u32) * (*w as u32) / (weight_total as u32))
                    .min(u16::MAX as u32) as u16;
                allocated[idx] = share;
                used = used.saturating_add(share);
            }
            remaining = remaining.saturating_sub(used);

            if remaining > 0 {
                for (idx, spec) in specs.iter().enumerate() {
                    if remaining == 0 {
                        break;
                    }
                    if matches!(spec, Some(WidthSpec::Weight(_))) {
                        allocated[idx] = allocated[idx].saturating_add(1);
                        remaining = remaining.saturating_sub(1);
                    }
                }
            }
        }

        let mut cursor_x: u16 = 0;
        let mut first_flow = true;

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

                child.set_bounds(position_anchored(
                    viewport_size,
                    (desired_w, desired_h),
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

            if !self.scrollable && cursor_x >= content_w {
                child.set_bounds(Rect::default());
                continue;
            }

            let slot_w = match specs[idx] {
                Some(WidthSpec::Fixed(w)) => w,
                Some(WidthSpec::Weight(_)) => allocated[idx],
                None => 0,
            };

            let w = if self.scrollable {
                slot_w
            } else {
                let max_w = content_w.saturating_sub(cursor_x);
                slot_w.min(max_w)
            };

            let available_h = content_h.saturating_sub(margin.top.saturating_add(margin.bottom));
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

impl View for HBox {
    fn is_focusable(&self) -> bool {
        self.children.iter().any(|c| c.view.is_focusable())
    }

    fn children(&self) -> &[ViewNode] {
        &self.children
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ViewNode>> {
        Some(&mut self.children)
    }

    fn is_scrollable(&self) -> bool {
        self.scrollable
    }

    fn content_size(&self) -> (u16, u16) {
        self.content_size
    }

    fn scroll_offset(&self) -> (u16, u16) {
        (self.scroll.x, self.scroll.y)
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.viewport_size
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.scroll_config
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        let _ = self.scroll_to_clamped(x, y);
    }

    fn scroll_to_child(&mut self, child_id: ViewId) {
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

    fn handle_event_capture(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        if let Event::Key(KeyEvent {
            code: KeyCode::Tab,
            modifiers,
            ..
        }) = event
        {
            if modifiers.contains(KeyModifiers::SHIFT) {
                self.focus_prev();
            } else {
                self.focus_next();
            }
            return ViewEventResult::consumed();
        }
        if let Event::Key(KeyEvent {
            code: KeyCode::BackTab,
            ..
        }) = event
        {
            self.focus_prev();
            return ViewEventResult::consumed();
        }
        ViewEventResult::ignored()
    }

    fn handle_event_bubble(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        if !self.scrollable {
            return ViewEventResult::ignored();
        }

        match event {
            Event::Key(KeyEvent { code, kind, .. }) => {
                if matches!(kind, KeyEventKind::Release) {
                    return ViewEventResult::ignored();
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
                    ViewEventResult::consumed()
                } else {
                    ViewEventResult::ignored()
                }
            }
            Event::Mouse(m) => {
                let Some(area) = self.last_area else {
                    return ViewEventResult::ignored();
                };
                if mouse_coords_local_to_area(area, *m).is_none() {
                    return ViewEventResult::ignored();
                }

                let step = self.scroll_config.wheel_step as i16;
                let changed = match m.kind {
                    MouseEventKind::ScrollUp => self.scroll_by(0, -step),
                    MouseEventKind::ScrollDown => self.scroll_by(0, step),
                    MouseEventKind::ScrollLeft => self.scroll_by(-step, 0),
                    MouseEventKind::ScrollRight => self.scroll_by(step, 0),
                    _ => false,
                };

                if changed {
                    ViewEventResult::consumed()
                } else {
                    ViewEventResult::ignored()
                }
            }
            _ => ViewEventResult::ignored(),
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: ViewContext<'_>) -> ViewEventResult {
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

            let scrollbars = self.scrollbars.unwrap_or_else(|| {
                let viewport = Rect {
                    x: 0,
                    y: 0,
                    width: area.width,
                    height: area.height,
                };
                Scrollbars {
                    viewport,
                    content: apply_padding(viewport, self.padding),
                    vbar: None,
                    hbar: None,
                    thickness: self.scroll_config.scrollbar_thickness.max(1),
                }
            });

            if self.scrollable {
                if let Some(drag) = self.scrollbar_drag {
                    match m.kind {
                        MouseEventKind::Drag(MouseButton::Left) => match drag {
                            ScrollbarDrag::Vertical { grab_offset } => {
                                let Some(vbar) = scrollbars.vbar else {
                                    self.scrollbar_drag = None;
                                    return ViewEventResult::consumed();
                                };
                                if vbar.height == 0 {
                                    return ViewEventResult::consumed();
                                }
                                let layout = scrollbar_layout_1d(
                                    vbar.height,
                                    scrollbars.content.height,
                                    self.content_size.1,
                                    self.scroll.y,
                                    self.scroll_config.arrows,
                                );
                                if layout.track_len == 0 {
                                    return ViewEventResult::consumed();
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
                                let _ = self.scroll_to_clamped(self.scroll.x, new_off);
                                return ViewEventResult::consumed();
                            }
                            ScrollbarDrag::Horizontal { grab_offset } => {
                                let Some(hbar) = scrollbars.hbar else {
                                    self.scrollbar_drag = None;
                                    return ViewEventResult::consumed();
                                };
                                if hbar.width == 0 {
                                    return ViewEventResult::consumed();
                                }
                                let layout = scrollbar_layout_1d(
                                    hbar.width,
                                    scrollbars.content.width,
                                    self.content_size.0,
                                    self.scroll.x,
                                    self.scroll_config.arrows,
                                );
                                if layout.track_len == 0 {
                                    return ViewEventResult::consumed();
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
                                let _ = self.scroll_to_clamped(new_off, self.scroll.y);
                                return ViewEventResult::consumed();
                            }
                        },
                        MouseEventKind::Up(MouseButton::Left) => {
                            self.scrollbar_drag = None;
                            return ViewEventResult::consumed();
                        }
                        _ => {}
                    }
                }

                if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                    if let Some(vbar) = scrollbars.vbar
                        && contains(vbar, local_x, local_y)
                        && vbar.height > 0
                    {
                        let pos = local_y.saturating_sub(vbar.y);
                        let layout = scrollbar_layout_1d(
                            vbar.height,
                            scrollbars.content.height,
                            self.content_size.1,
                            self.scroll.y,
                            self.scroll_config.arrows,
                        );
                        match scrollbar_hit_test(layout, pos) {
                            ScrollbarHit::ArrowDec => {
                                let _ = self.scroll_by(0, -1);
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::ArrowInc => {
                                let _ = self.scroll_by(0, 1);
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::Thumb { grab_offset } => {
                                self.scrollbar_drag = Some(ScrollbarDrag::Vertical { grab_offset });
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::TrackDec => {
                                let page = scrollbars.content.height as i16;
                                let _ = self.scroll_by(0, -(page));
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::TrackInc => {
                                let page = scrollbars.content.height as i16;
                                let _ = self.scroll_by(0, page);
                                return ViewEventResult::consumed();
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
                            self.scroll.x,
                            self.scroll_config.arrows,
                        );
                        match scrollbar_hit_test(layout, pos) {
                            ScrollbarHit::ArrowDec => {
                                let _ = self.scroll_by(-1, 0);
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::ArrowInc => {
                                let _ = self.scroll_by(1, 0);
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::Thumb { grab_offset } => {
                                self.scrollbar_drag =
                                    Some(ScrollbarDrag::Horizontal { grab_offset });
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::TrackDec => {
                                let page = scrollbars.content.width as i16;
                                let _ = self.scroll_by(-(page), 0);
                                return ViewEventResult::consumed();
                            }
                            ScrollbarHit::TrackInc => {
                                let page = scrollbars.content.width as i16;
                                let _ = self.scroll_by(page, 0);
                                return ViewEventResult::consumed();
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
            let point_x = if is_anchored {
                content_x
            } else {
                content_x.saturating_add(self.scroll.x)
            };
            let point_y = if is_anchored {
                content_y
            } else {
                content_y.saturating_add(self.scroll.y)
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
            let child_ctx = ViewContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
            };

            let res = self.children[child_idx]
                .view
                .handle_event(&child_event, child_ctx);
            if res.is_consumed() {
                return res;
            }

            if focus_changed {
                return ViewEventResult::consumed();
            }

            return self.handle_event_bubble(event, ctx);
        }

        if let Some(child_id) = self.focused.or_else(|| self.first_focusable_child())
            && let Some(child_idx) = self.children.iter().position(|c| c.id == child_id)
        {
            self.focused = Some(child_id);
            let child_focused = ctx.is_focused && self.focused == Some(child_id);
            let child_ctx = ViewContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
            };
            let res = self.children[child_idx].view.handle_event(event, child_ctx);
            if res.is_consumed() {
                return res;
            }
        }

        self.handle_event_bubble(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.last_area = Some(area);

        let thickness = self.scroll_config.scrollbar_thickness.max(1);

        let mut viewport_outer = area;
        let mut inner = apply_padding(viewport_outer, self.padding);
        let mut show_v = false;
        let mut show_h = false;

        if self.scrollable {
            if matches!(ctx.scrollbar_host, crate::view::ScrollbarHost::View) {
                for _ in 0..2 {
                    inner = apply_padding(viewport_outer, self.padding);
                    self.viewport_size = (inner.width, inner.height);
                    self.content_size = self.layout_children((inner.width, inner.height));

                    let new_show_v = should_show_scrollbar(
                        self.scroll_config.vertical_scrollbar,
                        self.content_size.1,
                        self.viewport_size.1,
                    );
                    let new_show_h = should_show_scrollbar(
                        self.scroll_config.horizontal_scrollbar,
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
                inner = apply_padding(area, self.padding);
                self.viewport_size = (inner.width, inner.height);
                self.content_size = self.layout_children((inner.width, inner.height));
                show_v = should_show_scrollbar(
                    self.scroll_config.vertical_scrollbar,
                    self.content_size.1,
                    self.viewport_size.1,
                );
                show_h = should_show_scrollbar(
                    self.scroll_config.horizontal_scrollbar,
                    self.content_size.0,
                    self.viewport_size.0,
                );
            }
        } else {
            inner = apply_padding(area, self.padding);
            self.viewport_size = (inner.width, inner.height);
            self.content_size = self.layout_children((inner.width, inner.height));
        }

        self.scroll = clamp_scroll_offset(self.content_size, self.viewport_size, self.scroll);

        if self.scrollable && matches!(ctx.scrollbar_host, crate::view::ScrollbarHost::View) {
            let viewport_local = Rect {
                x: viewport_outer.x.saturating_sub(area.x),
                y: viewport_outer.y.saturating_sub(area.y),
                width: viewport_outer.width,
                height: viewport_outer.height,
            };
            let content_local = apply_padding(viewport_local, self.padding);
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

        let scrollable = self.scrollable;
        let scroll = self.scroll;
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
            let child_ctx = ViewContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
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
            let child_ctx = ViewContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: child_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
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
                self.scroll_config.arrows,
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
                self.scroll_config.arrows,
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
