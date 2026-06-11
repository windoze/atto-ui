#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::Arc;

use atto_ui::composable::{
    Component, ComponentContext, DragOperation, DragPayload, DragPayloadType, DragSource,
    EdgeInsets, EventResult, MouseCoordinateSpace, ScrollConfig, ScrollContainer,
    ScrollContainerHost, ScrollContent, ScrollContentContext, ScrollOffset, Scrollable,
    ScrollbarDrag, ScrollbarHost, Scrollbars, draw_scrollbars, handle_scrollbar_mouse_event,
    should_show_scrollbar,
};
use atto_ui::reactive::Binding;
use atto_ui::runtime::{
    CallbackHandle, component_schema, event_handle, invalid_prop, prop_bool, prop_string, prop_u16,
    prop_u64, register_registry_extension, wrap_with_id,
};
use atto_ui::text::TextBuffer;
use atto_ui::{
    CallbackRegistry, ComponentError, ComponentPropertySchema, ComponentRegistry, ComponentSchema,
    ComponentSpec, ComponentValue, ComponentValueCodec, EventMeta, PropertyMeta, TreeError,
    ValueType,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use parking_lot::RwLock;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const FILE_TREE_NODE_IDS_DRAG_TYPE: DragPayloadType =
    DragPayloadType("atto-ui-file-tree/node-ids");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileTreeNodeId(u64);

impl FileTreeNodeId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for FileTreeNodeId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl atto_ui::ComponentValueCodec for FileTreeNodeId {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::U64(self.value())
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        value
            .as_u64()
            .map(FileTreeNodeId::new)
            .ok_or_else(|| ComponentError::invalid_value(name, "u64"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileTreeNodeKind {
    File,
    Directory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FileTreeGitStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Ignored,
    Clean,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileTreeNode {
    pub id: FileTreeNodeId,
    pub name: String,
    pub kind: FileTreeNodeKind,
    pub children: Vec<FileTreeNode>,
    pub is_expanded: bool,
    pub git_status: Option<FileTreeGitStatus>,
    /// Whether this directory's children have been loaded. When `false`, the node
    /// is rendered as expandable (showing the `+` indicator) even with no children
    /// yet, so callers can lazily load them on demand.
    pub children_loaded: bool,
}

impl FileTreeNode {
    pub fn file(id: impl Into<FileTreeNodeId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: FileTreeNodeKind::File,
            children: Vec::new(),
            is_expanded: false,
            git_status: None,
            children_loaded: true,
        }
    }

    pub fn dir(
        id: impl Into<FileTreeNodeId>,
        name: impl Into<String>,
        children: Vec<FileTreeNode>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: FileTreeNodeKind::Directory,
            children,
            is_expanded: false,
            git_status: None,
            children_loaded: true,
        }
    }

    pub fn with_expanded(mut self, expanded: bool) -> Self {
        self.is_expanded = expanded;
        self
    }

    pub fn with_children_loaded(mut self, loaded: bool) -> Self {
        self.children_loaded = loaded;
        self
    }

    pub fn with_git_status(mut self, status: FileTreeGitStatus) -> Self {
        self.git_status = Some(status);
        self
    }

    pub fn extension(&self) -> Option<&str> {
        if self.kind != FileTreeNodeKind::File {
            return None;
        }
        let (_, ext) = self.name.rsplit_once('.')?;
        (!ext.is_empty()).then_some(ext)
    }

    pub fn is_dir(&self) -> bool {
        matches!(self.kind, FileTreeNodeKind::Directory)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileTreeInlineEditKind {
    Rename,
    NewFile,
    NewFolder,
}

#[derive(Clone, Debug)]
pub struct FileTreeInlineEditState {
    pub node_id: Option<FileTreeNodeId>,
    pub parent_id: Option<FileTreeNodeId>,
    pub text: TextBuffer,
    pub kind: FileTreeInlineEditKind,
    replace_on_input: bool,
}

impl FileTreeInlineEditState {
    fn new(
        node_id: Option<FileTreeNodeId>,
        parent_id: Option<FileTreeNodeId>,
        text: impl Into<String>,
        kind: FileTreeInlineEditKind,
        replace_on_input: bool,
    ) -> Self {
        Self {
            node_id,
            parent_id,
            text: TextBuffer::with_text(text),
            kind,
            replace_on_input,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileTreeInlineEditCommit {
    pub node_id: Option<FileTreeNodeId>,
    pub parent_id: Option<FileTreeNodeId>,
    pub kind: FileTreeInlineEditKind,
    pub text: String,
    pub old_name: Option<String>,
    pub node_kind: Option<FileTreeNodeKind>,
}

pub trait FileTreeFilter: Send + Sync {
    fn include(&self, node: &FileTreeNode) -> bool;
}

impl<F> FileTreeFilter for F
where
    F: Fn(&FileTreeNode) -> bool + Send + Sync,
{
    fn include(&self, node: &FileTreeNode) -> bool {
        (self)(node)
    }
}

/// A file-type icon: the glyph string shown before the entry name plus an
/// optional foreground color. A color of `None` follows the row's normal/
/// highlight style. An empty glyph renders nothing.
///
/// Icons let callers opt into PowerLine / Nerd Font glyphs (with distinct
/// colors) where the terminal supports them. The default mapping is empty so
/// plain terminals never get unsupported characters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileTreeIcon {
    pub glyph: String,
    pub color: Option<Color>,
}

impl FileTreeIcon {
    pub fn new(glyph: impl Into<String>) -> Self {
        Self {
            glyph: glyph.into(),
            color: None,
        }
    }

    pub fn colored(glyph: impl Into<String>, color: Color) -> Self {
        Self {
            glyph: glyph.into(),
            color: Some(color),
        }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.glyph.is_empty()
    }
}

impl From<&str> for FileTreeIcon {
    fn from(glyph: &str) -> Self {
        Self::new(glyph)
    }
}

impl From<String> for FileTreeIcon {
    fn from(glyph: String) -> Self {
        Self::new(glyph)
    }
}

pub trait FileTreeGlyphProvider: Send + Sync {
    fn icon_for(&self, node: &FileTreeNode, is_expanded: bool) -> FileTreeIcon;
}

/// Icons prepended to file-tree entries, keyed by lowercased file extension
/// (plus directory/default fallbacks). By default the mapping is empty:
/// directory state is shown by the ▶/▼ indicator and no per-file type hint is
/// drawn. Add icons via [`FileTreeGlyphs::with_extension`].
#[derive(Clone, Debug, Default)]
pub struct FileTreeGlyphs {
    pub directory_closed: FileTreeIcon,
    pub directory_open: FileTreeIcon,
    pub file: FileTreeIcon,
    pub by_extension: BTreeMap<String, FileTreeIcon>,
}

impl FileTreeGlyphs {
    pub fn with_extension(mut self, ext: impl Into<String>, icon: impl Into<FileTreeIcon>) -> Self {
        self.set_extension(ext, icon);
        self
    }

    pub fn set_extension(&mut self, ext: impl Into<String>, icon: impl Into<FileTreeIcon>) {
        let key = ext.into().to_ascii_lowercase();
        self.by_extension.insert(key, icon.into());
    }
}

impl FileTreeGlyphProvider for FileTreeGlyphs {
    fn icon_for(&self, node: &FileTreeNode, is_expanded: bool) -> FileTreeIcon {
        if node.is_dir() {
            return if is_expanded {
                self.directory_open.clone()
            } else {
                self.directory_closed.clone()
            };
        }
        if let Some(ext) = node.extension()
            && let Some(icon) = self.by_extension.get(&ext.to_ascii_lowercase())
        {
            return icon.clone();
        }
        self.file.clone()
    }
}

pub struct FileTree {
    bindings: Arc<RwLock<FileTreeBindings>>,
    scroll: ScrollContainer,
    scrollbar_drag: Option<ScrollbarDrag>,
    last_area: Option<Rect>,
    min_size: (u16, u16),
}

struct FileTreeBindings {
    title: Binding<String>,
    roots: Binding<Vec<FileTreeNode>>,
    selection: Binding<Option<FileTreeNodeId>>,
    selections: Binding<BTreeSet<FileTreeNodeId>>,
    selection_anchor: Option<FileTreeNodeId>,
    enabled: Binding<bool>,
    border: Binding<bool>,
    height: Binding<u16>,
    filter: Option<Arc<dyn FileTreeFilter>>,
    glyphs: Arc<dyn FileTreeGlyphProvider>,
    on_select: Option<CallbackHandle>,
    on_rename: Option<CallbackHandle>,
    on_delete: Option<CallbackHandle>,
    inline_edit: Option<FileTreeInlineEditState>,
    pending_inline_commit: Option<FileTreeInlineEditCommit>,
    defer_inline_commits: bool,
}

impl FileTree {
    pub fn new(
        title: impl Into<Binding<String>>,
        roots: impl Into<Binding<Vec<FileTreeNode>>>,
        selection: Binding<Option<FileTreeNodeId>>,
    ) -> Self {
        Self::new_with_selections(title, roots, selection, Binding::new(BTreeSet::new()))
    }

    pub fn new_with_selections(
        title: impl Into<Binding<String>>,
        roots: impl Into<Binding<Vec<FileTreeNode>>>,
        selection: Binding<Option<FileTreeNodeId>>,
        selections: Binding<BTreeSet<FileTreeNodeId>>,
    ) -> Self {
        let roots = roots.into();
        if selection.get().is_none() {
            let nodes = roots.get();
            if let Some(first) = nodes.first() {
                selection.set(Some(first.id));
            }
        }
        let selection_anchor = selection.get();
        let mut selected_ids = selections.get();
        if let Some(id) = selection_anchor {
            selected_ids.insert(id);
        }
        selections.set(selected_ids);
        let bindings = Arc::new(RwLock::new(FileTreeBindings {
            title: title.into(),
            roots,
            selection,
            selections,
            selection_anchor,
            enabled: true.into(),
            border: true.into(),
            height: 10.into(),
            filter: None,
            glyphs: Arc::new(FileTreeGlyphs::default()),
            on_select: None,
            on_rename: None,
            on_delete: None,
            inline_edit: None,
            pending_inline_commit: None,
            defer_inline_commits: false,
        }));
        Self {
            scroll: build_scroll_container(bindings.clone()),
            bindings,
            scrollbar_drag: None,
            last_area: None,
            min_size: (4, 4),
        }
    }

    pub fn title(self, title: impl Into<Binding<String>>) -> Self {
        self.bindings.write().title = title.into();
        self
    }

    pub fn roots(self, roots: impl Into<Binding<Vec<FileTreeNode>>>) -> Self {
        self.bindings.write().roots = roots.into();
        self
    }

    pub fn enabled(self, enabled: impl Into<Binding<bool>>) -> Self {
        self.bindings.write().enabled = enabled.into();
        self
    }

    /// Controls whether the file tree draws its own border. When `false`, the
    /// widget renders borderless (useful when hosted directly in a window whose
    /// chrome already provides a border) while keeping scrollbars correct.
    pub fn border(self, border: impl Into<Binding<bool>>) -> Self {
        self.bindings.write().border = border.into();
        self
    }

    pub fn height(self, height: impl Into<Binding<u16>>) -> Self {
        self.bindings.write().height = height.into();
        self
    }

    pub fn filter(self, filter: impl FileTreeFilter + 'static) -> Self {
        self.bindings.write().filter = Some(Arc::new(filter));
        self
    }

    pub fn clear_filter(self) -> Self {
        self.bindings.write().filter = None;
        self
    }

    pub fn glyphs(self, glyphs: impl FileTreeGlyphProvider + 'static) -> Self {
        self.bindings.write().glyphs = Arc::new(glyphs);
        self
    }

    pub fn glyphs_arc(self, glyphs: Arc<dyn FileTreeGlyphProvider>) -> Self {
        self.bindings.write().glyphs = glyphs;
        self
    }

    pub fn on_select_callback(self, callback: CallbackHandle) -> Self {
        self.bindings.write().on_select = Some(callback);
        self
    }

    pub fn on_rename_callback(self, callback: CallbackHandle) -> Self {
        self.bindings.write().on_rename = Some(callback);
        self
    }

    pub fn on_delete_callback(self, callback: CallbackHandle) -> Self {
        self.bindings.write().on_delete = Some(callback);
        self
    }

    pub fn defer_inline_commits(self, defer: bool) -> Self {
        self.bindings.write().defer_inline_commits = defer;
        self
    }

    pub fn selected(&self) -> Option<FileTreeNodeId> {
        self.bindings.read().selection.get()
    }

    pub fn selected_ids(&self) -> BTreeSet<FileTreeNodeId> {
        self.bindings.read().selections.get()
    }

    pub fn inline_edit(&self) -> Option<FileTreeInlineEditState> {
        self.bindings.read().inline_edit.clone()
    }

    pub fn begin_inline_rename(
        &mut self,
        id: FileTreeNodeId,
        parent_id: Option<FileTreeNodeId>,
        name: impl Into<String>,
    ) {
        self.bindings.write().inline_edit = Some(FileTreeInlineEditState::new(
            Some(id),
            parent_id,
            name,
            FileTreeInlineEditKind::Rename,
            true,
        ));
    }

    pub fn begin_inline_new_file(&mut self, parent_id: Option<FileTreeNodeId>) {
        self.begin_inline_new(parent_id, FileTreeInlineEditKind::NewFile);
    }

    pub fn begin_inline_new_folder(&mut self, parent_id: Option<FileTreeNodeId>) {
        self.begin_inline_new(parent_id, FileTreeInlineEditKind::NewFolder);
    }

    fn begin_inline_new(
        &mut self,
        parent_id: Option<FileTreeNodeId>,
        kind: FileTreeInlineEditKind,
    ) {
        if let Some(parent_id) = parent_id {
            let roots = self.bindings.read().roots.clone();
            roots.update(|nodes| {
                if let Some(node) = find_node_mut(nodes, parent_id)
                    && node.is_dir()
                {
                    node.is_expanded = true;
                }
            });
        }
        self.bindings.write().inline_edit = Some(FileTreeInlineEditState::new(
            None, parent_id, "", kind, false,
        ));
    }

    pub fn cancel_inline_edit(&mut self) {
        let mut bindings = self.bindings.write();
        bindings.inline_edit = None;
        bindings.pending_inline_commit = None;
    }

    pub fn finish_inline_edit(&mut self) {
        self.cancel_inline_edit();
    }

    pub fn take_inline_edit_commit(&mut self) -> Option<FileTreeInlineEditCommit> {
        self.bindings.write().pending_inline_commit.take()
    }

    pub fn node_id_at_mouse_event(
        &self,
        mouse: MouseEvent,
        ctx: ComponentContext<'_>,
    ) -> Option<FileTreeNodeId> {
        let area = self.last_area?;
        let (_local_x, local_y) =
            mouse_coords_local_to_area(area, mouse, ctx.mouse_coordinate_space)?;
        let bindings = self.bindings.read();
        let inset = u16::from(bindings.border.get());
        let row = local_y.checked_sub(inset)? as usize;
        let roots = bindings.roots.get();
        let filter = bindings.filter.clone();
        let glyphs = bindings.glyphs.clone();
        drop(bindings);

        let content = FileTreeContent::new(self.bindings.clone());
        let entries = content.build_visible_entries(&roots, filter.as_deref(), glyphs.as_ref());
        let idx = self.scroll.scroll_offset().1 as usize + row;
        entries
            .get(idx)
            .filter(|entry| entry.inline_placeholder.is_none())
            .map(|entry| entry.id)
    }

    pub fn selections_binding(&self) -> Binding<BTreeSet<FileTreeNodeId>> {
        self.bindings.read().selections.clone()
    }

    pub fn with_min_height(mut self, height: u16) -> Self {
        self.min_size.1 = height;
        self
    }

    pub fn with_min_width(mut self, width: u16) -> Self {
        self.min_size.0 = width;
        self
    }

    pub fn with_min_size(mut self, width: u16, height: u16) -> Self {
        self.min_size = (width, height);
        self
    }
}

impl ComponentPropertySchema for FileTree {
    fn property_schema() -> Vec<PropertyMeta> {
        vec![
            PropertyMeta::new("title", ValueType::String),
            PropertyMeta::new("enabled", ValueType::Bool),
            PropertyMeta::new("border", ValueType::Bool),
            PropertyMeta::new("height", ValueType::U64),
            PropertyMeta::new("selection", ValueType::U64),
            PropertyMeta::new("nodes", ValueType::List),
            // Map of file extension -> icon (`"glyph"` string + optional `"color"`).
            // Write-only over the runtime: it configures the glyph provider but is
            // not read back via `get_property`.
            PropertyMeta::new("icons", ValueType::Map),
        ]
    }
}

impl Clone for FileTree {
    fn clone(&self) -> Self {
        let bindings = self.bindings.clone();
        Self {
            scroll: build_scroll_container(bindings.clone()),
            bindings,
            scrollbar_drag: None,
            last_area: None,
            min_size: self.min_size,
        }
    }
}

impl ::atto_ui::composable::Component for FileTree {
    fn property_names(&self) -> Vec<&'static str> {
        vec!["title", "enabled", "border", "height", "selection", "nodes"]
    }

    fn get_property(&self, name: &str) -> Option<ComponentValue> {
        let bindings = self.bindings.read();
        match name {
            "title" => Some(ComponentValue::String(bindings.title.get())),
            "enabled" => Some(ComponentValue::Bool(bindings.enabled.get())),
            "border" => Some(ComponentValue::Bool(bindings.border.get())),
            "height" => Some(ComponentValue::U64(bindings.height.get() as u64)),
            "selection" => match bindings.selection.get() {
                Some(id) => Some(ComponentValue::U64(id.value())),
                None => Some(ComponentValue::Null),
            },
            "nodes" | "roots" => Some(nodes_to_component_value(&bindings.roots.get())),
            _ => None,
        }
    }

    fn set_property(&mut self, name: &str, value: ComponentValue) -> Result<(), ComponentError> {
        match name {
            "title" => {
                let bindings = self.bindings.read();
                let v = ComponentValueCodec::from_component_value(value, name)?;
                bindings.title.set(v);
                Ok(())
            }
            "enabled" => {
                let bindings = self.bindings.read();
                let v = ComponentValueCodec::from_component_value(value, name)?;
                bindings.enabled.set(v);
                Ok(())
            }
            "border" => {
                let bindings = self.bindings.read();
                let v = ComponentValueCodec::from_component_value(value, name)?;
                bindings.border.set(v);
                Ok(())
            }
            "icons" => {
                let glyphs = parse_file_tree_glyphs(&value)
                    .map_err(|_| ComponentError::invalid_value(name, "map of extension to icon"))?;
                self.bindings.write().glyphs = Arc::new(glyphs);
                Ok(())
            }
            "height" => {
                let bindings = self.bindings.read();
                let v = ComponentValueCodec::from_component_value(value, name)?;
                bindings.height.set(v);
                Ok(())
            }
            "selection" => {
                let next = match value {
                    ComponentValue::Null => None,
                    other => match other.as_u64() {
                        Some(id) => Some(FileTreeNodeId::new(id)),
                        None => {
                            return Err(ComponentError::invalid_value(name, "u64 or null"));
                        }
                    },
                };
                let mut bindings = self.bindings.write();
                bindings.selection.set(next);
                bindings.selections.set(next.into_iter().collect());
                bindings.selection_anchor = next;
                Ok(())
            }
            "nodes" | "roots" => {
                let bindings = self.bindings.read();
                let nodes = parse_nodes_value(&value)
                    .map_err(|_| ComponentError::invalid_value(name, "list"))?;
                bindings.roots.set(nodes);
                Ok(())
            }
            _ => Err(ComponentError::unsupported_property(name)),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);

        let bindings = self.bindings.read();
        let enabled = bindings.enabled.get();
        let border = bindings.border.get();
        let style = if !enabled {
            ctx.theme.widget.disabled
        } else if ctx.is_focused {
            ctx.theme.widget.focused
        } else {
            ctx.theme.widget.normal
        };
        if border {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_set(ctx.theme.border_set(false))
                .title(bindings.title.get())
                .style(style);
            frame.render_widget(block, area);
        } else {
            // No border: still fill the background so content sits on the widget style.
            frame.render_widget(Block::default().style(style), area);
        }
        drop(bindings);

        let body_ctx = ComponentContext {
            scrollbar_host: ScrollbarHost::Window,
            drag: ctx.drag,
            ..ctx
        };
        // The inset is applied via the container's padding (not by pre-insetting
        // the area) so draw and event coordinates stay consistent in both
        // absolute and local mouse-coordinate spaces.
        self.scroll
            .set_padding(self.body_padding(border, ctx.scrollbar_host));
        self.scroll.draw(frame, area, body_ctx);

        if matches!(ctx.scrollbar_host, ScrollbarHost::Component) {
            self.draw_border_scrollbar(frame, area, border, ctx);
        } else {
            self.scrollbar_drag = None;
        }
    }
}

impl ::atto_ui::composable::DragAndDrop for FileTree {
    fn drag_source_at(
        &self,
        screen_x: u16,
        screen_y: u16,
        _ctx: ComponentContext<'_>,
    ) -> Option<DragSource> {
        if self.inline_edit().is_some() {
            return None;
        }
        let area = self.last_area?;
        if !contains(area, screen_x, screen_y) {
            return None;
        }

        let bindings = self.bindings.read();
        if !bindings.enabled.get() {
            return None;
        }
        let inset = u16::from(bindings.border.get());
        let row = screen_y.checked_sub(area.y)?.checked_sub(inset)? as usize;
        let roots = bindings.roots.get();
        let filter = bindings.filter.clone();
        let glyphs = bindings.glyphs.clone();
        let selected_ids = bindings.selections.get();
        drop(bindings);

        let content = FileTreeContent::new(self.bindings.clone());
        let entries = content.build_visible_entries(&roots, filter.as_deref(), glyphs.as_ref());
        let idx = self.scroll.scroll_offset().1 as usize + row;
        let entry = entries
            .get(idx)
            .filter(|entry| entry.inline_placeholder.is_none())?;

        let mut ids = if selected_ids.contains(&entry.id) {
            selected_ids
        } else {
            BTreeSet::from([entry.id])
        };
        ids.remove(&INLINE_PLACEHOLDER_ID);
        if ids.is_empty() {
            return None;
        }

        let data = ids
            .iter()
            .map(|id| id.value().to_string())
            .collect::<Vec<_>>()
            .join(",");
        let ghost = if ids.len() == 1 {
            entry.name.clone()
        } else {
            format!("{} items", ids.len())
        };

        Some(DragSource {
            payload: DragPayload::Custom {
                ty: FILE_TREE_NODE_IDS_DRAG_TYPE,
                data: data.into_bytes(),
            },
            operation: DragOperation::Move,
            threshold: 2,
            ghost: Some(ghost),
        })
    }
}

impl ::atto_ui::composable::Layout for FileTree {
    fn min_width(&self) -> u16 {
        self.min_size.0
    }

    fn min_height(&self) -> u16 {
        self.min_size.1
    }

    fn desired_height(&self) -> Option<u16> {
        let height = self.bindings.read().height.get();
        Some(height.max(self.min_size.1))
    }
}

impl ::atto_ui::composable::Scrollable for FileTree {
    fn is_scrollable(&self) -> bool {
        self.scroll.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.scroll.content_size()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.scroll.viewport_size()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.scroll.scroll_offset()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.scroll.scroll_config()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.scroll.set_scroll_offset(x, y);
    }
}

impl ::atto_ui::composable::FocusNav for FileTree {
    fn is_focusable(&self) -> bool {
        self.bindings.read().enabled.get()
    }
}

impl ::atto_ui::composable::DynamicTree for FileTree {}

impl ::atto_ui::composable::EventHandling for FileTree {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.bindings.read().enabled.get() {
            return EventResult::ignored();
        }

        // Border-mounted scrollbars (right + bottom) so tree content doesn't lose space.
        // When a parent hosts scrollbars (e.g. window chrome / splitter border), we skip this and
        // rely on the parent to handle scrollbar input.
        if matches!(ctx.scrollbar_host, ScrollbarHost::Component)
            && let Event::Mouse(m) = event
            && let Some(area) = self.last_area
            && let Some((local_x, local_y)) =
                mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
        {
            let border = self.bindings.read().border.get();
            let abs_event = MouseEvent {
                column: area.x.saturating_add(local_x),
                row: area.y.saturating_add(local_y),
                ..*m
            };
            if let Some(new_scroll) = self.handle_border_scrollbar_event(abs_event, area, border) {
                self.scroll.set_scroll_offset(new_scroll.x, new_scroll.y);
                return EventResult::consumed();
            }
        }

        let body_ctx = ComponentContext {
            scrollbar_host: ScrollbarHost::Window,
            drag: ctx.drag,
            ..ctx
        };
        self.scroll.handle_event(event, body_ctx)
    }
}

impl FileTree {
    fn scrollbar_visibility(&self) -> (bool, bool) {
        let cfg = self.scroll.scroll_config();
        let content_size = self.scroll.content_size();
        let viewport_size = self.scroll.viewport_size();
        (
            should_show_scrollbar(cfg.vertical_scrollbar, content_size.1, viewport_size.1),
            should_show_scrollbar(cfg.horizontal_scrollbar, content_size.0, viewport_size.0),
        )
    }

    /// Padding applied to the scroll body. With a border the content is inset by
    /// one cell on every side (the border itself); borderless, only a strip is
    /// reserved on the right/bottom for any self-hosted scrollbar.
    fn body_padding(&self, border: bool, host: ScrollbarHost) -> EdgeInsets {
        if border {
            return EdgeInsets::all(1);
        }
        let mut padding = EdgeInsets::ZERO;
        if matches!(host, ScrollbarHost::Component) {
            let (show_v, show_h) = self.scrollbar_visibility();
            if show_v {
                padding.right = 1;
            }
            if show_h {
                padding.bottom = 1;
            }
        }
        padding
    }

    fn border_scrollbars(&self, area: Rect, border: bool) -> Option<Scrollbars> {
        let inset: u16 = if border { 1 } else { 0 };
        if area.width <= 2 * inset || area.height <= 2 * inset {
            return None;
        }

        let (show_v, show_h) = self.scrollbar_visibility();
        if !show_v && !show_h {
            return None;
        }

        // With a border the scrollbar sits on the border line (outside content);
        // borderless, it occupies the last row/column so reserve a strip.
        let reserve_v = if !border && show_v { 1 } else { 0 };
        let reserve_h = if !border && show_h { 1 } else { 0 };

        let content_local = Rect {
            x: inset,
            y: inset,
            width: area
                .width
                .saturating_sub(2 * inset)
                .saturating_sub(reserve_v),
            height: area
                .height
                .saturating_sub(2 * inset)
                .saturating_sub(reserve_h),
        };
        if content_local.width == 0 || content_local.height == 0 {
            return None;
        }

        let vbar = show_v.then_some(Rect {
            x: area.width.saturating_sub(1),
            y: content_local.y,
            width: 1,
            height: content_local.height,
        });
        let hbar = show_h.then_some(Rect {
            x: content_local.x,
            y: area.height.saturating_sub(1),
            width: content_local.width,
            height: 1,
        });

        Some(Scrollbars {
            viewport: content_local,
            content: content_local,
            vbar,
            hbar,
            thickness: 1,
        })
    }

    fn draw_border_scrollbar(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        border: bool,
        ctx: ComponentContext<'_>,
    ) {
        let Some(scrollbars) = self.border_scrollbars(area, border) else {
            self.scrollbar_drag = None;
            return;
        };

        let cfg = self.scroll.scroll_config();
        let content_size = self.scroll.content_size();
        let viewport_size = self.scroll.viewport_size();
        let scroll = self.scroll.scroll_offset();

        draw_scrollbars(
            frame,
            area,
            scrollbars,
            viewport_size,
            content_size,
            ScrollOffset {
                x: scroll.0,
                y: scroll.1,
            },
            cfg,
            ctx.theme,
        );
    }

    fn handle_border_scrollbar_event(
        &mut self,
        m: MouseEvent,
        area: Rect,
        border: bool,
    ) -> Option<ScrollOffset> {
        let Some(scrollbars) = self.border_scrollbars(area, border) else {
            self.scrollbar_drag = None;
            return None;
        };

        let local_x = m.column.saturating_sub(area.x);
        let local_y = m.row.saturating_sub(area.y);

        let cfg = self.scroll.scroll_config();
        let content_size = self.scroll.content_size();
        let scroll = self.scroll.scroll_offset();

        handle_scrollbar_mouse_event(
            cfg,
            scrollbars,
            content_size,
            ScrollOffset {
                x: scroll.0,
                y: scroll.1,
            },
            &mut self.scrollbar_drag,
            local_x,
            local_y,
            m.kind,
        )
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

struct FileTreeContent {
    bindings: Arc<RwLock<FileTreeBindings>>,
    state: ListState,
    last_selection: Option<FileTreeNodeId>,
}

#[derive(Clone, Debug)]
struct VisibleEntry {
    id: FileTreeNodeId,
    parent_id: Option<FileTreeNodeId>,
    depth: usize,
    kind: FileTreeNodeKind,
    is_expanded: bool,
    git_status: Option<FileTreeGitStatus>,
    name: String,
    prefix: String,
    icon: FileTreeIcon,
    inline_placeholder: Option<FileTreeInlineEditKind>,
}

impl VisibleEntry {
    fn is_dir(&self) -> bool {
        matches!(self.kind, FileTreeNodeKind::Directory)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileTreeLineStyle {
    Normal,
    Icon(Option<Color>),
    GitStatus(FileTreeGitStatus),
    InlineEdit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileTreeLineSegment {
    text: String,
    style: FileTreeLineStyle,
}

const INLINE_PLACEHOLDER_ID: FileTreeNodeId = FileTreeNodeId::new(u64::MAX);

impl FileTreeContent {
    fn new(bindings: Arc<RwLock<FileTreeBindings>>) -> Self {
        Self {
            bindings,
            state: ListState::default(),
            last_selection: None,
        }
    }

    fn selection_binding(&self) -> Binding<Option<FileTreeNodeId>> {
        self.bindings.read().selection.clone()
    }

    fn selections_binding(&self) -> Binding<BTreeSet<FileTreeNodeId>> {
        self.bindings.read().selections.clone()
    }

    fn selection_anchor(&self) -> Option<FileTreeNodeId> {
        self.bindings.read().selection_anchor
    }

    fn set_selection_anchor(&mut self, anchor: Option<FileTreeNodeId>) {
        self.bindings.write().selection_anchor = anchor;
    }

    fn roots_binding(&self) -> Binding<Vec<FileTreeNode>> {
        self.bindings.read().roots.clone()
    }

    fn inline_edit(&self) -> Option<FileTreeInlineEditState> {
        self.bindings.read().inline_edit.clone()
    }

    fn update_inline_edit(&mut self, update: impl FnOnce(&mut FileTreeInlineEditState)) {
        let mut bindings = self.bindings.write();
        if let Some(edit) = &mut bindings.inline_edit {
            update(edit);
        }
    }

    fn clear_inline_edit(&mut self) {
        let mut bindings = self.bindings.write();
        bindings.inline_edit = None;
        bindings.pending_inline_commit = None;
    }

    fn defer_inline_commits(&self) -> bool {
        self.bindings.read().defer_inline_commits
    }

    fn set_pending_inline_commit(&mut self, commit: FileTreeInlineEditCommit) {
        self.bindings.write().pending_inline_commit = Some(commit);
    }

    fn bindings_snapshot(&self) -> FileTreeBindingsSnapshot {
        let bindings = self.bindings.read();
        FileTreeBindingsSnapshot {
            title: bindings.title.get(),
            roots: bindings.roots.get(),
            selections: bindings.selections.get(),
            enabled: bindings.enabled.get(),
            filter: bindings.filter.clone(),
            glyphs: bindings.glyphs.clone(),
            on_select: bindings.on_select.clone(),
            on_rename: bindings.on_rename.clone(),
            on_delete: bindings.on_delete.clone(),
        }
    }

    fn normalize_selection(&mut self, visible: &[VisibleEntry]) -> Option<usize> {
        let selection_binding = self.selection_binding();
        let selections_binding = self.selections_binding();
        let mut selection = selection_binding.get();
        if visible.is_empty() {
            if selection.take().is_some() {
                selection_binding.set(None);
            }
            if !selections_binding.get().is_empty() {
                selections_binding.set(BTreeSet::new());
            }
            self.set_selection_anchor(None);
            self.clear_inline_edit();
            self.last_selection = None;
            return None;
        }

        let visible_ids = visible
            .iter()
            .map(|entry| entry.id)
            .collect::<BTreeSet<_>>();
        let mut selection_idx = selection.and_then(|id| {
            let idx = visible.iter().position(|entry| entry.id == id)?;
            Some((idx, id))
        });

        if selection_idx.is_none() {
            let next_id = visible[0].id;
            selection_binding.set(Some(next_id));
            selection = Some(next_id);
            selection_idx = Some((0, next_id));
        }

        let selected_id = selection_idx.map(|(_, id)| id).or(selection);
        let mut selections = selections_binding.get();
        let before = selections.clone();
        selections.retain(|id| visible_ids.contains(id));
        if let Some(id) = selected_id {
            selections.insert(id);
        }
        if selections != before {
            selections_binding.set(selections);
        }

        if let Some(anchor) = self.selection_anchor()
            && !visible_ids.contains(&anchor)
        {
            self.set_selection_anchor(selected_id);
        }

        selection_idx.map(|(idx, _)| idx)
    }

    fn ensure_selection_visible(&mut self, selection: usize, host: &mut ScrollContainerHost) {
        let viewport_h = host.viewport_size().1;
        if viewport_h == 0 {
            return;
        }
        let scroll = host.scroll_offset();
        let sel = selection.min(u16::MAX as usize) as u16;
        let mut next_y = scroll.y;
        if sel < scroll.y {
            next_y = sel;
        } else if sel >= scroll.y.saturating_add(viewport_h) {
            next_y = sel.saturating_add(1).saturating_sub(viewport_h);
        }
        if next_y != scroll.y {
            host.set_scroll_offset(scroll.x, next_y);
        }
    }

    fn maybe_reset_inline_edit(&mut self, selection: Option<FileTreeNodeId>) {
        if let Some(edit) = self.inline_edit()
            && edit.kind == FileTreeInlineEditKind::Rename
            && selection != edit.node_id
        {
            self.clear_inline_edit();
        }
    }

    fn build_visible_entries(
        &self,
        roots: &[FileTreeNode],
        filter: Option<&dyn FileTreeFilter>,
        glyphs: &dyn FileTreeGlyphProvider,
    ) -> Vec<VisibleEntry> {
        let mut out = Vec::new();
        let mut ancestors_last = Vec::new();
        collect_visible_entries(
            roots,
            0,
            &mut ancestors_last,
            None,
            filter,
            glyphs,
            &mut out,
        );
        if let Some(edit) = self.inline_edit()
            && matches!(
                edit.kind,
                FileTreeInlineEditKind::NewFile | FileTreeInlineEditKind::NewFolder
            )
        {
            insert_inline_placeholder(&mut out, &edit, glyphs);
        }
        out
    }

    fn line_text(&self, entry: &VisibleEntry) -> String {
        let mut line = String::new();
        for segment in self.line_segments(entry) {
            line.push_str(&segment.text);
        }
        line
    }

    fn line_segments(&self, entry: &VisibleEntry) -> Vec<FileTreeLineSegment> {
        let mut segments = vec![FileTreeLineSegment {
            text: entry.prefix.clone(),
            style: FileTreeLineStyle::Normal,
        }];
        if !entry.icon.is_empty() {
            segments.push(FileTreeLineSegment {
                text: format!("{} ", entry.icon.glyph),
                style: FileTreeLineStyle::Icon(entry.icon.color),
            });
        }
        if let Some(status) = entry.git_status
            && let Some(badge) = git_status_badge(status)
        {
            segments.push(FileTreeLineSegment {
                text: format!("{badge} "),
                style: FileTreeLineStyle::GitStatus(status),
            });
        }

        let mut name = String::new();
        let editing = if let Some(edit) = self.inline_edit()
            && inline_edit_applies_to_entry(&edit, entry)
        {
            let text = edit.text.text();
            let cursor = edit.text.cursor_byte_index().min(text.len());
            let (left, right) = text.split_at(cursor);
            name.push_str(left);
            name.push('|');
            name.push_str(right);
            true
        } else {
            name.push_str(&entry.name);
            false
        };
        segments.push(FileTreeLineSegment {
            text: name,
            style: if editing {
                FileTreeLineStyle::InlineEdit
            } else {
                FileTreeLineStyle::Normal
            },
        });
        segments
    }

    fn selected_range_ids(
        &self,
        anchor: FileTreeNodeId,
        idx: usize,
        visible: &[VisibleEntry],
    ) -> BTreeSet<FileTreeNodeId> {
        visible_range_selection(anchor, idx, visible)
    }

    fn emit_select_if_needed(
        &self,
        old_selection: Option<FileTreeNodeId>,
        new_selection: Option<FileTreeNodeId>,
        cb: Option<&CallbackHandle>,
    ) {
        if old_selection != new_selection
            && let Some(cb) = cb
        {
            cb.emit_with(new_selection.map(|id| ComponentValue::U64(id.value())));
        }
    }

    fn apply_selection_state(
        &mut self,
        primary: Option<FileTreeNodeId>,
        selected_ids: BTreeSet<FileTreeNodeId>,
        anchor: Option<FileTreeNodeId>,
        idx: Option<usize>,
        host: &mut ScrollContainerHost,
        cb: Option<&CallbackHandle>,
    ) -> EventResult {
        let selection_binding = self.selection_binding();
        let selections_binding = self.selections_binding();
        let old_selection = selection_binding.get();
        let old_selected_ids = selections_binding.get();
        selection_binding.set(primary);
        selections_binding.set(selected_ids);
        self.set_selection_anchor(anchor);
        if let Some(idx) = idx {
            self.ensure_selection_visible(idx, host);
        }
        self.last_selection = primary;
        self.maybe_reset_inline_edit(primary);
        self.emit_select_if_needed(old_selection, primary, cb);
        if old_selection == primary && old_selected_ids == self.selections_binding().get() {
            EventResult::consumed()
        } else {
            EventResult::changed()
        }
    }

    fn select_index(
        &mut self,
        idx: usize,
        visible: &[VisibleEntry],
        host: &mut ScrollContainerHost,
        cb: Option<&CallbackHandle>,
    ) -> EventResult {
        if let Some(entry) = visible.get(idx) {
            return self.apply_selection_state(
                Some(entry.id),
                BTreeSet::from([entry.id]),
                Some(entry.id),
                Some(idx),
                host,
                cb,
            );
        }
        EventResult::ignored()
    }

    fn range_select_index(
        &mut self,
        idx: usize,
        visible: &[VisibleEntry],
        host: &mut ScrollContainerHost,
        cb: Option<&CallbackHandle>,
    ) -> EventResult {
        let Some(entry) = visible.get(idx) else {
            return EventResult::ignored();
        };
        let anchor = self
            .selection_anchor()
            .or_else(|| self.selection_binding().get())
            .unwrap_or(entry.id);
        let selected_ids = self.selected_range_ids(anchor, idx, visible);
        self.apply_selection_state(
            Some(entry.id),
            selected_ids,
            Some(anchor),
            Some(idx),
            host,
            cb,
        )
    }

    fn toggle_selection_index(
        &mut self,
        idx: usize,
        visible: &[VisibleEntry],
        host: &mut ScrollContainerHost,
        cb: Option<&CallbackHandle>,
    ) -> EventResult {
        let Some(entry) = visible.get(idx) else {
            return EventResult::ignored();
        };
        let selection_binding = self.selection_binding();
        let mut selected_ids = self.selections_binding().get();
        let old_primary = selection_binding.get();
        let mut primary = old_primary;

        if selected_ids.contains(&entry.id) {
            if selected_ids.len() > 1 {
                selected_ids.remove(&entry.id);
                if old_primary == Some(entry.id) {
                    primary = visible
                        .iter()
                        .find(|candidate| selected_ids.contains(&candidate.id))
                        .map(|candidate| candidate.id);
                }
            } else {
                primary = Some(entry.id);
            }
        } else {
            selected_ids.insert(entry.id);
            primary = Some(entry.id);
        }

        if let Some(id) = primary {
            selected_ids.insert(id);
        }

        self.apply_selection_state(primary, selected_ids, Some(entry.id), Some(idx), host, cb)
    }

    fn move_selection(
        &mut self,
        delta: i32,
        extend: bool,
        visible: &[VisibleEntry],
        host: &mut ScrollContainerHost,
        cb: Option<&CallbackHandle>,
    ) -> EventResult {
        let selection = self.selection_binding().get();
        let Some(current_idx) =
            selection.and_then(|id| visible.iter().position(|entry| entry.id == id))
        else {
            return EventResult::ignored();
        };

        let next_idx = if delta.is_negative() {
            current_idx.saturating_sub((-delta) as usize)
        } else {
            current_idx
                .saturating_add(delta as usize)
                .min(visible.len().saturating_sub(1))
        };
        if extend {
            self.range_select_index(next_idx, visible, host, cb)
        } else {
            self.select_index(next_idx, visible, host, cb)
        }
    }

    fn toggle_directory(&mut self, id: FileTreeNodeId) -> bool {
        let roots = self.roots_binding();
        let mut changed = false;
        roots.update(|nodes| {
            if let Some(node) = find_node_mut(nodes, id)
                && node.is_dir()
            {
                node.is_expanded = !node.is_expanded;
                changed = true;
            }
        });
        changed
    }

    fn expand_directory(&mut self, id: FileTreeNodeId) -> bool {
        let roots = self.roots_binding();
        let mut changed = false;
        roots.update(|nodes| {
            if let Some(node) = find_node_mut(nodes, id)
                && node.is_dir()
                && !node.is_expanded
            {
                node.is_expanded = true;
                changed = true;
            }
        });
        changed
    }

    fn collapse_directory(&mut self, id: FileTreeNodeId) -> bool {
        let roots = self.roots_binding();
        let mut changed = false;
        roots.update(|nodes| {
            if let Some(node) = find_node_mut(nodes, id)
                && node.is_dir()
                && node.is_expanded
            {
                node.is_expanded = false;
                changed = true;
            }
        });
        changed
    }

    fn start_rename(&mut self, id: FileTreeNodeId, parent_id: Option<FileTreeNodeId>, name: &str) {
        self.bindings.write().inline_edit = Some(FileTreeInlineEditState::new(
            Some(id),
            parent_id,
            name,
            FileTreeInlineEditKind::Rename,
            true,
        ));
    }

    fn commit_inline_edit(&mut self) -> Option<FileTreeInlineEditCommit> {
        let edit = self.inline_edit()?;
        let new_name = edit.text.text().to_string();
        let deferred = self.defer_inline_commits();
        if new_name.trim().is_empty() && !deferred {
            self.clear_inline_edit();
            return None;
        }

        let mut old_name = None;
        let mut node_kind = None;
        if let Some(id) = edit.node_id {
            let roots = self.roots_binding();
            if deferred {
                let nodes = roots.get();
                if let Some(node) = find_node(&nodes, id) {
                    old_name = Some(node.name.clone());
                    node_kind = Some(node.kind);
                }
            } else {
                roots.update(|nodes| {
                    if let Some(node) = find_node_mut(nodes, id) {
                        old_name = Some(node.name.clone());
                        node_kind = Some(node.kind);
                        if edit.kind == FileTreeInlineEditKind::Rename
                            && !new_name.trim().is_empty()
                        {
                            node.name = new_name.clone();
                        }
                    }
                });
            }
        }

        let commit = FileTreeInlineEditCommit {
            node_id: edit.node_id,
            parent_id: edit.parent_id,
            kind: edit.kind,
            text: new_name,
            old_name,
            node_kind,
        };
        if deferred {
            self.set_pending_inline_commit(commit.clone());
        } else {
            self.clear_inline_edit();
        }
        Some(commit)
    }

    fn delete_selected(
        &mut self,
        id: FileTreeNodeId,
    ) -> Option<(FileTreeNode, Option<FileTreeNodeId>)> {
        let roots = self.roots_binding();
        let mut removed = None;
        let mut parent = None;
        roots.update(|nodes| {
            parent = find_parent_id(nodes, id);
            removed = remove_node_by_id(nodes, id);
        });
        removed.map(|node| (node, parent))
    }
}

struct FileTreeBindingsSnapshot {
    #[allow(dead_code)]
    title: String,
    roots: Vec<FileTreeNode>,
    selections: BTreeSet<FileTreeNodeId>,
    enabled: bool,
    filter: Option<Arc<dyn FileTreeFilter>>,
    glyphs: Arc<dyn FileTreeGlyphProvider>,
    on_select: Option<CallbackHandle>,
    on_rename: Option<CallbackHandle>,
    on_delete: Option<CallbackHandle>,
}

impl ScrollContent for FileTreeContent {
    fn is_focusable(&self) -> bool {
        self.bindings.read().enabled.get()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.bindings.read().height.get())
    }

    fn content_size(
        &mut self,
        _viewport: (u16, u16),
        _ctx: ScrollContentContext<'_>,
    ) -> (u16, u16) {
        let snapshot = self.bindings_snapshot();
        let filter = snapshot.filter.as_deref();
        let entries = self.build_visible_entries(&snapshot.roots, filter, snapshot.glyphs.as_ref());
        let mut width = 0_u16;
        for entry in &entries {
            let line = self.line_text(entry);
            let w = UnicodeWidthStr::width(line.as_str()).min(u16::MAX as usize) as u16;
            width = width.max(w);
        }
        let height = entries.len().min(u16::MAX as usize) as u16;
        (width, height)
    }

    fn on_scrollbars(&mut self, _ctx: ScrollContentContext<'_>, host: &mut ScrollContainerHost) {
        let snapshot = self.bindings_snapshot();
        let filter = snapshot.filter.as_deref();
        let entries = self.build_visible_entries(&snapshot.roots, filter, snapshot.glyphs.as_ref());
        let selection = self.normalize_selection(&entries);
        let selection_now = self.selection_binding().get();
        self.maybe_reset_inline_edit(selection_now);
        if let Some(idx) = selection {
            let selected_id = entries[idx].id;
            if self.last_selection != Some(selected_id) {
                self.ensure_selection_visible(idx, host);
                self.last_selection = Some(selected_id);
            }
        }
    }

    fn handle_event(
        &mut self,
        event: &Event,
        _ctx: ScrollContentContext<'_>,
        host: &mut ScrollContainerHost,
    ) -> EventResult {
        let snapshot = self.bindings_snapshot();
        if !snapshot.enabled {
            return EventResult::ignored();
        }
        let filter = snapshot.filter.as_deref();
        let entries = self.build_visible_entries(&snapshot.roots, filter, snapshot.glyphs.as_ref());
        let selection_idx = self.normalize_selection(&entries);
        let selection_now = self.selection_binding().get();
        self.maybe_reset_inline_edit(selection_now);

        if self.inline_edit().is_some() {
            if let Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) = event
            {
                if matches!(kind, KeyEventKind::Release) {
                    return EventResult::ignored();
                }
                match code {
                    KeyCode::Esc => {
                        self.clear_inline_edit();
                        return EventResult::consumed();
                    }
                    KeyCode::Enter => {
                        let deferred = self.defer_inline_commits();
                        if let Some(commit) = self.commit_inline_edit() {
                            if deferred {
                                return EventResult::consumed();
                            }
                            if commit.kind == FileTreeInlineEditKind::Rename
                                && commit.old_name.as_deref() != Some(commit.text.as_str())
                                && let (Some(rename_id), Some(kind), Some(old_name)) =
                                    (commit.node_id, commit.node_kind, commit.old_name.as_deref())
                                && let Some(cb) = &snapshot.on_rename
                            {
                                cb.emit_with(Some(rename_payload(
                                    rename_id,
                                    kind,
                                    old_name,
                                    &commit.text,
                                )));
                            }
                            return EventResult::changed();
                        }
                        if deferred {
                            return EventResult::consumed();
                        }
                        return EventResult::consumed();
                    }
                    KeyCode::Backspace => {
                        self.update_inline_edit(|edit| {
                            edit.replace_on_input = false;
                            edit.text.backspace();
                        });
                        return EventResult::changed();
                    }
                    KeyCode::Delete => {
                        self.update_inline_edit(|edit| {
                            edit.replace_on_input = false;
                            edit.text.delete();
                        });
                        return EventResult::changed();
                    }
                    KeyCode::Left => {
                        self.update_inline_edit(|edit| {
                            edit.replace_on_input = false;
                            edit.text.move_left();
                        });
                        return EventResult::consumed();
                    }
                    KeyCode::Right => {
                        self.update_inline_edit(|edit| {
                            edit.replace_on_input = false;
                            edit.text.move_right();
                        });
                        return EventResult::consumed();
                    }
                    KeyCode::Home => {
                        self.update_inline_edit(|edit| {
                            edit.replace_on_input = false;
                            edit.text.move_home();
                        });
                        return EventResult::consumed();
                    }
                    KeyCode::End => {
                        self.update_inline_edit(|edit| {
                            edit.replace_on_input = false;
                            edit.text.move_end();
                        });
                        return EventResult::consumed();
                    }
                    KeyCode::Char(ch) if !modifiers.contains(KeyModifiers::CONTROL) => {
                        self.update_inline_edit(|edit| {
                            if edit.replace_on_input {
                                edit.text.set_text("");
                                edit.replace_on_input = false;
                            }
                            edit.text.insert_char(*ch);
                        });
                        return EventResult::changed();
                    }
                    _ => {}
                }
            }
            return EventResult::ignored();
        }

        match event {
            Event::Mouse(m) => {
                if m.kind != MouseEventKind::Down(MouseButton::Left) {
                    return EventResult::ignored();
                }
                let row = m.row as usize;
                let idx = host.scroll_offset().y as usize + row;
                if let Some(entry) = entries.get(idx) {
                    if entry.inline_placeholder.is_some() {
                        return EventResult::consumed();
                    }
                    let res = if m.modifiers.contains(KeyModifiers::SHIFT) {
                        self.range_select_index(idx, &entries, host, snapshot.on_select.as_ref())
                    } else if m.modifiers.contains(KeyModifiers::CONTROL) {
                        self.toggle_selection_index(
                            idx,
                            &entries,
                            host,
                            snapshot.on_select.as_ref(),
                        )
                    } else {
                        self.select_index(idx, &entries, host, snapshot.on_select.as_ref())
                    };
                    if !m
                        .modifiers
                        .intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL)
                        && entry.is_dir()
                        && self.toggle_directory(entry.id)
                    {
                        return EventResult::changed();
                    }
                    return res;
                }
                EventResult::ignored()
            }
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) => {
                if matches!(kind, KeyEventKind::Release) {
                    return EventResult::ignored();
                }
                let extend = modifiers.contains(KeyModifiers::SHIFT);

                match code {
                    KeyCode::Up => {
                        if entries.is_empty() {
                            return EventResult::ignored();
                        }
                        self.move_selection(-1, extend, &entries, host, snapshot.on_select.as_ref())
                    }
                    KeyCode::Down => {
                        if entries.is_empty() {
                            return EventResult::ignored();
                        }
                        self.move_selection(1, extend, &entries, host, snapshot.on_select.as_ref())
                    }
                    KeyCode::Left => {
                        let Some(idx) = selection_idx else {
                            return EventResult::ignored();
                        };
                        let entry = &entries[idx];
                        if entry.is_dir() && entry.is_expanded {
                            if self.collapse_directory(entry.id) {
                                return EventResult::changed();
                            }
                            return EventResult::consumed();
                        }
                        if let Some(parent) = entry.parent_id
                            && let Some(parent_idx) = entries.iter().position(|e| e.id == parent)
                        {
                            return self.select_index(
                                parent_idx,
                                &entries,
                                host,
                                snapshot.on_select.as_ref(),
                            );
                        }
                        EventResult::consumed()
                    }
                    KeyCode::Right => {
                        let Some(idx) = selection_idx else {
                            return EventResult::ignored();
                        };
                        let entry = &entries[idx];
                        if entry.is_dir() {
                            if !entry.is_expanded {
                                if self.expand_directory(entry.id) {
                                    return EventResult::changed();
                                }
                                return EventResult::consumed();
                            }
                            let mut child_idx = None;
                            for (i, e) in entries.iter().enumerate().skip(idx + 1) {
                                if e.depth <= entry.depth {
                                    break;
                                }
                                if e.parent_id == Some(entry.id) {
                                    child_idx = Some(i);
                                    break;
                                }
                            }
                            if let Some(child_idx) = child_idx {
                                return self.select_index(
                                    child_idx,
                                    &entries,
                                    host,
                                    snapshot.on_select.as_ref(),
                                );
                            }
                        }
                        EventResult::consumed()
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        let Some(idx) = selection_idx else {
                            return EventResult::ignored();
                        };
                        let entry = &entries[idx];
                        if entry.is_dir() {
                            if self.toggle_directory(entry.id) {
                                return EventResult::changed();
                            }
                            return EventResult::consumed();
                        }
                        EventResult::consumed()
                    }
                    KeyCode::Char('r') | KeyCode::F(2) => {
                        let Some(idx) = selection_idx else {
                            return EventResult::ignored();
                        };
                        let entry = &entries[idx];
                        self.start_rename(entry.id, entry.parent_id, &entry.name);
                        EventResult::consumed()
                    }
                    KeyCode::Delete | KeyCode::Backspace | KeyCode::Char('d') => {
                        let Some(idx) = selection_idx else {
                            return EventResult::ignored();
                        };
                        let entry = &entries[idx];
                        if let Some((removed, parent)) = self.delete_selected(entry.id) {
                            if let Some(cb) = &snapshot.on_delete {
                                cb.emit_with(Some(delete_payload(
                                    removed.id,
                                    removed.kind,
                                    &removed.name,
                                )));
                            }
                            let updated = self.build_visible_entries(
                                &self.roots_binding().get(),
                                filter,
                                snapshot.glyphs.as_ref(),
                            );
                            let next_id = if idx < updated.len() {
                                Some(updated[idx].id)
                            } else if idx > 0 {
                                updated.get(idx.saturating_sub(1)).map(|e| e.id)
                            } else {
                                parent
                            };
                            self.selection_binding().set(next_id);
                            let next_ids = next_id.into_iter().collect::<BTreeSet<_>>();
                            self.selections_binding().set(next_ids);
                            self.set_selection_anchor(next_id);
                            self.last_selection = next_id;
                            return EventResult::changed();
                        }
                        EventResult::ignored()
                    }
                    _ => EventResult::ignored(),
                }
            }
            _ => EventResult::ignored(),
        }
    }

    fn draw(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ScrollContentContext<'_>,
        _host: &mut ScrollContainerHost,
    ) {
        let snapshot = self.bindings_snapshot();
        let enabled = snapshot.enabled;
        let style = if !enabled {
            ctx.component.theme.widget.disabled
        } else if ctx.component.is_focused {
            ctx.component.theme.widget.focused
        } else {
            ctx.component.theme.widget.normal
        };
        // Use the same selection palette as ListBox/TableView so the highlighted
        // row's foreground stays readable on the selection background.
        let selection_style = ctx
            .component
            .theme
            .named_style("list-selection")
            .unwrap_or(ctx.component.theme.selection);
        let highlight_style = if enabled {
            selection_style
        } else {
            selection_style.patch(ctx.component.theme.widget.disabled)
        };

        let filter = snapshot.filter.as_deref();
        let entries = self.build_visible_entries(&snapshot.roots, filter, snapshot.glyphs.as_ref());
        let selection = self.normalize_selection(&entries);
        let selections = snapshot.selections;

        let scroll = ctx.info.scroll_offset;
        let viewport_w = area.width;
        let items: Vec<ListItem> = entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let selected =
                    selection.is_some_and(|sel| sel == idx) || selections.contains(&entry.id);
                // On the selected row the regular text adopts the highlight style so
                // its foreground contrasts with the selection background (otherwise
                // the dim widget foreground would sit on the highlight, unreadable).
                let normal_style = if selected { highlight_style } else { style };
                let segments = self.line_segments(entry);
                let visible_segments =
                    slice_segments_by_display_width(&segments, scroll.x, viewport_w);
                let spans = if visible_segments.is_empty() {
                    vec![Span::styled(String::new(), normal_style)]
                } else {
                    visible_segments
                        .into_iter()
                        .map(|segment| {
                            let segment_style = match segment.style {
                                FileTreeLineStyle::Normal => normal_style,
                                FileTreeLineStyle::Icon(color) => match color {
                                    // Keep the icon's own color (over the row's
                                    // background); without a color it follows the row.
                                    Some(color) => normal_style.fg(color),
                                    None => normal_style,
                                },
                                FileTreeLineStyle::GitStatus(status) => {
                                    git_status_style(ctx.component.theme, status)
                                }
                                FileTreeLineStyle::InlineEdit => ctx.component.theme.widget.focused,
                            };
                            Span::styled(segment.text, segment_style)
                        })
                        .collect::<Vec<_>>()
                };
                let item = ListItem::new(Line::from(spans));
                if selected {
                    item.style(highlight_style)
                } else {
                    item
                }
            })
            .collect();

        *self.state.selected_mut() = None;
        *self.state.offset_mut() = scroll.y as usize;

        if area.width > 0 && area.height > 0 {
            frame.render_widget(Clear, area);
            let list = List::new(items).style(style);
            frame.render_stateful_widget(list, area, &mut self.state);
        }
    }
}

fn collect_visible_entries(
    nodes: &[FileTreeNode],
    depth: usize,
    ancestors_last: &mut Vec<bool>,
    parent_id: Option<FileTreeNodeId>,
    filter: Option<&dyn FileTreeFilter>,
    glyphs: &dyn FileTreeGlyphProvider,
    out: &mut Vec<VisibleEntry>,
) {
    let mut visible_indices = Vec::new();
    for (idx, node) in nodes.iter().enumerate() {
        if filter_allows(node, filter) {
            visible_indices.push(idx);
        }
    }

    let total = visible_indices.len();
    for (pos, idx) in visible_indices.into_iter().enumerate() {
        let node = &nodes[idx];
        let is_last = pos + 1 == total;
        let prefix = build_prefix(node, depth, ancestors_last, is_last);
        let icon = glyphs.icon_for(node, node.is_expanded);
        out.push(VisibleEntry {
            id: node.id,
            parent_id,
            depth,
            kind: node.kind,
            is_expanded: node.is_expanded,
            git_status: node.git_status,
            name: node.name.clone(),
            prefix,
            icon,
            inline_placeholder: None,
        });

        if node.is_dir() && node.is_expanded && !node.children.is_empty() {
            ancestors_last.push(is_last);
            collect_visible_entries(
                &node.children,
                depth + 1,
                ancestors_last,
                Some(node.id),
                filter,
                glyphs,
                out,
            );
            ancestors_last.pop();
        }
    }
}

fn insert_inline_placeholder(
    entries: &mut Vec<VisibleEntry>,
    edit: &FileTreeInlineEditState,
    glyphs: &dyn FileTreeGlyphProvider,
) {
    let kind = match edit.kind {
        FileTreeInlineEditKind::NewFile => FileTreeNodeKind::File,
        FileTreeInlineEditKind::NewFolder => FileTreeNodeKind::Directory,
        FileTreeInlineEditKind::Rename => return,
    };
    let node = match kind {
        FileTreeNodeKind::File => FileTreeNode::file(INLINE_PLACEHOLDER_ID, ""),
        FileTreeNodeKind::Directory => FileTreeNode::dir(INLINE_PLACEHOLDER_ID, "", Vec::new()),
    };

    let (insert_at, depth) = if let Some(parent_id) = edit.parent_id {
        let Some(parent_idx) = entries.iter().position(|entry| entry.id == parent_id) else {
            return;
        };
        let parent_depth = entries[parent_idx].depth;
        let mut insert_at = parent_idx + 1;
        while insert_at < entries.len() && entries[insert_at].depth > parent_depth {
            insert_at += 1;
        }
        (insert_at, parent_depth + 1)
    } else {
        (entries.len(), 0)
    };

    let mut prefix = String::new();
    if depth > 0 {
        for _ in 1..depth {
            prefix.push_str("  ");
        }
        prefix.push_str("└─ ");
    }
    prefix.push_str("  ");
    let icon = glyphs.icon_for(&node, false);

    entries.insert(
        insert_at,
        VisibleEntry {
            id: INLINE_PLACEHOLDER_ID,
            parent_id: edit.parent_id,
            depth,
            kind,
            is_expanded: false,
            git_status: None,
            name: String::new(),
            prefix,
            icon,
            inline_placeholder: Some(edit.kind),
        },
    );
}

fn inline_edit_applies_to_entry(edit: &FileTreeInlineEditState, entry: &VisibleEntry) -> bool {
    match edit.kind {
        FileTreeInlineEditKind::Rename => edit.node_id == Some(entry.id),
        FileTreeInlineEditKind::NewFile | FileTreeInlineEditKind::NewFolder => {
            entry.inline_placeholder == Some(edit.kind)
        }
    }
}

fn build_prefix(
    node: &FileTreeNode,
    depth: usize,
    ancestors_last: &[bool],
    is_last: bool,
) -> String {
    let mut prefix = String::new();
    for last in ancestors_last {
        if *last {
            prefix.push_str("  ");
        } else {
            prefix.push_str("│ ");
        }
    }
    if depth > 0 {
        if is_last {
            prefix.push_str("└─ ");
        } else {
            prefix.push_str("├─ ");
        }
    }

    let indicator = if node.is_dir() && (!node.children.is_empty() || !node.children_loaded) {
        if node.is_expanded { '▼' } else { '▶' }
    } else {
        ' '
    };
    prefix.push(indicator);
    prefix.push(' ');
    prefix
}

fn filter_allows(node: &FileTreeNode, filter: Option<&dyn FileTreeFilter>) -> bool {
    match filter {
        Some(filter) => filter.include(node),
        None => true,
    }
}

fn slice_segments_by_display_width(
    segments: &[FileTreeLineSegment],
    start: u16,
    width: u16,
) -> Vec<FileTreeLineSegment> {
    if width == 0 {
        return Vec::new();
    }
    let end = start.saturating_add(width);
    let mut col: u16 = 0;
    let mut out = Vec::new();
    for segment in segments {
        let mut text = String::new();
        for g in segment.text.graphemes(true) {
            let w = UnicodeWidthStr::width(g).min(u16::MAX as usize) as u16;
            if col.saturating_add(w) <= start {
                col = col.saturating_add(w);
                continue;
            }
            if col >= end {
                break;
            }
            text.push_str(g);
            col = col.saturating_add(w);
            if col >= end {
                break;
            }
        }
        if !text.is_empty() {
            out.push(FileTreeLineSegment {
                text,
                style: segment.style,
            });
        }
        if col >= end {
            break;
        }
    }
    out
}

fn visible_range_selection(
    anchor: FileTreeNodeId,
    idx: usize,
    visible: &[VisibleEntry],
) -> BTreeSet<FileTreeNodeId> {
    let Some(anchor_idx) = visible.iter().position(|entry| entry.id == anchor) else {
        return visible
            .get(idx)
            .map(|entry| BTreeSet::from([entry.id]))
            .unwrap_or_default();
    };
    let (start, end) = if anchor_idx <= idx {
        (anchor_idx, idx)
    } else {
        (idx, anchor_idx)
    };
    visible[start..=end].iter().map(|entry| entry.id).collect()
}

fn git_status_badge(status: FileTreeGitStatus) -> Option<&'static str> {
    match status {
        FileTreeGitStatus::Modified => Some("M"),
        FileTreeGitStatus::Added => Some("A"),
        FileTreeGitStatus::Deleted => Some("D"),
        FileTreeGitStatus::Renamed => Some("R"),
        FileTreeGitStatus::Untracked => Some("?"),
        FileTreeGitStatus::Ignored => Some("I"),
        FileTreeGitStatus::Clean => None,
    }
}

fn git_status_style(theme: &atto_ui::theme::Theme, status: FileTreeGitStatus) -> Style {
    let named = match status {
        FileTreeGitStatus::Modified => "file-tree-git-modified",
        FileTreeGitStatus::Added => "file-tree-git-added",
        FileTreeGitStatus::Deleted => "file-tree-git-deleted",
        FileTreeGitStatus::Renamed => "file-tree-git-renamed",
        FileTreeGitStatus::Untracked => "file-tree-git-untracked",
        FileTreeGitStatus::Ignored => "file-tree-git-ignored",
        FileTreeGitStatus::Clean => "file-tree-git-clean",
    };
    theme
        .named_style(named)
        .unwrap_or_else(|| match status {
            FileTreeGitStatus::Modified => Style::default().fg(Color::Yellow),
            FileTreeGitStatus::Added => Style::default().fg(Color::Green),
            FileTreeGitStatus::Deleted => Style::default().fg(Color::Red),
            FileTreeGitStatus::Renamed => Style::default().fg(Color::LightBlue),
            FileTreeGitStatus::Untracked => Style::default().fg(Color::Magenta),
            FileTreeGitStatus::Ignored => Style::default().fg(Color::DarkGray),
            FileTreeGitStatus::Clean => theme.widget.accent,
        })
        .add_modifier(Modifier::BOLD)
}

fn find_node_mut(nodes: &mut [FileTreeNode], id: FileTreeNodeId) -> Option<&mut FileTreeNode> {
    for node in nodes {
        if node.id == id {
            return Some(node);
        }
        if let Some(found) = find_node_mut(&mut node.children, id) {
            return Some(found);
        }
    }
    None
}

fn find_node(nodes: &[FileTreeNode], id: FileTreeNodeId) -> Option<&FileTreeNode> {
    for node in nodes {
        if node.id == id {
            return Some(node);
        }
        if let Some(found) = find_node(&node.children, id) {
            return Some(found);
        }
    }
    None
}

fn find_parent_id(nodes: &[FileTreeNode], id: FileTreeNodeId) -> Option<FileTreeNodeId> {
    for node in nodes {
        if node.children.iter().any(|child| child.id == id) {
            return Some(node.id);
        }
        if let Some(found) = find_parent_id(&node.children, id) {
            return Some(found);
        }
    }
    None
}

fn remove_node_by_id(nodes: &mut Vec<FileTreeNode>, id: FileTreeNodeId) -> Option<FileTreeNode> {
    let mut idx = 0;
    while idx < nodes.len() {
        if nodes[idx].id == id {
            return Some(nodes.remove(idx));
        }
        if let Some(removed) = remove_node_by_id(&mut nodes[idx].children, id) {
            return Some(removed);
        }
        idx += 1;
    }
    None
}

fn rename_payload(
    id: FileTreeNodeId,
    kind: FileTreeNodeKind,
    old_name: &str,
    new_name: &str,
) -> ComponentValue {
    let mut map = BTreeMap::new();
    map.insert("id".to_string(), ComponentValue::U64(id.value()));
    map.insert("kind".to_string(), ComponentValue::String(kind_label(kind)));
    map.insert(
        "old_name".to_string(),
        ComponentValue::String(old_name.to_string()),
    );
    map.insert(
        "new_name".to_string(),
        ComponentValue::String(new_name.to_string()),
    );
    ComponentValue::Map(map)
}

fn delete_payload(id: FileTreeNodeId, kind: FileTreeNodeKind, name: &str) -> ComponentValue {
    let mut map = BTreeMap::new();
    map.insert("id".to_string(), ComponentValue::U64(id.value()));
    map.insert("kind".to_string(), ComponentValue::String(kind_label(kind)));
    map.insert("name".to_string(), ComponentValue::String(name.to_string()));
    ComponentValue::Map(map)
}

fn kind_label(kind: FileTreeNodeKind) -> String {
    match kind {
        FileTreeNodeKind::File => "file".to_string(),
        FileTreeNodeKind::Directory => "directory".to_string(),
    }
}

fn git_status_label(status: FileTreeGitStatus) -> String {
    match status {
        FileTreeGitStatus::Modified => "modified",
        FileTreeGitStatus::Added => "added",
        FileTreeGitStatus::Deleted => "deleted",
        FileTreeGitStatus::Renamed => "renamed",
        FileTreeGitStatus::Untracked => "untracked",
        FileTreeGitStatus::Ignored => "ignored",
        FileTreeGitStatus::Clean => "clean",
    }
    .to_string()
}

fn parse_git_status(value: &str) -> Option<FileTreeGitStatus> {
    match value.to_ascii_lowercase().as_str() {
        "modified" | "m" => Some(FileTreeGitStatus::Modified),
        "added" | "a" => Some(FileTreeGitStatus::Added),
        "deleted" | "d" => Some(FileTreeGitStatus::Deleted),
        "renamed" | "r" => Some(FileTreeGitStatus::Renamed),
        "untracked" | "?" => Some(FileTreeGitStatus::Untracked),
        "ignored" | "i" => Some(FileTreeGitStatus::Ignored),
        "clean" => Some(FileTreeGitStatus::Clean),
        _ => None,
    }
}

fn nodes_to_component_value(nodes: &[FileTreeNode]) -> ComponentValue {
    let items = nodes.iter().map(node_to_component_value).collect();
    ComponentValue::List(items)
}

fn node_to_component_value(node: &FileTreeNode) -> ComponentValue {
    let mut map = BTreeMap::new();
    map.insert("id".to_string(), ComponentValue::U64(node.id.value()));
    map.insert(
        "name".to_string(),
        ComponentValue::String(node.name.clone()),
    );
    map.insert(
        "kind".to_string(),
        ComponentValue::String(kind_label(node.kind)),
    );
    map.insert(
        "expanded".to_string(),
        ComponentValue::Bool(node.is_expanded),
    );
    if let Some(status) = node.git_status {
        map.insert(
            "git_status".to_string(),
            ComponentValue::String(git_status_label(status)),
        );
    }
    map.insert(
        "children".to_string(),
        nodes_to_component_value(&node.children),
    );
    ComponentValue::Map(map)
}

fn parse_nodes_value(value: &ComponentValue) -> Result<Vec<FileTreeNode>, String> {
    match value {
        ComponentValue::Null => Ok(Vec::new()),
        ComponentValue::List(items) => items.iter().map(parse_node_value).collect(),
        other => Err(format!("expected list, got {other:?}")),
    }
}

fn parse_node_value(value: &ComponentValue) -> Result<FileTreeNode, String> {
    let ComponentValue::Map(map) = value else {
        return Err("expected map".to_string());
    };

    let id_value = map.get("id").ok_or_else(|| "missing id".to_string())?;
    let id = id_value
        .as_u64()
        .ok_or_else(|| "id must be number".to_string())?;
    let name = map
        .get("name")
        .and_then(ComponentValue::as_str)
        .ok_or_else(|| "missing name".to_string())?
        .to_string();

    let children_value = map.get("children").or_else(|| map.get("nodes"));
    let children = match children_value {
        Some(value) => parse_nodes_value(value)?,
        None => Vec::new(),
    };

    let is_expanded = match map.get("expanded").or_else(|| map.get("is_expanded")) {
        Some(ComponentValue::Bool(v)) => *v,
        Some(other) => return Err(format!("expanded must be bool, got {other:?}")),
        None => false,
    };

    let kind = match map.get("kind").and_then(ComponentValue::as_str) {
        Some(kind) => match kind.to_ascii_lowercase().as_str() {
            "file" => FileTreeNodeKind::File,
            "dir" | "directory" => FileTreeNodeKind::Directory,
            other => return Err(format!("unknown kind {other}")),
        },
        None => {
            if children.is_empty() {
                FileTreeNodeKind::File
            } else {
                FileTreeNodeKind::Directory
            }
        }
    };

    let git_status = match map
        .get("git_status")
        .or_else(|| map.get("gitStatus"))
        .and_then(ComponentValue::as_str)
    {
        Some(value) => {
            Some(parse_git_status(value).ok_or_else(|| format!("unknown git status {value}"))?)
        }
        None => None,
    };

    Ok(FileTreeNode {
        id: FileTreeNodeId::new(id),
        name,
        kind,
        children,
        is_expanded,
        git_status,
        children_loaded: true,
    })
}

pub fn file_tree_schema() -> ComponentSchema {
    component_schema::<FileTree>("FileTree")
        .with_event(EventMeta::new("select").with_payload(ValueType::U64))
        .with_event(EventMeta::new("rename").with_payload(ValueType::Map))
        .with_event(EventMeta::new("delete").with_payload(ValueType::Map))
        .allow_children(false)
}

fn register_file_tree_extension(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    register_file_tree(registry, callbacks);
}

/// 将 `FileTree` 组件注册到 `atto-ui` 的全局动态组件注册表中。
///
/// 这使得 `Window::new_dynamic` / `Desktop::add_dynamic_window` 等基础框架入口
/// 在构建动态树时能够识别 `{"type": "FileTree", ...}`。
///
/// 返回：
/// - `true`：本次注册成功
/// - `false`：已注册过（幂等）
pub fn register_runtime_components() -> bool {
    register_registry_extension("atto-ui-file-tree", register_file_tree_extension)
}

pub fn register_file_tree(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = file_tree_schema();
    registry.register(schema, move |spec, _registry| {
        let title = prop_string(spec, "title")?.unwrap_or_default();
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let border = prop_bool(spec, "border")?.unwrap_or(true);
        let height = prop_u16(spec, "height")?;
        let roots = prop_nodes(spec, "nodes")?
            .or_else(|| prop_nodes(spec, "roots").ok().flatten())
            .unwrap_or_default();
        let selection = prop_u64(spec, "selection")?;

        let selection = selection.map(FileTreeNodeId::new);
        let mut tree = FileTree::new(title, Binding::new(roots), Binding::new(selection))
            .enabled(enabled)
            .border(border);
        if let Some(value) = spec.props.get("icons") {
            let glyphs = parse_file_tree_glyphs(value)
                .map_err(|reason| invalid_prop(spec, "icons", &reason, value))?;
            tree = tree.glyphs(glyphs);
        }
        if let Some(height) = height {
            tree = tree.height(height);
        }
        if let Some(cb) = event_handle(spec, "select", callbacks.clone()) {
            tree = tree.on_select_callback(cb);
        }
        if let Some(cb) = event_handle(spec, "rename", callbacks.clone()) {
            tree = tree.on_rename_callback(cb);
        }
        if let Some(cb) = event_handle(spec, "delete", callbacks.clone()) {
            tree = tree.on_delete_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(tree)))
    });
}

/// Parses a single icon value from the runtime/binding: either a bare glyph
/// string, or a map `{ "glyph": "...", "color": "<name|#rrggbb|index>" }`.
fn parse_file_tree_icon(value: &ComponentValue) -> Result<FileTreeIcon, String> {
    match value {
        ComponentValue::Null => Ok(FileTreeIcon::default()),
        ComponentValue::String(glyph) => Ok(FileTreeIcon::new(glyph.clone())),
        ComponentValue::Map(map) => {
            let glyph = map
                .get("glyph")
                .or_else(|| map.get("text"))
                .and_then(ComponentValue::as_str)
                .unwrap_or_default()
                .to_string();
            let color = match map.get("color").and_then(ComponentValue::as_str) {
                Some(raw) => {
                    Some(Color::from_str(raw).map_err(|_| format!("invalid color: {raw}"))?)
                }
                None => None,
            };
            Ok(FileTreeIcon { glyph, color })
        }
        other => Err(format!("expected icon string or map, got {other:?}")),
    }
}

/// Parses the `icons` property: a map of lowercased file extension → icon
/// (string or `{glyph,color}`). An empty/null map means no file-type icons.
fn parse_file_tree_glyphs(value: &ComponentValue) -> Result<FileTreeGlyphs, String> {
    let mut glyphs = FileTreeGlyphs::default();
    match value {
        ComponentValue::Null => {}
        ComponentValue::Map(map) => {
            for (ext, icon_value) in map {
                glyphs.set_extension(ext.clone(), parse_file_tree_icon(icon_value)?);
            }
        }
        other => return Err(format!("expected map of extension to icon, got {other:?}")),
    }
    Ok(glyphs)
}

fn prop_nodes(spec: &ComponentSpec, name: &str) -> Result<Option<Vec<FileTreeNode>>, TreeError> {
    match spec.props.get(name) {
        Some(value) => parse_nodes_value(value)
            .map(Some)
            .map_err(|reason| invalid_prop(spec, name, &reason, value)),
        None => Ok(None),
    }
}

fn build_scroll_container(bindings: Arc<RwLock<FileTreeBindings>>) -> ScrollContainer {
    // Padding is ZERO: the FileTree itself computes the content rect (border inset
    // plus any borderless scrollbar reservation) and passes it to the container.
    ScrollContainer::new(Box::new(FileTreeContent::new(bindings)))
        .with_padding(EdgeInsets::ZERO)
        .with_scroll_config(ScrollConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tree(expanded: bool) -> Vec<FileTreeNode> {
        vec![
            FileTreeNode::dir(
                1,
                "src",
                vec![
                    FileTreeNode::file(2, "main.rs"),
                    FileTreeNode::file(3, "lib.rs"),
                ],
            )
            .with_expanded(expanded),
            FileTreeNode::dir(4, "assets", vec![FileTreeNode::file(5, "logo.png")])
                .with_expanded(false),
            FileTreeNode::file(6, "README.md"),
        ]
    }

    #[test]
    fn visible_entries_respect_expand_state() {
        let glyphs = FileTreeGlyphs::default();
        let roots = sample_tree(false);
        let entries = FileTreeContent::new(Arc::new(RwLock::new(FileTreeBindings {
            title: "".into(),
            roots: roots.clone().into(),
            selection: None.into(),
            selections: BTreeSet::new().into(),
            selection_anchor: None,
            enabled: true.into(),
            border: true.into(),
            height: 10.into(),
            filter: None,
            glyphs: Arc::new(glyphs.clone()),
            on_select: None,
            on_rename: None,
            on_delete: None,
            inline_edit: None,
            pending_inline_commit: None,
            defer_inline_commits: false,
        })))
        .build_visible_entries(&roots, None, &glyphs);
        assert!(entries.iter().all(|e| e.name != "main.rs"));

        let roots = sample_tree(true);
        let entries = FileTreeContent::new(Arc::new(RwLock::new(FileTreeBindings {
            title: "".into(),
            roots: roots.clone().into(),
            selection: None.into(),
            selections: BTreeSet::new().into(),
            selection_anchor: None,
            enabled: true.into(),
            border: true.into(),
            height: 10.into(),
            filter: None,
            glyphs: Arc::new(glyphs.clone()),
            on_select: None,
            on_rename: None,
            on_delete: None,
            inline_edit: None,
            pending_inline_commit: None,
            defer_inline_commits: false,
        })))
        .build_visible_entries(&roots, None, &glyphs);
        assert!(entries.iter().any(|e| e.name == "main.rs"));
    }

    #[test]
    fn filter_excludes_subtree() {
        let glyphs = FileTreeGlyphs::default();
        let roots = vec![
            FileTreeNode::dir(1, ".git", vec![FileTreeNode::file(2, "config")]).with_expanded(true),
            FileTreeNode::file(3, "README.md"),
        ];
        let filter = Arc::new(|node: &FileTreeNode| !node.name.starts_with('.'));
        let entries = FileTreeContent::new(Arc::new(RwLock::new(FileTreeBindings {
            title: "".into(),
            roots: roots.clone().into(),
            selection: None.into(),
            selections: BTreeSet::new().into(),
            selection_anchor: None,
            enabled: true.into(),
            border: true.into(),
            height: 10.into(),
            filter: Some(filter.clone()),
            glyphs: Arc::new(glyphs.clone()),
            on_select: None,
            on_rename: None,
            on_delete: None,
            inline_edit: None,
            pending_inline_commit: None,
            defer_inline_commits: false,
        })))
        .build_visible_entries(&roots, Some(filter.as_ref()), &glyphs);
        assert!(entries.iter().all(|e| !e.name.contains(".git")));
        assert!(entries.iter().any(|e| e.name == "README.md"));
    }

    #[test]
    fn build_prefix_marks_unloaded_directory_as_expandable() {
        let loaded_empty = FileTreeNode::dir(1, "empty", Vec::new());
        let unloaded = FileTreeNode::dir(2, "lazy", Vec::new()).with_children_loaded(false);

        let loaded_prefix = build_prefix(&loaded_empty, 0, &[], true);
        let unloaded_prefix = build_prefix(&unloaded, 0, &[], true);

        // A loaded but empty directory shows no expand indicator.
        assert!(!loaded_prefix.contains('▶'));
        assert!(!loaded_prefix.contains('▼'));
        // An unloaded directory shows the collapsed ▶ indicator so it can be expanded.
        assert!(unloaded_prefix.contains('▶'));
    }

    #[test]
    fn glyphs_use_extension_mapping() {
        let mut glyphs = FileTreeGlyphs::default();
        glyphs.set_extension("rs", "rs");
        let node = FileTreeNode::file(1, "main.rs");
        let icon = glyphs.icon_for(&node, false);
        assert_eq!(icon.glyph, "rs");
        assert_eq!(icon.color, None);
    }

    #[test]
    fn icons_carry_optional_color() {
        let glyphs = FileTreeGlyphs::default()
            .with_extension("rs", FileTreeIcon::colored("\u{e7a8}", Color::Red));
        let node = FileTreeNode::file(1, "main.rs");
        let icon = glyphs.icon_for(&node, false);
        assert_eq!(icon.glyph, "\u{e7a8}");
        assert_eq!(icon.color, Some(Color::Red));

        // Unknown extensions fall back to the (empty) default icon.
        let other = FileTreeNode::file(2, "notes.txt");
        assert!(glyphs.icon_for(&other, false).is_empty());
    }

    #[test]
    fn mouse_click_selects_correct_row_in_both_coordinate_spaces() {
        use atto_ui::composable::{EventHandling, TabMode};
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let theme = atto_ui::theme::Theme::dark();
        // Off-origin area so absolute vs local coordinate spaces differ.
        let area = Rect::new(0, 5, 30, 12);

        // sample_tree(true) visible rows (border on): src(1), main.rs(2), lib.rs(3), ...
        // Inside the border the first content row is one below the top; main.rs is
        // the second content row.
        let select_main_rs = |space: MouseCoordinateSpace| -> Option<FileTreeNodeId> {
            let selection = Binding::new(None);
            let mut tree = FileTree::new("Files", sample_tree(true), selection.clone());
            let ctx = ComponentContext {
                theme: &theme,
                window_id: atto_ui::wm::WindowId::from_raw(1),
                is_focused: true,
                scrollbar_host: ScrollbarHost::Component,
                tab_mode: TabMode::Cycle,
                mouse_coordinate_space: space,
                drag: None,
            };
            let backend = TestBackend::new(30, 20);
            let mut terminal = Terminal::new(backend).expect("terminal");
            terminal.draw(|f| tree.draw(f, area, ctx)).expect("draw");

            let row = match space {
                // Absolute: border row at area.y, first entry at area.y+1, main.rs at area.y+2.
                MouseCoordinateSpace::Absolute => area.y + 2,
                // Local: coordinates are relative to the widget area (0-based).
                MouseCoordinateSpace::Local => 2,
            };
            let event = Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: area.x + 4,
                row,
                modifiers: KeyModifiers::NONE,
            });
            EventHandling::handle_event(&mut tree, &event, ctx);
            selection.get()
        };

        assert_eq!(
            select_main_rs(MouseCoordinateSpace::Absolute),
            Some(FileTreeNodeId::new(2)),
        );
        assert_eq!(
            select_main_rs(MouseCoordinateSpace::Local),
            Some(FileTreeNodeId::new(2)),
        );
    }

    #[test]
    fn visible_range_selection_uses_only_visible_rows() {
        let glyphs = FileTreeGlyphs::default();
        let roots = sample_tree(false);
        let content = FileTreeContent::new(Arc::new(RwLock::new(FileTreeBindings {
            title: "".into(),
            roots: roots.clone().into(),
            selection: None.into(),
            selections: BTreeSet::new().into(),
            selection_anchor: None,
            enabled: true.into(),
            border: true.into(),
            height: 10.into(),
            filter: None,
            glyphs: Arc::new(glyphs.clone()),
            on_select: None,
            on_rename: None,
            on_delete: None,
            inline_edit: None,
            pending_inline_commit: None,
            defer_inline_commits: false,
        })));
        let entries = content.build_visible_entries(&roots, None, &glyphs);
        let readme_idx = entries
            .iter()
            .position(|entry| entry.id == FileTreeNodeId::new(6))
            .expect("README visible");

        let selected = visible_range_selection(FileTreeNodeId::new(1), readme_idx, &entries);

        assert_eq!(
            selected,
            BTreeSet::from([
                FileTreeNodeId::new(1),
                FileTreeNodeId::new(4),
                FileTreeNodeId::new(6),
            ])
        );
        assert!(!selected.contains(&FileTreeNodeId::new(2)));
        assert!(!selected.contains(&FileTreeNodeId::new(5)));
    }

    #[test]
    fn git_status_badges_skip_clean_nodes() {
        let glyphs = FileTreeGlyphs::default();
        let roots = vec![
            FileTreeNode::file(1, "changed.rs").with_git_status(FileTreeGitStatus::Modified),
            FileTreeNode::file(2, "clean.rs").with_git_status(FileTreeGitStatus::Clean),
        ];
        let content = FileTreeContent::new(Arc::new(RwLock::new(FileTreeBindings {
            title: "".into(),
            roots: roots.clone().into(),
            selection: None.into(),
            selections: BTreeSet::new().into(),
            selection_anchor: None,
            enabled: true.into(),
            border: true.into(),
            height: 10.into(),
            filter: None,
            glyphs: Arc::new(glyphs.clone()),
            on_select: None,
            on_rename: None,
            on_delete: None,
            inline_edit: None,
            pending_inline_commit: None,
            defer_inline_commits: false,
        })));
        let entries = content.build_visible_entries(&roots, None, &glyphs);

        assert!(content.line_text(&entries[0]).contains("M changed.rs"));
        assert!(content.line_text(&entries[1]).contains("clean.rs"));
        assert!(!content.line_text(&entries[1]).contains("C clean.rs"));
    }

    #[test]
    fn runtime_schema_keeps_legacy_file_tree_properties() {
        let properties = file_tree_schema()
            .properties
            .into_iter()
            .map(|prop| prop.name)
            .collect::<Vec<_>>();

        assert_eq!(
            properties,
            vec![
                "title",
                "enabled",
                "border",
                "height",
                "selection",
                "nodes",
                "icons"
            ]
        );
    }

    #[test]
    fn parse_icons_prop_builds_glyphs_with_color() {
        let mut rs = BTreeMap::new();
        rs.insert(
            "glyph".to_string(),
            ComponentValue::String("\u{e7a8}".into()),
        );
        rs.insert(
            "color".to_string(),
            ComponentValue::String("#ff8800".into()),
        );
        let mut icons = BTreeMap::new();
        icons.insert("rs".to_string(), ComponentValue::Map(rs));
        // Bare-string form (no color).
        icons.insert("md".to_string(), ComponentValue::String("M".into()));

        let glyphs = parse_file_tree_glyphs(&ComponentValue::Map(icons)).expect("parse icons");

        let rs_icon = glyphs.icon_for(&FileTreeNode::file(1, "main.rs"), false);
        assert_eq!(rs_icon.glyph, "\u{e7a8}");
        assert_eq!(rs_icon.color, Some(Color::Rgb(0xff, 0x88, 0x00)));

        let md_icon = glyphs.icon_for(&FileTreeNode::file(2, "README.md"), false);
        assert_eq!(md_icon.glyph, "M");
        assert_eq!(md_icon.color, None);
    }

    #[test]
    fn set_icons_property_updates_provider_and_null_clears() {
        let mut tree = FileTree::new("Files", sample_tree(true), Binding::new(None));

        let mut entry = BTreeMap::new();
        entry.insert("glyph".to_string(), ComponentValue::String("R".into()));
        entry.insert("color".to_string(), ComponentValue::String("red".into()));
        let mut icons = BTreeMap::new();
        icons.insert("rs".to_string(), ComponentValue::Map(entry));

        tree.set_property("icons", ComponentValue::Map(icons))
            .expect("icons accepts a map");
        let icon = tree
            .bindings
            .read()
            .glyphs
            .icon_for(&FileTreeNode::file(1, "main.rs"), false);
        assert_eq!(icon.glyph, "R");
        assert_eq!(icon.color, Some(Color::Red));

        // Null resets to the empty default mapping.
        tree.set_property("icons", ComponentValue::Null)
            .expect("icons accepts null");
        assert!(
            tree.bindings
                .read()
                .glyphs
                .icon_for(&FileTreeNode::file(1, "main.rs"), false)
                .is_empty()
        );
    }

    #[test]
    fn border_property_round_trips() {
        let mut tree = FileTree::new("Files", sample_tree(true), Binding::new(None));
        // Defaults to drawing its own border.
        assert_eq!(
            tree.get_property("border"),
            Some(ComponentValue::Bool(true))
        );

        tree.set_property("border", ComponentValue::Bool(false))
            .expect("border accepts bool");
        assert_eq!(
            tree.get_property("border"),
            Some(ComponentValue::Bool(false))
        );

        // Builder form also works.
        let borderless =
            FileTree::new("Files", sample_tree(true), Binding::new(None)).border(false);
        assert_eq!(
            borderless.get_property("border"),
            Some(ComponentValue::Bool(false))
        );
    }

    #[test]
    fn runtime_selection_property_resets_multiselect_state() {
        let roots = sample_tree(true);
        let mut tree = FileTree::new("Files", roots, Binding::new(None));

        assert_eq!(
            tree.selected_ids(),
            BTreeSet::from([FileTreeNodeId::new(1)])
        );

        tree.set_property("selection", ComponentValue::U64(6))
            .expect("selection property should accept u64");

        assert_eq!(tree.selected(), Some(FileTreeNodeId::new(6)));
        assert_eq!(
            tree.selected_ids(),
            BTreeSet::from([FileTreeNodeId::new(6)])
        );

        tree.set_property("selection", ComponentValue::Null)
            .expect("selection property should accept null");

        assert_eq!(tree.selected(), None);
        assert!(tree.selected_ids().is_empty());
    }

    #[test]
    fn drag_source_at_emits_selected_node_ids_payload() {
        let theme = atto_ui::theme::Theme::dark();
        let selection = Binding::new(Some(FileTreeNodeId::new(2)));
        let selections = Binding::new(BTreeSet::from([
            FileTreeNodeId::new(2),
            FileTreeNodeId::new(3),
        ]));
        let mut tree =
            FileTree::new_with_selections("Files", sample_tree(true), selection, selections);
        tree.last_area = Some(Rect::new(0, 0, 40, 12));

        let source = atto_ui::composable::DragAndDrop::drag_source_at(
            &tree,
            2,
            2,
            ComponentContext {
                theme: &theme,
                window_id: atto_ui::wm::WindowId::from_raw(1),
                is_focused: true,
                scrollbar_host: ScrollbarHost::Component,
                tab_mode: atto_ui::composable::TabMode::Cycle,
                mouse_coordinate_space: MouseCoordinateSpace::Absolute,
                drag: None,
            },
        )
        .expect("drag source");

        assert_eq!(source.operation, DragOperation::Move);
        assert_eq!(source.ghost.as_deref(), Some("2 items"));
        assert_eq!(
            source.payload,
            DragPayload::Custom {
                ty: FILE_TREE_NODE_IDS_DRAG_TYPE,
                data: b"2,3".to_vec()
            }
        );
    }

    #[test]
    fn remove_node_by_id_removes_deep_child() {
        let mut roots = sample_tree(true);
        let removed = remove_node_by_id(&mut roots, FileTreeNodeId::new(3));
        assert!(removed.is_some());
        assert!(
            find_node_mut(&mut roots, FileTreeNodeId::new(3)).is_none(),
            "child should be removed"
        );
    }

    #[test]
    fn toggle_directory_updates_expanded_state() {
        let roots = sample_tree(false);
        let bindings = Arc::new(RwLock::new(FileTreeBindings {
            title: "".into(),
            roots: roots.clone().into(),
            selection: None.into(),
            selections: BTreeSet::new().into(),
            selection_anchor: None,
            enabled: true.into(),
            border: true.into(),
            height: 10.into(),
            filter: None,
            glyphs: Arc::new(FileTreeGlyphs::default()),
            on_select: None,
            on_rename: None,
            on_delete: None,
            inline_edit: None,
            pending_inline_commit: None,
            defer_inline_commits: false,
        }));
        let mut content = FileTreeContent::new(bindings.clone());
        let changed = content.toggle_directory(FileTreeNodeId::new(1));
        assert!(changed);
        let updated = bindings.read().roots.get();
        assert!(updated[0].is_expanded);
    }

    #[test]
    fn delete_selected_removes_root() {
        let roots = sample_tree(true);
        let bindings = Arc::new(RwLock::new(FileTreeBindings {
            title: "".into(),
            roots: roots.clone().into(),
            selection: None.into(),
            selections: BTreeSet::new().into(),
            selection_anchor: None,
            enabled: true.into(),
            border: true.into(),
            height: 10.into(),
            filter: None,
            glyphs: Arc::new(FileTreeGlyphs::default()),
            on_select: None,
            on_rename: None,
            on_delete: None,
            inline_edit: None,
            pending_inline_commit: None,
            defer_inline_commits: false,
        }));
        let mut content = FileTreeContent::new(bindings.clone());
        let removed = content.delete_selected(FileTreeNodeId::new(6));
        assert!(removed.is_some());
        let updated = bindings.read().roots.get();
        assert!(updated.iter().all(|node| node.id != FileTreeNodeId::new(6)));
    }

    #[test]
    fn rename_updates_node_name() {
        let roots = sample_tree(true);
        let bindings = Arc::new(RwLock::new(FileTreeBindings {
            title: "".into(),
            roots: roots.clone().into(),
            selection: None.into(),
            selections: BTreeSet::new().into(),
            selection_anchor: None,
            enabled: true.into(),
            border: true.into(),
            height: 10.into(),
            filter: None,
            glyphs: Arc::new(FileTreeGlyphs::default()),
            on_select: None,
            on_rename: None,
            on_delete: None,
            inline_edit: None,
            pending_inline_commit: None,
            defer_inline_commits: false,
        }));
        let mut content = FileTreeContent::new(bindings.clone());
        content.start_rename(FileTreeNodeId::new(6), None, "README.md");
        {
            let mut guard = bindings.write();
            if let Some(edit) = &mut guard.inline_edit {
                edit.text.set_text("README_NEW.md");
            }
        }
        let result = content.commit_inline_edit();
        assert!(result.is_some());
        let updated = bindings.read().roots.get();
        let readme = updated
            .iter()
            .find(|node| node.id == FileTreeNodeId::new(6));
        assert_eq!(readme.unwrap().name, "README_NEW.md");
    }
}
