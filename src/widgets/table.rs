use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use parking_lot::RwLock;
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use crate::ComponentCommand;
use crate::composable::scroll::{
    ScrollbarDrag, Scrollbars, draw_scrollbars, handle_scrollbar_mouse_event,
};
use crate::composable::{
    Component, ComponentContext, EdgeInsets, EventHandling, EventResult, FocusNav, Layout,
    ScrollConfig, ScrollContainer, ScrollContainerHost, ScrollContent, ScrollContentContext,
    ScrollOffset, Scrollable, ScrollbarHost, ScrollbarVisibility, should_show_scrollbar,
};
use crate::reactive::Binding;
use crate::runtime::CallbackHandle;
use crate::text::styled_text::spans_from_inline;
use atto_ui_macros::{ComponentProperties, component_properties};

#[derive(Clone, Debug, ComponentProperties)]
struct TableViewBindings {
    title: Binding<String>,
    headers: Binding<Vec<String>>,
    rows: Binding<Vec<Vec<String>>>,
    enabled: Binding<bool>,
    selection: Binding<usize>,
    height: Binding<u16>,
    #[component(skip)]
    on_change: Option<CallbackHandle>,
}

#[derive(ComponentProperties)]
pub struct TableView {
    #[component(delegate)]
    bindings: Arc<RwLock<TableViewBindings>>,
    scroll: ScrollContainer,
    scrollbar_drag: Option<ScrollbarDrag>,
    last_area: Option<Rect>,
    min_size: (u16, u16),
}

impl Clone for TableView {
    fn clone(&self) -> Self {
        let bindings = self.bindings.clone();
        Self {
            scroll: build_scroll_container(bindings.clone()),
            bindings,
            scrollbar_drag: None,
            last_area: None,
            min_size: self.min_size,
        }
    }
}

impl std::fmt::Debug for TableView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bindings = self.bindings.read();
        f.debug_struct("TableView")
            .field("title", &bindings.title.get())
            .field("enabled", &bindings.enabled.get())
            .field("height", &bindings.height.get())
            .field("min_size", &self.min_size)
            .finish()
    }
}

impl TableView {
    pub fn new(
        title: impl Into<Binding<String>>,
        headers: impl Into<Binding<Vec<String>>>,
        rows: impl Into<Binding<Vec<Vec<String>>>>,
        selection: Binding<usize>,
    ) -> Self {
        let rows = rows.into();
        let row_count = rows.get().len();
        if row_count > 0 {
            let selected = selection.get().min(row_count.saturating_sub(1));
            selection.set(selected);
        }
        let bindings = Arc::new(RwLock::new(TableViewBindings {
            title: title.into(),
            headers: headers.into(),
            rows,
            enabled: true.into(),
            selection,
            height: 8.into(),
            on_change: None,
        }));
        Self {
            scroll: build_scroll_container(bindings.clone()),
            bindings,
            scrollbar_drag: None,
            last_area: None,
            min_size: (3, 4),
        }
    }

    pub fn title(self, title: impl Into<Binding<String>>) -> Self {
        self.bindings.write().title = title.into();
        self
    }

    pub fn headers(self, headers: impl Into<Binding<Vec<String>>>) -> Self {
        self.bindings.write().headers = headers.into();
        self
    }

    pub fn on_change_callback(self, callback: CallbackHandle) -> Self {
        self.bindings.write().on_change = Some(callback);
        self
    }

    pub fn rows(self, rows: impl Into<Binding<Vec<Vec<String>>>>) -> Self {
        {
            let mut bindings = self.bindings.write();
            bindings.rows = rows.into();
            let row_count = bindings.rows.get().len();
            if row_count > 0 {
                let selected = bindings.selection.get().min(row_count.saturating_sub(1));
                bindings.selection.set(selected);
            }
        }
        self
    }

    pub fn enabled(self, enabled: impl Into<Binding<bool>>) -> Self {
        self.bindings.write().enabled = enabled.into();
        self
    }

    pub fn height(self, height: impl Into<Binding<u16>>) -> Self {
        self.bindings.write().height = height.into();
        self
    }

    pub fn with_min_width(mut self, width: u16) -> Self {
        self.min_size.0 = width;
        self
    }

    pub fn with_min_height(mut self, height: u16) -> Self {
        self.min_size.1 = height;
        self
    }

    pub fn with_min_size(mut self, width: u16, height: u16) -> Self {
        self.min_size = (width, height);
        self
    }
}

