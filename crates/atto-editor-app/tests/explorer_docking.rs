#![forbid(unsafe_code)]

use atto_editor_app::actions::AppAction;
use atto_editor_app::explorer_window::{ExplorerWindowCommand, ExplorerWindowView};
use atto_editor_app::window::{EditorWindowCommand, EditorWindowView};
use atto_ui::app::{Desktop, MenuBar};
use atto_ui::reactive::{Binding, EventQueue};
use atto_ui::theme::Theme;
use atto_ui::wm::{DockAutoHide, DockSide, Window, WindowDock, WindowKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

const EXPLORER_SIZE: u16 = 34;

fn explorer_dock(side: DockSide) -> WindowDock {
    WindowDock {
        side,
        size: EXPLORER_SIZE,
        min_size: 20,
        max_size: None,
        auto_hide: DockAutoHide::Disabled,
        handle_label: Some("Explorer".to_string()),
    }
}

fn editor_rect(work: Rect) -> Rect {
    Rect {
        x: work.x.saturating_add(3),
        y: work.y.saturating_add(2),
        width: work.width.saturating_sub(6).max(40),
        height: work.height.saturating_sub(4).max(12),
    }
}

fn draw_desktop(desktop: &mut Desktop, screen: Rect) {
    let backend = TestBackend::new(screen.width, screen.height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|f| desktop.draw(f)).expect("draw desktop");
}

#[test]
fn explorer_dock_reserves_editor_area_and_tracks_side_across_resize() {
    let screen = Rect::new(0, 0, 90, 28);
    let work = Desktop::layout(screen).work_area;
    let actions: EventQueue<AppAction> = EventQueue::new();
    let explorer_commands: EventQueue<ExplorerWindowCommand> = EventQueue::new();
    let editor_commands: EventQueue<EditorWindowCommand> = EventQueue::new();
    let editor_theme: Binding<atto_ui_editor::EditorThemeSet> =
        atto_ui_editor::EditorThemeSet::default().into();
    let clipboard: Binding<String> = String::new().into();

    let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(Vec::new()));
    let explorer_id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Explorer",
            Rect::default(),
            Box::new(ExplorerWindowView::new(
                actions.clone(),
                explorer_commands,
                Vec::new(),
            )),
        )
        .with_tag("atto-editor-app-explorer")
        .with_dock(Some(explorer_dock(DockSide::Left))),
        screen,
    );
    let editor_id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Atto Editor",
            editor_rect(work),
            Box::new(EditorWindowView::new(
                actions,
                editor_commands,
                editor_theme,
                clipboard,
            )),
        )
        .with_tag("atto-editor-app"),
        screen,
    );

    assert_eq!(
        desktop.wm.window(explorer_id).expect("explorer").rect.get(),
        Rect::new(0, 1, EXPLORER_SIZE, 26)
    );
    let editor_left_rect = desktop.wm.window(editor_id).expect("editor").rect.get();
    assert!(
        editor_left_rect.x >= EXPLORER_SIZE,
        "editor should not cover left dock: {editor_left_rect:?}"
    );

    let mut dock = desktop
        .wm
        .window(explorer_id)
        .expect("explorer")
        .dock
        .get()
        .expect("dock config");
    dock.side = DockSide::Right;
    desktop
        .wm
        .window_mut(explorer_id)
        .expect("explorer")
        .dock
        .set(Some(dock));
    draw_desktop(&mut desktop, screen);

    assert_eq!(
        desktop.wm.window(explorer_id).expect("explorer").rect.get(),
        Rect::new(56, 1, EXPLORER_SIZE, 26)
    );
    let editor_right_rect = desktop.wm.window(editor_id).expect("editor").rect.get();
    assert!(
        editor_right_rect.x.saturating_add(editor_right_rect.width) <= 56,
        "editor should not cover right dock: {editor_right_rect:?}"
    );

    let resized = Rect::new(0, 0, 110, 30);
    draw_desktop(&mut desktop, resized);

    assert_eq!(
        desktop.wm.window(explorer_id).expect("explorer").rect.get(),
        Rect::new(76, 1, EXPLORER_SIZE, 28)
    );
    let editor_resized_rect = desktop.wm.window(editor_id).expect("editor").rect.get();
    assert!(
        editor_resized_rect
            .x
            .saturating_add(editor_resized_rect.width)
            <= 76,
        "editor should stay inside resized dock reserve: {editor_resized_rect:?}"
    );
}
