//! Generic, menu-bar-agnostic navigation over a menu tree.
//!
//! These helpers operate on a root `&[MenuItem]` slice plus a selection `stack`
//! (`stack[depth]` is the highlighted row at that submenu depth). They are shared
//! by [`MenuBar`](super::model::MenuBar) dropdowns and the standalone
//! [`PopupMenu`](super::popup::PopupMenu); neither the menu-bar row nor an anchor
//! is baked in — the caller supplies the origin.

use ratatui::layout::Rect;

use super::layout::{display_label, dropdown_size, explicit_mnemonic};
use super::model::MenuItem;

/// Returns the item slice at the level addressed by all but the last `stack` entry.
pub(super) fn items_at<'a>(root: &'a [MenuItem], stack: &[usize]) -> Option<&'a [MenuItem]> {
    let mut items: &[MenuItem] = root;
    for &idx in stack.iter().take(stack.len().saturating_sub(1)) {
        let item = items.get(idx)?;
        items = &item.submenu;
    }
    Some(items)
}

/// Returns the currently selected item (deepest level, last `stack` index).
pub(super) fn item_at<'a>(root: &'a [MenuItem], stack: &[usize]) -> Option<&'a MenuItem> {
    let items = items_at(root, stack)?;
    let idx = *stack.last().unwrap_or(&0);
    items.get(idx)
}

/// Moves the selection within the current (deepest) level, wrapping at the ends.
pub(super) fn move_in_stack(root: &[MenuItem], stack: &mut Vec<usize>, delta: i32) {
    let Some(items) = items_at(root, stack) else {
        return;
    };
    if items.is_empty() {
        return;
    }
    if stack.is_empty() {
        stack.push(0);
    }
    let depth = stack.len().saturating_sub(1);
    let cur = stack[depth] as i32;
    let mut next = cur + delta;
    if next < 0 {
        next = items.len() as i32 - 1;
    }
    if next as usize >= items.len() {
        next = 0;
    }
    stack[depth] = next as usize;
}

/// Opens the currently selected item's submenu (pushes a new depth). Returns
/// `true` if a submenu was opened.
pub(super) fn open_submenu(root: &[MenuItem], stack: &mut Vec<usize>) -> bool {
    let Some(item) = item_at(root, stack) else {
        return false;
    };
    if item.submenu.is_empty() {
        return false;
    }
    stack.push(0);
    true
}

/// Computes the rect and item slice for each open dropdown level, starting at
/// `origin` and cascading right/down. Rects are unclamped, matching the
/// menu-bar dropdown geometry; callers that need on-screen placement clamp the
/// root origin beforehand (see [`super::layout::clamp_and_flip_anchor`]).
pub(super) fn levels_from_origin<'a>(
    root: &'a [MenuItem],
    stack: &[usize],
    origin: (u16, u16),
) -> Vec<(Rect, &'a [MenuItem])> {
    let mut items: &[MenuItem] = root;
    let mut origin_x = origin.0;
    let mut origin_y = origin.1;
    let mut levels = Vec::new();

    for (depth, &selected_idx) in stack.iter().enumerate() {
        let (w, h) = dropdown_size(items);
        let rect = Rect {
            x: origin_x,
            y: origin_y,
            width: w,
            height: h,
        };
        levels.push((rect, items));

        let Some(sel_item) = items.get(selected_idx) else {
            break;
        };
        if depth + 1 < stack.len() {
            items = &sel_item.submenu;
            origin_x = rect.x.saturating_add(rect.width);
            origin_y = rect.y.saturating_add(1 + selected_idx as u16);
        }
    }
    levels
}

/// The mnemonic character used to match a keypress against an item: explicit
/// `&`/`_` marker, else a single-char shortcut, else the first label character.
pub(super) fn item_mnemonic_or_first(item: &MenuItem) -> Option<char> {
    let label = item.label.get();
    item.mnemonic
        .get()
        .or_else(|| explicit_mnemonic(&label))
        .or_else(|| single_char_shortcut(item.shortcut.get()))
        .or_else(|| display_label(&label).text.trim_start().chars().next())
}

fn single_char_shortcut(shortcut: Option<String>) -> Option<char> {
    let shortcut = shortcut?;
    let mut chars = shortcut.chars();
    match (chars.next(), chars.next()) {
        (Some(ch), None) => Some(ch),
        _ => None,
    }
}
