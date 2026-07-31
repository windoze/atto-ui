//! Language-neutral runtime specs and tree operation primitives.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
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

/// Allocates callback ids, tracks which have been released, and queues invocations for the host.
///
/// Liveness lives here rather than in each language binding on purpose. A binding that forgets to
/// filter released ids would keep delivering events for callbacks its host has already dropped, and
/// that mistake is invisible from the binding's own code — so the registry that hands out the ids is
/// the only place a fix applies to every binding at once.
///
/// Note this tracks *released* ids rather than registered ones. Ids do not all originate from
/// [`Self::register`]: the Python binding constructs `CallbackId` directly from a Python integer, and
/// a spec deserialized from JSON carries ids in its `events` map that this registry never issued.
/// Treating "not registered" as "not live" would silently drop every one of those callbacks.
#[derive(Clone, Default)]
pub struct CallbackRegistry {
    next_id: Arc<AtomicU64>,
    queue: Arc<Mutex<VecDeque<CallbackInvocation>>>,
    /// Ids explicitly released by the host. Ids are never reused, so this only ever grows with
    /// genuine releases and an unknown id is treated as live.
    released: Arc<Mutex<HashSet<CallbackId>>>,
}

impl CallbackRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self) -> CallbackId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        CallbackId(id)
    }

    /// Releases a callback id so later emissions for it are dropped.
    ///
    /// Returns `true` if the id was live before this call. Also discards any invocations for it that
    /// are already queued but not yet drained, so a release takes effect immediately rather than
    /// letting one more event through.
    pub fn release(&self, callback_id: CallbackId) -> bool {
        let newly_released = self.released.lock().insert(callback_id);
        if newly_released {
            self.queue
                .lock()
                .retain(|invocation| invocation.callback_id != callback_id);
        }
        newly_released
    }

    /// Reports whether a callback id can still receive events.
    ///
    /// Ids this registry never issued count as live; see the type-level note.
    pub fn is_live(&self, callback_id: CallbackId) -> bool {
        !self.released.lock().contains(&callback_id)
    }

    pub fn emit(&self, invocation: CallbackInvocation) {
        if !self.is_live(invocation.callback_id) {
            return;
        }
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
    InsertBefore {
        parent_id: String,
        anchor_id: Option<String>,
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
    ClearProp {
        id: String,
        name: String,
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
    let mut next_root = root.clone();
    let mut index = SpecPathIndex::new(&next_root);
    for op in ops {
        structural |= apply_tree_op(&mut next_root, op, &mut index)?;
    }

    *root = next_root;
    Ok(structural)
}

type SpecPath = Vec<usize>;

struct SpecPathIndex {
    paths: HashMap<String, SpecPath>,
}

impl SpecPathIndex {
    fn new(root: &ComponentSpec) -> Self {
        let mut paths = HashMap::new();
        index_spec_paths(root, &mut Vec::new(), &mut paths);
        Self { paths }
    }

    fn rebuild(&mut self, root: &ComponentSpec) {
        *self = Self::new(root);
    }

    fn path(&self, id: &str) -> Option<&SpecPath> {
        self.paths.get(id)
    }
}

fn index_spec_paths(
    node: &ComponentSpec,
    path: &mut SpecPath,
    paths: &mut HashMap<String, SpecPath>,
) {
    if let Some(id) = &node.id {
        paths.entry(id.clone()).or_insert_with(|| path.clone());
    }
    for (idx, child) in node.children.iter().enumerate() {
        path.push(idx);
        index_spec_paths(child.node.as_ref(), path, paths);
        path.pop();
    }
}

fn apply_tree_op(
    root: &mut ComponentSpec,
    op: &TreeOp,
    index: &mut SpecPathIndex,
) -> Result<bool, TreeError> {
    match op {
        TreeOp::SetTree(spec) => {
            *root = spec.clone();
            index.rebuild(root);
            Ok(true)
        }
        TreeOp::Insert {
            parent_id,
            index: child_index,
            child,
        } => {
            let parent_path = index
                .path(parent_id)
                .cloned()
                .ok_or_else(|| TreeError::NotFound(parent_id.clone()))?;
            let parent = spec_at_path_mut(root, &parent_path)
                .ok_or_else(|| TreeError::NotFound(parent_id.clone()))?;
            let idx = (*child_index).min(parent.children.len());
            parent.children.insert(idx, child.clone());
            index.rebuild(root);
            Ok(true)
        }
        TreeOp::InsertBefore {
            parent_id,
            anchor_id,
            child,
        } => {
            insert_child_before_anchor(root, index, parent_id, anchor_id.as_deref(), child)?;
            Ok(true)
        }
        TreeOp::Remove { id } => {
            let path = index
                .path(id)
                .filter(|path| !path.is_empty())
                .cloned()
                .ok_or_else(|| TreeError::NotFound(id.clone()))?;
            remove_child_at_path(root, &path).ok_or_else(|| TreeError::NotFound(id.clone()))?;
            index.rebuild(root);
            Ok(true)
        }
        TreeOp::Replace { id, node } => {
            let path = index
                .path(id)
                .filter(|path| !path.is_empty())
                .cloned()
                .ok_or_else(|| TreeError::NotFound(id.clone()))?;
            let slot =
                child_at_path_mut(root, &path).ok_or_else(|| TreeError::NotFound(id.clone()))?;
            *slot = node.clone();
            index.rebuild(root);
            Ok(true)
        }
        TreeOp::Move {
            id,
            new_parent_id,
            index: child_index,
        } => {
            let moving_path = index
                .path(id)
                .filter(|path| !path.is_empty())
                .cloned()
                .ok_or_else(|| TreeError::NotFound(id.clone()))?;
            let target_path = index
                .path(new_parent_id)
                .cloned()
                .ok_or_else(|| TreeError::NotFound(new_parent_id.clone()))?;
            if target_path.starts_with(&moving_path) {
                return Err(TreeError::InvalidTreeOp(
                    "cannot move node into itself or descendant".to_string(),
                ));
            }

            let node =
                remove_child_at_path(root, &moving_path).expect("validated movable child exists");
            index.rebuild(root);
            let parent_path = index
                .path(new_parent_id)
                .cloned()
                .expect("validated target parent exists outside moved subtree");
            let parent = spec_at_path_mut(root, &parent_path)
                .expect("validated target parent path resolves");
            let idx = (*child_index).min(parent.children.len());
            parent.children.insert(idx, node);
            index.rebuild(root);
            Ok(true)
        }
        TreeOp::SetProp { id, name, value } => {
            let path = index
                .path(id)
                .cloned()
                .ok_or_else(|| TreeError::NotFound(id.clone()))?;
            let node =
                spec_at_path_mut(root, &path).ok_or_else(|| TreeError::NotFound(id.clone()))?;
            node.props.insert(name.clone(), value.clone());
            Ok(false)
        }
        TreeOp::ClearProp { id, name } => {
            let path = index
                .path(id)
                .cloned()
                .ok_or_else(|| TreeError::NotFound(id.clone()))?;
            let node =
                spec_at_path_mut(root, &path).ok_or_else(|| TreeError::NotFound(id.clone()))?;
            node.props.remove(name);
            Ok(false)
        }
        TreeOp::BindEvent {
            id,
            event,
            callback,
        } => {
            let path = index
                .path(id)
                .cloned()
                .ok_or_else(|| TreeError::NotFound(id.clone()))?;
            let node =
                spec_at_path_mut(root, &path).ok_or_else(|| TreeError::NotFound(id.clone()))?;
            node.events.insert(event.clone(), *callback);
            Ok(false)
        }
        TreeOp::ClearEvent { id, event } => {
            let path = index
                .path(id)
                .cloned()
                .ok_or_else(|| TreeError::NotFound(id.clone()))?;
            let node =
                spec_at_path_mut(root, &path).ok_or_else(|| TreeError::NotFound(id.clone()))?;
            node.events.remove(event);
            Ok(false)
        }
    }
}

fn spec_at_path_mut<'a>(
    mut node: &'a mut ComponentSpec,
    path: &[usize],
) -> Option<&'a mut ComponentSpec> {
    for &idx in path {
        node = node.children.get_mut(idx)?.node.as_mut();
    }
    Some(node)
}

