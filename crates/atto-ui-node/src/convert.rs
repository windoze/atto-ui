//! Serde-backed conversion helpers for the Node binding.
//!
//! The public JavaScript shape stays close to `NODE_BINDING.md`: component
//! values use plain JS primitives/arrays/objects, and tree operations use an
//! `op` discriminant such as `set_prop`.

use std::collections::BTreeMap;

use atto_ui::runtime::{
    AlignSpec, AnchorPlacementSpec, AnchorSpec, CallbackId, CallbackInvocation, ComponentSchema,
    ComponentSpec, ComponentSpecChild, ComponentValue, EdgeInsetsSpec, LayoutSpec,
    Rect as RuntimeRect, SizeSpec, TreeOp,
};
use napi::bindgen_prelude::{Env, Object, Result, Unknown};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Number, Value};

use crate::error::invalid_arg;
use crate::ids::CallbackHandles;

const TYPE_TAG: &str = "$type";
const DATA_FIELD: &str = "data";
const BYTES_TAG: &str = "bytes";
const LIST_TAG: &str = "list";
const MAP_TAG: &str = "map";
const RECT_TAG: &str = "rect";
const STRING_LIST_TAG: &str = "string_list";
const TABLE_TAG: &str = "table";
const JS_MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// Convert an arbitrary JavaScript value into `serde_json::Value` using napi-rs serde support.
pub fn unknown_to_json(env: &Env, value: Unknown<'_>) -> Result<Value> {
    env.from_js_value(value)
}

/// Convert a JavaScript object into `serde_json::Value` using napi-rs serde support.
pub fn object_to_json(env: &Env, value: Object<'_>) -> Result<Value> {
    env.from_js_value(value)
}

/// Convert `serde_json::Value` back into a JavaScript value.
pub fn json_to_unknown<'env>(env: &'env Env, value: &Value) -> Result<Unknown<'env>> {
    env.to_js_value(value)
}

/// Decode a JavaScript value as a `ComponentValue`.
pub fn component_value_from_unknown(env: &Env, value: Unknown<'_>) -> Result<ComponentValue> {
    component_value_from_json(unknown_to_json(env, value)?)
}

/// Encode a `ComponentValue` as a JavaScript value.
pub fn component_value_to_unknown<'env>(
    env: &'env Env,
    value: &ComponentValue,
) -> Result<Unknown<'env>> {
    let json = component_value_to_json(value)?;
    json_to_unknown(env, &json)
}

/// Decode a JSON value as a runtime `ComponentValue`.
pub fn component_value_from_json(value: Value) -> Result<ComponentValue> {
    component_value_from_value(&value)
}

/// Encode a runtime `ComponentValue` as the JavaScript-facing JSON shape.
pub fn component_value_to_json(value: &ComponentValue) -> Result<Value> {
    match value {
        ComponentValue::Null => Ok(Value::Null),
        ComponentValue::Bool(v) => Ok(Value::Bool(*v)),
        ComponentValue::I64(v) => Ok(Value::Number(Number::from(*v))),
        ComponentValue::U64(v) => Ok(Value::Number(Number::from(*v))),
        ComponentValue::F64(v) => number_from_f64(*v).map(Value::Number),
        ComponentValue::String(v) => Ok(Value::String(v.clone())),
        ComponentValue::StringList(values) => {
            let json = string_list_to_json(values);
            if values.is_empty() {
                Ok(tagged_component_value(STRING_LIST_TAG, json))
            } else {
                Ok(json)
            }
        }
        ComponentValue::Table(rows) => {
            let json = table_to_json(rows);
            if rows.is_empty() {
                Ok(tagged_component_value(TABLE_TAG, json))
            } else {
                Ok(json)
            }
        }
        ComponentValue::Rect(rect) => Ok(rect_to_json(rect)),
        ComponentValue::Bytes(bytes) => Ok(bytes_to_json(bytes)),
        ComponentValue::List(values) => {
            let values = values
                .iter()
                .map(component_value_to_json)
                .collect::<Result<Vec<_>>>()?;
            if array_decodes_as_specialized_value(&values) {
                Ok(tagged_component_value(LIST_TAG, Value::Array(values)))
            } else {
                Ok(Value::Array(values))
            }
        }
        ComponentValue::Map(values) => {
            let mut object = Map::new();
            for (key, value) in values {
                object.insert(key.clone(), component_value_to_json(value)?);
            }
            if object_decodes_as_specialized_value(&object) {
                Ok(tagged_component_value(MAP_TAG, Value::Object(object)))
            } else {
                Ok(Value::Object(object))
            }
        }
    }
}

/// Decode a JavaScript value as a `ComponentSpec`.
pub fn component_spec_from_unknown(
    env: &Env,
    value: Unknown<'_>,
    callbacks: &CallbackHandles,
) -> Result<ComponentSpec> {
    component_spec_from_json(unknown_to_json(env, value)?, callbacks)
}

/// Encode a `ComponentSpec` as a JavaScript value.
pub fn component_spec_to_unknown<'env>(
    env: &'env Env,
    value: &ComponentSpec,
    callbacks: &mut CallbackHandles,
) -> Result<Unknown<'env>> {
    let json = component_spec_to_json(value, callbacks)?;
    json_to_unknown(env, &json)
}

/// Decode a JSON value as a runtime `ComponentSpec`.
pub fn component_spec_from_json(
    value: Value,
    callbacks: &CallbackHandles,
) -> Result<ComponentSpec> {
    component_spec_from_value(&value, callbacks)
}

/// Encode a runtime `ComponentSpec` as the JavaScript-facing object shape.
pub fn component_spec_to_json(
    spec: &ComponentSpec,
    callbacks: &mut CallbackHandles,
) -> Result<Value> {
    let mut object = Map::new();
    object.insert("type".to_string(), Value::String(spec.type_name.clone()));
    if let Some(id) = &spec.id {
        object.insert("id".to_string(), Value::String(id.clone()));
    }
    if !spec.props.is_empty() {
        object.insert("props".to_string(), value_map_to_json(&spec.props)?);
    }
    if !spec.events.is_empty() {
        object.insert(
            "events".to_string(),
            callback_map_to_json(&spec.events, callbacks),
        );
    }
    if !spec.children.is_empty() {
        object.insert(
            "children".to_string(),
            Value::Array(
                spec.children
                    .iter()
                    .map(|child| component_spec_child_to_json(child, callbacks))
                    .collect::<Result<Vec<_>>>()?,
            ),
        );
    }
    Ok(Value::Object(object))
}

/// Decode a JavaScript value as a `TreeOp`.
pub fn tree_op_from_unknown(
    env: &Env,
    value: Unknown<'_>,
    callbacks: &CallbackHandles,
) -> Result<TreeOp> {
    tree_op_from_json(unknown_to_json(env, value)?, callbacks)
}

