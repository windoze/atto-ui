use std::collections::HashMap;

use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect as RatatuiRect;

use crate::ComponentError;
use crate::composable::{
    Component, ComponentContext, ComponentId, ComponentNode, DynamicTree, EventHandling,
    EventResult, FocusNav, Layout, ScrollConfig, Scrollable, TitleBarContent, TitleBarContext,
};

use super::props::layout_from_spec;
use super::registry::global_registry;
use super::{
    CallbackRegistry, ComponentRegistry, ComponentSpec, ComponentSpecChild, ComponentValue,
    TreeError, TreeOp,
};

pub struct ComponentTree {
    root: ComponentSpec,
    callbacks: CallbackRegistry,
    registry: ComponentRegistry<Box<dyn Component>>,
    view: Box<dyn Component>,
}

impl ComponentTree {
    pub fn new(root: ComponentSpec, callbacks: CallbackRegistry) -> Result<Self, TreeError> {
        let registry = global_registry(callbacks.clone());
        Self::new_with_registry(root, callbacks, registry)
    }

    pub fn new_with_registry(
        root: ComponentSpec,
        callbacks: CallbackRegistry,
        registry: ComponentRegistry<Box<dyn Component>>,
    ) -> Result<Self, TreeError> {
        let view = registry.build(&root)?;
        Ok(Self {
            root,
            callbacks,
            registry,
            view,
        })
    }

    pub fn registry(&self) -> &ComponentRegistry<Box<dyn Component>> {
        &self.registry
    }

    pub fn callbacks(&self) -> &CallbackRegistry {
        &self.callbacks
    }

    pub fn root_spec(&self) -> &ComponentSpec {
        &self.root
    }

    pub fn view(&self) -> &dyn Component {
        self.view.as_ref()
    }

    pub fn view_mut(&mut self) -> &mut dyn Component {
        self.view.as_mut()
    }

    pub fn apply_ops(&mut self, ops: &[TreeOp]) -> Result<bool, TreeError> {
        super::apply_tree_ops(&mut self.root, ops)
    }

    pub fn rebuild(&mut self) -> Result<(), TreeError> {
        self.view = self.registry.build(&self.root)?;
        Ok(())
    }

    fn replace_with_rebuilt_root(&mut self, root: ComponentSpec) -> Result<(), TreeError> {
        let view = self.registry.build(&root)?;
        self.root = root;
        self.view = view;
        Ok(())
    }

    fn restore_root_after_incremental_error(&mut self, root: ComponentSpec) {
        match self.registry.build(&root) {
            Ok(view) => {
                self.root = root;
                self.view = view;
            }
            Err(err) => {
                debug_assert!(false, "failed to rebuild previously valid root: {err}");
                self.root = root;
            }
        }
    }

    fn rebuild_next_or_restore(
        &mut self,
        next_root: ComponentSpec,
        original_root: &ComponentSpec,
    ) -> Result<bool, TreeError> {
        match self.replace_with_rebuilt_root(next_root) {
            Ok(()) => Ok(true),
            Err(err) => {
                self.restore_root_after_incremental_error(original_root.clone());
                Err(err)
            }
        }
    }

    pub fn apply_ops_and_rebuild(&mut self, ops: &[TreeOp]) -> Result<(), TreeError> {
        let mut next_root = self.root.clone();
        super::apply_tree_ops(&mut next_root, ops)?;
        self.replace_with_rebuilt_root(next_root)
    }