#[component_properties]
impl Component for TableView {
    fn apply_command(&mut self, command: ComponentCommand) -> EventResult {
        match command {
            ComponentCommand::SelectIndex(idx) => {
                let bindings = self.bindings.write();
                let row_count = bindings.rows.get().len();
                if row_count > 0 {
                    bindings.selection.set(idx.min(row_count.saturating_sub(1)));
                    EventResult::changed()
                } else {
                    EventResult::ignored()
                }
            }
            _ => EventResult::ignored(),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        let bindings = self.bindings.read();
        let enabled = bindings.enabled.get();
        let base_style: Style = if !enabled {
            ctx.theme.widget.disabled
        } else if ctx.is_focused {
            ctx.theme.widget.focused
        } else {
            ctx.theme.widget.normal
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(ctx.theme.border_set(false))
            .title(bindings.title.get())
            .style(base_style);
        frame.render_widget(block, area);

        let inner = Rect {
            x: area.x.saturating_add(1),
            y: area.y.saturating_add(1),
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let headers = bindings.headers.get();
        let rows = bindings.rows.get();
        let column_count = column_count(&headers, &rows);
        let widths = column_constraints(column_count);
        let header_height = if headers.is_empty() { 0 } else { 1 };
        let link_overlay = ctx.theme.named_style("markdown-link");

        if header_height > 0 {
            let header_style = if enabled {
                ctx.theme.widget.accent
            } else {
                ctx.theme.widget.disabled
            };
            let header_area = Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: 1.min(inner.height),
            };
            let header_cells = row_cells(&headers, column_count, header_style, link_overlay);
            let header = Row::new(header_cells).style(header_style);
            let header_table = Table::new(Vec::<Row>::new(), widths.clone())
                .header(header)
                .column_spacing(1)
                .style(base_style);
            frame.render_widget(header_table, header_area);
        }
        drop(bindings);

        let body_area = Rect {
            x: inner.x,
            y: inner.y.saturating_add(header_height),
            width: inner.width,
            height: inner.height.saturating_sub(header_height),
        };
        if body_area.width == 0 || body_area.height == 0 {
            return;
        }

        let body_ctx = ComponentContext {
            scrollbar_host: ScrollbarHost::Window,
            ..ctx
        };
        self.scroll.draw(frame, body_area, body_ctx);

        self.draw_border_scrollbar(frame, area, body_area, header_height, ctx);
    }
}

impl Layout for TableView {
    fn min_width(&self) -> u16 {
        self.min_size.0
    }

    fn min_height(&self) -> u16 {
        self.min_size.1
    }

    fn desired_height(&self) -> Option<u16> {
        let height = self.bindings.read().height.get();
        Some(height.max(self.min_size.1))
    }
}

impl FocusNav for TableView {
    fn is_focusable(&self) -> bool {
        self.bindings.read().enabled.get()
    }
}

impl EventHandling for TableView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.bindings.read().enabled.get() {
            return EventResult::ignored();
        }
        if let Event::Mouse(m) = event
            && let Some(area) = self.last_area
            && let Some((_, body_area, header_height)) = self.layout_areas(area)
        {
            let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
                return EventResult::ignored();
            };
            let abs_event = MouseEvent {
                column: area.x.saturating_add(local_x),
                row: area.y.saturating_add(local_y),
                ..*m
            };
            if let Some(new_scroll) =
                self.handle_border_scrollbar_event(abs_event, area, body_area, header_height)
            {
                self.scroll.set_scroll_offset(new_scroll.x, new_scroll.y);
                return EventResult::consumed();
            }

            let body_local = Rect {
                x: body_area.x.saturating_sub(area.x),
                y: body_area.y.saturating_sub(area.y),
                width: body_area.width,
                height: body_area.height,
            };
            if !contains(body_local, local_x, local_y) {
                return EventResult::ignored();
            }
        }

        let body_ctx = ComponentContext {
            scrollbar_host: ScrollbarHost::Window,
            ..ctx
        };
        self.scroll.handle_event(event, body_ctx)
    }
}

crate::impl_component_default_traits!(TableView => Scrollable, DynamicTree);

