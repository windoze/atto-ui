// Event routing and hit-testing for WindowManager.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent};
use ratatui::layout::Rect;

use crate::composable::scroll::{
    ScrollbarDrag, ScrollbarHit, scroll_offset_from_thumb_start, scrollbar_hit_test,
    scrollbar_layout_1d, should_show_scrollbar,
};
use crate::composable::{
    ComponentAction, ComponentContext, EventResult, MouseCoordinateSpace, ScrollbarHost, TabMode,
    TitleBarContext,
};
use crate::theme::Theme;

use super::{
    DragKind, DragState, HitRegion, HitTest, ResizeCorner, Window, WindowId, WindowKind,
    WindowManager, WindowManagerAction, WindowManagerInputMode, WindowState, chrome, placement,
};

impl WindowManager {
    pub fn handle_event(
        &mut self,
        event: &Event,
        bounds: Rect,
        mode: WindowManagerInputMode,
        theme: &Theme,
    ) -> WindowManagerAction {
        match event {
            Event::Mouse(m) => self.handle_mouse(m, bounds, theme),
            Event::Key(k) => self.handle_key(*k, bounds, mode),
            _ => WindowManagerAction::default(),
        }
    }

    pub fn dispatch_to_focused_view(
        &mut self,
        event: &Event,
        bounds: Rect,
        theme: &Theme,
    ) -> Option<(WindowId, EventResult)> {
        let id = self.focused()?;
        self.dispatch_to_window_view(id, event, bounds, theme)
    }

    pub fn dispatch_to_window_view(
        &mut self,
        id: WindowId,
        event: &Event,
        bounds: Rect,
        theme: &Theme,
    ) -> Option<(WindowId, EventResult)> {
        let idx = self.window_index_of(id)?;
        let is_focused = self.focused() == Some(id);
        let action = {
            let w = &mut self.windows[idx];
            let state = w.state.get();
            if state == WindowState::Minimized {
                return None;
            }

            let enforced_min_size = placement::window_enforced_min_size(w);
            // Ensure rect stays clamped before passing input.
            let rect = match state {
                WindowState::Maximized => bounds,
                _ => placement::normalize_rect(w.rect.get(), bounds, enforced_min_size),
            };
            w.rect.set(rect);
            if let Event::Mouse(m) = event {
                // Views render inside the inner rect; clicks on window chrome/borders should not
                // be delivered to the view layer.
                let inner = w.inner_rect();
                if !placement::contains(inner, m.column, m.row) {
                    return Some((id, EventResult::ignored()));
                }
            }
            let ctx = ComponentContext {
                theme,
                window_id: id,
                is_focused,
                scrollbar_host: if w.decorations.get().border.has_border() {
                    ScrollbarHost::Window
                } else {
                    ScrollbarHost::Component
                },
                tab_mode: TabMode::Cycle,
                mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            };
            w.view.handle_event(event, ctx)
        };
        Some((id, action))
    }

    pub fn window_at(&self, x: u16, y: u16) -> Option<WindowId> {
        let modal = self.active_modal_id();
        self.hit_test(x, y, modal).map(|hit| hit.window_id)
    }

    pub fn window_kind(&self, id: WindowId) -> Option<WindowKind> {
        self.window(id).map(|w| w.kind)
    }

    pub fn window(&self, id: WindowId) -> Option<&Window> {
        let idx = self.window_index_of(id)?;
        self.windows.get(idx)
    }