    pub fn apply_ops_incremental(&mut self, ops: &[TreeOp]) -> Result<bool, TreeError> {
        if ops.is_empty() {
            return Ok(false);
        }

        let has_set_tree = ops.iter().any(|op| matches!(op, TreeOp::SetTree(_)));
        let original_root = self.root.clone();
        let mut root_after_ops = Vec::with_capacity(ops.len());
        let mut next_root = original_root.clone();
        for op in ops {
            super::apply_tree_ops(&mut next_root, std::slice::from_ref(op))?;
            root_after_ops.push(next_root.clone());
        }
        if has_set_tree || !view_shape_matches_spec(&original_root, self.view.as_ref()) {
            return self.rebuild_next_or_restore(next_root, &original_root);
        }

        macro_rules! try_view_update {
            ($expr:expr) => {
                match $expr {
                    Ok(value) => value,
                    Err(err) => {
                        self.restore_root_after_incremental_error(original_root.clone());
                        return Err(err);
                    }
                }
            };
        }

        let mut structural = false;
        let mut view_index = ViewPathIndex::new(self.view.as_ref());
        for (op, root_after_op) in ops.iter().zip(root_after_ops.iter()) {
            match op {
                TreeOp::SetTree(_) => {}
                TreeOp::SetProp { id, name, value } => {
                    let Some(path) = view_index.path(id).cloned() else {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    };
                    if path.is_empty() {
                        let applied = try_view_update!(apply_property_at_path(
                            self.view.as_mut(),
                            &path,
                            id,
                            name,
                            value,
                        ));
                        match applied {
                            PropertyApply::Applied => {}
                            PropertyApply::UnsupportedProperty | PropertyApply::NotFound => {
                                return self.rebuild_next_or_restore(next_root, &original_root);
                            }
                        }
                        continue;
                    }

                    match try_view_update!(apply_property_at_path(
                        self.view.as_mut(),
                        &path,
                        id,
                        name,
                        value,
                    )) {
                        PropertyApply::Applied => {}
                        PropertyApply::UnsupportedProperty => {
                            if !try_view_update!(replace_node_with_spec_at_path(
                                self.view.as_mut(),
                                &path,
                                id,
                                root_after_op,
                                &self.registry,
                            )) {
                                return self.rebuild_next_or_restore(next_root, &original_root);
                            }
                            view_index.rebuild(self.view.as_ref());
                            structural = true;
                        }
                        PropertyApply::NotFound => {
                            return self.rebuild_next_or_restore(next_root, &original_root);
                        }
                    }
                }
                TreeOp::ClearProp { id, .. } => {
                    let Some(path) = view_index.path(id).cloned() else {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    };
                    if path.is_empty() {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    }
                    if !try_view_update!(replace_node_with_spec_at_path(
                        self.view.as_mut(),
                        &path,
                        id,
                        root_after_op,
                        &self.registry,
                    )) {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    }
                    view_index.rebuild(self.view.as_ref());
                    structural = true;
                }
                TreeOp::BindEvent { id, .. } | TreeOp::ClearEvent { id, .. } => {
                    let Some(path) = view_index.path(id).cloned() else {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    };
                    if path.is_empty() {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    }
                    if !try_view_update!(replace_node_with_spec_at_path(
                        self.view.as_mut(),
                        &path,
                        id,
                        root_after_op,
                        &self.registry,
                    )) {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    }
                    view_index.rebuild(self.view.as_ref());
                    structural = true;
                }
                TreeOp::Insert {
                    parent_id,
                    index,
                    child,
                } => {
                    let Some(parent_path) = view_index.path(parent_id).cloned() else {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    };
                    if !try_view_update!(insert_child_spec_at_path(
                        self.view.as_mut(),
                        &parent_path,
                        parent_id,
                        *index,
                        child,
                        &self.registry,
                    )) {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    }
                    view_index.rebuild(self.view.as_ref());
                    if !view_shape_matches_spec(root_after_op, self.view.as_ref()) {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    }
                    structural = true;
                }
                TreeOp::InsertBefore {
                    parent_id,
                    anchor_id,
                    child,
                } => {
                    if let Some(child_id) = child.node.id.as_deref()
                        && view_index
                            .path(child_id)
                            .is_some_and(|path| !path.is_empty())
                    {
                        if !move_node_before_anchor_indexed(
                            self.view.as_mut(),
                            child_id,
                            parent_id,
                            anchor_id.as_deref(),
                            &mut view_index,
                        ) {
                            return self.rebuild_next_or_restore(next_root, &original_root);
                        }
                    } else {
                        let Some(parent_path) = view_index.path(parent_id).cloned() else {
                            return self.rebuild_next_or_restore(next_root, &original_root);
                        };
                        if !try_view_update!(insert_child_spec_before_anchor_at_path(
                            self.view.as_mut(),
                            &parent_path,
                            parent_id,
                            anchor_id.as_deref(),
                            child,
                            &self.registry,
                        )) {
                            return self.rebuild_next_or_restore(next_root, &original_root);
                        }
                        view_index.rebuild(self.view.as_ref());
                    }
                    if !view_shape_matches_spec(root_after_op, self.view.as_ref()) {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    }
                    structural = true;
                }
                TreeOp::Remove { id } => {
                    let Some(path) = view_index.path(id).cloned() else {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    };
                    if path.is_empty() {
                        return Err(TreeError::InvalidTreeOp(
                            "cannot remove root node".to_string(),
                        ));
                    }
                    if !remove_node_at_path(self.view.as_mut(), &path, id) {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    }
                    view_index.rebuild(self.view.as_ref());
                    if !view_shape_matches_spec(root_after_op, self.view.as_ref()) {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    }
                    structural = true;
                }
                TreeOp::Replace { id, node } => {
                    let Some(path) = view_index.path(id).cloned() else {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    };
                    if path.is_empty() {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    }
                    if !try_view_update!(replace_node_at_path_with_child_spec(
                        self.view.as_mut(),
                        &path,
                        id,
                        node,
                        &self.registry,
                    )) {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    }
                    view_index.rebuild(self.view.as_ref());
                    if !view_shape_matches_spec(root_after_op, self.view.as_ref()) {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    }
                    structural = true;
                }
                TreeOp::Move {
                    id,
                    new_parent_id,
                    index,
                } => {
                    let Some(path) = view_index.path(id).cloned() else {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    };
                    if path.is_empty() {
                        return Err(TreeError::InvalidTreeOp(
                            "cannot move root node".to_string(),
                        ));
                    }
                    // `move_node` keeps the view tree intact on failure, so rebuilding remains safe.
                    if !move_node_indexed(
                        self.view.as_mut(),
                        id,
                        new_parent_id,
                        *index,
                        &mut view_index,
                    ) {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    }
                    if !view_shape_matches_spec(root_after_op, self.view.as_ref()) {
                        return self.rebuild_next_or_restore(next_root, &original_root);
                    }
                    structural = true;
                }
            }
        }

        self.root = next_root;
        Ok(structural)
    }
}

