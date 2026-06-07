use atto_ui::app::Desktop;
use atto_ui::reactive::Binding;
use atto_ui::wm::{Window, WindowId, WindowKind, WindowMinSizeMode};
use atto_ui_chat::{Artifact, ArtifactKind, ArtifactViewer};
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use crate::{
    DiffView, DiffViewConfig, DiffViewMode, EditorConfig, EditorSyntaxConfig, EditorThemeSet,
    EditorView,
};

pub struct RichArtifactViewer<'a> {
    desktop: &'a mut Desktop,
    screen: Rect,
    theme: Binding<EditorThemeSet>,
}

impl<'a> RichArtifactViewer<'a> {
    pub fn new(desktop: &'a mut Desktop, screen: Rect) -> Self {
        Self {
            desktop,
            screen,
            theme: EditorThemeSet::default().into(),
        }
    }

    pub fn with_theme(mut self, theme: impl Into<Binding<EditorThemeSet>>) -> Self {
        self.theme = theme.into();
        self
    }
}

impl ArtifactViewer for RichArtifactViewer<'_> {
    fn open(&mut self, artifact: Artifact) -> WindowId {
        let title = artifact_window_title(&artifact);
        let rect = artifact_window_rect(self.screen, &artifact);
        let view = match &artifact.kind {
            ArtifactKind::Diff => self.diff_body(&artifact),
            ArtifactKind::Code | ArtifactKind::File => self.code_body(&artifact),
        };

        self.desktop.add_window(
            Window::new(WindowKind::Normal, title, rect, view)
                .with_min_size(24, 6)
                .with_min_size_mode(WindowMinSizeMode::Scroll),
            self.screen,
        )
    }
}

impl RichArtifactViewer<'_> {
    fn code_body(&self, artifact: &Artifact) -> Box<dyn atto_ui::composable::Component> {
        let syntax = syntax_for_title(&artifact.title);
        let config = EditorConfig::new(artifact.content.clone());
        config.syntax.set(syntax);
        config
            .language_id
            .set(language_id_for_title(&artifact.title));
        config.read_only.set(true);
        config.show_folding_markers.set(false);

        let (view, _handle) = EditorView::new(config, self.theme.clone());
        Box::new(view)
    }

    fn diff_body(&self, artifact: &Artifact) -> Box<dyn atto_ui::composable::Component> {
        let (before, after) = before_after_from_unified_diff(&artifact.content);
        let config = DiffViewConfig::new(before, after)
            .mode(DiffViewMode::SideBySide)
            .syntax(syntax_for_title(&artifact.title));
        let (view, _handle) = DiffView::new(config, self.theme.clone());
        Box::new(view)
    }
}

fn artifact_window_title(artifact: &Artifact) -> String {
    format!("{}: {}", artifact.kind.label(), artifact.title)
}

fn artifact_window_rect(screen: Rect, artifact: &Artifact) -> Rect {
    let work = Desktop::layout(screen).work_area;
    if work.width == 0 || work.height == 0 {
        return work;
    }

    let content_width = artifact
        .content
        .lines()
        .map(|line| line.width())
        .max()
        .unwrap_or(0)
        .min(u16::MAX as usize) as u16;
    let content_height = artifact.content.lines().count().min(u16::MAX as usize) as u16;
    let preferred_width = match &artifact.kind {
        ArtifactKind::Diff => work.width.saturating_sub(4).max(40),
        ArtifactKind::Code | ArtifactKind::File => content_width.saturating_add(10).max(40),
    };
    let preferred_height = content_height.saturating_add(4).max(10);
    let max_width = work.width.saturating_sub(2).max(1);
    let max_height = work.height.saturating_sub(2).max(1);
    let width = preferred_width.min(max_width).max(1);
    let height = preferred_height.min(max_height).max(1);

    Rect {
        x: work.x + work.width.saturating_sub(width) / 2,
        y: work.y + work.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn syntax_for_title(title: &str) -> EditorSyntaxConfig {
    let title = title.to_ascii_lowercase();
    if title.ends_with(".rs") || title.ends_with(".patch") || title.ends_with(".diff") {
        EditorSyntaxConfig::SimpleRust
    } else if title.ends_with(".json") {
        EditorSyntaxConfig::SimpleJson
    } else if title.ends_with(".ini") {
        EditorSyntaxConfig::SimpleIni
    } else {
        EditorSyntaxConfig::None
    }
}

fn language_id_for_title(title: &str) -> String {
    let title = title.to_ascii_lowercase();
    if title.ends_with(".rs") {
        "rust".to_string()
    } else if title.ends_with(".json") {
        "json".to_string()
    } else if title.ends_with(".ini") {
        "ini".to_string()
    } else {
        "plaintext".to_string()
    }
}

fn before_after_from_unified_diff(diff: &str) -> (String, String) {
    let mut before = String::new();
    let mut after = String::new();
    let mut in_hunk = false;

    for raw in diff.lines() {
        if raw.starts_with("@@") {
            in_hunk = true;
            continue;
        }
        if !in_hunk || raw.starts_with(r"\ No newline at end of file") {
            continue;
        }
        if let Some(rest) = raw.strip_prefix(' ') {
            before.push_str(rest);
            before.push('\n');
            after.push_str(rest);
            after.push('\n');
        } else if let Some(rest) = raw.strip_prefix('-') {
            before.push_str(rest);
            before.push('\n');
        } else if let Some(rest) = raw.strip_prefix('+') {
            after.push_str(rest);
            after.push('\n');
        }
    }

    if before.is_empty() && after.is_empty() {
        (diff.to_string(), diff.to_string())
    } else {
        (before, after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_diff_content_reconstructs_before_and_after_hunk_text() {
        let diff = "--- a/main.rs\n+++ b/main.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-    old();\n+    new();\n }\n";

        let (before, after) = before_after_from_unified_diff(diff);

        assert!(before.contains("old();"));
        assert!(!before.contains("new();"));
        assert!(after.contains("new();"));
        assert!(!after.contains("old();"));
    }
}
