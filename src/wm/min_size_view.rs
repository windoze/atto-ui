use core::convert::Infallible;

use crossterm::event::{Event, MouseEvent};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Rect, Size};
use ratatui::style::Style;

use crate::composable::scroll::{clamp_scroll_offset, scroll_offset_for_input_event};
use crate::composable::{
    Component, ComponentContext, ComponentId, ComponentNode, DragOffer, DragSource, DropFeedback,
    DynamicTree, EventHandling, EventResult, FocusNav, Layout, MouseCoordinateSpace, ScrollConfig,
    ScrollOffset, Scrollable, TitleBarContent, TitleBarContext,
};
use crate::reactive::Binding;
use crate::wm::WindowMinSizeMode;
use crate::{CallbackRegistry, ComponentSpec, TreeError, TreeOp};
use atto_ui_macros::{ComponentProperties, component_properties};

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

fn mouse_coords_local_to_area(
    area: Rect,
    m: MouseEvent,
    coordinate_space: MouseCoordinateSpace,
) -> Option<(u16, u16)> {
    match coordinate_space {
        MouseCoordinateSpace::Absolute => contains(area, m.column, m.row).then(|| {
            (
                m.column.saturating_sub(area.x),
                m.row.saturating_sub(area.y),
            )
        }),
        MouseCoordinateSpace::Local => {
            (area.width > 0 && area.height > 0 && m.column < area.width && m.row < area.height)
                .then_some((m.column, m.row))
        }
    }
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
#[derive(ComponentProperties)]
pub(crate) struct WindowMinSizeView {
    inner: Box<dyn Component>,
    mode: Binding<WindowMinSizeMode>,
    overflow_active: bool,
    viewport_size: (u16, u16),
    content_size: (u16, u16),
    scroll: ScrollOffset,
    last_area: Option<Rect>,
    scroll_config: ScrollConfig,
}

impl WindowMinSizeView {
    pub(crate) fn new(inner: Box<dyn Component>, mode: Binding<WindowMinSizeMode>) -> Self {
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
        ctx: ComponentContext<'_>,
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

        let inner_ctx = ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused,
            scrollbar_host: ctx.scrollbar_host.for_child(),
            tab_mode: ctx.tab_mode,
            mouse_coordinate_space: ctx.mouse_coordinate_space,
            drag: ctx.drag,
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
        ctx: ComponentContext<'_>,
        scroll: ScrollOffset,
    ) -> EventResult {
        let Some(area) = self.last_area else {
            return EventResult::ignored();
        };

        let inner_ctx = ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused,
            scrollbar_host: ctx.scrollbar_host.for_child(),
            tab_mode: ctx.tab_mode,
            mouse_coordinate_space: ctx.mouse_coordinate_space.for_child(),
            drag: ctx.drag,
        };

        if let Event::Mouse(m) = event {
            let Some((local_x, local_y)) =
                mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
            else {
                return EventResult::ignored();
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
}

#[component_properties]
impl Component for WindowMinSizeView {
    fn type_name(&self) -> &'static str {
        self.inner.type_name()
    }

    fn is_tab_container(&self) -> bool {
        self.inner.is_tab_container()
    }

    fn property_names(&self) -> Vec<&'static str> {
        let mut props = self.__component_property_names();
        props.extend(self.inner.property_names());
        props
    }

    fn get_property(&self, name: &str) -> Option<::atto_ui::ComponentValue> {
        self.__component_get_property(name)
            .or_else(|| self.inner.get_property(name))
    }

    fn set_property(
        &mut self,
        name: &str,
        value: ::atto_ui::ComponentValue,
    ) -> Result<(), ::atto_ui::ComponentError> {
        if self.__component_set_property(name, value.clone()).is_ok() {
            return Ok(());
        }
        self.inner.set_property(name, value)
    }

    fn apply_command(&mut self, action: ::atto_ui::ComponentCommand) -> EventResult {
        self.inner.apply_command(action)
    }

    fn titlebar(&mut self, ctx: TitleBarContext<'_>) -> Option<TitleBarContent> {
        self.inner.titlebar(ctx)
    }

    fn handle_titlebar_event(&mut self, event: &Event, ctx: TitleBarContext<'_>) -> EventResult {
        self.inner.handle_titlebar_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
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

impl crate::composable::DragAndDrop for WindowMinSizeView {
    fn drag_source_at(
        &self,
        screen_x: u16,
        screen_y: u16,
        ctx: ComponentContext<'_>,
    ) -> Option<DragSource> {
        self.inner.drag_source_at(screen_x, screen_y, ctx)
    }

    fn drag_over(&mut self, offer: DragOffer<'_>, ctx: ComponentContext<'_>) -> DropFeedback {
        self.inner.drag_over(offer, ctx)
    }

    fn drop(&mut self, offer: DragOffer<'_>, ctx: ComponentContext<'_>) -> EventResult {
        crate::composable::DragAndDrop::drop(self.inner.as_mut(), offer, ctx)
    }

    fn drag_cancelled(&mut self, ctx: ComponentContext<'_>) {
        self.inner.drag_cancelled(ctx);
    }
}

impl Layout for WindowMinSizeView {
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
}

impl Scrollable for WindowMinSizeView {
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

    fn scroll_to_child(&mut self, child_id: ComponentId) {
        self.inner.scroll_to_child(child_id);
    }
}

impl FocusNav for WindowMinSizeView {
    fn focused_child(&self) -> Option<ComponentId> {
        self.inner.focused_child()
    }

    fn is_focusable(&self) -> bool {
        self.inner.is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        self.inner.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.inner.focus_last()
    }
}

impl DynamicTree for WindowMinSizeView {
    fn tag(&self) -> Option<&str> {
        self.inner.tag()
    }

    fn children(&self) -> &[ComponentNode] {
        self.inner.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        self.inner.children_mut()
    }

    fn apply_tree_ops(&mut self, ops: &[TreeOp]) -> Result<bool, TreeError> {
        self.inner.apply_tree_ops(ops)
    }

    fn rebuild_tree(&mut self) -> Result<(), TreeError> {
        self.inner.rebuild_tree()
    }

    fn dynamic_root_spec(&self) -> Option<&ComponentSpec> {
        self.inner.dynamic_root_spec()
    }

    fn dynamic_callbacks(&self) -> Option<&CallbackRegistry> {
        self.inner.dynamic_callbacks()
    }
}

impl EventHandling for WindowMinSizeView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
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

                if let Event::Mouse(m) = event
                    && mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space).is_none()
                {
                    return EventResult::ignored();
                }

                let Some(new_scroll) = scroll_offset_for_input_event(
                    self.scroll_config,
                    self.content_size,
                    self.viewport_size,
                    self.scroll,
                    event,
                ) else {
                    return EventResult::ignored();
                };

                if new_scroll == self.scroll {
                    EventResult::ignored()
                } else {
                    self.scroll = new_scroll;
                    EventResult::consumed()
                }
            }
            Some(WindowMinSizeMode::Enforce) | None => self.inner.handle_event(event, ctx),
        }
    }
}