/// Encode a `TreeOp` as a JavaScript value.
pub fn tree_op_to_unknown<'env>(
    env: &'env Env,
    value: &TreeOp,
    callbacks: &mut CallbackHandles,
) -> Result<Unknown<'env>> {
    let json = tree_op_to_json(value, callbacks)?;
    json_to_unknown(env, &json)
}

/// Decode a JSON object as a runtime `TreeOp`.
pub fn tree_op_from_json(value: Value, callbacks: &CallbackHandles) -> Result<TreeOp> {
    tree_op_from_value(&value, callbacks)
}

/// Encode a runtime `TreeOp` as a discriminated JavaScript union object.
pub fn tree_op_to_json(op: &TreeOp, callbacks: &mut CallbackHandles) -> Result<Value> {
    let mut object = Map::new();
    match op {
        TreeOp::SetTree(spec) => {
            object.insert("op".to_string(), Value::String("set_tree".to_string()));
            object.insert("tree".to_string(), component_spec_to_json(spec, callbacks)?);
        }
        TreeOp::Insert {
            parent_id,
            index,
            child,
        } => {
            object.insert("op".to_string(), Value::String("insert".to_string()));
            object.insert("parent_id".to_string(), Value::String(parent_id.clone()));
            object.insert("index".to_string(), usize_to_json(*index)?);
            object.insert(
                "child".to_string(),
                component_spec_child_to_json(child, callbacks)?,
            );
        }
        TreeOp::InsertBefore {
            parent_id,
            anchor_id,
            child,
        } => {
            object.insert("op".to_string(), Value::String("insert_before".to_string()));
            object.insert("parent_id".to_string(), Value::String(parent_id.clone()));
            object.insert(
                "anchor_id".to_string(),
                anchor_id
                    .as_ref()
                    .map(|id| Value::String(id.clone()))
                    .unwrap_or(Value::Null),
            );
            object.insert(
                "child".to_string(),
                component_spec_child_to_json(child, callbacks)?,
            );
        }
        TreeOp::Remove { id } => {
            object.insert("op".to_string(), Value::String("remove".to_string()));
            object.insert("id".to_string(), Value::String(id.clone()));
        }
        TreeOp::Replace { id, node } => {
            object.insert("op".to_string(), Value::String("replace".to_string()));
            object.insert("id".to_string(), Value::String(id.clone()));
            object.insert(
                "node".to_string(),
                component_spec_child_to_json(node, callbacks)?,
            );
        }
        TreeOp::Move {
            id,
            new_parent_id,
            index,
        } => {
            object.insert("op".to_string(), Value::String("move".to_string()));
            object.insert("id".to_string(), Value::String(id.clone()));
            object.insert(
                "new_parent_id".to_string(),
                Value::String(new_parent_id.clone()),
            );
            object.insert("index".to_string(), usize_to_json(*index)?);
        }
        TreeOp::SetProp { id, name, value } => {
            object.insert("op".to_string(), Value::String("set_prop".to_string()));
            object.insert("id".to_string(), Value::String(id.clone()));
            object.insert("name".to_string(), Value::String(name.clone()));
            object.insert("value".to_string(), component_value_to_json(value)?);
        }
        TreeOp::ClearProp { id, name } => {
            object.insert("op".to_string(), Value::String("clear_prop".to_string()));
            object.insert("id".to_string(), Value::String(id.clone()));
            object.insert("name".to_string(), Value::String(name.clone()));
        }
        TreeOp::BindEvent {
            id,
            event,
            callback,
        } => {
            object.insert("op".to_string(), Value::String("bind_event".to_string()));
            object.insert("id".to_string(), Value::String(id.clone()));
            object.insert("event".to_string(), Value::String(event.clone()));
            object.insert(
                "callback".to_string(),
                callback_id_to_json(*callback, callbacks),
            );
        }
        TreeOp::ClearEvent { id, event } => {
            object.insert("op".to_string(), Value::String("clear_event".to_string()));
            object.insert("id".to_string(), Value::String(id.clone()));
            object.insert("event".to_string(), Value::String(event.clone()));
        }
    }
    Ok(Value::Object(object))
}

/// Decode a single op or an array of ops as runtime `TreeOp` values.
pub fn tree_ops_from_json(value: Value, callbacks: &CallbackHandles) -> Result<Vec<TreeOp>> {
    match value {
        Value::Array(values) => values
            .into_iter()
            .map(|value| tree_op_from_json(value, callbacks))
            .collect(),
        other => tree_op_from_json(other, callbacks).map(|op| vec![op]),
    }
}

/// Encode runtime `TreeOp` values as a JavaScript array shape.
pub fn tree_ops_to_json(ops: &[TreeOp], callbacks: &mut CallbackHandles) -> Result<Value> {
    Ok(Value::Array(
        ops.iter()
            .map(|op| tree_op_to_json(op, callbacks))
            .collect::<Result<Vec<_>>>()?,
    ))
}

/// Decode a JavaScript value as a `CallbackInvocation`.
pub fn callback_invocation_from_unknown(
    env: &Env,
    value: Unknown<'_>,
    callbacks: &CallbackHandles,
) -> Result<CallbackInvocation> {
    callback_invocation_from_json(unknown_to_json(env, value)?, callbacks)
}

/// Encode a `CallbackInvocation` as a JavaScript value.
pub fn callback_invocation_to_unknown<'env>(
    env: &'env Env,
    value: &CallbackInvocation,
    callbacks: &mut CallbackHandles,
) -> Result<Unknown<'env>> {
    let json = callback_invocation_to_json(value, callbacks)?;
    json_to_unknown(env, &json)
}

/// Decode a JSON object as a runtime callback invocation.
pub fn callback_invocation_from_json(
    value: Value,
    callbacks: &CallbackHandles,
) -> Result<CallbackInvocation> {
    let object = expect_object(&value, "callback invocation")?;
    let callback = callback_id_from_value(
        expect_any_field(
            object,
            &["callbackId", "callback_id"],
            "callback invocation callbackId",
        )?,
        callbacks,
    )?;
    let target_id = get_any_field(object, &["targetId", "target_id"])
        .map(|value| expect_string(value, "callback invocation targetId"))
        .transpose()?;
    let event = expect_string(
        expect_field(object, "event", "callback invocation event")?,
        "callback invocation event",
    )?;
    let payload = get_field(object, "payload")
        .map(component_value_from_value)
        .transpose()?;
    Ok(CallbackInvocation {
        callback_id: callback,
        target_id,
        event,
        payload,
    })
}