impl TableView {
    fn layout_areas(&self, area: Rect) -> Option<(Rect, Rect, u16)> {
        let inner = Rect {
            x: area.x.saturating_add(1),
            y: area.y.saturating_add(1),
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        if inner.width == 0 || inner.height == 0 {
            return None;
        }
        let headers = self.bindings.read().headers.get();
        let header_height = if headers.is_empty() { 0 } else { 1 };
        let body_area = Rect {
            x: inner.x,
            y: inner.y.saturating_add(header_height),
            width: inner.width,
            height: inner.height.saturating_sub(header_height),
        };
        Some((inner, body_area, header_height))
    }

    fn border_scrollbars(
        &self,
        area: Rect,
        body_area: Rect,
        header_height: u16,
    ) -> Option<Scrollbars> {
        if body_area.width == 0 || body_area.height == 0 || area.width < 2 || area.height < 2 {
            return None;
        }
        let cfg = self.scroll.scroll_config();
        let content_size = self.scroll.content_size();
        let viewport_size = self.scroll.viewport_size();
        let show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_size.1, viewport_size.1);
        let show_h =
            should_show_scrollbar(cfg.horizontal_scrollbar, content_size.0, viewport_size.0);
        if !show_v && !show_h {
            return None;
        }
        let body_local = Rect {
            x: body_area.x.saturating_sub(area.x),
            y: body_area.y.saturating_sub(area.y),
            width: body_area.width,
            height: body_area.height,
        };
        let vbar = show_v.then_some(Rect {
            x: area.width.saturating_sub(1),
            y: if header_height == 0 { 0 } else { body_local.y },
            width: 1,
            height: if header_height == 0 {
                area.height
            } else {
                body_local.height
            },
        });
        let hbar = show_h.then_some(Rect {
            x: body_local.x,
            y: area.height.saturating_sub(1),
            width: body_local.width,
            height: 1,
        });
        Some(Scrollbars {
            viewport: body_local,
            content: body_local,
            vbar,
            hbar,
            thickness: 1,
        })
    }

    fn draw_border_scrollbar(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        body_area: Rect,
        header_height: u16,
        ctx: ComponentContext<'_>,
    ) {
        let Some(scrollbars) = self.border_scrollbars(area, body_area, header_height) else {
            self.scrollbar_drag = None;
            return;
        };
        let cfg = self.scroll.scroll_config();
        let content_size = self.scroll.content_size();
        let viewport_size = self.scroll.viewport_size();
        let scroll = self.scroll.scroll_offset();
        draw_scrollbars(
            frame,
            area,
            scrollbars,
            viewport_size,
            content_size,
            ScrollOffset {
                x: scroll.0,
                y: scroll.1,
            },
            cfg,
            ctx.theme,
        );
    }

    fn handle_border_scrollbar_event(
        &mut self,
        m: crossterm::event::MouseEvent,
        area: Rect,
        body_area: Rect,
        header_height: u16,
    ) -> Option<ScrollOffset> {
        let Some(scrollbars) = self.border_scrollbars(area, body_area, header_height) else {
            self.scrollbar_drag = None;
            return None;
        };
        let local_x = m.column.saturating_sub(area.x);
        let local_y = m.row.saturating_sub(area.y);
        let cfg = self.scroll.scroll_config();
        let content_size = self.scroll.content_size();
        let scroll = self.scroll.scroll_offset();
        handle_scrollbar_mouse_event(
            cfg,
            scrollbars,
            content_size,
            ScrollOffset {
                x: scroll.0,
                y: scroll.1,
            },
            &mut self.scrollbar_drag,
            local_x,
            local_y,
            m.kind,
        )
    }
}

struct TableBodyContent {
    bindings: Arc<RwLock<TableViewBindings>>,
    state: TableState,
    last_selection: Option<usize>,
}

impl TableBodyContent {
    fn new(bindings: Arc<RwLock<TableViewBindings>>) -> Self {
        Self {
            bindings,
            state: TableState::default(),
            last_selection: None,
        }
    }

    fn bindings(&self) -> TableViewBindings {
        self.bindings.read().clone()
    }

    fn normalize_selection(&mut self, row_count: usize) -> Option<usize> {
        if row_count == 0 {
            return None;
        }
        let bindings = self.bindings();
        let mut selection = bindings.selection.get();
        if selection >= row_count {
            selection = row_count.saturating_sub(1);
            bindings.selection.set(selection);
        }
        Some(selection)
    }

    fn ensure_selection_visible(&mut self, selection: usize, host: &mut ScrollContainerHost) {
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
}

impl ScrollContent for TableBodyContent {
    fn is_focusable(&self) -> bool {
        self.bindings().enabled.get()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.bindings().height.get())
    }

    fn content_size(&mut self, viewport: (u16, u16), _ctx: ScrollContentContext<'_>) -> (u16, u16) {
        let rows = self.bindings().rows.get();
        let height = rows.len().min(u16::MAX as usize) as u16;
        (viewport.0, height)
    }

    fn on_scrollbars(&mut self, _ctx: ScrollContentContext<'_>, host: &mut ScrollContainerHost) {
        let rows = self.bindings().rows.get();
        let selection = self.normalize_selection(rows.len());
        if selection != self.last_selection {
            if let Some(sel) = selection {
                self.ensure_selection_visible(sel, host);
            }
            self.last_selection = selection;
        }
    }

