use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use crate::reactive::Binding;
use crate::theme::Theme;

use super::{Control, ControlOutcome, FormAction};

#[derive(Clone, Debug)]
pub struct TableView {
    title: Binding<String>,
    headers: Binding<Vec<String>>,
    rows: Binding<Vec<Vec<String>>>,
    state: TableState,
    focused: bool,
    enabled: Binding<bool>,
    selection: Binding<usize>,
    height: Binding<u16>,
    area: Option<Rect>,
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
            focused: false,
            enabled: true.into(),
            selection,
            height: 8.into(),
            area: None,
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
}

impl Control for TableView {
    fn min_width(&self) -> u16 {
        3
    }

    fn min_height(&self) -> u16 {
        // Table needs borders + header + at least one data row to be usable.
        4
    }

    fn is_focusable(&self) -> bool {
        self.enabled.get()
    }

    fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn set_area(&mut self, area: Rect) {
        self.area = Some(area);
    }

    fn desired_height(&self) -> u16 {
        self.height.get().max(3)
    }

    fn handle_event(&mut self, event: &Event) -> (ControlOutcome, FormAction) {
        if !self.enabled.get() {
            return (ControlOutcome::Ignored, FormAction::None);
        }
        let rows = self.rows.get();
        if rows.is_empty() {
            return (ControlOutcome::Ignored, FormAction::None);
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
                    return (ControlOutcome::Ignored, FormAction::None);
                }
                let Some(area) = self.area else {
                    return (ControlOutcome::Ignored, FormAction::None);
                };
                let inner = Rect {
                    x: area.x.saturating_add(1),
                    y: area.y.saturating_add(1),
                    width: area.width.saturating_sub(2),
                    height: area.height.saturating_sub(2),
                };
                if inner.width == 0 || inner.height == 0 {
                    return (ControlOutcome::Ignored, FormAction::None);
                }

                // Skip header row (always rendered at the top of the table body).
                let data_y = inner.y.saturating_add(1);
                if m.row < data_y || m.row >= inner.y.saturating_add(inner.height) {
                    return (ControlOutcome::Ignored, FormAction::None);
                }
                let row = m.row.saturating_sub(data_y) as usize;
                if row < rows.len() {
                    self.state.select(Some(row));
                    self.selection.set(row);
                    return (ControlOutcome::Consumed, FormAction::Changed);
                }
                (ControlOutcome::Ignored, FormAction::None)
            }
            Event::Key(KeyEvent { code, .. }) => match code {
                KeyCode::Up => {
                    let next = if sel == 0 { rows.len() - 1 } else { sel - 1 };
                    self.state.select(Some(next));
                    self.selection.set(next);
                    (ControlOutcome::Consumed, FormAction::Changed)
                }
                KeyCode::Down => {
                    let next = (sel + 1) % rows.len();
                    self.state.select(Some(next));
                    self.selection.set(next);
                    (ControlOutcome::Consumed, FormAction::Changed)
                }
                _ => (ControlOutcome::Ignored, FormAction::None),
            },
            _ => (ControlOutcome::Ignored, FormAction::None),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
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
            theme.widget.disabled
        } else if self.focused {
            theme.widget.focused
        } else {
            theme.widget.normal
        };
        let highlight_style = if enabled {
            theme.selection
        } else {
            theme.selection.patch(theme.widget.disabled)
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
            theme.widget.accent
        } else {
            theme.widget.disabled
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
                    .border_set(theme.border_set(false))
                    .title(self.title.get()),
            )
            .row_highlight_style(highlight_style)
            .style(base_style);

        frame.render_stateful_widget(table, area, &mut self.state);
    }
}
