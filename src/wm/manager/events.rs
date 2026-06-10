// Event routing and hit-testing for WindowManager.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent};
use ratatui::layout::Rect;

use crate::composable::scroll::{
    ScrollbarDrag, ScrollbarHit, scroll_offset_from_thumb_start, scrollbar_hit_test,
    scrollbar_layout_1d, should_show_scrollbar,
};
use crate::composable::{
    Capture, ComponentAction, ComponentContext, ComponentId, DragContext, DragOffer, DragSource,
    DropEffect, DropFeedback, EventResult, MouseCoordinateSpace, ScrollbarHost, TabMode,
    TitleBarContext,
};
use crate::theme::Theme;

use super::{
    DragKind, DragState, GlobalDragState, HitRegion, HitTest, ResizeCorner, Window, WindowId,
    WindowKind, WindowManager, WindowManagerAction, WindowManagerInputMode, WindowState, chrome,
    docking, placement,
};

impl WindowManager {
    pub fn handle_event(
        &mut self,
        event: &Event,
        bounds: Rect,
        mode: WindowManagerInputMode,
        theme: &Theme,
    ) -> WindowManagerAction {
        self.apply_dock_layout(bounds);
        match event {
            Event::Mouse(m) => self.handle_mouse(m, bounds, theme),
            Event::Key(k) => self.handle_key(*k, bounds, mode, theme),
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
        let effective_bounds = self.apply_dock_layout(bounds);
        let idx = self.window_index_of(id)?;
        let is_focused = self.focused() == Some(id);
        let drag = drag_context_for_window(self.global_drag.as_ref(), id);
        // While this window holds pointer capture, deliver mouse events to its view
        // even when the pointer is outside the inner rect (so a pressed button keeps
        // tracking move/up after the pointer leaves it).
        let capture_active = self.pointer_capture == Some(id);
        let action = {
            let w = &mut self.windows[idx];
            let state = w.state.get();
            if state == WindowState::Minimized {
                return None;
            }

            let enforced_min_size = placement::window_enforced_min_size(w);
            // Ensure rect stays clamped before passing input.
            let rect = if w.dock.get().is_some() {
                w.rect.get()
            } else {
                match state {
                    WindowState::Maximized => effective_bounds,
                    _ => {
                        placement::normalize_rect(w.rect.get(), effective_bounds, enforced_min_size)
                    }
                }
            };
            w.rect.set(rect);
            if let Event::Mouse(m) = event {
                // Views render inside the inner rect; clicks on window chrome/borders should not
                // be delivered to the view layer.
                let inner = w.inner_rect();
                if !capture_active && !placement::contains(inner, m.column, m.row) {
                    return Some((id, EventResult::ignored()));
                }
            }
            let decorations = w.decorations.get();
            let ctx = window_component_context(
                theme,
                id,
                is_focused,
                decorations.border.has_border(),
                drag,
            );
            w.view.handle_event(event, ctx)
        };
        // The view (or a descendant) may request/release pointer capture; track it
        // at the window level so subsequent mouse events route back here.
        match action.capture {
            Capture::Request => self.pointer_capture = Some(id),
            Capture::Release => self.pointer_capture = None,
            Capture::None => {}
        }
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
                self.cancel_global_drag(bounds, theme);
                self.drag = None;
                self.mouse_capture = false;
                let Some(hit) = self.hit_test(m.column, m.row, modal) else {
                    if self.hide_auto_hide_docks_except(None) {
                        self.apply_dock_layout(bounds);
                    }
                    return WindowManagerAction::default();
                };

                self.mouse_capture = !matches!(hit.region, HitRegion::Body);
                let mut action = WindowManagerAction {
                    consumed: self.mouse_capture,
                    close: None,
                };

                let window_id = hit.window_id;
                let auto_hide_keep = self
                    .window(window_id)
                    .is_some_and(docking::window_is_auto_hide_dock)
                    .then_some(window_id);
                if self.hide_auto_hide_docks_except(auto_hide_keep) {
                    self.apply_dock_layout(bounds);
                }

                if modal.is_none()
                    && self
                        .window(window_id)
                        .is_some_and(|w| w.kind.is_focusable())
                {
                    self.focus(window_id);
                }

                match hit.region {
                    HitRegion::DockAutoHideHandle => {
                        let mut changed = false;
                        if let Some(w) = self.window_mut(window_id)
                            && let Some(mut dock) = w.dock.get()
                            && matches!(dock.auto_hide, super::DockAutoHide::Enabled { .. })
                        {
                            dock.auto_hide = super::DockAutoHide::Enabled { visible: true };
                            w.dock.set(Some(dock));
                            changed = true;
                        }
                        if changed {
                            self.apply_dock_layout(bounds);
                        }
                        action.consumed = true;
                    }
                    HitRegion::DockResizeEdge(side) => {
                        if let Some(w) = self.window_mut(window_id)
                            && let Some(dock) = w.dock.get()
                            && dock.side == side
                            && w.resizable.get()
                            && !matches!(
                                dock.auto_hide,
                                super::DockAutoHide::Enabled { visible: false }
                            )
                        {
                            self.drag = Some(DragState {
                                window_id,
                                kind: DragKind::DockResize {
                                    start_size: dock.size,
                                    side,
                                },
                            });
                        }
                    }
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

                            if w.dock.get().is_none()
                                && w.movable.get()
                                && w.state.get() != WindowState::Maximized
                            {
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
                        let effective_bounds = self.effective_work_area(bounds);
                        if let Some(w) = self.window_mut(window_id)
                            && w.dock.get().is_none()
                            && w.resizable.get()
                            && w.state.get() != WindowState::Maximized
                        {
                            let start_rect = placement::normalize_rect(
                                w.rect.get(),
                                effective_bounds,
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
                    HitRegion::Body => {
                        if let Some((source_component, source)) =
                            self.drag_source_for_window(window_id, m.column, m.row, bounds, theme)
                        {
                            self.global_drag = Some(GlobalDragState {
                                source_window: window_id,
                                source_component,
                                start_x: m.column,
                                start_y: m.row,
                                last_x: m.column,
                                last_y: m.row,
                                source,
                                active: false,
                                feedback: None,
                                target_window: None,
                            });
                        }
                    }
                }

                action
            }
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
                if self.global_drag.is_some() {
                    self.update_global_drag(m.column, m.row, bounds, theme);
                    return WindowManagerAction {
                        consumed: true,
                        close: None,
                    };
                }

                let Some(drag) = self.drag else {
                    return WindowManagerAction::default();
                };
                let effective_bounds = self.effective_work_area(bounds);
                let dock_area = match drag.kind {
                    DragKind::DockResize { .. } => {
                        Some(self.dock_area_for_window(drag.window_id, bounds))
                    }
                    _ => None,
                };
                let Some(w) = self.window_mut(drag.window_id) else {
                    self.drag = None;
                    return WindowManagerAction::default();
                };
                let mut dock_layout_changed = false;
                match drag.kind {
                    DragKind::Move { offset_x, offset_y } => {
                        if w.dock.get().is_some()
                            || !w.movable.get()
                            || w.state.get() == WindowState::Maximized
                        {
                            return WindowManagerAction::default();
                        }
                        let new_x = m.column.saturating_sub(offset_x);
                        let new_y = m.row.saturating_sub(offset_y);
                        let mut rect = w.rect.get();
                        rect.x = new_x;
                        rect.y = new_y;
                        w.rect.set(placement::normalize_rect(
                            rect,
                            effective_bounds,
                            placement::window_enforced_min_size(w),
                        ));
                    }
                    DragKind::Resize { start_rect, corner } => {
                        if w.dock.get().is_some()
                            || !w.resizable.get()
                            || w.state.get() == WindowState::Maximized
                        {
                            return WindowManagerAction::default();
                        }
                        w.rect.set(placement::resize_rect_from_corner(
                            start_rect,
                            corner,
                            m.column,
                            m.row,
                            effective_bounds,
                            placement::window_enforced_min_size(w),
                        ));
                    }
                    DragKind::DockResize { start_size, side } => {
                        let Some(area) = dock_area else {
                            return WindowManagerAction::default();
                        };
                        if let Some(mut dock) = w.dock.get() {
                            if dock.side != side
                                || !w.resizable.get()
                                || matches!(
                                    dock.auto_hide,
                                    super::DockAutoHide::Enabled { visible: false }
                                )
                            {
                                return WindowManagerAction::default();
                            }
                            let raw_size = docking::dock_size_from_pointer(
                                area, side, m.column, m.row, start_size,
                            );
                            dock.size = docking::clamp_dock_size(&dock, area, raw_size);
                            w.dock.set(Some(dock));
                            dock_layout_changed = true;
                        }
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
                if dock_layout_changed {
                    self.apply_dock_layout(bounds);
                }
                WindowManagerAction {
                    consumed: true,
                    close: None,
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.global_drag.is_some() {
                    return self.finish_global_drag(m.column, m.row, bounds, theme);
                }

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

    fn drag_source_for_window(
        &mut self,
        window_id: WindowId,
        x: u16,
        y: u16,
        bounds: Rect,
        theme: &Theme,
    ) -> Option<(Option<ComponentId>, DragSource)> {
        let effective_bounds = self.effective_work_area(bounds);
        let idx = self.window_index_of(window_id)?;
        let is_focused = self.focused() == Some(window_id);
        let w = &mut self.windows[idx];
        if w.state.get() == WindowState::Minimized {
            return None;
        }

        let enforced_min_size = placement::window_enforced_min_size(w);
        let rect = if w.dock.get().is_some() {
            w.rect.get()
        } else {
            match w.state.get() {
                WindowState::Maximized => effective_bounds,
                _ => placement::normalize_rect(w.rect.get(), effective_bounds, enforced_min_size),
            }
        };
        w.rect.set(rect);

        if !placement::contains(w.inner_rect(), x, y) {
            return None;
        }

        let decorations = w.decorations.get();
        let ctx = window_component_context(
            theme,
            window_id,
            is_focused,
            decorations.border.has_border(),
            None,
        );
        let source_component = w.view.focused_child();
        let source = w.view.drag_source_at(x, y, ctx)?;
        Some((source_component, source))
    }

    fn update_global_drag(&mut self, x: u16, y: u16, bounds: Rect, theme: &Theme) {
        let Some(mut state) = self.global_drag.take() else {
            return;
        };

        state.last_x = x;
        state.last_y = y;
        if !state.active && drag_distance_reached_threshold(&state, x, y) {
            state.active = true;
        }

        if state.active {
            let mut target_window = None;
            let mut feedback = None;
            if let Some(id) = self.window_at(x, y)
                && let Some(next_feedback) = self.drag_over_window(&state, id, x, y, bounds, theme)
            {
                target_window = Some(id);
                feedback = Some(next_feedback);
            }
            state.target_window = target_window;
            state.feedback = feedback;
        } else {
            state.target_window = None;
            state.feedback = None;
        }

        self.global_drag = Some(state);
    }

    fn finish_global_drag(
        &mut self,
        x: u16,
        y: u16,
        bounds: Rect,
        theme: &Theme,
    ) -> WindowManagerAction {
        let Some(mut state) = self.global_drag.take() else {
            return WindowManagerAction::default();
        };

        state.last_x = x;
        state.last_y = y;

        if !state.active {
            return WindowManagerAction::default();
        }

        let accepted = state
            .feedback
            .as_ref()
            .is_some_and(|feedback| feedback.effect != DropEffect::None);
        let mut close = None;
        if accepted {
            if let Some(target_id) = state.target_window {
                if let Some(res) = self.drop_on_window(&state, target_id, x, y, bounds, theme) {
                    if res.action == ComponentAction::CloseWindow {
                        close = Some(target_id);
                    }
                } else {
                    self.cancel_source_drag(&state, bounds, theme);
                }
            } else {
                self.cancel_source_drag(&state, bounds, theme);
            }
        } else {
            self.cancel_source_drag(&state, bounds, theme);
        }

        WindowManagerAction {
            consumed: true,
            close,
        }
    }

    fn cancel_global_drag(&mut self, bounds: Rect, theme: &Theme) -> bool {
        let Some(state) = self.global_drag.take() else {
            return false;
        };
        self.cancel_source_drag(&state, bounds, theme);
        true
    }

    fn drag_over_window(
        &mut self,
        state: &GlobalDragState,
        window_id: WindowId,
        x: u16,
        y: u16,
        bounds: Rect,
        theme: &Theme,
    ) -> Option<DropFeedback> {
        let effective_bounds = self.effective_work_area(bounds);
        let idx = self.window_index_of(window_id)?;
        let is_focused = self.focused() == Some(window_id);
        let w = &mut self.windows[idx];
        if w.state.get() == WindowState::Minimized {
            return None;
        }

        let enforced_min_size = placement::window_enforced_min_size(w);
        let rect = if w.dock.get().is_some() {
            w.rect.get()
        } else {
            match w.state.get() {
                WindowState::Maximized => effective_bounds,
                _ => placement::normalize_rect(w.rect.get(), effective_bounds, enforced_min_size),
            }
        };
        w.rect.set(rect);

        if !placement::contains(w.inner_rect(), x, y) {
            return None;
        }

        let offer = DragOffer {
            payload: &state.source.payload,
            operation: state.source.operation,
            screen_x: x,
            screen_y: y,
        };
        let decorations = w.decorations.get();
        let ctx = window_component_context(
            theme,
            window_id,
            is_focused,
            decorations.border.has_border(),
            Some(drag_context_from_state(state)),
        );
        Some(w.view.drag_over(offer, ctx))
    }

    fn drop_on_window(
        &mut self,
        state: &GlobalDragState,
        window_id: WindowId,
        x: u16,
        y: u16,
        bounds: Rect,
        theme: &Theme,
    ) -> Option<EventResult> {
        let effective_bounds = self.effective_work_area(bounds);
        let idx = self.window_index_of(window_id)?;
        let is_focused = self.focused() == Some(window_id);
        let w = &mut self.windows[idx];
        if w.state.get() == WindowState::Minimized {
            return None;
        }

        let enforced_min_size = placement::window_enforced_min_size(w);
        let rect = if w.dock.get().is_some() {
            w.rect.get()
        } else {
            match w.state.get() {
                WindowState::Maximized => effective_bounds,
                _ => placement::normalize_rect(w.rect.get(), effective_bounds, enforced_min_size),
            }
        };
        w.rect.set(rect);

        if !placement::contains(w.inner_rect(), x, y) {
            return None;
        }

        let offer = DragOffer {
            payload: &state.source.payload,
            operation: state.source.operation,
            screen_x: x,
            screen_y: y,
        };
        let decorations = w.decorations.get();
        let ctx = window_component_context(
            theme,
            window_id,
            is_focused,
            decorations.border.has_border(),
            Some(drag_context_from_state(state)),
        );
        Some(crate::composable::DragAndDrop::drop(
            w.view.as_mut(),
            offer,
            ctx,
        ))
    }

    fn cancel_source_drag(&mut self, state: &GlobalDragState, bounds: Rect, theme: &Theme) {
        let effective_bounds = self.effective_work_area(bounds);
        let Some(idx) = self.window_index_of(state.source_window) else {
            return;
        };
        let is_focused = self.focused() == Some(state.source_window);
        let w = &mut self.windows[idx];
        if w.state.get() == WindowState::Minimized {
            return;
        }

        let enforced_min_size = placement::window_enforced_min_size(w);
        let rect = if w.dock.get().is_some() {
            w.rect.get()
        } else {
            match w.state.get() {
                WindowState::Maximized => effective_bounds,
                _ => placement::normalize_rect(w.rect.get(), effective_bounds, enforced_min_size),
            }
        };
        w.rect.set(rect);

        let decorations = w.decorations.get();
        let drag = state.active.then(|| drag_context_from_state(state));
        let ctx = window_component_context(
            theme,
            state.source_window,
            is_focused,
            decorations.border.has_border(),
            drag,
        );
        let _source_component = state.source_component;
        w.view.drag_cancelled(ctx);
    }

    fn handle_key(
        &mut self,
        k: KeyEvent,
        bounds: Rect,
        mode: WindowManagerInputMode,
        theme: &Theme,
    ) -> WindowManagerAction {
        if k.code == KeyCode::Esc && self.global_drag.is_some() {
            let consumed = self.cancel_global_drag(bounds, theme);
            return WindowManagerAction {
                consumed,
                close: None,
            };
        }

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
        if let Some(modal_id) = modal {
            return self
                .window(modal_id)
                .and_then(|window| hit_test_window(window, x, y));
        }

        for window in self
            .windows
            .iter()
            .rev()
            .filter(|window| docking::window_is_visible_auto_hide_dock(window))
        {
            if let Some(hit) = hit_test_window(window, x, y) {
                return Some(hit);
            }
        }

        for window in self.windows.iter().rev() {
            if docking::window_is_visible_auto_hide_dock(window) {
                continue;
            }
            if let Some(hit) = hit_test_window(window, x, y) {
                return Some(hit);
            }
        }

        None
    }
}

fn hit_test_window(w: &Window, x: u16, y: u16) -> Option<HitTest> {
    let state = w.state.get();
    if state == WindowState::Minimized {
        return None;
    }
    let rect = w.rect.get();
    if !placement::contains(rect, x, y) {
        return None;
    }
    let decorations = w.decorations.get();

    if let Some(dock) = w.dock.get() {
        if matches!(dock.auto_hide, super::DockAutoHide::Enabled { .. })
            && placement::contains(docking::dock_handle_rect(rect, &dock), x, y)
        {
            return Some(HitTest {
                window_id: w.id,
                region: HitRegion::DockAutoHideHandle,
            });
        }

        if w.resizable.get()
            && !matches!(
                dock.auto_hide,
                super::DockAutoHide::Enabled { visible: false }
            )
            && placement::contains(docking::dock_resize_edge_rect(rect, dock.side), x, y)
        {
            return Some(HitTest {
                window_id: w.id,
                region: HitRegion::DockResizeEdge(dock.side),
            });
        }
    }

    if decorations.border.has_border()
        && w.dock.get().is_none()
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

    Some(HitTest {
        window_id: w.id,
        region: HitRegion::Body,
    })
}

fn drag_distance_reached_threshold(state: &GlobalDragState, x: u16, y: u16) -> bool {
    let threshold = state.source.threshold;
    threshold == 0
        || state.start_x.abs_diff(x) >= threshold
        || state.start_y.abs_diff(y) >= threshold
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

fn window_component_context<'a>(
    theme: &'a Theme,
    window_id: WindowId,
    is_focused: bool,
    has_border: bool,
    drag: Option<DragContext<'a>>,
) -> ComponentContext<'a> {
    ComponentContext {
        theme,
        window_id,
        is_focused,
        scrollbar_host: if has_border {
            ScrollbarHost::Window
        } else {
            ScrollbarHost::Component
        },
        tab_mode: TabMode::Cycle,
        mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        drag,
    }
}