/// Encode a runtime callback invocation as the JavaScript callback object shape.
pub fn callback_invocation_to_json(
    invocation: &CallbackInvocation,
    callbacks: &mut CallbackHandles,
) -> Result<Value> {
    let mut object = Map::new();
    object.insert(
        "callbackId".to_string(),
        callback_id_to_json(invocation.callback_id, callbacks),
    );
    object.insert(
        "targetId".to_string(),
        invocation
            .target_id
            .as_ref()
            .map(|id| Value::String(id.clone()))
            .unwrap_or(Value::Null),
    );
    object.insert("event".to_string(), Value::String(invocation.event.clone()));
    object.insert(
        "payload".to_string(),
        invocation
            .payload
            .as_ref()
            .map(component_value_to_json)
            .transpose()?
            .unwrap_or(Value::Null),
    );
    Ok(Value::Object(object))
}

/// Decode a JavaScript value as a `ComponentSchema`.
pub fn component_schema_from_unknown(env: &Env, value: Unknown<'_>) -> Result<ComponentSchema> {
    component_schema_from_json(unknown_to_json(env, value)?)
}

/// Encode a `ComponentSchema` as a JavaScript value.
pub fn component_schema_to_unknown<'env>(
    env: &'env Env,
    value: &ComponentSchema,
) -> Result<Unknown<'env>> {
    let json = component_schema_to_json(value)?;
    json_to_unknown(env, &json)
}

/// Decode a JSON object as a runtime component schema.
pub fn component_schema_from_json(value: Value) -> Result<ComponentSchema> {
    deserialize_from_json(value, "component schema")
}

/// Encode a runtime component schema using its serde representation.
pub fn component_schema_to_json(schema: &ComponentSchema) -> Result<Value> {
    serialize_to_json(schema, "component schema")
}

fn component_value_from_value(value: &Value) -> Result<ComponentValue> {
    match value {
        Value::Null => Ok(ComponentValue::Null),
        Value::Bool(v) => Ok(ComponentValue::Bool(*v)),
        Value::Number(number) => component_value_from_number(number),
        Value::String(v) => Ok(ComponentValue::String(v.clone())),
        Value::Array(values) => component_value_from_array(values),
        Value::Object(object) => component_value_from_object(object),
    }
}

fn component_value_from_number(number: &Number) -> Result<ComponentValue> {
    if number.is_f64() {
        return number
            .as_f64()
            .map(ComponentValue::F64)
            .ok_or_else(|| invalid_arg("component value number is not finite"));
    }
    if let Some(value) = number.as_i64() {
        if value < 0 {
            return Ok(ComponentValue::I64(value));
        }
        return Ok(ComponentValue::U64(value as u64));
    }
    if let Some(value) = number.as_u64() {
        return Ok(ComponentValue::U64(value));
    }
    Err(invalid_arg("unsupported component value number"))
}

fn component_value_from_array(values: &[Value]) -> Result<ComponentValue> {
    if values.len() == 4 && values.iter().all(Value::is_number) {
        return rect_from_array(values).map(ComponentValue::Rect);
    }
    if values.is_empty() {
        return Ok(ComponentValue::List(Vec::new()));
    }
    if values.iter().all(Value::is_string) {
        return string_list_from_json(values).map(ComponentValue::StringList);
    }
    if values.iter().all(is_string_array) {
        return table_from_json(values).map(ComponentValue::Table);
    }
    Ok(ComponentValue::List(
        values
            .iter()
            .map(component_value_from_value)
            .collect::<Result<Vec<_>>>()?,
    ))
}

fn component_value_from_object(object: &Map<String, Value>) -> Result<ComponentValue> {
    if let Some(tag) = type_tag(object) {
        let data = expect_field(object, DATA_FIELD, tag_data_context(tag))?;
        return match tag {
            BYTES_TAG => bytes_from_json(data).map(ComponentValue::Bytes),
            LIST_TAG => expect_array(data, "component value list")?
                .iter()
                .map(component_value_from_value)
                .collect::<Result<Vec<_>>>()
                .map(ComponentValue::List),
            MAP_TAG => value_map_from_json(data, "component value map").map(ComponentValue::Map),
            RECT_TAG => rect_from_value(data).map(ComponentValue::Rect),
            STRING_LIST_TAG => string_list_from_json(expect_array(data, "string list")?)
                .map(ComponentValue::StringList),
            TABLE_TAG => table_from_json(expect_array(data, "table")?).map(ComponentValue::Table),
            _ => Err(invalid_arg(format!(
                "unknown component value type tag: {tag}"
            ))),
        };
    }
    if is_rect_object(object) {
        return rect_from_object(object).map(ComponentValue::Rect);
    }
    let mut out = BTreeMap::new();
    for (key, value) in object {
        out.insert(key.clone(), component_value_from_value(value)?);
    }
    Ok(ComponentValue::Map(out))
}

fn component_spec_from_value(value: &Value, callbacks: &CallbackHandles) -> Result<ComponentSpec> {
    let object = expect_object(value, "component spec")?;
    let type_name = expect_string(
        expect_any_field(object, &["type", "type_name"], "component spec type")?,
        "component spec type",
    )?;
    let mut spec = ComponentSpec::new(type_name);

    if let Some(id) = get_field(object, "id") {
        spec.id = Some(expect_string(id, "component spec id")?);
    }
    if let Some(props) = get_field(object, "props") {
        spec.props = value_map_from_json(props, "component spec props")?;
    }
    if let Some(events) = get_field(object, "events") {
        spec.events = callback_map_from_json(events, "component spec events", callbacks)?;
    }
    if let Some(children) = get_field(object, "children") {
        let children = expect_array(children, "component spec children")?;
        spec.children = children
            .iter()
            .map(|child| component_spec_child_from_value(child, callbacks))
            .collect::<Result<Vec<_>>>()?;
    }

    Ok(spec)
}

fn component_spec_child_from_value(
    value: &Value,
    callbacks: &CallbackHandles,
) -> Result<ComponentSpecChild> {
    if let Value::Object(object) = value
        && (object.contains_key("node")
            || object.contains_key("layout")
            || object.contains_key("meta"))
    {
        let node_value = get_field(object, "node").unwrap_or(value);
        let mut child = ComponentSpecChild::new(component_spec_from_value(node_value, callbacks)?);
        if let Some(layout) = get_field(object, "layout") {
            child.layout = Some(layout_spec_from_value(layout)?);
        }
        if let Some(meta) = get_field(object, "meta") {
            child.meta = value_map_from_json(meta, "component child meta")?;
        }
        return Ok(child);
    }
    Ok(ComponentSpecChild::new(component_spec_from_value(
        value, callbacks,
    )?))
}

