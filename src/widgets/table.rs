use std::sync::Arc;

use crossterm::event::{Event, MouseEvent};
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
    MouseCoordinateSpace, ScrollConfig, ScrollContainer, ScrollContainerHost, ScrollContent,
    ScrollContentContext, ScrollOffset, Scrollable, ScrollbarHost, ScrollbarVisibility,
    should_show_scrollbar,
};
use crate::reactive::Binding;
use crate::runtime::CallbackHandle;
use crate::text::styled_text::spans_from_inline;
use atto_ui_macros::{ComponentProperties, component_properties};

use super::util::{
    NamedStyleCache, SelectionScroll, border_scrollbar_axis_span, contains,
    mouse_coords_local_to_area, visible_row_range, widget_style,
};

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
    markdown_link_style: NamedStyleCache,
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
            markdown_link_style: NamedStyleCache::default(),
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
            markdown_link_style: NamedStyleCache::default(),
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
        let base_style: Style = widget_style(ctx.theme, enabled, ctx.is_focused);
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
        let link_overlay = self.markdown_link_style.markdown_link(ctx.theme);

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
            drag: None,
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
            let Some((local_x, local_y)) =
                mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
            else {
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

            // The scroll body is inset by the border and header, so re-express the
            // click relative to `body_area` and hand it over as Local coordinates.
            // (ListBox can delegate the raw event because its scroll viewport is the
            // whole area; the table's viewport is the smaller body, so a raw event
            // would be offset by the border + header — breaking clicks when the
            // table is nested and the incoming coordinates are already Local.)
            let body_event = Event::Mouse(MouseEvent {
                column: local_x.saturating_sub(body_local.x),
                row: local_y.saturating_sub(body_local.y),
                ..*m
            });
            let body_ctx = ComponentContext {
                scrollbar_host: ScrollbarHost::Window,
                drag: None,
                mouse_coordinate_space: MouseCoordinateSpace::Local,
                ..ctx
            };
            return self.scroll.handle_event(&body_event, body_ctx);
        }

        let body_ctx = ComponentContext {
            scrollbar_host: ScrollbarHost::Window,
            drag: None,
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
        let (vbar_y, vbar_height) = if header_height == 0 {
            (0, area.height)
        } else {
            border_scrollbar_axis_span(area.height, body_local.y, body_local.height, cfg.arrows)
        };
        let vbar = show_v.then_some(Rect {
            x: area.width.saturating_sub(1),
            y: vbar_y,
            width: 1,
            height: vbar_height,
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
        m: MouseEvent,
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
    selection_scroll: SelectionScroll,
    markdown_link_style: NamedStyleCache,
}

impl TableBodyContent {
    fn new(bindings: Arc<RwLock<TableViewBindings>>) -> Self {
        Self {
            bindings,
            state: TableState::default(),
            selection_scroll: SelectionScroll::default(),
            markdown_link_style: NamedStyleCache::default(),
        }
    }
}

impl ScrollContent for TableBodyContent {
    fn is_focusable(&self) -> bool {
        self.bindings.read().enabled.get()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.bindings.read().height.get())
    }

    fn content_size(&mut self, viewport: (u16, u16), _ctx: ScrollContentContext<'_>) -> (u16, u16) {
        let bindings = self.bindings.read();
        let rows = bindings.rows.get();
        let height = rows.len().min(u16::MAX as usize) as u16;
        (viewport.0, height)
    }

    fn on_scrollbars(&mut self, _ctx: ScrollContentContext<'_>, host: &mut ScrollContainerHost) {
        let bindings = self.bindings.read();
        let rows = bindings.rows.get();
        self.selection_scroll
            .sync_selection_visible(&bindings.selection, rows.len(), host);
    }

    fn handle_event(
        &mut self,
        event: &Event,
        _ctx: ScrollContentContext<'_>,
        host: &mut ScrollContainerHost,
    ) -> EventResult {
        let bindings = self.bindings.read();
        if !bindings.enabled.get() {
            return EventResult::ignored();
        }
        let rows = bindings.rows.get();
        self.selection_scroll.handle_event(
            event,
            &bindings.selection,
            rows.len(),
            host,
            bindings.on_change.as_ref(),
        )
    }

    fn draw(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ScrollContentContext<'_>,
        _host: &mut ScrollContainerHost,
    ) {
        let bindings = self.bindings.read();
        let enabled = bindings.enabled.get();
        let base_style: Style =
            widget_style(ctx.component.theme, enabled, ctx.component.is_focused);
        let selection_style = ctx
            .component
            .theme
            .named_style("list-selection")
            .unwrap_or(ctx.component.theme.selection);
        let highlight_style = if enabled {
            selection_style
        } else {
            selection_style.patch(ctx.component.theme.widget.disabled)
        };

        let headers = bindings.headers.get();
        let rows = bindings.rows.get();
        let link_overlay = self.markdown_link_style.markdown_link(ctx.component.theme);
        let selection = self
            .selection_scroll
            .normalize_selection(&bindings.selection, rows.len());
        let column_count = column_count(&headers, &rows);
        let widths = column_constraints(column_count);
        let visible = visible_row_range(rows.len(), ctx.info.scroll_offset.y, area.height);
        let start = visible.start;

        let data_rows = rows[visible].iter().enumerate().map(|(offset, row)| {
            let idx = start + offset;
            let selected = selection.is_some_and(|sel| sel == idx);
            // Selected row cells use the highlight style so their text contrasts
            // with the highlight background instead of keeping the normal fg.
            let cell_style = if selected {
                highlight_style
            } else {
                base_style
            };
            let cells = row_cells(row, column_count, cell_style, link_overlay);
            let row = Row::new(cells);
            if selected {
                row.style(highlight_style)
            } else {
                row
            }
        });

        *self.state.selected_mut() = None;
        *self.state.selected_column_mut() = None;
        *self.state.offset_mut() = 0;

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

#[cfg(test)]
mod tests {
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::composable::{MouseCoordinateSpace, TabMode};
    use crate::theme::Theme;
    use crate::wm::WindowId;

    use super::*;

    fn component_context(theme: &Theme) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Window,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        }
    }

    fn screen_contents(terminal: &Terminal<TestBackend>, width: u16, height: u16) -> String {
        let buf = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..height {
            for x in 0..width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn cell_symbol(terminal: &Terminal<TestBackend>, x: u16, y: u16) -> String {
        terminal.backend().buffer()[(x, y)].symbol().to_string()
    }

    #[test]
    fn draw_slices_visible_rows_after_vertical_scroll() {
        let rows: Vec<Vec<String>> = (0..12)
            .map(|idx| vec![format!("row-{idx:02}"), format!("value-{idx:02}")])
            .collect();
        let mut table = TableView::new(
            "Rows",
            Binding::new(vec!["name".to_string(), "value".to_string()]),
            Binding::new(rows),
            Binding::new(0usize),
        )
        .height(7u16);
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(30, 7)).expect("terminal");
        let area = Rect::new(0, 0, 30, 7);

        terminal
            .draw(|f| table.draw(f, area, component_context(&theme)))
            .expect("initial draw");
        table.scroll.set_scroll_offset(0, 3);
        terminal
            .draw(|f| table.draw(f, area, component_context(&theme)))
            .expect("scrolled draw");

        let screen = screen_contents(&terminal, 30, 7);
        assert!(!screen.contains("row-02"), "screen was:\n{screen}");
        assert!(screen.contains("row-03"), "screen was:\n{screen}");
        assert!(screen.contains("row-06"), "screen was:\n{screen}");
        assert!(!screen.contains("row-07"), "screen was:\n{screen}");
    }

    #[test]
    fn short_table_scrollbar_keeps_arrows_and_track_visible() {
        let rows: Vec<Vec<String>> = (0..12)
            .map(|idx| vec![format!("row-{idx:02}"), format!("value-{idx:02}")])
            .collect();
        let mut table = TableView::new(
            "Rows",
            Binding::new(vec!["name".to_string(), "value".to_string()]),
            Binding::new(rows),
            Binding::new(0usize),
        )
        .height(4u16);
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(20, 4)).expect("terminal");
        let area = Rect::new(0, 0, 20, 4);

        terminal
            .draw(|f| table.draw(f, area, component_context(&theme)))
            .expect("draw");

        assert_eq!(cell_symbol(&terminal, 19, 0), "▲");
        assert_eq!(cell_symbol(&terminal, 19, 2), "░");
        assert_eq!(cell_symbol(&terminal, 19, 3), "▼");
    }

    #[test]
    fn keyboard_wraps_and_mouse_selection_updates_binding() {
        let rows: Vec<Vec<String>> = (0..5)
            .map(|idx| vec![format!("row-{idx}"), format!("value-{idx}")])
            .collect();
        let selection = Binding::new(0usize);
        let mut table = TableView::new(
            "Rows",
            Binding::new(vec!["name".to_string(), "value".to_string()]),
            Binding::new(rows),
            selection.clone(),
        )
        .height(6u16);
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(32, 9)).expect("terminal");
        let area = Rect::new(1, 1, 30, 6);
        terminal
            .draw(|f| table.draw(f, area, component_context(&theme)))
            .expect("draw");

        let up = Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(
            table.handle_event(&up, component_context(&theme)),
            EventResult::changed()
        );
        assert_eq!(selection.get(), 4);

        let down = Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(
            table.handle_event(&down, component_context(&theme)),
            EventResult::changed()
        );
        assert_eq!(selection.get(), 0);

        terminal
            .draw(|f| table.draw(f, area, component_context(&theme)))
            .expect("redraw");
        let click_row_two = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x + 2,
            row: area.y + 4,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            table.handle_event(&click_row_two, component_context(&theme)),
            EventResult::changed()
        );
        assert_eq!(selection.get(), 2);
    }
}
