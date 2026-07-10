//! Completion popup primitives for chat input overlays.
//!
//! The popup is intentionally independent from slash-command and mention providers. It owns the
//! filtered list rendering, keyboard selection, acceptance, dismissal, and anchor placement that the
//! provider-specific input features can reuse later.

use std::collections::HashSet;
use std::sync::Arc;

use atto_ui::composable::{
    Component, ComponentContext, EventHandling, EventResult, FocusNav, Layout, Scrollable,
};
use atto_ui::fuzzy::fuzzy_filter;
use atto_ui::reactive::{Binding, DirtyObserver};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const DEFAULT_TITLE: &str = "Completions";
const DEFAULT_EMPTY_LABEL: &str = "No matches";
const DEFAULT_MAX_HEIGHT: u16 = 8;
const DEFAULT_MIN_WIDTH: u16 = 18;

/// A single completion candidate shown by [`CompletionPopup`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionItem {
    /// Text displayed in the popup and used for fuzzy matching.
    pub label: String,
    /// Optional secondary text displayed after the label.
    pub detail: Option<String>,
    /// Text that callers should insert or otherwise accept when the item is confirmed.
    pub replacement: String,
}

impl CompletionItem {
    /// Creates an item whose replacement text is the same as its label.
    pub fn new(label: impl Into<String>) -> Self {
        let label = label.into();
        Self {
            replacement: label.clone(),
            label,
            detail: None,
        }
    }

    /// Creates an item with a display label and a distinct accepted replacement.
    pub fn with_replacement(label: impl Into<String>, replacement: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
            replacement: replacement.into(),
        }
    }

    /// Adds secondary explanatory text to the item.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Preferred side of the input anchor for popup placement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompletionPlacement {
    /// Prefer below when there is room; otherwise use the side with more space.
    #[default]
    Auto,
    /// Force the popup above the anchor when any vertical space is available.
    Above,
    /// Force the popup below the anchor when any vertical space is available.
    Below,
}

/// Input-area rectangle and placement preference for the overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompletionAnchor {
    /// Terminal rectangle occupied by the input widget that owns the popup.
    pub rect: Rect,
    /// Preferred placement relative to [`CompletionAnchor::rect`].
    pub placement: CompletionPlacement,
}

impl CompletionAnchor {
    /// Creates an auto-placed anchor for an input rectangle.
    pub fn new(rect: Rect) -> Self {
        Self {
            rect,
            placement: CompletionPlacement::Auto,
        }
    }

    /// Returns this anchor with an explicit placement preference.
    pub fn placement(mut self, placement: CompletionPlacement) -> Self {
        self.placement = placement;
        self
    }
}

