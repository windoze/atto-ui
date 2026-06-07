use std::time::Duration;

use anyhow::Result;
use ratatui::layout::Rect;

use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, run_crossterm_desktop,
};
use atto_ui::reactive::Binding;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_editor::{
    EditorConfig, EditorLspConfig, EditorLspMode, EditorSyntaxConfig, EditorThemeSet, EditorView,
};
use editor_core::CommentConfig;

fn main() -> Result<()> {
    // A deterministic app used by PTY tests to validate editor behavior.
    let diagnostics_mode = std::env::args().any(|arg| arg == "--diagnostics");
    let mut lines: Vec<String> = if diagnostics_mode {
        vec!["let bad = 1;".to_string(), String::new()]
    } else {
        vec![
            "tab:ab".to_string(),
            r#"{"s": "hello", "n": 42}"#.to_string(),
            "double: world".to_string(),
            "triple: full-line".to_string(),
            "rect:ab".to_string(),
            "rect:ab".to_string(),
            "rect:ab".to_string(),
            "let answer = 42;".to_string(),
            String::new(),
        ]
    };

    // Extra lines for paging / scrolling tests.
    for i in 0..120 {
        lines.push(format!("pd:{i:03} line for paging"));
    }
    lines.push(String::new());

    let initial_text = lines.join("\n");

    let text: Binding<String> = initial_text.into();
    let theme: Binding<EditorThemeSet> = EditorThemeSet::default().into();

    let editor_config = EditorConfig::new(text);
    editor_config.language_id.set("rust".to_string());
    editor_config.comment.set(Some(CommentConfig::line("//")));
    editor_config.syntax.set(EditorSyntaxConfig::SimpleJson);
    editor_config.indent.tab_width.set(4);
    editor_config.indent.insert_spaces.set(true);
    if diagnostics_mode {
        editor_config.syntax.set(EditorSyntaxConfig::None);
        editor_config.hover.enabled.set(false);
        editor_config
            .lsp
            .set(EditorLspMode::Enabled(EditorLspConfig {
                command: vec![sibling_binary("mock_lsp_server")],
                document_uri: "file:///diagnostics.rs".to_string(),
                language_id: "rust".to_string(),
                root_uri: None,
                workspace_folders: Vec::new(),
                initialize_timeout: Duration::from_secs(1),
                semantic_tokens: false,
                folding_ranges: false,
            }));
    }

    let (editor_view, _handle) = EditorView::new(editor_config, theme);

    let app_cfg = CrosstermAppConfig::default()
        .mouse_capture(true)
        .bracketed_paste(true)
        .cursor(CursorMode::Show)
        // PTY tests do their own waiting; keep draw ticks responsive.
        .tick_rate(Duration::from_millis(16));

    run_crossterm_desktop(
        app_cfg,
        move |screen: Rect| {
            let mut desktop = Desktop::new(atto_ui::theme::Theme::dark(), MenuBar::new(vec![]));
            let work = Desktop::layout(screen).work_area;

            desktop.add_window(
                Window::new(WindowKind::Normal, "Editor", work, Box::new(editor_view)),
                screen,
            );

            Ok(desktop)
        },
        |_desktop, _screen| Ok(AppControl::Continue),
        |_desktop, _event, _screen, _res| Ok(AppControl::Continue),
    )
}

fn sibling_binary(name: &str) -> String {
    let mut path = std::env::current_exe().expect("snapshot app path");
    path.set_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    path.to_string_lossy().into_owned()
}
