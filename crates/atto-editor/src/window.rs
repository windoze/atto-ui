use std::path::{Path, PathBuf};

use anyhow::Result;
use atto_ui::composable::{
    ComponentContext, EventResult, ScrollConfig, ScrollOffset, Scrollable, ScrollbarDrag,
    ScrollbarHost, Scrollbars, TitleBarContent, TitleBarContext, draw_scrollbars,
    handle_scrollbar_mouse_event, should_show_scrollbar,
};
use atto_ui::reactive::{Binding, DirtyObserver, EventQueue};
use crossterm::event::{Event, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::actions::AppAction;
use crate::language::{guess_language_id, lsp_mode_for_file, syntax_config_for_file};

#[derive(Clone, Debug)]
pub enum EditorWindowCommand {
    OpenFile(PathBuf),

    SaveActive,
    SaveAs(PathBuf),
    CloseActiveTab,

    SplitVertical,
    SplitHorizontal,
    CloseSplit,
}

#[derive(Clone)]
pub struct EditorWindowHandle {
    pub commands: EventQueue<EditorWindowCommand>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabCommand {
    SplitVertical,
    SplitHorizontal,
    CloseSplit,
}

struct TabState {
    path: Option<PathBuf>,
    title_base: String,
    text: Binding<String>,
    last_saved_text: String,
    text_observer: DirtyObserver,
    is_dirty: bool,
    commands: EventQueue<TabCommand>,
}

pub struct EditorWindowView {
    _actions: EventQueue<AppAction>,
    commands: EventQueue<EditorWindowCommand>,

    editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
    clipboard: Binding<String>,

    tab_window: atto_ui::composable::TabWindow,
    tabs: Vec<TabState>,
}

impl EditorWindowView {
    pub fn new(
        actions: EventQueue<AppAction>,
        commands: EventQueue<EditorWindowCommand>,
        editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
        clipboard: Binding<String>,
    ) -> Self {
        Self {
            _actions: actions,
            commands,
            editor_theme,
            clipboard,
            tab_window: atto_ui::composable::TabWindow::new(),
            tabs: Vec::new(),
        }
    }

    pub fn handle(commands: EventQueue<EditorWindowCommand>) -> EditorWindowHandle {
        EditorWindowHandle { commands }
    }

    fn canonicalize_best_effort(path: &Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn open_file_in_tab(&mut self, path: PathBuf) {
        let path = Self::canonicalize_best_effort(&path);

        if let Some((idx, _)) = self
            .tabs
            .iter()
            .enumerate()
            .find(|(_i, tab)| tab.path.as_ref().is_some_and(|p| p == &path))
        {
            let _ = self.tab_window.select_tab(idx);
            return;
        }

        let initial_text = std::fs::read_to_string(&path).unwrap_or_default();
        let title_base = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<file>")
            .to_string();

        let text: Binding<String> = initial_text.clone().into();
        let tab_commands: EventQueue<TabCommand> = EventQueue::new();

        let language_id = guess_language_id(&path);
        let syntax = syntax_config_for_file(&path, &language_id);
        let lsp = lsp_mode_for_file(&path, &language_id);

        let tab_view = DocumentTabView::new(
            tab_commands.clone(),
            self.editor_theme.clone(),
            self.clipboard.clone(),
            text.clone(),
            language_id,
            syntax,
            lsp,
        );

        let idx = self
            .tab_window
            .add_tab(title_base.clone(), Box::new(tab_view));
        let _ = self.tab_window.select_tab(idx);

        let mut text_observer = text.dirty_observer();
        text.check_dirty(&mut text_observer);

        self.tabs.push(TabState {
            path: Some(path.clone()),
            title_base,
            text: text.clone(),
            last_saved_text: initial_text,
            text_observer,
            is_dirty: false,
            commands: tab_commands.clone(),
        });
    }

    fn close_active_tab(&mut self) {
        let Some(active) = self.tab_window.active_tab() else {
            return;
        };
        if self.tab_window.remove_tab(active).is_some() && active < self.tabs.len() {
            self.tabs.remove(active);
        }
    }

    fn send_tab_command_to_active(&mut self, cmd: TabCommand) {
        let Some(active) = self.tab_window.active_tab() else {
            return;
        };
        if let Some(tab) = self.tabs.get(active) {
            tab.commands.push(cmd);
        }
    }

    fn save_active(&mut self) -> Result<()> {
        let Some(active) = self.tab_window.active_tab() else {
            return Ok(());
        };
        let Some(tab) = self.tabs.get_mut(active) else {
            return Ok(());
        };
        let Some(path) = tab.path.clone() else {
            return Ok(());
        };

        std::fs::write(&path, tab.text.get())?;
        tab.last_saved_text = tab.text.get();
        tab.is_dirty = false;
        self.tab_window
            .set_tab_title(active, tab.title_base.clone());
        Ok(())
    }

    fn save_as_active(&mut self, path: PathBuf) -> Result<()> {
        let Some(active) = self.tab_window.active_tab() else {
            return Ok(());
        };
        let Some(tab) = self.tabs.get_mut(active) else {
            return Ok(());
        };

        std::fs::write(&path, tab.text.get())?;
        tab.path = Some(Self::canonicalize_best_effort(&path));
        tab.title_base = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<file>")
            .to_string();
        tab.last_saved_text = tab.text.get();
        tab.is_dirty = false;
        self.tab_window
            .set_tab_title(active, tab.title_base.clone());
        Ok(())
    }

    fn update_tab_titles(&mut self) {
        for (idx, tab) in self.tabs.iter_mut().enumerate() {
            if !tab.text.check_dirty(&mut tab.text_observer) {
                continue;
            }
            tab.is_dirty = tab.text.get() != tab.last_saved_text;
            let title = if tab.is_dirty {
                format!("{}*", tab.title_base)
            } else {
                tab.title_base.clone()
            };
            self.tab_window.set_tab_title(idx, title);
        }
    }

    #[cfg(any())]
    fn sidebar_layout(&self, area: Rect) -> EditorWindowLayout {
        if !self.sidebar_visible.get() || area.width == 0 || area.height == 0 {
            return EditorWindowLayout {
                sidebar: Rect::default(),
                divider: Rect::default(),
                main: area,
            };
        }

        let divider = 1u16;
        let max_sidebar_w = area.width.saturating_sub(divider);
        let sidebar_w = self
            .sidebar_width
            .get()
            .min(max_sidebar_w.saturating_sub(8))
            .max(12)
            .min(max_sidebar_w);

        match self.sidebar_side.get() {
            SidebarSide::Left => {
                let sidebar = Rect {
                    x: area.x,
                    y: area.y,
                    width: sidebar_w,
                    height: area.height,
                };
                let divider_w = divider.min(area.width.saturating_sub(sidebar_w));
                let divider = Rect {
                    x: area.x.saturating_add(sidebar_w),
                    y: area.y,
                    width: divider_w,
                    height: area.height,
                };
                let main = Rect {
                    x: divider.x.saturating_add(divider.width),
                    y: area.y,
                    width: area
                        .width
                        .saturating_sub(sidebar_w.saturating_add(divider_w)),
                    height: area.height,
                };
                EditorWindowLayout {
                    sidebar,
                    divider,
                    main,
                }
            }
            SidebarSide::Right => {
                let divider_w = divider.min(area.width.saturating_sub(sidebar_w));
                let main_w = area
                    .width
                    .saturating_sub(sidebar_w.saturating_add(divider_w));
                let sidebar = Rect {
                    x: area.x.saturating_add(main_w).saturating_add(divider_w),
                    y: area.y,
                    width: sidebar_w,
                    height: area.height,
                };
                let main = Rect {
                    x: area.x,
                    y: area.y,
                    width: main_w,
                    height: area.height,
                };
                let divider = Rect {
                    x: area.x.saturating_add(main_w),
                    y: area.y,
                    width: divider_w,
                    height: area.height,
                };
                EditorWindowLayout {
                    sidebar,
                    divider,
                    main,
                }
            }
        }
    }

    #[cfg(any())]
    fn hit_test_pane(&self, m: MouseEvent) -> Option<FocusPane> {
        let Some(layout) = self.last_layout else {
            return None;
        };
        if layout.sidebar.width > 0
            && layout.sidebar.height > 0
            && contains(layout.sidebar, m.column, m.row)
        {
            return Some(FocusPane::Sidebar);
        }
        if layout.main.width > 0 && layout.main.height > 0 && contains(layout.main, m.column, m.row)
        {
            return Some(FocusPane::Editor);
        }
        None
    }

    #[cfg(any())]
    fn split_left_pane(&self) -> FocusPane {
        match self.sidebar_side.get() {
            SidebarSide::Left => FocusPane::Sidebar,
            SidebarSide::Right => FocusPane::Editor,
        }
    }

    #[cfg(any())]
    fn split_pane_scrollbars(
        &self,
        area: Rect,
        layout: &EditorWindowLayout,
        pane: FocusPane,
        bounds: Rect,
        viewport_size: (u16, u16),
        show_v: bool,
        show_h: bool,
    ) -> Scrollbars {
        let bounds_local = Rect {
            x: bounds.x.saturating_sub(area.x),
            y: bounds.y.saturating_sub(area.y),
            width: bounds.width,
            height: bounds.height,
        };
        let divider_local = Rect {
            x: layout.divider.x.saturating_sub(area.x),
            y: layout.divider.y.saturating_sub(area.y),
            width: layout.divider.width,
            height: layout.divider.height,
        };

        let vbar_x = if pane == self.split_left_pane() {
            divider_local.x
        } else {
            bounds_local
                .x
                .saturating_add(bounds_local.width)
                .saturating_sub(1)
        };
        let hbar_y = bounds_local
            .y
            .saturating_add(bounds_local.height)
            .saturating_sub(1);

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

    #[cfg(any())]
    fn draw_split_scrollbars(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ComponentContext<'_>,
    ) {
        let Some(layout) = self.last_layout else {
            return;
        };
        if !self.sidebar_visible.get() || layout.divider.width == 0 || layout.divider.height == 0 {
            self.scrollbar_drag = None;
            return;
        }

        // Only the left pane needs internal scrollbars (mounted on the divider). The right pane
        // uses the window-border scrollbar.
        match self.split_left_pane() {
            FocusPane::Sidebar => {
                if !self.file_tree.is_scrollable()
                    || layout.sidebar.width == 0
                    || layout.sidebar.height == 0
                {
                    return;
                }

                let cfg = self.file_tree.scroll_config();
                let (content_w, content_h) = self.file_tree.content_size();
                let (viewport_w, viewport_h) = self.file_tree.viewport_size();
                let (scroll_x, scroll_y) = self.file_tree.scroll_offset();

                let show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_h, viewport_h);
                let show_h = should_show_scrollbar(cfg.horizontal_scrollbar, content_w, viewport_w);
                if !show_v && !show_h {
                    return;
                }

                let scrollbars = self.split_pane_scrollbars(
                    area,
                    &layout,
                    FocusPane::Sidebar,
                    layout.sidebar,
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
            FocusPane::Editor => {
                if !self.tab_window.is_scrollable()
                    || layout.main.width == 0
                    || layout.main.height == 0
                {
                    return;
                }

                let cfg = self.tab_window.scroll_config();
                let (content_w, content_h) = self.tab_window.content_size();
                let (viewport_w, viewport_h) = self.tab_window.viewport_size();
                let (scroll_x, scroll_y) = self.tab_window.scroll_offset();

                let show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_h, viewport_h);
                let show_h = should_show_scrollbar(cfg.horizontal_scrollbar, content_w, viewport_w);
                if !show_v && !show_h {
                    return;
                }

                let scrollbars = self.split_pane_scrollbars(
                    area,
                    &layout,
                    FocusPane::Editor,
                    layout.main,
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
    }

    #[cfg(any())]
    fn handle_split_scrollbar_event(
        &mut self,
        area: Rect,
        local_x: u16,
        local_y: u16,
        kind: MouseEventKind,
    ) -> Option<(FocusPane, ScrollOffset)> {
        let Some(layout) = self.last_layout else {
            return None;
        };
        if !self.sidebar_visible.get() || layout.divider.width == 0 || layout.divider.height == 0 {
            self.scrollbar_drag = None;
            return None;
        }

        match self.split_left_pane() {
            FocusPane::Sidebar => {
                if !self.file_tree.is_scrollable()
                    || layout.sidebar.width == 0
                    || layout.sidebar.height == 0
                {
                    return None;
                }

                let cfg = self.file_tree.scroll_config();
                let (content_w, content_h) = self.file_tree.content_size();
                let (viewport_w, viewport_h) = self.file_tree.viewport_size();
                let (scroll_x, scroll_y) = self.file_tree.scroll_offset();

                let show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_h, viewport_h);
                let show_h = should_show_scrollbar(cfg.horizontal_scrollbar, content_w, viewport_w);
                if !show_v && !show_h {
                    return None;
                }

                let scrollbars = self.split_pane_scrollbars(
                    area,
                    &layout,
                    FocusPane::Sidebar,
                    layout.sidebar,
                    (viewport_w, viewport_h),
                    show_v,
                    show_h,
                );

                let mut drag = self
                    .scrollbar_drag
                    .and_then(|(pane, drag)| (pane == FocusPane::Sidebar).then_some(drag));

                let next = handle_scrollbar_mouse_event(
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
                )?;

                self.scrollbar_drag = drag.map(|d| (FocusPane::Sidebar, d));
                Some((FocusPane::Sidebar, next))
            }
            FocusPane::Editor => {
                if !self.tab_window.is_scrollable()
                    || layout.main.width == 0
                    || layout.main.height == 0
                {
                    return None;
                }

                let cfg = self.tab_window.scroll_config();
                let (content_w, content_h) = self.tab_window.content_size();
                let (viewport_w, viewport_h) = self.tab_window.viewport_size();
                let (scroll_x, scroll_y) = self.tab_window.scroll_offset();

                let show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_h, viewport_h);
                let show_h = should_show_scrollbar(cfg.horizontal_scrollbar, content_w, viewport_w);
                if !show_v && !show_h {
                    return None;
                }

                let scrollbars = self.split_pane_scrollbars(
                    area,
                    &layout,
                    FocusPane::Editor,
                    layout.main,
                    (viewport_w, viewport_h),
                    show_v,
                    show_h,
                );

                let mut drag = self
                    .scrollbar_drag
                    .and_then(|(pane, drag)| (pane == FocusPane::Editor).then_some(drag));

                let next = handle_scrollbar_mouse_event(
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
                )?;

                self.scrollbar_drag = drag.map(|d| (FocusPane::Editor, d));
                Some((FocusPane::Editor, next))
            }
        }
    }

    #[cfg(any())]
    fn maybe_open_selected_from_sidebar(&mut self, new_window: bool) -> bool {
        let Some(id) = self.tree_selection.get() else {
            return false;
        };
        let kind = self.tree_id_to_kind.get(&id).copied();
        if kind != Some(FileTreeNodeKind::File) {
            return false;
        }
        let Some(path) = self.tree_id_to_path.get(&id).cloned() else {
            return false;
        };

        if new_window {
            self.actions.push(AppAction::OpenFileInNewWindow(path));
        } else {
            self.open_file_in_tab(path);
        }
        true
    }

    fn handle_commands(&mut self) {
        for cmd in self.commands.drain() {
            match cmd {
                EditorWindowCommand::OpenFile(path) => self.open_file_in_tab(path),
                EditorWindowCommand::SaveActive => {
                    let _ = self.save_active();
                }
                EditorWindowCommand::SaveAs(path) => {
                    let _ = self.save_as_active(path);
                }
                EditorWindowCommand::CloseActiveTab => self.close_active_tab(),
                EditorWindowCommand::SplitVertical => {
                    self.send_tab_command_to_active(TabCommand::SplitVertical)
                }
                EditorWindowCommand::SplitHorizontal => {
                    self.send_tab_command_to_active(TabCommand::SplitHorizontal)
                }
                EditorWindowCommand::CloseSplit => {
                    self.send_tab_command_to_active(TabCommand::CloseSplit)
                }
            }
        }
    }
}

#[cfg(any())]
impl ::atto_ui::composable::Component for EditorWindowView {
    fn titlebar(&mut self, ctx: TitleBarContext<'_>) -> Option<TitleBarContent> {
        self.tab_window.titlebar(ctx)
    }

    fn handle_titlebar_event(&mut self, event: &Event, ctx: TitleBarContext<'_>) -> EventResult {
        self.tab_window.handle_titlebar_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.handle_commands();
        self.update_tab_titles();

        self.last_area = Some(area);
        let layout = self.sidebar_layout(area);
        self.last_layout = Some(layout);

        if self.sidebar_visible.get() && layout.sidebar.width > 0 {
            let child_ctx = ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: ctx.is_focused && self.focused == FocusPane::Sidebar,
                scrollbar_host: if matches!(ctx.scrollbar_host, ScrollbarHost::Window) {
                    ScrollbarHost::Window
                } else {
                    ctx.scrollbar_host.for_child()
                },
                tab_mode: ctx.tab_mode.for_child(),
            };
            self.file_tree.draw(frame, layout.sidebar, child_ctx);
        }

        let child_ctx = ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused && self.focused == FocusPane::Editor,
            scrollbar_host: if matches!(ctx.scrollbar_host, ScrollbarHost::Window) {
                ScrollbarHost::Window
            } else {
                ctx.scrollbar_host.for_child()
            },
            tab_mode: ctx.tab_mode.for_child(),
        };
        self.tab_window.draw(frame, layout.main, child_ctx);

        // Divider between sidebar and editor (also serves as the sidebar scrollbar "mount").
        if self.sidebar_visible.get() && layout.divider.width > 0 && layout.divider.height > 0 {
            let style = ctx.theme.widget.dim;
            let border_set = ctx.theme.border_set(false);
            let symbol = border_set.vertical_left;

            let buf = frame.buffer_mut();
            for y in layout.divider.y..layout.divider.y.saturating_add(layout.divider.height) {
                for x in layout.divider.x..layout.divider.x.saturating_add(layout.divider.width) {
                    buf[(x, y)].set_symbol(symbol).set_style(style);
                }
            }
        }

        self.draw_split_scrollbars(frame, area, ctx);
    }
}

#[cfg(any())]
impl ::atto_ui::composable::Layout for EditorWindowView {}

#[cfg(any())]
impl ::atto_ui::composable::Scrollable for EditorWindowView {
    fn is_scrollable(&self) -> bool {
        if !self.sidebar_visible.get() {
            return self.tab_window.is_scrollable();
        }

        // When the sidebar is visible, the "right pane" owns the window-border scrollbars.
        match self.sidebar_side.get() {
            SidebarSide::Left => self.tab_window.is_scrollable(),
            SidebarSide::Right => self.file_tree.is_scrollable(),
        }
    }

    fn content_size(&self) -> (u16, u16) {
        if !self.sidebar_visible.get() {
            return self.tab_window.content_size();
        }
        match self.sidebar_side.get() {
            SidebarSide::Left => self.tab_window.content_size(),
            SidebarSide::Right => self.file_tree.content_size(),
        }
    }

    fn viewport_size(&self) -> (u16, u16) {
        if !self.sidebar_visible.get() {
            return self.tab_window.viewport_size();
        }
        match self.sidebar_side.get() {
            SidebarSide::Left => self.tab_window.viewport_size(),
            SidebarSide::Right => self.file_tree.viewport_size(),
        }
    }

    fn scroll_offset(&self) -> (u16, u16) {
        if !self.sidebar_visible.get() {
            return self.tab_window.scroll_offset();
        }
        match self.sidebar_side.get() {
            SidebarSide::Left => self.tab_window.scroll_offset(),
            SidebarSide::Right => self.file_tree.scroll_offset(),
        }
    }

    fn scroll_config(&self) -> ScrollConfig {
        if !self.sidebar_visible.get() {
            return self.tab_window.scroll_config();
        }
        match self.sidebar_side.get() {
            SidebarSide::Left => self.tab_window.scroll_config(),
            SidebarSide::Right => self.file_tree.scroll_config(),
        }
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        if !self.sidebar_visible.get() {
            self.tab_window.set_scroll_offset(x, y);
            return;
        }

        match self.sidebar_side.get() {
            SidebarSide::Left => self.tab_window.set_scroll_offset(x, y),
            SidebarSide::Right => self.file_tree.set_scroll_offset(x, y),
        }
    }
}

#[cfg(any())]
impl ::atto_ui::composable::FocusNav for EditorWindowView {
    fn is_focusable(&self) -> bool {
        true
    }

    fn focus_first(&mut self) -> bool {
        self.focused = if self.sidebar_visible.get() {
            FocusPane::Sidebar
        } else {
            FocusPane::Editor
        };
        true
    }
}

#[cfg(any())]
impl ::atto_ui::composable::DynamicTree for EditorWindowView {}

#[cfg(any())]
impl ::atto_ui::composable::EventHandling for EditorWindowView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.handle_commands();

        // Keep dirty markers reasonably fresh even if the app runs tick-driven.
        self.update_tab_titles();

        // Global shortcuts (window scoped).
        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) = event
            && matches!(kind, KeyEventKind::Press)
        {
            let is_ctrl = modifiers.contains(KeyModifiers::CONTROL);
            match (code, is_ctrl) {
                (KeyCode::Char('w'), true) | (KeyCode::Char('W'), true) => {
                    self.close_active_tab();
                    return EventResult::consumed();
                }
                (KeyCode::Char('s'), true) | (KeyCode::Char('S'), true) => {
                    let _ = self.save_active();
                    return EventResult::consumed();
                }
                (KeyCode::Char('e'), true) | (KeyCode::Char('E'), true) => {
                    let cur = self.sidebar_visible.get();
                    self.sidebar_visible.set(!cur);
                    if !self.sidebar_visible.get() {
                        self.focused = FocusPane::Editor;
                    }
                    return EventResult::consumed();
                }
                _ => {}
            }
        }

        match event {
            Event::Mouse(m) => {
                if self.sidebar_visible.get()
                    && let Some(area) = self.last_area
                    && let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m)
                    && let Some((pane, next)) =
                        self.handle_split_scrollbar_event(area, local_x, local_y, m.kind)
                {
                    match pane {
                        FocusPane::Sidebar => self.file_tree.set_scroll_offset(next.x, next.y),
                        FocusPane::Editor => self.tab_window.set_scroll_offset(next.x, next.y),
                    }
                    self.focused = pane;
                    return EventResult::consumed();
                }

                if matches!(
                    m.kind,
                    MouseEventKind::Down(crossterm::event::MouseButton::Left)
                ) {
                    if let Some(pane) = self.hit_test_pane(*m) {
                        self.focused = pane;
                    }

                    // Double click on the same file opens it.
                    if self.focused == FocusPane::Sidebar {
                        let now = Instant::now();
                        let sel = self.tree_selection.get();
                        if let (Some(prev), Some(sel)) = (self.last_sidebar_click, sel)
                            && prev.1 == sel
                            && now.duration_since(prev.0) <= Duration::from_millis(450)
                        {
                            let _ = self.maybe_open_selected_from_sidebar(false);
                        }
                        if let Some(sel) = sel {
                            self.last_sidebar_click = Some((now, sel));
                        }
                    }
                }

                let layout = self.last_layout.unwrap_or(EditorWindowLayout {
                    sidebar: Rect::default(),
                    divider: Rect::default(),
                    main: self.last_area.unwrap_or(Rect::default()),
                });

                if self.sidebar_visible.get()
                    && self.focused == FocusPane::Sidebar
                    && layout.sidebar.width > 0
                {
                    let child_ctx = ComponentContext {
                        theme: ctx.theme,
                        window_id: ctx.window_id,
                        is_focused: ctx.is_focused,
                        scrollbar_host: if matches!(ctx.scrollbar_host, ScrollbarHost::Window) {
                            ScrollbarHost::Window
                        } else {
                            ctx.scrollbar_host.for_child()
                        },
                        tab_mode: ctx.tab_mode.for_child(),
                    };
                    return self.file_tree.handle_event(event, child_ctx);
                }

                let child_ctx = ComponentContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: ctx.is_focused,
                    scrollbar_host: if matches!(ctx.scrollbar_host, ScrollbarHost::Window) {
                        ScrollbarHost::Window
                    } else {
                        ctx.scrollbar_host.for_child()
                    },
                    tab_mode: ctx.tab_mode.for_child(),
                };
                self.tab_window.handle_event(event, child_ctx)
            }
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                modifiers,
                kind: KeyEventKind::Press,
                ..
            }) if self.focused == FocusPane::Sidebar => {
                // In the sidebar: Enter opens files. Ctrl+Enter opens in a new window.
                let new_window = modifiers.contains(KeyModifiers::CONTROL);
                if self.maybe_open_selected_from_sidebar(new_window) {
                    return EventResult::consumed();
                }

                let child_ctx = ComponentContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: ctx.is_focused,
                    scrollbar_host: if matches!(ctx.scrollbar_host, ScrollbarHost::Window) {
                        ScrollbarHost::Window
                    } else {
                        ctx.scrollbar_host.for_child()
                    },
                    tab_mode: ctx.tab_mode.for_child(),
                };
                self.file_tree.handle_event(event, child_ctx)
            }
            _ => {
                if self.sidebar_visible.get() && self.focused == FocusPane::Sidebar {
                    let child_ctx = ComponentContext {
                        theme: ctx.theme,
                        window_id: ctx.window_id,
                        is_focused: ctx.is_focused,
                        scrollbar_host: if matches!(ctx.scrollbar_host, ScrollbarHost::Window) {
                            ScrollbarHost::Window
                        } else {
                            ctx.scrollbar_host.for_child()
                        },
                        tab_mode: ctx.tab_mode.for_child(),
                    };
                    return self.file_tree.handle_event(event, child_ctx);
                }

                let child_ctx = ComponentContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: ctx.is_focused,
                    scrollbar_host: if matches!(ctx.scrollbar_host, ScrollbarHost::Window) {
                        ScrollbarHost::Window
                    } else {
                        ctx.scrollbar_host.for_child()
                    },
                    tab_mode: ctx.tab_mode.for_child(),
                };
                self.tab_window.handle_event(event, child_ctx)
            }
        }
    }
}

