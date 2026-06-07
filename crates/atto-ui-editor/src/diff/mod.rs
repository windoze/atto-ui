//! Read-only diff viewer built on the headless `editor-core-diff` / `editor-core-diff-view`
//! crates.
//!
//! Two modes:
//! - [`DiffViewMode::Unified`] renders a single column (like a read-only editor view).
//! - [`DiffViewMode::SideBySide`] uses a [`Splitter`] to place the two sides next to each other;
//!   moving the divider rebuilds the projection (re-wrapping + re-deriving the shared row axis)
//!   while preserving the scroll position. Both sides scroll together.

mod pane;
mod render;
mod session;

use atto_ui::composable::{
    Component, ComponentContext, DynamicTree, EventHandling, EventResult, FocusNav, Layout,
    ScrollConfig, Scrollable, Splitter,
};
use atto_ui::reactive::{Binding, DirtyObserver};
use crossterm::event::{Event, KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use editor_core_diff::LineDiffConfig;
use editor_core_diff_view::DiffMode;
use ratatui::Frame;
use ratatui::layout::Rect;

use self::pane::DiffPane;
use self::render::{GutterLayout, gutter_width, line_number_digits, render_column};
use self::session::{AFTER_SIDE, BEFORE_SIDE, DiffSession, SharedSession};
use crate::EditorSyntaxConfig;
use crate::theme::EditorThemeSet;

const WHEEL_STEP: isize = 3;

/// Projection mode exposed to hosts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffViewMode {
    /// Single-column unified diff.
    Unified,
    /// Two-column side-by-side diff.
    SideBySide,
}

impl From<DiffViewMode> for DiffMode {
    fn from(mode: DiffViewMode) -> Self {
        match mode {
            DiffViewMode::Unified => DiffMode::Unified,
            DiffViewMode::SideBySide => DiffMode::SideBySide,
        }
    }
}

/// Reactive configuration for a [`DiffView`].
#[derive(Clone)]
pub struct DiffViewConfig {
    pub before: Binding<String>,
    pub after: Binding<String>,
    pub mode: Binding<DiffViewMode>,
    pub show_line_numbers: Binding<bool>,
    pub syntax: Binding<EditorSyntaxConfig>,
    pub line_diff: LineDiffConfig,
}

impl DiffViewConfig {
    pub fn new(before: impl Into<Binding<String>>, after: impl Into<Binding<String>>) -> Self {
        Self {
            before: before.into(),
            after: after.into(),
            mode: DiffViewMode::SideBySide.into(),
            show_line_numbers: true.into(),
            syntax: EditorSyntaxConfig::None.into(),
            line_diff: LineDiffConfig::default(),
        }
    }

    pub fn mode(mut self, mode: impl Into<Binding<DiffViewMode>>) -> Self {
        self.mode = mode.into();
        self
    }

    pub fn show_line_numbers(mut self, show: impl Into<Binding<bool>>) -> Self {
        self.show_line_numbers = show.into();
        self
    }

    pub fn syntax(mut self, syntax: impl Into<Binding<EditorSyntaxConfig>>) -> Self {
        self.syntax = syntax.into();
        self
    }

    pub fn line_diff(mut self, cfg: LineDiffConfig) -> Self {
        self.line_diff = cfg;
        self
    }
}

/// Handle for host integration (toggle mode, swap texts, retheme) without owning the view.
#[derive(Clone)]
pub struct DiffViewHandle {
    pub before: Binding<String>,
    pub after: Binding<String>,
    pub mode: Binding<DiffViewMode>,
    pub show_line_numbers: Binding<bool>,
    pub syntax: Binding<EditorSyntaxConfig>,
    pub theme: Binding<EditorThemeSet>,
}

pub struct DiffView {
    config: DiffViewConfig,
    theme: Binding<EditorThemeSet>,
    shared: SharedSession,
    splitter: Splitter,

    before_observer: DirtyObserver,
    after_observer: DirtyObserver,
    mode_observer: DirtyObserver,
    show_ln_observer: DirtyObserver,
    syntax_observer: DirtyObserver,

