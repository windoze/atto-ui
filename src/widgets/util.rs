//! Shared helpers for built-in widgets.

use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::composable::{EventResult, ScrollContainerHost};
use crate::reactive::Binding;
use crate::runtime::CallbackHandle;
use crate::theme::Theme;

/// Returns the row range visible in a vertically scrolled viewport.
pub(crate) fn visible_row_range(
    row_count: usize,
    scroll_y: u16,
    viewport_height: u16,
) -> std::ops::Range<usize> {
    let start = usize::from(scroll_y).min(row_count);
    let end = start
        .saturating_add(usize::from(viewport_height))
        .min(row_count);
    start..end
}

/// Returns the standard widget style for disabled, focused, and normal states.
pub(crate) fn widget_style(theme: &Theme, enabled: bool, focused: bool) -> Style {
    if !enabled {
        theme.widget.disabled
    } else if focused {
        theme.widget.focused
    } else {
        theme.widget.normal
    }
}

/// Checks whether a terminal coordinate falls inside a non-empty rectangle.
pub(crate) fn contains(rect: Rect, x: u16, y: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

/// Converts absolute or already-local mouse coordinates to coordinates local to `area`.
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

/// Shared selection and vertical scroll behavior for row-based widgets.
#[derive(Clone, Debug, Default)]
pub(crate) struct SelectionScroll {
    last_selection: Option<usize>,
}

impl SelectionScroll {
    /// Clamps the selected index into the available row range and returns it.
    pub(crate) fn normalize_selection(
        &mut self,
        selection: &Binding<usize>,
        row_count: usize,
    ) -> Option<usize> {
        if row_count == 0 {
            return None;
        }
        let mut selected = selection.get();
        if selected >= row_count {
            selected = row_count.saturating_sub(1);
            selection.set(selected);
        }
        Some(selected)
    }

    /// Keeps the current selection visible when scrollbars recalculate layout.
    pub(crate) fn sync_selection_visible(
        &mut self,
        selection: &Binding<usize>,
        row_count: usize,
        host: &mut ScrollContainerHost,
    ) {
        let selected = self.normalize_selection(selection, row_count);
        if selected != self.last_selection {
            if let Some(idx) = selected {
                ensure_selection_visible(idx, host);
            }
            self.last_selection = selected;
        }
    }

    /// Handles row selection from mouse clicks and Up/Down keys.
    pub(crate) fn handle_event(
        &mut self,
        event: &Event,
        selection: &Binding<usize>,
        row_count: usize,
        host: &mut ScrollContainerHost,
        on_change: Option<&CallbackHandle>,
    ) -> EventResult {
        let Some(selected) = self.normalize_selection(selection, row_count) else {
            return EventResult::ignored();
        };

        match event {
            Event::Mouse(m) => {
                if m.kind != MouseEventKind::Down(MouseButton::Left) {
                    return EventResult::ignored();
                }
                let idx = host.scroll_offset().y as usize + m.row as usize;
                if idx < row_count {
                    return self.select(selection, idx, host, on_change);
                }
                EventResult::ignored()
            }
            Event::Key(KeyEvent { code, .. }) => match code {
                KeyCode::Up => {
                    let next = if selected == 0 {
                        row_count - 1
                    } else {
                        selected.saturating_sub(1)
                    };
                    self.select(selection, next, host, on_change)
                }
                KeyCode::Down => {
                    let next = (selected + 1) % row_count;
                    self.select(selection, next, host, on_change)
                }
                _ => EventResult::ignored(),
            },
            _ => EventResult::ignored(),
        }
    }

    fn select(
        &mut self,
        selection: &Binding<usize>,
        idx: usize,
        host: &mut ScrollContainerHost,
        on_change: Option<&CallbackHandle>,
    ) -> EventResult {
        selection.set(idx);
        ensure_selection_visible(idx, host);
        self.last_selection = Some(idx);
        if let Some(cb) = on_change {
            cb.emit();
        }
        EventResult::changed()
    }
}

fn ensure_selection_visible(selection: usize, host: &mut ScrollContainerHost) {
    let viewport_h = host.viewport_size().1;
    if viewport_h == 0 {
        return;
    }
    let scroll = host.scroll_offset();
    let sel = selection.min(u16::MAX as usize) as u16;
    let mut next_y = scroll.y;
    if sel < scroll.y {
        next_y = sel;
    } else if sel >= scroll.y.saturating_add(viewport_h) {
        next_y = sel.saturating_add(1).saturating_sub(viewport_h);
    }
    if next_y != scroll.y {
        host.set_scroll_offset(scroll.x, next_y);
    }
}

#[cfg(test)]
mod tests {
    use super::visible_row_range;

    #[test]
    fn visible_row_range_clamps_to_rows() {
        assert_eq!(visible_row_range(10, 0, 3), 0..3);
        assert_eq!(visible_row_range(10, 8, 5), 8..10);
        assert_eq!(visible_row_range(10, 10, 5), 10..10);
        assert_eq!(visible_row_range(10, 99, 5), 10..10);
    }

    #[test]
    fn visible_row_range_handles_empty_viewport_or_rows() {
        assert_eq!(visible_row_range(0, 0, 5), 0..0);
        assert_eq!(visible_row_range(10, 4, 0), 4..4);
    }
}
