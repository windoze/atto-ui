use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use atto_ui::composable::{
    Component, ComponentContext, EventResult, TitleBarContent, TitleBarContext,
};
use atto_ui::reactive::{Binding, DirtyObserver, EventQueue};
use atto_ui_file_tree::{FileTree, FileTreeGlyphs, FileTreeNode, FileTreeNodeId, FileTreeNodeKind};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::actions::AppAction;
use crate::language::{guess_language_id, lsp_mode_for_file, syntax_config_for_file};
use crate::workspace::{WorkspaceTreeOptions, build_workspace_tree};

#[derive(Clone, Debug)]
pub enum EditorWindowCommand {
    OpenFile(PathBuf),
    AddWorkspaceRoot(PathBuf),

    SaveActive,
    SaveAs(PathBuf),
    CloseActiveTab,

    SplitVertical,
    SplitHorizontal,
    CloseSplit,

    ToggleSidebar,
    SidebarLeft,
    SidebarRight,
}

#[derive(Clone)]
pub struct EditorWindowHandle {
    pub commands: EventQueue<EditorWindowCommand>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SidebarSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusPane {
    Sidebar,
    Editor,
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
    actions: EventQueue<AppAction>,
    commands: EventQueue<EditorWindowCommand>,

    editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
    clipboard: Binding<String>,

    sidebar_visible: Binding<bool>,
    sidebar_side: Binding<SidebarSide>,
    sidebar_width: Binding<u16>,
    focused: FocusPane,

    workspace_roots: Vec<PathBuf>,
    tree_nodes: Binding<Vec<FileTreeNode>>,
    tree_selection: Binding<Option<FileTreeNodeId>>,
    tree_id_to_path: HashMap<FileTreeNodeId, PathBuf>,
    tree_id_to_kind: HashMap<FileTreeNodeId, FileTreeNodeKind>,
    file_tree: FileTree,

    tab_window: atto_ui::composable::TabWindow,
    tabs: Vec<TabState>,

    last_area: Option<Rect>,
    last_layout: Option<EditorWindowLayout>,
    last_sidebar_click: Option<(Instant, FileTreeNodeId)>,
}

#[derive(Clone, Copy, Debug)]
struct EditorWindowLayout {
    sidebar: Rect,
    main: Rect,
}

impl EditorWindowView {
    pub fn new(
        actions: EventQueue<AppAction>,
        commands: EventQueue<EditorWindowCommand>,
        editor_theme: Binding<atto_ui_editor::EditorThemeSet>,
        clipboard: Binding<String>,
        workspace_roots: Vec<PathBuf>,
    ) -> Self {
        let tree_nodes: Binding<Vec<FileTreeNode>> = Binding::new(Vec::new());
        let tree_selection: Binding<Option<FileTreeNodeId>> = Binding::new(None);

        let glyphs = FileTreeGlyphs::default()
            .with_extension("rs", "rs")
            .with_extension("toml", "tm")
            .with_extension("json", "js")
            .with_extension("md", "md")
            .with_extension("yml", "yml")
            .with_extension("yaml", "yml")
            .with_extension("py", "py")
            .with_extension("js", "js")
            .with_extension("ts", "ts");

        let file_tree = FileTree::new("Workspace", tree_nodes.clone(), tree_selection.clone())
            .glyphs(glyphs)
            .with_min_width(12)
            .with_min_height(6);

        let mut view = Self {
            actions,
            commands,
            editor_theme,
            clipboard,
            sidebar_visible: true.into(),
            sidebar_side: SidebarSide::Left.into(),
            sidebar_width: 28u16.into(),
            focused: FocusPane::Editor,
            workspace_roots,
            tree_nodes,
            tree_selection,
            tree_id_to_path: HashMap::new(),
            tree_id_to_kind: HashMap::new(),
            file_tree,
            tab_window: atto_ui::composable::TabWindow::new(),
            tabs: Vec::new(),
            last_area: None,
            last_layout: None,
            last_sidebar_click: None,
        };

        view.refresh_workspace_tree();
        view
    }

    pub fn handle(commands: EventQueue<EditorWindowCommand>) -> EditorWindowHandle {
        EditorWindowHandle { commands }
    }

    fn refresh_workspace_tree(&mut self) {
        let tree = build_workspace_tree(&self.workspace_roots, WorkspaceTreeOptions::default());
        self.tree_id_to_path = tree.id_to_path;
        self.tree_id_to_kind = tree.id_to_kind;
        self.tree_nodes.set(tree.roots);
    }

