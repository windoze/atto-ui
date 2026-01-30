use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::view::{View, ViewContext, ViewEventResult};

use super::layout::{add_signed, apply_padding, apply_padding_local};
use super::{Align, Anchor, EdgeInsets, LayoutParams, Size, ViewId, ViewNode};

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

    fn hit_test_child(&self, x: u16, y: u16) -> Option<ViewId> {
        for child in self
            .children
            .iter()
            .rev()
            .filter(|c| c.layout.anchor.is_some())
        {
            if contains(child.bounds(), x, y) {
                return Some(child.id);
            }
        }

        for child in self
            .children
            .iter()
            .rev()
            .filter(|c| c.layout.anchor.is_none())
        {
            if contains(child.bounds(), x, y) {
                return Some(child.id);
            }
        }
        None
    }

    fn layout_children(&mut self, content_size: (u16, u16)) {
        let (content_w, content_h) = content_size;
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
                    content_size,
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

            if cursor_y >= content_h {
                child.set_bounds(Rect::default());
                continue;
            }

            let slot_h = match specs[idx] {
                Some(HeightSpec::Fixed(h)) => h,
                Some(HeightSpec::Weight(_)) => allocated[idx],
                None => 0,
            };

            let max_h = content_h.saturating_sub(cursor_y);
            let h = slot_h.min(max_h);

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

            let content_size = apply_padding_local((area.width, area.height), self.padding);

            // Ignore clicks in the padding or outside the last known viewport.
            if local_x < self.padding.left
                || local_y < self.padding.top
                || local_x >= self.padding.left.saturating_add(content_size.0)
                || local_y >= self.padding.top.saturating_add(content_size.1)
            {
                return self.handle_event_bubble(event, ctx);
            }

            let content_x = local_x.saturating_sub(self.padding.left);
            let content_y = local_y.saturating_sub(self.padding.top);

            let Some(child_id) = self.hit_test_child(content_x, content_y) else {
                return self.handle_event_bubble(event, ctx);
            };
            let Some(child_idx) = self.children.iter().position(|c| c.id == child_id) else {
                return self.handle_event_bubble(event, ctx);
            };

            let child_bounds = self.children[child_idx].bounds();
            let child_x = content_x.saturating_sub(child_bounds.x);
            let child_y = content_y.saturating_sub(child_bounds.y);

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

        let inner = apply_padding(area, self.padding);
        self.layout_children((inner.width, inner.height));

        for child in self
            .children
            .iter_mut()
            .filter(|c| c.layout.anchor.is_none())
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
            };
            child.view.draw(frame, abs, child_ctx);
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

    fn hit_test_child(&self, x: u16, y: u16) -> Option<ViewId> {
        for child in self
            .children
            .iter()
            .rev()
            .filter(|c| c.layout.anchor.is_some())
        {
            if contains(child.bounds(), x, y) {
                return Some(child.id);
            }
        }

        for child in self
            .children
            .iter()
            .rev()
            .filter(|c| c.layout.anchor.is_none())
        {
            if contains(child.bounds(), x, y) {
                return Some(child.id);
            }
        }
        None
    }

    fn layout_children(&mut self, content_size: (u16, u16)) {
        let (content_w, content_h) = content_size;
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
                    content_size,
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

            if cursor_x >= content_w {
                child.set_bounds(Rect::default());
                continue;
            }

            let slot_w = match specs[idx] {
                Some(WidthSpec::Fixed(w)) => w,
                Some(WidthSpec::Weight(_)) => allocated[idx],
                None => 0,
            };

            let max_w = content_w.saturating_sub(cursor_x);
            let w = slot_w.min(max_w);

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

            let content_size = apply_padding_local((area.width, area.height), self.padding);

            if local_x < self.padding.left
                || local_y < self.padding.top
                || local_x >= self.padding.left.saturating_add(content_size.0)
                || local_y >= self.padding.top.saturating_add(content_size.1)
            {
                return self.handle_event_bubble(event, ctx);
            }

            let content_x = local_x.saturating_sub(self.padding.left);
            let content_y = local_y.saturating_sub(self.padding.top);

            let Some(child_id) = self.hit_test_child(content_x, content_y) else {
                return self.handle_event_bubble(event, ctx);
            };
            let Some(child_idx) = self.children.iter().position(|c| c.id == child_id) else {
                return self.handle_event_bubble(event, ctx);
            };

            let child_bounds = self.children[child_idx].bounds();
            let child_x = content_x.saturating_sub(child_bounds.x);
            let child_y = content_y.saturating_sub(child_bounds.y);

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

        let inner = apply_padding(area, self.padding);
        self.layout_children((inner.width, inner.height));

        for child in self
            .children
            .iter_mut()
            .filter(|c| c.layout.anchor.is_none())
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
            };
            child.view.draw(frame, abs, child_ctx);
        }
    }
}