impl Component for ComponentTree {
    fn type_name(&self) -> &'static str {
        self.view.type_name()
    }

    fn is_tab_container(&self) -> bool {
        self.view.is_tab_container()
    }

    fn property_names(&self) -> Vec<&'static str> {
        self.view.property_names()
    }

    fn get_property(&self, name: &str) -> Option<ComponentValue> {
        self.view.get_property(name)
    }

    fn set_property(&mut self, name: &str, value: ComponentValue) -> Result<(), ComponentError> {
        self.view.set_property(name, value)
    }

    fn apply_command(&mut self, command: crate::ComponentCommand) -> EventResult {
        self.view.apply_command(command)
    }

    fn titlebar(&mut self, ctx: TitleBarContext<'_>) -> Option<TitleBarContent> {
        self.view.titlebar(ctx)
    }

    fn handle_titlebar_event(&mut self, event: &Event, ctx: TitleBarContext<'_>) -> EventResult {
        self.view.handle_titlebar_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: RatatuiRect, ctx: ComponentContext<'_>) {
        self.view.draw(frame, area, ctx);
    }
}

impl Layout for ComponentTree {
    fn min_width(&self) -> u16 {
        self.view.min_width()
    }

    fn min_height(&self) -> u16 {
        self.view.min_height()
    }

    fn min_size(&self) -> (u16, u16) {
        self.view.min_size()
    }

    fn desired_width(&self) -> Option<u16> {
        self.view.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.view.desired_height()
    }
}

