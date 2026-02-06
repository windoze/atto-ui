use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use unicode_width::UnicodeWidthStr;

use crate::composable::scroll::{
    ScrollbarDrag, ScrollbarHit, scroll_offset_from_thumb_start, scrollbar_hit_test,
    scrollbar_layout_1d, should_show_scrollbar,
};
use crate::composable::{Component, ComponentContext, EventResult, ScrollConfig, ScrollbarHost};
use crate::reactive::Binding;

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

#[derive(Clone, Debug)]
pub struct ListBox {
    title: Binding<String>,
    items: Binding<Vec<String>>,
    state: ListState,
    enabled: Binding<bool>,
    selection: Binding<usize>,
    height: Binding<u16>,
    last_area: Option<Rect>,
    min_size: (u16, u16),
    scroll_config: ScrollConfig,
    viewport_size: (u16, u16),
    content_size: (u16, u16),
    scrollbar_drag: Option<ScrollbarDrag>,
}

impl ListBox {
    pub fn new(
        title: impl Into<Binding<String>>,
        items: impl Into<Binding<Vec<String>>>,
        selection: Binding<usize>,
    ) -> Self {
        let mut state = ListState::default();
        let items = items.into();
        let items_len = items.get().len();
        if items_len > 0 {
            let selected = selection.get().min(items_len.saturating_sub(1));
            selection.set(selected);
            state.select(Some(selected));
        }
        Self {
            title: title.into(),
            items,
            state,
            enabled: true.into(),
            selection,
            height: 7.into(),
            last_area: None,
            min_size: (3, 3), // Minimum size to render borders and one item.
            scroll_config: ScrollConfig::default()
                .horizontal_scrollbar(crate::composable::ScrollbarVisibility::Never),
            viewport_size: (0, 0),
            content_size: (0, 0),
            scrollbar_drag: None,
        }
    }

    pub fn title(mut self, title: impl Into<Binding<String>>) -> Self {
        self.title = title.into();
        self
    }

    pub fn items(mut self, items: impl Into<Binding<Vec<String>>>) -> Self {
        self.items = items.into();
        self
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    pub fn height(mut self, height: impl Into<Binding<u16>>) -> Self {
        self.height = height.into();
        self
    }

    pub fn scroll_config(mut self, config: ScrollConfig) -> Self {
        self.scroll_config = config;
        self
    }

    pub fn selected(&self) -> Option<usize> {
        self.state.selected().or_else(|| {
            let items = self.items.get();
            (!items.is_empty() && self.selection.get() < items.len())
                .then_some(self.selection.get())
        })
    }

    pub fn with_min_height(mut self, height: u16) -> Self {
        self.min_size.1 = height;
        self
    }

    pub fn with_min_width(mut self, width: u16) -> Self {
        self.min_size.0 = width;
        self
    }

    pub fn with_min_size(mut self, width: u16, height: u16) -> Self {
        self.min_size = (width, height);
        self
    }
}

impl Component for ListBox {
    fn min_width(&self) -> u16 {
        self.min_size.0
    }

    fn min_height(&self) -> u16 {
        self.min_size.1
    }

    fn is_focusable(&self) -> bool {
        self.enabled.get()
    }

    fn is_scrollable(&self) -> bool {
        true
    }

    fn content_size(&self) -> (u16, u16) {
        self.content_size
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.viewport_size
    }

    fn scroll_offset(&self) -> (u16, u16) {
        let y = self.state.offset().min(u16::MAX as usize) as u16;
        (0, y)
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.scroll_config
    }

