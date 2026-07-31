use crate::ComponentValueCodec;
use crate::composable::{Align, Anchor, AnchorPlacement, EdgeInsets, LayoutParams, Size};

use super::{
    AlignSpec, AnchorPlacementSpec, AnchorSpec, ComponentSpec, ComponentValue, EdgeInsetsSpec,
    LayoutSpec, SizeSpec, TreeError,
};

pub(super) fn layout_from_spec(spec: &LayoutSpec) -> LayoutParams {
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
        // Non-finite floats are rejected here for the same reason `expect_f64` rejects them: they
        // survive a JSON round-trip only in one direction. Keeping the build-time and set-time paths
        // in agreement matters because a spec built from a rejected value would otherwise be
        // reachable through `rebuild`.
        Some(value) => match value.as_f64() {
            Some(v) if v.is_finite() => Ok(Some(v)),
            Some(_) => Err(invalid_prop(spec, name, "finite f64", value)),
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

pub(super) fn prop_edge_insets(
    spec: &ComponentSpec,
    name: &str,
) -> Result<Option<EdgeInsets>, TreeError> {
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
    // Single source of truth for the ComponentValue -> EdgeInsets conversion lives on the codec
    // (`component_api.rs`); the build path just adapts the error into `TreeError`. Keeping one
    // implementation prevents the build-time and set_property-time rules from drifting apart.
    EdgeInsets::from_component_value(value.clone(), name)
        .map_err(|_| invalid_prop(spec, name, "padding", value))
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
