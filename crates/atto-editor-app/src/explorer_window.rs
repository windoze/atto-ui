use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use atto_ui::composable::{ComponentContext, EventResult, ScrollConfig, ScrollbarHost};
use atto_ui::reactive::{Binding, EventQueue};
use atto_ui_file_tree::{FileTree, FileTreeGlyphs, FileTreeNode, FileTreeNodeId, FileTreeNodeKind};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::Rect;

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
    tree_id_to_path: HashMap<FileTreeNodeId, PathBuf>,
    tree_id_to_kind: HashMap<FileTreeNodeId, FileTreeNodeKind>,
    file_tree: FileTree,

    last_click: Option<(Instant, FileTreeNodeId)>,
}

impl ExplorerWindowView {
    pub fn new(
        actions: EventQueue<AppAction>,
        commands: EventQueue<ExplorerWindowCommand>,
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
            workspace_roots,
            tree_nodes,
            tree_selection,
            tree_id_to_path: HashMap::new(),
            tree_id_to_kind: HashMap::new(),
            file_tree,
            last_click: None,
        };

        view.refresh_workspace_tree();
        view
    }

    pub fn handle(commands: EventQueue<ExplorerWindowCommand>) -> ExplorerWindowHandle {
        ExplorerWindowHandle { commands }
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

impl ::atto_ui::composable::Component for ExplorerWindowView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.handle_commands();

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

        match event {
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                modifiers,
                kind: KeyEventKind::Press,
                ..
            }) => {
                let target = if modifiers.contains(KeyModifiers::CONTROL) {
                    OpenTarget::NewWindow
                } else {
                    OpenTarget::NewTab
                };
                if self.maybe_open_selected(target) {
                    return EventResult::consumed();
                }
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

        self.file_tree.handle_event(event, child_ctx)
    }
}
