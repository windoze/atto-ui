//! Reusable fuzzy picker views for editor-app modals.

use atto_ui::composable::{
    Component, ComponentContext, EventHandling, EventResult, FocusNav, Layout, Scrollable,
};
use atto_ui::fuzzy::{fuzzy_filter, fuzzy_match};
use atto_ui::reactive::{Binding, EventQueue};
use atto_ui::widgets::TextBox;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// One selectable row in a picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerItem<A> {
    pub title: String,
    pub subtitle: String,
    pub shortcut: Option<String>,
    pub action: A,
}

impl<A> PickerItem<A> {
    /// Creates a picker item with a title and accepted action.
    pub fn new(title: impl Into<String>, action: A) -> Self {
        Self {
            title: title.into(),
            subtitle: String::new(),
            shortcut: None,
            action,
        }
    }

    /// Sets secondary text displayed and searched below the title.
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = subtitle.into();
        self
    }

    /// Sets the user-facing shortcut label displayed on the row.
    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    fn search_text(&self) -> String {
        let mut text = self.title.clone();
        if !self.subtitle.is_empty() {
            text.push(' ');
            text.push_str(&self.subtitle);
        }
        if let Some(shortcut) = &self.shortcut
            && !shortcut.is_empty()
        {
            text.push(' ');
            text.push_str(shortcut);
        }
        text
    }
}

/// Returns whether an item matches a query using the shared fuzzy matcher.
pub fn picker_item_matches<A>(item: &PickerItem<A>, query: &str) -> bool {
    fuzzy_match(&item.search_text(), query).is_some()
}

/// Events emitted by a picker view to its host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerEvent<A> {
    Accepted(A),
    Submitted(String),
    Closed,
}

/// A generic fuzzy picker with a query textbox and keyboard navigation.
pub struct PickerView<A> {
    title: String,
    placeholder: String,
    query: Binding<String>,
    input: TextBox,
    items: Vec<PickerItem<A>>,
    search_texts: Vec<String>,
    filtered: Vec<usize>,
    selected: usize,
    scroll: usize,
    max_results: usize,
    submit_query_on_empty: bool,
    last_filter_query: Option<String>,
    last_area: Option<Rect>,
    events: EventQueue<PickerEvent<A>>,
}

impl<A> PickerView<A> {
    /// Creates a picker over a static item list.
    pub fn new(
        title: impl Into<String>,
        items: Vec<PickerItem<A>>,
        events: EventQueue<PickerEvent<A>>,
    ) -> Self {
        let query = Binding::new(String::new());
        let placeholder = "Type to filter...".to_string();
        let input = TextBox::new("Query", query.clone()).placeholder(placeholder.clone());
        let search_texts = items.iter().map(PickerItem::search_text).collect();
        Self {
            title: title.into(),
            placeholder,
            query,
            input,
            items,
            search_texts,
            filtered: Vec::new(),
            selected: 0,
            scroll: 0,
            max_results: 200,
            submit_query_on_empty: false,
            last_filter_query: None,
            last_area: None,
            events,
        }
    }

