// Rendering code for WindowManager.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders};

use crate::composable::{ComponentContext, ScrollbarHost, TabMode, TitleBarContext};
use crate::theme::Theme;

use super::{WindowBorderStyle, WindowManager, WindowState, chrome, placement};

impl WindowManager {
    pub fn draw(&mut self, frame: &mut Frame<'_>, bounds: Rect, theme: &Theme) {
        let focused = self.focused();
        let modal = self.active_modal_id();
        if modal.is_some() {
            // Dim the desktop behind the modal.
            fill_rect(frame.buffer_mut(), bounds, theme.desktop_dim, ' ');
        }

        for window in self.windows.iter_mut() {
            let state = window.state.get();
            if state == WindowState::Minimized {
                continue;
            }

            let enforced_min_size = placement::window_enforced_min_size(window);
            let rect = match state {
                WindowState::Maximized => bounds,
                _ => placement::normalize_rect(window.rect.get(), bounds, enforced_min_size),
            };
            window.rect.set(rect);

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
                    chrome::draw_titlebar_spans(frame.buffer_mut(), &layout, &content, title_style);
                } else {
                    let title = window.title.get();
                    chrome::draw_titlebar_text(frame.buffer_mut(), &layout, &title, title_style);
                }
                chrome::draw_titlebar_buttons(frame.buffer_mut(), &layout, title_style, theme);
            }

            let inner = window.inner_rect();
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
            };
            window.view.draw(frame, inner, ctx);

            if decorations.border.has_border() {
                chrome::draw_window_border_scrollbars(
                    frame.buffer_mut(),
                    rect,
                    inner,
                    window.view.as_ref(),
                    theme,
                );
            }
        }
    }
}

pub(super) fn draw_shadow(buf: &mut Buffer, rect: Rect, bounds: Rect, style: Style) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    let style = reset_style(style);
    let shadow_x = rect.x.saturating_add(rect.width);
    let shadow_y = rect.y.saturating_add(rect.height);

    // Vertical shadow.
    if shadow_x < bounds.x.saturating_add(bounds.width) {
        for y in rect.y.saturating_add(1)..rect.y.saturating_add(rect.height) {
            if y >= bounds.y.saturating_add(bounds.height) {
                break;
            }
            if shadow_x < bounds.x || y < bounds.y {
                continue;
            }
            if let Some(cell) = buf.cell_mut((shadow_x, y)) {
                cell.set_symbol(" ");
                cell.set_style(style);
            }
        }
    }

    // Horizontal shadow.
    if shadow_y < bounds.y.saturating_add(bounds.height) {
        for x in rect.x.saturating_add(1)..rect.x.saturating_add(rect.width) {
            if x >= bounds.x.saturating_add(bounds.width) {
                break;
            }
            if x < bounds.x || shadow_y < bounds.y {
                continue;
            }
            if let Some(cell) = buf.cell_mut((x, shadow_y)) {
                cell.set_symbol(" ");
                cell.set_style(style);
            }
        }
    }

    // Bottom-right corner.
    if shadow_x < bounds.x.saturating_add(bounds.width)
        && shadow_y < bounds.y.saturating_add(bounds.height)
        && shadow_x >= bounds.x
        && shadow_y >= bounds.y
        && let Some(cell) = buf.cell_mut((shadow_x, shadow_y))
    {
        cell.set_symbol(" ");
        cell.set_style(style);
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

fn reset_style(style: Style) -> Style {
    Style::reset().patch(style)
}
