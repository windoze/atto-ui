use std::collections::{BTreeSet, HashMap};
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use atto_ui::composable::{
    ComponentContext, EventResult, MouseCoordinateSpace, ScrollConfig, ScrollbarHost,
};
use atto_ui::reactive::{Binding, EventQueue};
use atto_ui_file_tree::{
    FileTree, FileTreeGlyphs, FileTreeInlineEditCommit, FileTreeInlineEditKind, FileTreeNode,
    FileTreeNodeId, FileTreeNodeKind,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};

use crate::actions::{AppAction, OpenTarget};
use crate::workspace::{WorkspaceTreeOptions, build_workspace_tree};

#[derive(Clone, Debug)]
pub enum ExplorerWindowCommand {
    SetWorkspaceRoots(Vec<PathBuf>),
    AddWorkspaceRoot(PathBuf),
    Refresh,
}

#[derive(Clone)]
pub struct ExplorerWindowHandle {
    pub commands: EventQueue<ExplorerWindowCommand>,
}

pub struct ExplorerWindowView {
    actions: EventQueue<AppAction>,
    commands: EventQueue<ExplorerWindowCommand>,

    workspace_roots: Vec<PathBuf>,

    tree_nodes: Binding<Vec<FileTreeNode>>,
    tree_selection: Binding<Option<FileTreeNodeId>>,
    tree_selections: Binding<BTreeSet<FileTreeNodeId>>,
    tree_id_to_path: HashMap<FileTreeNodeId, PathBuf>,
    tree_id_to_kind: HashMap<FileTreeNodeId, FileTreeNodeKind>,
    tree_id_to_parent: HashMap<FileTreeNodeId, Option<FileTreeNodeId>>,
    tree_path_to_id: HashMap<PathBuf, FileTreeNodeId>,
    file_tree: FileTree,

