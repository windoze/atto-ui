//! Typeahead input and command palette widgets backed by the reusable fuzzy matcher.

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::ComponentCommand;
use crate::composable::{
    Component, ComponentContext, EventHandling, EventResult, FocusNav, Layout, Scrollable,
};
use crate::fuzzy::fuzzy_filter;
use crate::reactive::Binding;
use crate::runtime::{CallbackHandle, ComponentValue};
use crate::text::TextBuffer;
use atto_ui_macros::{ComponentProperties, component_properties};

use super::util::{mouse_coords_local_to_area, widget_style};

#[derive(Clone, Debug)]
struct VisibleSuggestion {
    label: String,
}

/// A focused input with a fuzzy-filtered completion popup.
#[derive(Clone, Debug, ComponentProperties)]
pub struct TypeAhead {
    title: Binding<String>,
    query: Binding<String>,
    items: Binding<Vec<String>>,
    enabled: Binding<bool>,
    selection: Binding<usize>,
    accepted: Binding<String>,
    open: Binding<bool>,
    open_on_empty: Binding<bool>,
    placeholder: Option<Binding<String>>,
    height: Binding<u16>,
    max_results: Binding<usize>,
    #[component(skip)]
    buffer: TextBuffer,
    #[component(skip)]
    scroll: u16,
    #[component(skip)]
    last_area: Option<Rect>,
    #[component(skip)]
    on_change_callback: Option<CallbackHandle>,
    #[component(skip)]
    on_accept_callback: Option<CallbackHandle>,
    #[component(skip)]
    on_close_callback: Option<CallbackHandle>,
}

impl TypeAhead {
    /// Creates a typeahead input over a shared query and item list.
    pub fn new(
        title: impl Into<Binding<String>>,
        query: impl Into<Binding<String>>,
        items: impl Into<Binding<Vec<String>>>,
    ) -> Self {
        let query = query.into();
        let initial = query.get();
        Self {
            title: title.into(),
            query,
            items: items.into(),
            enabled: true.into(),
            selection: 0usize.into(),
            accepted: String::new().into(),
            open: false.into(),
            open_on_empty: false.into(),
            placeholder: None,
            height: 8u16.into(),
            max_results: 8usize.into(),
            buffer: TextBuffer::with_text(initial),
            scroll: 0,
            last_area: None,
            on_change_callback: None,
            on_accept_callback: None,
            on_close_callback: None,
        }
    }

    pub fn title(mut self, title: impl Into<Binding<String>>) -> Self {
        self.title = title.into();
        self
    }

    pub fn query(mut self, query: impl Into<Binding<String>>) -> Self {
        self.query = query.into();
        self.buffer.set_text(self.query.get());
        self.selection.set(0);
        self
    }

    pub fn items(mut self, items: impl Into<Binding<Vec<String>>>) -> Self {
        self.items = items.into();
        self.selection.set(0);
        self
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    pub fn selection(mut self, selection: impl Into<Binding<usize>>) -> Self {
        self.selection = selection.into();
        self
    }

    pub fn accepted(mut self, accepted: impl Into<Binding<String>>) -> Self {
        self.accepted = accepted.into();
        self
    }

    pub fn open(mut self, open: impl Into<Binding<bool>>) -> Self {
        self.open = open.into();
        self
    }

    pub fn open_on_empty(mut self, open_on_empty: impl Into<Binding<bool>>) -> Self {
        self.open_on_empty = open_on_empty.into();
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<Binding<String>>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn height(mut self, height: impl Into<Binding<u16>>) -> Self {
        self.height = height.into();
        self
    }

    pub fn max_results(mut self, max_results: impl Into<Binding<usize>>) -> Self {
        self.max_results = max_results.into();
        self
    }

    pub fn on_change_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_change_callback = Some(callback);
        self
    }

    pub fn on_accept_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_accept_callback = Some(callback);
        self
    }

