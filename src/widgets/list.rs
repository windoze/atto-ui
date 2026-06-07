use std::sync::Arc;

use crate::text::styled_text::{inline_display_width, parse_inline, slice_spans_from_segments};
use crossterm::event::{Event, MouseEvent};
use parking_lot::RwLock;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::ComponentCommand;
use crate::composable::scroll::{
    ScrollbarDrag, Scrollbars, draw_scrollbars, handle_scrollbar_mouse_event,
};
use crate::composable::{
    Component, ComponentContext, EdgeInsets, EventHandling, EventResult, FocusNav, Layout,
    ScrollConfig, ScrollContainer, ScrollContainerHost, ScrollContent, ScrollContentContext,
    ScrollOffset, Scrollable, ScrollbarHost, should_show_scrollbar,
};
use crate::reactive::Binding;
use crate::runtime::CallbackHandle;
use atto_ui_macros::{ComponentProperties, component_properties};

use super::util::{
    NamedStyleCache, SelectionScroll, mouse_coords_local_to_area, visible_row_range, widget_style,
};

#[derive(Clone, Debug, ComponentProperties)]
struct ListBoxBindings {
    title: Binding<String>,
    items: Binding<Vec<String>>,
    enabled: Binding<bool>,
    selection: Binding<usize>,
    height: Binding<u16>,
    #[component(skip)]
    on_change: Option<CallbackHandle>,
}

#[derive(ComponentProperties)]
pub struct ListBox {
    #[component(delegate)]
    bindings: Arc<RwLock<ListBoxBindings>>,
    scroll: ScrollContainer,
    min_size: (u16, u16),
    last_area: Option<Rect>,
    scrollbar_drag: Option<ScrollbarDrag>,
}

