//! Split-capable document tab view hosted inside the editor window.

use atto_ui::composable::{
    ComponentContext, EventResult, ScrollConfig, ScrollOffset, Scrollable as _, ScrollbarDrag,
    Scrollbars, draw_scrollbars, handle_scrollbar_mouse_event, should_show_scrollbar,
};
use atto_ui::reactive::{Binding, EventQueue};
use crossterm::event::{Event, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::actions::JumpTarget;

use super::util::{contains, mouse_coords_local_to_area};

#[derive(Clone)]
pub(super) struct SaveSettingsBindings {
    pub(super) format_on_save: Binding<bool>,
    pub(super) trim_trailing_whitespace_on_save: Binding<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TabCommand {
    SplitVertical,
    SplitHorizontal,
    CloseSplit,
    EditorAction(atto_ui_editor::EditorAction),
    FormatDocument { save_after: bool },
    JumpTo(JumpTarget),
    RequestDocumentSymbols,
    RequestWorkspaceSymbols(String),
}

pub(super) struct DocumentTabView {
    commands: EventQueue<TabCommand>,
    editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
    clipboard: Binding<String>,
    text: Binding<String>,
    save_settings: SaveSettingsBindings,
    language_id: String,
    syntax: atto_ui_editor::EditorSyntaxConfig,

    focused: SplitFocus,
    split: Option<atto_ui::composable::SplitterOrientation>,

    primary: atto_ui_editor::EditorView,
    secondary: Option<atto_ui_editor::EditorView>,

    scrollbar_drag: Option<(SplitFocus, ScrollbarDrag)>,
    last_layout: Option<TabLayout>,
    last_area: Option<Rect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SplitFocus {
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug)]
struct TabLayout {
    primary: Rect,
    divider: Option<Rect>,
    secondary: Option<Rect>,
}

impl DocumentTabView {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        commands: EventQueue<TabCommand>,
        editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
        clipboard: Binding<String>,
        text: Binding<String>,
        save_settings: SaveSettingsBindings,
        language_id: String,
        syntax: atto_ui_editor::EditorSyntaxConfig,
        lsp: atto_ui_editor::EditorLspMode,
        lsp_client: Option<atto_ui_editor::SharedEditorLspClient>,
    ) -> (Self, atto_ui_editor::EditorViewHandle) {
        let (primary, primary_handle) = build_editor_view(
            text.clone(),
            save_settings.clone(),
            clipboard.clone(),
            editor_theme.clone(),
            language_id.clone(),
            syntax.clone(),
            lsp.clone(),
            lsp_client,
        );

        let view = Self {
            commands,
            editor_theme,
            clipboard,
            text,
            save_settings,
            language_id,
            syntax,
            focused: SplitFocus::Primary,
            split: None,
            primary,
            secondary: None,
            scrollbar_drag: None,
            last_layout: None,
            last_area: None,
        };
        (view, primary_handle)
    }

    fn handle_commands(&mut self) {
        for cmd in self.commands.drain() {
            match cmd {
                TabCommand::SplitVertical => {
                    self.ensure_split(Some(atto_ui::composable::SplitterOrientation::Vertical))
                }
                TabCommand::SplitHorizontal => {
                    self.ensure_split(Some(atto_ui::composable::SplitterOrientation::Horizontal))
                }
                TabCommand::CloseSplit => self.close_split(),
                TabCommand::EditorAction(action) => {
                    let _ = self.handle_editor_action(action);
                }
                TabCommand::FormatDocument { save_after } => {
                    let _ = self.request_format_document(save_after);
                }
                TabCommand::JumpTo(target) => {
                    let _ = self.jump_to(target);
                }
                TabCommand::RequestDocumentSymbols => {
                    let _ = self.primary.request_document_symbols();
                }
                TabCommand::RequestWorkspaceSymbols(query) => {
                    let _ = self.primary.request_workspace_symbols(query);
                }
            }
        }
    }

    fn handle_editor_action(&mut self, action: atto_ui_editor::EditorAction) -> bool {
        match self.focused {
            SplitFocus::Primary => self.primary.handle_editor_action(action),
            SplitFocus::Secondary => {
                if let Some(view) = self.secondary.as_mut() {
                    view.handle_editor_action(action)
                } else {
                    self.primary.handle_editor_action(action)
                }
            }
        }
    }

    fn request_format_document(&mut self, save_after: bool) -> bool {
        self.primary.request_format_document_now(save_after)
    }

    fn jump_to(&mut self, target: JumpTarget) -> bool {
        let view = match self.focused {
            SplitFocus::Primary => &mut self.primary,
            SplitFocus::Secondary => {
                if let Some(view) = self.secondary.as_mut() {
                    view
                } else {
                    &mut self.primary
                }
            }
        };
        match target {
            JumpTarget::CharOffset { offset } => view.jump_to_offset(offset),
            JumpTarget::CharPosition { line, column } => view.jump_to_position(line, column),
            JumpTarget::Utf16Position { line, character } => {
                view.jump_to_utf16_position(line, character)
            }
        }
    }

    fn ensure_split(&mut self, orientation: Option<atto_ui::composable::SplitterOrientation>) {
        self.split = orientation;
        if self.secondary.is_some() {
            return;
        }

        // Secondary view shares the same text binding but disables LSP to avoid starting multiple
        // servers for the same document.
        let (secondary, _secondary_handle) = build_editor_view(
            self.text.clone(),
            self.save_settings.clone(),
            self.clipboard.clone(),
            self.editor_theme.clone(),
            self.language_id.clone(),
            self.syntax.clone(),
            atto_ui_editor::EditorLspMode::Disabled,
            None,
        );
        self.secondary = Some(secondary);
        self.focused = SplitFocus::Secondary;
    }

    fn close_split(&mut self) {
        self.secondary = None;
        self.split = None;
        self.focused = SplitFocus::Primary;
    }

    fn layout(&self, area: Rect) -> TabLayout {
        let Some(orientation) = self.split else {
            return TabLayout {
                primary: area,
                divider: None,
                secondary: None,
            };
        };
        if self.secondary.is_none() || area.width == 0 || area.height == 0 {
            return TabLayout {
                primary: area,
                divider: None,
                secondary: None,
            };
        }

        match orientation {
            atto_ui::composable::SplitterOrientation::Vertical => {
                let divider = 1u16;
                let available = area.width.saturating_sub(divider);
                let w1 = available / 2;
                let w2 = available.saturating_sub(w1);
                let primary = Rect {
                    x: area.x,
                    y: area.y,
                    width: w1,
                    height: area.height,
                };
                let divider_rect = Rect {
                    x: area.x.saturating_add(w1),
                    y: area.y,
                    width: divider.min(area.width.saturating_sub(w1)),
                    height: area.height,
                };
                let secondary = Rect {
                    x: area.x.saturating_add(w1).saturating_add(divider),
                    y: area.y,
                    width: w2,
                    height: area.height,
                };
                TabLayout {
                    primary,
                    divider: Some(divider_rect),
                    secondary: Some(secondary),
                }
            }
            atto_ui::composable::SplitterOrientation::Horizontal => {
                let divider = 1u16;
                let available = area.height.saturating_sub(divider);
                let h1 = available / 2;
                let h2 = available.saturating_sub(h1);
                let primary = Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: h1,
                };
                let divider_rect = Rect {
                    x: area.x,
                    y: area.y.saturating_add(h1),
                    width: area.width,
                    height: divider.min(area.height.saturating_sub(h1)),
                };
                let secondary = Rect {
                    x: area.x,
                    y: area.y.saturating_add(h1).saturating_add(divider),
                    width: area.width,
                    height: h2,
                };
                TabLayout {
                    primary,
                    divider: Some(divider_rect),
                    secondary: Some(secondary),
                }
            }
        }
    }

    fn hit_test(&self, m: MouseEvent) -> Option<SplitFocus> {
        let layout = self.last_layout?;
        if contains(layout.primary, m.column, m.row) {
            return Some(SplitFocus::Primary);
        }
        if let Some(r) = layout.secondary
            && contains(r, m.column, m.row)
        {
            return Some(SplitFocus::Secondary);
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn split_child_scrollbars(
        &self,
        area: Rect,
        layout: &TabLayout,
        child: SplitFocus,
        child_bounds: Rect,
        viewport_size: (u16, u16),
        show_v: bool,
        show_h: bool,
    ) -> Scrollbars {
        let orientation = self
            .split
            .unwrap_or(atto_ui::composable::SplitterOrientation::Vertical);

        let divider = layout.divider.unwrap_or(Rect {
            x: area.x.saturating_add(area.width),
            y: area.y.saturating_add(area.height),
            width: 0,
            height: 0,
        });

        let bounds_local = Rect {
            x: child_bounds.x.saturating_sub(area.x),
            y: child_bounds.y.saturating_sub(area.y),
            width: child_bounds.width,
            height: child_bounds.height,
        };
        let divider_local = Rect {
            x: divider.x.saturating_sub(area.x),
            y: divider.y.saturating_sub(area.y),
            width: divider.width,
            height: divider.height,
        };

        let vbar_x = match (orientation, child) {
            (atto_ui::composable::SplitterOrientation::Vertical, SplitFocus::Primary) => {
                divider_local.x
            }
            _ => bounds_local
                .x
                .saturating_add(bounds_local.width)
                .saturating_sub(1),
        };
        let hbar_y = match (orientation, child) {
            (atto_ui::composable::SplitterOrientation::Horizontal, SplitFocus::Primary) => {
                divider_local.y
            }
            _ => bounds_local
                .y
                .saturating_add(bounds_local.height)
                .saturating_sub(1),
        };

        let vbar = show_v
            .then(|| {
                if bounds_local.width == 0 || bounds_local.height == 0 {
                    return None;
                }
                Some(Rect {
                    x: vbar_x,
                    y: bounds_local.y,
                    width: 1,
                    height: bounds_local.height,
                })
            })
            .flatten();

        let hbar = show_h
            .then(|| {
                if bounds_local.width == 0 || bounds_local.height == 0 {
                    return None;
                }
                Some(Rect {
                    x: bounds_local.x,
                    y: hbar_y,
                    width: bounds_local.width,
                    height: 1,
                })
            })
            .flatten();

        let (vbar, hbar) = match (vbar, hbar) {
            (Some(mut v), Some(mut h)) => {
                if h.y >= v.y && h.y < v.y.saturating_add(v.height) {
                    v.height = h.y.saturating_sub(v.y);
                }
                if v.x >= h.x && v.x < h.x.saturating_add(h.width) {
                    h.width = v.x.saturating_sub(h.x);
                }

                let v = (v.width > 0 && v.height > 0).then_some(v);
                let h = (h.width > 0 && h.height > 0).then_some(h);
                (v, h)
            }
            other => other,
        };

        let content = Rect {
            x: 0,
            y: 0,
            width: viewport_size.0,
            height: viewport_size.1,
        };

        Scrollbars {
            viewport: content,
            content,
            vbar,
            hbar,
            thickness: 1,
        }
    }

    fn draw_split_scrollbars(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ComponentContext<'_>,
    ) {
        let Some(layout) = self.last_layout else {
            return;
        };
        if self.split.is_none() || self.secondary.is_none() {
            self.scrollbar_drag = None;
            return;
        }

        for (child, view, bounds) in [
            (SplitFocus::Primary, &self.primary, layout.primary),
            (
                SplitFocus::Secondary,
                self.secondary.as_ref().unwrap_or(&self.primary),
                layout.secondary.unwrap_or(layout.primary),
            ),
        ] {
            if !view.is_scrollable() || bounds.width == 0 || bounds.height == 0 {
                continue;
            }

            let cfg = view.scroll_config();
            let (content_w, content_h) = view.content_size();
            let (viewport_w, viewport_h) = view.viewport_size();
            let (scroll_x, scroll_y) = view.scroll_offset();

            let show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_h, viewport_h);
            let show_h = should_show_scrollbar(cfg.horizontal_scrollbar, content_w, viewport_w);
            if !show_v && !show_h {
                continue;
            }

            let scrollbars = self.split_child_scrollbars(
                area,
                &layout,
                child,
                bounds,
                (viewport_w, viewport_h),
                show_v,
                show_h,
            );

            draw_scrollbars(
                frame,
                area,
                scrollbars,
                (viewport_w, viewport_h),
                (content_w, content_h),
                ScrollOffset {
                    x: scroll_x,
                    y: scroll_y,
                },
                cfg,
                ctx.theme,
            );
        }
    }

    fn handle_split_scrollbar_event(
        &mut self,
        area: Rect,
        local_x: u16,
        local_y: u16,
        kind: MouseEventKind,
    ) -> Option<(SplitFocus, ScrollOffset)> {
        let layout = self.last_layout?;
        if self.split.is_none() || self.secondary.is_none() {
            self.scrollbar_drag = None;
            return None;
        }

        // Primary pane is always present.
        {
            let view = &self.primary;
            let bounds = layout.primary;

            let cfg = view.scroll_config();
            let (content_w, content_h) = view.content_size();
            let (viewport_w, viewport_h) = view.viewport_size();
            let (scroll_x, scroll_y) = view.scroll_offset();

            let show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_h, viewport_h);
            let show_h = should_show_scrollbar(cfg.horizontal_scrollbar, content_w, viewport_w);
            if show_v || show_h {
                let scrollbars = self.split_child_scrollbars(
                    area,
                    &layout,
                    SplitFocus::Primary,
                    bounds,
                    (viewport_w, viewport_h),
                    show_v,
                    show_h,
                );

                let mut drag = self
                    .scrollbar_drag
                    .and_then(|(which, drag)| (which == SplitFocus::Primary).then_some(drag));

                if let Some(next) = handle_scrollbar_mouse_event(
                    cfg,
                    scrollbars,
                    (content_w, content_h),
                    ScrollOffset {
                        x: scroll_x,
                        y: scroll_y,
                    },
                    &mut drag,
                    local_x,
                    local_y,
                    kind,
                ) {
                    self.scrollbar_drag = drag.map(|d| (SplitFocus::Primary, d));
                    return Some((SplitFocus::Primary, next));
                }
            }
        }

        // Secondary pane only exists while a split is active.
        if let Some(view) = self.secondary.as_ref()
            && let Some(bounds) = layout.secondary
        {
            let cfg = view.scroll_config();
            let (content_w, content_h) = view.content_size();
            let (viewport_w, viewport_h) = view.viewport_size();
            let (scroll_x, scroll_y) = view.scroll_offset();

            let show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_h, viewport_h);
            let show_h = should_show_scrollbar(cfg.horizontal_scrollbar, content_w, viewport_w);
            if show_v || show_h {
                let scrollbars = self.split_child_scrollbars(
                    area,
                    &layout,
                    SplitFocus::Secondary,
                    bounds,
                    (viewport_w, viewport_h),
                    show_v,
                    show_h,
                );

                let mut drag = self
                    .scrollbar_drag
                    .and_then(|(which, drag)| (which == SplitFocus::Secondary).then_some(drag));

                if let Some(next) = handle_scrollbar_mouse_event(
                    cfg,
                    scrollbars,
                    (content_w, content_h),
                    ScrollOffset {
                        x: scroll_x,
                        y: scroll_y,
                    },
                    &mut drag,
                    local_x,
                    local_y,
                    kind,
                ) {
                    self.scrollbar_drag = drag.map(|d| (SplitFocus::Secondary, d));
                    return Some((SplitFocus::Secondary, next));
                }
            }
        }

        None
    }
}