    fn set_scroll_offset(&mut self, _x: u16, y: u16) {
        let items = self.items.get();
        if items.is_empty() {
            return;
        }

        let viewport_h = self.viewport_size.1.max(1) as usize;
        let content_h = items.len();
        let max_off = content_h.saturating_sub(viewport_h);
        let desired_off = (y as usize).min(max_off);
        *self.state.offset_mut() = desired_off;

        let mut selected = self
            .state
            .selected()
            .unwrap_or_else(|| self.selection.get());
        if selected >= content_h {
            selected = content_h.saturating_sub(1);
        }

        if selected < desired_off {
            selected = desired_off;
        } else if selected >= desired_off.saturating_add(viewport_h) {
            selected = desired_off.saturating_add(viewport_h.saturating_sub(1));
        }

        self.state.select(Some(selected));
        self.selection.set(selected);
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }
        let items = self.items.get();
        if items.is_empty() {
            return EventResult::ignored();
        }
        // Sync from external selection.
        let ext = self.selection.get();
        if ext < items.len() {
            self.state.select(Some(ext));
        }
        let sel = self.state.selected().unwrap_or(0);
        match event {
            Event::Mouse(m) => {
                let Some(area) = self.last_area else {
                    return EventResult::ignored();
                };
                let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
                    return EventResult::ignored();
                };

                let inner = Rect {
                    x: 1,
                    y: 1,
                    width: area.width.saturating_sub(2),
                    height: area.height.saturating_sub(2),
                };
                if inner.width == 0 || inner.height == 0 {
                    return EventResult::ignored();
                }

                let cfg = self.scroll_config;
                let thickness = cfg.scrollbar_thickness.max(1);
                let viewport_h = inner.height;
                let content_h = items.len().min(u16::MAX as usize) as u16;
                let show_v = matches!(ctx.scrollbar_host, ScrollbarHost::Component)
                    && should_show_scrollbar(cfg.vertical_scrollbar, content_h, viewport_h);
                let content = if show_v {
                    Rect {
                        x: inner.x,
                        y: inner.y,
                        width: inner.width.saturating_sub(thickness),
                        height: inner.height,
                    }
                } else {
                    inner
                };
                let vbar = show_v.then(|| Rect {
                    x: content.x.saturating_add(content.width),
                    y: inner.y,
                    width: thickness.min(inner.width),
                    height: inner.height,
                });

                if let Some(drag) = self.scrollbar_drag {
                    match m.kind {
                        MouseEventKind::Drag(MouseButton::Left) => {
                            let Some(vbar) = vbar else {
                                self.scrollbar_drag = None;
                                return EventResult::consumed();
                            };
                            if vbar.height == 0 {
                                return EventResult::consumed();
                            }

                            let layout = scrollbar_layout_1d(
                                vbar.height,
                                viewport_h,
                                content_h,
                                self.scroll_offset().1,
                                cfg.arrows,
                            );
                            if layout.track_len == 0 {
                                return EventResult::consumed();
                            }

                            let pos = local_y
                                .saturating_sub(vbar.y)
                                .min(vbar.height.saturating_sub(1));
                            let pos_in_track = pos
                                .saturating_sub(layout.track_start)
                                .min(layout.track_len.saturating_sub(1));

                            let ScrollbarDrag::Vertical { grab_offset } = drag else {
                                self.scrollbar_drag = None;
                                return EventResult::consumed();
                            };

                            let max_start = layout.track_len.saturating_sub(layout.thumb_len);
                            let new_thumb_start =
                                pos_in_track.saturating_sub(grab_offset).min(max_start);
                            let new_off = scroll_offset_from_thumb_start(
                                layout.track_len,
                                viewport_h,
                                content_h,
                                new_thumb_start,
                            );
                            self.set_scroll_offset(0, new_off);
                            return EventResult::consumed();
                        }
                        MouseEventKind::Up(MouseButton::Left) => {
                            self.scrollbar_drag = None;
                            return EventResult::consumed();
                        }
                        _ => {}
                    }
                }

                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if let Some(vbar) = vbar
                            && contains(vbar, local_x, local_y)
                            && vbar.height > 0
                        {
                            let pos = local_y.saturating_sub(vbar.y);
                            let layout = scrollbar_layout_1d(
                                vbar.height,
                                viewport_h,
                                content_h,
                                self.scroll_offset().1,
                                cfg.arrows,
                            );
                            match scrollbar_hit_test(layout, pos) {
                                ScrollbarHit::ArrowDec => {
                                    let next = sel.saturating_sub(1);
                                    self.state.select(Some(next));
                                    self.selection.set(next);
                                    return EventResult::changed();
                                }
                                ScrollbarHit::ArrowInc => {
                                    let next = (sel + 1).min(items.len().saturating_sub(1));
                                    self.state.select(Some(next));
                                    self.selection.set(next);
                                    return EventResult::changed();
                                }
                                ScrollbarHit::TrackDec => {
                                    let page = viewport_h.max(1) as usize;
                                    let next = sel.saturating_sub(page);
                                    self.state.select(Some(next));
                                    self.selection.set(next);
                                    return EventResult::changed();
                                }
                                ScrollbarHit::TrackInc => {
                                    let page = viewport_h.max(1) as usize;
                                    let next = (sel + page).min(items.len().saturating_sub(1));
                                    self.state.select(Some(next));
                                    self.selection.set(next);
                                    return EventResult::changed();
                                }
                                ScrollbarHit::Thumb { grab_offset } => {
                                    self.scrollbar_drag =
                                        Some(ScrollbarDrag::Vertical { grab_offset });
                                    return EventResult::consumed();
                                }
                                ScrollbarHit::None => {}
                            }
                        }

                        if !contains(content, local_x, local_y) {
                            return EventResult::ignored();
                        }

                        let row = local_y.saturating_sub(content.y) as usize;
                        let idx = self.state.offset().saturating_add(row);
                        if idx < items.len() {
                            self.state.select(Some(idx));
                            self.selection.set(idx);
                            return EventResult::changed();
                        }
                        EventResult::ignored()
                    }
                    MouseEventKind::ScrollUp => {
                        if !contains(inner, local_x, local_y) {
                            return EventResult::ignored();
                        }
                        let step = cfg.wheel_step.max(1) as usize;
                        let next = sel.saturating_sub(step);
                        self.state.select(Some(next));
                        self.selection.set(next);
                        EventResult::changed()
                    }
                    MouseEventKind::ScrollDown => {
                        if !contains(inner, local_x, local_y) {
                            return EventResult::ignored();
                        }
                        let step = cfg.wheel_step.max(1) as usize;
                        let next = (sel + step).min(items.len().saturating_sub(1));
                        self.state.select(Some(next));
                        self.selection.set(next);
                        EventResult::changed()
                    }
                    _ => EventResult::ignored(),
                }
            }
            Event::Key(KeyEvent { code, .. }) => match code {
                KeyCode::Up => {
                    let next = if sel == 0 {
                        items.len() - 1
                    } else {
                        sel.saturating_sub(1)
                    };
                    self.state.select(Some(next));
                    self.selection.set(next);
                    EventResult::changed()
                }
                KeyCode::Down => {
                    let next = (sel + 1) % items.len();
                    self.state.select(Some(next));
                    self.selection.set(next);
                    EventResult::changed()
                }
                KeyCode::PageUp => {
                    let page = self.viewport_size.1.max(1) as usize;
                    let next = sel.saturating_sub(page);
                    self.state.select(Some(next));
                    self.selection.set(next);
                    EventResult::changed()
                }
                KeyCode::PageDown => {
                    let page = self.viewport_size.1.max(1) as usize;
                    let next = (sel + page).min(items.len().saturating_sub(1));
                    self.state.select(Some(next));
                    self.selection.set(next);
                    EventResult::changed()
                }
                KeyCode::Home => {
                    self.state.select(Some(0));
                    self.selection.set(0);
                    EventResult::changed()
                }
                KeyCode::End => {
                    let last = items.len().saturating_sub(1);
                    self.state.select(Some(last));
                    self.selection.set(last);
                    EventResult::changed()
                }
                _ => EventResult::ignored(),
            },
            _ => EventResult::ignored(),
        }
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.height.get().max(self.min_size.1))
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        let items = self.items.get();
        if !items.is_empty() {
            let ext = self.selection.get();
            if ext < items.len() {
                self.state.select(Some(ext));
            } else {
                self.state.select(Some(0));
                self.selection.set(0);
            }
        }
        let enabled = self.enabled.get();
        let style = if !enabled {
            ctx.theme.widget.disabled
        } else if ctx.is_focused {
            ctx.theme.widget.focused
        } else {
            ctx.theme.widget.normal
        };
        let highlight_style = if enabled {
            ctx.theme.selection
        } else {
            ctx.theme.selection.patch(ctx.theme.widget.disabled)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(ctx.theme.border_set(false))
            .title(self.title.get());
        frame.render_widget(block.border_style(style), area);

        let inner = Rect {
            x: area.x.saturating_add(1),
            y: area.y.saturating_add(1),
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        if inner.width == 0 || inner.height == 0 {
            self.viewport_size = (0, 0);
            self.content_size = (0, 0);
            return;
        }

        let cfg = self.scroll_config;
        let thickness = cfg.scrollbar_thickness.max(1);
        let content_h = items.len().min(u16::MAX as usize) as u16;
        let show_v = matches!(ctx.scrollbar_host, ScrollbarHost::Component)
            && should_show_scrollbar(cfg.vertical_scrollbar, content_h, inner.height);
        let content = if show_v {
            Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width.saturating_sub(thickness),
                height: inner.height,
            }
        } else {
            inner
        };

        let max_item_w = items
            .iter()
            .map(|s| UnicodeWidthStr::width(s.as_str()).min(u16::MAX as usize) as u16)
            .max()
            .unwrap_or(0);
        self.viewport_size = (content.width, content.height);
        self.content_size = (max_item_w, content_h);

        if content.width == 0 || content.height == 0 {
            return;
        }

        let items: Vec<ListItem> = items
            .iter()
            .map(|s| ListItem::new(Line::raw(s.clone())))
            .collect();
        let list = List::new(items)
            .highlight_style(highlight_style)
            .style(style);
        frame.render_stateful_widget(list, content, &mut self.state);

        if show_v {
            let vbar = Rect {
                x: content.x.saturating_add(content.width),
                y: inner.y,
                width: thickness.min(inner.width),
                height: inner.height,
            };
            self.draw_vscrollbar(frame, vbar, ctx, content_h);
        }
    }
}

