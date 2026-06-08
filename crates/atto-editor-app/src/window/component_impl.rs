//! `EditorWindowView` trait implementations.

use atto_ui::composable::{
    ComponentContext, EventResult, ScrollConfig, ScrollbarHost, TitleBarContent, TitleBarContext,
};
use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::EditorWindowView;

impl ::atto_ui::composable::Component for EditorWindowView {
    fn titlebar(&mut self, ctx: TitleBarContext<'_>) -> Option<TitleBarContent> {
        self.tab_window.titlebar(ctx)
    }

    fn handle_titlebar_event(&mut self, event: &Event, ctx: TitleBarContext<'_>) -> EventResult {
        let result = self.tab_window.handle_titlebar_event(event, ctx);
        self.sync_active_diagnostics_summary();
        self.sync_active_status();
        self.sync_tab_summaries();
        result
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.handle_commands();
        self.update_tab_titles();
        self.sync_editor_events();
        self.sync_active_diagnostics_summary();
        self.sync_active_status();
        self.sync_tab_summaries();

        let child_ctx = ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused,
            scrollbar_host: if matches!(ctx.scrollbar_host, ScrollbarHost::Window) {
                ScrollbarHost::Window
            } else {
                ctx.scrollbar_host.for_child()
            },
            tab_mode: ctx.tab_mode.for_child(),
            mouse_coordinate_space: ctx.mouse_coordinate_space,
            drag: None,
        };
        self.tab_window.draw(frame, area, child_ctx);
        self.sync_editor_events();
        self.sync_active_diagnostics_summary();
        self.sync_active_status();
        self.sync_tab_summaries();
    }
}

impl ::atto_ui::composable::DragAndDrop for EditorWindowView {}

impl ::atto_ui::composable::Layout for EditorWindowView {
    fn min_width(&self) -> u16 {
        self.tab_window.min_width()
    }

    fn min_height(&self) -> u16 {
        self.tab_window.min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.tab_window.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.tab_window.desired_height()
    }
}

impl ::atto_ui::composable::Scrollable for EditorWindowView {
    fn is_scrollable(&self) -> bool {
        self.tab_window.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.tab_window.content_size()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.tab_window.scroll_offset()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.tab_window.viewport_size()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.tab_window.scroll_config()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.tab_window.set_scroll_offset(x, y);
    }

    fn scroll_to_child(&mut self, child_id: atto_ui::composable::ComponentId) {
        self.tab_window.scroll_to_child(child_id);
    }
}

impl ::atto_ui::composable::FocusNav for EditorWindowView {
    fn is_focusable(&self) -> bool {
        self.tab_window.is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        self.tab_window.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.tab_window.focus_last()
    }
}

impl ::atto_ui::composable::DynamicTree for EditorWindowView {}

impl ::atto_ui::composable::EventHandling for EditorWindowView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let child_ctx = ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused,
            scrollbar_host: if matches!(ctx.scrollbar_host, ScrollbarHost::Window) {
                ScrollbarHost::Window
            } else {
                ctx.scrollbar_host.for_child()
            },
            tab_mode: ctx.tab_mode.for_child(),
            mouse_coordinate_space: ctx.mouse_coordinate_space,
            drag: None,
        };
        let result = self.tab_window.handle_event(event, child_ctx);
        self.sync_editor_events();
        self.sync_active_diagnostics_summary();
        self.sync_active_status();
        self.sync_tab_summaries();
        result
    }
}
