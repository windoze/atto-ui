use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::layout::Rect;
use std::cmp::Ordering;

use super::layout::{Align, Anchor, add_signed};
use super::node::{ComponentId, ComponentNode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabDirection {
    Next,
    Prev,
}

pub(crate) fn tab_direction_for_event(event: &Event) -> Option<TabDirection> {
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

pub(crate) fn focusable_children_in_tab_order(children: &[ComponentNode]) -> Vec<ComponentId> {
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

pub(crate) fn contains(rect: Rect, x: u16, y: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

pub(crate) fn clamp_u16(v: u16, min: u16, max: u16) -> u16 {
    if v < min {
        min
    } else if v > max {
        max
    } else {
        v
    }
}

pub(crate) fn mouse_coords_local_to_area(area: Rect, m: MouseEvent) -> Option<(u16, u16)> {
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

pub(crate) fn position_anchored(
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

pub(crate) fn align_within(slot: Rect, desired: (u16, u16), align_x: Align, align_y: Align) -> Rect {
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