impl ::atto_ui::composable::Component for EditorWindowView {
    fn titlebar(&mut self, ctx: TitleBarContext<'_>) -> Option<TitleBarContent> {
        self.tab_window.titlebar(ctx)
    }

    fn handle_titlebar_event(&mut self, event: &Event, ctx: TitleBarContext<'_>) -> EventResult {
        self.tab_window.handle_titlebar_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.handle_commands();
        self.update_tab_titles();

        let child_ctx = ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused,
            scrollbar_host: if matches!(ctx.scrollbar_host, ScrollbarHost::Window) {
                ScrollbarHost::Window
            } else {
                ctx.scrollbar_host.for_child()
            },
            tab_mode: ctx.tab_mode.for_child(),
        };
        self.tab_window.draw(frame, area, child_ctx);
    }
}

impl ::atto_ui::composable::Layout for EditorWindowView {
    fn min_width(&self) -> u16 {
        self.tab_window.min_width()
    }

    fn min_height(&self) -> u16 {
        self.tab_window.min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.tab_window.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.tab_window.desired_height()
    }
}

impl ::atto_ui::composable::Scrollable for EditorWindowView {
    fn is_scrollable(&self) -> bool {
        self.tab_window.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.tab_window.content_size()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.tab_window.scroll_offset()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.tab_window.viewport_size()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.tab_window.scroll_config()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.tab_window.set_scroll_offset(x, y);
    }

