use std::sync::Arc;

use crossterm::event::Event;
use parking_lot::RwLock;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;

use atto_ui::composable::{
    Component, ComponentContext, EventResult, ScrollConfig, ScrollContainer, ScrollContainerHost,
    ScrollContent, ScrollContentContext, ScrollbarVisibility,
};
use atto_ui::reactive::Binding;

use super::{LinkCallback, MarkdownShared};

/// Markdown viewer component.
pub struct MarkdownViewer {
    pub(super) shared: Arc<RwLock<MarkdownShared>>,
    scroll_config: Binding<ScrollConfig>,
    scroll: ScrollContainer,
}

impl MarkdownViewer {
    pub fn new(markdown: impl Into<Binding<String>>) -> Self {
        let markdown = markdown.into();
        let width = Binding::new(None);
        let show_markers = Binding::new(false);
        let vertical_scrollbar = Binding::new(ScrollbarVisibility::Auto);
        let max_code_height = Binding::new(super::DEFAULT_CODE_BLOCK_MAX_HEIGHT);
        let max_table_height = Binding::new(super::DEFAULT_TABLE_MAX_HEIGHT);
        let fg_override = Binding::new(None);
        let bg_override = Binding::new(None);
        let link_callback = LinkCallback::new();

        let shared = Arc::new(RwLock::new(MarkdownShared::new(
            markdown.clone(),
            width.clone(),
            show_markers.clone(),
            vertical_scrollbar.clone(),
            max_code_height.clone(),
            max_table_height.clone(),
            fg_override.clone(),
            bg_override.clone(),
            link_callback.clone(),
        )));

        let scroll_config = Binding::new(
            ScrollConfig::default()
                .vertical_scrollbar(vertical_scrollbar.get())
                .horizontal_scrollbar(ScrollbarVisibility::Never),
        );
        let content = MarkdownContent {
            shared: shared.clone(),
        };
        let scroll =
            ScrollContainer::new(Box::new(content)).with_scroll_config(scroll_config.clone());

        Self {
            shared,
            scroll_config,
            scroll,
        }
    }

    pub fn markdown(self, markdown: impl Into<String>) -> Self {
        self.shared.write().markdown.set(markdown.into());
        self
    }

    pub fn wrap_width(self, width: u16) -> Self {
        self.shared.write().width.set(Some(width));
        self
    }

    pub fn width(self, width: u16) -> Self {
        self.wrap_width(width)
    }

    pub fn show_markers(self, show: bool) -> Self {
        self.shared.write().show_markers.set(show);
        self
    }

    pub fn vertical_scrollbar(self, vis: ScrollbarVisibility) -> Self {
        self.shared.write().vertical_scrollbar.set(vis);
        self.scroll_config.update(|cfg| {
            cfg.vertical_scrollbar = vis;
        });
        self
    }

    pub fn code_block_max_height(self, height: u16) -> Self {
        self.shared.write().max_code_height.set(height);
        self
    }

    pub fn table_max_height(self, height: u16) -> Self {
        self.shared.write().max_table_height.set(height);
        self
    }

    pub fn text_color(self, color: Color) -> Self {
        self.shared.write().fg_override.set(Some(color));
        self
    }

    pub fn background(self, color: Color) -> Self {
        self.shared.write().bg_override.set(Some(color));
        self
    }

    pub fn on_link<F>(self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.shared
            .write()
            .link_callback
            .set(Some(Arc::new(callback)));
        self
    }
}

impl Component for MarkdownViewer {
    fn is_focusable(&self) -> bool {
        self.scroll.is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        self.scroll.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.scroll.focus_last()
    }

    fn min_width(&self) -> u16 {
        self.scroll.min_width()
    }

    fn min_height(&self) -> u16 {
        self.scroll.min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.scroll.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.scroll.desired_height()
    }

    fn is_scrollable(&self) -> bool {
        self.scroll.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.scroll.content_size()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.scroll.viewport_size()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.scroll.scroll_config()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.scroll.scroll_offset()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.scroll.set_scroll_offset(x, y);
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.scroll.handle_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.scroll.draw(frame, area, ctx);
    }
}

struct MarkdownContent {
    shared: Arc<RwLock<MarkdownShared>>,
}

impl ScrollContent for MarkdownContent {
    fn desired_width(&self) -> Option<u16> {
        self.shared.read().width.get()
    }

    fn desired_height(&self) -> Option<u16> {
        let mut shared = self.shared.write();
        if shared.vertical_scrollbar.get() != ScrollbarVisibility::Never {
            return None;
        }

        let wrap_width = shared
            .width
            .get()
            .or(shared.cache.last_wrap_width)
            .unwrap_or(0);
        if wrap_width == 0 {
            return None;
        }
        let layout_width = wrap_width.max(1);
        shared.ensure_layout(layout_width);
        shared
            .cache
            .layout
            .as_ref()
            .map(|layout| layout.total_height)
    }

    fn content_size(&mut self, viewport: (u16, u16), _ctx: ScrollContentContext<'_>) -> (u16, u16) {
        let mut shared = self.shared.write();
        let wrap_width = shared.resolve_wrap_width(viewport.0);
        let layout_width = wrap_width.max(1);
        shared.ensure_layout(layout_width);
        let height = shared
            .cache
            .layout
            .as_ref()
            .map(|layout| layout.total_height)
            .unwrap_or(0);
        (wrap_width, height)
    }

    fn handle_event(
        &mut self,
        event: &Event,
        ctx: ScrollContentContext<'_>,
        _host: &mut ScrollContainerHost,
    ) -> EventResult {
        let mut shared = self.shared.write();
        super::events::handle_content_event(&mut shared, event, ctx)
    }

    fn draw(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ScrollContentContext<'_>,
        _host: &mut ScrollContainerHost,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let mut shared = self.shared.write();
        let wrap_width = shared.resolve_wrap_width(area.width);
        let layout_width = wrap_width.max(1);
        shared.ensure_layout(layout_width);

        super::render::draw_content(&mut shared, frame, area, ctx);
    }
}
