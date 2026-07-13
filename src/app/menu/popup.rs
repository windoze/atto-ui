//! Standalone popup / context menu.
//!
//! Conceptually a dropdown menu that is *not* anchored under a menu-bar title:
//! it reuses the same column rendering ([`draw_dropdown_column`]), sizing
//! ([`dropdown_size`]), and stack-based navigation ([`super::nav`]) as the
//! menu bar, but is anchored at an arbitrary screen point (e.g. the mouse
//! position on right-click).
//!
//! It is hosted in a borderless [`WindowKind::Modal`] window so it reliably
//! receives both keyboard (focus is locked to the modal) and mouse (hit-testing
//! is confined to it), and closes on Esc / activation / outside-click. The modal
//! opts out of the desktop backdrop dim so the surrounding UI stays legible.

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::composable::{
    Component, ComponentContext, DragAndDrop, DynamicTree, EventHandling, EventResult, FocusNav,
    Layout, Scrollable,
};
use crate::reactive::Binding;
use crate::wm::{Window, WindowBorderStyle, WindowButtons, WindowDecorations, WindowKind};

use super::draw::draw_dropdown_column;
use super::layout::{clamp_and_flip_anchor, contains, dropdown_size};
use super::model::MenuItem;
use super::nav::{
    item_at, item_mnemonic_or_first, items_at, levels_from_origin, move_in_stack, open_submenu,
};

/// A keyboard- and mouse-navigable popup menu rendered as a modal window view.
pub struct PopupMenu {
    items: Vec<MenuItem>,
    /// Selected row at each open submenu depth. `stack[0]` is the root level.
    stack: Vec<usize>,
    /// Shared with the hosting window's rect so submenu cascades can expand the
    /// modal's hit-test/draw bounds. Refreshed lazily; may be updated on the
    /// next depth change.
    rect: Binding<Rect>,
    /// Full screen bounds, refreshed each draw from `frame.area()`.
    screen: Rect,
}

impl PopupMenu {
    fn new(items: Vec<MenuItem>, rect: Binding<Rect>) -> Self {
        Self {
            items,
            stack: vec![0],
            rect,
            screen: Rect::default(),
        }
    }

    fn origin(&self) -> (u16, u16) {
        let rect = self.rect.get();
        (rect.x, rect.y)
    }

    fn levels(&self) -> Vec<(Rect, &[MenuItem])> {
        levels_from_origin(&self.items, &self.stack, self.origin())
    }

    /// Activates the currently selected item: invokes its callback and requests
    /// window close. Returns the resulting `EventResult`.
    fn activate_selected(&mut self) -> EventResult {
        if open_submenu(&self.items, &mut self.stack) {
            return EventResult::consumed();
        }
        let Some(item) = item_at(&self.items, &self.stack) else {
            return EventResult::consumed();
        };
        if !item.enabled.get() {
            return EventResult::consumed();
        }
        if let Some(cb) = &item.on_activate {
            cb();
            return EventResult::close_window();
        }
        EventResult::consumed()
    }

    fn handle_char(&mut self, c: char) -> EventResult {
        let Some(items) = items_at(&self.items, &self.stack) else {
            return EventResult::consumed();
        };
        let hit = items.iter().enumerate().find(|(_, item)| {
            item.enabled.get()
                && item_mnemonic_or_first(item).is_some_and(|m| char_eq_ignore_case(m, c))
        });
        let Some((idx, _)) = hit else {
            return EventResult::consumed();
        };
        let depth = self.stack.len().saturating_sub(1);
        if depth < self.stack.len() {
            self.stack[depth] = idx;
        }
        self.activate_selected()
    }