    // Unified-mode viewport tracking (side-by-side scrolling lives in the panes).
    uni_viewport_rows: usize,
    uni_viewport_cols: u16,
    uni_content_rows: usize,
}

impl DiffView {
    pub fn new(
        config: DiffViewConfig,
        theme: impl Into<Binding<EditorThemeSet>>,
    ) -> (Self, DiffViewHandle) {
        let theme = theme.into();

        let session = DiffSession::new(
            &config.before.get(),
            &config.after.get(),
            config.line_diff,
            config.mode.get().into(),
            config.show_line_numbers.get(),
            config.syntax.get(),
        );
        let shared = session.into_shared();

        let before_pane = DiffPane::new(shared.clone(), theme.clone(), 0, BEFORE_SIDE);
        let after_pane = DiffPane::new(shared.clone(), theme.clone(), 1, AFTER_SIDE);
        let splitter = Splitter::vertical(before_pane, after_pane);

        let handle = DiffViewHandle {
            before: config.before.clone(),
            after: config.after.clone(),
            mode: config.mode.clone(),
            show_line_numbers: config.show_line_numbers.clone(),
            syntax: config.syntax.clone(),
            theme: theme.clone(),
        };

        let view = Self {
            before_observer: config.before.dirty_observer(),
            after_observer: config.after.dirty_observer(),
            mode_observer: config.mode.dirty_observer(),
            show_ln_observer: config.show_line_numbers.dirty_observer(),
            syntax_observer: config.syntax.dirty_observer(),
            config,
            theme,
            shared,
            splitter,
            uni_viewport_rows: 0,
            uni_viewport_cols: 0,
            uni_content_rows: 0,
        };

        (view, handle)
    }

    fn sync_config(&mut self) {
        let before_dirty = self.config.before.check_dirty(&mut self.before_observer);
        let after_dirty = self.config.after.check_dirty(&mut self.after_observer);
        if before_dirty || after_dirty {
            self.shared
                .lock()
                .unwrap()
                .set_texts(&self.config.before.get(), &self.config.after.get());
        }
        if self.config.mode.check_dirty(&mut self.mode_observer) {
            self.shared
                .lock()
                .unwrap()
                .set_mode(self.config.mode.get().into());
        }
        if self
            .config
            .show_line_numbers
            .check_dirty(&mut self.show_ln_observer)
        {
            self.shared
                .lock()
                .unwrap()
                .set_show_line_numbers(self.config.show_line_numbers.get());
        }
        if self.config.syntax.check_dirty(&mut self.syntax_observer) {
            self.shared
                .lock()
                .unwrap()
                .set_syntax(self.config.syntax.get());
        }
    }

    fn is_unified(&self) -> bool {
        matches!(self.config.mode.get(), DiffViewMode::Unified)
    }

    fn draw_unified(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let theme = self.theme.get().default;
        let mut sess = self.shared.lock().unwrap();

        let before_digits = line_number_digits(sess.side_line_count(BEFORE_SIDE));
        let after_digits = line_number_digits(sess.side_line_count(AFTER_SIDE));
        let layout = GutterLayout::Unified {
            before_digits,
            after_digits,
        };
        let show_ln = sess.show_line_numbers();
        let gw = gutter_width(layout, show_ln);
        let text_w = area.width.saturating_sub(gw) as usize;

        sess.report_column_width(0, text_w);

        self.uni_viewport_rows = area.height as usize;
        self.uni_viewport_cols = area.width;
        self.uni_content_rows = sess.row_count();
        let scroll = sess.scroll_top();

        render_column(
            frame,
            area,
            &theme,
            sess.visible_projection(),
            0,
            scroll,
            layout,
            show_ln,
        );
    }

    fn unified_scroll_by(&mut self, delta: isize) {
        let rows = self.uni_viewport_rows.max(1);
        self.shared.lock().unwrap().scroll_by(delta, rows);
    }

