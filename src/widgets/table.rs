use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use crate::composable::{Component, ComponentContext, EventResult};
use crate::reactive::Binding;

#[derive(Clone, Debug)]
pub struct TableView {
    title: Binding<String>,
    headers: Binding<Vec<String>>,
    rows: Binding<Vec<Vec<String>>>,
    state: TableState,
    enabled: Binding<bool>,
    selection: Binding<usize>,
    height: Binding<u16>,
    last_area: Option<Rect>,
    min_size: (u16, u16),
}

impl TableView {
    pub fn new(
        title: impl Into<Binding<String>>,
        headers: impl Into<Binding<Vec<String>>>,
        rows: impl Into<Binding<Vec<Vec<String>>>>,
        selection: Binding<usize>,
    ) -> Self {
        let mut state = TableState::default();
        let rows = rows.into();
        let row_count = rows.get().len();
        if row_count > 0 {
            let selected = selection.get().min(row_count.saturating_sub(1));
            selection.set(selected);
            state.select(Some(selected));
        }
        Self {
            title: title.into(),
            headers: headers.into(),
            rows,
            state,
            enabled: true.into(),
            selection,
            height: 8.into(),
            last_area: None,
            min_size: (3, 4), // Minimum size to render borders, header, and one row.
        }
    }

    pub fn title(mut self, title: impl Into<Binding<String>>) -> Self {
        self.title = title.into();
        self
    }

    pub fn headers(mut self, headers: impl Into<Binding<Vec<String>>>) -> Self {
        self.headers = headers.into();
        self
    }

    pub fn rows(mut self, rows: impl Into<Binding<Vec<Vec<String>>>>) -> Self {
        self.rows = rows.into();
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

impl Component for TableView {
    fn min_width(&self) -> u16 {
        self.min_size.0
    }

    fn min_height(&self) -> u16 {
        self.min_size.1
    }

    fn is_focusable(&self) -> bool {
        self.enabled.get()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.height.get().max(self.min_size.1))
    }

    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }
        let rows = self.rows.get();
        if rows.is_empty() {
            return EventResult::ignored();
        }
        let ext = self.selection.get();
        if ext < rows.len() {
            self.state.select(Some(ext));
        }
        let sel = self.state.selected().unwrap_or(0);
        match event {
            Event::Mouse(m) => {
                use crossterm::event::MouseButton;
                use crossterm::event::MouseEventKind;

                if m.kind != MouseEventKind::Down(MouseButton::Left) {
                    return EventResult::ignored();
                }
                let Some(area) = self.last_area else {
                    return EventResult::ignored();
                };
                let inner = Rect {
                    x: area.x.saturating_add(1),
                    y: area.y.saturating_add(1),
                    width: area.width.saturating_sub(2),
                    height: area.height.saturating_sub(2),
                };
                if inner.width == 0 || inner.height == 0 {
                    return EventResult::ignored();
                }

                // Skip header row (always rendered at the top of the table body).
                let data_y = inner.y.saturating_add(1);
                if m.row < data_y || m.row >= inner.y.saturating_add(inner.height) {
                    return EventResult::ignored();
                }
                let row = m.row.saturating_sub(data_y) as usize;
                if row < rows.len() {
                    self.state.select(Some(row));
                    self.selection.set(row);
                    return EventResult::changed();
                }
                EventResult::ignored()
            }
            Event::Key(KeyEvent { code, .. }) => match code {
                KeyCode::Up => {
                    let next = if sel == 0 { rows.len() - 1 } else { sel - 1 };
                    self.state.select(Some(next));
                    self.selection.set(next);
                    EventResult::changed()
                }
                KeyCode::Down => {
                    let next = (sel + 1) % rows.len();
                    self.state.select(Some(next));
                    self.selection.set(next);
                    EventResult::changed()
                }
                _ => EventResult::ignored(),
            },
            _ => EventResult::ignored(),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        let rows = self.rows.get();
        let headers = self.headers.get();
        if !rows.is_empty() {
            let ext = self.selection.get();
            if ext < rows.len() {
                self.state.select(Some(ext));
            } else {
                self.state.select(Some(0));
                self.selection.set(0);
            }
        }
        let enabled = self.enabled.get();
        let base_style: Style = if !enabled {
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

        let widths = if headers.is_empty() {
            vec![Constraint::Percentage(100)]
        } else {
            let pct = (100 / headers.len().max(1)) as u16;
            headers
                .iter()
                .map(|_| Constraint::Percentage(pct.max(1)))
                .collect()
        };

        let header_style = if enabled {
            ctx.theme.widget.accent
        } else {
            ctx.theme.widget.disabled
        };
        let header = Row::new(headers.iter().cloned().map(Cell::from)).style(header_style);
        let data_rows = rows
            .iter()
            .map(|r| Row::new(r.iter().cloned().map(Cell::from)));
        let table = Table::new(data_rows, widths)
            .header(header)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(ctx.theme.border_set(false))
                    .title(self.title.get()),
            )
            .row_highlight_style(highlight_style)
            .style(base_style);

        frame.render_stateful_widget(table, area, &mut self.state);
    }
}
