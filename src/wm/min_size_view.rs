use core::convert::Infallible;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Rect, Size};
use ratatui::style::Style;

use crate::reactive::Binding;
use crate::view::{View, ViewContext, ViewEventResult};
use crate::views::scroll::{ScrollConfig, ScrollOffset, clamp_scroll_offset, max_scroll_offset};
use crate::wm::WindowMinSizeMode;

#[derive(Debug, Clone)]
struct OffscreenBackend {
    buffer: Buffer,
    fill_style: Style,
    cursor_visible: bool,
    cursor_pos: Position,
}

impl OffscreenBackend {
    fn new(width: u16, height: u16, style: Style) -> Self {
        let rect = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(rect);
        fill_buffer(&mut buffer, style);
        Self {
            buffer,
            fill_style: style,
            cursor_visible: false,
            cursor_pos: Position::new(0, 0),
        }
    }

    fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    fn cursor_position(&self) -> Position {
        self.cursor_pos
    }
}

impl Backend for OffscreenBackend {
    type Error = Infallible;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        for (x, y, c) in content {
            if x < self.buffer.area.width && y < self.buffer.area.height {
                self.buffer[(x, y)] = c.clone();
            }
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.cursor_visible = false;
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.cursor_visible = true;
        Ok(())
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        Ok(self.cursor_pos)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.cursor_pos = position.into();
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        fill_buffer(&mut self.buffer, self.fill_style);
        Ok(())
    }

    fn clear_region(&mut self, _clear_type: ClearType) -> Result<(), Self::Error> {
        // This backend is only used for single-frame offscreen rendering; clearing everything
        // is sufficient for our needs.
        self.clear()
    }

    fn size(&self) -> Result<Size, Self::Error> {
        Ok(self.buffer.area.as_size())
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        Ok(WindowSize {
            columns_rows: self.buffer.area.as_size(),
            pixels: Size::new(0, 0),
        })
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn mouse_coords_local_to_area(area: Rect, m: MouseEvent) -> Option<(u16, u16)> {
    if contains(area, m.column, m.row) {
        return Some((
            m.column.saturating_sub(area.x),
            m.row.saturating_sub(area.y),
        ));
    }

    // Nested containers may forward mouse coordinates already relative to this view.
    if m.column < area.width && m.row < area.height {
        return Some((m.column, m.row));
    }

    None
}

fn fill_buffer(buf: &mut Buffer, style: Style) {
    for cell in buf.content.iter_mut() {
        cell.reset();
        cell.set_symbol(" ");
        cell.set_style(style);
    }
}

/// Window-level minimum-size overflow handler.
///
/// In `Enforce` mode this is a transparent wrapper around the inner view.
///
/// In `Clip`/`Scroll` modes, when the viewport is smaller than the inner view's minimum size,
/// the inner view is rendered at its minimum size and then clipped (or panned with scrollbars).
pub(crate) struct WindowMinSizeView {
    inner: Box<dyn View>,
    mode: Binding<WindowMinSizeMode>,
    overflow_active: bool,
    viewport_size: (u16, u16),
    content_size: (u16, u16),
    scroll: ScrollOffset,
    last_area: Option<Rect>,
    scroll_config: ScrollConfig,
}

impl WindowMinSizeView {
    pub(crate) fn new(inner: Box<dyn View>, mode: Binding<WindowMinSizeMode>) -> Self {
        Self {
            inner,
            mode,
            overflow_active: false,
            viewport_size: (0, 0),
            content_size: (0, 0),
            scroll: ScrollOffset::ZERO,
            last_area: None,
            scroll_config: ScrollConfig::default(),
        }
    }

    fn overflow_mode(&self) -> Option<WindowMinSizeMode> {
        if !self.overflow_active {
            return None;
        }
        match self.mode.get() {
            WindowMinSizeMode::Enforce => None,
            WindowMinSizeMode::Clip => Some(WindowMinSizeMode::Clip),
            WindowMinSizeMode::Scroll => Some(WindowMinSizeMode::Scroll),
        }
    }

    fn should_overflow(&self, area: Rect) -> bool {
        let mode = self.mode.get();
        if matches!(mode, WindowMinSizeMode::Enforce) {
            return false;
        }

        let (min_w, min_h) = self.inner.min_size();
        area.width < min_w || area.height < min_h
    }

    fn draw_overflow(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ViewContext<'_>,
        scroll: ScrollOffset,
    ) {
        let (min_w, min_h) = self.inner.min_size();
        let content_w = area.width.max(min_w);
        let content_h = area.height.max(min_h);

        self.viewport_size = (area.width, area.height);
        self.content_size = (content_w, content_h);

        let clamped = clamp_scroll_offset(self.content_size, self.viewport_size, scroll);
        self.scroll = clamped;

        if area.width == 0 || area.height == 0 {
            return;
        }

        let inner_ctx = ViewContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused,
            scrollbar_host: ctx.scrollbar_host.for_child(),
            tab_mode: ctx.tab_mode,
        };

        let backend =
            OffscreenBackend::new(content_w.max(1), content_h.max(1), ctx.theme.window_bg);
        let mut terminal = Terminal::new(backend).expect("offscreen terminal");
        terminal
            .try_draw(|f| {
                self.inner
                    .draw(f, Rect::new(0, 0, content_w, content_h), inner_ctx);
                Ok::<(), Infallible>(())
            })
            .expect("offscreen draw");

        let backend = terminal.backend();
        let src_buf = backend.buffer();
        let dst_buf = frame.buffer_mut();

        let scroll = self.scroll;

        for dy in 0..area.height {
            let src_y = scroll.y.saturating_add(dy);
            for dx in 0..area.width {
                let src_x = scroll.x.saturating_add(dx);
                let Some(cell) = src_buf.cell((src_x, src_y)) else {
                    continue;
                };
                if let Some(dst) =
                    dst_buf.cell_mut((area.x.saturating_add(dx), area.y.saturating_add(dy)))
                {
                    *dst = cell.clone();
                }
            }
        }

        if backend.cursor_visible() && ctx.is_focused {
            let Position { x: cx, y: cy } = backend.cursor_position();
            let within_x = cx >= scroll.x && cx < scroll.x.saturating_add(area.width);
            let within_y = cy >= scroll.y && cy < scroll.y.saturating_add(area.height);
            if within_x && within_y {
                let vx = cx.saturating_sub(scroll.x);
                let vy = cy.saturating_sub(scroll.y);
                frame.set_cursor_position((area.x.saturating_add(vx), area.y.saturating_add(vy)));
            }
        }
    }

    fn forward_event_scrolled(
        &mut self,
        event: &Event,
        ctx: ViewContext<'_>,
        scroll: ScrollOffset,
    ) -> ViewEventResult {
        let Some(area) = self.last_area else {
            return ViewEventResult::ignored();
        };

        let inner_ctx = ViewContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused,
            scrollbar_host: ctx.scrollbar_host.for_child(),
            tab_mode: ctx.tab_mode,
        };

        if let Event::Mouse(m) = event {
            let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
                return ViewEventResult::ignored();
            };
            let translated = MouseEvent {
                kind: m.kind,
                column: local_x.saturating_add(scroll.x),
                row: local_y.saturating_add(scroll.y),
                modifiers: m.modifiers,
            };
            return self
                .inner
                .handle_event(&Event::Mouse(translated), inner_ctx);
        }