impl Default for CompletionAnchor {
    fn default() -> Self {
        Self::new(Rect::new(0, 0, 0, 0))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CompletionMatch {
    item: CompletionItem,
    positions: Vec<usize>,
}

/// Reusable anchored completion popup for chat input affordances.
pub struct CompletionPopup {
    query: Binding<String>,
    items: Binding<Vec<CompletionItem>>,
    open: Binding<bool>,
    selection: Binding<usize>,
    accepted: Binding<Option<CompletionItem>>,
    anchor: Binding<CompletionAnchor>,
    title: Binding<String>,
    empty_label: Binding<String>,
    max_height: Binding<u16>,
    min_width: Binding<u16>,
    scroll: usize,
    last_popup_rect: Option<Rect>,
    query_observer: DirtyObserver,
    items_observer: DirtyObserver,
    on_accept: Option<Arc<dyn Fn(CompletionItem) + Send + Sync>>,
    on_close: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl CompletionPopup {
    /// Creates a popup over a shared query and item list.
    pub fn new(
        query: impl Into<Binding<String>>,
        items: impl Into<Binding<Vec<CompletionItem>>>,
    ) -> Self {
        let query = query.into();
        let items = items.into();
        Self {
            query_observer: query.dirty_observer(),
            items_observer: items.dirty_observer(),
            query,
            items,
            open: false.into(),
            selection: 0usize.into(),
            accepted: Binding::new(None),
            anchor: CompletionAnchor::default().into(),
            title: DEFAULT_TITLE.into(),
            empty_label: DEFAULT_EMPTY_LABEL.into(),
            max_height: DEFAULT_MAX_HEIGHT.into(),
            min_width: DEFAULT_MIN_WIDTH.into(),
            scroll: 0,
            last_popup_rect: None,
            on_accept: None,
            on_close: None,
        }
    }

    /// Replaces the query binding used for fuzzy filtering.
    pub fn query(mut self, query: impl Into<Binding<String>>) -> Self {
        self.query = query.into();
        self.query_observer = self.query.dirty_observer();
        self.reset_viewport();
        self
    }

    /// Replaces the candidate list binding.
    pub fn items(mut self, items: impl Into<Binding<Vec<CompletionItem>>>) -> Self {
        self.items = items.into();
        self.items_observer = self.items.dirty_observer();
        self.reset_viewport();
        self
    }

    /// Sets the open-state binding.
    pub fn open(mut self, open: impl Into<Binding<bool>>) -> Self {
        self.open = open.into();
        self
    }

    /// Sets the selected match index binding.
    pub fn selection(mut self, selection: impl Into<Binding<usize>>) -> Self {
        self.selection = selection.into();
        self
    }

    /// Sets the binding that receives the last accepted item.
    pub fn accepted(mut self, accepted: impl Into<Binding<Option<CompletionItem>>>) -> Self {
        self.accepted = accepted.into();
        self
    }

    /// Sets the input anchor binding used for overlay placement.
    pub fn anchor(mut self, anchor: impl Into<Binding<CompletionAnchor>>) -> Self {
        self.anchor = anchor.into();
        self
    }

    /// Sets the popup title rendered in its border.
    pub fn title(mut self, title: impl Into<Binding<String>>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets the message displayed when the query has no matching candidates.
    pub fn empty_label(mut self, empty_label: impl Into<Binding<String>>) -> Self {
        self.empty_label = empty_label.into();
        self
    }

    /// Sets the maximum popup height, including borders.
    pub fn max_height(mut self, max_height: impl Into<Binding<u16>>) -> Self {
        self.max_height = max_height.into();
        self
    }

    /// Sets the minimum popup width, including borders.
    pub fn min_width(mut self, min_width: impl Into<Binding<u16>>) -> Self {
        self.min_width = min_width.into();
        self
    }

    /// Registers a callback fired after an item is accepted.
    pub fn on_accept<F>(mut self, callback: F) -> Self
    where
        F: Fn(CompletionItem) + Send + Sync + 'static,
    {
        self.on_accept = Some(Arc::new(callback));
        self
    }

    /// Registers a callback fired when the popup is closed with Esc.
    pub fn on_close<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_close = Some(Arc::new(callback));
        self
    }

    /// Returns the popup rectangle used during the most recent draw call.
    pub fn last_popup_rect(&self) -> Option<Rect> {
        self.last_popup_rect
    }

    /// Returns whether the current query has at least one matching candidate.
    pub(crate) fn has_matches(&mut self) -> bool {
        self.sync_query_and_items();
        !self.filtered_items().is_empty()
    }

    fn sync_query_and_items(&mut self) {
        let query_changed = self.query.check_dirty(&mut self.query_observer);
        let items_changed = self.items.check_dirty(&mut self.items_observer);
        if query_changed || items_changed {
            self.reset_viewport();
        }
    }

    fn reset_viewport(&mut self) {
        self.scroll = 0;
        self.selection.set(0);
    }

    fn filtered_items(&self) -> Vec<CompletionMatch> {
        let items = self.items.get();
        if items.is_empty() {
            return Vec::new();
        }

        let labels: Vec<String> = items.iter().map(|item| item.label.clone()).collect();
        fuzzy_filter(&labels, &self.query.get(), labels.len())
            .into_iter()
            .map(|matched| CompletionMatch {
                item: items[matched.index].clone(),
                positions: matched.positions,
            })
            .collect()
    }

    fn normalized_selection(&self, len: usize) -> Option<usize> {
        if len == 0 {
            return None;
        }

        let raw = self.selection.get();
        let selected = raw.min(len.saturating_sub(1));
        if selected != raw {
            self.selection.set(selected);
        }
        Some(selected)
    }

    fn move_selection(&mut self, delta: isize) -> EventResult {
        self.sync_query_and_items();
        if !self.open.get() {
            return EventResult::ignored();
        }

        let matches = self.filtered_items();
        let len = matches.len();
        let Some(selected) = self.normalized_selection(len) else {
            return EventResult::consumed();
        };

        let next = if delta < 0 {
            if selected == 0 { len - 1 } else { selected - 1 }
        } else {
            (selected + 1) % len
        };
        self.selection.set(next);
        self.ensure_selection_visible(next, self.last_capacity(), len);
        EventResult::changed()
    }

    fn accept_selected(&mut self) -> EventResult {
        self.sync_query_and_items();
        if !self.open.get() {
            return EventResult::ignored();
        }

        let matches = self.filtered_items();
        let Some(selected) = self.normalized_selection(matches.len()) else {
            return EventResult::consumed();
        };
        let accepted = matches[selected].item.clone();
        self.accepted.set(Some(accepted.clone()));
        self.open.set(false);
        self.scroll = 0;
        if let Some(callback) = &self.on_accept {
            callback(accepted);
        }
        EventResult::submitted()
    }

    fn close_popup(&mut self) -> EventResult {
        if !self.open.get() {
            return EventResult::ignored();
        }

        self.open.set(false);
        self.scroll = 0;
        if let Some(callback) = &self.on_close {
            callback();
        }
        EventResult::consumed()
    }

    fn last_capacity(&self) -> usize {
        self.last_popup_rect
            .map(|rect| rect.height.saturating_sub(2).max(1) as usize)
            .unwrap_or_else(|| self.max_height.get().saturating_sub(2).max(1) as usize)
    }

    fn ensure_selection_visible(&mut self, selected: usize, capacity: usize, len: usize) {
        if len == 0 || capacity == 0 {
            self.scroll = 0;
            return;
        }

        if selected < self.scroll {
            self.scroll = selected;
        } else if selected >= self.scroll.saturating_add(capacity) {
            self.scroll = selected.saturating_add(1).saturating_sub(capacity);
        }

        let max_scroll = len.saturating_sub(capacity);
        self.scroll = self.scroll.min(max_scroll);
    }

    fn desired_popup_width(&self, matches: &[CompletionMatch], bounds: Rect) -> u16 {
        let anchor_width = self.anchor.get().rect.width;
        let item_width = matches
            .iter()
            .map(|matched| completion_item_width(&matched.item))
            .max()
            .unwrap_or_else(|| display_width(self.empty_label.get().as_str()).saturating_add(2));
        let desired = item_width
            .saturating_add(2)
            .max(anchor_width)
            .max(self.min_width.get().max(1));
        desired.min(bounds.width.max(1))
    }

    fn desired_popup_height(&self, matches: &[CompletionMatch]) -> u16 {
        let row_count = matches.len().max(1).min(u16::MAX as usize) as u16;
        row_count
            .saturating_add(2)
            .min(self.max_height.get().max(3))
            .max(3)
    }

    fn draw_empty_row(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ComponentContext<'_>,
        base: Style,
    ) {
        let text = format!("  {}", self.empty_label.get());
        let line = clip_spans_to_width(vec![Span::styled(text, ctx.theme.widget.dim)], area.width);
        frame.render_widget(Paragraph::new(line).style(base), area);
    }

    fn draw_match_row(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ComponentContext<'_>,
        matched: &CompletionMatch,
        selected: bool,
        base: Style,
    ) {
        let row_style = if selected { ctx.theme.selection } else { base };
        let match_style = if selected {
            row_style.add_modifier(Modifier::UNDERLINED)
        } else {
            base.patch(ctx.theme.widget.accent.add_modifier(Modifier::BOLD))
        };
        let detail_style = if selected {
            row_style
        } else {
            ctx.theme.widget.dim
        };
        let prefix = if selected { "> " } else { "  " };
        let mut spans = vec![Span::styled(prefix.to_string(), row_style)];
        spans.extend(highlight_label_spans(
            &matched.item.label,
            &matched.positions,
            row_style,
            match_style,
        ));
        if let Some(detail) = &matched.item.detail
            && !detail.is_empty()
        {
            spans.push(Span::styled("  ".to_string(), row_style));
            spans.push(Span::styled(detail.clone(), detail_style));
        }
        let line = clip_spans_to_width(spans, area.width);
        frame.render_widget(Paragraph::new(line).style(row_style), area);
    }
}

impl Component for CompletionPopup {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_popup_rect = None;
        self.sync_query_and_items();
        if !self.open.get() || area.width == 0 || area.height == 0 {
            return;
        }

        let matches = self.filtered_items();
        let width = self.desired_popup_width(&matches, area);
        let height = self.desired_popup_height(&matches);
        let Some(popup) = popup_rect(area, self.anchor.get(), width, height) else {
            return;
        };
        self.last_popup_rect = Some(popup);

        frame.render_widget(Clear, popup);
        let base = ctx.theme.window_bg.patch(ctx.theme.widget.normal);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(ctx.theme.border_set(ctx.is_focused))
            .title(self.title.get())
            .style(base);
        frame.render_widget(block, popup);

        let inner = inner_rect(popup);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if matches.is_empty() {
            let row = Rect::new(inner.x, inner.y, inner.width, 1);
            self.draw_empty_row(frame, row, ctx, base);
            return;
        }

        let selected = self.normalized_selection(matches.len()).unwrap_or(0);
        let capacity = inner.height as usize;
        self.ensure_selection_visible(selected, capacity, matches.len());
        let end = self.scroll.saturating_add(capacity).min(matches.len());
        for (row, matched) in matches[self.scroll..end].iter().enumerate() {
            let item_index = self.scroll.saturating_add(row);
            let row_area = Rect::new(
                inner.x,
                inner.y.saturating_add(row.min(u16::MAX as usize) as u16),
                inner.width,
                1,
            );
            self.draw_match_row(frame, row_area, ctx, matched, item_index == selected, base);
        }
    }
}

impl Layout for CompletionPopup {
    fn min_width(&self) -> u16 {
        self.min_width.get().max(1)
    }

    fn min_height(&self) -> u16 {
        3
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.max_height.get().max(3))
    }
}

impl FocusNav for CompletionPopup {
    fn is_focusable(&self) -> bool {
        self.open.get()
    }
}

impl EventHandling for CompletionPopup {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        match event {
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) if !matches!(kind, KeyEventKind::Release) => match code {
                KeyCode::Esc => self.close_popup(),
                KeyCode::Up if !modifiers.contains(KeyModifiers::CONTROL) => {
                    self.move_selection(-1)
                }
                KeyCode::Down if !modifiers.contains(KeyModifiers::CONTROL) => {
                    self.move_selection(1)
                }
                KeyCode::Enter => self.accept_selected(),
                _ => EventResult::ignored(),
            },
            _ => EventResult::ignored(),
        }
    }
}