fn component_spec_child_to_json(
    child: &ComponentSpecChild,
    callbacks: &mut CallbackHandles,
) -> Result<Value> {
    if child.layout.is_none() && child.meta.is_empty() {
        return component_spec_to_json(child.node.as_ref(), callbacks);
    }

    let mut object = Map::new();
    object.insert(
        "node".to_string(),
        component_spec_to_json(child.node.as_ref(), callbacks)?,
    );
    if let Some(layout) = &child.layout {
        object.insert("layout".to_string(), layout_spec_to_json(layout)?);
    }
    if !child.meta.is_empty() {
        object.insert("meta".to_string(), value_map_to_json(&child.meta)?);
    }
    Ok(Value::Object(object))
}

fn layout_spec_from_value(value: &Value) -> Result<LayoutSpec> {
    let object = expect_object(value, "layout")?;
    let mut layout = LayoutSpec::default();
    if let Some(width) = get_field(object, "width") {
        layout.width = size_spec_from_value(width)?;
    }
    if let Some(height) = get_field(object, "height") {
        layout.height = size_spec_from_value(height)?;
    }
    if let Some(margin) = get_field(object, "margin") {
        layout.margin = edge_insets_from_value(margin)?;
    }
    if let Some(align_x) = get_field(object, "align_x") {
        layout.align_x = align_spec_from_value(align_x)?;
    }
    if let Some(align_y) = get_field(object, "align_y") {
        layout.align_y = align_spec_from_value(align_y)?;
    }
    if let Some(anchor) = get_field(object, "anchor") {
        layout.anchor = Some(anchor_placement_from_value(anchor)?);
    }
    if let Some(tab_index) = get_field(object, "tab_index") {
        layout.tab_index = Some(i32_from_value(tab_index, "layout tab_index")?);
    }
    Ok(layout)
}

fn layout_spec_to_json(layout: &LayoutSpec) -> Result<Value> {
    let mut object = Map::new();
    object.insert("width".to_string(), size_spec_to_json(layout.width));
    object.insert("height".to_string(), size_spec_to_json(layout.height));
    object.insert("margin".to_string(), edge_insets_to_json(layout.margin));
    object.insert(
        "align_x".to_string(),
        Value::String(align_spec_to_string(layout.align_x).to_string()),
    );
    object.insert(
        "align_y".to_string(),
        Value::String(align_spec_to_string(layout.align_y).to_string()),
    );
    if let Some(anchor) = layout.anchor {
        object.insert("anchor".to_string(), anchor_placement_to_json(anchor));
    }
    if let Some(tab_index) = layout.tab_index {
        object.insert(
            "tab_index".to_string(),
            Value::Number(Number::from(tab_index)),
        );
    }
    Ok(Value::Object(object))
}

fn tree_op_from_value(value: &Value, callbacks: &CallbackHandles) -> Result<TreeOp> {
    let object = expect_object(value, "tree op")?;
    let op_name = expect_string(
        expect_any_field(object, &["op", "type", "kind"], "tree op op")?,
        "tree op op",
    )?;
    match normalize_name(&op_name).as_str() {
        "settree" => {
            let tree = expect_any_field(object, &["tree", "spec", "root"], "set_tree tree")?;
            Ok(TreeOp::SetTree(component_spec_from_value(tree, callbacks)?))
        }
        "insert" => Ok(TreeOp::Insert {
            parent_id: expect_string(
                expect_field(object, "parent_id", "insert parent_id")?,
                "insert parent_id",
            )?,
            index: usize_from_value(
                expect_field(object, "index", "insert index")?,
                "insert index",
            )?,
            child: component_spec_child_from_value(
                expect_field(object, "child", "insert child")?,
                callbacks,
            )?,
        }),
        "insertbefore" => Ok(TreeOp::InsertBefore {
            parent_id: expect_string(
                expect_field(object, "parent_id", "insert_before parent_id")?,
                "insert_before parent_id",
            )?,
            anchor_id: get_field(object, "anchor_id")
                .map(|value| expect_string(value, "insert_before anchor_id"))
                .transpose()?,
            child: component_spec_child_from_value(
                expect_field(object, "child", "insert_before child")?,
                callbacks,
            )?,
        }),
        "remove" => Ok(TreeOp::Remove {
            id: expect_string(expect_field(object, "id", "remove id")?, "remove id")?,
        }),
        "replace" => Ok(TreeOp::Replace {
            id: expect_string(expect_field(object, "id", "replace id")?, "replace id")?,
            node: component_spec_child_from_value(
                expect_field(object, "node", "replace node")?,
                callbacks,
            )?,
        }),
        "move" => Ok(TreeOp::Move {
            id: expect_string(expect_field(object, "id", "move id")?, "move id")?,
            new_parent_id: expect_string(
                expect_field(object, "new_parent_id", "move new_parent_id")?,
                "move new_parent_id",
            )?,
            index: usize_from_value(expect_field(object, "index", "move index")?, "move index")?,
        }),
        "setprop" => Ok(TreeOp::SetProp {
            id: expect_string(expect_field(object, "id", "set_prop id")?, "set_prop id")?,
            name: expect_string(
                expect_field(object, "name", "set_prop name")?,
                "set_prop name",
            )?,
            value: component_value_from_value(expect_field(object, "value", "set_prop value")?)?,
        }),
        "clearprop" => Ok(TreeOp::ClearProp {
            id: expect_string(
                expect_field(object, "id", "clear_prop id")?,
                "clear_prop id",
            )?,
            name: expect_string(
                expect_field(object, "name", "clear_prop name")?,
                "clear_prop name",
            )?,
        }),
        "bindevent" => Ok(TreeOp::BindEvent {
            id: expect_string(
                expect_field(object, "id", "bind_event id")?,
                "bind_event id",
            )?,
            event: expect_string(
                expect_field(object, "event", "bind_event event")?,
                "bind_event event",
            )?,
            callback: callback_id_from_value(
                expect_field(object, "callback", "bind_event callback")?,
                callbacks,
            )?,
        }),
        "clearevent" => Ok(TreeOp::ClearEvent {
            id: expect_string(
                expect_field(object, "id", "clear_event id")?,
                "clear_event id",
            )?,
            event: expect_string(
                expect_field(object, "event", "clear_event event")?,
                "clear_event event",
            )?,
        }),
        _ => Err(invalid_arg(format!("unknown tree op: {op_name}"))),
    }
}

