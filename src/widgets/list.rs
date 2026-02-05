use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::composable::{Component, ComponentContext, EventResult};
use crate::reactive::Binding;

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

    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
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
                if m.row < inner.y || m.row >= inner.y.saturating_add(inner.height) {
                    return EventResult::ignored();
                }
                let row = m.row.saturating_sub(inner.y) as usize;
                if row < items.len() {
                    self.state.select(Some(row));
                    self.selection.set(row);
                    return EventResult::changed();
                }
                EventResult::ignored()
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
        let items: Vec<ListItem> = items
            .iter()
            .map(|s| ListItem::new(Line::raw(s.clone())))
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(ctx.theme.border_set(false))
                    .title(self.title.get()),
            )
            .highlight_style(highlight_style)
            .style(style);
        frame.render_stateful_widget(list, area, &mut self.state);
    }
}
