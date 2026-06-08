use crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};
use editor_core::Position;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use serde_json::Value;

use atto_ui::composable::{ComponentContext, EventResult, MouseCoordinateSpace};
use atto_ui::reactive::Binding;
use atto_ui::wm::{Window, WindowDecorations, WindowId, WindowKind, WindowManager};

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
    /// The document position this popup is describing (used for suppression / re-show logic).
    pub anchor: Position,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeActionItemView {
    pub title: String,
    pub kind: Option<String>,
    pub is_preferred: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeActionPopupModel {
    /// Desired popup rect in screen coordinates.
    pub rect: Rect,
    pub items: Vec<CodeActionItemView>,
    pub selected: usize,
    /// First visible item index (for scrolling long lists).
    pub scroll: usize,
    /// When set, the editor should apply the selected code action and then clear this field.
    pub accept: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenamePopupModel {
    /// Desired popup rect in screen coordinates.
    pub rect: Rect,
    pub value: String,
    /// Cursor position as a character index within `value`.
    pub cursor: usize,
    /// When true, the next text-editing key replaces the prepared default value.
    pub replace_on_input: bool,
}

#[derive(Clone, Debug)]
struct HoverPopupView {
    model: Binding<Option<HoverPopupModel>>,
    dismissed: Binding<Option<Position>>,
    theme: Binding<EditorThemeSet>,
    language_id: Binding<String>,
    last_area: Option<Rect>,
}

impl HoverPopupView {
    fn new(
        model: Binding<Option<HoverPopupModel>>,
        dismissed: Binding<Option<Position>>,
        theme: Binding<EditorThemeSet>,
        language_id: Binding<String>,
    ) -> Self {
        Self {
            model,
            dismissed,
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

impl ::atto_ui::composable::Component for HoverPopupView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, _ctx: ComponentContext<'_>) {
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

impl ::atto_ui::composable::DragAndDrop for HoverPopupView {}

impl ::atto_ui::composable::Layout for HoverPopupView {}

impl ::atto_ui::composable::Scrollable for HoverPopupView {}

impl ::atto_ui::composable::FocusNav for HoverPopupView {}

impl ::atto_ui::composable::DynamicTree for HoverPopupView {}

impl ::atto_ui::composable::EventHandling for HoverPopupView {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if matches!(
            event,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                ..
            })
        ) {
            // Clicking a hover tooltip should always dismiss it.
            if let Some(model) = self.model.get() {
                self.dismissed.set(Some(model.anchor));
            }
            self.model.set(None);
            return EventResult::consumed();
        }
        EventResult::ignored()
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

impl ::atto_ui::composable::Component for CompletionPopupView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, _ctx: ComponentContext<'_>) {
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

impl ::atto_ui::composable::DragAndDrop for CompletionPopupView {}

impl ::atto_ui::composable::Layout for CompletionPopupView {}

impl ::atto_ui::composable::Scrollable for CompletionPopupView {}

impl ::atto_ui::composable::FocusNav for CompletionPopupView {}

impl ::atto_ui::composable::DynamicTree for CompletionPopupView {}

impl ::atto_ui::composable::EventHandling for CompletionPopupView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Some(mut model) = self.model.get() else {
            return EventResult::ignored();
        };

        let Event::Mouse(m) = event else {
            return EventResult::ignored();
        };
        let Some(area) = self.last_area else {
            return EventResult::ignored();
        };

        let Some((local_x, local_y)) =
            mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
        else {
            return EventResult::ignored();
        };

        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // The popup view draws its own border, so y=0/y=height-1 are borders.
                if area.width < 3 || area.height < 3 {
                    return EventResult::ignored();
                }
                if local_x == 0
                    || local_y == 0
                    || local_x + 1 >= area.width
                    || local_y + 1 >= area.height
                {
                    return EventResult::ignored();
                }

                let item_row = (local_y - 1) as usize;
                let idx = model.scroll.saturating_add(item_row);
                if idx >= model.items.len() {
                    return EventResult::ignored();
                }

                model.selected = idx;
                model.accept = Some(idx);
                self.model.set(Some(model));
                EventResult::consumed()
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
                EventResult::consumed()
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
                EventResult::consumed()
            }
            _ => EventResult::ignored(),
        }
    }
}

#[derive(Clone, Debug)]
struct CodeActionPopupView {
    model: Binding<Option<CodeActionPopupModel>>,
    theme: Binding<EditorThemeSet>,
    language_id: Binding<String>,
    last_area: Option<Rect>,
}