    /// Sets placeholder text for the query input.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self.input =
            TextBox::new("Query", self.query.clone()).placeholder(self.placeholder.clone());
        self
    }

    /// Sets an upper bound for filtered rows kept in memory.
    pub fn max_results(mut self, max_results: usize) -> Self {
        self.max_results = max_results.max(1);
        self.invalidate_filter();
        self
    }

    pub fn submit_query_on_empty(mut self, submit: bool) -> Self {
        self.submit_query_on_empty = submit;
        self
    }

    /// Updates the query programmatically.
    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query.set(query.into());
        self.selected = 0;
        self.scroll = 0;
        self.invalidate_filter();
        self.refresh_filter();
    }

    /// Returns the current query string.
    pub fn query(&self) -> String {
        self.query.get()
    }

    /// Returns filtered item titles in their current order.
    pub fn filtered_titles(&mut self) -> Vec<String> {
        self.refresh_filter();
        self.filtered
            .iter()
            .map(|index| self.items[*index].title.clone())
            .collect()
    }

    /// Returns the selected index within the filtered list.
    pub fn selected_filtered_index(&mut self) -> Option<usize> {
        self.refresh_filter();
        self.clamp_selection();
        (!self.filtered.is_empty()).then_some(self.selected)
    }

    fn invalidate_filter(&mut self) {
        self.last_filter_query = None;
    }

    fn refresh_filter(&mut self) {
        let query = self.query.get();
        if self.last_filter_query.as_ref() == Some(&query) {
            self.clamp_selection();
            return;
        }

        self.filtered = fuzzy_filter(&self.search_texts, &query, self.max_results)
            .into_iter()
            .map(|matched| matched.index)
            .collect();
        self.selected = 0;
        self.scroll = 0;
        self.last_filter_query = Some(query);
        self.clamp_selection();
    }

    fn clamp_selection(&mut self) {
        if self.filtered.is_empty() {
            self.selected = 0;
            self.scroll = 0;
            return;
        }
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
        self.clamp_scroll(self.visible_capacity().max(1));
    }

    fn visible_capacity(&self) -> usize {
        self.last_area
            .map(|area| picker_list_area(area).height as usize)
            .unwrap_or(8)
    }

    fn clamp_scroll(&mut self, capacity: usize) {
        if self.filtered.is_empty() || capacity == 0 {
            self.scroll = 0;
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll.saturating_add(capacity) {
            self.scroll = self.selected.saturating_add(1).saturating_sub(capacity);
        }
        let max_scroll = self.filtered.len().saturating_sub(capacity);
        self.scroll = self.scroll.min(max_scroll);
    }

    fn move_selection(&mut self, delta: isize) -> EventResult {
        self.refresh_filter();
        if self.filtered.is_empty() {
            return EventResult::consumed();
        }

        let last = self.filtered.len().saturating_sub(1);
        if delta < 0 {
            self.selected = self.selected.saturating_sub(delta.unsigned_abs());
        } else {
            self.selected = self.selected.saturating_add(delta as usize).min(last);
        }
        self.clamp_scroll(self.visible_capacity().max(1));
        EventResult::changed()
    }

    fn accept_selected(&mut self) -> EventResult
    where
        A: Clone,
    {
        self.refresh_filter();
        let Some(item_index) = self.filtered.get(self.selected).copied() else {
            if self.submit_query_on_empty {
                let query = self.query.get();
                if !query.trim().is_empty() {
                    self.events.push(PickerEvent::Submitted(query));
                    return EventResult::close_window();
                }
            }
            return EventResult::consumed();
        };
        self.events
            .push(PickerEvent::Accepted(self.items[item_index].action.clone()));
        EventResult::close_window()
    }

    fn close(&self) -> EventResult {
        self.events.push(PickerEvent::Closed);
        EventResult::close_window()
    }

    fn child_ctx<'a>(ctx: ComponentContext<'a>) -> ComponentContext<'a> {
        ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused,
            scrollbar_host: ctx.scrollbar_host.for_child(),
            tab_mode: ctx.tab_mode.for_child(),
            mouse_coordinate_space: ctx.mouse_coordinate_space.for_child(),
            drag: ctx.drag,
        }
    }
}

impl<A: Clone + Send + 'static> Component for PickerView<A> {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        self.refresh_filter();
        if area.width == 0 || area.height == 0 {
            return;
        }

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(ctx.theme.border_set(false))
            .title(self.title.as_str())
            .style(ctx.theme.window_bg);
        frame.render_widget(block, area);

        let input_area = picker_input_area(area);
        if input_area.height > 0 && input_area.width > 0 {
            self.input.draw(frame, input_area, Self::child_ctx(ctx));
        }

        let list_area = picker_list_area(area);
        self.draw_list(frame, list_area, ctx);
    }
}

impl<A: Clone + Send + 'static> Layout for PickerView<A> {
    fn min_width(&self) -> u16 {
        32
    }

    fn min_height(&self) -> u16 {
        8
    }
}

impl<A: Clone + Send + 'static> FocusNav for PickerView<A> {
    fn is_focusable(&self) -> bool {
        true
    }
}

impl<A: Clone + Send + 'static> EventHandling for PickerView<A> {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.refresh_filter();
        match event {
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) => {
                if matches!(kind, KeyEventKind::Release) {
                    return EventResult::ignored();
                }
                let page = self.visible_capacity().max(1) as isize;
                match code {
                    KeyCode::Esc => self.close(),
                    KeyCode::Enter => self.accept_selected(),
                    KeyCode::Up if !modifiers.contains(KeyModifiers::CONTROL) => {
                        self.move_selection(-1)
                    }
                    KeyCode::Down if !modifiers.contains(KeyModifiers::CONTROL) => {
                        self.move_selection(1)
                    }
                    KeyCode::PageUp => self.move_selection(-page),
                    KeyCode::PageDown => self.move_selection(page),
                    _ => {
                        let before = self.query.get();
                        let result = self.input.handle_event(event, Self::child_ctx(ctx));
                        if self.query.get() != before {
                            self.selected = 0;
                            self.scroll = 0;
                            self.invalidate_filter();
                            self.refresh_filter();
                            EventResult::changed()
                        } else {
                            result
                        }
                    }
                }
            }
            _ => self.input.handle_event(event, Self::child_ctx(ctx)),
        }
    }
}

impl<A: Clone + Send + 'static> Scrollable for PickerView<A> {}
impl<A: Clone + Send + 'static> atto_ui::composable::DynamicTree for PickerView<A> {}
impl<A: Clone + Send + 'static> atto_ui::composable::DragAndDrop for PickerView<A> {}

