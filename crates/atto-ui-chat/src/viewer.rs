use atto_ui::app::Desktop;
use atto_ui::composable::{Component, ComponentContext, EventResult, Layout};
use atto_ui::wm::{Window, WindowId, WindowKind, WindowMinSizeMode};
use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::{Artifact, ArtifactKind};

pub trait ArtifactViewer {
    fn open(&mut self, artifact: Artifact) -> WindowId;
}

pub struct TextArtifactViewer<'a> {
    desktop: &'a mut Desktop,
    screen: Rect,
}

impl<'a> TextArtifactViewer<'a> {
    pub fn new(desktop: &'a mut Desktop, screen: Rect) -> Self {
        Self { desktop, screen }
    }
}

impl ArtifactViewer for TextArtifactViewer<'_> {
    fn open(&mut self, artifact: Artifact) -> WindowId {
        let title = artifact_window_title(&artifact);
        let rect = artifact_window_rect(self.screen, &artifact);
        let view = Box::new(TextArtifactBody::new(artifact));
        self.desktop.add_window(
            Window::new(WindowKind::Normal, title, rect, view)
                .with_min_size(20, 5)
                .with_min_size_mode(WindowMinSizeMode::Scroll),
            self.screen,
        )
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

    let content_width = artifact_display_lines(artifact)
        .iter()
        .map(|line| line.width())
        .max()
        .unwrap_or(0)
        .min(u16::MAX as usize) as u16;
    let content_height = artifact_display_lines(artifact)
        .len()
        .min(u16::MAX as usize) as u16;

    let preferred_width = content_width.saturating_add(2).max(32);
    let preferred_height = content_height.saturating_add(2).max(8);
    let max_width = work.width.saturating_sub(2).max(1);
    let max_height = work.height.saturating_sub(2).max(1);
    let width = preferred_width.min(max_width).max(1);
    let height = preferred_height.min(max_height).max(1);
    let x = if work.width > width.saturating_add(2) {
        work.x + work.width - width - 2
    } else {
        work.x
    };
    let y = if work.height > height.saturating_add(1) {
        work.y + 1
    } else {
        work.y
    };

    Rect {
        x,
        y,
        width,
        height,
    }
}

struct TextArtifactBody {
    artifact: Artifact,
}

impl TextArtifactBody {
    fn new(artifact: Artifact) -> Self {
        Self { artifact }
    }

    fn display_lines(&self) -> Vec<String> {
        artifact_display_lines(&self.artifact)
    }
}

impl Component for TextArtifactBody {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let base = ctx.theme.widget.normal;
        let lines = self
            .display_lines()
            .into_iter()
            .enumerate()
            .map(|(idx, line)| {
                if idx == 0 {
                    Line::styled(line, base.add_modifier(Modifier::BOLD))
                } else if self.artifact.kind == ArtifactKind::Diff {
                    Line::styled(line.clone(), diff_line_style(&line, base))
                } else {
                    Line::styled(line, base)
                }
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), area);
    }
}

impl atto_ui::composable::DragAndDrop for TextArtifactBody {}

impl Layout for TextArtifactBody {
    fn min_width(&self) -> u16 {
        1
    }

    fn min_height(&self) -> u16 {
        1
    }

    fn desired_width(&self) -> Option<u16> {
        let width = self
            .display_lines()
            .iter()
            .map(|line| line.width())
            .max()
            .unwrap_or(1)
            .min(u16::MAX as usize) as u16;
        Some(width.max(1))
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.display_lines().len().min(u16::MAX as usize) as u16)
    }
}

impl atto_ui::composable::Scrollable for TextArtifactBody {}
impl atto_ui::composable::FocusNav for TextArtifactBody {}
impl atto_ui::composable::DynamicTree for TextArtifactBody {}

impl atto_ui::composable::EventHandling for TextArtifactBody {
    fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }
}

fn artifact_display_lines(artifact: &Artifact) -> Vec<String> {
    let mut lines = vec![format!(
        "{} Artifact: {}",
        artifact.kind.label(),
        artifact.title
    )];
    lines.push(String::new());
    if artifact.content.is_empty() {
        lines.push("(empty)".to_string());
    } else {
        lines.extend(artifact.content.lines().map(str::to_string));
    }
    lines
}

fn diff_line_style(line: &str, base: Style) -> Style {
    if line.starts_with("@@") {
        base.fg(Color::Yellow)
    } else if line.starts_with("+++") || line.starts_with("---") {
        base.fg(Color::Cyan)
    } else if line.starts_with('+') {
        base.fg(Color::Green)
    } else if line.starts_with('-') {
        base.fg(Color::Red)
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_line_style_colors_unified_diff_prefixes() {
        let base = Style::default();

        assert_eq!(diff_line_style("+added", base).fg, Some(Color::Green));
        assert_eq!(diff_line_style("-removed", base).fg, Some(Color::Red));
        assert_eq!(diff_line_style("@@ hunk", base).fg, Some(Color::Yellow));
        assert_eq!(diff_line_style(" context", base).fg, None);
    }
}