    fn scroll_to_child(&mut self, child_id: atto_ui::composable::ComponentId) {
        self.tab_window.scroll_to_child(child_id);
    }
}

impl ::atto_ui::composable::FocusNav for EditorWindowView {
    fn is_focusable(&self) -> bool {
        self.tab_window.is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        self.tab_window.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.tab_window.focus_last()
    }
}

impl ::atto_ui::composable::DynamicTree for EditorWindowView {}

impl ::atto_ui::composable::EventHandling for EditorWindowView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let child_ctx = ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused,
            scrollbar_host: if matches!(ctx.scrollbar_host, ScrollbarHost::Window) {
                ScrollbarHost::Window
            } else {
                ctx.scrollbar_host.for_child()
            },
            tab_mode: ctx.tab_mode.for_child(),
        };
        self.tab_window.handle_event(event, child_ctx)
    }
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

struct DocumentTabView {
    commands: EventQueue<TabCommand>,
    editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
    clipboard: Binding<String>,
    text: Binding<String>,
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
    fn new(
        commands: EventQueue<TabCommand>,
        editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
        clipboard: Binding<String>,
        text: Binding<String>,
        language_id: String,
        syntax: atto_ui_editor::EditorSyntaxConfig,
        lsp: atto_ui_editor::EditorLspMode,
    ) -> Self {
        let primary = build_editor_view(
            text.clone(),
            clipboard.clone(),
            editor_theme.clone(),
            language_id.clone(),
            syntax.clone(),
            lsp.clone(),
        );

        Self {
            commands,
            editor_theme,
            clipboard,
            text,
            language_id,
            syntax,
            focused: SplitFocus::Primary,
            split: None,
            primary,
            secondary: None,
            scrollbar_drag: None,
            last_layout: None,
            last_area: None,
        }
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
        let secondary = build_editor_view(
            self.text.clone(),
            self.clipboard.clone(),
            self.editor_theme.clone(),
            self.language_id.clone(),
            self.syntax.clone(),
            atto_ui_editor::EditorLspMode::Disabled,
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

        // Primary pane (always present).
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

        // Secondary pane (split active).
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
            ..ctx
        };
        self.primary.draw(frame, layout.primary, primary_ctx);

        if let Some(r) = layout.secondary
            && let Some(view) = self.secondary.as_mut()
        {
            let secondary_ctx = ComponentContext {
                is_focused: secondary_focused,
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
                && let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m)
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

        // Route the event.
        match self.focused {
            SplitFocus::Primary => {
                let child_ctx = ComponentContext {
                    is_focused: primary_focused,
                    ..ctx
                };
                self.primary.handle_event(event, child_ctx)
            }
            SplitFocus::Secondary => {
                if let Some(view) = self.secondary.as_mut() {
                    let child_ctx = ComponentContext {
                        is_focused: secondary_focused,
                        ..ctx
                    };
                    view.handle_event(event, child_ctx)
                } else {
                    let child_ctx = ComponentContext {
                        is_focused: primary_focused,
                        ..ctx
                    };
                    self.primary.handle_event(event, child_ctx)
                }
            }
        }
    }
}

fn build_editor_view(
    text: Binding<String>,
    clipboard: Binding<String>,
    theme: Binding<atto_ui_editor::EditorThemeSet>,
    language_id: String,
    syntax: atto_ui_editor::EditorSyntaxConfig,
    lsp: atto_ui_editor::EditorLspMode,
) -> atto_ui_editor::EditorView {
    let mut cfg = atto_ui_editor::EditorConfig::new(text);
    cfg.clipboard = clipboard;
    cfg.language_id.set(language_id);
    cfg.syntax.set(syntax);
    cfg.lsp.set(lsp);

    let (view, _handle) = atto_ui_editor::EditorView::new(cfg, theme);
    view
}