impl CodeActionPopupView {
    fn new(
        model: Binding<Option<CodeActionPopupModel>>,
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

impl ::atto_ui::composable::Component for CodeActionPopupView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, _ctx: ComponentContext<'_>) {
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
            lines.push(Line::from(Span::styled(
                code_action_line(item, area.width.saturating_sub(2) as usize),
                style,
            )));
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.popup_border)
            .style(theme.popup);

        frame.render_widget(Paragraph::new(lines).block(block), area);
    }
}

impl ::atto_ui::composable::DragAndDrop for CodeActionPopupView {}

impl ::atto_ui::composable::Layout for CodeActionPopupView {}

impl ::atto_ui::composable::Scrollable for CodeActionPopupView {}

impl ::atto_ui::composable::FocusNav for CodeActionPopupView {}

impl ::atto_ui::composable::DynamicTree for CodeActionPopupView {}

impl ::atto_ui::composable::EventHandling for CodeActionPopupView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Some(mut model) = self.model.get() else {
            return EventResult::ignored();
        };

        let Event::Mouse(m) = event else {
            return EventResult::ignored();
        };
        let Some(area) = self.last_area else {
            return EventResult::ignored();
        };

        let Some((local_x, local_y)) =
            mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
        else {
            return EventResult::ignored();
        };

        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if area.width < 3 || area.height < 3 {
                    return EventResult::ignored();
                }
                if local_x == 0
                    || local_y == 0
                    || local_x + 1 >= area.width
                    || local_y + 1 >= area.height
                {
                    return EventResult::ignored();
                }

                let item_row = (local_y - 1) as usize;
                let idx = model.scroll.saturating_add(item_row);
                if idx >= model.items.len() {
                    return EventResult::ignored();
                }

                model.selected = idx;
                model.accept = Some(idx);
                self.model.set(Some(model));
                EventResult::consumed()
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
                EventResult::consumed()
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
                EventResult::consumed()
            }
            _ => EventResult::ignored(),
        }
    }
}

#[derive(Clone, Debug)]
struct RenamePopupView {
    model: Binding<Option<RenamePopupModel>>,
    theme: Binding<EditorThemeSet>,
    language_id: Binding<String>,
}

impl RenamePopupView {
    fn new(
        model: Binding<Option<RenamePopupModel>>,
        theme: Binding<EditorThemeSet>,
        language_id: Binding<String>,
    ) -> Self {
        Self {
            model,
            theme,
            language_id,
        }
    }

    fn editor_theme(&self) -> EditorTheme {
        let theme_set = self.theme.get();
        let language_id = self.language_id.get();
        theme_set.for_language(language_id.as_str()).clone()
    }
}

impl ::atto_ui::composable::Component for RenamePopupView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, _ctx: ComponentContext<'_>) {
        let Some(model) = self.model.get() else {
            return;
        };
        let theme = self.editor_theme();
        let cursor = model.cursor.min(model.value.chars().count());
        let mut before = String::new();
        let mut at_cursor = " ".to_string();
        let mut after = String::new();
        for (idx, ch) in model.value.chars().enumerate() {
            if idx < cursor {
                before.push(ch);
            } else if idx == cursor {
                at_cursor = ch.to_string();
            } else {
                after.push(ch);
            }
        }

        let line = Line::from(vec![
            Span::styled("Rename: ", theme.popup),
            Span::styled(before, theme.popup),
            Span::styled(at_cursor, theme.popup_selected),
            Span::styled(after, theme.popup),
        ]);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.popup_border)
            .style(theme.popup);

        frame.render_widget(Paragraph::new(vec![line]).block(block), area);
    }
}

impl ::atto_ui::composable::DragAndDrop for RenamePopupView {}

impl ::atto_ui::composable::Layout for RenamePopupView {}

impl ::atto_ui::composable::Scrollable for RenamePopupView {}

impl ::atto_ui::composable::FocusNav for RenamePopupView {}

impl ::atto_ui::composable::DynamicTree for RenamePopupView {}

impl ::atto_ui::composable::EventHandling for RenamePopupView {}