impl Scrollable for CompletionPopup {}
impl atto_ui::composable::DragAndDrop for CompletionPopup {}
impl atto_ui::composable::DynamicTree for CompletionPopup {}

fn completion_item_width(item: &CompletionItem) -> u16 {
    let label_width = display_width(&item.label);
    let detail_width = item
        .detail
        .as_deref()
        .filter(|detail| !detail.is_empty())
        .map(|detail| display_width(detail).saturating_add(2))
        .unwrap_or(0);
    label_width.saturating_add(detail_width).saturating_add(2)
}

fn display_width(text: &str) -> u16 {
    UnicodeWidthStr::width(text).min(u16::MAX as usize) as u16
}

fn popup_rect(bounds: Rect, anchor: CompletionAnchor, width: u16, height: u16) -> Option<Rect> {
    if bounds.width == 0 || bounds.height == 0 {
        return None;
    }

    let width = width.clamp(1, bounds.width);
    let height = height.max(1);
    let bounds_right = rect_right(bounds);
    let bounds_bottom = rect_bottom(bounds);
    let max_x = bounds_right.saturating_sub(width);
    let x = anchor.rect.x.clamp(bounds.x, max_x);
    let above_space = anchor.rect.y.saturating_sub(bounds.y);
    let below_space = bounds_bottom.saturating_sub(rect_bottom(anchor.rect));

    let mut placement = match anchor.placement {
        CompletionPlacement::Auto => {
            if below_space >= height || below_space >= above_space {
                CompletionPlacement::Below
            } else {
                CompletionPlacement::Above
            }
        }
        forced => forced,
    };
    if placement == CompletionPlacement::Below && below_space == 0 && above_space > 0 {
        placement = CompletionPlacement::Above;
    } else if placement == CompletionPlacement::Above && above_space == 0 && below_space > 0 {
        placement = CompletionPlacement::Below;
    }

    let available = match placement {
        CompletionPlacement::Above => above_space,
        CompletionPlacement::Below | CompletionPlacement::Auto => below_space,
    };
    if available == 0 {
        return None;
    }

    let height = height.min(available).min(bounds.height).max(1);
    let y = match placement {
        CompletionPlacement::Above => anchor.rect.y.saturating_sub(height).max(bounds.y),
        CompletionPlacement::Below | CompletionPlacement::Auto => {
            rect_bottom(anchor.rect).min(bounds_bottom.saturating_sub(height))
        }
    };
    Some(Rect::new(x, y, width, height))
}

