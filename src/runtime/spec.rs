//! Language-neutral runtime specs and tree operation primitives.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

type Factory<T> =
    dyn Fn(&ComponentSpec, &ComponentRegistry<T>) -> Result<T, TreeError> + Send + Sync;

struct ComponentFactory<T> {
    schema: ComponentSchema,
    build: Arc<Factory<T>>,
}

/// 语言无关的组件注册表。
///
/// 该注册表负责将 `ComponentSpec` 转换为宿主侧的具体组件类型 `T`。
pub struct ComponentRegistry<T> {
    factories: BTreeMap<String, ComponentFactory<T>>,
}

impl<T> ComponentRegistry<T> {
    pub fn new() -> Self {
        Self {
            factories: BTreeMap::new(),
        }
    }

    pub fn register<F>(&mut self, schema: ComponentSchema, build: F)
    where
        F: Fn(&ComponentSpec, &ComponentRegistry<T>) -> Result<T, TreeError>
            + Send
            + Sync
            + 'static,
    {
        let key = schema.type_name.clone();
        self.factories.insert(
            key,
            ComponentFactory {
                schema,
                build: Arc::new(build),
            },
        );
    }

    pub fn schema(&self, type_name: &str) -> Option<&ComponentSchema> {
        self.factories.get(type_name).map(|factory| &factory.schema)
    }

    pub fn schemas(&self) -> impl Iterator<Item = &ComponentSchema> {
        self.factories.values().map(|factory| &factory.schema)
    }

    pub fn build(&self, spec: &ComponentSpec) -> Result<T, TreeError> {
        let factory = self
            .factories
            .get(&spec.type_name)
            .ok_or_else(|| TreeError::UnknownComponent(spec.type_name.clone()))?;
        (factory.build)(spec, self)
    }
}

impl<T> Default for ComponentRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ComponentValue {
    Null,
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    String(String),
    StringList(Vec<String>),
    Table(Vec<Vec<String>>),
    Rect(Rect),
    Bytes(Vec<u8>),
    List(Vec<ComponentValue>),
    Map(BTreeMap<String, ComponentValue>),
}