impl Scrollable for ComponentTree {
    fn is_scrollable(&self) -> bool {
        self.view.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.view.content_size()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.view.scroll_offset()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.view.viewport_size()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.view.scroll_config()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.view.set_scroll_offset(x, y);
    }

    fn scroll_to(&mut self, x: u16, y: u16) {
        self.view.scroll_to(x, y);
    }

    fn scroll_to_child(&mut self, child_id: ComponentId) {
        self.view.scroll_to_child(child_id);
    }
}

impl FocusNav for ComponentTree {
    fn focused_child(&self) -> Option<ComponentId> {
        self.view.focused_child()
    }

    fn is_focusable(&self) -> bool {
        self.view.is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        self.view.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.view.focus_last()
    }
}

impl DynamicTree for ComponentTree {
    fn tag(&self) -> Option<&str> {
        self.view.tag()
    }

    fn children(&self) -> &[ComponentNode] {
        self.view.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        self.view.children_mut()
    }

    fn apply_tree_ops(&mut self, ops: &[TreeOp]) -> Result<bool, TreeError> {
        self.apply_ops_incremental(ops)
    }

    fn rebuild_tree(&mut self) -> Result<(), TreeError> {
        self.rebuild()
    }

    fn dynamic_root_spec(&self) -> Option<&ComponentSpec> {
        Some(self.root_spec())
    }

    fn dynamic_callbacks(&self) -> Option<&CallbackRegistry> {
        Some(self.callbacks())
    }
}

impl EventHandling for ComponentTree {
    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.view.handle_event_capture(event, ctx)
    }

    fn handle_event_bubble(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.view.handle_event_bubble(event, ctx)
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.view.handle_event(event, ctx)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PropertyApply {
    Applied,
    UnsupportedProperty,
    NotFound,
}

#[cfg(test)]
pub(super) fn apply_property_to_view(
    view: &mut dyn Component,
    id: &str,
    name: &str,
    value: &ComponentValue,
) -> Result<PropertyApply, TreeError> {
    let index = ViewPathIndex::new(view);
    let Some(path) = index.path(id) else {
        return Ok(PropertyApply::NotFound);
    };
    apply_property_at_path(view, path, id, name, value)
}

fn apply_property_at_path(
    view: &mut dyn Component,
    path: &[usize],
    id: &str,
    name: &str,
    value: &ComponentValue,
) -> Result<PropertyApply, TreeError> {
    let Some(target) = view_at_path_mut(view, path) else {
        return Ok(PropertyApply::NotFound);
    };
    if target.tag() != Some(id) {
        return Ok(PropertyApply::NotFound);
    }
    match target.set_property(name, value.clone()) {
        Ok(()) => Ok(PropertyApply::Applied),
        Err(ComponentError::UnsupportedProperty(_)) => Ok(PropertyApply::UnsupportedProperty),
        Err(ComponentError::NotFound(_)) => Ok(PropertyApply::NotFound),
        Err(ComponentError::InvalidValue { expected, .. }) => Err(TreeError::InvalidProperty {
            id: id.to_string(),
            name: name.to_string(),
            reason: format!("expected {expected}"),
        }),
        Err(err) => Err(TreeError::InvalidProperty {
            id: id.to_string(),
            name: name.to_string(),
            reason: format!("{err:?}"),
        }),
    }
}

struct TakenNode {
    node: ComponentNode,
    parent_path: Vec<usize>,
    index: usize,
}

type ViewPath = Vec<usize>;

struct ViewPathIndex {
    paths: HashMap<String, ViewPath>,
}

fn view_shape_matches_spec(spec: &ComponentSpec, view: &dyn Component) -> bool {
    if spec.id.as_deref() != view.tag() {
        return false;
    }

    let children = view.children();
    if spec.children.len() != children.len() {
        return false;
    }

    spec.children
        .iter()
        .zip(children.iter())
        .all(|(child_spec, child_view)| {
            view_shape_matches_spec(child_spec.node.as_ref(), child_view.view.as_ref())
        })
}

impl ViewPathIndex {
    fn new(view: &dyn Component) -> Self {
        let mut paths = HashMap::new();
        index_view_paths(view, &mut Vec::new(), &mut paths);
        Self { paths }
    }

    fn rebuild(&mut self, view: &dyn Component) {
        *self = Self::new(view);
    }

    fn path(&self, id: &str) -> Option<&ViewPath> {
        self.paths.get(id)
    }
}

fn index_view_paths(
    view: &dyn Component,
    path: &mut ViewPath,
    paths: &mut HashMap<String, ViewPath>,
) {
    if let Some(id) = view.tag() {
        paths.entry(id.to_string()).or_insert_with(|| path.clone());
    }
    for (idx, child) in view.children().iter().enumerate() {
        path.push(idx);
        index_view_paths(child.view.as_ref(), path, paths);
        path.pop();
    }
}

fn view_at_path<'a>(view: &'a dyn Component, path: &[usize]) -> Option<&'a dyn Component> {
    let mut current = view;
    for &idx in path {
        current = current.children().get(idx)?.view.as_ref();
    }
    Some(current)
}

