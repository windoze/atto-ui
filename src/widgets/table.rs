use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use crate::reactive::PropertyBinding;
use crate::theme::Theme;

use super::{Control, ControlOutcome, FormAction};

#[derive(Clone, Debug)]
pub struct TableView {
    title: String,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    state: TableState,
    focused: bool,
    enabled: bool,
    selection: PropertyBinding<usize>,
    height: u16,
    area: Option<Rect>,
}

impl TableView {
    pub fn new(
        title: impl Into<String>,
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
        selection: PropertyBinding<usize>,
    ) -> Self {
        let mut state = TableState::default();
        if !rows.is_empty() && selection.get() < rows.len() {
            state.select(Some(selection.get()));
        } else if !rows.is_empty() {
            selection.set(0);
            state.select(Some(0));
        }
        Self {
            title: title.into(),
            headers,
            rows,
            state,
            focused: false,
            enabled: true,
            selection,
            height: 8,
            area: None,
        }
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_height(mut self, height: u16) -> Self {
        self.height = height.max(3);
        self
    }
}

impl Control for TableView {
    fn is_focusable(&self) -> bool {
        self.enabled
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn set_area(&mut self, area: Rect) {
        self.area = Some(area);
    }

    fn desired_height(&self) -> u16 {
        self.height
    }

    fn handle_event(&mut self, event: &Event) -> (ControlOutcome, FormAction) {
        if !self.enabled {
            return (ControlOutcome::Ignored, FormAction::None);
        }
        if self.rows.is_empty() {
            return (ControlOutcome::Ignored, FormAction::None);
        }
        let ext = self.selection.get();
        if ext < self.rows.len() {
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
                if row < self.rows.len() {
                    self.state.select(Some(row));
                    self.selection.set(row);
                    return (ControlOutcome::Consumed, FormAction::Changed);
                }
                (ControlOutcome::Ignored, FormAction::None)
            }
            Event::Key(KeyEvent { code, .. }) => match code {
                KeyCode::Up => {
                    let next = if sel == 0 {
                        self.rows.len() - 1
                    } else {
                        sel - 1
                    };
                    self.state.select(Some(next));
                    self.selection.set(next);
                    (ControlOutcome::Consumed, FormAction::Changed)
                }
                KeyCode::Down => {
                    let next = (sel + 1) % self.rows.len();
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
        if !self.rows.is_empty() {
            let ext = self.selection.get();
            if ext < self.rows.len() {
                self.state.select(Some(ext));
            }
        }
        let base_style: Style = if !self.enabled {
            theme.widget.disabled
        } else if self.focused {
            theme.widget.focused
        } else {
            theme.widget.normal
        };
        let highlight_style = if self.enabled {
            theme.selection
        } else {
            theme.selection.patch(theme.widget.disabled)
        };

        let widths = if self.headers.is_empty() {
            vec![Constraint::Percentage(100)]
        } else {
            let pct = (100 / self.headers.len().max(1)) as u16;
            self.headers
                .iter()
                .map(|_| Constraint::Percentage(pct.max(1)))
                .collect()
        };

        let header_style = if self.enabled {
            theme.widget.accent
        } else {
            theme.widget.disabled
        };
        let header = Row::new(self.headers.iter().cloned().map(Cell::from)).style(header_style);
        let rows = self
            .rows
            .iter()
            .map(|r| Row::new(r.iter().cloned().map(Cell::from)));
        let table = Table::new(rows, widths)
            .header(header)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(theme.border_set(false))
                    .title(self.title.clone()),
            )
            .row_highlight_style(highlight_style)
            .style(base_style);

        frame.render_stateful_widget(table, area, &mut self.state);
    }
}