    fn canonicalize_best_effort(path: &Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn add_workspace_root(&mut self, root: PathBuf) {
        let root = Self::canonicalize_best_effort(&root);
        if self.workspace_roots.iter().any(|p| p == &root) {
            return;
        }
        self.workspace_roots.push(root);
        self.refresh_workspace_tree();
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
            self.focused = FocusPane::Editor;
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

        if let Some(parent) = path.parent()
            && parent.is_dir()
        {
            self.add_workspace_root(parent.to_path_buf());
        }

        self.focused = FocusPane::Editor;
    }

    fn close_active_tab(&mut self) {
        let Some(active) = self.tab_window.active_tab() else {
            return;
        };
        if self.tab_window.remove_tab(active).is_some() {
            if active < self.tabs.len() {
                self.tabs.remove(active);
            }
            if self.tab_window.active_tab().is_none() {
                self.focused = FocusPane::Sidebar;
            }
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

    fn sidebar_layout(&self, area: Rect) -> EditorWindowLayout {
        if !self.sidebar_visible.get() || area.width == 0 || area.height == 0 {
            return EditorWindowLayout {
                sidebar: Rect::default(),
                main: area,
            };
        }

        let sidebar_w = self
            .sidebar_width
            .get()
            .min(area.width.saturating_sub(8))
            .max(12);

        match self.sidebar_side.get() {
            SidebarSide::Left => {
                let sidebar = Rect {
                    x: area.x,
                    y: area.y,
                    width: sidebar_w,
                    height: area.height,
                };
                let main = Rect {
                    x: area.x + sidebar_w,
                    y: area.y,
                    width: area.width.saturating_sub(sidebar_w),
                    height: area.height,
                };
                EditorWindowLayout { sidebar, main }
            }
            SidebarSide::Right => {
                let sidebar = Rect {
                    x: area.x + area.width.saturating_sub(sidebar_w),
                    y: area.y,
                    width: sidebar_w,
                    height: area.height,
                };
                let main = Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width.saturating_sub(sidebar_w),
                    height: area.height,
                };
                EditorWindowLayout { sidebar, main }
            }
        }
    }

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
                EditorWindowCommand::AddWorkspaceRoot(path) => self.add_workspace_root(path),
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
                EditorWindowCommand::ToggleSidebar => {
                    let cur = self.sidebar_visible.get();
                    self.sidebar_visible.set(!cur);
                    if !self.sidebar_visible.get() {
                        self.focused = FocusPane::Editor;
                    }
                }
                EditorWindowCommand::SidebarLeft => self.sidebar_side.set(SidebarSide::Left),
                EditorWindowCommand::SidebarRight => self.sidebar_side.set(SidebarSide::Right),
            }
        }
    }
}

impl Component for EditorWindowView {
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

    fn titlebar(&mut self, ctx: TitleBarContext<'_>) -> Option<TitleBarContent> {
        self.tab_window.titlebar(ctx)
    }

    fn handle_titlebar_event(&mut self, event: &Event, ctx: TitleBarContext<'_>) -> EventResult {
        self.tab_window.handle_titlebar_event(event, ctx)
    }

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
                        scrollbar_host: ctx.scrollbar_host.for_child(),
                        tab_mode: ctx.tab_mode.for_child(),
                    };
                    return self.file_tree.handle_event(event, child_ctx);
                }

                let child_ctx = ComponentContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: ctx.is_focused,
                    scrollbar_host: ctx.scrollbar_host.for_child(),
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
                    scrollbar_host: ctx.scrollbar_host.for_child(),
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
                        scrollbar_host: ctx.scrollbar_host.for_child(),
                        tab_mode: ctx.tab_mode.for_child(),
                    };
                    return self.file_tree.handle_event(event, child_ctx);
                }

                let child_ctx = ComponentContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: ctx.is_focused,
                    scrollbar_host: ctx.scrollbar_host.for_child(),
                    tab_mode: ctx.tab_mode.for_child(),
                };
                self.tab_window.handle_event(event, child_ctx)
            }
        }
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
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
            };
            self.file_tree.draw(frame, layout.sidebar, child_ctx);
        }

        let child_ctx = ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused && self.focused == FocusPane::Editor,
            scrollbar_host: ctx.scrollbar_host.for_child(),
            tab_mode: ctx.tab_mode.for_child(),
        };
        self.tab_window.draw(frame, layout.main, child_ctx);
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

    last_layout: Option<TabLayout>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SplitFocus {
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug)]
struct TabLayout {
    primary: Rect,
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
            last_layout: None,
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
                secondary: None,
            };
        };
        if self.secondary.is_none() || area.width == 0 || area.height == 0 {
            return TabLayout {
                primary: area,
                secondary: None,
            };
        }

        match orientation {
            atto_ui::composable::SplitterOrientation::Vertical => {
                let w1 = area.width / 2;
                let w2 = area.width.saturating_sub(w1);
                let primary = Rect {
                    x: area.x,
                    y: area.y,
                    width: w1,
                    height: area.height,
                };
                let secondary = Rect {
                    x: area.x + w1,
                    y: area.y,
                    width: w2,
                    height: area.height,
                };
                TabLayout {
                    primary,
                    secondary: Some(secondary),
                }
            }
            atto_ui::composable::SplitterOrientation::Horizontal => {
                let h1 = area.height / 2;
                let h2 = area.height.saturating_sub(h1);
                let primary = Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: h1,
                };
                let secondary = Rect {
                    x: area.x,
                    y: area.y + h1,
                    width: area.width,
                    height: h2,
                };
                TabLayout {
                    primary,
                    secondary: Some(secondary),
                }
            }
        }
    }

    fn hit_test(&self, m: MouseEvent) -> Option<SplitFocus> {
        let Some(layout) = self.last_layout else {
            return None;
        };
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
}

impl Component for DocumentTabView {
    fn is_focusable(&self) -> bool {
        true
    }

    fn focus_first(&mut self) -> bool {
        self.focused = SplitFocus::Primary;
        true
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.handle_commands();

        match event {
            Event::Mouse(m) => {
                if matches!(
                    m.kind,
                    MouseEventKind::Down(crossterm::event::MouseButton::Left)
                ) && let Some(focus) = self.hit_test(*m)
                {
                    self.focused = focus;
                }
            }
            _ => {}
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

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.handle_commands();
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