fn child_at_path_mut<'a>(
    root: &'a mut ComponentSpec,
    path: &[usize],
) -> Option<&'a mut ComponentSpecChild> {
    let (&idx, parent_path) = path.split_last()?;
    let parent = spec_at_path_mut(root, parent_path)?;
    parent.children.get_mut(idx)
}

fn remove_child_at_path(root: &mut ComponentSpec, path: &[usize]) -> Option<ComponentSpecChild> {
    let (&idx, parent_path) = path.split_last()?;
    let parent = spec_at_path_mut(root, parent_path)?;
    if idx < parent.children.len() {
        Some(parent.children.remove(idx))
    } else {
        None
    }
}

fn insert_child_before_anchor(
    root: &mut ComponentSpec,
    index: &mut SpecPathIndex,
    parent_id: &str,
    anchor_id: Option<&str>,
    child: &ComponentSpecChild,
) -> Result<(), TreeError> {
    if let Some(child_id) = child.node.id.as_deref()
        && let Some(existing_path) = index.path(child_id).cloned()
    {
        if existing_path.is_empty() {
            return Err(TreeError::InvalidTreeOp(
                "cannot move root node".to_string(),
            ));
        }
        let target_path = index
            .path(parent_id)
            .cloned()
            .ok_or_else(|| TreeError::NotFound(parent_id.to_string()))?;
        if target_path.starts_with(&existing_path) {
            return Err(TreeError::InvalidTreeOp(
                "cannot move node into itself or descendant".to_string(),
            ));
        }
        if anchor_id == Some(child_id) {
            let current_parent_path = &existing_path[..existing_path.len() - 1];
            if target_path.as_slice() == current_parent_path {
                return Ok(());
            }
            return Err(TreeError::NotFound(child_id.to_string()));
        }

        let node =
            remove_child_at_path(root, &existing_path).expect("validated movable child exists");
        index.rebuild(root);
        insert_detached_child_before_anchor(root, index, parent_id, anchor_id, node)?;
        index.rebuild(root);
        return Ok(());
    }

    insert_detached_child_before_anchor(root, index, parent_id, anchor_id, child.clone())?;
    index.rebuild(root);
    Ok(())
}

