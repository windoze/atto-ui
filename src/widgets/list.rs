use std::sync::Arc;

use crate::text::styled_text::{inline_display_width, parse_inline, slice_spans_from_segments};
use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEventKind};
use parking_lot::RwLock;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use atto_ui_macros::{ComponentProps, component_props};
use crate::ComponentCommand;
use crate::composable::{
    Component, ComponentContext, EdgeInsets, EventResult, ScrollConfig, ScrollContainer,
    ScrollContainerHost, ScrollContent, ScrollContentContext,
};
use crate::runtime::CallbackHandle;
use crate::reactive::Binding;

#[derive(Clone, Debug, ComponentProps)]
struct ListBoxBindings {
    title: Binding<String>,
    items: Binding<Vec<String>>,
    enabled: Binding<bool>,
    selection: Binding<usize>,
    height: Binding<u16>,
    #[component(skip)]
    on_change: Option<CallbackHandle>,
}

#[derive(ComponentProps)]
pub struct ListBox {
    #[component(delegate)]
    bindings: Arc<RwLock<ListBoxBindings>>,
    scroll: ScrollContainer,
    min_size: (u16, u16),
}

impl Clone for ListBox {
    fn clone(&self) -> Self {
        let bindings = self.bindings.clone();
        Self {
            scroll: build_scroll_container(bindings.clone()),
            bindings,
            min_size: self.min_size,
        }
    }
}

impl std::fmt::Debug for ListBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bindings = self.bindings.read();
        f.debug_struct("ListBox")
            .field("title", &bindings.title.get())
            .field("enabled", &bindings.enabled.get())
            .field("height", &bindings.height.get())
            .field("min_size", &self.min_size)
            .finish()
    }
}

impl ListBox {
    pub fn new(
        title: impl Into<Binding<String>>,
        items: impl Into<Binding<Vec<String>>>,
        selection: Binding<usize>,
    ) -> Self {
        let items = items.into();
        let items_len = items.get().len();
        if items_len > 0 {
            let selected = selection.get().min(items_len.saturating_sub(1));
            selection.set(selected);
        }
        let bindings = Arc::new(RwLock::new(ListBoxBindings {
            title: title.into(),
            items,
            enabled: true.into(),
            selection,
            height: 7.into(),
            on_change: None,
        }));
        Self {
            scroll: build_scroll_container(bindings.clone()),
            bindings,
            min_size: (3, 3),
        }
    }

    pub fn title(self, title: impl Into<Binding<String>>) -> Self {
        self.bindings.write().title = title.into();
        self
    }