impl<A> PickerView<A> {
    fn draw_list(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        self.clamp_scroll(area.height as usize);

        if self.filtered.is_empty() {
            let line = Line::from(vec![Span::styled("  No matches", ctx.theme.widget.dim)]);
            frame.render_widget(Paragraph::new(line).style(ctx.theme.window_bg), area);
            return;
        }

        let visible = area.height as usize;
        for row in 0..visible {
            let row_area = Rect {
                x: area.x,
                y: area.y.saturating_add(row as u16),
                width: area.width,
                height: 1,
            };
            let Some(filtered_index) = self.filtered.get(self.scroll + row).copied() else {
                frame.render_widget(
                    Paragraph::new(Line::raw("")).style(ctx.theme.window_bg),
                    row_area,
                );
                continue;
            };
            let item = &self.items[filtered_index];
            let is_selected = self.scroll + row == self.selected;
            let style = if is_selected {
                ctx.theme.selection
            } else {
                ctx.theme.window_bg
            };
            frame.render_widget(
                Paragraph::new(picker_item_line(item, is_selected, ctx)).style(style),
                row_area,
            );
        }
    }
}

fn picker_input_area(area: Rect) -> Rect {
    let inner = inner_rect(area);
    Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: inner.height.min(3),
    }
}

fn picker_list_area(area: Rect) -> Rect {
    let inner = inner_rect(area);
    let input_height = inner.height.min(3);
    Rect {
        x: inner.x,
        y: inner.y.saturating_add(input_height),
        width: inner.width,
        height: inner.height.saturating_sub(input_height),
    }
}

fn inner_rect(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

fn picker_item_line<A>(
    item: &PickerItem<A>,
    selected: bool,
    ctx: ComponentContext<'_>,
) -> Line<'static> {
    let prefix = if selected { "> " } else { "  " };
    if selected {
        return Line::raw(format!(
            "{prefix}{}{}{}",
            item.title,
            subtitle_text(item),
            shortcut_text(item)
        ));
    }

    let mut spans = vec![
        Span::styled(prefix.to_string(), ctx.theme.widget.accent),
        Span::styled(item.title.clone(), ctx.theme.window_bg),
    ];
    if !item.subtitle.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(item.subtitle.clone(), ctx.theme.widget.dim));
    }
    if let Some(shortcut) = &item.shortcut
        && !shortcut.is_empty()
    {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(shortcut.clone(), ctx.theme.widget.accent));
    }
    Line::from(spans)
}

fn subtitle_text<A>(item: &PickerItem<A>) -> String {
    if item.subtitle.is_empty() {
        String::new()
    } else {
        format!("  {}", item.subtitle)
    }
}

fn shortcut_text<A>(item: &PickerItem<A>) -> String {
    match &item.shortcut {
        Some(shortcut) if !shortcut.is_empty() => format!("  {shortcut}"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atto_ui::composable::{MouseCoordinateSpace, ScrollbarHost, TabMode};
    use atto_ui::theme::Theme;
    use atto_ui::wm::WindowId;

    fn context(theme: &Theme) -> ComponentContext<'_> {
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

    fn picker() -> PickerView<&'static str> {
        PickerView::new(
            "Commands",
            vec![
                PickerItem::new("Save", "save").subtitle("File"),
                PickerItem::new("Save As", "save-as").subtitle("File"),
                PickerItem::new("Toggle Explorer", "toggle").subtitle("View"),
            ],
            EventQueue::new(),
        )
    }

    #[test]
    fn picker_filters_query_with_stable_tie_order() {
        let mut picker = picker();

        picker.set_query("save");

        assert_eq!(picker.filtered_titles(), vec!["Save", "Save As"]);
    }

    #[test]
    fn picker_uses_fuzzy_match_for_item_search_text() {
        let item = PickerItem::new("Toggle Explorer", "toggle")
            .subtitle("View")
            .shortcut("Ctrl+Alt+K Ctrl+Alt+E");

        assert!(picker_item_matches(&item, "tex"));
        assert!(!picker_item_matches(&item, "zzz"));
    }

    #[test]
    fn picker_navigation_clamps_selection_to_filtered_items() {
        let theme = Theme::dark();
        let mut picker = picker();
        picker.set_query("save");

        picker.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
            context(&theme),
        );
        assert_eq!(picker.selected_filtered_index(), Some(1));

        picker.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
            context(&theme),
        );
        assert_eq!(picker.selected_filtered_index(), Some(1));

        picker.set_query("toggle");
        assert_eq!(picker.selected_filtered_index(), Some(0));
        assert_eq!(picker.filtered_titles(), vec!["Toggle Explorer"]);
    }

    #[test]
    fn picker_enter_emits_accept_and_closes() {
        let theme = Theme::dark();
        let events = EventQueue::new();
        let mut picker = PickerView::new(
            "Commands",
            vec![PickerItem::new("Save", "save")],
            events.clone(),
        );

        let result = picker.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            context(&theme),
        );

        assert_eq!(result, EventResult::close_window());
        assert_eq!(events.drain(), vec![PickerEvent::Accepted("save")]);
    }

    #[test]
    fn picker_can_submit_non_empty_query_without_matches() {
        let theme = Theme::dark();
        let events = EventQueue::new();
        let mut picker = PickerView::<&'static str>::new("Search", Vec::new(), events.clone())
            .submit_query_on_empty(true);
        picker.set_query("TODO");

        let result = picker.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            context(&theme),
        );

        assert_eq!(result, EventResult::close_window());
        assert_eq!(events.drain(), vec![PickerEvent::Submitted("TODO".into())]);
    }
}
