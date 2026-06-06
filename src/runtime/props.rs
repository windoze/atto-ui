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
