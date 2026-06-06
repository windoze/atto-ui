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

    pub fn apply_ops_and_rebuild(&mut self, ops: &[TreeOp]) -> Result<(), TreeError> {
        super::apply_tree_ops(&mut self.root, ops)?;
        self.rebuild()
    }

    pub fn apply_ops_incremental(&mut self, ops: &[TreeOp]) -> Result<bool, TreeError> {
        let has_set_tree = ops.iter().any(|op| matches!(op, TreeOp::SetTree(_)));
        let mut root_after_ops = Vec::with_capacity(ops.len());
        let mut next_root = self.root.clone();
        for op in ops {
            super::apply_tree_ops(&mut next_root, std::slice::from_ref(op))?;
            root_after_ops.push(next_root.clone());
        }
        self.root = next_root;
        if has_set_tree {
            self.rebuild()?;
            return Ok(true);
        }

        let mut structural = false;
        for (op, root_after_op) in ops.iter().zip(root_after_ops.iter()) {
            match op {
                TreeOp::SetTree(_) => {}
                TreeOp::SetProp { id, name, value } => {
                    if id_matches_root(&self.view, id) {
                        let applied = apply_property_to_view(self.view.as_mut(), id, name, value)?;
                        match applied {
                            PropertyApply::Applied => {}
                            PropertyApply::UnsupportedProperty | PropertyApply::NotFound => {
                                self.rebuild()?;
                                return Ok(true);
                            }
                        }
                        continue;
                    }

                    match apply_property_to_view(self.view.as_mut(), id, name, value)? {
                        PropertyApply::Applied => {}
                        PropertyApply::UnsupportedProperty => {
                            if !replace_node_with_spec(
                                self.view.as_mut(),
                                id,
                                root_after_op,
                                &self.registry,
                            )? {
                                self.rebuild()?;
                                return Ok(true);
                            }
                            structural = true;
                        }
                        PropertyApply::NotFound => {
                            self.rebuild()?;
                            return Ok(true);
                        }
                    }
                }
                TreeOp::BindEvent { id, .. } | TreeOp::ClearEvent { id, .. } => {
                    if id_matches_root(&self.view, id) {
                        self.rebuild()?;
                        return Ok(true);
                    }
                    if !replace_node_with_spec(
                        self.view.as_mut(),
                        id,
                        root_after_op,
                        &self.registry,
                    )? {
                        self.rebuild()?;
                        return Ok(true);
                    }
                    structural = true;
                }
                TreeOp::Insert {
                    parent_id,
                    index,
                    child,
                } => {
                    if !insert_child_spec(
                        self.view.as_mut(),
                        parent_id,
                        *index,
                        child,
                        &self.registry,
                    )? {
                        self.rebuild()?;
                        return Ok(true);
                    }
                    structural = true;
                }
                TreeOp::Remove { id } => {
                    if id_matches_root(&self.view, id) {
                        return Err(TreeError::InvalidTreeOp(
                            "cannot remove root node".to_string(),
                        ));
                    }
                    if !remove_node(self.view.as_mut(), id) {
                        self.rebuild()?;
                        return Ok(true);
                    }
                    structural = true;
                }
                TreeOp::Replace { id, node } => {
                    if id_matches_root(&self.view, id) {
                        self.rebuild()?;
                        return Ok(true);
                    }
                    if !replace_node_with_child_spec(self.view.as_mut(), id, node, &self.registry)?
                    {
                        self.rebuild()?;
                        return Ok(true);
                    }
                    structural = true;
                }
                TreeOp::Move {
                    id,
                    new_parent_id,
                    index,
                } => {
                    if id_matches_root(&self.view, id) {
                        return Err(TreeError::InvalidTreeOp(
                            "cannot move root node".to_string(),
                        ));
                    }
                    // `move_node` keeps the view tree intact on failure, so rebuilding remains safe.
                    if !move_node(self.view.as_mut(), id, new_parent_id, *index) {
                        self.rebuild()?;
                        return Ok(true);
                    }
                    structural = true;
                }
            }
        }

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

fn id_matches_root(view: &dyn Component, id: &str) -> bool {
    view.tag().is_some_and(|view_id| view_id == id)
}

pub(super) fn apply_property_to_view(
    view: &mut dyn Component,
    id: &str,
    name: &str,
    value: &ComponentValue,
) -> Result<PropertyApply, TreeError> {
    if view.tag() == Some(id) {
        return match view.set_property(name, value.clone()) {
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
        };
    }

    if let Some(children) = view.children_mut() {
        for child in children.iter_mut() {
            let applied = apply_property_to_view(child.view.as_mut(), id, name, value)?;
            if applied != PropertyApply::NotFound {
                return Ok(applied);
            }
        }
    }

    Ok(PropertyApply::NotFound)
}

fn can_insert_into(view: &dyn Component, parent_id: &str) -> bool {
    if view.tag() == Some(parent_id) {
        return !view.is_tab_container();
    }

    view.children()
        .iter()
        .any(|child| can_insert_into(child.view.as_ref(), parent_id))
}

struct TakenNode {
    node: ComponentNode,
    parent_path: Vec<usize>,
    index: usize,
}

fn find_child_spec_by_id<'a>(root: &'a ComponentSpec, id: &str) -> Option<&'a ComponentSpecChild> {
    for child in &root.children {
        if child.node.id.as_deref() == Some(id) {
            return Some(child);
        }
        if let Some(found) = find_child_spec_by_id(child.node.as_ref(), id) {
            return Some(found);
        }
    }
    None
}

