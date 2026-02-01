use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::reactive::PropertyBinding;
use crate::theme::Theme;

use super::{Control, ControlOutcome, FormAction};

#[derive(Clone, Debug)]
pub struct ListBox {
    title: String,
    items: Vec<String>,
    state: ListState,
    focused: bool,
    enabled: bool,
    selection: PropertyBinding<usize>,
    height: u16,
    area: Option<Rect>,
}

impl ListBox {
    pub fn new(
        title: impl Into<String>,
        items: Vec<String>,
        selection: PropertyBinding<usize>,
    ) -> Self {
        let mut state = ListState::default();
        if !items.is_empty() && selection.get() < items.len() {
            state.select(Some(selection.get()));
        } else if !items.is_empty() {
            selection.set(0);
            state.select(Some(0));
        }
        Self {
            title: title.into(),
            items,
            state,
            focused: false,
            enabled: true,
            selection,
            height: 7,
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

    pub fn selected(&self) -> Option<usize> {
        self.state.selected().or_else(|| {
            (!self.items.is_empty() && self.selection.get() < self.items.len())
                .then_some(self.selection.get())
        })
    }
}

impl Control for ListBox {
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

    fn handle_event(&mut self, event: &Event) -> (ControlOutcome, FormAction) {
        if !self.enabled {
            return (ControlOutcome::Ignored, FormAction::None);
        }
        if self.items.is_empty() {
            return (ControlOutcome::Ignored, FormAction::None);
        }
        // Sync from external selection.
        let ext = self.selection.get();
        if ext < self.items.len() {
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
                if m.row < inner.y || m.row >= inner.y.saturating_add(inner.height) {
                    return (ControlOutcome::Ignored, FormAction::None);
                }
                let row = m.row.saturating_sub(inner.y) as usize;
                if row < self.items.len() {
                    self.state.select(Some(row));
                    self.selection.set(row);
                    return (ControlOutcome::Consumed, FormAction::Changed);
                }
                (ControlOutcome::Ignored, FormAction::None)
            }
            Event::Key(KeyEvent { code, .. }) => match code {
                KeyCode::Up => {
                    let next = if sel == 0 {
                        self.items.len() - 1
                    } else {
                        sel.saturating_sub(1)
                    };
                    self.state.select(Some(next));
                    self.selection.set(next);
                    (ControlOutcome::Consumed, FormAction::Changed)
                }
                KeyCode::Down => {
                    let next = (sel + 1) % self.items.len();
                    self.state.select(Some(next));
                    self.selection.set(next);
                    (ControlOutcome::Consumed, FormAction::Changed)
                }
                _ => (ControlOutcome::Ignored, FormAction::None),
            },
            _ => (ControlOutcome::Ignored, FormAction::None),
        }
    }

    fn desired_height(&self) -> u16 {
        self.height
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        if !self.items.is_empty() {
            let ext = self.selection.get();
            if ext < self.items.len() {
                self.state.select(Some(ext));
            }
        }
        let style = if !self.enabled {
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
        let items: Vec<ListItem> = self
            .items
            .iter()
            .map(|s| ListItem::new(Line::raw(s.clone())))
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(theme.border_set(false))
                    .title(self.title.clone()),
            )
            .highlight_style(highlight_style)
            .style(style);
        frame.render_stateful_widget(list, area, &mut self.state);
    }
}