    last_click: Option<(Instant, FileTreeNodeId)>,
    last_area: Option<Rect>,
    context_menu: Option<ExplorerContextMenu>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExplorerContextAction {
    NewFile,
    NewFolder,
    Rename,
    Delete,
    Cut,
    Copy,
    Paste,
    CopyPath,
    Reveal,
}

impl ExplorerContextAction {
    const ALL: [Self; 9] = [
        Self::NewFile,
        Self::NewFolder,
        Self::Rename,
        Self::Delete,
        Self::Cut,
        Self::Copy,
        Self::Paste,
        Self::CopyPath,
        Self::Reveal,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::NewFile => "New File",
            Self::NewFolder => "New Folder",
            Self::Rename => "Rename",
            Self::Delete => "Delete",
            Self::Cut => "Cut",
            Self::Copy => "Copy",
            Self::Paste => "Paste",
            Self::CopyPath => "Copy Path",
            Self::Reveal => "Reveal",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ExplorerContextMenu {
    target: Option<FileTreeNodeId>,
    selected: usize,
    x: u16,
    y: u16,
}

impl ExplorerWindowView {
    pub fn new(
        actions: EventQueue<AppAction>,
        commands: EventQueue<ExplorerWindowCommand>,
        workspace_roots: Vec<PathBuf>,
    ) -> Self {
        let tree_nodes: Binding<Vec<FileTreeNode>> = Binding::new(Vec::new());
        let tree_selection: Binding<Option<FileTreeNodeId>> = Binding::new(None);
        let tree_selections: Binding<BTreeSet<FileTreeNodeId>> = Binding::new(BTreeSet::new());

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

        let file_tree = FileTree::new_with_selections(
            "Workspace",
            tree_nodes.clone(),
            tree_selection.clone(),
            tree_selections.clone(),
        )
        .glyphs(glyphs)
        .defer_inline_commits(true)
        .with_min_width(12)
        .with_min_height(6);

        let mut view = Self {
            actions,
            commands,
            workspace_roots,
            tree_nodes,
            tree_selection,
            tree_selections,
            tree_id_to_path: HashMap::new(),
            tree_id_to_kind: HashMap::new(),
            tree_id_to_parent: HashMap::new(),
            tree_path_to_id: HashMap::new(),
            file_tree,
            last_click: None,
            last_area: None,
            context_menu: None,
        };

        view.refresh_workspace_tree();
        view
    }

    pub fn handle(commands: EventQueue<ExplorerWindowCommand>) -> ExplorerWindowHandle {
        ExplorerWindowHandle { commands }
    }

    pub fn selected_node_ids(&self) -> BTreeSet<FileTreeNodeId> {
        self.tree_selections.get()
    }

    fn refresh_workspace_tree(&mut self) {
        let tree = build_workspace_tree(&self.workspace_roots, WorkspaceTreeOptions::default());
        self.tree_id_to_parent = parent_map_for_roots(&tree.roots);
        self.tree_path_to_id = tree
            .id_to_path
            .iter()
            .map(|(id, path)| (Self::canonicalize_best_effort(path), *id))
            .collect();
        self.tree_id_to_path = tree.id_to_path;
        self.tree_id_to_kind = tree.id_to_kind;
        self.tree_selections
            .update(|ids| ids.retain(|id| self.tree_id_to_path.contains_key(id)));
        if let Some(id) = self.tree_selection.get()
            && !self.tree_id_to_path.contains_key(&id)
        {
            self.tree_selection.set(None);
        }
        self.tree_nodes.set(tree.roots);
    }

    fn canonicalize_best_effort(path: &Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn set_workspace_roots(&mut self, roots: Vec<PathBuf>) {
        let mut next = Vec::<PathBuf>::new();
        for root in roots {
            let root = Self::canonicalize_best_effort(&root);
            if next.iter().any(|p| p == &root) {
                continue;
            }
            next.push(root);
        }
        self.workspace_roots = next;
        self.refresh_workspace_tree();
    }

    fn add_workspace_root(&mut self, root: PathBuf) {
        let root = Self::canonicalize_best_effort(&root);
        if self.workspace_roots.iter().any(|p| p == &root) {
            return;
        }
        self.workspace_roots.push(root);
        self.refresh_workspace_tree();
    }

    fn select_path(&mut self, path: &Path) -> bool {
        let path = Self::canonicalize_best_effort(path);
        let Some(id) = self.tree_path_to_id.get(&path).copied() else {
            return false;
        };
        self.tree_selection.set(Some(id));
        self.tree_selections.set(BTreeSet::from([id]));
        true
    }

    fn show_status(&self, message: impl Into<String>) {
        self.actions
            .push(AppAction::ShowStatusMessage(message.into()));
    }

    fn context_action_target(&self, target: Option<FileTreeNodeId>) -> Option<FileTreeNodeId> {
        target.or_else(|| self.tree_selection.get())
    }

    fn begin_context_new(
        &mut self,
        target: Option<FileTreeNodeId>,
        kind: FileTreeInlineEditKind,
    ) -> EventResult {
        let Some((parent_id, _parent_path)) = self.parent_for_new(target) else {
            self.show_status("Explorer: no workspace folder for new item");
            return EventResult::consumed();
        };
        match kind {
            FileTreeInlineEditKind::NewFile => {
                self.file_tree.begin_inline_new_file(Some(parent_id))
            }
            FileTreeInlineEditKind::NewFolder => {
                self.file_tree.begin_inline_new_folder(Some(parent_id));
            }
            FileTreeInlineEditKind::Rename => {
                self.show_status("Explorer: rename is not a new-item action");
                return EventResult::consumed();
            }
        }
        EventResult::changed()
    }

    fn begin_context_rename(&mut self, target: Option<FileTreeNodeId>) -> EventResult {
        let Some(id) = self.context_action_target(target) else {
            self.show_status("Explorer: no file tree item selected for rename");
            return EventResult::consumed();
        };
        let Some(path) = self.tree_id_to_path.get(&id) else {
            self.show_status("Explorer: selected item is no longer available");
            return EventResult::consumed();
        };
        let Some(name) = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
        else {
            self.show_status("Explorer: selected item has no file name");
            return EventResult::consumed();
        };
        let parent_id = self.tree_id_to_parent.get(&id).copied().flatten();
        self.tree_selection.set(Some(id));
        self.tree_selections.set(BTreeSet::from([id]));
        self.file_tree.begin_inline_rename(id, parent_id, name);
        EventResult::changed()
    }

    fn parent_for_new(&self, target: Option<FileTreeNodeId>) -> Option<(FileTreeNodeId, PathBuf)> {
        let target = self.context_action_target(target);
        if let Some(id) = target {
            let path = self.tree_id_to_path.get(&id)?;
            let kind = self.tree_id_to_kind.get(&id).copied()?;
            if kind == FileTreeNodeKind::Directory {
                return Some((id, path.clone()));
            }
            let parent_id = self.tree_id_to_parent.get(&id).copied().flatten()?;
            let parent_path = path.parent()?.to_path_buf();
            return Some((parent_id, parent_path));
        }

        let root = self.workspace_roots.first()?;
        let root = Self::canonicalize_best_effort(root);
        let id = self.tree_path_to_id.get(&root).copied()?;
        Some((id, root))
    }

    fn execute_context_action(
        &mut self,
        action: ExplorerContextAction,
        target: Option<FileTreeNodeId>,
    ) -> EventResult {
        self.context_menu = None;
        match action {
            ExplorerContextAction::NewFile => {
                self.begin_context_new(target, FileTreeInlineEditKind::NewFile)
            }
            ExplorerContextAction::NewFolder => {
                self.begin_context_new(target, FileTreeInlineEditKind::NewFolder)
            }
            ExplorerContextAction::Rename => self.begin_context_rename(target),
            ExplorerContextAction::CopyPath => {
                if let Some(id) = self.context_action_target(target)
                    && let Some(path) = self.tree_id_to_path.get(&id)
                {
                    self.show_status(format!("Explorer path: {}", path.display()));
                } else {
                    self.show_status("Explorer: no path selected");
                }
                EventResult::consumed()
            }
            ExplorerContextAction::Delete => {
                self.show_status("Explorer: delete requires confirmation and is not enabled yet");
                EventResult::consumed()
            }
            ExplorerContextAction::Cut
            | ExplorerContextAction::Copy
            | ExplorerContextAction::Paste => {
                self.show_status("Explorer: clipboard actions are scheduled for T24");
                EventResult::consumed()
            }
            ExplorerContextAction::Reveal => {
                self.show_status("Explorer: reveal is not available in this terminal session");
                EventResult::consumed()
            }
        }
    }

    fn process_inline_commit(&mut self) -> EventResult {
        let Some(commit) = self.file_tree.take_inline_edit_commit() else {
            return EventResult::ignored();
        };
        match self.apply_inline_commit(commit) {
            Ok(path_to_select) => {
                self.file_tree.finish_inline_edit();
                self.refresh_workspace_tree();
                self.select_path(&path_to_select);
                EventResult::changed()
            }
            Err(message) => {
                self.show_status(format!("Explorer: {message}"));
                EventResult::consumed()
            }
        }
    }

    fn apply_inline_commit(&mut self, commit: FileTreeInlineEditCommit) -> Result<PathBuf, String> {
        let name = validate_file_name(&commit.text)?;
        match commit.kind {
            FileTreeInlineEditKind::Rename => {
                let id = commit
                    .node_id
                    .ok_or_else(|| "rename target is missing".to_string())?;
                let old_path = self
                    .tree_id_to_path
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| "rename target is no longer available".to_string())?;
                let new_path = old_path.with_file_name(name);
                if Self::canonicalize_best_effort(&old_path)
                    == Self::canonicalize_best_effort(&new_path)
                {
                    return Ok(old_path);
                }
                ensure_target_available(&new_path)?;
                fs::rename(&old_path, &new_path).map_err(|err| format!("rename failed: {err}"))?;
                Ok(new_path)
            }
            FileTreeInlineEditKind::NewFile | FileTreeInlineEditKind::NewFolder => {
                let parent_id = commit
                    .parent_id
                    .ok_or_else(|| "new item parent is missing".to_string())?;
                let parent = self
                    .tree_id_to_path
                    .get(&parent_id)
                    .cloned()
                    .ok_or_else(|| "new item parent is no longer available".to_string())?;
                let target = parent.join(name);
                ensure_target_available(&target)?;
                match commit.kind {
                    FileTreeInlineEditKind::NewFile => {
                        OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&target)
                            .map_err(|err| format!("create file failed: {err}"))?;
                    }
                    FileTreeInlineEditKind::NewFolder => {
                        fs::create_dir(&target)
                            .map_err(|err| format!("create folder failed: {err}"))?;
                    }
                    FileTreeInlineEditKind::Rename => unreachable!(),
                }
                Ok(target)
            }
        }
    }

    fn open_context_menu(
        &mut self,
        target: Option<FileTreeNodeId>,
        mouse: MouseEvent,
        ctx: ComponentContext<'_>,
    ) {
        let (x, y) = match ctx.mouse_coordinate_space {
            MouseCoordinateSpace::Absolute => (mouse.column, mouse.row),
            MouseCoordinateSpace::Local => {
                let area = self.last_area.unwrap_or_default();
                (
                    area.x.saturating_add(mouse.column),
                    area.y.saturating_add(mouse.row),
                )
            }
        };
        self.context_menu = Some(ExplorerContextMenu {
            target,
            selected: 0,
            x,
            y,
        });
    }

    fn context_menu_rect(&self, menu: ExplorerContextMenu) -> Rect {
        let area = self.last_area.unwrap_or_default();
        let label_width = ExplorerContextAction::ALL
            .iter()
            .map(|action| action.label().len())
            .max()
            .unwrap_or(0) as u16;
        let width = label_width.saturating_add(4).max(14);
        let height = ExplorerContextAction::ALL.len() as u16 + 2;
        let max_x = area.x.saturating_add(area.width.saturating_sub(width));
        let max_y = area.y.saturating_add(area.height.saturating_sub(height));
        Rect {
            x: menu.x.clamp(area.x, max_x),
            y: menu.y.clamp(area.y, max_y),
            width: width.min(area.width),
            height: height.min(area.height),
        }
    }

    fn handle_context_menu_event(&mut self, event: &Event) -> Option<EventResult> {
        let menu = self.context_menu?;
        match event {
            Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Press,
                ..
            }) => match code {
                KeyCode::Esc => {
                    self.context_menu = None;
                    Some(EventResult::consumed())
                }
                KeyCode::Up => {
                    if let Some(menu) = &mut self.context_menu {
                        menu.selected = menu
                            .selected
                            .checked_sub(1)
                            .unwrap_or(ExplorerContextAction::ALL.len() - 1);
                    }
                    Some(EventResult::changed())
                }
                KeyCode::Down => {
                    if let Some(menu) = &mut self.context_menu {
                        menu.selected = (menu.selected + 1) % ExplorerContextAction::ALL.len();
                    }
                    Some(EventResult::changed())
                }
                KeyCode::Enter => {
                    let action = ExplorerContextAction::ALL[menu.selected];
                    Some(self.execute_context_action(action, menu.target))
                }
                _ => Some(EventResult::consumed()),
            },
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                ..
            }) => {
                let rect = self.context_menu_rect(menu);
                let inside = *column >= rect.x
                    && *column < rect.x.saturating_add(rect.width)
                    && *row >= rect.y
                    && *row < rect.y.saturating_add(rect.height);
                if !inside {
                    self.context_menu = None;
                    return Some(EventResult::consumed());
                }
                let item_row = row.saturating_sub(rect.y).saturating_sub(1) as usize;
                if let Some(action) = ExplorerContextAction::ALL.get(item_row).copied() {
                    return Some(self.execute_context_action(action, menu.target));
                }
                Some(EventResult::consumed())
            }
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Right),
                ..
            }) => {
                self.context_menu = None;
                Some(EventResult::consumed())
            }
            Event::Key(KeyEvent { .. }) | Event::Mouse(_) => Some(EventResult::consumed()),
            _ => Some(EventResult::ignored()),
        }
    }

    fn draw_context_menu(
        &mut self,
        frame: &mut Frame<'_>,
        ctx: ComponentContext<'_>,
        menu: ExplorerContextMenu,
    ) {
        let rect = self.context_menu_rect(menu);
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        frame.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(ctx.theme.border_set(false))
            .title("Explorer")
            .style(ctx.theme.menu_item);
        let items = ExplorerContextAction::ALL
            .iter()
            .enumerate()
            .map(|(idx, action)| {
                let prefix = if idx == menu.selected { "> " } else { "  " };
                let item = ListItem::new(Line::raw(format!("{prefix}{}", action.label())));
                if idx == menu.selected {
                    item.style(ctx.theme.menu_item_selected)
                } else {
                    item.style(ctx.theme.menu_item)
                }
            })
            .collect::<Vec<_>>();
        frame.render_widget(List::new(items).block(block), rect);
    }

    fn maybe_open_selected(&mut self, target: OpenTarget) -> bool {
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

        self.actions.push(AppAction::OpenPath { path, target });
        true
    }

    fn handle_commands(&mut self) {
        for cmd in self.commands.drain() {
            match cmd {
                ExplorerWindowCommand::SetWorkspaceRoots(roots) => self.set_workspace_roots(roots),
                ExplorerWindowCommand::AddWorkspaceRoot(root) => self.add_workspace_root(root),
                ExplorerWindowCommand::Refresh => self.refresh_workspace_tree(),
            }
        }
    }
}

