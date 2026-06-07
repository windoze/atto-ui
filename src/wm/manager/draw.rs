// Rendering code for WindowManager.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::composable::{
    ComponentContext, DragContext, DragPayload, DropEffect, MouseCoordinateSpace, ScrollbarHost,
    TabMode, TitleBarContext,
};
use crate::drawing::draw_shadow;
use crate::theme::Theme;

use super::{
    GlobalDragState, WindowBorderStyle, WindowDock, WindowId, WindowManager, WindowState, chrome,
    docking, placement,
};

impl WindowManager {
    pub fn draw(&mut self, frame: &mut Frame<'_>, bounds: Rect, theme: &Theme) {
        let effective_bounds = self.apply_dock_layout(bounds);
        let focused = self.focused();
        let modal = self.active_modal_id();
        if modal.is_some() {
            // Dim the desktop behind the modal.
            fill_rect(frame.buffer_mut(), bounds, theme.desktop_dim, ' ');
        }
        let global_drag = self.global_drag.as_ref();

        for overlay_pass in [false, true] {
            for window in self.windows.iter_mut() {
                let is_auto_hide_overlay =
                    modal.is_none() && docking::window_is_visible_auto_hide_dock(window);
                if is_auto_hide_overlay != overlay_pass {
                    continue;
                }

                let state = window.state.get();
                if state == WindowState::Minimized {
                    continue;
                }

                let enforced_min_size = placement::window_enforced_min_size(window);
                let rect = if window.dock.get().is_some() {
                    window.rect.get()
                } else {
                    match state {
                        WindowState::Maximized => effective_bounds,
                        _ => placement::normalize_rect(
                            window.rect.get(),
                            effective_bounds,
                            enforced_min_size,
                        ),
                    }
                };
                window.rect.set(rect);
                let dock = window.dock.get();
                let is_auto_hidden = dock.as_ref().is_some_and(|dock| {
                    matches!(
                        dock.auto_hide,
                        super::DockAutoHide::Enabled { visible: false }
                    )
                });

                if modal.is_some() && Some(window.id) != modal {
                    // Block non-modal windows visually by dimming their chrome.
                }

                let decorations = window.decorations.get();
                if decorations.shadow {
                    draw_shadow(frame.buffer_mut(), rect, bounds, theme.window_shadow);
                }

                fill_rect(frame.buffer_mut(), rect, theme.window_bg, ' ');

                let is_focused = focused == Some(window.id);
                let border_style = theme.window_bg.patch(if is_focused {
                    theme.window_border_focused
                } else {
                    theme.window_border
                });
                let title_style = theme.window_bg.patch(if is_focused {
                    theme.window_title_focused
                } else {
                    theme.window_title
                });

                if decorations.border.has_border() {
                    let border_set = match decorations.border {
                        WindowBorderStyle::Normal => theme.border_set(is_focused),
                        WindowBorderStyle::Thin => theme.border_set(false),
                        WindowBorderStyle::Borderless => theme.border_set(false),
                    };
                    let block = Block::default()
                        .borders(Borders::ALL)
                        .border_style(border_style)
                        .border_set(border_set);
                    frame.render_widget(block, rect);
                    let buttons = chrome::effective_titlebar_buttons(window, &decorations);
                    let layout = chrome::titlebar_layout(rect, &buttons);
                    let titlebar_ctx = TitleBarContext {
                        theme,
                        window_id: window.id,
                        is_focused,
                        area: layout.text_area,
                    };
                    if let Some(content) = window.view.titlebar(titlebar_ctx) {
                        chrome::draw_titlebar_spans(
                            frame.buffer_mut(),
                            &layout,
                            &content,
                            title_style,
                        );
                    } else {
                        let title = window.title.get();
                        chrome::draw_titlebar_text(
                            frame.buffer_mut(),
                            &layout,
                            &title,
                            title_style,
                        );
                    }
                    chrome::draw_titlebar_buttons(frame.buffer_mut(), &layout, title_style, theme);
                }

                let inner = window.inner_rect();
                if !is_auto_hidden {
                    let drag = drag_context_for_window(global_drag, window.id);
                    let ctx = ComponentContext {
                        theme,
                        window_id: window.id,
                        is_focused,
                        scrollbar_host: if decorations.border.has_border() {
                            ScrollbarHost::Window
                        } else {
                            ScrollbarHost::Component
                        },
                        tab_mode: TabMode::Cycle,
                        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
                        drag,
                    };
                    window.view.draw(frame, inner, ctx);
                }

                if !is_auto_hidden && decorations.border.has_border() {
                    chrome::draw_window_border_scrollbars(
                        frame.buffer_mut(),
                        rect,
                        inner,
                        window.view.as_ref(),
                        theme,
                    );
                }

                if let Some(dock) = dock.as_ref()
                    && matches!(dock.auto_hide, super::DockAutoHide::Enabled { .. })
                {
                    draw_dock_auto_hide_handle(frame.buffer_mut(), rect, dock, theme);
                }
            }
        }

        draw_global_drag_overlay(frame.buffer_mut(), bounds, global_drag, theme);
    }
}

fn fill_rect(buf: &mut Buffer, rect: Rect, style: Style, ch: char) {
    let style = reset_style(style);
    let symbol = ch.to_string();
    for y in rect.y..rect.y.saturating_add(rect.height) {
        for x in rect.x..rect.x.saturating_add(rect.width) {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_style(style);
                cell.set_symbol(&symbol);
            }
        }
    }
}