fn size_spec_from_value(value: &Value) -> Result<SizeSpec> {
    match value {
        Value::String(name) => match normalize_name(name).as_str() {
            "fill" => Ok(SizeSpec::Fill),
            "content" => Ok(SizeSpec::Content),
            _ => Err(invalid_arg(format!("invalid size spec: {name}"))),
        },
        Value::Number(_) => u16_from_value(value, "fixed size").map(SizeSpec::Fixed),
        Value::Object(object) => {
            if let Some(value) = get_field(object, "fixed") {
                return u16_from_value(value, "fixed size").map(SizeSpec::Fixed);
            }
            if let Some(value) = get_field(object, "weight") {
                return u16_from_value(value, "weighted size").map(SizeSpec::Weight);
            }
            if matches!(get_field(object, "fill"), Some(Value::Bool(true))) {
                return Ok(SizeSpec::Fill);
            }
            if matches!(get_field(object, "content"), Some(Value::Bool(true))) {
                return Ok(SizeSpec::Content);
            }
            Err(invalid_arg("invalid size spec object"))
        }
        _ => Err(invalid_arg("invalid size spec")),
    }
}

fn size_spec_to_json(size: SizeSpec) -> Value {
    match size {
        SizeSpec::Fill => Value::String("fill".to_string()),
        SizeSpec::Content => Value::String("content".to_string()),
        SizeSpec::Fixed(value) => single_number_object("fixed", value),
        SizeSpec::Weight(value) => single_number_object("weight", value),
    }
}

fn edge_insets_from_value(value: &Value) -> Result<EdgeInsetsSpec> {
    match value {
        Value::Number(_) => {
            let value = u16_from_value(value, "edge inset")?;
            Ok(EdgeInsetsSpec {
                top: value,
                right: value,
                bottom: value,
                left: value,
            })
        }
        Value::Array(values) => {
            if values.len() != 4 {
                return Err(invalid_arg("edge insets array must contain 4 values"));
            }
            Ok(EdgeInsetsSpec {
                top: u16_from_value(&values[0], "edge inset top")?,
                right: u16_from_value(&values[1], "edge inset right")?,
                bottom: u16_from_value(&values[2], "edge inset bottom")?,
                left: u16_from_value(&values[3], "edge inset left")?,
            })
        }
        Value::Object(object) => Ok(EdgeInsetsSpec {
            top: optional_u16_field(object, "top", "edge inset top")?.unwrap_or(0),
            right: optional_u16_field(object, "right", "edge inset right")?.unwrap_or(0),
            bottom: optional_u16_field(object, "bottom", "edge inset bottom")?.unwrap_or(0),
            left: optional_u16_field(object, "left", "edge inset left")?.unwrap_or(0),
        }),
        _ => Err(invalid_arg("invalid edge insets")),
    }
}

fn edge_insets_to_json(insets: EdgeInsetsSpec) -> Value {
    let mut object = Map::new();
    object.insert("top".to_string(), Value::Number(Number::from(insets.top)));
    object.insert(
        "right".to_string(),
        Value::Number(Number::from(insets.right)),
    );
    object.insert(
        "bottom".to_string(),
        Value::Number(Number::from(insets.bottom)),
    );
    object.insert("left".to_string(), Value::Number(Number::from(insets.left)));
    Value::Object(object)
}

fn align_spec_from_value(value: &Value) -> Result<AlignSpec> {
    let name = expect_string(value, "align spec")?;
    match normalize_name(&name).as_str() {
        "start" => Ok(AlignSpec::Start),
        "center" => Ok(AlignSpec::Center),
        "end" => Ok(AlignSpec::End),
        "stretch" => Ok(AlignSpec::Stretch),
        _ => Err(invalid_arg(format!("invalid align spec: {name}"))),
    }
}

fn align_spec_to_string(align: AlignSpec) -> &'static str {
    match align {
        AlignSpec::Start => "start",
        AlignSpec::Center => "center",
        AlignSpec::End => "end",
        AlignSpec::Stretch => "stretch",
    }
}

fn anchor_placement_from_value(value: &Value) -> Result<AnchorPlacementSpec> {
    let object = expect_object(value, "anchor placement")?;
    Ok(AnchorPlacementSpec {
        anchor: anchor_spec_from_value(expect_field(object, "anchor", "anchor")?)?,
        offset_x: optional_i16_field(object, "offset_x", "anchor offset_x")?.unwrap_or(0),
        offset_y: optional_i16_field(object, "offset_y", "anchor offset_y")?.unwrap_or(0),
    })
}

fn anchor_placement_to_json(anchor: AnchorPlacementSpec) -> Value {
    let mut object = Map::new();
    object.insert(
        "anchor".to_string(),
        Value::String(anchor_spec_to_string(anchor.anchor).to_string()),
    );
    object.insert(
        "offset_x".to_string(),
        Value::Number(Number::from(anchor.offset_x)),
    );
    object.insert(
        "offset_y".to_string(),
        Value::Number(Number::from(anchor.offset_y)),
    );
    Value::Object(object)
}

fn anchor_spec_from_value(value: &Value) -> Result<AnchorSpec> {
    let name = expect_string(value, "anchor spec")?;
    match normalize_name(&name).as_str() {
        "topleft" => Ok(AnchorSpec::TopLeft),
        "topright" => Ok(AnchorSpec::TopRight),
        "bottomleft" => Ok(AnchorSpec::BottomLeft),
        "bottomright" => Ok(AnchorSpec::BottomRight),
        "top" => Ok(AnchorSpec::Top),
        "bottom" => Ok(AnchorSpec::Bottom),
        "left" => Ok(AnchorSpec::Left),
        "right" => Ok(AnchorSpec::Right),
        "center" => Ok(AnchorSpec::Center),
        _ => Err(invalid_arg(format!("invalid anchor spec: {name}"))),
    }
}

fn anchor_spec_to_string(anchor: AnchorSpec) -> &'static str {
    match anchor {
        AnchorSpec::TopLeft => "top_left",
        AnchorSpec::TopRight => "top_right",
        AnchorSpec::BottomLeft => "bottom_left",
        AnchorSpec::BottomRight => "bottom_right",
        AnchorSpec::Top => "top",
        AnchorSpec::Bottom => "bottom",
        AnchorSpec::Left => "left",
        AnchorSpec::Right => "right",
        AnchorSpec::Center => "center",
    }
}

fn value_map_from_json(value: &Value, context: &str) -> Result<BTreeMap<String, ComponentValue>> {
    let object = expect_object(value, context)?;
    let mut out = BTreeMap::new();
    for (key, value) in object {
        out.insert(key.clone(), component_value_from_value(value)?);
    }
    Ok(out)
}

fn value_map_to_json(values: &BTreeMap<String, ComponentValue>) -> Result<Value> {
    let mut object = Map::new();
    for (key, value) in values {
        object.insert(key.clone(), component_value_to_json(value)?);
    }
    Ok(Value::Object(object))
}

fn callback_map_from_json(
    value: &Value,
    context: &str,
    callbacks: &CallbackHandles,
) -> Result<BTreeMap<String, CallbackId>> {
    let object = expect_object(value, context)?;
    let mut out = BTreeMap::new();
    for (key, value) in object {
        out.insert(key.clone(), callback_id_from_value(value, callbacks)?);
    }
    Ok(out)
}