fn rect_right(rect: Rect) -> u16 {
    rect.x.saturating_add(rect.width)
}

fn rect_bottom(rect: Rect) -> u16 {
    rect.y.saturating_add(rect.height)
}

fn inner_rect(rect: Rect) -> Rect {
    Rect::new(
        rect.x.saturating_add(1),
        rect.y.saturating_add(1),
        rect.width.saturating_sub(2),
        rect.height.saturating_sub(2),
    )
}

fn highlight_label_spans(
    label: &str,
    positions: &[usize],
    normal: Style,
    matched: Style,
) -> Vec<Span<'static>> {
    let position_set: HashSet<usize> = positions.iter().copied().collect();
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut current = String::new();
    let mut current_style = normal;
    let mut has_current = false;

    for (byte_idx, grapheme) in label.grapheme_indices(true) {
        let end = byte_idx.saturating_add(grapheme.len());
        let style = if position_set
            .iter()
            .any(|position| *position >= byte_idx && *position < end)
        {
            matched
        } else {
            normal
        };

        if has_current && style != current_style {
            out.push(Span::styled(std::mem::take(&mut current), current_style));
        }
        current_style = style;
        has_current = true;
        current.push_str(grapheme);
    }

    if has_current {
        out.push(Span::styled(current, current_style));
    }
    out
}

