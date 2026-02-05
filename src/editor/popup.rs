use crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use serde_json::Value;

use crate::reactive::Binding;
use crate::view::{View, ViewContext, ViewEventResult};
use crate::wm::{Window, WindowDecorations, WindowId, WindowKind, WindowManager};

use super::theme::{EditorTheme, EditorThemeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LspHoverContents {
    PlainText(Vec<String>),
}

impl LspHoverContents {
    pub fn lines(&self) -> &[String] {
        match self {
            Self::PlainText(lines) => lines,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoverPopupModel {
    /// Desired popup rect in screen coordinates.
    pub rect: Rect,
    pub contents: LspHoverContents,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LspCompletionItemEdit {
    /// Store raw LSP completion item JSON for later application.
    Raw(Value),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompletionItem {
    pub label: String,
    pub detail: Option<String>,
    pub edit: LspCompletionItemEdit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompletionPopupModel {
    /// Desired popup rect in screen coordinates.
    pub rect: Rect,
    pub items: Vec<CompletionItem>,
    pub selected: usize,
    /// First visible item index (for scrolling long lists).
    pub scroll: usize,
    /// When set (typically by mouse click), the editor should accept the completion and then
    /// clear this field.
    pub accept: Option<usize>,
}

#[derive(Clone, Debug)]
struct HoverPopupView {
    model: Binding<Option<HoverPopupModel>>,
    theme: Binding<EditorThemeSet>,
    language_id: Binding<String>,
    last_area: Option<Rect>,
}

impl HoverPopupView {
    fn new(
        model: Binding<Option<HoverPopupModel>>,
        theme: Binding<EditorThemeSet>,
        language_id: Binding<String>,
    ) -> Self {
        Self {
            model,
            theme,
            language_id,
            last_area: None,
        }
    }

    fn editor_theme(&self) -> EditorTheme {
        let theme_set = self.theme.get();
        let language_id = self.language_id.get();
        theme_set.for_language(language_id.as_str()).clone()
    }
}

impl View for HoverPopupView {
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        if matches!(
            event,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                ..
            })
        ) {
            // Clicking a hover tooltip should always dismiss it.
            self.model.set(None);
            return ViewEventResult::consumed();
        }
        ViewEventResult::ignored()
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, _ctx: ViewContext<'_>) {
        self.last_area = Some(area);
        let Some(model) = self.model.get() else {
            return;
        };
        let theme = self.editor_theme();
        let lines: Vec<Line<'static>> = model
            .contents
            .lines()
            .iter()
            .map(|l| Line::from(l.clone()))
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.popup_border)
            .style(theme.popup);
        frame.render_widget(Paragraph::new(lines).style(theme.popup).block(block), area);
    }
}

#[derive(Clone, Debug)]
struct CompletionPopupView {
    model: Binding<Option<CompletionPopupModel>>,
    theme: Binding<EditorThemeSet>,
    language_id: Binding<String>,
    last_area: Option<Rect>,
}

impl CompletionPopupView {
    fn new(
        model: Binding<Option<CompletionPopupModel>>,
        theme: Binding<EditorThemeSet>,
        language_id: Binding<String>,
    ) -> Self {
        Self {
            model,
            theme,
            language_id,
            last_area: None,
        }
    }

    fn editor_theme(&self) -> EditorTheme {
        let theme_set = self.theme.get();
        let language_id = self.language_id.get();
        theme_set.for_language(language_id.as_str()).clone()
    }
}

impl View for CompletionPopupView {
    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        let Some(mut model) = self.model.get() else {
            return ViewEventResult::ignored();
        };

        let Event::Mouse(m) = event else {
            return ViewEventResult::ignored();
        };
        let Some(area) = self.last_area else {
            return ViewEventResult::ignored();
        };

        // Mouse events may arrive in screen coordinates (WindowManager dispatch) or already-local
        // (nested containers). Interpret both.
        let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m) else {
            return ViewEventResult::ignored();
        };

        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // The popup view draws its own border, so y=0/y=height-1 are borders.
                if area.width < 3 || area.height < 3 {
                    return ViewEventResult::ignored();
                }
                if local_x == 0
                    || local_y == 0
                    || local_x + 1 >= area.width
                    || local_y + 1 >= area.height
                {
                    return ViewEventResult::ignored();
                }

                let item_row = (local_y - 1) as usize;
                let idx = model.scroll.saturating_add(item_row);
                if idx >= model.items.len() {
                    return ViewEventResult::ignored();
                }

                model.selected = idx;
                model.accept = Some(idx);
                self.model.set(Some(model));
                ViewEventResult::consumed()
            }
            MouseEventKind::ScrollUp => {
                let visible = area.height.saturating_sub(2) as usize;
                model.scroll = model.scroll.saturating_sub(1);
                model.selected = model.selected.min(model.items.len().saturating_sub(1));
                if model.selected < model.scroll {
                    model.selected = model.scroll;
                }
                if visible > 0 && model.selected >= model.scroll + visible {
                    model.selected = model.scroll + visible - 1;
                }
                self.model.set(Some(model));
                ViewEventResult::consumed()
            }
            MouseEventKind::ScrollDown => {
                let visible = area.height.saturating_sub(2) as usize;
                if visible > 0 && model.scroll + visible < model.items.len() {
                    model.scroll = model.scroll.saturating_add(1);
                    model.selected = model.selected.min(model.items.len().saturating_sub(1));
                    if model.selected < model.scroll {
                        model.selected = model.scroll;
                    }
                    if model.selected >= model.scroll + visible {
                        model.selected = model.scroll + visible - 1;
                    }
                    self.model.set(Some(model));
                }
                ViewEventResult::consumed()
            }
            _ => ViewEventResult::ignored(),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, _ctx: ViewContext<'_>) {
        self.last_area = Some(area);
        let Some(model) = self.model.get() else {
            return;
        };
        let theme = self.editor_theme();

        let inner_height = area.height.saturating_sub(2) as usize;
        let mut lines: Vec<Line<'static>> = Vec::with_capacity(inner_height);

        for row in 0..inner_height {
            let idx = model.scroll.saturating_add(row);
            if idx >= model.items.len() {
                lines.push(Line::from(""));
                continue;
            }

            let item = &model.items[idx];
            let mut style = theme.popup;
            if idx == model.selected {
                style = theme.popup_selected;
            }

            let label = item.label.clone();
            let detail = item.detail.clone().unwrap_or_default();
            let line = if detail.is_empty() {
                Line::from(vec![Span::styled(label, style)])
            } else {
                Line::from(vec![
                    Span::styled(label, style),
                    Span::styled(" ", style),
                    Span::styled(detail, style.add_modifier(ratatui::style::Modifier::DIM)),
                ])
            };
            lines.push(line);
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.popup_border)
            .style(theme.popup);

        frame.render_widget(Paragraph::new(lines).block(block), area);
    }
}