    pub fn items(self, items: impl Into<Binding<Vec<String>>>) -> Self {
        {
            let mut bindings = self.bindings.write();
            bindings.items = items.into();
            let items_len = bindings.items.get().len();
            if items_len > 0 {
                let selected = bindings.selection.get().min(items_len.saturating_sub(1));
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

    pub fn on_change_callback(self, callback: CallbackHandle) -> Self {
        self.bindings.write().on_change = Some(callback);
        self
    }

    pub fn selected(&self) -> Option<usize> {
        let bindings = self.bindings.read();
        let items = bindings.items.get();
        if items.is_empty() {
            return None;
        }
        let selection = bindings.selection.get();
        (selection < items.len()).then_some(selection)
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

#[component_props]
impl Component for ListBox {
    fn apply_command(&mut self, command: ComponentCommand) -> EventResult {
        match command {
            ComponentCommand::SelectIndex(idx) => {
                let bindings = self.bindings.write();
                let items_len = bindings.items.get().len();
                if items_len > 0 {
                    bindings
                        .selection
                        .set(idx.min(items_len.saturating_sub(1)));
                    EventResult::changed()
                } else {
                    EventResult::ignored()
                }
            }
            _ => EventResult::ignored(),
        }
    }

    fn min_width(&self) -> u16 {
        self.min_size.0
    }

    fn min_height(&self) -> u16 {
        self.min_size.1
    }

    fn is_focusable(&self) -> bool {
        self.bindings.read().enabled.get()
    }

    fn desired_height(&self) -> Option<u16> {
        let height = self.bindings.read().height.get();
        Some(height.max(self.min_size.1))
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.bindings.read().enabled.get() {
            return EventResult::ignored();
        }
        self.scroll.handle_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let bindings = self.bindings.read();
        let enabled = bindings.enabled.get();
        let style = if !enabled {
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
            .style(style);
        frame.render_widget(block, area);
        drop(bindings);

        self.scroll.draw(frame, area, ctx);
    }
}

struct ListBoxContent {
    bindings: Arc<RwLock<ListBoxBindings>>,
    state: ListState,
    last_selection: Option<usize>,
}

impl ListBoxContent {
    fn new(bindings: Arc<RwLock<ListBoxBindings>>) -> Self {
        Self {
            bindings,
            state: ListState::default(),
            last_selection: None,
        }
    }

    fn bindings(&self) -> ListBoxBindings {
        self.bindings.read().clone()
    }

    fn normalize_selection(&mut self, items_len: usize) -> Option<usize> {
        if items_len == 0 {
            return None;
        }
        let bindings = self.bindings();
        let mut selection = bindings.selection.get();
        if selection >= items_len {
            selection = items_len.saturating_sub(1);
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

    fn content_size_for_items(items: &[String]) -> (u16, u16) {
        let height = items.len().min(u16::MAX as usize) as u16;
        let mut width = 0_u16;
        for item in items {
            let w = inline_display_width(item.as_str());
            width = width.max(w);
        }
        (width, height)
    }
}

impl ScrollContent for ListBoxContent {
    fn is_focusable(&self) -> bool {
        self.bindings().enabled.get()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.bindings().height.get())
    }

    fn content_size(
        &mut self,
        _viewport: (u16, u16),
        _ctx: ScrollContentContext<'_>,
    ) -> (u16, u16) {
        let items = self.bindings().items.get();
        Self::content_size_for_items(&items)
    }

    fn on_scrollbars(&mut self, _ctx: ScrollContentContext<'_>, host: &mut ScrollContainerHost) {
        let items = self.bindings().items.get();
        let selection = self.normalize_selection(items.len());
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
        let items = bindings.items.get();
        let Some(selection) = self.normalize_selection(items.len()) else {
            return EventResult::ignored();
        };

        match event {
            Event::Mouse(m) => {
                if m.kind != MouseEventKind::Down(MouseButton::Left) {
                    return EventResult::ignored();
                }
                let row = m.row as usize;
                let idx = host.scroll_offset().y as usize + row;
                if idx < items.len() {
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
                        items.len() - 1
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
                    let next = (selection + 1) % items.len();
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
        let style = if !enabled {
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

        let items = bindings.items.get();
        let selection = self.normalize_selection(items.len());
        let scroll = ctx.info.scroll_offset;
        let viewport_w = area.width;
        let link_overlay = ctx.component.theme.named_style("markdown-link");
        let items: Vec<ListItem> = items
            .iter()
            .enumerate()
            .map(|(idx, s)| {
                let segments = parse_inline(s);
                let spans =
                    slice_spans_from_segments(&segments, scroll.x, viewport_w, style, link_overlay);
                let item = ListItem::new(Line::from(spans));
                if selection.is_some_and(|sel| sel == idx) {
                    item.style(highlight_style)
                } else {
                    item
                }
            })
            .collect();

        *self.state.selected_mut() = None;
        *self.state.offset_mut() = scroll.y as usize;

        if area.width > 0 && area.height > 0 {
            let list = List::new(items).style(style);
            frame.render_stateful_widget(list, area, &mut self.state);
        }
    }
}

fn build_scroll_container(bindings: Arc<RwLock<ListBoxBindings>>) -> ScrollContainer {
    ScrollContainer::new(Box::new(ListBoxContent::new(bindings)))
        .with_padding(EdgeInsets::all(1))
        .with_scroll_config(ScrollConfig::default())
}