fn insert_detached_child_before_anchor(
    root: &mut ComponentSpec,
    index: &SpecPathIndex,
    parent_id: &str,
    anchor_id: Option<&str>,
    child: ComponentSpecChild,
) -> Result<(), TreeError> {
    let parent_path = index
        .path(parent_id)
        .cloned()
        .ok_or_else(|| TreeError::NotFound(parent_id.to_string()))?;
    let parent = spec_at_path_mut(root, &parent_path)
        .ok_or_else(|| TreeError::NotFound(parent_id.into()))?;
    let idx = child_index_before_anchor(&parent.children, anchor_id)?;
    parent.children.insert(idx, child);
    Ok(())
}

fn child_index_before_anchor(
    children: &[ComponentSpecChild],
    anchor_id: Option<&str>,
) -> Result<usize, TreeError> {
    let Some(anchor_id) = anchor_id else {
        return Ok(children.len());
    };
    children
        .iter()
        .position(|child| child.node.id.as_deref() == Some(anchor_id))
        .ok_or_else(|| TreeError::NotFound(anchor_id.to_string()))
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

    fn child_ids(node: &ComponentSpec) -> Vec<Option<&str>> {
        node.children
            .iter()
            .map(|child| child.node.id.as_deref())
            .collect()
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
    fn tree_ops_insert_before_appends_or_inserts_before_anchor() {
        let mut tree = sample_tree();

        assert!(
            apply_tree_ops(
                &mut tree,
                &[TreeOp::InsertBefore {
                    parent_id: "root".into(),
                    anchor_id: None,
                    child: ComponentSpecChild::new(ComponentSpec::new("Label").with_id("c")),
                }],
            )
            .unwrap()
        );
        assert_eq!(child_ids(&tree), vec![Some("a"), Some("b"), Some("c")]);

        assert!(
            apply_tree_ops(
                &mut tree,
                &[TreeOp::InsertBefore {
                    parent_id: "root".into(),
                    anchor_id: Some("b".into()),
                    child: ComponentSpecChild::new(ComponentSpec::new("Label").with_id("x")),
                }],
            )
            .unwrap()
        );
        assert_eq!(
            child_ids(&tree),
            vec![Some("a"), Some("x"), Some("b"), Some("c")]
        );
    }

    #[test]
    fn tree_ops_insert_before_existing_child_moves_after_detach() {
        let mut tree = ComponentSpec::new("VStack")
            .with_id("root")
            .with_child(ComponentSpecChild::new(
                ComponentSpec::new("Label").with_id("a"),
            ))
            .with_child(ComponentSpecChild::new(
                ComponentSpec::new("Label").with_id("b"),
            ))
            .with_child(ComponentSpecChild::new(
                ComponentSpec::new("Label").with_id("c"),
            ));

        assert!(
            apply_tree_ops(
                &mut tree,
                &[TreeOp::InsertBefore {
                    parent_id: "root".into(),
                    anchor_id: Some("c".into()),
                    child: ComponentSpecChild::new(ComponentSpec::new("Label").with_id("a")),
                }],
            )
            .unwrap()
        );
        assert_eq!(child_ids(&tree), vec![Some("b"), Some("a"), Some("c")]);
    }

    #[test]
    fn tree_ops_insert_before_rejects_move_into_descendant() {
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
            &[TreeOp::InsertBefore {
                parent_id: "child".into(),
                anchor_id: None,
                child: ComponentSpecChild::new(ComponentSpec::new("VStack").with_id("parent")),
            }],
        )
        .expect_err("moving into a descendant should fail");

        assert!(matches!(err, TreeError::InvalidTreeOp(_)));
        assert_eq!(tree, original);
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
    fn tree_ops_path_index_tracks_shifted_paths_within_batch() {
        let mut tree = ComponentSpec::new("VStack")
            .with_id("root")
            .with_child(ComponentSpecChild::new(
                ComponentSpec::new("Label").with_id("a"),
            ))
            .with_child(ComponentSpecChild::new(
                ComponentSpec::new("Label").with_id("b"),
            ))
            .with_child(ComponentSpecChild::new(
                ComponentSpec::new("Label").with_id("c"),
            ));

        let ops = vec![
            TreeOp::Remove { id: "a".into() },
            TreeOp::SetProp {
                id: "c".into(),
                name: "text".into(),
                value: ComponentValue::String("updated".into()),
            },
            TreeOp::Replace {
                id: "b".into(),
                node: ComponentSpecChild::new(ComponentSpec::new("Button").with_id("b")),
            },
            TreeOp::Move {
                id: "c".into(),
                new_parent_id: "root".into(),
                index: 0,
            },
        ];

        assert!(apply_tree_ops(&mut tree, &ops).unwrap());
        let ids: Vec<Option<&str>> = tree
            .children
            .iter()
            .map(|child| child.node.id.as_deref())
            .collect();
        assert_eq!(ids, vec![Some("c"), Some("b")]);
        assert_eq!(
            tree.children[0].node.props.get("text"),
            Some(&ComponentValue::String("updated".into()))
        );
        assert_eq!(tree.children[1].node.type_name, "Button");
    }

    #[test]
    fn tree_ops_batch_failure_preserves_tree() {
        let mut tree = sample_tree();
        let original = tree.clone();

        let err = apply_tree_ops(
            &mut tree,
            &[
                TreeOp::Insert {
                    parent_id: "root".into(),
                    index: 0,
                    child: ComponentSpecChild::new(ComponentSpec::new("Label").with_id("c")),
                },
                TreeOp::SetProp {
                    id: "missing".into(),
                    name: "text".into(),
                    value: ComponentValue::String("updated".into()),
                },
            ],
        )
        .expect_err("missing target should fail");

        assert_eq!(err, TreeError::NotFound("missing".into()));
        assert_eq!(tree, original);
    }

    #[test]
    fn tree_ops_set_clear_prop_and_event() {
        let mut tree = sample_tree();
        let cb = CallbackId(42);
        let ops = vec![
            TreeOp::SetProp {
                id: "a".to_string(),
                name: "text".to_string(),
                value: ComponentValue::String("hi".into()),
            },
            TreeOp::ClearProp {
                id: "a".to_string(),
                name: "text".to_string(),
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
        assert_eq!(node.node.props.get("text"), None);
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