    fn unified_handle_key(&mut self, key: KeyEvent) -> EventResult {
        let page = self.uni_viewport_rows.max(1) as isize;
        match key.code {
            KeyCode::Up => self.unified_scroll_by(-1),
            KeyCode::Down => self.unified_scroll_by(1),
            KeyCode::PageUp => self.unified_scroll_by(-page),
            KeyCode::PageDown => self.unified_scroll_by(page),
            KeyCode::Home => self
                .shared
                .lock()
                .unwrap()
                .set_scroll_top(0, self.uni_viewport_rows),
            KeyCode::End => {
                let rows = self.uni_viewport_rows.max(1);
                let mut sess = self.shared.lock().unwrap();
                let max = sess.max_scroll_top(rows);
                sess.set_scroll_top(max, rows);
            }
            KeyCode::Char('z') => {
                if !self
                    .shared
                    .lock()
                    .unwrap()
                    .toggle_hunk_at_or_after_scroll(self.uni_viewport_rows.max(1))
                {
                    return EventResult::ignored();
                }
            }
            _ => return EventResult::ignored(),
        }
        EventResult::consumed()
    }

    fn unified_handle_mouse(&mut self, m: MouseEvent) -> EventResult {
        match m.kind {
            MouseEventKind::ScrollUp => {
                self.unified_scroll_by(-WHEEL_STEP);
                EventResult::consumed()
            }
            MouseEventKind::ScrollDown => {
                self.unified_scroll_by(WHEEL_STEP);
                EventResult::consumed()
            }
            _ => EventResult::ignored(),
        }
    }
}

impl Component for DiffView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.sync_config();
        if self.is_unified() {
            self.draw_unified(frame, area);
        } else {
            self.splitter.draw(frame, area, ctx);
        }
    }
}

impl Layout for DiffView {
    fn min_width(&self) -> u16 {
        16
    }

    fn min_height(&self) -> u16 {
        3
    }
}

impl Scrollable for DiffView {
    fn is_scrollable(&self) -> bool {
        // Side-by-side delegates scrollbars to the splitter's per-child borders.
        self.is_unified()
    }

    fn content_size(&self) -> (u16, u16) {
        if self.is_unified() {
            (
                self.uni_viewport_cols,
                self.uni_content_rows.min(u16::MAX as usize) as u16,
            )
        } else {
            (0, 0)
        }
    }

    fn viewport_size(&self) -> (u16, u16) {
        if self.is_unified() {
            (
                self.uni_viewport_cols,
                self.uni_viewport_rows.min(u16::MAX as usize) as u16,
            )
        } else {
            (0, 0)
        }
    }

    fn scroll_offset(&self) -> (u16, u16) {
        if self.is_unified() {
            let top = self.shared.lock().unwrap().scroll_top();
            (0, top.min(u16::MAX as usize) as u16)
        } else {
            (0, 0)
        }
    }

    fn set_scroll_offset(&mut self, _x: u16, y: u16) {
        if self.is_unified() {
            let rows = self.uni_viewport_rows.max(1);
            self.shared.lock().unwrap().set_scroll_top(y as usize, rows);
        }
    }

    fn scroll_config(&self) -> ScrollConfig {
        ScrollConfig::default()
    }
}

impl FocusNav for DiffView {
    fn is_focusable(&self) -> bool {
        true
    }

    fn focus_first(&mut self) -> bool {
        if self.is_unified() {
            true
        } else {
            self.splitter.focus_first()
        }
    }

    fn focus_last(&mut self) -> bool {
        if self.is_unified() {
            true
        } else {
            self.splitter.focus_last()
        }
    }
}

impl DynamicTree for DiffView {}

impl EventHandling for DiffView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if self.is_unified() {
            match event {
                Event::Mouse(m) => self.unified_handle_mouse(*m),
                Event::Key(key) if ctx.is_focused => self.unified_handle_key(*key),
                _ => EventResult::ignored(),
            }
        } else {
            self.splitter.handle_event(event, ctx)
        }
    }
}
