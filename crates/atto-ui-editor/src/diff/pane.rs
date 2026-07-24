// One side of the side-by-side diff. Both panes share a `DiffSession`, so scrolling either one
// (wheel, keys, or scrollbar drag routed by the `Splitter`) moves both in lockstep.

use atto_ui::composable::{
    Component, ComponentContext, DynamicTree, EventHandling, EventResult, FocusNav, Layout,
    ScrollConfig, Scrollable,
};
use atto_ui::reactive::Binding;
use crossterm::event::{Event, KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;

use super::render::{GutterLayout, gutter_width, line_number_digits, render_column};
use super::session::SharedSession;
use crate::theme::EditorThemeSet;

const WHEEL_STEP: isize = 3;

pub(crate) struct DiffPane {
    shared: SharedSession,
    theme: Binding<EditorThemeSet>,
    column: usize,
    side: usize,
    viewport_rows: usize,
    content_rows: usize,
    viewport_cols: u16,
}

impl DiffPane {
    pub(crate) fn new(
        shared: SharedSession,
        theme: Binding<EditorThemeSet>,
        column: usize,
        side: usize,
    ) -> Self {
        Self {
            shared,
            theme,
            column,
            side,
            viewport_rows: 0,
            content_rows: 0,
            viewport_cols: 0,
        }
    }

    fn gutter_layout(&self, digits: usize) -> GutterLayout {
        GutterLayout::Side {
            side: self.side,
            digits,
        }
    }

    fn scroll_by(&mut self, delta: isize) {
        let rows = self.viewport_rows.max(1);
        self.shared.lock().unwrap().scroll_by(delta, rows);
    }

    fn handle_key(&mut self, key: KeyEvent) -> EventResult {
        let page = self.viewport_rows.max(1) as isize;
        match key.code {
            KeyCode::Up => self.scroll_by(-1),
            KeyCode::Down => self.scroll_by(1),
            KeyCode::PageUp => self.scroll_by(-page),
            KeyCode::PageDown => self.scroll_by(page),
            KeyCode::Home => self
                .shared
                .lock()
                .unwrap()
                .set_scroll_top(0, self.viewport_rows),
            KeyCode::End => {
                let rows = self.viewport_rows.max(1);
                let mut sess = self.shared.lock().unwrap();
                let max = sess.max_scroll_top(rows);
                sess.set_scroll_top(max, rows);
            }
            KeyCode::Char('z') => {
                if !self
                    .shared
                    .lock()
                    .unwrap()
                    .toggle_hunk_at_or_after_scroll(self.viewport_rows.max(1))
                {
                    return EventResult::ignored();
                }
            }
            _ => return EventResult::ignored(),
        }
        EventResult::consumed()
    }

    fn handle_mouse(&mut self, m: MouseEvent) -> EventResult {
        match m.kind {
            MouseEventKind::ScrollUp => {
                self.scroll_by(-WHEEL_STEP);
                EventResult::consumed()
            }
            MouseEventKind::ScrollDown => {
                self.scroll_by(WHEEL_STEP);
                EventResult::consumed()
            }
            _ => EventResult::ignored(),
        }
    }
}

impl Component for DiffPane {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, _ctx: ComponentContext<'_>) {
        let theme = self.theme.get().default;
        let mut sess = self.shared.lock().unwrap();

        let digits = line_number_digits(sess.side_line_count(self.side));
        let show_ln = sess.show_line_numbers();
        let layout = self.gutter_layout(digits);
        let gw = gutter_width(layout, show_ln);
        let text_w = area.width.saturating_sub(gw) as usize;

        sess.report_column_width(self.column, text_w, area.height as usize);

        self.viewport_rows = area.height as usize;
        self.viewport_cols = area.width;
        self.content_rows = sess.row_count();
        let scroll = sess.scroll_top();

        render_column(
            frame,
            area,
            &theme,
            sess.visible_projection(),
            self.column,
            scroll,
            layout,
            show_ln,
        );
    }
}

impl atto_ui::composable::DragAndDrop for DiffPane {}

impl Layout for DiffPane {
    fn min_width(&self) -> u16 {
        8
    }

    fn min_height(&self) -> u16 {
        1
    }
}

impl Scrollable for DiffPane {
    fn is_scrollable(&self) -> bool {
        true
    }

    fn content_size(&self) -> (u16, u16) {
        (
            self.viewport_cols,
            self.content_rows.min(u16::MAX as usize) as u16,
        )
    }

    fn viewport_size(&self) -> (u16, u16) {
        (
            self.viewport_cols,
            self.viewport_rows.min(u16::MAX as usize) as u16,
        )
    }

    fn scroll_offset(&self) -> (u16, u16) {
        let top = self.shared.lock().unwrap().scroll_top();
        (0, top.min(u16::MAX as usize) as u16)
    }

    fn set_scroll_offset(&mut self, _x: u16, y: u16) {
        let rows = self.viewport_rows.max(1);
        self.shared.lock().unwrap().set_scroll_top(y as usize, rows);
    }

    fn scroll_config(&self) -> ScrollConfig {
        ScrollConfig::default()
    }
}

impl FocusNav for DiffPane {
    fn is_focusable(&self) -> bool {
        true
    }
}

impl DynamicTree for DiffPane {}

impl EventHandling for DiffPane {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        match event {
            Event::Mouse(m) => self.handle_mouse(*m),
            Event::Key(key) if ctx.is_focused => self.handle_key(*key),
            _ => EventResult::ignored(),
        }
    }
}