    pub fn on_close_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_close_callback = Some(callback);
        self
    }

    fn emit_change(&self, query: String) {
        if let Some(cb) = &self.on_change_callback {
            cb.emit_with(Some(ComponentValue::String(query)));
        }
    }

    fn emit_accept(&self, value: String) {
        if let Some(cb) = &self.on_accept_callback {
            cb.emit_with(Some(ComponentValue::String(value)));
        }
    }

    fn emit_close(&self) {
        if let Some(cb) = &self.on_close_callback {
            cb.emit();
        }
    }

    fn sync_external_query(&mut self) {
        let external = self.query.get();
        if external != self.buffer.text() {
            self.buffer.set_text(external);
            self.scroll = 0;
            self.selection.set(0);
        }
    }

    fn set_query_from_buffer(&mut self) {
        let query = self.buffer.text().to_string();
        self.query.set(query.clone());
        self.selection.set(0);
        self.open_for_current_query();
        self.emit_change(query);
    }

    fn set_query_text(&mut self, query: String) -> EventResult {
        self.buffer.set_text(query.clone());
        self.query.set(query.clone());
        self.selection.set(0);
        self.scroll = 0;
        self.open_for_current_query();
        self.emit_change(query);
        EventResult::changed()
    }

    fn open_for_current_query(&self) {
        let should_open = !self.buffer.text().is_empty() || self.open_on_empty.get();
        self.open.set(should_open);
    }

    fn should_show_popup(&self) -> bool {
        self.open.get() && (!self.buffer.text().is_empty() || self.open_on_empty.get())
    }

    fn close_popup(&self) -> EventResult {
        if self.open.get() {
            self.open.set(false);
            self.emit_close();
            EventResult::consumed()
        } else {
            EventResult::ignored()
        }
    }

    fn visible_suggestions(&self, capacity: usize) -> Vec<VisibleSuggestion> {
        if capacity == 0 || !self.should_show_popup() {
            return Vec::new();
        }

        let items = self.items.get();
        let limit = capacity.min(self.max_results.get().max(1));
        fuzzy_filter(&items, self.buffer.text(), limit)
            .into_iter()
            .map(|matched| VisibleSuggestion {
                label: matched.candidate.to_string(),
            })
            .collect()
    }

    fn suggestion_capacity(&self) -> usize {
        self.last_area
            .map(suggestion_capacity_for_area)
            .unwrap_or_else(|| self.max_results.get().max(1))
    }

    fn normalized_selection(&self, len: usize) -> Option<usize> {
        if len == 0 {
            return None;
        }
        let selected = self.selection.get().min(len.saturating_sub(1));
        self.selection.set(selected);
        Some(selected)
    }

    fn move_selection(&self, delta: isize) -> EventResult {
        let suggestions = self.visible_suggestions(self.suggestion_capacity());
        let len = suggestions.len();
        if len == 0 {
            self.open_for_current_query();
            return EventResult::ignored();
        }

        self.open.set(true);
        let selected = self.normalized_selection(len).unwrap_or(0);
        let next = if delta < 0 {
            if selected == 0 { len - 1 } else { selected - 1 }
        } else {
            (selected + 1) % len
        };
        self.selection.set(next);
        EventResult::changed()
    }

    fn accept_selected(&mut self) -> EventResult {
        let suggestions = self.visible_suggestions(self.suggestion_capacity());
        let Some(selected) = self.normalized_selection(suggestions.len()) else {
            if self.buffer.text().is_empty() {
                return EventResult::ignored();
            }
            return self.accept_text(self.buffer.text().to_string());
        };
        self.accept_text(suggestions[selected].label.clone())
    }

    fn accept_text(&mut self, value: String) -> EventResult {
        self.buffer.set_text(value.clone());
        self.query.set(value.clone());
        self.accepted.set(value.clone());
        self.selection.set(0);
        self.open.set(false);
        self.scroll = 0;
        self.emit_change(value.clone());
        self.emit_accept(value);
        EventResult::submitted()
    }

    fn clear_query(&mut self) -> EventResult {
        if self.buffer.text().is_empty() {
            self.open_for_current_query();
            return EventResult::consumed();
        }
        self.buffer.set_text("");
        self.query.set(String::new());
        self.selection.set(0);
        self.scroll = 0;
        self.open_for_current_query();
        self.emit_change(String::new());
        EventResult::changed()
    }

    fn insert_text(&mut self, text: &str) -> EventResult {
        if text.is_empty() {
            return EventResult::ignored();
        }
        self.buffer.insert_str(text);
        self.set_query_from_buffer();
        EventResult::changed()
    }

    fn adjust_scroll(&mut self, input_width: u16) {
        if input_width == 0 {
            self.scroll = 0;
            return;
        }
        let cursor_col = self.buffer.cursor_display_col();
        if cursor_col < self.scroll {
            self.scroll = cursor_col;
        } else if cursor_col >= self.scroll.saturating_add(input_width) {
            self.scroll = cursor_col.saturating_sub(input_width.saturating_sub(1));
        }
    }
}