    /// Hit-tests `(col, row)` against the visible levels. On a row, updates the
    /// selection stack and returns the row's activation result; inside the menu
    /// but on no row, consumes; fully outside, returns `None`.
    fn hit(&mut self, col: u16, row: u16) -> Option<EventResult> {
        let levels = self.levels();
        for (depth, (rect, items)) in levels.iter().enumerate() {
            if !contains(*rect, col, row) {
                continue;
            }
            let inner = Rect {
                x: rect.x.saturating_add(1),
                y: rect.y.saturating_add(1),
                width: rect.width.saturating_sub(2),
                height: rect.height.saturating_sub(2),
            };
            if !contains(inner, col, row) {
                return Some(EventResult::consumed());
            }
            let idx = (row - inner.y) as usize;
            if idx >= items.len() {
                return Some(EventResult::consumed());
            }
            self.stack.truncate(depth + 1);
            if self.stack.len() < depth + 1 {
                self.stack.resize(depth + 1, 0);
            }
            self.stack[depth] = idx;
            return Some(self.activate_selected());
        }
        None
    }
}

impl Component for PopupMenu {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.screen = frame.area();
        // The window's inner rect equals the borderless window rect; draw the
        // dropdown column(s) starting there.
        let levels = levels_from_origin(&self.items, &self.stack, (area.x, area.y));
        for (depth, (rect, items)) in levels.into_iter().enumerate() {
            let selected = self.stack.get(depth).copied().unwrap_or(0);
            draw_dropdown_column(frame, rect, self.screen, items, selected, ctx.theme);
        }
    }
}

impl Layout for PopupMenu {}
impl Scrollable for PopupMenu {}
impl FocusNav for PopupMenu {}
impl DragAndDrop for PopupMenu {}
impl DynamicTree for PopupMenu {}

impl EventHandling for PopupMenu {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        match event {
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) => {
                if matches!(kind, KeyEventKind::Release) {
                    return EventResult::ignored();
                }
                match code {
                    KeyCode::Esc => EventResult::close_window(),
                    KeyCode::Up => {
                        move_in_stack(&self.items, &mut self.stack, -1);
                        EventResult::consumed()
                    }
                    KeyCode::Down => {
                        move_in_stack(&self.items, &mut self.stack, 1);
                        EventResult::consumed()
                    }
                    KeyCode::Left => {
                        if self.stack.len() > 1 {
                            self.stack.pop();
                        }
                        EventResult::consumed()
                    }
                    KeyCode::Right => {
                        open_submenu(&self.items, &mut self.stack);
                        EventResult::consumed()
                    }
                    KeyCode::Enter => self.activate_selected(),
                    KeyCode::Char(c)
                        if !modifiers.contains(KeyModifiers::CONTROL)
                            && !modifiers.contains(KeyModifiers::ALT) =>
                    {
                        self.handle_char(*c)
                    }
                    _ => EventResult::consumed(),
                }
            }
            Event::Mouse(MouseEvent {
                kind, column, row, ..
            }) => match kind {
                MouseEventKind::Down(MouseButton::Left) => self
                    .hit(*column, *row)
                    .unwrap_or_else(EventResult::consumed),
                MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
                    // Hover-highlight: update selection without activating.
                    let levels = self.levels();
                    for (depth, (rect, items)) in levels.iter().enumerate() {
                        let inner = Rect {
                            x: rect.x.saturating_add(1),
                            y: rect.y.saturating_add(1),
                            width: rect.width.saturating_sub(2),
                            height: rect.height.saturating_sub(2),
                        };
                        if contains(inner, *column, *row) {
                            let idx = (*row - inner.y) as usize;
                            if idx < items.len() {
                                self.stack.truncate(depth + 1);
                                if self.stack.len() < depth + 1 {
                                    self.stack.resize(depth + 1, 0);
                                }
                                self.stack[depth] = idx;
                            }
                            break;
                        }
                    }
                    EventResult::consumed()
                }
                _ => EventResult::consumed(),
            },
            _ => EventResult::consumed(),
        }
    }
}

fn char_eq_ignore_case(a: char, b: char) -> bool {
    a == b || a.to_lowercase().to_string() == b.to_lowercase().to_string()
}

