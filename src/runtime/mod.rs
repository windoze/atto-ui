//! Runtime integration layer.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use crossterm::event::Event;
use parking_lot::Mutex;
use ratatui::Frame;
use ratatui::layout::Rect;

use atto_ui_runtime::{
    ActionMeta, AlignSpec, AnchorPlacementSpec, AnchorSpec, CallbackId, CallbackInvocation,
    CallbackRegistry, ComponentRegistry, ComponentSchema, ComponentSpec, ComponentSpecChild,
    ComponentValue, EdgeInsetsSpec, EventMeta, LayoutSpec, SizeSpec, TreeError, TreeOp, ValueType,
    apply_tree_ops,
};

use crate::composable::{
    Align, Anchor, AnchorPlacement, Border, Component, ComponentContext, ComponentId,
    ComponentNode, Divider, DividerOrientation, EdgeInsets, EventResult, Grid, HStack, Label,
    LayoutParams, ScrollConfig, Size, Spacer, Splitter, SplitterOrientation, TabView, Text,
    TextBox, TitleBarContent, TitleBarContext, VStack, Visibility,
};
use crate::reactive::Binding;
use crate::widgets::{
    Button, Checkbox, ListBox, ProgressBar, RadioGroup, Slider, Spinner, StyledLabel,
    TabHeaderPosition, TableView,
};
use crate::{ComponentError, ComponentPropertySchema};

#[derive(Clone)]
pub struct CallbackHandle {
    registry: CallbackRegistry,
    callback_id: CallbackId,
    target_id: Option<String>,
    event: String,
}

impl CallbackHandle {
    pub fn new(
        registry: CallbackRegistry,
        callback_id: CallbackId,
        target_id: Option<String>,
        event: impl Into<String>,
    ) -> Self {
        Self {
            registry,
            callback_id,
            target_id,
            event: event.into(),
        }
    }

    pub fn emit(&self) {
        self.emit_with(None);
    }

    pub fn emit_with(&self, payload: Option<ComponentValue>) {
        self.registry.emit(CallbackInvocation {
            callback_id: self.callback_id,
            target_id: self.target_id.clone(),
            event: self.event.clone(),
            payload,
        });
    }
}

impl fmt::Debug for CallbackHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallbackHandle")
            .field("callback_id", &self.callback_id)
            .field("target_id", &self.target_id)
            .field("event", &self.event)
            .finish()
    }
}

/// 全局组件注册表扩展。
///
/// 通过扩展注册，可以让上层 crate（例如 `atto-ui-file-tree`）在不修改基础框架的情况下
/// 将自己的组件注册到动态系统中。
pub type RegistryExtension = fn(&mut ComponentRegistry<Box<dyn Component>>, CallbackRegistry);

static REGISTRY_EXTENSIONS: OnceLock<Mutex<BTreeMap<String, RegistryExtension>>> = OnceLock::new();

fn registry_extensions() -> &'static Mutex<BTreeMap<String, RegistryExtension>> {
    REGISTRY_EXTENSIONS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// 注册一个全局组件注册表扩展（幂等）。
///
/// 返回：
/// - `true`：本次注册成功（此前没有同名扩展）
/// - `false`：已存在同名扩展，本次调用不会覆盖
pub fn register_registry_extension(name: impl Into<String>, register: RegistryExtension) -> bool {
    let name = name.into();
    let mut guard = registry_extensions().lock();
    if guard.contains_key(&name) {
        return false;
    }
    guard.insert(name, register);
    true
}

/// 动态组件注册表：内置组件 + 全局扩展组件。
pub fn global_registry(callbacks: CallbackRegistry) -> ComponentRegistry<Box<dyn Component>> {
    let mut registry = builtin_registry(callbacks.clone());
    let extensions = {
        let guard = registry_extensions().lock();
        guard.values().copied().collect::<Vec<_>>()
    };
    for register in extensions {
        register(&mut registry, callbacks.clone());
    }
    registry
}