impl ::atto_ui::composable::Component for DocumentTabView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.handle_commands();
        self.last_area = Some(area);
        let layout = self.layout(area);
        self.last_layout = Some(layout);

        let (primary_focused, secondary_focused) = match self.focused {
            SplitFocus::Primary => (ctx.is_focused, false),
            SplitFocus::Secondary => (false, ctx.is_focused),
        };

        let primary_ctx = ComponentContext {
            is_focused: primary_focused,
            drag: None,
            ..ctx
        };
        self.primary.draw(frame, layout.primary, primary_ctx);

        if let Some(r) = layout.secondary
            && let Some(view) = self.secondary.as_mut()
        {
            let secondary_ctx = ComponentContext {
                is_focused: secondary_focused,
                drag: None,
                ..ctx
            };
            view.draw(frame, r, secondary_ctx);
        }

        if let Some(divider) = layout.divider
            && divider.width > 0
            && divider.height > 0
        {
            let style = ctx.theme.widget.dim;
            let border_set = ctx.theme.border_set(false);
            let symbol = match self
                .split
                .unwrap_or(atto_ui::composable::SplitterOrientation::Vertical)
            {
                atto_ui::composable::SplitterOrientation::Vertical => border_set.vertical_left,
                atto_ui::composable::SplitterOrientation::Horizontal => border_set.horizontal_top,
            };

            let buf = frame.buffer_mut();
            for y in divider.y..divider.y.saturating_add(divider.height) {
                for x in divider.x..divider.x.saturating_add(divider.width) {
                    buf[(x, y)].set_symbol(symbol).set_style(style);
                }
            }
        }

        self.draw_split_scrollbars(frame, area, ctx);
    }
}