fn callback_map_to_json(
    values: &BTreeMap<String, CallbackId>,
    callbacks: &mut CallbackHandles,
) -> Value {
    let mut object = Map::new();
    for (key, value) in values {
        object.insert(key.clone(), callback_id_to_json(*value, callbacks));
    }
    Value::Object(object)
}

fn callback_id_from_value(value: &Value, callbacks: &CallbackHandles) -> Result<CallbackId> {
    let handle = expect_string(value, "callback id handle")?;
    callbacks.resolve(&handle)
}

fn callback_id_to_json(value: CallbackId, callbacks: &mut CallbackHandles) -> Value {
    Value::String(callbacks.handle_for(value))
}

fn tagged_component_value(tag: &str, data: Value) -> Value {
    let mut object = Map::new();
    object.insert(TYPE_TAG.to_string(), Value::String(tag.to_string()));
    object.insert(DATA_FIELD.to_string(), data);
    Value::Object(object)
}

fn string_list_from_json(values: &[Value]) -> Result<Vec<String>> {
    values
        .iter()
        .map(|value| expect_string(value, "string list item"))
        .collect()
}

fn string_list_to_json(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}

fn table_from_json(values: &[Value]) -> Result<Vec<Vec<String>>> {
    values
        .iter()
        .map(|value| {
            expect_array(value, "table row")?
                .iter()
                .map(|cell| expect_string(cell, "table cell"))
                .collect::<Result<Vec<_>>>()
        })
        .collect()
}

fn table_to_json(rows: &[Vec<String>]) -> Value {
    Value::Array(
        rows.iter()
            .map(|row| Value::Array(row.iter().cloned().map(Value::String).collect()))
            .collect(),
    )
}

fn type_tag(object: &Map<String, Value>) -> Option<&str> {
    object
        .get(TYPE_TAG)
        .and_then(Value::as_str)
        .filter(|tag| is_known_component_value_tag(tag))
}

fn is_known_component_value_tag(tag: &str) -> bool {
    matches!(
        tag,
        BYTES_TAG | LIST_TAG | MAP_TAG | RECT_TAG | STRING_LIST_TAG | TABLE_TAG
    )
}

fn tag_data_context(tag: &str) -> &'static str {
    match tag {
        BYTES_TAG => "bytes data",
        LIST_TAG => "component value list data",
        MAP_TAG => "component value map data",
        RECT_TAG => "rect data",
        STRING_LIST_TAG => "string list data",
        TABLE_TAG => "table data",
        _ => "component value data",
    }
}

fn array_decodes_as_specialized_value(values: &[Value]) -> bool {
    !values.is_empty()
        && (values.iter().all(Value::is_string)
            || values.iter().all(is_string_array)
            || is_rect_array_shape(values))
}

fn object_decodes_as_specialized_value(object: &Map<String, Value>) -> bool {
    is_rect_object(object)
        || object
            .get(TYPE_TAG)
            .and_then(Value::as_str)
            .is_some_and(is_known_component_value_tag)
}

fn rect_from_object(object: &Map<String, Value>) -> Result<RuntimeRect> {
    Ok(RuntimeRect {
        x: u16_from_value(expect_field(object, "x", "rect x")?, "rect x")?,
        y: u16_from_value(expect_field(object, "y", "rect y")?, "rect y")?,
        width: u16_from_value(expect_field(object, "width", "rect width")?, "rect width")?,
        height: u16_from_value(
            expect_field(object, "height", "rect height")?,
            "rect height",
        )?,
    })
}

fn rect_from_value(value: &Value) -> Result<RuntimeRect> {
    match value {
        Value::Object(object) => rect_from_object(object),
        Value::Array(values) => rect_from_array(values),
        _ => Err(invalid_arg("rect must be an object or 4-item array")),
    }
}

fn rect_from_array(values: &[Value]) -> Result<RuntimeRect> {
    if values.len() != 4 {
        return Err(invalid_arg("rect array must contain 4 values"));
    }
    Ok(RuntimeRect {
        x: u16_from_value(&values[0], "rect x")?,
        y: u16_from_value(&values[1], "rect y")?,
        width: u16_from_value(&values[2], "rect width")?,
        height: u16_from_value(&values[3], "rect height")?,
    })
}

fn rect_to_json(rect: &RuntimeRect) -> Value {
    let mut object = Map::new();
    object.insert("x".to_string(), Value::Number(Number::from(rect.x)));
    object.insert("y".to_string(), Value::Number(Number::from(rect.y)));
    object.insert("width".to_string(), Value::Number(Number::from(rect.width)));
    object.insert(
        "height".to_string(),
        Value::Number(Number::from(rect.height)),
    );
    Value::Object(object)
}

fn bytes_from_json(value: &Value) -> Result<Vec<u8>> {
    expect_array(value, "bytes")?
        .iter()
        .map(|value| {
            let value = u64_from_value(value, "byte")?;
            u8::try_from(value).map_err(|_| invalid_arg("byte must be between 0 and 255"))
        })
        .collect()
}

fn bytes_to_json(bytes: &[u8]) -> Value {
    tagged_component_value(
        BYTES_TAG,
        Value::Array(
            bytes
                .iter()
                .copied()
                .map(|byte| Value::Number(Number::from(byte)))
                .collect(),
        ),
    )
}

fn serialize_to_json<T>(value: &T, context: &str) -> Result<Value>
where
    T: Serialize,
{
    serde_json::to_value(value).map_err(|err| invalid_arg(format!("{context}: {err}")))
}

fn deserialize_from_json<T>(value: Value, context: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value).map_err(|err| invalid_arg(format!("{context}: {err}")))
}

fn expect_object<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| invalid_arg(format!("{context} must be an object")))
}

fn expect_array<'a>(value: &'a Value, context: &str) -> Result<&'a [Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| invalid_arg(format!("{context} must be an array")))
}

fn expect_string(value: &Value, context: &str) -> Result<String> {
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| invalid_arg(format!("{context} must be a string")))
}

fn expect_field<'a>(
    object: &'a Map<String, Value>,
    name: &str,
    context: &str,
) -> Result<&'a Value> {
    get_field(object, name).ok_or_else(|| invalid_arg(format!("{context} is required")))
}

fn expect_any_field<'a>(
    object: &'a Map<String, Value>,
    names: &[&str],
    context: &str,
) -> Result<&'a Value> {
    get_any_field(object, names).ok_or_else(|| invalid_arg(format!("{context} is required")))
}

fn get_field<'a>(object: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    object.get(name).filter(|value| !value.is_null())
}

fn get_any_field<'a>(object: &'a Map<String, Value>, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|name| get_field(object, name))
}