pub fn builtin_registry(callbacks: CallbackRegistry) -> ComponentRegistry<Box<dyn Component>> {
    let mut registry = ComponentRegistry::new();

    register_button(&mut registry, callbacks.clone());
    register_checkbox(&mut registry, callbacks.clone());
    register_label(&mut registry);
    register_styled_label(&mut registry, callbacks.clone());
    register_text(&mut registry);
    register_textbox(&mut registry, callbacks.clone());
    register_slider(&mut registry, callbacks.clone());
    register_progress_bar(&mut registry);
    register_radio_group(&mut registry, callbacks.clone());
    register_list_box(&mut registry, callbacks.clone());
    register_table_view(&mut registry, callbacks.clone());
    register_spinner(&mut registry);
    register_tab_view(&mut registry, callbacks.clone());
    register_stack::<VStack>(&mut registry, "VStack", StackAxis::Vertical);
    register_stack::<HStack>(&mut registry, "HStack", StackAxis::Horizontal);
    register_grid(&mut registry);
    register_splitter(&mut registry);
    register_divider(&mut registry);
    register_spacer(&mut registry);
    register_border(&mut registry);
    register_visibility(&mut registry);

    registry
}

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
        apply_tree_ops(&mut self.root, ops)
    }

    pub fn rebuild(&mut self) -> Result<(), TreeError> {
        self.view = self.registry.build(&self.root)?;
        Ok(())
    }

    pub fn apply_ops_and_rebuild(&mut self, ops: &[TreeOp]) -> Result<(), TreeError> {
        apply_tree_ops(&mut self.root, ops)?;
        self.rebuild()
    }

    pub fn apply_ops_incremental(&mut self, ops: &[TreeOp]) -> Result<bool, TreeError> {
        let has_set_tree = ops.iter().any(|op| matches!(op, TreeOp::SetTree(_)));
        apply_tree_ops(&mut self.root, ops)?;
        if has_set_tree {
            self.rebuild()?;
            return Ok(true);
        }

        let mut structural = false;
        for op in ops {
            match op {
                TreeOp::SetTree(_) => {}
                TreeOp::SetProp { id, name, value } => {
                    if id_matches_root(&self.view, id) {
                        let applied = apply_property_to_view(self.view.as_mut(), id, name, value)?;
                        if applied == PropertyApply::NeedsRebuild {
                            self.rebuild()?;
                            return Ok(true);
                        }
                        if applied == PropertyApply::NotFound {
                            self.rebuild()?;
                            return Ok(true);
                        }
                        continue;
                    }

                    match apply_property_to_view(self.view.as_mut(), id, name, value)? {
                        PropertyApply::Applied => {}
                        PropertyApply::NeedsRebuild => {
                            if !replace_node_with_spec(
                                self.view.as_mut(),
                                id,
                                &self.root,
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
                    if !replace_node_with_spec(self.view.as_mut(), id, &self.root, &self.registry)?
                    {
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

    fn tag(&self) -> Option<&str> {
        self.view.tag()
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

    fn children(&self) -> &[ComponentNode] {
        self.view.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        self.view.children_mut()
    }

    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.view.handle_event_capture(event, ctx)
    }

    fn handle_event_bubble(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.view.handle_event_bubble(event, ctx)
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.view.handle_event(event, ctx)
    }

    fn titlebar(&mut self, ctx: TitleBarContext<'_>) -> Option<TitleBarContent> {
        self.view.titlebar(ctx)
    }

    fn handle_titlebar_event(&mut self, event: &Event, ctx: TitleBarContext<'_>) -> EventResult {
        self.view.handle_titlebar_event(event, ctx)
    }

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

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.view.draw(frame, area, ctx);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropertyApply {
    Applied,
    NeedsRebuild,
    NotFound,
}

fn id_matches_root(view: &dyn Component, id: &str) -> bool {
    view.tag().is_some_and(|view_id| view_id == id)
}

fn apply_property_to_view(
    view: &mut dyn Component,
    id: &str,
    name: &str,
    value: &ComponentValue,
) -> Result<PropertyApply, TreeError> {
    if view.tag() == Some(id) {
        return match view.set_property(name, value.clone()) {
            Ok(()) => Ok(PropertyApply::Applied),
            Err(ComponentError::UnsupportedProperty(_)) => Ok(PropertyApply::NeedsRebuild),
            Err(ComponentError::NotFound(_)) => Ok(PropertyApply::NeedsRebuild),
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

fn is_tab_view(view: &dyn Component) -> bool {
    view.type_name()
        .rsplit("::")
        .next()
        .is_some_and(|name| name == "TabView")
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
    let tab_view = is_tab_view(view);
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
        if is_tab_view(view) {
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
    let tab_view = is_tab_view(view);
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

fn move_node(view: &mut dyn Component, id: &str, new_parent_id: &str, index: usize) -> bool {
    let Some(node) = take_node(view, id) else {
        return false;
    };
    let mut node = Some(node);
    if insert_existing_node(view, new_parent_id, index, &mut node) {
        return node.is_none();
    }
    false
}

fn take_node(view: &mut dyn Component, id: &str) -> Option<ComponentNode> {
    let tab_view = is_tab_view(view);
    let children = view.children_mut()?;
    if !tab_view {
        if let Some(idx) = children
            .iter()
            .position(|child| child.view.tag() == Some(id))
        {
            return Some(children.remove(idx));
        }
    } else if children.iter().any(|child| child.view.tag() == Some(id)) {
        return None;
    }

    for child in children.iter_mut() {
        if let Some(found) = take_node(child.view.as_mut(), id) {
            return Some(found);
        }
    }
    None
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
        if is_tab_view(view) {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StackAxis {
    Vertical,
    Horizontal,
}

pub fn component_schema<T: ComponentPropertySchema>(type_name: &str) -> ComponentSchema {
    let mut schema = ComponentSchema::new(type_name).with_properties(T::property_schema());
    schema.dedup_properties();
    schema
}

fn register_button(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<Button>("Button")
        .with_event(EventMeta::new("click"))
        .with_action(ActionMeta::new("click"))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let label = prop_string(spec, "label")?.unwrap_or_else(|| "Button".to_string());
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let mut button = Button::new(label).enabled(enabled);
        if let Some(cb) = event_handle(spec, "click", callbacks.clone()) {
            button = button.on_click_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(button)))
    });
}

fn register_checkbox(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<Checkbox>("Checkbox")
        .with_event(EventMeta::new("change"))
        .with_action(ActionMeta::new("toggle"))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let label = prop_string(spec, "label")?.unwrap_or_default();
        let checked = prop_bool(spec, "checked")?.unwrap_or(false);
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let mut checkbox = Checkbox::new(label, Binding::new(checked)).enabled(enabled);
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            checkbox = checkbox.on_change_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(checkbox)))
    });
}

fn register_label(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Label>("Label").allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let text = prop_string(spec, "text")?.unwrap_or_default();
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let label = Label::new(text).enabled(enabled);
        Ok(wrap_with_id(spec, Box::new(label)))
    });
}

fn register_styled_label(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<StyledLabel>("StyledLabel")
        .with_event(EventMeta::new("link").with_payload(ValueType::String))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let text = prop_string(spec, "text")?.unwrap_or_default();
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let mut label = StyledLabel::new(text).enabled(enabled);
        if let Some(cb) = event_handle(spec, "link", callbacks.clone()) {
            label = label.on_link_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(label)))
    });
}

fn register_text(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Text>("Text").allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let text = prop_string(spec, "text")?.unwrap_or_default();
        let view = Text::new(text);
        Ok(wrap_with_id(spec, Box::new(view)))
    });
}

fn register_textbox(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<TextBox>("TextBox")
        .with_event(EventMeta::new("change"))
        .with_event(EventMeta::new("submit"))
        .with_action(ActionMeta::new("input_text").with_payload(ValueType::String))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let title = prop_string(spec, "title")?.unwrap_or_default();
        let text = prop_string(spec, "text")?.unwrap_or_default();
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let clipboard = prop_string(spec, "clipboard")?.unwrap_or_default();
        let placeholder = prop_string(spec, "placeholder")?;

        let mut textbox = TextBox::new(title, Binding::new(text))
            .enabled(enabled)
            .clipboard(clipboard);
        if let Some(value) = placeholder {
            textbox = textbox.placeholder(value);
        }
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            textbox = textbox.on_change_callback(cb);
        }
        if let Some(cb) = event_handle(spec, "submit", callbacks.clone()) {
            textbox = textbox.on_submit_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(textbox)))
    });
}

