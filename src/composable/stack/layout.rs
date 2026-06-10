use ratatui::layout::Rect;

use crate::composable::{Component, LayoutParams, Size};

pub(super) fn desired_size_for_slot(
    view: &dyn Component,
    slot: Rect,
    layout: LayoutParams,
) -> (u16, u16) {
    let min_w = view.min_width();
    let min_h = view.min_height();
    // Fill/Weight fill the slot they were allocated (flex semantics); Content and
    // Fixed size to their intrinsic/fixed value and are then aligned within the slot.
    let w = match layout.width {
        Size::Fixed(w) => w,
        Size::Content => view.desired_width().unwrap_or(slot.width),
        Size::Fill | Size::Weight(_) => slot.width,
    };
    let h = match layout.height {
        Size::Fixed(h) => h,
        Size::Content => view.desired_height().unwrap_or(slot.height),
        Size::Fill | Size::Weight(_) => slot.height,
    };
    (w.max(min_w), h.max(min_h))
}
