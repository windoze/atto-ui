use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use ratatui::layout::Rect;

use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, run_crossterm_desktop,
};
use atto_ui::reactive::Binding;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_editor::{EditorConfig, EditorLspConfig, EditorLspMode, EditorThemeSet, EditorView};

fn guess_language_id(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "rs" => "rust",
        "toml" => "toml",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "md" | "markdown" => "markdown",
        "py" => "python",
        "js" => "javascript",
        "jsx" => "javascriptreact",
        "ts" => "typescript",
        "tsx" => "typescriptreact",
        _ => "plaintext",
    }
    .to_string()
}

fn parse_cmd_env(var: &str) -> Option<Vec<String>> {
    let raw = env::var(var).ok()?;
    let parts = raw
        .split_whitespace()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    (!parts.is_empty()).then_some(parts)
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let file_path = args.get(1).map(PathBuf::from);

    let initial_text = if let Some(path) = file_path.as_ref() {
        fs::read_to_string(path).unwrap_or_default()
    } else {
        "atto-ui editor demo\n\n- type to edit\n- Ctrl+Space: LSP completion (if enabled)\n- F12: go to definition (if enabled)\n".to_string()
    };

    let text: Binding<String> = initial_text.into();
    let theme: Binding<EditorThemeSet> = EditorThemeSet::default().into();

    let editor_config = EditorConfig::new(text.clone());
    if let Some(path) = file_path.as_ref() {
        editor_config.language_id.set(guess_language_id(path));
    }

    // Optional LSP wiring via env vars:
    // - `ATTO_UI_EDITOR_LSP_CMD`: server command (e.g. "rust-analyzer")
    // - `ATTO_UI_EDITOR_LSP_LANGUAGE_ID`: override language id (optional)
    if let (Some(path), Some(cmd)) = (file_path.as_ref(), parse_cmd_env("ATTO_UI_EDITOR_LSP_CMD")) {
        let language_id = env::var("ATTO_UI_EDITOR_LSP_LANGUAGE_ID")
            .ok()
            .unwrap_or_else(|| guess_language_id(path));
        let lsp_cfg = EditorLspConfig::for_file_path(path, language_id, cmd);
        editor_config.lsp.set(EditorLspMode::Enabled(lsp_cfg));
    }

    let (editor_view, editor_handle) = EditorView::new(editor_config, theme.clone());
    let editor_events = editor_handle.events.clone();

    let app_cfg = CrosstermAppConfig::default()
        .bracketed_paste(true)
        .cursor(CursorMode::Show);

    run_crossterm_desktop(
        app_cfg,
        move |screen: Rect| {
            let mut desktop = Desktop::new(atto_ui::theme::Theme::dark(), MenuBar::new(vec![]));
            let work = Desktop::layout(screen).work_area;

            desktop.add_window(
                Window::new(
                    WindowKind::Normal,
                    "Editor",
                    Rect {
                        x: work.x + 2,
                        y: work.y + 1,
                        width: work.width.saturating_sub(4).max(20),
                        height: work.height.saturating_sub(2).max(8),
                    },
                    Box::new(editor_view),
                ),
                screen,
            );

            Ok(desktop)
        },
        move |_desktop: &mut Desktop, _screen: Rect| {
            // Drain goto results (host callback hook).
            for ev in editor_events.drain() {
                match ev {
                    atto_ui_editor::EditorEvent::LspGoto { kind: _, locations } => {
                        // For the demo, just open a tooltip window with the first target.
                        if let Some(loc) = locations.first() {
                            let _ = loc;
                        }
                    }
                    atto_ui_editor::EditorEvent::CodeActionMessage { message: _ } => {}
                }
            }

            Ok(AppControl::Continue)
        },
        |_desktop, _event, _screen, _res| Ok(AppControl::Continue),
    )
}