fn register_slider(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<Slider>("Slider")
        .with_event(EventMeta::new("change"))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let min = prop_f64(spec, "min")?.unwrap_or(0.0);
        let max = prop_f64(spec, "max")?.unwrap_or(1.0);
        let value = prop_f64(spec, "value")?.unwrap_or(min);
        let step = prop_f64(spec, "step")?.unwrap_or(1.0);
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let mut slider = Slider::new(min, max, Binding::new(value))
            .step(step)
            .enabled(enabled);
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            slider = slider.on_change_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(slider)))
    });
}

fn register_progress_bar(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<ProgressBar>("ProgressBar").allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let min = prop_f64(spec, "min")?.unwrap_or(0.0);
        let max = prop_f64(spec, "max")?.unwrap_or(1.0);
        let value = prop_f64(spec, "value")?.unwrap_or(min);
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let show_text = prop_bool(spec, "show_text")?.unwrap_or(false);
        let text = prop_string(spec, "text")?;
        let mut bar = ProgressBar::new(min, max, Binding::new(value))
            .enabled(enabled)
            .show_text(show_text);
        if let Some(text) = text {
            bar = bar.text(text);
        }
        Ok(wrap_with_id(spec, Box::new(bar)))
    });
}

fn register_radio_group(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<RadioGroup>("RadioGroup")
        .with_event(EventMeta::new("change"))
        .with_action(ActionMeta::new("select_index").with_payload(ValueType::U64))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let label = prop_string(spec, "label")?.unwrap_or_default();
        let options = prop_vec_string(spec, "options")?.unwrap_or_default();
        let selection = prop_usize(spec, "selection")?.unwrap_or(0);
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let height = prop_u16(spec, "height")?;

        let mut radio =
            RadioGroup::new(label, Binding::new(options), Binding::new(selection)).enabled(enabled);
        if let Some(height) = height {
            radio = radio.height(height);
        }
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            radio = radio.on_change_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(radio)))
    });
}

fn register_list_box(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<ListBox>("ListBox")
        .with_event(EventMeta::new("change"))
        .with_action(ActionMeta::new("select_index").with_payload(ValueType::U64))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let title = prop_string(spec, "title")?.unwrap_or_default();
        let items = prop_vec_string(spec, "items")?.unwrap_or_default();
        let selection = prop_usize(spec, "selection")?.unwrap_or(0);
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let height = prop_u16(spec, "height")?;

        let mut list =
            ListBox::new(title, Binding::new(items), Binding::new(selection)).enabled(enabled);
        if let Some(height) = height {
            list = list.height(height);
        }
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            list = list.on_change_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(list)))
    });
}

fn register_table_view(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<TableView>("TableView")
        .with_event(EventMeta::new("change"))
        .with_action(ActionMeta::new("select_index").with_payload(ValueType::U64))
        .allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let title = prop_string(spec, "title")?.unwrap_or_default();
        let headers = prop_vec_string(spec, "headers")?.unwrap_or_default();
        let rows = prop_table(spec, "rows")?.unwrap_or_default();
        let selection = prop_usize(spec, "selection")?.unwrap_or(0);
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let height = prop_u16(spec, "height")?;

        let mut table = TableView::new(
            title,
            Binding::new(headers),
            Binding::new(rows),
            Binding::new(selection),
        )
        .enabled(enabled);
        if let Some(height) = height {
            table = table.height(height);
        }
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            table = table.on_change_callback(cb);
        }
        Ok(wrap_with_id(spec, Box::new(table)))
    });
}