impl ::atto_ui::composable::DragAndDrop for DocumentTabView {}

impl ::atto_ui::composable::Layout for DocumentTabView {}

impl ::atto_ui::composable::Scrollable for DocumentTabView {
    fn is_scrollable(&self) -> bool {
        if self.split.is_some() && self.secondary.is_some() {
            return false;
        }
        match self.focused {
            SplitFocus::Primary => self.primary.is_scrollable(),
            SplitFocus::Secondary => self
                .secondary
                .as_ref()
                .unwrap_or(&self.primary)
                .is_scrollable(),
        }
    }

    fn content_size(&self) -> (u16, u16) {
        match self.focused {
            SplitFocus::Primary => self.primary.content_size(),
            SplitFocus::Secondary => self
                .secondary
                .as_ref()
                .unwrap_or(&self.primary)
                .content_size(),
        }
    }

    fn viewport_size(&self) -> (u16, u16) {
        match self.focused {
            SplitFocus::Primary => self.primary.viewport_size(),
            SplitFocus::Secondary => self
                .secondary
                .as_ref()
                .unwrap_or(&self.primary)
                .viewport_size(),
        }
    }

    fn scroll_offset(&self) -> (u16, u16) {
        match self.focused {
            SplitFocus::Primary => self.primary.scroll_offset(),
            SplitFocus::Secondary => self
                .secondary
                .as_ref()
                .unwrap_or(&self.primary)
                .scroll_offset(),
        }
    }