#[component_properties]
impl Component for TypeAhead {
    fn apply_command(&mut self, command: ComponentCommand) -> EventResult {
        match command {
            ComponentCommand::InputText(value) => self.set_query_text(value),
            ComponentCommand::SelectIndex(idx) => {
                let len = self.visible_suggestions(self.suggestion_capacity()).len();
                if len == 0 {
                    return EventResult::ignored();
                }
                self.selection.set(idx.min(len.saturating_sub(1)));
                EventResult::changed()
            }
            ComponentCommand::Submit => self.accept_selected(),
            _ => EventResult::ignored(),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        self.sync_external_query();
        let enabled = self.enabled.get();
        let style = widget_style(ctx.theme, enabled, ctx.is_focused);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(ctx.theme.border_set(false))
            .title(self.title.get())
            .style(style);
        frame.render_widget(block, area);

        let inner = Rect {
            x: area.x.saturating_add(1),
            y: area.y.saturating_add(1),
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        self.draw_input(frame, inner, ctx, style);
        self.draw_popup(frame, inner, ctx, style);
    }
}

impl Layout for TypeAhead {
    fn min_width(&self) -> u16 {
        12
    }

    fn min_height(&self) -> u16 {
        3
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.height.get().max(3))
    }
}

impl FocusNav for TypeAhead {
    fn is_focusable(&self) -> bool {
        self.enabled.get()
    }
}

impl EventHandling for TypeAhead {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }
        self.sync_external_query();

        match event {
            Event::Paste(text) => self.insert_text(text),
            Event::Mouse(mouse) => {
                let Some(area) = self.last_area else {
                    return EventResult::ignored();
                };
                let Some((local_x, local_y)) =
                    mouse_coords_local_to_area(area, *mouse, ctx.mouse_coordinate_space)
                else {
                    return EventResult::ignored();
                };
                let inner = Rect {
                    x: 1,
                    y: 1,
                    width: area.width.saturating_sub(2),
                    height: area.height.saturating_sub(2),
                };
                if inner.width == 0 || inner.height == 0 {
                    return EventResult::ignored();
                }
                if local_x < inner.x
                    || local_x >= inner.x.saturating_add(inner.width)
                    || local_y < inner.y
                    || local_y >= inner.y.saturating_add(inner.height)
                {
                    return EventResult::ignored();
                }

                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) if local_y == inner.y => {
                        let prefix_width = 2;
                        let col = local_x
                            .saturating_sub(inner.x.saturating_add(prefix_width))
                            .saturating_add(self.scroll);
                        self.buffer.set_cursor_display_col(col);
                        self.open_for_current_query();
                        EventResult::consumed()
                    }
                    MouseEventKind::Down(MouseButton::Left) if local_y > inner.y => {
                        let row = local_y.saturating_sub(inner.y).saturating_sub(1) as usize;
                        let suggestions =
                            self.visible_suggestions(suggestion_capacity_for_area(area));
                        if row < suggestions.len() {
                            self.selection.set(row);
                            self.accept_text(suggestions[row].label.clone())
                        } else {
                            EventResult::ignored()
                        }
                    }
                    _ => EventResult::ignored(),
                }
            }
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) => {
                if matches!(kind, KeyEventKind::Release) {
                    return EventResult::ignored();
                }
                let mods = *modifiers;
                match code {
                    KeyCode::Esc => self.close_popup(),
                    KeyCode::Up if !mods.contains(KeyModifiers::CONTROL) => self.move_selection(-1),
                    KeyCode::Down if !mods.contains(KeyModifiers::CONTROL) => {
                        self.move_selection(1)
                    }
                    KeyCode::Enter => self.accept_selected(),
                    KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => {
                        self.clear_query()
                    }
                    KeyCode::Backspace => {
                        self.buffer.backspace();
                        self.set_query_from_buffer();
                        EventResult::changed()
                    }
                    KeyCode::Delete => {
                        self.buffer.delete();
                        self.set_query_from_buffer();
                        EventResult::changed()
                    }
                    KeyCode::Left => {
                        self.buffer.move_left();
                        EventResult::consumed()
                    }
                    KeyCode::Right => {
                        self.buffer.move_right();
                        EventResult::consumed()
                    }
                    KeyCode::Home => {
                        self.buffer.move_home();
                        EventResult::consumed()
                    }
                    KeyCode::End => {
                        self.buffer.move_end();
                        EventResult::consumed()
                    }
                    KeyCode::Char(c)
                        if !mods.contains(KeyModifiers::CONTROL)
                            && !mods.contains(KeyModifiers::ALT) =>
                    {
                        self.buffer.insert_char(*c);
                        self.set_query_from_buffer();
                        EventResult::changed()
                    }
                    _ => EventResult::ignored(),
                }
            }
            _ => EventResult::ignored(),
        }
    }
}