/// Builds a modal popup-menu window: a borderless, non-dimming, non-movable
/// modal hosting a [`PopupMenu`], sized to `items` and placed at `anchor`
/// (clamped/flipped within `screen`).
pub fn popup_menu_window(
    items: Vec<MenuItem>,
    anchor: (u16, u16),
    screen: Rect,
    title: impl Into<Binding<String>>,
) -> Window {
    let (w, h) = dropdown_size(&items);
    let rect = clamp_and_flip_anchor(screen, anchor, w, h);
    let rect_binding: Binding<Rect> = Binding::new(rect);

    let view = PopupMenu::new(items, rect_binding.clone());
    let decorations = WindowDecorations {
        border: WindowBorderStyle::Borderless,
        shadow: true,
        buttons: WindowButtons {
            minimize: false,
            maximize: false,
            close: false,
        },
        backdrop_dim: false,
    };

    let window = Window::new(WindowKind::Modal, title, rect_binding, Box::new(view))
        .with_decorations(decorations)
        .with_min_size(w, h);
    window.movable.set(false);
    window.resizable.set(false);
    window
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crossterm::event::{KeyEvent, KeyEventKind};

    use crate::composable::{
        ComponentAction, EventOutcome, MouseCoordinateSpace, ScrollbarHost, TabMode,
    };
    use crate::theme::Theme;
    use crate::wm::WindowId;

    use super::*;

    fn ctx(theme: &Theme) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        }
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new_with_kind(
            code,
            KeyModifiers::NONE,
            KeyEventKind::Press,
        ))
    }

    /// Three items, each recording its index into a shared counter when invoked.
    fn menu(hits: &Arc<AtomicUsize>) -> PopupMenu {
        let make = |tag: usize| {
            let hits = Arc::clone(hits);
            move || {
                hits.store(tag + 1, Ordering::SeqCst);
            }
        };
        let items = vec![
            MenuItem::action("Rerun", make(0)),
            MenuItem::action("Copy command", make(1)),
            MenuItem::action("Copy output", make(2)),
        ];
        PopupMenu::new(items, Binding::new(Rect::new(0, 0, 18, 5)))
    }

    #[test]
    fn down_twice_then_enter_activates_third_item() {
        let theme = Theme::dark();
        let hits = Arc::new(AtomicUsize::new(0));
        let mut popup = menu(&hits);

        popup.handle_event(&key(KeyCode::Down), ctx(&theme));
        popup.handle_event(&key(KeyCode::Down), ctx(&theme));
        let res = popup.handle_event(&key(KeyCode::Enter), ctx(&theme));

        assert_eq!(res.action, ComponentAction::CloseWindow);
        assert_eq!(hits.load(Ordering::SeqCst), 3, "third item should activate");
    }

    #[test]
    fn mnemonic_char_activates_matching_item() {
        let theme = Theme::dark();
        let hits = Arc::new(AtomicUsize::new(0));
        let mut popup = menu(&hits);

        // 'r' matches "Rerun" (first-char fallback).
        let res = popup.handle_event(&key(KeyCode::Char('r')), ctx(&theme));

        assert_eq!(res.action, ComponentAction::CloseWindow);
        assert_eq!(hits.load(Ordering::SeqCst), 1, "Rerun should activate");
    }

    #[test]
    fn esc_requests_window_close_without_activating() {
        let theme = Theme::dark();
        let hits = Arc::new(AtomicUsize::new(0));
        let mut popup = menu(&hits);

        let res = popup.handle_event(&key(KeyCode::Esc), ctx(&theme));

        assert_eq!(res.action, ComponentAction::CloseWindow);
        assert_eq!(res.outcome, EventOutcome::Consumed);
        assert_eq!(
            hits.load(Ordering::SeqCst),
            0,
            "Esc must not activate any item"
        );
    }

    #[test]
    fn mouse_click_on_row_activates_it() {
        let theme = Theme::dark();
        let hits = Arc::new(AtomicUsize::new(0));
        let mut popup = menu(&hits);

        // Row 1 ("Copy command") is at inner y=1 within a bordered 18x5 rect at (0,0).
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 2,
            modifiers: KeyModifiers::NONE,
        });
        let res = popup.handle_event(&click, ctx(&theme));

        assert_eq!(res.action, ComponentAction::CloseWindow);
        assert_eq!(
            hits.load(Ordering::SeqCst),
            2,
            "Copy command (row 1) should activate"
        );
    }
}