fn parent_map_for_roots(roots: &[FileTreeNode]) -> HashMap<FileTreeNodeId, Option<FileTreeNodeId>> {
    let mut out = HashMap::new();
    collect_parent_map(roots, None, &mut out);
    out
}

fn collect_parent_map(
    nodes: &[FileTreeNode],
    parent: Option<FileTreeNodeId>,
    out: &mut HashMap<FileTreeNodeId, Option<FileTreeNodeId>>,
) {
    for node in nodes {
        out.insert(node.id, parent);
        collect_parent_map(&node.children, Some(node.id), out);
    }
}

fn validate_file_name(raw: &str) -> Result<&str, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("file name cannot be empty".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Err("file name cannot contain path separators".to_string());
    }
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(value)), None) if value == OsStr::new(name) => Ok(name),
        _ => Err("file name must be a single path segment".to_string()),
    }
}

fn ensure_target_available(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(format!("target already exists: {}", path.display())),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "target is not accessible: {}: {err}",
            path.display()
        )),
    }
}

impl ::atto_ui::composable::Component for ExplorerWindowView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.handle_commands();
        self.last_area = Some(area);

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
            mouse_coordinate_space: ctx.mouse_coordinate_space,
            drag: None,
        };
        self.file_tree.draw(frame, area, child_ctx);
        if let Some(menu) = self.context_menu {
            self.draw_context_menu(frame, ctx, menu);
        }
    }
}

