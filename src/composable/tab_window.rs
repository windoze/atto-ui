use crossterm::event::{Event, MouseButton, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use atto_ui_macros::{Automatable, automate_component};
use super::{
    Component, ComponentContext, EventResult, TitleBarContent, TitleBarContext, TitleBarSpan,
};

#[derive(Automatable)]
pub struct TabWindowTab {
    pub title: String,
    pub view: Box<dyn Component>,
}

#[derive(Automatable)]
pub struct TabWindow {
    tabs: Vec<TabWindowTab>,
    active: usize,
}

impl TabWindow {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
        }
    }

    pub fn with_tab(title: impl Into<String>, view: Box<dyn Component>) -> Self {
        let mut out = Self::new();
        out.add_tab(title, view);
        out
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn active_tab(&self) -> Option<usize> {
        if self.tabs.is_empty() {
            None
        } else {
            Some(self.active.min(self.tabs.len().saturating_sub(1)))
        }
    }

    pub fn add_tab(&mut self, title: impl Into<String>, view: Box<dyn Component>) -> usize {
        let index = self.tabs.len();
        self.tabs.push(TabWindowTab {
            title: title.into(),
            view,
        });
        if self.tabs.len() == 1 {
            self.active = 0;
            let _ = self.tabs[0].view.focus_first();
        }
        index
    }

    pub fn insert_tab(
        &mut self,
        index: usize,
        title: impl Into<String>,
        view: Box<dyn Component>,
    ) -> usize {
        let idx = index.min(self.tabs.len());
        self.tabs.insert(
            idx,
            TabWindowTab {
                title: title.into(),
                view,
            },
        );
        if self.tabs.len() == 1 {
            self.active = 0;
            let _ = self.tabs[0].view.focus_first();
        } else if idx <= self.active {
            self.active += 1;
        }
        idx
    }

    pub fn remove_tab(&mut self, index: usize) -> Option<TabWindowTab> {
        if index >= self.tabs.len() {
            return None;
        }
        let removed = self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active = 0;
            return Some(removed);
        }
        if self.active == index {
            self.active = self.active.min(self.tabs.len().saturating_sub(1));
            let _ = self.tabs[self.active].view.focus_first();
        } else if index < self.active {
            self.active = self.active.saturating_sub(1);
        }
        Some(removed)
    }

    pub fn set_tab_title(&mut self, index: usize, title: impl Into<String>) -> bool {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.title = title.into();
            true
        } else {
            false
        }
    }

    pub fn select_tab(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() || self.active == index {
            return false;
        }
        self.active = index;
        let _ = self.tabs[self.active].view.focus_first();
        true
    }

    fn active_view(&self) -> Option<&(dyn Component + '_)> {
        if let Some(tab) = self.tabs.get(self.active) {
            Some(tab.view.as_ref())
        } else {
            None
        }
    }

    fn active_view_mut(&mut self) -> Option<&mut (dyn Component + '_)> {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            Some(tab.view.as_mut())
        } else {
            None
        }
    }

    fn titlebar_layout(&self, ctx: &TitleBarContext<'_>) -> Option<TabTitlebarLayout> {
        if self.tabs.is_empty() || ctx.area.width == 0 {
            return None;
        }

        let theme = ctx.theme;
        let sep = theme.glyph("tab-separator").unwrap_or("|");
        let active_left = theme.glyph("tab-active-left").unwrap_or(">");
        let active_right = theme.glyph("tab-active-right").unwrap_or("<");

        let fallback_active = if ctx.is_focused {
            theme.window_title_focused
        } else {
            theme.window_title
        };
        let fallback_inactive = theme.window_title;

        let active_style = theme
            .named_style("tab-title-active")
            .unwrap_or(fallback_active);
        let inactive_style = theme
            .named_style("tab-title-inactive")
            .unwrap_or(fallback_inactive);
        let sep_style = theme
            .named_style("tab-title-separator")
            .unwrap_or(fallback_inactive);
        let marker_style = theme
            .named_style("tab-title-marker")
            .unwrap_or(fallback_active);

        let active_style = theme.window_bg.patch(active_style);
        let inactive_style = theme.window_bg.patch(inactive_style);
        let sep_style = theme.window_bg.patch(sep_style);
        let marker_style = theme.window_bg.patch(marker_style);

        let mut builder = TitleBarBuilder::new(ctx.area.width);
        let mut hits = Vec::new();

        if !builder.push_sep(sep, sep_style) {
            return Some(TabTitlebarLayout {
                content: builder.finish(),
                hits,
            });
        }

        for (idx, tab) in self.tabs.iter().enumerate() {
            if idx > 0 {
                let (glyph, style) = if idx == self.active {
                    (active_left, marker_style)
                } else if idx == self.active.saturating_add(1) {
                    (active_right, marker_style)
                } else {
                    (sep, sep_style)
                };
                if !builder.push_sep(glyph, style) {
                    break;
                }
            }

            let label_style = if idx == self.active {
                active_style
            } else {
                inactive_style
            };
            if !builder.push_label(&tab.title, label_style, idx, &mut hits) {
                break;
            }
        }

        let _ = builder.push_sep(sep, sep_style);

        Some(TabTitlebarLayout {
            content: builder.finish(),
            hits,
        })
    }
}

impl Default for TabWindow {
    fn default() -> Self {
        Self::new()
    }
}

#[automate_component]
impl Component for TabWindow {
    fn is_focusable(&self) -> bool {
        self.active_view().is_some_and(|v| v.is_focusable())
    }

    fn focus_first(&mut self) -> bool {
        self.active_view_mut().is_some_and(|v| v.focus_first())
    }