fn register_spinner(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Spinner>("Spinner").allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let text = prop_string(spec, "text")?.unwrap_or_default();
        let enabled = prop_bool(spec, "enabled")?.unwrap_or(true);
        let running = prop_bool(spec, "running")?.unwrap_or(true);
        let spinner = Spinner::new(text).enabled(enabled).running(running);
        Ok(wrap_with_id(spec, Box::new(spinner)))
    });
}

fn register_tab_view(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    callbacks: CallbackRegistry,
) {
    let schema = component_schema::<TabView>("TabView")
        .with_event(EventMeta::new("change"))
        .with_action(ActionMeta::new("select_index").with_payload(ValueType::U64))
        .allow_children(true);

    registry.register(schema, move |spec, registry| {
        let selection = prop_usize(spec, "selection")?.unwrap_or(0);
        let header_position = prop_string(spec, "header_position")?
            .and_then(|value| TabHeaderPosition::parse(&value))
            .unwrap_or(TabHeaderPosition::Top);

        let mut tabs = TabView::new()
            .selection(Binding::new(selection))
            .header_position(header_position);
        if let Some(cb) = event_handle(spec, "change", callbacks.clone()) {
            tabs = tabs.on_change_callback(cb);
        }

        for (idx, child) in spec.children.iter().enumerate() {
            let title = child
                .meta
                .get("title")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Tab{}", idx + 1));
            let view = registry.build(&child.node)?;
            tabs.add_tab(title, view);
        }

        Ok(wrap_with_id(spec, Box::new(tabs)))
    });
}

fn register_stack<T: StackBuilder + Component + ComponentPropertySchema + 'static>(
    registry: &mut ComponentRegistry<Box<dyn Component>>,
    name: &str,
    axis: StackAxis,
) {
    let schema = component_schema::<T>(name).allow_children(true);

    registry.register(schema, move |spec, registry| {
        let spacing = prop_u16(spec, "spacing")?.unwrap_or(0);
        let padding = prop_edge_insets(spec, "padding")?.unwrap_or(EdgeInsets::ZERO);
        let scrollable = prop_bool(spec, "scrollable")?.unwrap_or(false);
        let mut stack = match axis {
            StackAxis::Vertical => T::new().with_spacing(spacing).with_padding(padding),
            StackAxis::Horizontal => T::new().with_spacing(spacing).with_padding(padding),
        };
        if scrollable {
            stack = stack.with_scrollable(scrollable);
        }

        for child in &spec.children {
            let view = registry.build(&child.node)?;
            let layout = child
                .layout
                .as_ref()
                .map(layout_from_spec)
                .unwrap_or_default();
            stack.add_child_with_layout(view, layout);
        }

        Ok(wrap_with_id(spec, Box::new(stack)))
    });
}

fn register_grid(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Grid>("Grid").allow_children(true);

    registry.register(schema, move |spec, registry| {
        let columns = prop_usize(spec, "columns")?.unwrap_or(1);
        let row_gap = prop_u16(spec, "row_gap")?.unwrap_or(0);
        let column_gap = prop_u16(spec, "column_gap")?.unwrap_or(0);
        let padding = prop_edge_insets(spec, "padding")?.unwrap_or(EdgeInsets::ZERO);
        let scrollable = prop_bool(spec, "scrollable")?.unwrap_or(false);

        let mut grid = Grid::new()
            .with_columns(columns)
            .with_row_gap(row_gap)
            .with_column_gap(column_gap)
            .with_padding(padding)
            .with_scrollable(scrollable);

        for child in &spec.children {
            let view = registry.build(&child.node)?;
            let layout = child
                .layout
                .as_ref()
                .map(layout_from_spec)
                .unwrap_or_default();
            grid.add_child_with_layout(view, layout);
        }

        Ok(wrap_with_id(spec, Box::new(grid)))
    });
}

fn register_splitter(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Splitter>("Splitter").allow_children(true);

    registry.register(schema, move |spec, registry| {
        let orientation = prop_string(spec, "orientation")?
            .and_then(|value| SplitterOrientation::parse(&value))
            .unwrap_or(SplitterOrientation::Vertical);

        let first = spec
            .children
            .first()
            .map(|child| registry.build(&child.node))
            .transpose()?
            .unwrap_or_else(|| Box::new(Spacer::new()));
        let second = spec
            .children
            .get(1)
            .map(|child| registry.build(&child.node))
            .transpose()?
            .unwrap_or_else(|| Box::new(Spacer::new()));

        let mut splitter = Splitter::new(orientation, first, second);

        if let Some(split_pos) = prop_u16(spec, "split_pos")? {
            splitter.set_split_position(split_pos);
        }
        if let Some(min_first) = prop_u16(spec, "min_first")? {
            splitter = splitter.min_first(min_first);
        }
        if let Some(min_second) = prop_u16(spec, "min_second")? {
            splitter = splitter.min_second(min_second);
        }
        let border = prop_bool(spec, "border")?.unwrap_or(true);
        splitter = splitter.with_border(border);

        Ok(wrap_with_id(spec, Box::new(splitter)))
    });
}

fn register_divider(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Divider>("Divider").allow_children(false);

    registry.register(schema, move |spec, _registry| {
        let orientation = prop_string(spec, "orientation")?
            .and_then(|value| DividerOrientation::parse(&value))
            .unwrap_or(DividerOrientation::Horizontal);
        let view = Divider::new(orientation);
        Ok(wrap_with_id(spec, Box::new(view)))
    });
}