crate::impl_component_default_traits!(TypeAhead => Scrollable, DynamicTree);

impl TypeAhead {
    fn draw_input(
        &mut self,
        frame: &mut Frame<'_>,
        inner: Rect,
        ctx: ComponentContext<'_>,
        style: ratatui::style::Style,
    ) {
        let input_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        };
        let prefix = "> ";
        let prefix_width = display_width(prefix);
        let input_width = inner.width.saturating_sub(prefix_width);
        self.adjust_scroll(input_width);

        let text = self.buffer.text();
        let spans = if text.is_empty() {
            let placeholder = self
                .placeholder
                .as_ref()
                .map(|binding| binding.get())
                .filter(|value| !value.is_empty())
                .unwrap_or_default();
            vec![
                Span::styled(prefix, ctx.theme.widget.accent),
                Span::styled(
                    slice_by_width(&placeholder, 0, input_width),
                    ctx.theme.widget.dim,
                ),
            ]
        } else {
            vec![
                Span::styled(prefix, ctx.theme.widget.accent),
                Span::styled(slice_by_width(text, self.scroll, input_width), style),
            ]
        };
        frame.render_widget(Paragraph::new(Line::from(spans)).style(style), input_area);

        if ctx.is_focused && input_width > 0 {
            let cursor_col = self
                .buffer
                .cursor_display_col()
                .saturating_sub(self.scroll)
                .min(input_width.saturating_sub(1));
            frame.set_cursor_position((
                inner
                    .x
                    .saturating_add(prefix_width)
                    .saturating_add(cursor_col),
                inner.y,
            ));
        }
    }

    fn draw_popup(
        &mut self,
        frame: &mut Frame<'_>,
        inner: Rect,
        ctx: ComponentContext<'_>,
        style: ratatui::style::Style,
    ) {
        let capacity = inner.height.saturating_sub(1) as usize;
        if capacity == 0 || !self.should_show_popup() {
            return;
        }

        let suggestions = self.visible_suggestions(capacity);
        let selected = self.normalized_selection(suggestions.len());
        if suggestions.is_empty() {
            let area = Rect {
                x: inner.x,
                y: inner.y.saturating_add(1),
                width: inner.width,
                height: 1,
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![Span::styled(
                    "  No matches",
                    ctx.theme.widget.dim,
                )]))
                .style(style),
                area,
            );
            return;
        }

        for (row, suggestion) in suggestions.iter().enumerate() {
            let row_area = Rect {
                x: inner.x,
                y: inner.y.saturating_add(1).saturating_add(row as u16),
                width: inner.width,
                height: 1,
            };
            let is_selected = selected == Some(row);
            let row_style = if is_selected {
                ctx.theme.selection
            } else {
                style
            };
            let prefix = if is_selected { "> " } else { "  " };
            let text = format!("{prefix}{}", suggestion.label);
            frame.render_widget(
                Paragraph::new(Line::raw(slice_by_width(&text, 0, inner.width))).style(row_style),
                row_area,
            );
        }
    }
}

/// Command palette built from a typeahead input and an always-open command list.
#[derive(Clone, Debug, ComponentProperties)]
pub struct CommandPalette {
    #[component(delegate)]
    inner: TypeAhead,
}