fn clip_spans_to_width(spans: Vec<Span<'static>>, width: u16) -> Line<'static> {
    if width == 0 {
        return Line::raw("");
    }

    let mut out = Vec::new();
    let mut used = 0u16;
    'spans: for span in spans {
        let mut text = String::new();
        for grapheme in span.content.as_ref().graphemes(true) {
            let grapheme_width = UnicodeWidthStr::width(grapheme)
                .max(1)
                .min(u16::MAX as usize) as u16;
            if used.saturating_add(grapheme_width) > width {
                if !text.is_empty() {
                    out.push(Span::styled(text, span.style));
                }
                break 'spans;
            }
            used = used.saturating_add(grapheme_width);
            text.push_str(grapheme);
        }
        if !text.is_empty() {
            out.push(Span::styled(text, span.style));
        }
    }

    Line::from(out)
}

#[cfg(test)]
mod tests {
    use atto_ui::composable::{MouseCoordinateSpace, ScrollbarHost, TabMode};
    use atto_ui::theme::Theme;
    use atto_ui::wm::WindowId;
    use crossterm::event::{KeyModifiers, MouseEventKind};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    use super::*;

    fn context(theme: &Theme) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        }
    }

    fn draw_popup(
        popup: &mut CompletionPopup,
        width: u16,
        height: u16,
    ) -> (Vec<String>, Vec<Vec<Color>>) {
        let theme = Theme::dark();
        let ctx = context(&theme);
        let backend = TestBackend::new(width.max(1), height.max(1));
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| popup.draw(frame, Rect::new(0, 0, width, height), ctx))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let mut lines = Vec::new();
        let mut fgs = Vec::new();
        for y in 0..height {
            let mut line = String::new();
            let mut row_fgs = Vec::new();
            for x in 0..width {
                let cell = buffer.cell((x, y)).expect("cell");
                line.push_str(cell.symbol());
                row_fgs.push(cell.fg);
            }
            lines.push(line);
            fgs.push(row_fgs);
        }
        (lines, fgs)
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn keyboard_moves_accepts_and_closes() {
        let theme = Theme::dark();
        let open = Binding::new(true);
        let selection = Binding::new(0usize);
        let accepted = Binding::new(None);
        let mut popup = CompletionPopup::new(
            "",
            vec![
                CompletionItem::new("/clear"),
                CompletionItem::new("/model"),
                CompletionItem::new("/review"),
            ],
        )
        .open(open.clone())
        .selection(selection.clone())
        .accepted(accepted.clone())
        .anchor(CompletionAnchor::new(Rect::new(2, 2, 24, 3)));

        assert_eq!(
            popup.handle_event(&key(KeyCode::Down), context(&theme)),
            EventResult::changed()
        );
        assert_eq!(selection.get(), 1);

        assert_eq!(
            popup.handle_event(&key(KeyCode::Enter), context(&theme)),
            EventResult::submitted()
        );
        assert_eq!(
            accepted.get().map(|item: CompletionItem| item.label),
            Some("/model".to_string())
        );
        assert!(!open.get());

        open.set(true);
        assert_eq!(
            popup.handle_event(&key(KeyCode::Esc), context(&theme)),
            EventResult::consumed()
        );
        assert!(!open.get());
    }

    #[test]
    fn empty_candidates_render_message_and_block_submit() {
        let theme = Theme::dark();
        let open = Binding::new(true);
        let accepted = Binding::new(None);
        let mut popup = CompletionPopup::new("zz", vec![CompletionItem::new("/clear")])
            .open(open.clone())
            .accepted(accepted.clone())
            .anchor(CompletionAnchor::new(Rect::new(0, 0, 20, 1)));

        let (lines, _) = draw_popup(&mut popup, 40, 8);
        assert!(lines.iter().any(|line| line.contains(DEFAULT_EMPTY_LABEL)));
        assert_eq!(
            popup.handle_event(&key(KeyCode::Enter), context(&theme)),
            EventResult::consumed()
        );
        assert_eq!(accepted.get(), None);
        assert!(open.get());
    }

    #[test]
    fn long_lists_scroll_selected_candidate_into_view() {
        let theme = Theme::dark();
        let open = Binding::new(true);
        let items = (0..8)
            .map(|idx| CompletionItem::new(format!("item-{idx:02}")))
            .collect::<Vec<_>>();
        let mut popup = CompletionPopup::new("", items)
            .open(open)
            .anchor(CompletionAnchor::new(Rect::new(0, 0, 20, 1)))
            .max_height(5u16);

        let (initial, _) = draw_popup(&mut popup, 30, 10);
        assert!(initial.iter().any(|line| line.contains("item-00")));

        for _ in 0..4 {
            popup.handle_event(&key(KeyCode::Down), context(&theme));
        }
        let (scrolled, _) = draw_popup(&mut popup, 30, 10);
        assert!(scrolled.iter().any(|line| line.contains("item-04")));
        assert!(!scrolled.iter().any(|line| line.contains("item-00")));
    }

    #[test]
    fn auto_anchor_places_popup_above_when_below_space_is_small() {
        let mut popup = CompletionPopup::new(
            "",
            vec![CompletionItem::new("one"), CompletionItem::new("two")],
        )
        .open(true)
        .anchor(CompletionAnchor::new(Rect::new(2, 7, 16, 2)))
        .max_height(5u16);

        draw_popup(&mut popup, 30, 10);
        let rect = popup.last_popup_rect().expect("popup rect");
        assert!(
            rect.y < 7,
            "popup should be placed above the input anchor: {rect:?}"
        );
    }

    #[test]
    fn match_highlighting_uses_accent_without_breaking_wide_chars() {
        let items = vec![
            CompletionItem::new("xxpf 你好"),
            CompletionItem::new("other xx"),
        ];
        let mut popup = CompletionPopup::new("xx", items)
            .open(true)
            .selection(1usize)
            .anchor(CompletionAnchor::new(Rect::new(0, 0, 24, 1)))
            .max_height(5u16);

        let (lines, colors) = draw_popup(&mut popup, 32, 8);
        let row_y = lines
            .iter()
            .position(|line| line.contains("xxpf"))
            .expect("first row should render");
        let row = &lines[row_y];
        assert!(row.contains('你'));
        assert!(row.contains('好'));
        let highlight_byte = row.find('x').expect("match x");
        let normal_byte = row.find('p').expect("normal p");
        let highlight_x = UnicodeWidthStr::width(&row[..highlight_byte]);
        let normal_x = UnicodeWidthStr::width(&row[..normal_byte]);
        assert_ne!(colors[row_y][highlight_x], colors[row_y][normal_x]);
    }

    #[test]
    fn clip_spans_to_width_keeps_wide_graphemes_intact() {
        let line = clip_spans_to_width(vec![Span::raw("ab你c")], 4);
        let text = line_text(&line);
        assert_eq!(text, "ab你");
        assert_eq!(UnicodeWidthStr::width(text.as_str()), 4);
    }

    #[test]
    fn release_and_control_navigation_do_not_capture_events() {
        let theme = Theme::dark();
        let mut popup = CompletionPopup::new("", vec![CompletionItem::new("one")])
            .open(true)
            .anchor(CompletionAnchor::new(Rect::new(0, 0, 10, 1)));
        let release = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Down,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));
        let ctrl_up = Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL));

        assert_eq!(
            popup.handle_event(&release, context(&theme)),
            EventResult::ignored()
        );
        assert_eq!(
            popup.handle_event(&ctrl_up, context(&theme)),
            EventResult::ignored()
        );

        let mouse = Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            popup.handle_event(&mouse, context(&theme)),
            EventResult::ignored()
        );
    }
}