pub(crate) fn code_action_line(item: &CodeActionItemView, max_width: usize) -> String {
    let mut line = String::new();
    if item.is_preferred {
        line.push_str("* ");
    } else {
        line.push_str("  ");
    }
    line.push_str(&item.title);
    if let Some(kind) = &item.kind
        && !kind.is_empty()
    {
        line.push_str("  ");
        line.push_str(kind);
    }

    if max_width >= 3 && line.chars().count() > max_width {
        line.chars()
            .take(max_width.saturating_sub(3))
            .collect::<String>()
            + "..."
    } else {
        line
    }
}

fn popup_decorations() -> WindowDecorations {
    WindowDecorations {
        border: atto_ui::wm::WindowBorderStyle::Borderless,
        shadow: false,
        buttons: atto_ui::wm::WindowButtons {
            minimize: false,
            maximize: false,
            close: false,
        },
    }
}

/// Manages the tooltip windows used by an [`crate::EditorView`].
///
/// This is intentionally separate from the `Component` itself: it needs access to the host
/// `WindowManager` to create/close popup windows.
#[derive(Debug)]
pub struct EditorPopupWindows {
    hover_id: Option<WindowId>,
    completion_id: Option<WindowId>,
    code_action_id: Option<WindowId>,
    rename_id: Option<WindowId>,
    hover: Binding<Option<HoverPopupModel>>,
    hover_dismissed: Binding<Option<Position>>,
    completion: Binding<Option<CompletionPopupModel>>,
    code_action: Binding<Option<CodeActionPopupModel>>,
    rename: Binding<Option<RenamePopupModel>>,
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

fn mouse_coords_local_to_area(
    area: Rect,
    m: MouseEvent,
    coordinate_space: MouseCoordinateSpace,
) -> Option<(u16, u16)> {
    match coordinate_space {
        MouseCoordinateSpace::Absolute => contains(area, m.column, m.row).then(|| {
            (
                m.column.saturating_sub(area.x),
                m.row.saturating_sub(area.y),
            )
        }),
        MouseCoordinateSpace::Local => {
            (area.width > 0 && area.height > 0 && m.column < area.width && m.row < area.height)
                .then_some((m.column, m.row))
        }
    }
}

impl EditorPopupWindows {
    pub fn new(handle: &super::view::EditorViewHandle) -> Self {
        Self {
            hover_id: None,
            completion_id: None,
            code_action_id: None,
            rename_id: None,
            hover: handle.hover_popup.clone(),
            hover_dismissed: handle.hover_popup_dismissed.clone(),
            completion: handle.completion_popup.clone(),
            code_action: handle.code_action_popup.clone(),
            rename: handle.rename_popup.clone(),
            theme: handle.theme.clone(),
            language_id: handle.language_id.clone(),
        }
    }

    pub fn sync(&mut self, wm: &mut WindowManager, bounds: Rect) {
        self.sync_hover(wm, bounds);
        self.sync_completion(wm, bounds);
        self.sync_code_action(wm, bounds);
        self.sync_rename(wm, bounds);
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
                        self.hover_dismissed.clone(),
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

    fn sync_code_action(&mut self, wm: &mut WindowManager, bounds: Rect) {
        let model = self.code_action.get();
        match (model, self.code_action_id) {
            (None, Some(id)) => {
                wm.close(id);
                self.code_action_id = None;
            }
            (Some(model), None) => {
                let window = Window::new(
                    WindowKind::Tooltip,
                    "code actions",
                    model.rect,
                    Box::new(CodeActionPopupView::new(
                        self.code_action.clone(),
                        self.theme.clone(),
                        self.language_id.clone(),
                    )),
                )
                .with_decorations(popup_decorations());
                let id = wm.add_window(window, bounds);
                self.code_action_id = Some(id);
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

    fn sync_rename(&mut self, wm: &mut WindowManager, bounds: Rect) {
        let model = self.rename.get();
        match (model, self.rename_id) {
            (None, Some(id)) => {
                wm.close(id);
                self.rename_id = None;
            }
            (Some(model), None) => {
                let window = Window::new(
                    WindowKind::Tooltip,
                    "rename",
                    model.rect,
                    Box::new(RenamePopupView::new(
                        self.rename.clone(),
                        self.theme.clone(),
                        self.language_id.clone(),
                    )),
                )
                .with_decorations(popup_decorations());
                let id = wm.add_window(window, bounds);
                self.rename_id = Some(id);
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
        if let Some(id) = self.code_action_id.take() {
            wm.close(id);
        }
        if let Some(id) = self.rename_id.take() {
            wm.close(id);
        }
    }
}