impl ListBox {
    fn draw_vscrollbar(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ComponentContext<'_>,
        content_h: u16,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let cfg = self.scroll_config;
        let viewport_h = self.viewport_size.1;
        let scroll_y = self.scroll_offset().1;

        let layout = scrollbar_layout_1d(area.height, viewport_h, content_h, scroll_y, cfg.arrows);

        let track_style = ctx.theme.scrollbar_track;
        let thumb_style = ctx.theme.scrollbar_thumb;
        let arrow_style = ctx.theme.scrollbar_arrow;
        let buf = frame.buffer_mut();

        let track = ctx.theme.glyph("scrollbar-track").unwrap_or("░");
        let thumb = ctx.theme.glyph("scrollbar-thumb").unwrap_or("█");
        let arrow_up = ctx.theme.glyph("scrollbar-up-arrow").unwrap_or("▲");
        let arrow_down = ctx.theme.glyph("scrollbar-down-arrow").unwrap_or("▼");

        for dy in 0..area.height {
            let (symbol, style) = if layout.has_arrows && dy == 0 {
                (arrow_up, arrow_style)
            } else if layout.has_arrows && dy == layout.bar_len.saturating_sub(1) {
                (arrow_down, arrow_style)
            } else if dy >= layout.thumb_start
                && dy < layout.thumb_start.saturating_add(layout.thumb_len)
            {
                (thumb, thumb_style)
            } else {
                (track, track_style)
            };
            for dx in 0..area.width {
                buf[(area.x.saturating_add(dx), area.y.saturating_add(dy))]
                    .set_symbol(symbol)
                    .set_style(style);
            }
        }
    }
}