fn optional_u16_field(
    object: &Map<String, Value>,
    name: &str,
    context: &str,
) -> Result<Option<u16>> {
    get_field(object, name)
        .map(|value| u16_from_value(value, context))
        .transpose()
}

fn optional_i16_field(
    object: &Map<String, Value>,
    name: &str,
    context: &str,
) -> Result<Option<i16>> {
    get_field(object, name)
        .map(|value| i16_from_value(value, context))
        .transpose()
}

fn u64_from_value(value: &Value, context: &str) -> Result<u64> {
    let number = value
        .as_number()
        .ok_or_else(|| invalid_arg(format!("{context} must be a number")))?;
    if let Some(value) = number.as_u64() {
        return Ok(value);
    }
    if let Some(value) = number.as_i64()
        && value >= 0
    {
        return Ok(value as u64);
    }
    if let Some(value) = number.as_f64()
        && value.is_finite()
        && value.fract() == 0.0
        && (0.0..=JS_MAX_SAFE_INTEGER).contains(&value)
    {
        return Ok(value as u64);
    }
    Err(invalid_arg(format!(
        "{context} must be a non-negative integer"
    )))
}

fn usize_from_value(value: &Value, context: &str) -> Result<usize> {
    usize::try_from(u64_from_value(value, context)?)
        .map_err(|_| invalid_arg(format!("{context} is too large")))
}

fn u16_from_value(value: &Value, context: &str) -> Result<u16> {
    u16::try_from(u64_from_value(value, context)?)
        .map_err(|_| invalid_arg(format!("{context} must fit in u16")))
}

fn i32_from_value(value: &Value, context: &str) -> Result<i32> {
    i32::try_from(i64_from_value(value, context)?)
        .map_err(|_| invalid_arg(format!("{context} must fit in i32")))
}

fn i16_from_value(value: &Value, context: &str) -> Result<i16> {
    i16::try_from(i64_from_value(value, context)?)
        .map_err(|_| invalid_arg(format!("{context} must fit in i16")))
}

fn i64_from_value(value: &Value, context: &str) -> Result<i64> {
    let number = value
        .as_number()
        .ok_or_else(|| invalid_arg(format!("{context} must be a number")))?;
    if let Some(value) = number.as_i64() {
        return Ok(value);
    }
    if let Some(value) = number.as_u64() {
        return i64::try_from(value).map_err(|_| invalid_arg(format!("{context} must fit in i64")));
    }
    if let Some(value) = number.as_f64()
        && value.is_finite()
        && value.fract() == 0.0
        && (-JS_MAX_SAFE_INTEGER..=JS_MAX_SAFE_INTEGER).contains(&value)
    {
        return Ok(value as i64);
    }
    Err(invalid_arg(format!("{context} must be an integer")))
}

fn number_from_f64(value: f64) -> Result<Number> {
    Number::from_f64(value).ok_or_else(|| invalid_arg("f64 component value must be finite"))
}

fn usize_to_json(value: usize) -> Result<Value> {
    let value = u64::try_from(value).map_err(|_| invalid_arg("usize is too large for JSON"))?;
    Ok(Value::Number(Number::from(value)))
}

fn single_number_object(name: &str, value: u16) -> Value {
    let mut object = Map::new();
    object.insert(name.to_string(), Value::Number(Number::from(value)));
    Value::Object(object)
}

fn is_string_array(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|values| values.iter().all(Value::is_string))
}

fn is_rect_array_shape(values: &[Value]) -> bool {
    values.len() == 4 && values.iter().all(Value::is_number)
}

