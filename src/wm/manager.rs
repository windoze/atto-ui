use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;
use crate::view::{ScrollbarHost, ViewContext, ViewEventResult};
use crate::views::{
    ScrollbarDrag, ScrollbarHit, scroll_offset_from_thumb_start, scrollbar_hit_test,
    scrollbar_layout_1d, should_show_scrollbar,
};

use super::{Window, WindowBorderStyle, WindowButtons, WindowId, WindowKind, WindowState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowManagerInputMode {
    Normal,
    WindowManagement,
}

#[derive(Debug, Default)]
pub struct WindowManagerAction {
    pub consumed: bool,
    pub close: Option<WindowId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResizeCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug)]
enum DragKind {
    Move {
        offset_x: u16,
        offset_y: u16,
    },
    Resize {
        start_rect: Rect,
        corner: ResizeCorner,
    },
    Scrollbar {
        drag: ScrollbarDrag,
    },
}

#[derive(Clone, Copy, Debug)]
struct DragState {
    window_id: WindowId,
    kind: DragKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HitRegion {
    TitleBar,
    MinimizeButton,
    MaximizeButton,
    CloseButton,
    ResizeHandle(ResizeCorner),
    VScrollbar,
    HScrollbar,
    Body,
}

#[derive(Clone, Copy, Debug)]
struct HitTest {
    window_id: WindowId,
    region: HitRegion,
}

#[derive(Default)]
pub struct WindowManager {
    next_id: u64,
    windows: Vec<Window>,
    focused: Option<WindowId>,
    drag: Option<DragState>,
    mouse_capture: bool,
}

impl WindowManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    pub fn windows_mut(&mut self) -> &mut [Window] {
        &mut self.windows
    }

    pub fn has_active_modal(&self) -> bool {
        self.active_modal_id().is_some()
    }

    pub fn focused(&self) -> Option<WindowId> {
        self.active_modal_id().or(self.focused)
    }

    pub fn add_window(&mut self, mut window: Window, bounds: Rect) -> WindowId {
        self.next_id += 1;
        let id = WindowId(self.next_id);
        window.id = id;

        let (min_w, min_h) = window.min_size.get();
        let (view_min_w, view_min_h) = window.view.min_size();
        let min_size = (min_w.max(view_min_w), min_h.max(view_min_h));
        window.min_size.set(min_size);

        let rect = normalize_rect(window.rect.get(), bounds, min_size);
        window.rect.set(rect);

        if window.kind == WindowKind::Modal {
            // Ensure modals are always on top and focused.
            self.focused = Some(id);
        } else if window.kind.is_focusable() {
            self.focused = Some(id);
        }

        self.windows.push(window);
        self.bring_to_front(id);
        id
    }

    pub fn close(&mut self, id: WindowId) {
        self.drag = match self.drag {
            Some(d) if d.window_id == id => None,
            other => other,
        };
        self.mouse_capture = false;
        let was_focused = self.focused == Some(id);
        self.windows.retain(|w| w.id != id);
        if was_focused {
            self.focused = self.topmost_focusable_id();
        }
    }

    pub fn request_close(&mut self, id: WindowId) -> bool {
        let allow = {
            let Some(w) = self.window_mut(id) else {
                return false;
            };
            w.allow_close()
        };
        if allow {
            self.close(id);
            true
        } else {
            false
        }
    }

    pub fn bring_to_front(&mut self, id: WindowId) {
        let Some(pos) = self.windows.iter().position(|w| w.id == id) else {
            return;
        };
        let w = self.windows.remove(pos);
        self.windows.push(w);
    }

    pub fn focus(&mut self, id: WindowId) {
        if self.active_modal_id().is_some() {
            return;
        }
        if !self
            .windows
            .iter()
            .any(|w| w.id == id && w.kind.is_focusable())
        {
            return;
        }
        self.focused = Some(id);
        self.bring_to_front(id);
    }

