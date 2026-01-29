use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::theme::Theme;

use super::{Control, ControlOutcome, FormAction};

#[derive(Clone, Debug)]
pub struct ListBox {
    title: String,
    items: Vec<String>,
    state: ListState,
    focused: bool,
    height: u16,
}

impl ListBox {
    pub fn new(title: impl Into<String>, items: Vec<String>) -> Self {
        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }
        Self {
            title: title.into(),
            items,
            state,
            focused: false,
            height: 7,
        }
    }

    pub fn with_height(mut self, height: u16) -> Self {
        self.height = height.max(3);
        self
    }

    pub fn selected(&self) -> Option<usize> {
        self.state.selected()
    }
}

impl Control for ListBox {
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn handle_event(&mut self, event: &Event) -> (ControlOutcome, FormAction) {
        let Event::Key(KeyEvent { code, .. }) = event else {
            return (ControlOutcome::Ignored, FormAction::None);
        };
        if self.items.is_empty() {
            return (ControlOutcome::Ignored, FormAction::None);
        }
        let sel = self.state.selected().unwrap_or(0);
        match code {
            KeyCode::Up => {
                let next = if sel == 0 {
                    self.items.len() - 1
                } else {
                    sel.saturating_sub(1)
                };
                self.state.select(Some(next));
                (ControlOutcome::Consumed, FormAction::Changed)
            }
            KeyCode::Down => {
                let next = (sel + 1) % self.items.len();
                self.state.select(Some(next));
                (ControlOutcome::Consumed, FormAction::Changed)
            }
            _ => (ControlOutcome::Ignored, FormAction::None),
        }
    }

    fn desired_height(&self) -> u16 {
        self.height
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let style = if self.focused {
            theme.widget.focused
        } else {
            theme.widget.normal
        };
        let items: Vec<ListItem> = self
            .items
            .iter()
            .map(|s| ListItem::new(Line::raw(s.clone())))
            .collect();
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(self.title.clone()))
            .highlight_style(theme.menu_item_selected)
            .style(style);
        frame.render_stateful_widget(list, area, &mut self.state);
    }
}