fn register_spacer(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Spacer>("Spacer").allow_children(false);

    registry.register(schema, move |spec, _registry| {
        Ok(wrap_with_id(spec, Box::new(Spacer::new())))
    });
}

fn register_border(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Border>("Border").allow_children(true);

    registry.register(schema, move |spec, registry| {
        let inner = spec
            .children
            .first()
            .map(|child| registry.build(&child.node))
            .transpose()?
            .unwrap_or_else(|| Box::new(Spacer::new()));
        let border = prop_bool(spec, "border")?.unwrap_or(true);
        let view = Border::new(inner).with_border(border);
        Ok(wrap_with_id(spec, Box::new(view)))
    });
}

fn register_visibility(registry: &mut ComponentRegistry<Box<dyn Component>>) {
    let schema = component_schema::<Visibility>("Visibility").allow_children(true);

    registry.register(schema, move |spec, registry| {
        let visible = prop_bool(spec, "visible")?.unwrap_or(true);
        let inner = spec
            .children
            .first()
            .map(|child| registry.build(&child.node))
            .transpose()?
            .unwrap_or_else(|| Box::new(Spacer::new()));
        let view = Visibility::new(Binding::new(visible), inner);
        Ok(wrap_with_id(spec, Box::new(view)))
    });
}

pub fn wrap_with_id(spec: &ComponentSpec, view: Box<dyn Component>) -> Box<dyn Component> {
    match &spec.id {
        Some(id) => Box::new(crate::composable::ComponentTag::boxed(id.clone(), view)),
        None => view,
    }
}

pub fn event_handle(
    spec: &ComponentSpec,
    name: &str,
    callbacks: CallbackRegistry,
) -> Option<CallbackHandle> {
    let callback = spec.events.get(name).copied()?;
    Some(CallbackHandle::new(
        callbacks,
        callback,
        spec.id.clone(),
        name.to_string(),
    ))
}

fn layout_from_spec(spec: &LayoutSpec) -> LayoutParams {
    LayoutParams {
        width: size_from_spec(spec.width),
        height: size_from_spec(spec.height),
        margin: edge_insets_from_spec(spec.margin),
        align_x: align_from_spec(spec.align_x),
        align_y: align_from_spec(spec.align_y),
        anchor: spec.anchor.map(anchor_from_spec),
        tab_index: spec.tab_index,
    }
}

fn size_from_spec(spec: SizeSpec) -> Size {
    match spec {
        SizeSpec::Fill => Size::Fill,
        SizeSpec::Fixed(v) => Size::Fixed(v),
        SizeSpec::Weight(v) => Size::Weight(v),
        SizeSpec::Content => Size::Content,
    }
}

fn align_from_spec(spec: AlignSpec) -> Align {
    match spec {
        AlignSpec::Start => Align::Start,
        AlignSpec::Center => Align::Center,
        AlignSpec::End => Align::End,
        AlignSpec::Stretch => Align::Stretch,
    }
}

fn anchor_from_spec(spec: AnchorPlacementSpec) -> AnchorPlacement {
    AnchorPlacement {
        anchor: anchor_kind_from_spec(spec.anchor),
        offset_x: spec.offset_x,
        offset_y: spec.offset_y,
    }
}

fn anchor_kind_from_spec(spec: AnchorSpec) -> Anchor {
    match spec {
        AnchorSpec::TopLeft => Anchor::TopLeft,
        AnchorSpec::TopRight => Anchor::TopRight,
        AnchorSpec::BottomLeft => Anchor::BottomLeft,
        AnchorSpec::BottomRight => Anchor::BottomRight,
        AnchorSpec::Top => Anchor::Top,
        AnchorSpec::Bottom => Anchor::Bottom,
        AnchorSpec::Left => Anchor::Left,
        AnchorSpec::Right => Anchor::Right,
        AnchorSpec::Center => Anchor::Center,
    }
}

fn edge_insets_from_spec(spec: EdgeInsetsSpec) -> EdgeInsets {
    EdgeInsets {
        top: spec.top,
        right: spec.right,
        bottom: spec.bottom,
        left: spec.left,
    }
}

pub fn prop_string(spec: &ComponentSpec, name: &str) -> Result<Option<String>, TreeError> {
    match spec.props.get(name) {
        Some(ComponentValue::String(v)) => Ok(Some(v.clone())),
        Some(other) => Err(invalid_prop(spec, name, "string", other)),
        None => Ok(None),
    }
}

pub fn prop_bool(spec: &ComponentSpec, name: &str) -> Result<Option<bool>, TreeError> {
    match spec.props.get(name) {
        Some(ComponentValue::Bool(v)) => Ok(Some(*v)),
        Some(other) => Err(invalid_prop(spec, name, "bool", other)),
        None => Ok(None),
    }
}

pub fn prop_u16(spec: &ComponentSpec, name: &str) -> Result<Option<u16>, TreeError> {
    match spec.props.get(name) {
        Some(value) => match value.as_u64() {
            Some(v) => Ok(Some(v.min(u16::MAX as u64) as u16)),
            None => Err(invalid_prop(spec, name, "u16", value)),
        },
        None => Ok(None),
    }
}

pub fn prop_u64(spec: &ComponentSpec, name: &str) -> Result<Option<u64>, TreeError> {
    match spec.props.get(name) {
        Some(value) => match value.as_u64() {
            Some(v) => Ok(Some(v)),
            None => Err(invalid_prop(spec, name, "u64", value)),
        },
        None => Ok(None),
    }
}