fn is_rect_object(object: &Map<String, Value>) -> bool {
    object.len() == 4
        && object.contains_key("x")
        && object.contains_key("y")
        && object.contains_key("width")
        && object.contains_key("height")
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|ch| *ch != '_' && *ch != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use atto_ui::runtime::{ActionMeta, EventMeta, PropertyMeta, ValueType};
    use serde_json::json;

    #[test]
    fn component_value_round_trips_all_branches() {
        let mut map = BTreeMap::new();
        map.insert(
            "nested".to_string(),
            ComponentValue::String("value".to_string()),
        );

        let cases = vec![
            ComponentValue::Null,
            ComponentValue::Bool(true),
            ComponentValue::I64(-7),
            ComponentValue::U64(42),
            ComponentValue::F64(3.25),
            ComponentValue::String("hello".to_string()),
            ComponentValue::StringList(vec!["a".to_string(), "b".to_string()]),
            ComponentValue::StringList(Vec::new()),
            ComponentValue::Table(vec![vec!["a".to_string()], vec!["b".to_string()]]),
            ComponentValue::Table(Vec::new()),
            ComponentValue::Rect(RuntimeRect {
                x: 1,
                y: 2,
                width: 30,
                height: 10,
            }),
            ComponentValue::Bytes(vec![0, 127, 255]),
            ComponentValue::List(vec![ComponentValue::Bool(false), ComponentValue::U64(9)]),
            ComponentValue::Map(map),
        ];

        for case in cases {
            let encoded = component_value_to_json(&case).unwrap();
            let decoded = component_value_from_json(encoded).unwrap();
            assert_eq!(decoded, case);
        }
    }

    #[test]
    fn component_value_round_trips_ambiguous_json_shapes() {
        let mut rect_like_map = BTreeMap::new();
        rect_like_map.insert("x".to_string(), ComponentValue::U64(1));
        rect_like_map.insert("y".to_string(), ComponentValue::U64(2));
        rect_like_map.insert("width".to_string(), ComponentValue::U64(30));
        rect_like_map.insert("height".to_string(), ComponentValue::U64(10));

        let mut typed_like_map = BTreeMap::new();
        typed_like_map.insert(
            TYPE_TAG.to_string(),
            ComponentValue::String(BYTES_TAG.to_string()),
        );
        typed_like_map.insert(
            DATA_FIELD.to_string(),
            ComponentValue::List(vec![ComponentValue::U64(1)]),
        );

        let cases = vec![
            ComponentValue::List(vec![ComponentValue::String("value".to_string())]),
            ComponentValue::List(vec![ComponentValue::StringList(vec!["cell".to_string()])]),
            ComponentValue::List(vec![
                ComponentValue::U64(1),
                ComponentValue::U64(2),
                ComponentValue::U64(3),
                ComponentValue::U64(4),
            ]),
            ComponentValue::Map(rect_like_map),
            ComponentValue::Map(typed_like_map),
        ];

        for case in cases {
            let encoded = component_value_to_json(&case).unwrap();
            let decoded = component_value_from_json(encoded).unwrap();
            assert_eq!(decoded, case);
        }
    }

    #[test]
    fn component_value_accepts_rect_tuple_shape() {
        let decoded = component_value_from_json(json!([1, 2, 30, 10])).unwrap();
        assert_eq!(
            decoded,
            ComponentValue::Rect(RuntimeRect {
                x: 1,
                y: 2,
                width: 30,
                height: 10,
            })
        );
    }

    #[test]
    fn component_spec_round_trips_js_shape() {
        let mut callbacks = CallbackHandles::new();
        let click = callbacks.handle_for(CallbackId(7));
        let input = json!({
            "type": "VStack",
            "id": "root",
            "props": {
                "enabled": true,
                "labels": ["one", "two"]
            },
            "events": { "click": click.clone() },
            "children": [{
                "node": {
                    "type": "Text",
                    "id": "title",
                    "props": { "text": "Hi" }
                },
                "layout": {
                    "width": { "fixed": 12 },
                    "height": "content",
                    "margin": [1, 2, 3, 4],
                    "align_x": "center",
                    "align_y": "start",
                    "anchor": { "anchor": "top_left", "offset_x": 1, "offset_y": -1 },
                    "tab_index": 2
                },
                "meta": { "slot": "header" }
            }]
        });

        let spec = component_spec_from_json(input, &callbacks).unwrap();
        assert_eq!(spec.type_name, "VStack");
        assert_eq!(spec.id.as_deref(), Some("root"));
        assert_eq!(spec.events.get("click"), Some(&CallbackId(7)));
        assert_eq!(spec.children.len(), 1);
        assert!(spec.children[0].layout.is_some());

        let encoded = component_spec_to_json(&spec, &mut callbacks).unwrap();
        assert_eq!(encoded["type"], json!("VStack"));
        assert_eq!(encoded["events"]["click"], json!(click));
        assert!(encoded.get("type_name").is_none());
        assert_eq!(component_spec_from_json(encoded, &callbacks).unwrap(), spec);
    }

    #[test]
    fn tree_op_parses_every_variant() {
        let mut callbacks = CallbackHandles::new();
        let callback = callbacks.handle_for(CallbackId(99));
        let spec = json!({ "type": "Text", "id": "text", "props": { "text": "Hi" } });
        let child = json!({ "type": "Button", "id": "button" });
        let cases = vec![
            json!({ "op": "set_tree", "tree": spec }),
            json!({ "op": "insert", "parent_id": "root", "index": 0, "child": child }),
            json!({ "op": "insert_before", "parent_id": "root", "anchor_id": "button", "child": { "type": "Label", "id": "label" } }),
            json!({ "op": "insert_before", "parent_id": "root", "anchor_id": null, "child": { "type": "Label", "id": "tail" } }),
            json!({ "op": "remove", "id": "old" }),
            json!({ "op": "replace", "id": "old", "node": { "type": "Text", "id": "new" } }),
            json!({ "op": "move", "id": "node", "new_parent_id": "root", "index": 1 }),
            json!({ "op": "set_prop", "id": "node", "name": "text", "value": "New" }),
            json!({ "op": "clear_prop", "id": "node", "name": "text" }),
            json!({ "op": "bind_event", "id": "node", "event": "click", "callback": callback }),
            json!({ "op": "clear_event", "id": "node", "event": "click" }),
        ];

        for case in cases {
            let op = tree_op_from_json(case, &callbacks).unwrap();
            let encoded = tree_op_to_json(&op, &mut callbacks).unwrap();
            assert_eq!(tree_op_from_json(encoded, &callbacks).unwrap(), op);
        }
    }

    #[test]
    fn tree_ops_accept_single_or_array() {
        let mut callbacks = CallbackHandles::new();
        let single = tree_ops_from_json(json!({ "op": "remove", "id": "a" }), &callbacks).unwrap();
        assert_eq!(
            single,
            vec![TreeOp::Remove {
                id: "a".to_string()
            }]
        );

        let many = tree_ops_from_json(
            json!([
                { "op": "remove", "id": "a" },
                { "op": "clear_event", "id": "a", "event": "click" }
            ]),
            &callbacks,
        )
        .unwrap();
        assert_eq!(many.len(), 2);
        assert!(
            matches!(tree_ops_to_json(&many, &mut callbacks).unwrap(), Value::Array(values) if values.len() == 2)
        );
    }

    #[test]
    fn callback_invocation_uses_js_field_names() {
        let mut callbacks = CallbackHandles::new();
        let invocation = CallbackInvocation {
            callback_id: CallbackId(11),
            target_id: Some("ok".to_string()),
            event: "click".to_string(),
            payload: Some(ComponentValue::String("payload".to_string())),
        };

        let encoded = callback_invocation_to_json(&invocation, &mut callbacks).unwrap();
        assert!(encoded["callbackId"].as_str().is_some());
        assert_eq!(encoded["targetId"], json!("ok"));
        assert_eq!(
            callback_invocation_from_json(encoded, &callbacks).unwrap(),
            invocation
        );

        let snake_handle = callbacks.handle_for(CallbackId(12));

        let snake_case = json!({
            "callback_id": snake_handle,
            "target_id": null,
            "event": "change",
            "payload": null
        });
        let decoded = callback_invocation_from_json(snake_case, &callbacks).unwrap();
        assert_eq!(decoded.callback_id, CallbackId(12));
        assert_eq!(decoded.target_id, None);
        assert_eq!(decoded.payload, None);
    }

    #[test]
    fn component_schema_round_trips_via_serde_json() {
        let schema = ComponentSchema::new("Text")
            .with_properties(vec![PropertyMeta::new("text", ValueType::String)])
            .with_action(ActionMeta::new("submit"))
            .with_event(EventMeta::new("click"))
            .allow_children(false);

        let encoded = component_schema_to_json(&schema).unwrap();
        assert_eq!(component_schema_from_json(encoded).unwrap(), schema);
    }

    #[test]
    fn invalid_input_reports_context() {
        let callbacks = CallbackHandles::new();
        let missing_op = tree_op_from_json(json!({ "id": "node" }), &callbacks).unwrap_err();
        assert!(missing_op.reason.contains("tree op op"));

        let missing_type =
            component_spec_from_json(json!({ "id": "root" }), &callbacks).unwrap_err();
        assert!(missing_type.reason.contains("component spec type"));

        let numeric_callback = tree_op_from_json(
            json!({ "op": "bind_event", "id": "node", "event": "click", "callback": 99 }),
            &callbacks,
        )
        .unwrap_err();
        assert!(numeric_callback.reason.contains("callback id handle"));

        let missing_bytes_data =
            component_value_from_json(json!({ "$type": "bytes" })).unwrap_err();
        assert!(missing_bytes_data.reason.contains("bytes data"));

        let invalid_rect = component_value_from_json(json!([1, 2, 3, 70000])).unwrap_err();
        assert!(invalid_rect.reason.contains("rect height"));
    }
}
