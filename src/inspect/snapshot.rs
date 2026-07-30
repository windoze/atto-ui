//! Snapshot / geometry / buffer helpers shared across the tree builders and
//! the inspector facade.
//!
//! These small utilities convert between ratatui and runtime rects, shorten
//! type names for display, stringify / crop buffers for snapshot export, and
//! compute click center points for coordinate-fallback dispatch.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::runtime::Rect as RuntimeRect;

pub(super) fn runtime_rect(rect: Rect) -> RuntimeRect {
    RuntimeRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

pub(super) fn short_type_name(full: &'static str) -> String {
    full.rsplit("::").next().unwrap_or(full).to_string()
}

pub(super) fn buffer_to_string(buffer: &Buffer) -> String {
    let mut out = String::new();
    let width = buffer.area.width;
    let height = buffer.area.height;
    for y in 0..height {
        for x in 0..width {
            if let Some(cell) = buffer.cell((x, y)) {
                out.push_str(cell.symbol());
            }
        }
        if y + 1 < height {
            out.push('\n');
        }
    }
    out
}

pub(super) fn crop_buffer(buffer: &Buffer, area: Rect) -> Buffer {
    let mut out = Buffer::empty(Rect::new(0, 0, area.width, area.height));
    for y in 0..area.height {
        for x in 0..area.width {
            let src_x = area.x.saturating_add(x);
            let src_y = area.y.saturating_add(y);
            if let Some(cell) = buffer.cell((src_x, src_y)) {
                out[(x, y)] = cell.clone();
            }
        }
    }
    out
}

pub(super) fn center_point(bounds: Rect) -> Option<(u16, u16)> {
    if bounds.width == 0 || bounds.height == 0 {
        return None;
    }
    let x = bounds.x.saturating_add(bounds.width / 2);
    let y = bounds.y.saturating_add(bounds.height / 2);
    Some((x, y))
}