    fn focus_last(&mut self) -> bool {
        self.active_view_mut().is_some_and(|v| v.focus_last())
    }

    fn min_width(&self) -> u16 {
        self.tabs
            .iter()
            .map(|t| t.view.min_width())
            .max()
            .unwrap_or(0)
    }

    fn min_height(&self) -> u16 {
        self.tabs
            .iter()
            .map(|t| t.view.min_height())
            .max()
            .unwrap_or(0)
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Some(view) = self.active_view_mut() else {
            return EventResult::ignored();
        };
        view.handle_event(event, ctx)
    }

    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Some(view) = self.active_view_mut() else {
            return EventResult::ignored();
        };
        view.handle_event_capture(event, ctx)
    }

    fn handle_event_bubble(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Some(view) = self.active_view_mut() else {
            return EventResult::ignored();
        };
        view.handle_event_bubble(event, ctx)
    }

    fn titlebar(&mut self, ctx: TitleBarContext<'_>) -> Option<TitleBarContent> {
        self.titlebar_layout(&ctx).map(|layout| layout.content)
    }

    fn handle_titlebar_event(&mut self, event: &Event, ctx: TitleBarContext<'_>) -> EventResult {
        let Event::Mouse(m) = event else {
            return EventResult::ignored();
        };
        if !matches!(m.kind, MouseEventKind::Down(MouseButton::Left)) {
            return EventResult::ignored();
        }
        if ctx.area.width == 0 {
            return EventResult::ignored();
        }
        if m.row != ctx.area.y
            || m.column < ctx.area.x
            || m.column >= ctx.area.x.saturating_add(ctx.area.width)
        {
            return EventResult::ignored();
        }

        let Some(layout) = self.titlebar_layout(&ctx) else {
            return EventResult::ignored();
        };
        let local_x = m.column.saturating_sub(ctx.area.x);
        for hit in layout.hits {
            if local_x >= hit.start && local_x <= hit.end {
                let changed = self.select_tab(hit.index);
                return if changed {
                    EventResult::changed()
                } else {
                    EventResult::consumed()
                };
            }
        }
        EventResult::ignored()
    }

    fn is_scrollable(&self) -> bool {
        self.active_view().is_some_and(|v| v.is_scrollable())
    }

    fn content_size(&self) -> (u16, u16) {
        self.active_view().map_or((0, 0), |v| v.content_size())
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.active_view().map_or((0, 0), |v| v.scroll_offset())
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.active_view().map_or((0, 0), |v| v.viewport_size())
    }

    fn scroll_config(&self) -> super::ScrollConfig {
        self.active_view()
            .map_or(super::ScrollConfig::default(), |v| v.scroll_config())
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        if let Some(view) = self.active_view_mut() {
            view.set_scroll_offset(x, y);
        }
    }

    fn scroll_to_child(&mut self, child_id: super::ComponentId) {
        if let Some(view) = self.active_view_mut() {
            view.scroll_to_child(child_id);
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let Some(view) = self.active_view_mut() else {
            return;
        };
        view.draw(frame, area, ctx);
    }
}

struct TabTitleHit {
    index: usize,
    start: u16,
    end: u16,
}

struct TabTitlebarLayout {
    content: TitleBarContent,
    hits: Vec<TabTitleHit>,
}

struct TitleBarBuilder {
    max_width: u16,
    pos: u16,
    spans: Vec<TitleBarSpan>,
}

impl TitleBarBuilder {
    fn new(max_width: u16) -> Self {
        Self {
            max_width,
            pos: 0,
            spans: Vec::new(),
        }
    }

    fn finish(self) -> TitleBarContent {
        TitleBarContent { spans: self.spans }
    }

    fn push_sep(&mut self, glyph: &str, style: Style) -> bool {
        if !self.push_text(" ", style) {
            return false;
        }
        if !self.push_text(glyph, style) {
            return false;
        }
        if !self.push_text(" ", style) {
            return false;
        }
        true
    }

    fn push_label(
        &mut self,
        text: &str,
        style: Style,
        index: usize,
        hits: &mut Vec<TabTitleHit>,
    ) -> bool {
        let mut start: Option<u16> = None;
        let mut end: u16 = 0;
        let mut fully_written = true;

        for g in text.graphemes(true) {
            let w = (UnicodeWidthStr::width(g) as u16).max(1);
            if self.pos.saturating_add(w) > self.max_width {
                fully_written = false;
                break;
            }
            if start.is_none() {
                start = Some(self.pos);
            }
            end = self.pos.saturating_add(w).saturating_sub(1);
            self.push_grapheme(g, style);
        }

        if let Some(start) = start {
            hits.push(TabTitleHit { index, start, end });
        }

        fully_written
    }

    fn push_text(&mut self, text: &str, style: Style) -> bool {
        let mut fully_written = true;
        for g in text.graphemes(true) {
            let w = (UnicodeWidthStr::width(g) as u16).max(1);
            if self.pos.saturating_add(w) > self.max_width {
                fully_written = false;
                break;
            }
            self.push_grapheme(g, style);
        }
        fully_written
    }

    fn push_grapheme(&mut self, g: &str, style: Style) {
        if self.pos >= self.max_width {
            return;
        }
        if let Some(last) = self.spans.last_mut()
            && last.style == Some(style)
        {
            last.text.push_str(g);
        } else {
            self.spans.push(TitleBarSpan::styled(g, style));
        }
        let w = (UnicodeWidthStr::width(g) as u16).max(1);
        self.pos = self.pos.saturating_add(w);
    }
}