fn replace_node_with_spec(
    view: &mut dyn Component,
    id: &str,
    root: &ComponentSpec,
    registry: &ComponentRegistry<Box<dyn Component>>,
) -> Result<bool, TreeError> {
    let Some(child_spec) = find_child_spec_by_id(root, id) else {
        return Ok(false);
    };
    replace_node_with_child_spec(view, id, child_spec, registry)
}

fn replace_node_with_child_spec(
    view: &mut dyn Component,
    id: &str,
    child: &ComponentSpecChild,
    registry: &ComponentRegistry<Box<dyn Component>>,
) -> Result<bool, TreeError> {
    let tab_view = view.is_tab_container();
    let Some(children) = view.children_mut() else {
        return Ok(false);
    };
    for node in children.iter_mut() {
        if node.view.tag() == Some(id) {
            if tab_view {
                return Ok(false);
            }
            let new_view = registry.build(&child.node)?;
            node.view = new_view;
            node.layout = child
                .layout
                .as_ref()
                .map(layout_from_spec)
                .unwrap_or_default();
            return Ok(true);
        }
        if replace_node_with_child_spec(node.view.as_mut(), id, child, registry)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn insert_child_spec(
    view: &mut dyn Component,
    parent_id: &str,
    index: usize,
    child: &ComponentSpecChild,
    registry: &ComponentRegistry<Box<dyn Component>>,
) -> Result<bool, TreeError> {
    if view.tag() == Some(parent_id) {
        if view.is_tab_container() {
            return Ok(false);
        }
        let Some(children) = view.children_mut() else {
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
        return Ok(true);
    }

    if let Some(children) = view.children_mut() {
        for node in children.iter_mut() {
            if insert_child_spec(node.view.as_mut(), parent_id, index, child, registry)? {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

fn remove_node(view: &mut dyn Component, id: &str) -> bool {
    let tab_view = view.is_tab_container();
    let Some(children) = view.children_mut() else {
        return false;
    };
    if tab_view {
        if children.iter().any(|child| child.view.tag() == Some(id)) {
            return false;
        }
    } else if let Some(idx) = children
        .iter()
        .position(|child| child.view.tag() == Some(id))
    {
        children.remove(idx);
        return true;
    }

    for child in children.iter_mut() {
        if remove_node(child.view.as_mut(), id) {
            return true;
        }
    }

    false
}

pub(super) fn move_node(
    view: &mut dyn Component,
    id: &str,
    new_parent_id: &str,
    index: usize,
) -> bool {
    if !can_insert_into(view, new_parent_id) {
        return false;
    }

    let Some(taken) = take_node(view, id) else {
        return false;
    };
    let restore_path = taken.parent_path;
    let restore_index = taken.index;
    let mut node = Some(taken.node);
    let inserted = insert_existing_node(view, new_parent_id, index, &mut node);
    debug_assert_eq!(inserted, node.is_none());
    if inserted && node.is_none() {
        return true;
    }

    if let Some(node) = node {
        let restored = restore_node(view, &restore_path, restore_index, node).is_ok();
        debug_assert!(
            restored,
            "failed to restore node after move insertion failure"
        );
    }

    false
}

fn take_node(view: &mut dyn Component, id: &str) -> Option<TakenNode> {
    take_node_at_path(view, id, &mut Vec::new())
}

fn take_node_at_path(
    view: &mut dyn Component,
    id: &str,
    parent_path: &mut Vec<usize>,
) -> Option<TakenNode> {
    let tab_view = view.is_tab_container();
    let children = view.children_mut()?;
    if !tab_view {
        if let Some(idx) = children
            .iter()
            .position(|child| child.view.tag() == Some(id))
        {
            return Some(TakenNode {
                node: children.remove(idx),
                parent_path: parent_path.clone(),
                index: idx,
            });
        }
    } else if children.iter().any(|child| child.view.tag() == Some(id)) {
        return None;
    }

    for (child_idx, child) in children.iter_mut().enumerate() {
        parent_path.push(child_idx);
        let found = take_node_at_path(child.view.as_mut(), id, parent_path);
        parent_path.pop();
        if let Some(found) = found {
            return Some(found);
        }
    }
    None
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

fn insert_existing_node(
    view: &mut dyn Component,
    parent_id: &str,
    index: usize,
    node: &mut Option<ComponentNode>,
) -> bool {
    if node.is_none() {
        return true;
    }
    if view.tag() == Some(parent_id) {
        if view.is_tab_container() {
            return false;
        }
        let Some(children) = view.children_mut() else {
            return false;
        };
        let mut node = node.take().expect("node present");
        node.parent = children.first().and_then(|existing| existing.parent);
        let idx = index.min(children.len());
        children.insert(idx, node);
        return true;
    }

    if let Some(children) = view.children_mut() {
        for child in children.iter_mut() {
            if insert_existing_node(child.view.as_mut(), parent_id, index, node) {
                return node.is_none();
            }
        }
    }

    false
}