pub fn prop_usize(spec: &ComponentSpec, name: &str) -> Result<Option<usize>, TreeError> {
    match spec.props.get(name) {
        Some(value) => match value.as_u64() {
            Some(v) => Ok(Some(v as usize)),
            None => Err(invalid_prop(spec, name, "usize", value)),
        },
        None => Ok(None),
    }
}

pub fn prop_f64(spec: &ComponentSpec, name: &str) -> Result<Option<f64>, TreeError> {
    match spec.props.get(name) {
        Some(value) => match value.as_f64() {
            Some(v) => Ok(Some(v)),
            None => Err(invalid_prop(spec, name, "f64", value)),
        },
        None => Ok(None),
    }
}

pub fn prop_vec_string(spec: &ComponentSpec, name: &str) -> Result<Option<Vec<String>>, TreeError> {
    match spec.props.get(name) {
        Some(ComponentValue::StringList(v)) => Ok(Some(v.clone())),
        Some(other) => Err(invalid_prop(spec, name, "string list", other)),
        None => Ok(None),
    }
}

pub fn prop_table(spec: &ComponentSpec, name: &str) -> Result<Option<Vec<Vec<String>>>, TreeError> {
    match spec.props.get(name) {
        Some(ComponentValue::Table(v)) => Ok(Some(v.clone())),
        Some(other) => Err(invalid_prop(spec, name, "table", other)),
        None => Ok(None),
    }
}

fn prop_edge_insets(spec: &ComponentSpec, name: &str) -> Result<Option<EdgeInsets>, TreeError> {
    let Some(value) = spec.props.get(name) else {
        return Ok(None);
    };
    edge_insets_from_value(spec, name, value).map(Some)
}

fn edge_insets_from_value(
    spec: &ComponentSpec,
    name: &str,
    value: &ComponentValue,
) -> Result<EdgeInsets, TreeError> {
    match value {
        ComponentValue::U64(v) => {
            let val = (*v).min(u16::MAX as u64) as u16;
            Ok(EdgeInsets::all(val))
        }
        ComponentValue::I64(v) if *v >= 0 => {
            let val = (*v as u64).min(u16::MAX as u64) as u16;
            Ok(EdgeInsets::all(val))
        }
        ComponentValue::F64(v) if *v >= 0.0 => {
            let val = (*v as u64).min(u16::MAX as u64) as u16;
            Ok(EdgeInsets::all(val))
        }
        ComponentValue::Map(map) => {
            let top = map
                .get("top")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                .min(u16::MAX as u64) as u16;
            let right = map
                .get("right")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                .min(u16::MAX as u64) as u16;
            let bottom = map
                .get("bottom")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                .min(u16::MAX as u64) as u16;
            let left = map
                .get("left")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                .min(u16::MAX as u64) as u16;
            Ok(EdgeInsets {
                top,
                right,
                bottom,
                left,
            })
        }
        ComponentValue::List(values) => {
            if values.len() != 4 {
                return Err(invalid_prop(spec, name, "padding list of 4", value));
            }
            let to_u16 = |idx: usize| -> Option<u16> {
                values
                    .get(idx)
                    .and_then(|v| v.as_u64())
                    .map(|v| v.min(u16::MAX as u64) as u16)
            };
            let top = to_u16(0).unwrap_or(0);
            let right = to_u16(1).unwrap_or(0);
            let bottom = to_u16(2).unwrap_or(0);
            let left = to_u16(3).unwrap_or(0);
            Ok(EdgeInsets {
                top,
                right,
                bottom,
                left,
            })
        }
        _ => Err(invalid_prop(spec, name, "padding", value)),
    }
}

pub fn invalid_prop(
    spec: &ComponentSpec,
    name: &str,
    expected: &str,
    value: &ComponentValue,
) -> TreeError {
    TreeError::InvalidProperty {
        id: spec.id.clone().unwrap_or_else(|| spec.type_name.clone()),
        name: name.to_string(),
        reason: format!("expected {expected}, got {value:?}"),
    }
}

pub fn invalid_prop_reason(
    spec: &ComponentSpec,
    name: &str,
    reason: impl Into<String>,
) -> TreeError {
    TreeError::InvalidProperty {
        id: spec.id.clone().unwrap_or_else(|| spec.type_name.clone()),
        name: name.to_string(),
        reason: reason.into(),
    }
}

trait StackBuilder {
    fn new() -> Self;
    fn with_spacing(self, spacing: impl Into<Binding<u16>>) -> Self;
    fn with_padding(self, padding: impl Into<Binding<EdgeInsets>>) -> Self;
    fn with_scrollable(self, scrollable: impl Into<Binding<bool>>) -> Self;
    fn add_child_with_layout(&mut self, view: Box<dyn Component>, layout: LayoutParams);
}

impl StackBuilder for VStack {
    fn new() -> Self {
        VStack::new()
    }

    fn with_spacing(self, spacing: impl Into<Binding<u16>>) -> Self {
        self.with_spacing(spacing)
    }

    fn with_padding(self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.with_padding(padding)
    }

    fn with_scrollable(self, scrollable: impl Into<Binding<bool>>) -> Self {
        self.with_scrollable(scrollable)
    }