fn view_at_path_mut<'a>(
    view: &'a mut dyn Component,
    path: &[usize],
) -> Option<&'a mut dyn Component> {
    let mut current = view;
    for &idx in path {
        current = current.children_mut()?.get_mut(idx)?.view.as_mut();
    }
    Some(current)
}

fn child_spec_at_path<'a>(
    root: &'a ComponentSpec,
    path: &[usize],
) -> Option<&'a ComponentSpecChild> {
    let (&idx, parent_path) = path.split_last()?;
    let mut parent = root;
    for &parent_idx in parent_path {
        parent = parent.children.get(parent_idx)?.node.as_ref();
    }
    parent.children.get(idx)
}

fn replace_node_with_spec_at_path(
    view: &mut dyn Component,
    path: &[usize],
    id: &str,
    root: &ComponentSpec,
    registry: &ComponentRegistry<Box<dyn Component>>,
) -> Result<bool, TreeError> {
    let Some(child_spec) = child_spec_at_path(root, path) else {
        return Ok(false);
    };
    replace_node_at_path_with_child_spec(view, path, id, child_spec, registry)
}

fn replace_node_at_path_with_child_spec(
    view: &mut dyn Component,
    path: &[usize],
    id: &str,
    child: &ComponentSpecChild,
    registry: &ComponentRegistry<Box<dyn Component>>,
) -> Result<bool, TreeError> {
    let Some((&idx, parent_path)) = path.split_last() else {
        return Ok(false);
    };
    let Some(parent) = view_at_path_mut(view, parent_path) else {
        return Ok(false);
    };
    if parent.is_tab_container() {
        return Ok(false);
    }
    let Some(children) = parent.children_mut() else {
        return Ok(false);
    };
    let Some(node) = children.get_mut(idx) else {
        return Ok(false);
    };
    if node.view.tag() != Some(id) {
        return Ok(false);
    }
    let new_view = registry.build(&child.node)?;
    node.view = new_view;
    node.layout = child
        .layout
        .as_ref()
        .map(layout_from_spec)
        .unwrap_or_default();
    Ok(true)
}

fn insert_child_spec_at_path(
    view: &mut dyn Component,
    parent_path: &[usize],
    parent_id: &str,
    index: usize,
    child: &ComponentSpecChild,
    registry: &ComponentRegistry<Box<dyn Component>>,
) -> Result<bool, TreeError> {
    let Some(parent) = view_at_path_mut(view, parent_path) else {
        return Ok(false);
    };
    if parent.tag() != Some(parent_id) || parent.is_tab_container() {
        return Ok(false);
    }
    let Some(children) = parent.children_mut() else {
        return Ok(false);
    };
    let layout = child
        .layout
        .as_ref()
        .map(layout_from_spec)
        .unwrap_or_default();
    let mut node = ComponentNode::new(registry.build(&child.node)?).with_layout(layout);
    node.parent = children.first().and_then(|existing| existing.parent);
    let idx = index.min(children.len());
    children.insert(idx, node);
    Ok(true)
}