    pub fn window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        let idx = self.window_index_of(id)?;
        self.windows.get_mut(idx)
    }

    fn handle_mouse(&mut self, m: &MouseEvent, bounds: Rect, theme: &Theme) -> WindowManagerAction {
        use crossterm::event::MouseEventKind;

        let modal = self.active_modal_id();

        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.drag = None;
                self.mouse_capture = false;
                let Some(hit) = self.hit_test(m.column, m.row, modal) else {
                    return WindowManagerAction::default();
                };

                self.mouse_capture = !matches!(hit.region, HitRegion::Body);
                let mut action = WindowManagerAction {
                    consumed: self.mouse_capture,
                    close: None,
                };

                let window_id = hit.window_id;
                if modal.is_none()
                    && self
                        .window(window_id)
                        .is_some_and(|w| w.kind.is_focusable())
                {
                    self.focus(window_id);
                }

                match hit.region {
                    HitRegion::CloseButton => {
                        if let Some(w) = self.window_mut(window_id)
                            && w.closable.get()
                            && w.decorations.get().buttons.close
                        {
                            action.close = Some(window_id);
                        }
                    }
                    HitRegion::MaximizeButton => {
                        let can_maximize = self
                            .window(window_id)
                            .is_some_and(chrome::can_toggle_maximize);
                        if can_maximize {
                            self.toggle_maximize(window_id, bounds);
                        }
                    }
                    HitRegion::MinimizeButton => {
                        let minimized = match self.window_mut(window_id) {
                            Some(w) if chrome::can_minimize(w) => {
                                w.state.set(WindowState::Minimized);
                                true
                            }
                            _ => false,
                        };
                        if minimized {
                            self.focused = self.topmost_focusable_id();
                        }
                    }
                    HitRegion::TitleBar => {
                        let is_focused = self.focused() == Some(window_id);
                        if let Some(w) = self.window_mut(window_id) {
                            let deco = w.decorations.get();
                            let buttons = chrome::effective_titlebar_buttons(w, &deco);
                            let layout = chrome::titlebar_layout(w.rect.get(), &buttons);
                            let ctx = TitleBarContext {
                                theme,
                                window_id,
                                is_focused,
                                area: layout.text_area,
                            };
                            let res = w.view.handle_titlebar_event(&Event::Mouse(*m), ctx);
                            if res.action == ComponentAction::CloseWindow {
                                action.close = Some(window_id);
                                action.consumed = true;
                                return action;
                            }
                            if res.is_consumed() {
                                action.consumed = true;
                                return action;
                            }

                            if w.movable.get() && w.state.get() != WindowState::Maximized {
                                let rect = w.rect.get();
                                self.drag = Some(DragState {
                                    window_id,
                                    kind: DragKind::Move {
                                        offset_x: m.column.saturating_sub(rect.x),
                                        offset_y: m.row.saturating_sub(rect.y),
                                    },
                                });
                            }
                        }
                    }
                    HitRegion::ResizeHandle(corner) => {
                        if let Some(w) = self.window_mut(window_id)
                            && w.resizable.get()
                            && w.state.get() != WindowState::Maximized
                        {
                            let start_rect = placement::normalize_rect(
                                w.rect.get(),
                                bounds,
                                placement::window_enforced_min_size(w),
                            );
                            w.rect.set(start_rect);
                            self.drag = Some(DragState {
                                window_id,
                                kind: DragKind::Resize { start_rect, corner },
                            });
                        }
                    }
                    HitRegion::VScrollbar => {
                        let Some(w) = self.window_mut(window_id) else {
                            return action;
                        };
                        let inner = w.inner_rect();
                        if inner.height == 0 {
                            return action;
                        }

                        let cfg = w.view.scroll_config();
                        let (_content_w, content_h) = w.view.content_size();
                        let (_viewport_w, viewport_h) = w.view.viewport_size();
                        let (scroll_x, scroll_y) = w.view.scroll_offset();

                        // `hit_test` only returns VScrollbar when it should be visible, but
                        // recompute bar_len defensively.
                        let bar_len = inner.height;
                        let pos = m.row.saturating_sub(inner.y).min(bar_len.saturating_sub(1));
                        let layout = scrollbar_layout_1d(
                            bar_len, viewport_h, content_h, scroll_y, cfg.arrows,
                        );

                        match scrollbar_hit_test(layout, pos) {
                            ScrollbarHit::ArrowDec => {
                                w.view
                                    .set_scroll_offset(scroll_x, scroll_y.saturating_sub(1));
                            }
                            ScrollbarHit::ArrowInc => {
                                w.view
                                    .set_scroll_offset(scroll_x, scroll_y.saturating_add(1));
                            }
                            ScrollbarHit::TrackDec => {
                                w.view.set_scroll_offset(
                                    scroll_x,
                                    scroll_y.saturating_sub(viewport_h),
                                );
                            }
                            ScrollbarHit::TrackInc => {
                                w.view.set_scroll_offset(
                                    scroll_x,
                                    scroll_y.saturating_add(viewport_h),
                                );
                            }
                            ScrollbarHit::Thumb { grab_offset } => {
                                self.drag = Some(DragState {
                                    window_id,
                                    kind: DragKind::Scrollbar {
                                        drag: ScrollbarDrag::Vertical { grab_offset },
                                    },
                                });
                            }
                            ScrollbarHit::None => {}
                        }
                    }
                    HitRegion::HScrollbar => {
                        let Some(w) = self.window_mut(window_id) else {
                            return action;
                        };
                        let inner = w.inner_rect();
                        if inner.width == 0 {
                            return action;
                        }

                        let cfg = w.view.scroll_config();
                        let (content_w, _content_h) = w.view.content_size();
                        let (viewport_w, _viewport_h) = w.view.viewport_size();
                        let (scroll_x, scroll_y) = w.view.scroll_offset();

                        let bar_len = inner.width;
                        let pos = m
                            .column
                            .saturating_sub(inner.x)
                            .min(bar_len.saturating_sub(1));
                        let layout = scrollbar_layout_1d(
                            bar_len, viewport_w, content_w, scroll_x, cfg.arrows,
                        );

                        match scrollbar_hit_test(layout, pos) {
                            ScrollbarHit::ArrowDec => {
                                w.view
                                    .set_scroll_offset(scroll_x.saturating_sub(1), scroll_y);
                            }
                            ScrollbarHit::ArrowInc => {
                                w.view
                                    .set_scroll_offset(scroll_x.saturating_add(1), scroll_y);
                            }
                            ScrollbarHit::TrackDec => {
                                w.view.set_scroll_offset(
                                    scroll_x.saturating_sub(viewport_w),
                                    scroll_y,
                                );
                            }
                            ScrollbarHit::TrackInc => {
                                w.view.set_scroll_offset(
                                    scroll_x.saturating_add(viewport_w),
                                    scroll_y,
                                );
                            }
                            ScrollbarHit::Thumb { grab_offset } => {
                                self.drag = Some(DragState {
                                    window_id,
                                    kind: DragKind::Scrollbar {
                                        drag: ScrollbarDrag::Horizontal { grab_offset },
                                    },
                                });
                            }
                            ScrollbarHit::None => {}
                        }
                    }
                    HitRegion::Body => {}
                }

                action
            }
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
                let Some(drag) = self.drag else {
                    return WindowManagerAction::default();
                };
                let Some(w) = self.window_mut(drag.window_id) else {
                    self.drag = None;
                    return WindowManagerAction::default();
                };
                match drag.kind {
                    DragKind::Move { offset_x, offset_y } => {
                        if !w.movable.get() || w.state.get() == WindowState::Maximized {
                            return WindowManagerAction::default();
                        }
                        let new_x = m.column.saturating_sub(offset_x);
                        let new_y = m.row.saturating_sub(offset_y);
                        let mut rect = w.rect.get();
                        rect.x = new_x;
                        rect.y = new_y;
                        w.rect.set(placement::normalize_rect(
                            rect,
                            bounds,
                            placement::window_enforced_min_size(w),
                        ));
                    }
                    DragKind::Resize { start_rect, corner } => {
                        if !w.resizable.get() || w.state.get() == WindowState::Maximized {
                            return WindowManagerAction::default();
                        }
                        w.rect.set(placement::resize_rect_from_corner(
                            start_rect,
                            corner,
                            m.column,
                            m.row,
                            bounds,
                            placement::window_enforced_min_size(w),
                        ));
                    }
                    DragKind::Scrollbar { drag } => {
                        if !w.decorations.get().border.has_border() {
                            return WindowManagerAction::default();
                        }

                        let cfg = w.view.scroll_config();
                        let (content_w, content_h) = w.view.content_size();
                        let (viewport_w, viewport_h) = w.view.viewport_size();
                        let (scroll_x, scroll_y) = w.view.scroll_offset();

                        let inner = w.inner_rect();

                        match drag {
                            ScrollbarDrag::Vertical { grab_offset } => {
                                if inner.height == 0 {
                                    return WindowManagerAction::default();
                                }
                                let layout = scrollbar_layout_1d(
                                    inner.height,
                                    viewport_h,
                                    content_h,
                                    scroll_y,
                                    cfg.arrows,
                                );
                                if layout.track_len == 0 {
                                    return WindowManagerAction::default();
                                }

                                let pos = m
                                    .row
                                    .saturating_sub(inner.y)
                                    .min(inner.height.saturating_sub(1));
                                let pos_in_track = pos
                                    .saturating_sub(layout.track_start)
                                    .min(layout.track_len.saturating_sub(1));

                                let max_start = layout.track_len.saturating_sub(layout.thumb_len);
                                let new_thumb_start =
                                    pos_in_track.saturating_sub(grab_offset).min(max_start);
                                let new_off = scroll_offset_from_thumb_start(
                                    layout.track_len,
                                    viewport_h,
                                    content_h,
                                    new_thumb_start,
                                );
                                w.view.set_scroll_offset(scroll_x, new_off);
                            }
                            ScrollbarDrag::Horizontal { grab_offset } => {
                                if inner.width == 0 {
                                    return WindowManagerAction::default();
                                }
                                let layout = scrollbar_layout_1d(
                                    inner.width,
                                    viewport_w,
                                    content_w,
                                    scroll_x,
                                    cfg.arrows,
                                );
                                if layout.track_len == 0 {
                                    return WindowManagerAction::default();
                                }

                                let pos = m
                                    .column
                                    .saturating_sub(inner.x)
                                    .min(inner.width.saturating_sub(1));
                                let pos_in_track = pos
                                    .saturating_sub(layout.track_start)
                                    .min(layout.track_len.saturating_sub(1));

                                let max_start = layout.track_len.saturating_sub(layout.thumb_len);
                                let new_thumb_start =
                                    pos_in_track.saturating_sub(grab_offset).min(max_start);
                                let new_off = scroll_offset_from_thumb_start(
                                    layout.track_len,
                                    viewport_w,
                                    content_w,
                                    new_thumb_start,
                                );
                                w.view.set_scroll_offset(new_off, scroll_y);
                            }
                        }
                    }
                }
                WindowManagerAction {
                    consumed: true,
                    close: None,
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let consumed = self.drag.is_some() || self.mouse_capture;
                self.drag = None;
                self.mouse_capture = false;
                WindowManagerAction {
                    consumed,
                    close: None,
                }
            }
            _ => WindowManagerAction::default(),
        }
    }

    fn handle_key(
        &mut self,
        k: KeyEvent,
        bounds: Rect,
        mode: WindowManagerInputMode,
    ) -> WindowManagerAction {
        if k.code == KeyCode::F(6) && mode == WindowManagerInputMode::Normal {
            self.focus_next();
            return WindowManagerAction {
                consumed: true,
                close: None,
            };
        }

        if mode != WindowManagerInputMode::WindowManagement {
            return WindowManagerAction::default();
        }

        let mut action = WindowManagerAction {
            consumed: true,
            close: None,
        };

        match k.code {
            KeyCode::Left => {
                if k.modifiers.contains(KeyModifiers::SHIFT) {
                    self.resize_focused(-1, 0, bounds);
                } else {
                    self.move_focused(-1, 0, bounds);
                }
            }
            KeyCode::Right => {
                if k.modifiers.contains(KeyModifiers::SHIFT) {
                    self.resize_focused(1, 0, bounds);
                } else {
                    self.move_focused(1, 0, bounds);
                }
            }
            KeyCode::Up => {
                if k.modifiers.contains(KeyModifiers::SHIFT) {
                    self.resize_focused(0, -1, bounds);
                } else {
                    self.move_focused(0, -1, bounds);
                }
            }
            KeyCode::Down => {
                if k.modifiers.contains(KeyModifiers::SHIFT) {
                    self.resize_focused(0, 1, bounds);
                } else {
                    self.move_focused(0, 1, bounds);
                }
            }
            KeyCode::Tab => self.focus_next(),
            KeyCode::Char('c') => {
                if let Some(id) = self.focused() {
                    action.close = Some(id);
                }
            }
            KeyCode::Char('m') => self.minimize_focused(),
            KeyCode::Char('r') => self.restore_focused(),
            KeyCode::Char('x') => self.toggle_maximize_focused(bounds),
            _ => action.consumed = false,
        }

        action
    }

    fn hit_test(&self, x: u16, y: u16, modal: Option<WindowId>) -> Option<HitTest> {
        let iter: Box<dyn Iterator<Item = &Window>> = if let Some(modal_id) = modal {
            Box::new(self.windows.iter().filter(move |w| w.id == modal_id))
        } else {
            Box::new(self.windows.iter().rev())
        };

        for w in iter {
            let state = w.state.get();
            if state == WindowState::Minimized {
                continue;
            }
            let rect = w.rect.get();
            if !placement::contains(rect, x, y) {
                continue;
            }
            let decorations = w.decorations.get();

            if decorations.border.has_border()
                && w.resizable.get()
                && state != WindowState::Maximized
                && rect.width >= 2
                && rect.height >= 2
            {
                let left = rect.x;
                let top = rect.y;
                let right = rect.x.saturating_add(rect.width).saturating_sub(1);
                let bottom = rect.y.saturating_add(rect.height).saturating_sub(1);

                let corner = if x == left && y == top {
                    Some(ResizeCorner::TopLeft)
                } else if x == right && y == top {
                    Some(ResizeCorner::TopRight)
                } else if x == left && y == bottom {
                    Some(ResizeCorner::BottomLeft)
                } else if x == right && y == bottom {
                    Some(ResizeCorner::BottomRight)
                } else {
                    None
                };

                if let Some(corner) = corner {
                    return Some(HitTest {
                        window_id: w.id,
                        region: HitRegion::ResizeHandle(corner),
                    });
                }
            }

            if let Some(titlebar) = w.titlebar_rect()
                && y == titlebar.y
                && x >= titlebar.x
                && x < titlebar.x + titlebar.width
            {
                if let Some(btn) = chrome::hit_test_buttons(w, x, y) {
                    return Some(HitTest {
                        window_id: w.id,
                        region: btn,
                    });
                }
                return Some(HitTest {
                    window_id: w.id,
                    region: HitRegion::TitleBar,
                });
            }

            if decorations.border.has_border()
                && w.view.is_scrollable()
                && rect.width > 1
                && rect.height > 1
            {
                let cfg = w.view.scroll_config();
                let (content_w, content_h) = w.view.content_size();
                let (viewport_w, viewport_h) = w.view.viewport_size();

                let show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_h, viewport_h);
                let show_h = should_show_scrollbar(cfg.horizontal_scrollbar, content_w, viewport_w);

                let left = rect.x;
                let top = rect.y;
                let right = rect.x.saturating_add(rect.width).saturating_sub(1);
                let bottom = rect.y.saturating_add(rect.height).saturating_sub(1);

                // Scrollbars occupy the right/bottom border lines (excluding the corners).
                if show_v && x == right && y > top && y < bottom {
                    return Some(HitTest {
                        window_id: w.id,
                        region: HitRegion::VScrollbar,
                    });
                }
                if show_h && y == bottom && x > left && x < right {
                    return Some(HitTest {
                        window_id: w.id,
                        region: HitRegion::HScrollbar,
                    });
                }
            }

            return Some(HitTest {
                window_id: w.id,
                region: HitRegion::Body,
            });
        }
        None
    }
}