impl CommandPalette {
    /// Creates a command palette over a shared query and command list.
    pub fn new(
        title: impl Into<Binding<String>>,
        query: impl Into<Binding<String>>,
        commands: impl Into<Binding<Vec<String>>>,
    ) -> Self {
        Self {
            inner: TypeAhead::new(title, query, commands)
                .placeholder("Type a command, /command, or @file")
                .open(true)
                .open_on_empty(true),
        }
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.inner = self.inner.enabled(enabled);
        self
    }

    pub fn selection(mut self, selection: impl Into<Binding<usize>>) -> Self {
        self.inner = self.inner.selection(selection);
        self
    }

    pub fn accepted(mut self, accepted: impl Into<Binding<String>>) -> Self {
        self.inner = self.inner.accepted(accepted);
        self
    }

    pub fn open(mut self, open: impl Into<Binding<bool>>) -> Self {
        self.inner = self.inner.open(open);
        self
    }

    pub fn open_on_empty(mut self, open_on_empty: impl Into<Binding<bool>>) -> Self {
        self.inner = self.inner.open_on_empty(open_on_empty);
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<Binding<String>>) -> Self {
        self.inner = self.inner.placeholder(placeholder);
        self
    }

    pub fn height(mut self, height: impl Into<Binding<u16>>) -> Self {
        self.inner = self.inner.height(height);
        self
    }

    pub fn max_results(mut self, max_results: impl Into<Binding<usize>>) -> Self {
        self.inner = self.inner.max_results(max_results);
        self
    }

    pub fn on_change_callback(mut self, callback: CallbackHandle) -> Self {
        self.inner = self.inner.on_change_callback(callback);
        self
    }

    pub fn on_accept_callback(mut self, callback: CallbackHandle) -> Self {
        self.inner = self.inner.on_accept_callback(callback);
        self
    }

    pub fn on_close_callback(mut self, callback: CallbackHandle) -> Self {
        self.inner = self.inner.on_close_callback(callback);
        self
    }
}

#[component_properties]
impl Component for CommandPalette {
    fn apply_command(&mut self, command: ComponentCommand) -> EventResult {
        self.inner.apply_command(command)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.inner.draw(frame, area, ctx);
    }
}

impl Layout for CommandPalette {
    fn min_width(&self) -> u16 {
        self.inner.min_width()
    }

    fn min_height(&self) -> u16 {
        self.inner.min_height()
    }

    fn desired_height(&self) -> Option<u16> {
        self.inner.desired_height()
    }
}

impl FocusNav for CommandPalette {
    fn is_focusable(&self) -> bool {
        self.inner.is_focusable()
    }
}

impl EventHandling for CommandPalette {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.inner.handle_event(event, ctx)
    }
}

impl Scrollable for CommandPalette {}
crate::impl_component_default_traits!(CommandPalette => DynamicTree);

fn suggestion_capacity_for_area(area: Rect) -> usize {
    area.height.saturating_sub(2).saturating_sub(1) as usize
}

fn display_width(text: &str) -> u16 {
    UnicodeWidthStr::width(text).min(u16::MAX as usize) as u16
}

fn slice_by_width(text: &str, start_col: u16, width: u16) -> String {
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut col: u16 = 0;
    let end = start_col.saturating_add(width);

    for g in text.graphemes(true) {
        let w = (UnicodeWidthStr::width(g) as u16).max(1);
        let next = col.saturating_add(w);
        if next <= start_col {
            col = next;
            continue;
        }
        if col >= end {
            break;
        }
        out.push_str(g);
        col = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    use crate::composable::{MouseCoordinateSpace, ScrollbarHost, TabMode};
    use crate::theme::Theme;
    use crate::wm::WindowId;

    use super::*;

    fn context(theme: &Theme) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Window,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
        }
    }

    #[test]
    fn typeahead_accepts_fuzzy_selection() {
        let theme = Theme::dark();
        let accepted = Binding::new(String::new());
        let mut typeahead = TypeAhead::new(
            "TypeAhead",
            Binding::new("/".to_string()),
            Binding::new(vec!["/open-file".to_string(), "/search-files".to_string()]),
        )
        .accepted(accepted.clone())
        .open(true);
        typeahead.last_area = Some(Rect::new(0, 0, 40, 8));

        typeahead.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            context(&theme),
        );
        let result = typeahead.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            context(&theme),
        );

        assert!(result.is_consumed());
        assert_eq!(accepted.get(), "/search-files");
    }
}