fn draw_dock_auto_hide_handle(buf: &mut Buffer, rect: Rect, dock: &WindowDock, theme: &Theme) {
    let handle = docking::dock_handle_rect(rect, dock);
    if handle.width == 0 || handle.height == 0 {
        return;
    }

    let style = theme
        .named_style("dock-auto-hide-handle")
        .unwrap_or(theme.status_bar_key);
    fill_rect(buf, handle, style, ' ');

    let fallback = match dock.side {
        super::DockSide::Left => ">",
        super::DockSide::Right => "<",
        super::DockSide::Bottom => "^",
        super::DockSide::Top => "v",
    };
    let label = dock.handle_label.as_deref().unwrap_or(fallback);
    if matches!(dock.side, super::DockSide::Left | super::DockSide::Right) {
        draw_vertical_handle_label(buf, handle, label, style);
    } else {
        draw_overlay_text(buf, handle, handle.x, handle.y, label, style);
    }
}

fn draw_vertical_handle_label(buf: &mut Buffer, rect: Rect, label: &str, style: Style) {
    let style = reset_style(style);
    let bottom = rect.y.saturating_add(rect.height).saturating_sub(1);
    let mut row = rect.y;

    for grapheme in label.graphemes(true) {
        if row > bottom {
            break;
        }
        if UnicodeWidthStr::width(grapheme) != 1 {
            continue;
        }
        if let Some(cell) = buf.cell_mut((rect.x, row)) {
            cell.set_symbol(grapheme);
            cell.set_style(style);
        }
        row = row.saturating_add(1);
    }
}

fn draw_global_drag_overlay(
    buf: &mut Buffer,
    bounds: Rect,
    state: Option<&GlobalDragState>,
    theme: &Theme,
) {
    let Some(state) = state.filter(|state| state.active) else {
        return;
    };

    if let Some(feedback) = &state.feedback
        && let Some(rect) = feedback.rect.and_then(|rect| clipped_rect(rect, bounds))
    {
        let style_name = if feedback.effect == DropEffect::None {
            "drop-target-reject"
        } else {
            "drop-target-active"
        };
        let style = theme
            .named_style(style_name)
            .unwrap_or(theme.selection)
            .patch(
                theme
                    .named_style("drop-insertion-marker")
                    .unwrap_or_default(),
            );
        apply_overlay_style(buf, rect, style);
    }

    let label = state
        .source
        .ghost
        .clone()
        .unwrap_or_else(|| drag_payload_label(&state.source.payload));
    draw_overlay_text(
        buf,
        bounds,
        state.last_x,
        state.last_y,
        &label,
        theme
            .named_style("drag-ghost")
            .unwrap_or(theme.widget.accent),
    );
}

fn apply_overlay_style(buf: &mut Buffer, rect: Rect, style: Style) {
    let style = reset_style(style);
    for y in rect.y..rect.y.saturating_add(rect.height) {
        for x in rect.x..rect.x.saturating_add(rect.width) {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_style(style);
            }
        }
    }
}

fn draw_overlay_text(buf: &mut Buffer, bounds: Rect, x: u16, y: u16, text: &str, style: Style) {
    if bounds.width == 0 || bounds.height == 0 || !contains(bounds, x, y) {
        return;
    }

    let right = bounds.x.saturating_add(bounds.width).saturating_sub(1);
    let mut cursor = x;
    let style = reset_style(style);
    for grapheme in text.graphemes(true) {
        let width = (UnicodeWidthStr::width(grapheme) as u16).max(1);
        let end = cursor.saturating_add(width).saturating_sub(1);
        if cursor > right || end > right {
            break;
        }
        let Some(cell) = buf.cell_mut((cursor, y)) else {
            break;
        };
        cell.set_symbol(grapheme);
        cell.set_style(style);
        for dx in 1..width {
            if let Some(trailing) = buf.cell_mut((cursor.saturating_add(dx), y)) {
                trailing.reset();
                trailing.set_style(style);
            }
        }
        cursor = cursor.saturating_add(width);
    }
}

fn drag_payload_label(payload: &DragPayload) -> String {
    match payload {
        DragPayload::Text(text) => text.clone(),
        DragPayload::FilePath(path) => path.display().to_string(),
        DragPayload::ComponentId(_) => "component".to_string(),
        DragPayload::WindowId(id) => format!("window {}", id.raw()),
        DragPayload::Custom { ty, .. } => format!("custom {}", ty.0),
    }
}

fn drag_context_from_state(state: &GlobalDragState) -> DragContext<'_> {
    DragContext {
        payload: &state.source.payload,
        operation: state.source.operation,
        source_window: state.source_window,
    }
}

fn drag_context_for_window<'a>(
    state: Option<&'a GlobalDragState>,
    window_id: WindowId,
) -> Option<DragContext<'a>> {
    let state = state?;
    if !state.active {
        return None;
    }
    if state.source_window == window_id || state.target_window == Some(window_id) {
        Some(drag_context_from_state(state))
    } else {
        None
    }
}

fn clipped_rect(rect: Rect, bounds: Rect) -> Option<Rect> {
    let x0 = rect.x.max(bounds.x);
    let y0 = rect.y.max(bounds.y);
    let x1 = rect
        .x
        .saturating_add(rect.width)
        .min(bounds.x.saturating_add(bounds.width));
    let y1 = rect
        .y
        .saturating_add(rect.height)
        .min(bounds.y.saturating_add(bounds.height));
    (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
}

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn reset_style(style: Style) -> Style {
    Style::reset().patch(style)
}