impl Clone for ListBox {
    fn clone(&self) -> Self {
        let bindings = self.bindings.clone();
        Self {
            scroll: build_scroll_container(bindings.clone()),
            bindings,
            min_size: self.min_size,
            last_area: None,
            scrollbar_drag: None,
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
            last_area: None,
            scrollbar_drag: None,
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

#[component_properties]
impl Component for ListBox {
    fn apply_command(&mut self, command: ComponentCommand) -> EventResult {
        match command {
            ComponentCommand::SelectIndex(idx) => {
                let bindings = self.bindings.write();
                let items_len = bindings.items.get().len();
                if items_len > 0 {
                    bindings.selection.set(idx.min(items_len.saturating_sub(1)));
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
        let style = widget_style(ctx.theme, enabled, ctx.is_focused);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(ctx.theme.border_set(false))
            .title(bindings.title.get())
            .style(style);
        frame.render_widget(block, area);
        drop(bindings);

        let body_ctx = ComponentContext {
            scrollbar_host: ScrollbarHost::Window,
            drag: None,
            ..ctx
        };
        self.scroll.draw(frame, area, body_ctx);

        self.draw_border_scrollbar(frame, area, ctx);
    }
}

impl Layout for ListBox {
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

impl FocusNav for ListBox {
    fn is_focusable(&self) -> bool {
        self.bindings.read().enabled.get()
    }
}

impl EventHandling for ListBox {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.bindings.read().enabled.get() {
            return EventResult::ignored();
        }

        // Border-mounted scrollbars (right + bottom) so the list content doesn't lose space.
        if let Event::Mouse(m) = event
            && let Some(area) = self.last_area
            && let Some((local_x, local_y)) =
                mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
        {
            let abs_event = MouseEvent {
                column: area.x.saturating_add(local_x),
                row: area.y.saturating_add(local_y),
                ..*m
            };
            if let Some(new_scroll) = self.handle_border_scrollbar_event(abs_event, area) {
                self.scroll.set_scroll_offset(new_scroll.x, new_scroll.y);
                return EventResult::consumed();
            }
        }

        let body_ctx = ComponentContext {
            scrollbar_host: ScrollbarHost::Window,
            drag: None,
            ..ctx
        };
        self.scroll.handle_event(event, body_ctx)
    }
}

crate::impl_component_default_traits!(ListBox => Scrollable, DynamicTree);

impl ListBox {
    fn border_scrollbars(&self, area: Rect) -> Option<Scrollbars> {
        if area.width < 3 || area.height < 3 {
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

        let content_local = Rect {
            x: 1,
            y: 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        if content_local.width == 0 || content_local.height == 0 {
            return None;
        }

        let vbar = show_v.then_some(Rect {
            x: area.width.saturating_sub(1),
            y: content_local.y,
            width: 1,
            height: content_local.height,
        });
        let hbar = show_h.then_some(Rect {
            x: content_local.x,
            y: area.height.saturating_sub(1),
            width: content_local.width,
            height: 1,
        });

        Some(Scrollbars {
            viewport: content_local,
            content: content_local,
            vbar,
            hbar,
            thickness: 1,
        })
    }

    fn draw_border_scrollbar(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ComponentContext<'_>,
    ) {
        let Some(scrollbars) = self.border_scrollbars(area) else {
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

    fn handle_border_scrollbar_event(&mut self, m: MouseEvent, area: Rect) -> Option<ScrollOffset> {
        let Some(scrollbars) = self.border_scrollbars(area) else {
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

struct ListBoxContent {
    bindings: Arc<RwLock<ListBoxBindings>>,
    state: ListState,
    selection_scroll: SelectionScroll,
    markdown_link_style: NamedStyleCache,
}

impl ListBoxContent {
    fn new(bindings: Arc<RwLock<ListBoxBindings>>) -> Self {
        Self {
            bindings,
            state: ListState::default(),
            selection_scroll: SelectionScroll::default(),
            markdown_link_style: NamedStyleCache::default(),
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
        self.bindings.read().enabled.get()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.bindings.read().height.get())
    }

    fn content_size(
        &mut self,
        _viewport: (u16, u16),
        _ctx: ScrollContentContext<'_>,
    ) -> (u16, u16) {
        let bindings = self.bindings.read();
        let items = bindings.items.get();
        Self::content_size_for_items(&items)
    }

    fn on_scrollbars(&mut self, _ctx: ScrollContentContext<'_>, host: &mut ScrollContainerHost) {
        let bindings = self.bindings.read();
        let items = bindings.items.get();
        self.selection_scroll
            .sync_selection_visible(&bindings.selection, items.len(), host);
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
        let items = bindings.items.get();
        self.selection_scroll.handle_event(
            event,
            &bindings.selection,
            items.len(),
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
        let style = widget_style(ctx.component.theme, enabled, ctx.component.is_focused);
        let highlight_style = if enabled {
            ctx.component.theme.selection
        } else {
            ctx.component
                .theme
                .selection
                .patch(ctx.component.theme.widget.disabled)
        };

        let items = bindings.items.get();
        let selection = self
            .selection_scroll
            .normalize_selection(&bindings.selection, items.len());
        let scroll = ctx.info.scroll_offset;
        let viewport_w = area.width;
        let link_overlay = self.markdown_link_style.markdown_link(ctx.component.theme);
        let visible = visible_row_range(items.len(), scroll.y, area.height);
        let start = visible.start;
        let items: Vec<ListItem> = items[visible]
            .iter()
            .enumerate()
            .map(|(offset, s)| {
                let idx = start + offset;
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
        *self.state.offset_mut() = 0;

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

    #[test]
    fn draw_slices_visible_rows_after_vertical_scroll() {
        let items: Vec<String> = (0..20).map(|idx| format!("row-{idx:02}")).collect();
        let mut list = ListBox::new("Rows", Binding::new(items), Binding::new(0usize)).height(7u16);
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(20, 7)).expect("terminal");
        let area = Rect::new(0, 0, 20, 7);

        terminal
            .draw(|f| list.draw(f, area, component_context(&theme)))
            .expect("initial draw");
        list.scroll.set_scroll_offset(0, 5);
        terminal
            .draw(|f| list.draw(f, area, component_context(&theme)))
            .expect("scrolled draw");

        let screen = screen_contents(&terminal, 20, 7);
        assert!(!screen.contains("row-04"), "screen was:\n{screen}");
        assert!(screen.contains("row-05"), "screen was:\n{screen}");
        assert!(screen.contains("row-09"), "screen was:\n{screen}");
        assert!(!screen.contains("row-10"), "screen was:\n{screen}");
    }

    #[test]
    fn keyboard_wraps_and_mouse_selection_updates_binding() {
        let items: Vec<String> = (0..5).map(|idx| format!("row-{idx}")).collect();
        let selection = Binding::new(0usize);
        let mut list = ListBox::new("Rows", Binding::new(items), selection.clone()).height(5u16);
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(20, 8)).expect("terminal");
        let area = Rect::new(2, 1, 16, 5);
        terminal
            .draw(|f| list.draw(f, area, component_context(&theme)))
            .expect("draw");

        let up = Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(
            list.handle_event(&up, component_context(&theme)),
            EventResult::changed()
        );
        assert_eq!(selection.get(), 4);

        let down = Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(
            list.handle_event(&down, component_context(&theme)),
            EventResult::changed()
        );
        assert_eq!(selection.get(), 0);

        terminal
            .draw(|f| list.draw(f, area, component_context(&theme)))
            .expect("redraw");
        let click_row_one = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x + 2,
            row: area.y + 2,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            list.handle_event(&click_row_one, component_context(&theme)),
            EventResult::changed()
        );
        assert_eq!(selection.get(), 1);
    }
}