        self.inner.handle_event(event, inner_ctx)
    }

    fn scroll_by(&mut self, dx: i16, dy: i16) -> bool {
        let max = max_scroll_offset(self.content_size, self.viewport_size);
        let scroll = self.scroll;
        let desired = ScrollOffset {
            x: if dx.is_negative() {
                scroll.x.saturating_sub(dx.wrapping_abs() as u16)
            } else {
                scroll.x.saturating_add(dx as u16)
            },
            y: if dy.is_negative() {
                scroll.y.saturating_sub(dy.wrapping_abs() as u16)
            } else {
                scroll.y.saturating_add(dy as u16)
            },
        };
        let clamped = clamp_scroll_offset(self.content_size, self.viewport_size, desired);
        let changed = clamped != scroll;
        self.scroll = clamped;
        if self.scroll.x > max.x || self.scroll.y > max.y {
            self.scroll = max;
        }
        changed
    }
}

impl View for WindowMinSizeView {
    fn is_focusable(&self) -> bool {
        self.inner.is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        self.inner.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.inner.focus_last()
    }

    fn min_width(&self) -> u16 {
        self.inner.min_width()
    }

    fn min_height(&self) -> u16 {
        self.inner.min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.inner.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.inner.desired_height()
    }

    fn children(&self) -> &[crate::views::ViewNode] {
        self.inner.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<crate::views::ViewNode>> {
        self.inner.children_mut()
    }

    fn is_scrollable(&self) -> bool {
        match self.overflow_mode() {
            Some(WindowMinSizeMode::Scroll) => true,
            Some(WindowMinSizeMode::Clip) => false,
            None => self.inner.is_scrollable(),
            Some(WindowMinSizeMode::Enforce) => self.inner.is_scrollable(),
        }
    }

    fn content_size(&self) -> (u16, u16) {
        if matches!(self.overflow_mode(), Some(WindowMinSizeMode::Scroll)) {
            self.content_size
        } else {
            self.inner.content_size()
        }
    }

    fn scroll_offset(&self) -> (u16, u16) {
        if matches!(self.overflow_mode(), Some(WindowMinSizeMode::Scroll)) {
            (self.scroll.x, self.scroll.y)
        } else {
            self.inner.scroll_offset()
        }
    }

    fn viewport_size(&self) -> (u16, u16) {
        if matches!(self.overflow_mode(), Some(WindowMinSizeMode::Scroll)) {
            self.viewport_size
        } else {
            self.inner.viewport_size()
        }
    }

    fn scroll_config(&self) -> ScrollConfig {
        if matches!(self.overflow_mode(), Some(WindowMinSizeMode::Scroll)) {
            self.scroll_config
        } else {
            self.inner.scroll_config()
        }
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        if matches!(self.overflow_mode(), Some(WindowMinSizeMode::Scroll)) {
            self.scroll =
                clamp_scroll_offset(self.content_size, self.viewport_size, ScrollOffset { x, y });
        } else {
            self.inner.set_scroll_offset(x, y);
        }
    }

    fn scroll_to_child(&mut self, child_id: crate::views::ViewId) {
        if matches!(self.overflow_mode(), Some(WindowMinSizeMode::Scroll)) {
            // Window-level panning does not know how to target individual descendants; defer.
            self.inner.scroll_to_child(child_id);
        } else {
            self.inner.scroll_to_child(child_id);
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: ViewContext<'_>) -> ViewEventResult {
        let Some(area) = self.last_area else {
            return self.inner.handle_event(event, ctx);
        };

        match self.overflow_mode() {
            Some(WindowMinSizeMode::Clip) => {
                self.forward_event_scrolled(event, ctx, ScrollOffset::ZERO)
            }
            Some(WindowMinSizeMode::Scroll) => {
                let forwarded = self.forward_event_scrolled(event, ctx, self.scroll);
                if forwarded.is_consumed() {
                    return forwarded;
                }

                match event {
                    Event::Key(KeyEvent { code, kind, .. }) => {
                        if matches!(kind, KeyEventKind::Release) {
                            return ViewEventResult::ignored();
                        }

                        let max = max_scroll_offset(self.content_size, self.viewport_size);
                        let changed = match code {
                            KeyCode::Up => self.scroll_by(0, -1),
                            KeyCode::Down => self.scroll_by(0, 1),
                            KeyCode::Left => self.scroll_by(-1, 0),
                            KeyCode::Right => self.scroll_by(1, 0),
                            KeyCode::PageUp => self.scroll_by(0, -(self.viewport_size.1 as i16)),
                            KeyCode::PageDown => self.scroll_by(0, self.viewport_size.1 as i16),
                            KeyCode::Home => {
                                let before = self.scroll;
                                self.scroll = ScrollOffset::ZERO;
                                before != self.scroll
                            }
                            KeyCode::End => {
                                let before = self.scroll;
                                self.scroll = max;
                                before != self.scroll
                            }
                            _ => false,
                        };

                        if changed {
                            ViewEventResult::consumed()
                        } else {
                            ViewEventResult::ignored()
                        }
                    }
                    Event::Mouse(m) => {
                        if mouse_coords_local_to_area(area, *m).is_none() {
                            return ViewEventResult::ignored();
                        }

                        let step = self.scroll_config.wheel_step as i16;
                        let changed = match m.kind {
                            MouseEventKind::ScrollUp => self.scroll_by(0, -step),
                            MouseEventKind::ScrollDown => self.scroll_by(0, step),
                            MouseEventKind::ScrollLeft => self.scroll_by(-step, 0),
                            MouseEventKind::ScrollRight => self.scroll_by(step, 0),
                            _ => false,
                        };

                        if changed {
                            ViewEventResult::consumed()
                        } else {
                            ViewEventResult::ignored()
                        }
                    }
                    _ => ViewEventResult::ignored(),
                }
            }
            Some(WindowMinSizeMode::Enforce) | None => self.inner.handle_event(event, ctx),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.last_area = Some(area);

        let overflow_now = self.should_overflow(area);
        self.overflow_active = overflow_now;

        match self.overflow_mode() {
            Some(WindowMinSizeMode::Clip) => {
                self.draw_overflow(frame, area, ctx, ScrollOffset::ZERO);
            }
            Some(WindowMinSizeMode::Scroll) => {
                self.draw_overflow(frame, area, ctx, self.scroll);
            }
            Some(WindowMinSizeMode::Enforce) | None => {
                self.inner.draw(frame, area, ctx);
            }
        }
    }
}