impl ComponentValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ComponentValue::String(v) => Some(v.as_str()),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            ComponentValue::U64(v) => Some(*v),
            ComponentValue::I64(v) if *v >= 0 => Some(*v as u64),
            ComponentValue::F64(v) if *v >= 0.0 => Some(*v as u64),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            ComponentValue::I64(v) => Some(*v),
            ComponentValue::U64(v) => Some(*v as i64),
            ComponentValue::F64(v) => Some(*v as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            ComponentValue::F64(v) => Some(*v),
            ComponentValue::I64(v) => Some(*v as f64),
            ComponentValue::U64(v) => Some(*v as f64),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueType {
    Bool,
    I64,
    U64,
    F64,
    String,
    StringList,
    Table,
    Rect,
    Bytes,
    List,
    Map,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyMeta {
    pub name: String,
    pub value_type: ValueType,
    pub readable: bool,
    pub writable: bool,
}

impl PropertyMeta {
    pub fn new(name: impl Into<String>, value_type: ValueType) -> Self {
        Self {
            name: name.into(),
            value_type,
            readable: true,
            writable: true,
        }
    }

    pub fn read_only(mut self) -> Self {
        self.writable = false;
        self
    }

    pub fn write_only(mut self) -> Self {
        self.readable = false;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionMeta {
    pub name: String,
    pub payload: Option<ValueType>,
}

impl ActionMeta {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            payload: None,
        }
    }

    pub fn with_payload(mut self, payload: ValueType) -> Self {
        self.payload = Some(payload);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMeta {
    pub name: String,
    pub payload: Option<ValueType>,
}

impl EventMeta {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            payload: None,
        }
    }

    pub fn with_payload(mut self, payload: ValueType) -> Self {
        self.payload = Some(payload);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentSchema {
    pub type_name: String,
    pub properties: Vec<PropertyMeta>,
    pub actions: Vec<ActionMeta>,
    pub events: Vec<EventMeta>,
    pub allows_children: bool,
}

impl ComponentSchema {
    pub fn new(type_name: impl Into<String>) -> Self {
        Self {
            type_name: type_name.into(),
            properties: Vec::new(),
            actions: Vec::new(),
            events: Vec::new(),
            allows_children: false,
        }
    }

    pub fn with_properties(mut self, properties: Vec<PropertyMeta>) -> Self {
        self.properties = properties;
        self
    }

    pub fn with_action(mut self, action: ActionMeta) -> Self {
        self.actions.push(action);
        self
    }

    pub fn with_event(mut self, event: EventMeta) -> Self {
        self.events.push(event);
        self
    }

    pub fn allow_children(mut self, allow: bool) -> Self {
        self.allows_children = allow;
        self
    }

    pub fn dedup_properties(&mut self) {
        let mut seen = std::collections::BTreeSet::new();
        self.properties
            .retain(|prop| seen.insert(prop.name.clone()));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlignSpec {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SizeSpec {
    Fill,
    Fixed(u16),
    Weight(u16),
    Content,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnchorSpec {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Top,
    Bottom,
    Left,
    Right,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorPlacementSpec {
    pub anchor: AnchorSpec,
    pub offset_x: i16,
    pub offset_y: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeInsetsSpec {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl EdgeInsetsSpec {
    pub const ZERO: Self = Self {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutSpec {
    pub width: SizeSpec,
    pub height: SizeSpec,
    pub margin: EdgeInsetsSpec,
    pub align_x: AlignSpec,
    pub align_y: AlignSpec,
    pub anchor: Option<AnchorPlacementSpec>,
    pub tab_index: Option<i32>,
}

impl Default for LayoutSpec {
    fn default() -> Self {
        Self {
            width: SizeSpec::Fill,
            height: SizeSpec::Fill,
            margin: EdgeInsetsSpec::ZERO,
            align_x: AlignSpec::Start,
            align_y: AlignSpec::Start,
            anchor: None,
            tab_index: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentSpecChild {
    pub node: Box<ComponentSpec>,
    pub layout: Option<LayoutSpec>,
    pub meta: BTreeMap<String, ComponentValue>,
}

impl ComponentSpecChild {
    pub fn new(node: ComponentSpec) -> Self {
        Self {
            node: Box::new(node),
            layout: None,
            meta: BTreeMap::new(),
        }
    }

    pub fn with_layout(mut self, layout: LayoutSpec) -> Self {
        self.layout = Some(layout);
        self
    }

    pub fn with_meta(mut self, key: impl Into<String>, value: ComponentValue) -> Self {
        self.meta.insert(key.into(), value);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentSpec {
    pub type_name: String,
    pub id: Option<String>,
    pub props: BTreeMap<String, ComponentValue>,
    pub events: BTreeMap<String, CallbackId>,
    pub children: Vec<ComponentSpecChild>,
}

impl ComponentSpec {
    pub fn new(type_name: impl Into<String>) -> Self {
        Self {
            type_name: type_name.into(),
            id: None,
            props: BTreeMap::new(),
            events: BTreeMap::new(),
            children: Vec::new(),
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn with_prop(mut self, name: impl Into<String>, value: ComponentValue) -> Self {
        self.props.insert(name.into(), value);
        self
    }

    pub fn with_event(mut self, name: impl Into<String>, callback: CallbackId) -> Self {
        self.events.insert(name.into(), callback);
        self
    }

    pub fn with_child(mut self, child: ComponentSpecChild) -> Self {
        self.children.push(child);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CallbackId(pub u64);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CallbackInvocation {
    pub callback_id: CallbackId,
    pub target_id: Option<String>,
    pub event: String,
    pub payload: Option<ComponentValue>,
}

#[derive(Clone, Default)]
pub struct CallbackRegistry {
    next_id: Arc<AtomicU64>,
    queue: Arc<Mutex<VecDeque<CallbackInvocation>>>,
}

impl CallbackRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self) -> CallbackId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        CallbackId(id)
    }

    pub fn emit(&self, invocation: CallbackInvocation) {
        self.queue.lock().push_back(invocation);
    }

    pub fn emit_simple(
        &self,
        callback_id: CallbackId,
        target_id: Option<String>,
        event: impl Into<String>,
    ) {
        self.emit(CallbackInvocation {
            callback_id,
            target_id,
            event: event.into(),
            payload: None,
        });
    }

    pub fn drain(&self) -> Vec<CallbackInvocation> {
        let mut guard = self.queue.lock();
        let mut out = Vec::new();
        while let Some(invocation) = guard.pop_front() {
            out.push(invocation);
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TreeOp {
    SetTree(ComponentSpec),
    Insert {
        parent_id: String,
        index: usize,
        child: ComponentSpecChild,
    },
    Remove {
        id: String,
    },
    Replace {
        id: String,
        node: ComponentSpecChild,
    },
    Move {
        id: String,
        new_parent_id: String,
        index: usize,
    },
    SetProp {
        id: String,
        name: String,
        value: ComponentValue,
    },
    BindEvent {
        id: String,
        event: String,
        callback: CallbackId,
    },
    ClearEvent {
        id: String,
        event: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeError {
    MissingId(String),
    NotFound(String),
    UnknownComponent(String),
    InvalidProperty {
        id: String,
        name: String,
        reason: String,
    },
    InvalidEvent {
        id: String,
        name: String,
    },
    InvalidTreeOp(String),
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TreeError::MissingId(id) => write!(f, "missing id: {id}"),
            TreeError::NotFound(id) => write!(f, "node not found: {id}"),
            TreeError::UnknownComponent(name) => write!(f, "unknown component: {name}"),
            TreeError::InvalidProperty { id, name, reason } => {
                write!(f, "invalid property {name} for {id}: {reason}")
            }
            TreeError::InvalidEvent { id, name } => {
                write!(f, "invalid event {name} for {id}")
            }
            TreeError::InvalidTreeOp(reason) => write!(f, "invalid tree op: {reason}"),
        }
    }
}

impl std::error::Error for TreeError {}

pub fn apply_tree_ops(root: &mut ComponentSpec, ops: &[TreeOp]) -> Result<bool, TreeError> {
    let mut structural = false;
    for op in ops {
        match op {
            TreeOp::SetTree(spec) => {
                *root = spec.clone();
                structural = true;
            }
            TreeOp::Insert {
                parent_id,
                index,
                child,
            } => {
                let parent = find_by_id_mut(root, parent_id)
                    .ok_or_else(|| TreeError::NotFound(parent_id.clone()))?;
                let idx = (*index).min(parent.children.len());
                parent.children.insert(idx, child.clone());
                structural = true;
            }
            TreeOp::Remove { id } => {
                if remove_by_id(root, id) {
                    structural = true;
                } else {
                    return Err(TreeError::NotFound(id.clone()));
                }
            }
            TreeOp::Replace { id, node } => {
                if replace_by_id(root, id, node.clone()) {
                    structural = true;
                } else {
                    return Err(TreeError::NotFound(id.clone()));
                }
            }
            TreeOp::Move {
                id,
                new_parent_id,
                index,
            } => {
                let moving_node = find_child_node_by_id(root, id)
                    .ok_or_else(|| TreeError::NotFound(id.clone()))?;
                if find_by_id(moving_node, new_parent_id).is_some() {
                    return Err(TreeError::InvalidTreeOp(
                        "cannot move node into itself or descendant".to_string(),
                    ));
                }
                if find_by_id(root, new_parent_id).is_none() {
                    return Err(TreeError::NotFound(new_parent_id.clone()));
                }

                let node = take_by_id(root, id).expect("validated movable child exists");
                let parent = find_by_id_mut(root, new_parent_id)
                    .expect("validated target parent exists outside moved subtree");
                let idx = (*index).min(parent.children.len());
                parent.children.insert(idx, node);
                structural = true;
            }
            TreeOp::SetProp { id, name, value } => {
                let node =
                    find_by_id_mut(root, id).ok_or_else(|| TreeError::NotFound(id.clone()))?;
                node.props.insert(name.clone(), value.clone());
            }
            TreeOp::BindEvent {
                id,
                event,
                callback,
            } => {
                let node =
                    find_by_id_mut(root, id).ok_or_else(|| TreeError::NotFound(id.clone()))?;
                node.events.insert(event.clone(), *callback);
            }
            TreeOp::ClearEvent { id, event } => {
                let node =
                    find_by_id_mut(root, id).ok_or_else(|| TreeError::NotFound(id.clone()))?;
                node.events.remove(event);
            }
        }
    }

    Ok(structural)
}

fn find_by_id<'a>(node: &'a ComponentSpec, id: &str) -> Option<&'a ComponentSpec> {
    if node.id.as_deref() == Some(id) {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_by_id(child.node.as_ref(), id) {
            return Some(found);
        }
    }
    None
}

fn find_child_node_by_id<'a>(node: &'a ComponentSpec, id: &str) -> Option<&'a ComponentSpec> {
    for child in &node.children {
        if child.node.id.as_deref() == Some(id) {
            return Some(child.node.as_ref());
        }
        if let Some(found) = find_child_node_by_id(child.node.as_ref(), id) {
            return Some(found);
        }
    }
    None
}

fn find_by_id_mut<'a>(node: &'a mut ComponentSpec, id: &str) -> Option<&'a mut ComponentSpec> {
    if node.id.as_deref() == Some(id) {
        return Some(node);
    }
    for child in &mut node.children {
        if let Some(found) = find_by_id_mut(child.node.as_mut(), id) {
            return Some(found);
        }
    }
    None
}

fn remove_by_id(node: &mut ComponentSpec, id: &str) -> bool {
    let mut idx = None;
    for (i, child) in node.children.iter().enumerate() {
        if child.node.id.as_deref() == Some(id) {
            idx = Some(i);
            break;
        }
    }
    if let Some(i) = idx {
        node.children.remove(i);
        return true;
    }

    for child in &mut node.children {
        if remove_by_id(child.node.as_mut(), id) {
            return true;
        }
    }
    false
}

fn replace_by_id(node: &mut ComponentSpec, id: &str, new_node: ComponentSpecChild) -> bool {
    for child in &mut node.children {
        if child.node.id.as_deref() == Some(id) {
            *child = new_node;
            return true;
        }
    }
    for child in &mut node.children {
        if replace_by_id(child.node.as_mut(), id, new_node.clone()) {
            return true;
        }
    }
    false
}

fn take_by_id(node: &mut ComponentSpec, id: &str) -> Option<ComponentSpecChild> {
    let mut idx = None;
    for (i, child) in node.children.iter().enumerate() {
        if child.node.id.as_deref() == Some(id) {
            idx = Some(i);
            break;
        }
    }
    if let Some(i) = idx {
        return Some(node.children.remove(i));
    }

    for child in &mut node.children {
        if let Some(found) = take_by_id(child.node.as_mut(), id) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tree() -> ComponentSpec {
        ComponentSpec::new("VStack")
            .with_id("root")
            .with_child(ComponentSpecChild::new(
                ComponentSpec::new("Label").with_id("a"),
            ))
            .with_child(ComponentSpecChild::new(
                ComponentSpec::new("Label").with_id("b"),
            ))
    }

    #[test]
    fn tree_ops_insert_remove_replace_move() {
        let mut tree = sample_tree();

        let ops = vec![TreeOp::Insert {
            parent_id: "root".to_string(),
            index: 1,
            child: ComponentSpecChild::new(ComponentSpec::new("Label").with_id("c")),
        }];
        assert!(apply_tree_ops(&mut tree, &ops).unwrap());
        assert_eq!(tree.children.len(), 3);
        assert_eq!(tree.children[1].node.id.as_deref(), Some("c"));

        let ops = vec![TreeOp::Remove {
            id: "a".to_string(),
        }];
        assert!(apply_tree_ops(&mut tree, &ops).unwrap());
        assert_eq!(tree.children.len(), 2);

        let ops = vec![TreeOp::Replace {
            id: "b".to_string(),
            node: ComponentSpecChild::new(ComponentSpec::new("Button").with_id("b")),
        }];
        assert!(apply_tree_ops(&mut tree, &ops).unwrap());
        assert_eq!(tree.children[1].node.type_name, "Button");

        let ops = vec![TreeOp::Move {
            id: "c".to_string(),
            new_parent_id: "root".to_string(),
            index: 0,
        }];
        assert!(apply_tree_ops(&mut tree, &ops).unwrap());
        assert_eq!(tree.children[0].node.id.as_deref(), Some("c"));
    }

    #[test]
    fn tree_ops_move_missing_parent_preserves_tree() {
        let mut tree = sample_tree();
        let original = tree.clone();

        let err = apply_tree_ops(
            &mut tree,
            &[TreeOp::Move {
                id: "a".to_string(),
                new_parent_id: "missing".to_string(),
                index: 0,
            }],
        )
        .expect_err("missing parent should fail");

        assert_eq!(err, TreeError::NotFound("missing".to_string()));
        assert_eq!(tree, original);
    }

    #[test]
    fn tree_ops_move_into_descendant_preserves_tree() {
        let mut tree =
            ComponentSpec::new("VStack")
                .with_id("root")
                .with_child(ComponentSpecChild::new(
                    ComponentSpec::new("VStack").with_id("parent").with_child(
                        ComponentSpecChild::new(ComponentSpec::new("Label").with_id("child")),
                    ),
                ));
        let original = tree.clone();

        let err = apply_tree_ops(
            &mut tree,
            &[TreeOp::Move {
                id: "parent".to_string(),
                new_parent_id: "child".to_string(),
                index: 0,
            }],
        )
        .expect_err("moving into a descendant should fail");

        assert!(matches!(err, TreeError::InvalidTreeOp(_)));
        assert_eq!(tree, original);
    }

    #[test]
    fn tree_ops_set_prop_and_event() {
        let mut tree = sample_tree();
        let cb = CallbackId(42);
        let ops = vec![
            TreeOp::SetProp {
                id: "a".to_string(),
                name: "text".to_string(),
                value: ComponentValue::String("hi".into()),
            },
            TreeOp::BindEvent {
                id: "a".to_string(),
                event: "click".to_string(),
                callback: cb,
            },
        ];
        assert!(!apply_tree_ops(&mut tree, &ops).unwrap());
        let node = tree
            .children
            .iter()
            .find(|child| child.node.id.as_deref() == Some("a"))
            .unwrap();
        assert_eq!(
            node.node.props.get("text"),
            Some(&ComponentValue::String("hi".into()))
        );
        assert_eq!(node.node.events.get("click"), Some(&cb));
    }

    #[test]
    fn callback_registry_drains() {
        let registry = CallbackRegistry::new();
        let id = registry.register();
        registry.emit_simple(id, Some("x".into()), "click");
        assert!(!registry.is_empty());
        let events = registry.drain();
        assert_eq!(events.len(), 1);
        assert!(registry.is_empty());
        assert_eq!(events[0].callback_id, id);
    }

    #[test]
    fn component_registry_builds_by_type() {
        let mut registry = ComponentRegistry::<String>::new();
        let schema = ComponentSchema::new("Label");
        registry.register(schema.clone(), |spec, _| {
            Ok(format!(
                "{}:{}",
                spec.type_name,
                spec.id.clone().unwrap_or_default()
            ))
        });

        let spec = ComponentSpec::new("Label").with_id("x");
        let built = registry.build(&spec).expect("build");
        assert_eq!(built, "Label:x");
        assert_eq!(registry.schema("Label"), Some(&schema));
        assert!(registry.schema("Missing").is_none());
    }
}
