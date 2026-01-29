use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};
use ratatui::Frame;

use crate::theme::Theme;

use super::{Control, ControlOutcome, FormAction};

#[derive(Clone, Debug)]
pub struct TableView {
    title: String,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    state: TableState,
    focused: bool,
    height: u16,
}

impl TableView {
    pub fn new(title: impl Into<String>, headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        let mut state = TableState::default();
        if !rows.is_empty() {
            state.select(Some(0));
        }
        Self {
            title: title.into(),
            headers,
            rows,
            state,
            focused: false,
            height: 8,
        }
    }

    pub fn with_height(mut self, height: u16) -> Self {
        self.height = height.max(3);
        self
    }
}

impl Control for TableView {
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn desired_height(&self) -> u16 {
        self.height
    }

    fn handle_event(&mut self, event: &Event) -> (ControlOutcome, FormAction) {
        let Event::Key(KeyEvent { code, .. }) = event else {
            return (ControlOutcome::Ignored, FormAction::None);
        };
        if self.rows.is_empty() {
            return (ControlOutcome::Ignored, FormAction::None);
        }
        let sel = self.state.selected().unwrap_or(0);
        match code {
            KeyCode::Up => {
                let next = if sel == 0 { self.rows.len() - 1 } else { sel - 1 };
                self.state.select(Some(next));
                (ControlOutcome::Consumed, FormAction::Changed)
            }
            KeyCode::Down => {
                let next = (sel + 1) % self.rows.len();
                self.state.select(Some(next));
                (ControlOutcome::Consumed, FormAction::Changed)
            }
            _ => (ControlOutcome::Ignored, FormAction::None),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let base_style: Style = if self.focused {
            theme.widget.focused
        } else {
            theme.widget.normal
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

        let header = Row::new(self.headers.iter().cloned().map(Cell::from)).style(theme.widget.accent);
        let rows = self
            .rows
            .iter()
            .map(|r| Row::new(r.iter().cloned().map(Cell::from)));
        let table = Table::new(rows, widths)
            .header(header)
            .block(Block::default().borders(Borders::ALL).title(self.title.clone()))
            .row_highlight_style(theme.menu_item_selected)
            .style(base_style);

        frame.render_stateful_widget(table, area, &mut self.state);
    }
}