impl ::atto_ui::composable::DragAndDrop for ExplorerWindowView {}

impl ::atto_ui::composable::Layout for ExplorerWindowView {}

impl ::atto_ui::composable::Scrollable for ExplorerWindowView {
    fn is_scrollable(&self) -> bool {
        self.file_tree.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.file_tree.content_size()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.file_tree.viewport_size()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.file_tree.scroll_offset()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.file_tree.scroll_config()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.file_tree.set_scroll_offset(x, y);
    }
}

impl ::atto_ui::composable::FocusNav for ExplorerWindowView {
    fn is_focusable(&self) -> bool {
        true
    }
}

impl ::atto_ui::composable::DynamicTree for ExplorerWindowView {}

impl ::atto_ui::composable::EventHandling for ExplorerWindowView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.handle_commands();

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
                (KeyCode::Char('e'), true) | (KeyCode::Char('E'), true) => {
                    self.actions.push(AppAction::ToggleExplorer);
                    return EventResult::consumed();
                }
                _ => {}
            }
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
            mouse_coordinate_space: ctx.mouse_coordinate_space,
            drag: None,
        };

        if let Some(result) = self.handle_context_menu_event(event) {
            return result;
        }

        match event {
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                modifiers,
                kind: KeyEventKind::Press,
                ..
            }) if self.file_tree.inline_edit().is_none() => {
                let target = if modifiers.contains(KeyModifiers::CONTROL) {
                    OpenTarget::NewWindow
                } else {
                    OpenTarget::NewTab
                };
                if self.maybe_open_selected(target) {
                    return EventResult::consumed();
                }
            }
            Event::Mouse(
                mouse @ MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Right),
                    ..
                },
            ) => {
                let target = self.file_tree.node_id_at_mouse_event(*mouse, child_ctx);
                self.open_context_menu(target, *mouse, child_ctx);
                return EventResult::consumed();
            }
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                ..
            }) => {
                // Let the underlying file tree update selection first, then resolve double-click
                // based on the selected node id.
                let res = self.file_tree.handle_event(event, child_ctx);

                let now = Instant::now();
                let sel = self.tree_selection.get();
                let is_double_click = self.last_click.is_some_and(|(prev_at, prev_id)| {
                    Some(prev_id) == sel
                        && now.duration_since(prev_at) <= Duration::from_millis(450)
                });

                let opened = if is_double_click {
                    self.maybe_open_selected(OpenTarget::NewTab)
                } else {
                    false
                };

                self.last_click = sel.map(|id| (now, id));

                return if opened { EventResult::consumed() } else { res };
            }
            _ => {}
        }

        let res = self.file_tree.handle_event(event, child_ctx);
        let commit_res = self.process_inline_commit();
        if commit_res.is_consumed() {
            commit_res
        } else {
            res
        }
    }
}