    fn add_child_with_layout(&mut self, view: Box<dyn Component>, layout: LayoutParams) {
        self.add_child_with_layout(view, layout);
    }
}

impl StackBuilder for HStack {
    fn new() -> Self {
        HStack::new()
    }

    fn with_spacing(self, spacing: impl Into<Binding<u16>>) -> Self {
        self.with_spacing(spacing)
    }

    fn with_padding(self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.with_padding(padding)
    }

    fn with_scrollable(self, scrollable: impl Into<Binding<bool>>) -> Self {
        self.with_scrollable(scrollable)
    }

    fn add_child_with_layout(&mut self, view: Box<dyn Component>, layout: LayoutParams) {
        self.add_child_with_layout(view, layout);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composable::{Component, ComponentContext, EventResult, ScrollbarHost, TabMode};
    use crate::theme::Theme;
    use crate::wm::WindowId;
    use atto_ui_runtime::ComponentSpecChild;
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    #[test]
    fn component_button_click_emits_callback() {
        let callbacks = CallbackRegistry::new();
        let cb = callbacks.register();

        let mut spec = ComponentSpec::new("Button")
            .with_id("btn")
            .with_prop("label", ComponentValue::String("OK".into()));
        spec.events.insert("click".into(), cb);

        let registry = builtin_registry(callbacks.clone());
        let mut view = registry.build(&spec).expect("build");

        let ctx = ComponentContext {
            theme: &Theme::dark(),
            window_id: WindowId(1),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
        };
        let event = Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let result = view.handle_event(&event, ctx);
        assert_eq!(result, EventResult::submitted());

        let events = callbacks.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].callback_id, cb);
        assert_eq!(events[0].target_id.as_deref(), Some("btn"));
        assert_eq!(events[0].event, "click");
    }

    #[test]
    fn component_checkbox_change_emits_callback() {
        let callbacks = CallbackRegistry::new();
        let cb = callbacks.register();

        let mut spec = ComponentSpec::new("Checkbox")
            .with_id("chk")
            .with_prop("label", ComponentValue::String("A".into()))
            .with_prop("checked", ComponentValue::Bool(false));
        spec.events.insert("change".into(), cb);

        let registry = builtin_registry(callbacks.clone());
        let mut view = registry.build(&spec).expect("build");

        let ctx = ComponentContext {
            theme: &Theme::dark(),
            window_id: WindowId(1),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
        };
        let event = Event::Key(KeyEvent {
            code: KeyCode::Char(' '),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let result = view.handle_event(&event, ctx);
        assert_eq!(result, EventResult::changed());

        let events = callbacks.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].callback_id, cb);
        assert_eq!(events[0].target_id.as_deref(), Some("chk"));
        assert_eq!(events[0].event, "change");
    }

    #[test]
    fn component_tree_ops_rebuild_children() {
        let callbacks = CallbackRegistry::new();
        let mut tree = ComponentTree::new(ComponentSpec::new("VStack").with_id("root"), callbacks)
            .expect("tree");

        let child = ComponentSpecChild::new(
            ComponentSpec::new("Label")
                .with_id("title")
                .with_prop("text", ComponentValue::String("Hello".into())),
        )
        .with_layout(LayoutSpec {
            width: SizeSpec::Fixed(8),
            height: SizeSpec::Content,
            ..LayoutSpec::default()
        });

        tree.apply_ops(&[TreeOp::Insert {
            parent_id: "root".into(),
            index: 0,
            child,
        }])
        .expect("apply ops");

        tree.rebuild().expect("rebuild");
        let children = tree.view().children();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].layout.width, Size::Fixed(8));
        let value = children[0]
            .view
            .get_property("text")
            .expect("text property");
        assert_eq!(value, ComponentValue::String("Hello".into()));
    }

    #[test]
    fn component_tree_incremental_set_prop_updates_view() {
        let callbacks = CallbackRegistry::new();
        let root =
            ComponentSpec::new("VStack")
                .with_id("root")
                .with_child(ComponentSpecChild::new(
                    ComponentSpec::new("Label")
                        .with_id("title")
                        .with_prop("text", ComponentValue::String("A".into())),
                ));
        let mut tree = ComponentTree::new(root, callbacks).expect("tree");

        let changed = tree
            .apply_ops_incremental(&[TreeOp::SetProp {
                id: "title".into(),
                name: "text".into(),
                value: ComponentValue::String("B".into()),
            }])
            .expect("apply");
        assert!(!changed);

        let children = tree.view().children();
        let value = children[0]
            .view
            .get_property("text")
            .expect("text property");
        assert_eq!(value, ComponentValue::String("B".into()));
    }

    #[test]
    fn component_tree_incremental_insert_child() {
        let callbacks = CallbackRegistry::new();
        let root = ComponentSpec::new("VStack").with_id("root");
        let mut tree = ComponentTree::new(root, callbacks).expect("tree");

        let child = ComponentSpecChild::new(
            ComponentSpec::new("Label")
                .with_id("a")
                .with_prop("text", ComponentValue::String("Hello".into())),
        )
        .with_layout(LayoutSpec {
            width: SizeSpec::Fixed(5),
            height: SizeSpec::Content,
            ..LayoutSpec::default()
        });

        let changed = tree
            .apply_ops_incremental(&[TreeOp::Insert {
                parent_id: "root".into(),
                index: 0,
                child,
            }])
            .expect("apply");
        assert!(changed);

        let children = tree.view().children();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].layout.width, Size::Fixed(5));
        let value = children[0]
            .view
            .get_property("text")
            .expect("text property");
        assert_eq!(value, ComponentValue::String("Hello".into()));
    }

    #[test]
    fn component_tree_incremental_remove_child() {
        let callbacks = CallbackRegistry::new();
        let root = ComponentSpec::new("VStack")
            .with_id("root")
            .with_child(ComponentSpecChild::new(
                ComponentSpec::new("Label").with_id("a"),
            ))
            .with_child(ComponentSpecChild::new(
                ComponentSpec::new("Label").with_id("b"),
            ));
        let mut tree = ComponentTree::new(root, callbacks).expect("tree");

        let changed = tree
            .apply_ops_incremental(&[TreeOp::Remove { id: "a".into() }])
            .expect("apply");
        assert!(changed);

        let children = tree.view().children();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].view.tag(), Some("b"));
    }

    #[test]
    fn component_tree_incremental_move_child() {
        let callbacks = CallbackRegistry::new();
        let root = ComponentSpec::new("VStack")
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
        let mut tree = ComponentTree::new(root, callbacks).expect("tree");

        let changed = tree
            .apply_ops_incremental(&[TreeOp::Move {
                id: "c".into(),
                new_parent_id: "root".into(),
                index: 0,
            }])
            .expect("apply");
        assert!(changed);

        let children = tree.view().children();
        let ids: Vec<Option<&str>> = children.iter().map(|child| child.view.tag()).collect();
        assert_eq!(ids, vec![Some("c"), Some("a"), Some("b")]);
    }

    #[test]
    fn component_tree_incremental_replace_child() {
        let callbacks = CallbackRegistry::new();
        let root =
            ComponentSpec::new("VStack")
                .with_id("root")
                .with_child(ComponentSpecChild::new(
                    ComponentSpec::new("Label").with_id("a"),
                ));
        let mut tree = ComponentTree::new(root, callbacks).expect("tree");

        let node = ComponentSpecChild::new(
            ComponentSpec::new("Button")
                .with_id("a")
                .with_prop("label", ComponentValue::String("OK".into())),
        )
        .with_layout(LayoutSpec {
            width: SizeSpec::Fixed(6),
            height: SizeSpec::Content,
            ..LayoutSpec::default()
        });

        let changed = tree
            .apply_ops_incremental(&[TreeOp::Replace {
                id: "a".into(),
                node,
            }])
            .expect("apply");
        assert!(changed);

        let children = tree.view().children();
        assert_eq!(children[0].layout.width, Size::Fixed(6));
        assert!(children[0].view.type_name().ends_with("Button"));
        let value = children[0]
            .view
            .get_property("label")
            .expect("label property");
        assert_eq!(value, ComponentValue::String("OK".into()));
    }

    #[test]
    fn component_tree_incremental_bind_and_clear_event() {
        let callbacks = CallbackRegistry::new();
        let cb = callbacks.register();
        let root =
            ComponentSpec::new("VStack")
                .with_id("root")
                .with_child(ComponentSpecChild::new(
                    ComponentSpec::new("Button")
                        .with_id("btn")
                        .with_prop("label", ComponentValue::String("Go".into())),
                ));
        let mut tree = ComponentTree::new(root, callbacks.clone()).expect("tree");

        let changed = tree
            .apply_ops_incremental(&[TreeOp::BindEvent {
                id: "btn".into(),
                event: "click".into(),
                callback: cb,
            }])
            .expect("bind");
        assert!(changed);

        let ctx = ComponentContext {
            theme: &Theme::dark(),
            window_id: WindowId(1),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
        };
        let event = Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        {
            let children = tree.view_mut().children_mut().expect("children");
            let result = children[0].view.handle_event(&event, ctx);
            assert_eq!(result, EventResult::submitted());
        }
        let events = callbacks.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].callback_id, cb);

        tree.apply_ops_incremental(&[TreeOp::ClearEvent {
            id: "btn".into(),
            event: "click".into(),
        }])
        .expect("clear");

        {
            let children = tree.view_mut().children_mut().expect("children");
            let result = children[0].view.handle_event(&event, ctx);
            assert_eq!(result, EventResult::submitted());
        }
        let events = callbacks.drain();
        assert!(events.is_empty());
    }

    #[test]
    fn builtin_schema_includes_button_props() {
        let registry = builtin_registry(CallbackRegistry::new());
        let schema = registry.schema("Button").expect("schema");
        assert!(schema.properties.iter().any(|prop| prop.name == "label"));
        assert!(schema.events.iter().any(|event| event.name == "click"));
    }

    #[test]
    fn builtin_schema_includes_stack_padding() {
        let registry = builtin_registry(CallbackRegistry::new());
        let schema = registry.schema("VStack").expect("schema");
        let padding = schema
            .properties
            .iter()
            .find(|prop| prop.name == "padding")
            .expect("padding");
        assert_eq!(padding.value_type, ValueType::Map);
    }

    #[test]
    fn builtin_schema_includes_styled_label_link_event() {
        let registry = builtin_registry(CallbackRegistry::new());
        let schema = registry.schema("StyledLabel").expect("schema");
        let link = schema
            .events
            .iter()
            .find(|event| event.name == "link")
            .expect("link");
        assert_eq!(link.payload, Some(ValueType::String));
    }
}
