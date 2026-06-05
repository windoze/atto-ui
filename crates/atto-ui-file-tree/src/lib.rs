#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::Arc;

use atto_ui::composable::{
    Component, ComponentContext, EdgeInsets, EventResult, ScrollConfig, ScrollContainer,
    ScrollContainerHost, ScrollContent, ScrollContentContext, ScrollOffset, ScrollbarDrag,
    ScrollbarHost, Scrollbars, draw_scrollbars, handle_scrollbar_mouse_event,
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
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileTreeNode {
    pub id: FileTreeNodeId,
    pub name: String,
    pub kind: FileTreeNodeKind,
    pub children: Vec<FileTreeNode>,
    pub is_expanded: bool,
}

impl FileTreeNode {
    pub fn file(id: impl Into<FileTreeNodeId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: FileTreeNodeKind::File,
            children: Vec::new(),
            is_expanded: false,
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
        }
    }

    pub fn with_expanded(mut self, expanded: bool) -> Self {
        self.is_expanded = expanded;
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

pub trait FileTreeGlyphProvider: Send + Sync {
    fn glyph_for(&self, node: &FileTreeNode, is_expanded: bool) -> String;
}

#[derive(Clone, Debug)]
pub struct FileTreeGlyphs {
    pub directory_closed: String,
    pub directory_open: String,
    pub file: String,
    pub by_extension: BTreeMap<String, String>,
}

impl Default for FileTreeGlyphs {
    fn default() -> Self {
        Self {
            directory_closed: "dir".to_string(),
            directory_open: "dir".to_string(),
            file: "file".to_string(),
            by_extension: BTreeMap::new(),
        }
    }
}

impl FileTreeGlyphs {
    pub fn with_extension(mut self, ext: impl Into<String>, glyph: impl Into<String>) -> Self {
        self.set_extension(ext, glyph);
        self
    }

    pub fn set_extension(&mut self, ext: impl Into<String>, glyph: impl Into<String>) {
        let key = ext.into().to_ascii_lowercase();
        self.by_extension.insert(key, glyph.into());
    }
}

impl FileTreeGlyphProvider for FileTreeGlyphs {
    fn glyph_for(&self, node: &FileTreeNode, is_expanded: bool) -> String {
        if node.is_dir() {
            if is_expanded {
                return self.directory_open.clone();
            }
            return self.directory_closed.clone();
        }
        if let Some(ext) = node.extension()
            && let Some(glyph) = self.by_extension.get(&ext.to_ascii_lowercase())
        {
            return glyph.clone();
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
    enabled: Binding<bool>,
    height: Binding<u16>,
    filter: Option<Arc<dyn FileTreeFilter>>,
    glyphs: Arc<dyn FileTreeGlyphProvider>,
    on_select: Option<CallbackHandle>,
    on_rename: Option<CallbackHandle>,
    on_delete: Option<CallbackHandle>,
}

impl FileTree {
    pub fn new(
        title: impl Into<Binding<String>>,
        roots: impl Into<Binding<Vec<FileTreeNode>>>,
        selection: Binding<Option<FileTreeNodeId>>,
    ) -> Self {
        let roots = roots.into();
        if selection.get().is_none() {
            let nodes = roots.get();
            if let Some(first) = nodes.first() {
                selection.set(Some(first.id));
            }
        }
        let bindings = Arc::new(RwLock::new(FileTreeBindings {
            title: title.into(),
            roots,
            selection,
            enabled: true.into(),
            height: 10.into(),
            filter: None,
            glyphs: Arc::new(FileTreeGlyphs::default()),
            on_select: None,
            on_rename: None,
            on_delete: None,
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

    pub fn selected(&self) -> Option<FileTreeNodeId> {
        self.bindings.read().selection.get()
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
            PropertyMeta::new("height", ValueType::U64),
            PropertyMeta::new("selection", ValueType::U64),
            PropertyMeta::new("nodes", ValueType::List),
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

impl Component for FileTree {
    fn property_names(&self) -> Vec<&'static str> {
        vec!["title", "enabled", "height", "selection", "nodes"]
    }

    fn get_property(&self, name: &str) -> Option<ComponentValue> {
        let bindings = self.bindings.read();
        match name {
            "title" => Some(ComponentValue::String(bindings.title.get())),
            "enabled" => Some(ComponentValue::Bool(bindings.enabled.get())),
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
        let bindings = self.bindings.read();
        match name {
            "title" => {
                let v = ComponentValueCodec::from_component_value(value, name)?;
                bindings.title.set(v);
                Ok(())
            }
            "enabled" => {
                let v = ComponentValueCodec::from_component_value(value, name)?;
                bindings.enabled.set(v);
                Ok(())
            }
            "height" => {
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
                bindings.selection.set(next);
                Ok(())
            }
            "nodes" | "roots" => {
                let nodes = parse_nodes_value(&value)
                    .map_err(|_| ComponentError::invalid_value(name, "list"))?;
                bindings.roots.set(nodes);
                Ok(())
            }
            _ => Err(ComponentError::unsupported_property(name)),
        }
    }

    fn min_width(&self) -> u16 {
        self.min_size.0
    }

    fn min_height(&self) -> u16 {
        self.min_size.1
    }

    fn is_focusable(&self) -> bool {
        self.bindings.read().enabled.get()
    }

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

    fn desired_height(&self) -> Option<u16> {
        let height = self.bindings.read().height.get();
        Some(height.max(self.min_size.1))
    }

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
            && let Some((local_x, local_y)) = mouse_coords_local_to_area(area, *m)
        {
            let abs_event = MouseEvent {
                column: area.x.saturating_add(local_x),
                row: area.y.saturating_add(local_y),
                ..*m
            };
            if let Some(new_scroll) = self.handle_border_scrollbar_event(abs_event, area) {
                self.scroll.set_scroll_offset(new_scroll.x, new_scroll.y);
                return EventResult::consumed();
            }
        }

        let body_ctx = ComponentContext {
            scrollbar_host: ScrollbarHost::Window,
            ..ctx
        };
        self.scroll.handle_event(event, body_ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);

        let bindings = self.bindings.read();
        let enabled = bindings.enabled.get();
        let style = if !enabled {
            ctx.theme.widget.disabled
        } else if ctx.is_focused {
            ctx.theme.widget.focused
        } else {
            ctx.theme.widget.normal
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(ctx.theme.border_set(false))
            .title(bindings.title.get())
            .style(style);
        frame.render_widget(block, area);
        drop(bindings);

        let body_ctx = ComponentContext {
            scrollbar_host: ScrollbarHost::Window,
            ..ctx
        };
        self.scroll.draw(frame, area, body_ctx);

        if matches!(ctx.scrollbar_host, ScrollbarHost::Component) {
            self.draw_border_scrollbar(frame, area, ctx);
        } else {
            self.scrollbar_drag = None;
        }
    }
}

impl FileTree {
    fn border_scrollbars(&self, area: Rect) -> Option<Scrollbars> {
        if area.width < 3 || area.height < 3 {
            return None;
        }

        let cfg = self.scroll.scroll_config();
        let content_size = self.scroll.content_size();
        let viewport_size = self.scroll.viewport_size();

        let show_v = should_show_scrollbar(cfg.vertical_scrollbar, content_size.1, viewport_size.1);
        let show_h =
            should_show_scrollbar(cfg.horizontal_scrollbar, content_size.0, viewport_size.0);
        if !show_v && !show_h {
            return None;
        }

        let content_local = Rect {
            x: 1,
            y: 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
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
        ctx: ComponentContext<'_>,
    ) {
        let Some(scrollbars) = self.border_scrollbars(area) else {
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

    fn handle_border_scrollbar_event(&mut self, m: MouseEvent, area: Rect) -> Option<ScrollOffset> {
        let Some(scrollbars) = self.border_scrollbars(area) else {
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

struct FileTreeContent {
    bindings: Arc<RwLock<FileTreeBindings>>,
    state: ListState,
    rename: Option<RenameState>,
    last_selection: Option<FileTreeNodeId>,
}

struct RenameState {
    id: FileTreeNodeId,
    buffer: TextBuffer,
    replace_on_input: bool,
}

#[derive(Clone, Debug)]
struct VisibleEntry {
    id: FileTreeNodeId,
    parent_id: Option<FileTreeNodeId>,
    depth: usize,
    kind: FileTreeNodeKind,
    is_expanded: bool,
    name: String,
    prefix: String,
}

impl VisibleEntry {
    fn is_dir(&self) -> bool {
        matches!(self.kind, FileTreeNodeKind::Directory)
    }
}

impl FileTreeContent {
    fn new(bindings: Arc<RwLock<FileTreeBindings>>) -> Self {
        Self {
            bindings,
            state: ListState::default(),
            rename: None,
            last_selection: None,
        }
    }

    fn selection_binding(&self) -> Binding<Option<FileTreeNodeId>> {
        self.bindings.read().selection.clone()
    }

    fn roots_binding(&self) -> Binding<Vec<FileTreeNode>> {
        self.bindings.read().roots.clone()
    }

    fn bindings_snapshot(&self) -> FileTreeBindingsSnapshot {
        let bindings = self.bindings.read();
        FileTreeBindingsSnapshot {
            title: bindings.title.get(),
            roots: bindings.roots.get(),
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
        let selection = selection_binding.get();
        if visible.is_empty() {
            if selection.is_some() {
                selection_binding.set(None);
            }
            self.rename = None;
            self.last_selection = None;
            return None;
        }

        if let Some(id) = selection
            && let Some(idx) = visible.iter().position(|entry| entry.id == id)
        {
            return Some(idx);
        }

        let next_id = visible[0].id;
        selection_binding.set(Some(next_id));
        Some(0)
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

    fn maybe_reset_rename(&mut self, selection: Option<FileTreeNodeId>) {
        if let Some(rename) = &self.rename
            && selection != Some(rename.id)
        {
            self.rename = None;
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
        out
    }

    fn line_text(&self, entry: &VisibleEntry) -> String {
        let mut line = String::new();
        line.push_str(&entry.prefix);
        if let Some(rename) = &self.rename
            && rename.id == entry.id
        {
            let text = rename.buffer.text();
            let cursor = rename.buffer.cursor_byte_index().min(text.len());
            let (left, right) = text.split_at(cursor);
            line.push_str(left);
            line.push('|');
            line.push_str(right);
            return line;
        }
        line.push_str(&entry.name);
        line
    }

    fn select_index(
        &mut self,
        idx: usize,
        visible: &[VisibleEntry],
        host: &mut ScrollContainerHost,
        cb: Option<&CallbackHandle>,
    ) -> EventResult {
        if let Some(entry) = visible.get(idx) {
            let selection_binding = self.selection_binding();
            selection_binding.set(Some(entry.id));
            self.ensure_selection_visible(idx, host);
            self.last_selection = Some(entry.id);
            self.maybe_reset_rename(Some(entry.id));
            if let Some(cb) = cb {
                cb.emit_with(Some(ComponentValue::U64(entry.id.value())));
            }
            return EventResult::changed();
        }
        EventResult::ignored()
    }

    fn move_selection(
        &mut self,
        delta: i32,
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
        self.select_index(next_idx, visible, host, cb)
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

    fn start_rename(&mut self, id: FileTreeNodeId, name: &str) {
        self.rename = Some(RenameState {
            id,
            buffer: TextBuffer::with_text(name),
            replace_on_input: true,
        });
    }

    fn commit_rename(&mut self, id: FileTreeNodeId) -> Option<(String, String, FileTreeNodeKind)> {
        let Some(rename) = &self.rename else {
            return None;
        };
        if rename.id != id {
            return None;
        }
        let new_name = rename.buffer.text().to_string();
        if new_name.trim().is_empty() {
            self.rename = None;
            return None;
        }
        let roots = self.roots_binding();
        let mut old_name = None;
        let mut kind = None;
        roots.update(|nodes| {
            if let Some(node) = find_node_mut(nodes, id) {
                old_name = Some(node.name.clone());
                kind = Some(node.kind);
                node.name = new_name.clone();
            }
        });
        self.rename = None;
        old_name.map(|old| (old, new_name, kind.unwrap_or(FileTreeNodeKind::File)))
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
        self.maybe_reset_rename(selection_now);
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
        self.maybe_reset_rename(selection_now);

        if self.rename.is_some() {
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
                        self.rename = None;
                        return EventResult::consumed();
                    }
                    KeyCode::Enter => {
                        let rename_id = self.rename.as_ref().map(|r| r.id);
                        if let Some(rename_id) = rename_id
                            && let Some((old_name, new_name, kind)) = self.commit_rename(rename_id)
                            && old_name != new_name
                        {
                            if let Some(cb) = &snapshot.on_rename {
                                cb.emit_with(Some(rename_payload(
                                    rename_id, kind, &old_name, &new_name,
                                )));
                            }
                            return EventResult::changed();
                        }
                        return EventResult::consumed();
                    }
                    KeyCode::Backspace => {
                        if let Some(rename) = &mut self.rename {
                            rename.replace_on_input = false;
                            rename.buffer.backspace();
                        }
                        return EventResult::changed();
                    }
                    KeyCode::Delete => {
                        if let Some(rename) = &mut self.rename {
                            rename.replace_on_input = false;
                            rename.buffer.delete();
                        }
                        return EventResult::changed();
                    }
                    KeyCode::Left => {
                        if let Some(rename) = &mut self.rename {
                            rename.replace_on_input = false;
                            rename.buffer.move_left();
                        }
                        return EventResult::consumed();
                    }
                    KeyCode::Right => {
                        if let Some(rename) = &mut self.rename {
                            rename.replace_on_input = false;
                            rename.buffer.move_right();
                        }
                        return EventResult::consumed();
                    }
                    KeyCode::Home => {
                        if let Some(rename) = &mut self.rename {
                            rename.replace_on_input = false;
                            rename.buffer.move_home();
                        }
                        return EventResult::consumed();
                    }
                    KeyCode::End => {
                        if let Some(rename) = &mut self.rename {
                            rename.replace_on_input = false;
                            rename.buffer.move_end();
                        }
                        return EventResult::consumed();
                    }
                    KeyCode::Char(ch) if !modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(rename) = &mut self.rename {
                            if rename.replace_on_input {
                                rename.buffer.set_text("");
                                rename.replace_on_input = false;
                            }
                            rename.buffer.insert_char(*ch);
                        }
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
                    let res = self.select_index(idx, &entries, host, snapshot.on_select.as_ref());
                    if entry.is_dir() && self.toggle_directory(entry.id) {
                        return EventResult::changed();
                    }
                    return res;
                }
                EventResult::ignored()
            }
            Event::Key(KeyEvent { code, kind, .. }) => {
                if matches!(kind, KeyEventKind::Release) {
                    return EventResult::ignored();
                }

                match code {
                    KeyCode::Up => {
                        if entries.is_empty() {
                            return EventResult::ignored();
                        }
                        self.move_selection(-1, &entries, host, snapshot.on_select.as_ref())
                    }
                    KeyCode::Down => {
                        if entries.is_empty() {
                            return EventResult::ignored();
                        }
                        self.move_selection(1, &entries, host, snapshot.on_select.as_ref())
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
                        self.start_rename(entry.id, &entry.name);
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
        let highlight_style = if enabled {
            ctx.component.theme.selection
        } else {
            ctx.component
                .theme
                .selection
                .patch(ctx.component.theme.widget.disabled)
        };

        let filter = snapshot.filter.as_deref();
        let entries = self.build_visible_entries(&snapshot.roots, filter, snapshot.glyphs.as_ref());
        let selection = self.normalize_selection(&entries);

        let scroll = ctx.info.scroll_offset;
        let viewport_w = area.width;
        let items: Vec<ListItem> = entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let full_line = self.line_text(entry);
                let visible = slice_by_display_width(&full_line, scroll.x, viewport_w);
                let item = ListItem::new(Line::from(Span::styled(visible, style)));
                if selection.is_some_and(|sel| sel == idx) {
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
        let prefix = build_prefix(node, depth, ancestors_last, is_last, glyphs);
        out.push(VisibleEntry {
            id: node.id,
            parent_id,
            depth,
            kind: node.kind,
            is_expanded: node.is_expanded,
            name: node.name.clone(),
            prefix,
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

fn build_prefix(
    node: &FileTreeNode,
    depth: usize,
    ancestors_last: &[bool],
    is_last: bool,
    glyphs: &dyn FileTreeGlyphProvider,
) -> String {
    let mut prefix = String::new();
    for last in ancestors_last {
        if *last {
            prefix.push_str("  ");
        } else {
            prefix.push_str("| ");
        }
    }
    if depth > 0 {
        if is_last {
            prefix.push_str("`- ");
        } else {
            prefix.push_str("|- ");
        }
    }

    let indicator = if node.is_dir() && !node.children.is_empty() {
        if node.is_expanded { '-' } else { '+' }
    } else {
        ' '
    };
    prefix.push(indicator);
    prefix.push(' ');

    let glyph = glyphs.glyph_for(node, node.is_expanded);
    if !glyph.is_empty() {
        prefix.push_str(&glyph);
        prefix.push(' ');
    }
    prefix
}

fn filter_allows(node: &FileTreeNode, filter: Option<&dyn FileTreeFilter>) -> bool {
    match filter {
        Some(filter) => filter.include(node),
        None => true,
    }
}

fn slice_by_display_width(text: &str, start: u16, width: u16) -> String {
    if width == 0 {
        return String::new();
    }
    let end = start.saturating_add(width);
    let mut col: u16 = 0;
    let mut out = String::new();
    for g in text.graphemes(true) {
        let w = UnicodeWidthStr::width(g).min(u16::MAX as usize) as u16;
        if col.saturating_add(w) <= start {
            col = col.saturating_add(w);
            continue;
        }
        if col >= end {
            break;
        }
        out.push_str(g);
        col = col.saturating_add(w);
        if col >= end {
            break;
        }
    }
    out
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

    Ok(FileTreeNode {
        id: FileTreeNodeId::new(id),
        name,
        kind,
        children,
        is_expanded,
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
        let height = prop_u16(spec, "height")?;
        let roots = prop_nodes(spec, "nodes")?
            .or_else(|| prop_nodes(spec, "roots").ok().flatten())
            .unwrap_or_default();
        let selection = prop_u64(spec, "selection")?;

        let selection = selection.map(FileTreeNodeId::new);
        let mut tree =
            FileTree::new(title, Binding::new(roots), Binding::new(selection)).enabled(enabled);
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

fn prop_nodes(spec: &ComponentSpec, name: &str) -> Result<Option<Vec<FileTreeNode>>, TreeError> {
    match spec.props.get(name) {
        Some(value) => parse_nodes_value(value)
            .map(Some)
            .map_err(|reason| invalid_prop(spec, name, &reason, value)),
        None => Ok(None),
    }
}

fn build_scroll_container(bindings: Arc<RwLock<FileTreeBindings>>) -> ScrollContainer {
    ScrollContainer::new(Box::new(FileTreeContent::new(bindings)))
        .with_padding(EdgeInsets::all(1))
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
            enabled: true.into(),
            height: 10.into(),
            filter: None,
            glyphs: Arc::new(glyphs.clone()),
            on_select: None,
            on_rename: None,
            on_delete: None,
        })))
        .build_visible_entries(&roots, None, &glyphs);
        assert!(entries.iter().all(|e| e.name != "main.rs"));

        let roots = sample_tree(true);
        let entries = FileTreeContent::new(Arc::new(RwLock::new(FileTreeBindings {
            title: "".into(),
            roots: roots.clone().into(),
            selection: None.into(),
            enabled: true.into(),
            height: 10.into(),
            filter: None,
            glyphs: Arc::new(glyphs.clone()),
            on_select: None,
            on_rename: None,
            on_delete: None,
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
            enabled: true.into(),
            height: 10.into(),
            filter: Some(filter.clone()),
            glyphs: Arc::new(glyphs.clone()),
            on_select: None,
            on_rename: None,
            on_delete: None,
        })))
        .build_visible_entries(&roots, Some(filter.as_ref()), &glyphs);
        assert!(entries.iter().all(|e| !e.name.contains(".git")));
        assert!(entries.iter().any(|e| e.name == "README.md"));
    }

    #[test]
    fn glyphs_use_extension_mapping() {
        let mut glyphs = FileTreeGlyphs::default();
        glyphs.set_extension("rs", "rs");
        let node = FileTreeNode::file(1, "main.rs");
        let glyph = glyphs.glyph_for(&node, false);
        assert_eq!(glyph, "rs");
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
            enabled: true.into(),
            height: 10.into(),
            filter: None,
            glyphs: Arc::new(FileTreeGlyphs::default()),
            on_select: None,
            on_rename: None,
            on_delete: None,
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
            enabled: true.into(),
            height: 10.into(),
            filter: None,
            glyphs: Arc::new(FileTreeGlyphs::default()),
            on_select: None,
            on_rename: None,
            on_delete: None,
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
            enabled: true.into(),
            height: 10.into(),
            filter: None,
            glyphs: Arc::new(FileTreeGlyphs::default()),
            on_select: None,
            on_rename: None,
            on_delete: None,
        }));
        let mut content = FileTreeContent::new(bindings.clone());
        content.start_rename(FileTreeNodeId::new(6), "README.md");
        if let Some(rename) = &mut content.rename {
            rename.buffer.set_text("README_NEW.md");
        }
        let result = content.commit_rename(FileTreeNodeId::new(6));
        assert!(result.is_some());
        let updated = bindings.read().roots.get();
        let readme = updated
            .iter()
            .find(|node| node.id == FileTreeNodeId::new(6));
        assert_eq!(readme.unwrap().name, "README_NEW.md");
    }
}