fn insert_child_spec_before_anchor_at_path(
    view: &mut dyn Component,
    parent_path: &[usize],
    parent_id: &str,
    anchor_id: Option<&str>,
    child: &ComponentSpecChild,
    registry: &ComponentRegistry<Box<dyn Component>>,
) -> Result<bool, TreeError> {
    let Some(parent) = view_at_path_mut(view, parent_path) else {
        return Ok(false);
    };
    if parent.tag() != Some(parent_id) || parent.is_tab_container() {
        return Ok(false);
    }
    let Some(children) = parent.children_mut() else {
        return Ok(false);
    };
    let Some(idx) = child_node_index_before_anchor(children, anchor_id) else {
        return Ok(false);
    };
    let layout = child
        .layout
        .as_ref()
        .map(layout_from_spec)
        .unwrap_or_default();
    let mut node = ComponentNode::new(registry.build(&child.node)?).with_layout(layout);
    node.parent = children.first().and_then(|existing| existing.parent);
    children.insert(idx, node);
    Ok(true)
}

fn child_node_index_before_anchor(
    children: &[ComponentNode],
    anchor_id: Option<&str>,
) -> Option<usize> {
    match anchor_id {
        Some(anchor_id) => children
            .iter()
            .position(|child| child.view.tag() == Some(anchor_id)),
        None => Some(children.len()),
    }
}

fn remove_node_at_path(view: &mut dyn Component, path: &[usize], id: &str) -> bool {
    let Some((&idx, parent_path)) = path.split_last() else {
        return false;
    };
    let Some(parent) = view_at_path_mut(view, parent_path) else {
        return false;
    };
    if parent.is_tab_container() {
        return false;
    }
    let Some(children) = parent.children_mut() else {
        return false;
    };
    if children.get(idx).and_then(|child| child.view.tag()) != Some(id) {
        return false;
    }
    children.remove(idx);
    true
}

#[cfg(test)]
pub(super) fn move_node(
    view: &mut dyn Component,
    id: &str,
    new_parent_id: &str,
    index: usize,
) -> bool {
    let mut view_index = ViewPathIndex::new(view);
    move_node_indexed(view, id, new_parent_id, index, &mut view_index)
}

fn move_node_indexed(
    view: &mut dyn Component,
    id: &str,
    new_parent_id: &str,
    index: usize,
    view_index: &mut ViewPathIndex,
) -> bool {
    let Some(parent_path) = view_index.path(new_parent_id).cloned() else {
        return false;
    };
    if !can_insert_at_path(view, &parent_path, new_parent_id) {
        return false;
    }
    let Some(path) = view_index.path(id).filter(|path| !path.is_empty()).cloned() else {
        return false;
    };
    let Some(taken) = take_node_at_path(view, &path, id) else {
        return false;
    };
    let restore_path = taken.parent_path;
    let restore_index = taken.index;
    let mut node = Some(taken.node);
    view_index.rebuild(view);
    let inserted = view_index
        .path(new_parent_id)
        .cloned()
        .is_some_and(|parent_path| {
            insert_existing_node_at_path(view, &parent_path, new_parent_id, index, &mut node)
        });
    debug_assert_eq!(inserted, node.is_none());
    if inserted && node.is_none() {
        view_index.rebuild(view);
        return true;
    }

    if let Some(node) = node {
        let restored = restore_node(view, &restore_path, restore_index, node).is_ok();
        debug_assert!(
            restored,
            "failed to restore node after move insertion failure"
        );
    }
    view_index.rebuild(view);

    false
}

