use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use ratatui::layout::Rect;

use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, run_crossterm_desktop,
};
use atto_ui::reactive::Binding;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_editor::{DiffView, DiffViewConfig, DiffViewMode, EditorThemeSet};

// Deterministic before/after sample used by PTY tests. Covers: a removed line, an added line,
// a replaced line (remove+add), multiple hunks, and a long line that soft-wraps.
const BEFORE: &str = "ctx top one\n\
REMOVED_LINE gone\n\
ctx shared two\n\
ctx shared three\n\
OLD_TEXT replaced\n\
ctx shared four\n\
LONG this is a deliberately long line that must soft wrap across several visual rows inside the diff column\n\
ctx bottom five\n";

const AFTER: &str = "ctx top one\n\
ctx shared two\n\
ADDED_LINE fresh\n\
ctx shared three\n\
NEW_TEXT replaced\n\
ctx shared four\n\
LONG this is a deliberately long line that must soft wrap across several visual rows inside the diff column\n\
ctx bottom five\n";

fn main() -> Result<()> {
    let theme: Binding<EditorThemeSet> = EditorThemeSet::default().into();
    let mode: Binding<DiffViewMode> = DiffViewMode::SideBySide.into();

    let config = DiffViewConfig::new(BEFORE.to_string(), AFTER.to_string()).mode(mode.clone());

    let (diff_view, _handle) = DiffView::new(config, theme);

    let app_cfg = CrosstermAppConfig::default()
        .mouse_capture(true)
        .cursor(CursorMode::Hide)
        .tick_rate(Duration::from_millis(16));

    let mode_toggle = mode.clone();

    run_crossterm_desktop(
        app_cfg,
        move |screen: Rect| {
            let mut desktop = Desktop::new(atto_ui::theme::Theme::dark(), MenuBar::new(vec![]));
            let work = Desktop::layout(screen).work_area;
            desktop.add_window(
                Window::new(WindowKind::Normal, "Diff", work, Box::new(diff_view)),
                screen,
            );
            Ok(desktop)
        },
        |_desktop, _screen| Ok(AppControl::Continue),
        move |_desktop, event, _screen, _res| {
            if let Event::Key(key) = event {
                match key.code {
                    KeyCode::Char('u') => mode_toggle.set(DiffViewMode::Unified),
                    KeyCode::Char('s') => mode_toggle.set(DiffViewMode::SideBySide),
                    _ => {}
                }
            }
            Ok(AppControl::Continue)
        },
    )
}