    fn scroll_config(&self) -> ScrollConfig {
        match self.focused {
            SplitFocus::Primary => self.primary.scroll_config(),
            SplitFocus::Secondary => self
                .secondary
                .as_ref()
                .unwrap_or(&self.primary)
                .scroll_config(),
        }
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        match self.focused {
            SplitFocus::Primary => self.primary.set_scroll_offset(x, y),
            SplitFocus::Secondary => {
                if let Some(view) = self.secondary.as_mut() {
                    view.set_scroll_offset(x, y);
                } else {
                    self.primary.set_scroll_offset(x, y);
                }
            }
        }
    }
}

impl ::atto_ui::composable::FocusNav for DocumentTabView {
    fn is_focusable(&self) -> bool {
        true
    }

    fn focus_first(&mut self) -> bool {
        self.focused = SplitFocus::Primary;
        true
    }
}

impl ::atto_ui::composable::DynamicTree for DocumentTabView {}

impl ::atto_ui::composable::EventHandling for DocumentTabView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.handle_commands();

        if let Event::Mouse(m) = event {
            if let Some(area) = self.last_area
                && let Some((local_x, local_y)) =
                    mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
                && let Some((which, next)) =
                    self.handle_split_scrollbar_event(area, local_x, local_y, m.kind)
            {
                match which {
                    SplitFocus::Primary => {
                        self.primary.set_scroll_offset(next.x, next.y);
                    }
                    SplitFocus::Secondary => {
                        if let Some(view) = self.secondary.as_mut() {
                            view.set_scroll_offset(next.x, next.y);
                        } else {
                            self.primary.set_scroll_offset(next.x, next.y);
                        }
                    }
                }
                self.focused = which;
                return EventResult::consumed();
            }

            if matches!(
                m.kind,
                MouseEventKind::Down(crossterm::event::MouseButton::Left)
            ) && let Some(focus) = self.hit_test(*m)
            {
                self.focused = focus;
            }
        }

        let (primary_focused, secondary_focused) = match self.focused {
            SplitFocus::Primary => (ctx.is_focused, false),
            SplitFocus::Secondary => (false, ctx.is_focused),
        };

        // Route the event to the pane that currently owns focus.
        match self.focused {
            SplitFocus::Primary => {
                let child_ctx = ComponentContext {
                    is_focused: primary_focused,
                    drag: None,
                    ..ctx
                };
                self.primary.handle_event(event, child_ctx)
            }
            SplitFocus::Secondary => {
                if let Some(view) = self.secondary.as_mut() {
                    let child_ctx = ComponentContext {
                        is_focused: secondary_focused,
                        drag: None,
                        ..ctx
                    };
                    view.handle_event(event, child_ctx)
                } else {
                    let child_ctx = ComponentContext {
                        is_focused: primary_focused,
                        drag: None,
                        ..ctx
                    };
                    self.primary.handle_event(event, child_ctx)
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_editor_view(
    text: Binding<String>,
    save_settings: SaveSettingsBindings,
    clipboard: Binding<String>,
    theme: Binding<atto_ui_editor::EditorThemeSet>,
    language_id: String,
    syntax: atto_ui_editor::EditorSyntaxConfig,
    lsp: atto_ui_editor::EditorLspMode,
    lsp_client: Option<atto_ui_editor::SharedEditorLspClient>,
) -> (atto_ui_editor::EditorView, atto_ui_editor::EditorViewHandle) {
    let mut cfg = atto_ui_editor::EditorConfig::new(text);
    cfg.clipboard = clipboard;
    cfg.format_on_save = save_settings.format_on_save;
    cfg.trim_trailing_whitespace_on_save = save_settings.trim_trailing_whitespace_on_save;
    cfg.comment
        .set(crate::language::comment_config_for_language(&language_id));
    cfg.indent
        .language
        .set(crate::language::indentation_config_for_language(
            &language_id,
        ));
    cfg.auto_pairs
        .set(crate::language::auto_pairs_config_for_language(
            &language_id,
        ));
    cfg.language_id.set(language_id);
    cfg.syntax.set(syntax);
    cfg.lsp.set(lsp);

    let (mut view, handle) = atto_ui_editor::EditorView::new(cfg, theme);
    if let Some(client) = lsp_client {
        view.set_lsp_client(client);
    }
    (view, handle)
}