    pub fn focus_next(&mut self) {
        if self.active_modal_id().is_some() {
            return;
        }
        let ids: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|w| w.kind.is_focusable() && w.state.get() != WindowState::Minimized)
            .map(|w| w.id)
            .collect();
        if ids.is_empty() {
            self.focused = None;
            return;
        }
        let current = self.focused;
        let next = match current.and_then(|c| ids.iter().position(|id| *id == c)) {
            Some(idx) => ids[(idx + 1) % ids.len()],
            None => ids[0],
        };
        self.focused = Some(next);
        self.bring_to_front(next);
    }

    pub fn move_focused(&mut self, dx: i16, dy: i16, bounds: Rect) {
        let Some(id) = self.focused() else { return };
        self.move_window(id, dx, dy, bounds);
    }

    pub fn resize_focused(&mut self, dw: i16, dh: i16, bounds: Rect) {
        let Some(id) = self.focused() else { return };
        self.resize_window(id, dw, dh, bounds);
    }

    pub fn toggle_maximize_focused(&mut self, bounds: Rect) {
        let Some(id) = self.focused() else { return };
        let can_toggle = self
            .windows
            .iter()
            .any(|w| w.id == id && can_toggle_maximize(w));
        if can_toggle {
            self.toggle_maximize(id, bounds);
        }
    }

    pub fn minimize_focused(&mut self) {
        let Some(id) = self.focused() else { return };
        let can_minimize = self.windows.iter().any(|w| w.id == id && can_minimize(w));
        if !can_minimize {
            return;
        }
        if let Some(w) = self.window_mut(id) {
            w.state.set(WindowState::Minimized);
        }
        self.focused = self.topmost_focusable_id();
    }

    pub fn restore_focused(&mut self) {
        let Some(id) = self.focused() else { return };
        if let Some(w) = self.window_mut(id)
            && w.state.get() == WindowState::Minimized
        {
            w.state.set(WindowState::Normal);
        }
    }

    pub fn handle_event(
        &mut self,
        event: &Event,
        bounds: Rect,
        mode: WindowManagerInputMode,
    ) -> WindowManagerAction {
        match event {
            Event::Mouse(m) => self.handle_mouse(m, bounds),
            Event::Key(k) => self.handle_key(*k, bounds, mode),
            _ => WindowManagerAction::default(),
        }
    }

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

            let rect = match state {
                WindowState::Maximized => bounds,
                _ => normalize_rect(window.rect.get(), bounds, window.min_size.get()),
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
                let title = window.title.get();
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
                let buttons = effective_titlebar_buttons(window, &decorations);
                draw_titlebar(
                    frame.buffer_mut(),
                    rect,
                    &title,
                    title_style,
                    &buttons,
                    theme,
                );
            }

            let inner = window.inner_rect();
            let ctx = ViewContext {
                theme,
                window_id: window.id,
                is_focused,
                scrollbar_host: if decorations.border.has_border() {
                    ScrollbarHost::Window
                } else {
                    ScrollbarHost::View
                },
            };
            window.view.draw(frame, inner, ctx);

            if decorations.border.has_border() {
                draw_window_border_scrollbars(
                    frame.buffer_mut(),
                    rect,
                    inner,
                    window.view.as_ref(),
                    theme,
                );
            }
        }
    }

    pub fn dispatch_to_focused_view(
        &mut self,
        event: &Event,
        bounds: Rect,
        theme: &Theme,
    ) -> Option<(WindowId, ViewEventResult)> {
        let id = self.focused()?;
        let idx = self.windows.iter().position(|w| w.id == id)?;
        let is_focused = true;
        let action = {
            let w = &mut self.windows[idx];
            let state = w.state.get();
            if state == WindowState::Minimized {
                return None;
            }
            // Ensure rect stays clamped before passing input.
            let rect = match state {
                WindowState::Maximized => bounds,
                _ => normalize_rect(w.rect.get(), bounds, w.min_size.get()),
            };
            w.rect.set(rect);
            if let Event::Mouse(m) = event {
                // Views render inside the inner rect; clicks on window chrome/borders should not
                // be delivered to the view layer.
                let inner = w.inner_rect();
                if !contains(inner, m.column, m.row) {
                    return Some((id, ViewEventResult::ignored()));
                }
            }
            let ctx = ViewContext {
                theme,
                window_id: id,
                is_focused,
                scrollbar_host: if w.decorations.get().border.has_border() {
                    ScrollbarHost::Window
                } else {
                    ScrollbarHost::View
                },
            };
            w.view.handle_event(event, ctx)
        };
        Some((id, action))
    }

    pub fn window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    fn topmost_focusable_id(&self) -> Option<WindowId> {
        self.windows
            .iter()
            .rev()
            .find(|w| w.kind.is_focusable() && w.state.get() != WindowState::Minimized)
            .map(|w| w.id)
    }

    fn active_modal_id(&self) -> Option<WindowId> {
        self.windows
            .iter()
            .rev()
            .find(|w| w.kind.is_modal() && w.state.get() != WindowState::Minimized)
            .map(|w| w.id)
    }

    fn handle_mouse(&mut self, m: &MouseEvent, bounds: Rect) -> WindowManagerAction {
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
                        .windows
                        .iter()
                        .any(|w| w.id == window_id && w.kind.is_focusable())
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
                            .windows
                            .iter()
                            .any(|w| w.id == window_id && can_toggle_maximize(w));
                        if can_maximize {
                            self.toggle_maximize(window_id, bounds);
                        }
                    }
                    HitRegion::MinimizeButton => {
                        let minimized = match self.window_mut(window_id) {
                            Some(w) if can_minimize(w) => {
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
                        if let Some(w) = self.window_mut(window_id)
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
                    HitRegion::ResizeHandle(corner) => {
                        if let Some(w) = self.window_mut(window_id)
                            && w.resizable.get()
                            && w.state.get() != WindowState::Maximized
                        {
                            let start_rect = normalize_rect(w.rect.get(), bounds, w.min_size.get());
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
                        w.rect.set(normalize_rect(rect, bounds, w.min_size.get()));
                    }
                    DragKind::Resize { start_rect, corner } => {
                        if !w.resizable.get() || w.state.get() == WindowState::Maximized {
                            return WindowManagerAction::default();
                        }
                        w.rect.set(resize_rect_from_corner(
                            start_rect,
                            corner,
                            m.column,
                            m.row,
                            bounds,
                            w.min_size.get(),
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
            if !contains(rect, x, y) {
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
                if let Some(btn) = hit_test_buttons(w, x, y) {
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

    fn move_window(&mut self, id: WindowId, dx: i16, dy: i16, bounds: Rect) {
        let Some(w) = self.window_mut(id) else { return };
        if !w.movable.get() || w.state.get() == WindowState::Maximized {
            return;
        }
        let mut rect = w.rect.get();
        rect.x = add_signed(rect.x, dx);
        rect.y = add_signed(rect.y, dy);
        w.rect.set(normalize_rect(rect, bounds, w.min_size.get()));
    }

    fn resize_window(&mut self, id: WindowId, dw: i16, dh: i16, bounds: Rect) {
        let Some(w) = self.window_mut(id) else { return };
        if !w.resizable.get() || w.state.get() == WindowState::Maximized {
            return;
        }
        let (min_w, min_h) = w.min_size.get();
        let mut rect = w.rect.get();
        rect.width = add_signed(rect.width, dw).max(min_w);
        rect.height = add_signed(rect.height, dh).max(min_h);
        w.rect.set(normalize_rect(rect, bounds, w.min_size.get()));
    }

    fn toggle_maximize(&mut self, id: WindowId, bounds: Rect) {
        let Some(w) = self.window_mut(id) else { return };
        match w.state.get() {
            WindowState::Maximized => {
                w.state.set(WindowState::Normal);
                if let Some(r) = w.restore_rect.take() {
                    w.rect.set(normalize_rect(r, bounds, w.min_size.get()));
                }
            }
            WindowState::Normal => {
                w.restore_rect = Some(w.rect.get());
                w.state.set(WindowState::Maximized);
                w.rect.set(bounds);
            }
            WindowState::Minimized => {}
        }
    }
}

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn add_signed(v: u16, dv: i16) -> u16 {
    if dv.is_negative() {
        v.saturating_sub(dv.wrapping_abs() as u16)
    } else {
        v.saturating_add(dv as u16)
    }
}

fn normalize_rect(mut rect: Rect, bounds: Rect, min_size: (u16, u16)) -> Rect {
    let min_w = min_size.0.min(bounds.width);
    let min_h = min_size.1.min(bounds.height);

    rect.width = rect.width.max(min_w).min(bounds.width);
    rect.height = rect.height.max(min_h).min(bounds.height);

    let max_x = bounds
        .x
        .saturating_add(bounds.width.saturating_sub(rect.width));
    let max_y = bounds
        .y
        .saturating_add(bounds.height.saturating_sub(rect.height));

    rect.x = rect.x.clamp(bounds.x, max_x);
    rect.y = rect.y.clamp(bounds.y, max_y);
    rect
}

fn resize_rect_from_corner(
    start: Rect,
    corner: ResizeCorner,
    pointer_x: u16,
    pointer_y: u16,
    bounds: Rect,
    min_size: (u16, u16),
) -> Rect {
    if start.width == 0 || start.height == 0 || bounds.width == 0 || bounds.height == 0 {
        return start;
    }

    let bounds_left = bounds.x;
    let bounds_top = bounds.y;
    let bounds_right = bounds.x.saturating_add(bounds.width).saturating_sub(1);
    let bounds_bottom = bounds.y.saturating_add(bounds.height).saturating_sub(1);

    let start_left = start.x;
    let start_top = start.y;
    let start_right = start.x.saturating_add(start.width).saturating_sub(1);
    let start_bottom = start.y.saturating_add(start.height).saturating_sub(1);

    let (left, right) = match corner {
        ResizeCorner::BottomRight | ResizeCorner::TopRight => {
            // Left is fixed.
            let max_w = bounds_right.saturating_sub(start_left).saturating_add(1);
            let min_w = min_size.0.min(max_w);
            let right_min = start_left.saturating_add(min_w).saturating_sub(1);
            (start_left, pointer_x.clamp(right_min, bounds_right))
        }
        ResizeCorner::BottomLeft | ResizeCorner::TopLeft => {
            // Right is fixed.
            let max_w = start_right.saturating_sub(bounds_left).saturating_add(1);
            let min_w = min_size.0.min(max_w);
            let left_max = start_right.saturating_sub(min_w).saturating_add(1);
            (pointer_x.clamp(bounds_left, left_max), start_right)
        }
    };

    let (top, bottom) = match corner {
        ResizeCorner::BottomRight | ResizeCorner::BottomLeft => {
            // Top is fixed.
            let max_h = bounds_bottom.saturating_sub(start_top).saturating_add(1);
            let min_h = min_size.1.min(max_h);
            let bottom_min = start_top.saturating_add(min_h).saturating_sub(1);
            (start_top, pointer_y.clamp(bottom_min, bounds_bottom))
        }
        ResizeCorner::TopRight | ResizeCorner::TopLeft => {
            // Bottom is fixed.
            let max_h = start_bottom.saturating_sub(bounds_top).saturating_add(1);
            let min_h = min_size.1.min(max_h);
            let top_max = start_bottom.saturating_sub(min_h).saturating_add(1);
            (pointer_y.clamp(bounds_top, top_max), start_bottom)
        }
    };

    Rect {
        x: left,
        y: top,
        width: right.saturating_sub(left).saturating_add(1),
        height: bottom.saturating_sub(top).saturating_add(1),
    }
}

fn draw_shadow(buf: &mut Buffer, rect: Rect, bounds: Rect, style: Style) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

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

fn draw_window_border_scrollbars(
    buf: &mut Buffer,
    rect: Rect,
    inner: Rect,
    view: &dyn crate::view::View,
    theme: &Theme,
) {
    if rect.width < 3 || rect.height < 3 || inner.width == 0 || inner.height == 0 {
        return;
    }
    if !view.is_scrollable() {
        return;
    }

    let cfg = view.scroll_config();
    let (content_w, content_h) = view.content_size();
    let (viewport_w, viewport_h) = view.viewport_size();
    let (scroll_x, scroll_y) = view.scroll_offset();

    let show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_h, viewport_h);
    let show_h = should_show_scrollbar(cfg.horizontal_scrollbar, content_w, viewport_w);

    let thumb_style = theme.window_bg.patch(theme.scrollbar_thumb);
    let arrow_style = theme.window_bg.patch(theme.scrollbar_arrow);

    let thumb = theme.glyph("scrollbar-thumb").unwrap_or("█");
    let arrow_up = theme.glyph("scrollbar-up-arrow").unwrap_or("▲");
    let arrow_down = theme.glyph("scrollbar-down-arrow").unwrap_or("▼");
    let arrow_left = theme.glyph("scrollbar-left-arrow").unwrap_or("◄");
    let arrow_right = theme.glyph("scrollbar-right-arrow").unwrap_or("►");

    // Vertical scrollbar on the right border (excluding corners).
    if show_v {
        let layout = scrollbar_layout_1d(inner.height, viewport_h, content_h, scroll_y, cfg.arrows);
        let x = rect.x.saturating_add(rect.width).saturating_sub(1);
        for i in 0..inner.height {
            let (symbol, style) = if layout.has_arrows && i == 0 {
                (arrow_up, arrow_style)
            } else if layout.has_arrows && i == layout.bar_len.saturating_sub(1) {
                (arrow_down, arrow_style)
            } else if i >= layout.thumb_start
                && i < layout.thumb_start.saturating_add(layout.thumb_len)
            {
                (thumb, thumb_style)
            } else {
                continue;
            };
            if let Some(cell) = buf.cell_mut((x, inner.y.saturating_add(i))) {
                cell.set_symbol(symbol);
                cell.set_style(style);
            }
        }
    }

    // Horizontal scrollbar on the bottom border (excluding corners).
    if show_h {
        let layout = scrollbar_layout_1d(inner.width, viewport_w, content_w, scroll_x, cfg.arrows);
        let y = rect.y.saturating_add(rect.height).saturating_sub(1);
        for i in 0..inner.width {
            let (symbol, style) = if layout.has_arrows && i == 0 {
                (arrow_left, arrow_style)
            } else if layout.has_arrows && i == layout.bar_len.saturating_sub(1) {
                (arrow_right, arrow_style)
            } else if i >= layout.thumb_start
                && i < layout.thumb_start.saturating_add(layout.thumb_len)
            {
                (thumb, thumb_style)
            } else {
                continue;
            };
            if let Some(cell) = buf.cell_mut((inner.x.saturating_add(i), y)) {
                cell.set_symbol(symbol);
                cell.set_style(style);
            }
        }
    }
}

fn draw_titlebar(
    buf: &mut Buffer,
    rect: Rect,
    title: &str,
    style: Style,
    buttons: &WindowButtons,
    theme: &Theme,
) {
    if rect.width < 3 {
        return;
    }
    let inner_left = rect.x.saturating_add(1);
    let inner_right = rect.x.saturating_add(rect.width).saturating_sub(2);
    let mut cursor = inner_left;

    // Buttons (right side).
    let mut button_cols = Vec::new();
    if buttons.minimize {
        button_cols.push((HitRegion::MinimizeButton, inner_right.saturating_sub(4)));
    }
    if buttons.maximize {
        button_cols.push((HitRegion::MaximizeButton, inner_right.saturating_sub(2)));
    }
    if buttons.close {
        button_cols.push((HitRegion::CloseButton, inner_right));
    }

    // Title, truncated to not overwrite buttons.
    let reserved_right = button_cols
        .iter()
        .map(|(_, col)| *col)
        .min()
        .unwrap_or(inner_right)
        .saturating_sub(2);

    for g in title.graphemes(true) {
        if cursor > reserved_right {
            break;
        }
        let Some(cell) = buf.cell_mut((cursor, rect.y)) else {
            break;
        };
        cell.set_style(style);
        cell.set_symbol(g);
        cursor = cursor
            .saturating_add(UnicodeWidthStr::width(g) as u16)
            .max(cursor + 1);
    }

    // Draw buttons.
    for (region, col) in button_cols {
        if col < inner_left || col > inner_right {
            continue;
        }
        let symbol = match region {
            HitRegion::MinimizeButton => theme.glyph("minimize-button").unwrap_or("−"),
            HitRegion::MaximizeButton => theme.glyph("maximize-button").unwrap_or("□"),
            HitRegion::CloseButton => theme.glyph("close-button").unwrap_or("×"),
            _ => "?",
        };
        if let Some(cell) = buf.cell_mut((col, rect.y)) {
            cell.set_style(style);
            cell.set_symbol(symbol);
        }
    }
}

fn hit_test_buttons(w: &Window, x: u16, y: u16) -> Option<HitRegion> {
    let rect = w.rect.get();
    let deco = w.decorations.get();
    let buttons = effective_titlebar_buttons(w, &deco);
    if y != rect.y || rect.width < 3 {
        return None;
    }
    let inner_right = rect.x.saturating_add(rect.width).saturating_sub(2);
    if buttons.close && x == inner_right {
        return Some(HitRegion::CloseButton);
    }
    if buttons.maximize && x == inner_right.saturating_sub(2) {
        return Some(HitRegion::MaximizeButton);
    }
    if buttons.minimize && x == inner_right.saturating_sub(4) {
        return Some(HitRegion::MinimizeButton);
    }
    None
}

fn effective_titlebar_buttons(w: &Window, deco: &super::WindowDecorations) -> WindowButtons {
    let mut buttons = deco.buttons.clone();
    if !w.resizable.get() {
        buttons.minimize = false;
        buttons.maximize = false;
    }
    buttons
}

fn can_toggle_maximize(w: &Window) -> bool {
    let decorations = w.decorations.get();
    let state = w.state.get();
    if state == WindowState::Maximized {
        // Always allow restoring from maximized, even if the window later becomes fixed-size.
        return true;
    }
    effective_titlebar_buttons(w, &decorations).maximize
}

fn can_minimize(w: &Window) -> bool {
    let decorations = w.decorations.get();
    effective_titlebar_buttons(w, &decorations).minimize
}

#[cfg(test)]
mod tests {
    use super::{WindowManager, draw_shadow};
    use crate::theme::Theme;
    use crate::view::{View, ViewContext, ViewEventResult};
    use crate::views::{ScrollConfig, ScrollbarVisibility};
    use crate::wm::{Window, WindowBorderStyle, WindowKind};
    use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::Frame;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::{Color, Style};

    #[derive(Default)]
    struct DummyView;

    impl View for DummyView {
        fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ViewContext<'_>) {}
        fn handle_event(&mut self, _event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
            ViewEventResult::ignored()
        }
    }

    #[test]
    fn focus_cycles_between_windows() {
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut wm = WindowManager::new();
        let id1 = wm.add_window(
            Window::new(
                WindowKind::Normal,
                "One",
                Rect {
                    x: 1,
                    y: 1,
                    width: 20,
                    height: 6,
                },
                Box::new(DummyView),
            ),
            bounds,
        );
        let id2 = wm.add_window(
            Window::new(
                WindowKind::Normal,
                "Two",
                Rect {
                    x: 3,
                    y: 3,
                    width: 20,
                    height: 6,
                },
                Box::new(DummyView),
            ),
            bounds,
        );

        assert_eq!(wm.focused(), Some(id2));
        wm.focus_next();
        assert_eq!(wm.focused(), Some(id1));
    }

    #[test]
    fn modal_window_blocks_focus_changes() {
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut wm = WindowManager::new();
        let _id1 = wm.add_window(
            Window::new(
                WindowKind::Normal,
                "One",
                Rect {
                    x: 1,
                    y: 1,
                    width: 20,
                    height: 6,
                },
                Box::new(DummyView),
            ),
            bounds,
        );
        let modal_id = wm.add_window(
            Window::new(
                WindowKind::Modal,
                "Modal",
                Rect {
                    x: 10,
                    y: 8,
                    width: 30,
                    height: 8,
                },
                Box::new(DummyView),
            ),
            bounds,
        );

        assert_eq!(wm.focused(), Some(modal_id));
        wm.focus_next();
        assert_eq!(wm.focused(), Some(modal_id));
    }

    #[test]
    fn window_scrollbars_do_not_overwrite_resize_corners() {
        #[derive(Default)]
        struct ScrollableDummyView {
            viewport: (u16, u16),
        }

        impl View for ScrollableDummyView {
            fn is_scrollable(&self) -> bool {
                true
            }

            fn content_size(&self) -> (u16, u16) {
                (200, 200)
            }

            fn scroll_offset(&self) -> (u16, u16) {
                (0, 0)
            }

            fn viewport_size(&self) -> (u16, u16) {
                self.viewport
            }

            fn scroll_config(&self) -> ScrollConfig {
                ScrollConfig::default()
                    .vertical_scrollbar(ScrollbarVisibility::Always)
                    .horizontal_scrollbar(ScrollbarVisibility::Always)
            }

            fn draw(&mut self, _frame: &mut Frame<'_>, area: Rect, _ctx: ViewContext<'_>) {
                self.viewport = (area.width, area.height);
            }
        }

        let bounds = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let rect = Rect {
            x: 2,
            y: 2,
            width: 20,
            height: 8,
        };

        let mut wm = WindowManager::new();
        wm.add_window(
            Window::new(
                WindowKind::Normal,
                "Scroll",
                rect,
                Box::new(ScrollableDummyView::default()),
            ),
            bounds,
        );

        let theme = Theme::dark();
        let backend = TestBackend::new(bounds.width, bounds.height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

        let buf = terminal.backend().buffer();

        let left = rect.x;
        let top = rect.y;
        let right = rect.x.saturating_add(rect.width).saturating_sub(1);
        let bottom = rect.y.saturating_add(rect.height).saturating_sub(1);

        assert_eq!(
            buf.cell((left, top)).expect("top-left").symbol(),
            "╔",
            "top-left corner should remain a resize handle"
        );
        assert_eq!(
            buf.cell((right, top)).expect("top-right").symbol(),
            "╗",
            "top-right corner should remain a resize handle"
        );
        assert_eq!(
            buf.cell((left, bottom)).expect("bottom-left").symbol(),
            "╚",
            "bottom-left corner should remain a resize handle"
        );
        assert_eq!(
            buf.cell((right, bottom)).expect("bottom-right").symbol(),
            "╝",
            "bottom-right corner should remain a resize handle"
        );

        // Sanity: scrollbar arrows are drawn adjacent to the corners, not on them.
        assert_eq!(buf.cell((right, top + 1)).expect("vbar up").symbol(), "▲");
        assert_eq!(
            buf.cell((left + 1, bottom)).expect("hbar left").symbol(),
            "◄"
        );
        assert_eq!(
            buf.cell((right - 1, bottom)).expect("hbar right").symbol(),
            "►"
        );
    }

    #[test]
    fn mouse_drag_resize_handles_work_on_all_corners() {
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let cases = [
            (
                "top-left",
                (2, 2),
                (0, 0),
                Rect {
                    x: 0,
                    y: 0,
                    width: 22,
                    height: 8,
                },
            ),
            (
                "top-right",
                (21, 2),
                (25, 0),
                Rect {
                    x: 2,
                    y: 0,
                    width: 24,
                    height: 8,
                },
            ),
            (
                "bottom-left",
                (2, 7),
                (0, 9),
                Rect {
                    x: 0,
                    y: 2,
                    width: 22,
                    height: 8,
                },
            ),
            (
                "bottom-right",
                (21, 7),
                (25, 9),
                Rect {
                    x: 2,
                    y: 2,
                    width: 24,
                    height: 8,
                },
            ),
        ];

        for (label, down, drag, expected) in cases {
            let mut wm = WindowManager::new();
            let id = wm.add_window(
                Window::new(
                    WindowKind::Normal,
                    "Resizable",
                    Rect {
                        x: 2,
                        y: 2,
                        width: 20,
                        height: 6,
                    },
                    Box::new(DummyView),
                ),
                bounds,
            );

            wm.handle_event(
                &Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: down.0,
                    row: down.1,
                    modifiers: KeyModifiers::NONE,
                }),
                bounds,
                super::WindowManagerInputMode::Normal,
            );
            wm.handle_event(
                &Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Drag(MouseButton::Left),
                    column: drag.0,
                    row: drag.1,
                    modifiers: KeyModifiers::NONE,
                }),
                bounds,
                super::WindowManagerInputMode::Normal,
            );

            let w = wm.window_mut(id).expect("window");
            assert_eq!(w.rect.get(), expected, "case {label}");
        }
    }

    #[test]
    fn close_hook_can_cancel_close_request() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let bounds = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let calls2 = Arc::clone(&calls);

        let mut wm = WindowManager::new();
        let id = wm.add_window(
            Window::new(
                WindowKind::Normal,
                "Hooked",
                Rect {
                    x: 2,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(DummyView),
            )
            .with_close_hook(move |_id| {
                calls2.fetch_add(1, Ordering::SeqCst);
                false
            }),
            bounds,
        );

        assert!(wm.window_mut(id).is_some());
        assert!(!wm.request_close(id), "expected close to be cancelled");
        assert!(wm.window_mut(id).is_some(), "window should remain");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn shadow_includes_bottom_right_corner() {
        let bounds = Rect::new(0, 0, 10, 10);
        let rect = Rect::new(1, 1, 3, 3);
        let style = Style::default().bg(Color::Red);

        let mut buf = Buffer::empty(bounds);
        assert_eq!(buf.cell((4, 4)).unwrap().bg, Color::Reset);

        draw_shadow(&mut buf, rect, bounds, style);

        assert_eq!(buf.cell((4, 4)).unwrap().bg, Color::Red);
    }

    #[test]
    fn window_background_is_opaque_by_default() {
        #[derive(Default)]
        struct UnderlayView {
            target: (u16, u16),
        }

        impl View for UnderlayView {
            fn handle_event(&mut self, _event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
                ViewEventResult::ignored()
            }

            fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, _ctx: ViewContext<'_>) {
                let (x, y) = self.target;
                if x >= area.x
                    && x < area.x.saturating_add(area.width)
                    && y >= area.y
                    && y < area.y.saturating_add(area.height)
                    && let Some(cell) = frame.buffer_mut().cell_mut((x, y))
                {
                    cell.set_symbol("X");
                }
            }
        }

        #[derive(Default)]
        struct OverlayView;

        impl View for OverlayView {
            fn handle_event(&mut self, _event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
                ViewEventResult::ignored()
            }

            fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ViewContext<'_>) {}
        }

        let theme = Theme::dark();
        let bounds = Rect::new(0, 0, 30, 10);
        let target = (5, 3);

        let mut wm = WindowManager::new();
        let underlay = Window::new(
            WindowKind::Normal,
            "Underlay",
            Rect::new(1, 1, 20, 7),
            Box::new(UnderlayView { target }),
        );
        underlay.decorations.update(|d| d.shadow = false);
        wm.add_window(underlay, bounds);

        let overlay_rect = Rect::new(5, 3, 20, 6);
        let overlay = Window::new(
            WindowKind::Normal,
            "Overlay",
            overlay_rect,
            Box::new(OverlayView),
        );
        overlay.decorations.update(|d| d.shadow = false);
        wm.add_window(overlay, bounds);

        let backend = TestBackend::new(bounds.width, bounds.height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

        let cell = terminal.backend().buffer().cell(target).expect("cell");
        assert_ne!(
            cell.bg,
            Color::Reset,
            "expected window background fill to set a non-reset bg color (including border)"
        );
        assert_ne!(
            cell.symbol(),
            "X",
            "expected overlapping window to clear underlay content (including border)"
        );
    }

    #[test]
    fn thin_border_uses_single_line_glyphs_even_when_focused() {
        let theme = Theme::dark();
        let bounds = Rect::new(0, 0, 40, 15);
        let rect = Rect::new(2, 2, 16, 7);

        let mut wm = WindowManager::new();
        let w = Window::new(WindowKind::Normal, "Thin", rect, Box::new(DummyView));
        w.decorations.update(|d| d.border = WindowBorderStyle::Thin);
        wm.add_window(w, bounds);

        let backend = TestBackend::new(bounds.width, bounds.height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

        let buf = terminal.backend().buffer();

        let left = rect.x;
        let top = rect.y;
        let right = rect.x.saturating_add(rect.width).saturating_sub(1);
        let bottom = rect.y.saturating_add(rect.height).saturating_sub(1);

        assert_eq!(buf.cell((left, top)).expect("tl").symbol(), "┌");
        assert_eq!(buf.cell((right, top)).expect("tr").symbol(), "┐");
        assert_eq!(buf.cell((left, bottom)).expect("bl").symbol(), "└");
        assert_eq!(buf.cell((right, bottom)).expect("br").symbol(), "┘");
    }

    #[test]
    fn fixed_size_windows_hide_and_disable_minimize_maximize() {
        let theme = Theme::dark();
        let bounds = Rect::new(0, 0, 40, 15);
        let rect = Rect::new(2, 2, 18, 7);

        let mut wm = WindowManager::new();
        let w = Window::new(WindowKind::Normal, "Fixed", rect, Box::new(DummyView));
        w.resizable.set(false);
        let id = wm.add_window(w, bounds);

        wm.toggle_maximize_focused(bounds);
        wm.minimize_focused();
        let state = wm.window_mut(id).expect("window").state.get();
        assert_eq!(
            state,
            crate::wm::WindowState::Normal,
            "expected fixed-size window to ignore maximize/minimize actions"
        );

        let backend = TestBackend::new(bounds.width, bounds.height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| wm.draw(f, bounds, &theme)).expect("draw");

        let buf = terminal.backend().buffer();
        let inner_right = rect.x.saturating_add(rect.width).saturating_sub(2);

        assert_eq!(
            buf.cell((inner_right, rect.y)).expect("close btn").symbol(),
            "×",
            "expected close button to still be visible"
        );
        assert_eq!(
            buf.cell((inner_right.saturating_sub(2), rect.y))
                .expect("maximize slot")
                .symbol(),
            "═",
            "expected maximize button to be hidden for fixed-size windows"
        );
        assert_eq!(
            buf.cell((inner_right.saturating_sub(4), rect.y))
                .expect("minimize slot")
                .symbol(),
            "═",
            "expected minimize button to be hidden for fixed-size windows"
        );
    }
}