fn move_node_before_anchor_indexed(
    view: &mut dyn Component,
    id: &str,
    new_parent_id: &str,
    anchor_id: Option<&str>,
    view_index: &mut ViewPathIndex,
) -> bool {
    let Some(parent_path) = view_index.path(new_parent_id).cloned() else {
        return false;
    };
    if !can_insert_at_path(view, &parent_path, new_parent_id) {
        return false;
    }
    let Some(path) = view_index.path(id).filter(|path| !path.is_empty()).cloned() else {
        return false;
    };
    if parent_path.starts_with(&path) {
        return false;
    }
    if anchor_id == Some(id) {
        let current_parent_path = &path[..path.len() - 1];
        return parent_path.as_slice() == current_parent_path;
    }

    let Some(taken) = take_node_at_path(view, &path, id) else {
        return false;
    };
    let restore_path = taken.parent_path;
    let restore_index = taken.index;
    let mut node = Some(taken.node);
    view_index.rebuild(view);
    let inserted = view_index
        .path(new_parent_id)
        .cloned()
        .is_some_and(|parent_path| {
            insert_existing_node_before_anchor_at_path(
                view,
                &parent_path,
                new_parent_id,
                anchor_id,
                &mut node,
            )
        });
    debug_assert_eq!(inserted, node.is_none());
    if inserted && node.is_none() {
        view_index.rebuild(view);
        return true;
    }

    if let Some(node) = node {
        let restored = restore_node(view, &restore_path, restore_index, node).is_ok();
        debug_assert!(
            restored,
            "failed to restore node after insert-before move insertion failure"
        );
    }
    view_index.rebuild(view);

    false
}

fn can_insert_at_path(view: &dyn Component, parent_path: &[usize], parent_id: &str) -> bool {
    let Some(parent) = view_at_path(view, parent_path) else {
        return false;
    };
    parent.tag() == Some(parent_id) && !parent.is_tab_container()
}

fn take_node_at_path(view: &mut dyn Component, path: &[usize], id: &str) -> Option<TakenNode> {
    let (&idx, parent_path) = path.split_last()?;
    let parent = view_at_path_mut(view, parent_path)?;
    if parent.is_tab_container() {
        return None;
    }
    let children = parent.children_mut()?;
    if children.get(idx).and_then(|child| child.view.tag()) != Some(id) {
        return None;
    }
    Some(TakenNode {
        node: children.remove(idx),
        parent_path: parent_path.to_vec(),
        index: idx,
    })
}

fn restore_node(
    view: &mut dyn Component,
    parent_path: &[usize],
    index: usize,
    node: ComponentNode,
) -> Result<(), ComponentNode> {
    let Some(children) = view.children_mut() else {
        return Err(node);
    };
    let Some((&child_idx, remaining_path)) = parent_path.split_first() else {
        let idx = index.min(children.len());
        children.insert(idx, node);
        return Ok(());
    };
    let Some(child) = children.get_mut(child_idx) else {
        return Err(node);
    };
    restore_node(child.view.as_mut(), remaining_path, index, node)
}

fn insert_existing_node_at_path(
    view: &mut dyn Component,
    parent_path: &[usize],
    parent_id: &str,
    index: usize,
    node: &mut Option<ComponentNode>,
) -> bool {
    if node.is_none() {
        return true;
    }
    let Some(parent) = view_at_path_mut(view, parent_path) else {
        return false;
    };
    if parent.tag() != Some(parent_id) || parent.is_tab_container() {
        return false;
    }
    let Some(children) = parent.children_mut() else {
        return false;
    };
    let mut node = node.take().expect("node present");
    node.parent = children.first().and_then(|existing| existing.parent);
    let idx = index.min(children.len());
    children.insert(idx, node);
    true
}

fn insert_existing_node_before_anchor_at_path(
    view: &mut dyn Component,
    parent_path: &[usize],
    parent_id: &str,
    anchor_id: Option<&str>,
    node: &mut Option<ComponentNode>,
) -> bool {
    if node.is_none() {
        return true;
    }
    let Some(parent) = view_at_path_mut(view, parent_path) else {
        return false;
    };
    if parent.tag() != Some(parent_id) || parent.is_tab_container() {
        return false;
    }
    let Some(children) = parent.children_mut() else {
        return false;
    };
    let Some(idx) = child_node_index_before_anchor(children, anchor_id) else {
        return false;
    };
    let mut node = node.take().expect("node present");
    node.parent = children.first().and_then(|existing| existing.parent);
    children.insert(idx, node);
    true
}