fn popup_decorations() -> WindowDecorations {
    WindowDecorations {
        border: crate::wm::WindowBorderStyle::Borderless,
        shadow: false,
        buttons: crate::wm::WindowButtons {
            minimize: false,
            maximize: false,
            close: false,
        },
    }
}

/// Manages the tooltip windows used by an [`crate::editor::EditorView`].
///
/// This is intentionally separate from the `View` itself: it needs access to the host
/// `WindowManager` to create/close popup windows.
#[derive(Debug)]
pub struct EditorPopupWindows {
    hover_id: Option<WindowId>,
    completion_id: Option<WindowId>,
    hover: Binding<Option<HoverPopupModel>>,
    completion: Binding<Option<CompletionPopupModel>>,
    theme: Binding<EditorThemeSet>,
    language_id: Binding<String>,
}

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn mouse_coords_local_to_area(area: Rect, m: MouseEvent) -> Option<(u16, u16)> {
    if contains(area, m.column, m.row) {
        return Some((
            m.column.saturating_sub(area.x),
            m.row.saturating_sub(area.y),
        ));
    }

    // Nested containers may forward mouse coordinates already relative to this view.
    if m.column < area.width && m.row < area.height {
        return Some((m.column, m.row));
    }

    None
}

impl EditorPopupWindows {
    pub fn new(handle: &super::view::EditorViewHandle) -> Self {
        Self {
            hover_id: None,
            completion_id: None,
            hover: handle.hover_popup.clone(),
            completion: handle.completion_popup.clone(),
            theme: handle.theme.clone(),
            language_id: handle.language_id.clone(),
        }
    }

    pub fn sync(&mut self, wm: &mut WindowManager, bounds: Rect) {
        self.sync_hover(wm, bounds);
        self.sync_completion(wm, bounds);
    }

    fn sync_hover(&mut self, wm: &mut WindowManager, bounds: Rect) {
        let model = self.hover.get();
        match (model, self.hover_id) {
            (None, Some(id)) => {
                wm.close(id);
                self.hover_id = None;
            }
            (Some(model), None) => {
                let window = Window::new(
                    WindowKind::Tooltip,
                    "hover",
                    model.rect,
                    Box::new(HoverPopupView::new(
                        self.hover.clone(),
                        self.theme.clone(),
                        self.language_id.clone(),
                    )),
                )
                .with_decorations(popup_decorations());
                let id = wm.add_window(window, bounds);
                self.hover_id = Some(id);
            }
            (Some(model), Some(id)) => {
                if let Some(w) = wm.window_mut(id) {
                    w.rect.set(model.rect);
                    w.decorations.set(popup_decorations());
                }
                wm.bring_to_front(id);
            }
            (None, None) => {}
        }
    }

    fn sync_completion(&mut self, wm: &mut WindowManager, bounds: Rect) {
        let model = self.completion.get();
        match (model, self.completion_id) {
            (None, Some(id)) => {
                wm.close(id);
                self.completion_id = None;
            }
            (Some(model), None) => {
                let window = Window::new(
                    WindowKind::Tooltip,
                    "completion",
                    model.rect,
                    Box::new(CompletionPopupView::new(
                        self.completion.clone(),
                        self.theme.clone(),
                        self.language_id.clone(),
                    )),
                )
                .with_decorations(popup_decorations());
                let id = wm.add_window(window, bounds);
                self.completion_id = Some(id);
            }
            (Some(model), Some(id)) => {
                if let Some(w) = wm.window_mut(id) {
                    w.rect.set(model.rect);
                    w.decorations.set(popup_decorations());
                }
                wm.bring_to_front(id);
            }
            (None, None) => {}
        }
    }

    pub fn close_all(&mut self, wm: &mut WindowManager) {
        if let Some(id) = self.hover_id.take() {
            wm.close(id);
        }
        if let Some(id) = self.completion_id.take() {
            wm.close(id);
        }
    }
}