    fn handle_event(
        &mut self,
        event: &Event,
        _ctx: ScrollContentContext<'_>,
        host: &mut ScrollContainerHost,
    ) -> EventResult {
        let bindings = self.bindings();
        if !bindings.enabled.get() {
            return EventResult::ignored();
        }
        let rows = bindings.rows.get();
        let Some(selection) = self.normalize_selection(rows.len()) else {
            return EventResult::ignored();
        };

        match event {
            Event::Mouse(m) => {
                if m.kind != MouseEventKind::Down(MouseButton::Left) {
                    return EventResult::ignored();
                }
                let row = m.row as usize;
                let idx = host.scroll_offset().y as usize + row;
                if idx < rows.len() {
                    bindings.selection.set(idx);
                    self.ensure_selection_visible(idx, host);
                    self.last_selection = Some(idx);
                    if let Some(cb) = &bindings.on_change {
                        cb.emit();
                    }
                    return EventResult::changed();
                }
                EventResult::ignored()
            }
            Event::Key(KeyEvent { code, .. }) => match code {
                KeyCode::Up => {
                    let next = if selection == 0 {
                        rows.len() - 1
                    } else {
                        selection.saturating_sub(1)
                    };
                    bindings.selection.set(next);
                    self.ensure_selection_visible(next, host);
                    self.last_selection = Some(next);
                    if let Some(cb) = &bindings.on_change {
                        cb.emit();
                    }
                    EventResult::changed()
                }
                KeyCode::Down => {
                    let next = (selection + 1) % rows.len();
                    bindings.selection.set(next);
                    self.ensure_selection_visible(next, host);
                    self.last_selection = Some(next);
                    if let Some(cb) = &bindings.on_change {
                        cb.emit();
                    }
                    EventResult::changed()
                }
                _ => EventResult::ignored(),
            },
            _ => EventResult::ignored(),
        }
    }

    fn draw(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ScrollContentContext<'_>,
        _host: &mut ScrollContainerHost,
    ) {
        let bindings = self.bindings();
        let enabled = bindings.enabled.get();
        let base_style: Style = if !enabled {
            ctx.component.theme.widget.disabled
        } else if ctx.component.is_focused {
            ctx.component.theme.widget.focused
        } else {
            ctx.component.theme.widget.normal
        };
        let highlight_style = if enabled {
            ctx.component.theme.selection
        } else {
            ctx.component
                .theme
                .selection
                .patch(ctx.component.theme.widget.disabled)
        };

        let headers = bindings.headers.get();
        let rows = bindings.rows.get();
        let link_overlay = ctx.component.theme.named_style("markdown-link");
        let selection = self.normalize_selection(rows.len());
        let column_count = column_count(&headers, &rows);
        let widths = column_constraints(column_count);

        let data_rows = rows.iter().enumerate().map(|(idx, row)| {
            let cells = row_cells(row, column_count, base_style, link_overlay);
            let row = Row::new(cells);
            if selection.is_some_and(|sel| sel == idx) {
                row.style(highlight_style)
            } else {
                row
            }
        });

        *self.state.selected_mut() = None;
        *self.state.selected_column_mut() = None;
        *self.state.offset_mut() = ctx.info.scroll_offset.y as usize;

        if area.width > 0 && area.height > 0 {
            let table = Table::new(data_rows, widths)
                .column_spacing(1)
                .style(base_style);
            frame.render_stateful_widget(table, area, &mut self.state);
        }
    }
}

fn build_scroll_container(bindings: Arc<RwLock<TableViewBindings>>) -> ScrollContainer {
    ScrollContainer::new(Box::new(TableBodyContent::new(bindings)))
        .with_padding(EdgeInsets::ZERO)
        .with_scroll_config(
            ScrollConfig::default().horizontal_scrollbar(ScrollbarVisibility::Never),
        )
}

fn column_count(headers: &[String], rows: &[Vec<String>]) -> usize {
    let mut count = headers.len();
    for row in rows {
        count = count.max(row.len());
    }
    count.max(1)
}

fn column_constraints(column_count: usize) -> Vec<Constraint> {
    let pct = (100 / column_count.max(1)) as u16;
    (0..column_count)
        .map(|_| Constraint::Percentage(pct.max(1)))
        .collect()
}

fn row_cells(
    row: &[String],
    column_count: usize,
    base_style: Style,
    link_overlay: Option<Style>,
) -> Vec<Cell<'static>> {
    (0..column_count)
        .map(|idx| row.get(idx).cloned().unwrap_or_default())
        .map(|text| styled_cell(&text, base_style, link_overlay))
        .collect()
}

fn styled_cell(text: &str, base_style: Style, link_overlay: Option<Style>) -> Cell<'static> {
    let spans = spans_from_inline(text, base_style, link_overlay);
    Cell::from(Line::from(spans))
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

    // Nested containers receive mouse coordinates already relative to their own origin.
    if m.column < area.width && m.row < area.height {
        return Some((m.column, m.row));
    }

    None
}
