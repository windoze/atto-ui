use std::sync::Arc;

use parking_lot::RwLock as ParkingRwLock;
use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

use crate::composable::EdgeInsets;
use crate::runtime::{ComponentValue, PropertyMeta, Rect as RuntimeRect};

pub trait ComponentValueCodec: Sized {
    fn to_component_value(&self) -> ComponentValue;
    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError>;
}

pub trait ComponentPropertySchema {
    fn property_schema() -> Vec<PropertyMeta>;
}

impl<T: ComponentPropertySchema> ComponentPropertySchema for Box<T> {
    fn property_schema() -> Vec<PropertyMeta> {
        T::property_schema()
    }
}

impl<T: ComponentPropertySchema> ComponentPropertySchema for Arc<T> {
    fn property_schema() -> Vec<PropertyMeta> {
        T::property_schema()
    }
}

impl<T: ComponentPropertySchema> ComponentPropertySchema for ParkingRwLock<T> {
    fn property_schema() -> Vec<PropertyMeta> {
        T::property_schema()
    }
}

impl<T: ComponentPropertySchema> ComponentPropertySchema for std::sync::RwLock<T> {
    fn property_schema() -> Vec<PropertyMeta> {
        T::property_schema()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ComponentCommand {
    Click,
    Toggle,
    InputText(String),
    SelectIndex(usize),
    Submit,
    Custom { name: String, payload: Vec<u8> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentTarget {
    Id(String),
    Focused,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentError {
    NotFound(String),
    UnsupportedProperty(String),
    InvalidValue { name: String, expected: String },
    ActionNotSupported(String),
    RenderFailed(String),
    Timeout(String),
}

impl ComponentError {
    pub fn not_found(id: impl Into<String>) -> Self {
        ComponentError::NotFound(id.into())
    }

    pub fn unsupported_property(name: impl Into<String>) -> Self {
        ComponentError::UnsupportedProperty(name.into())
    }

    pub fn invalid_value(name: impl Into<String>, expected: impl Into<String>) -> Self {
        ComponentError::InvalidValue {
            name: name.into(),
            expected: expected.into(),
        }
    }

    pub fn action_not_supported(name: impl Into<String>) -> Self {
        ComponentError::ActionNotSupported(name.into())
    }

    pub(crate) fn render_failed(err: impl ToString) -> Self {
        ComponentError::RenderFailed(err.to_string())
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        ComponentError::Timeout(message.into())
    }
}

impl ComponentValueCodec for String {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::String(self.clone())
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        expect_string(value, name)
    }
}

impl ComponentValueCodec for bool {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::Bool(*self)
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        expect_bool(value, name)
    }
}

impl ComponentValueCodec for f64 {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::F64(*self)
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        expect_f64(value, name)
    }
}

impl ComponentValueCodec for f32 {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::F64(*self as f64)
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        Ok(expect_f64(value, name)? as f32)
    }
}

impl ComponentValueCodec for i64 {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::I64(*self)
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        match value {
            ComponentValue::I64(v) => Ok(v),
            ComponentValue::U64(v) => Ok(v as i64),
            ComponentValue::F64(v) => Ok(v as i64),
            _ => Err(ComponentError::invalid_value(name, "i64")),
        }
    }
}

impl ComponentValueCodec for u64 {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::U64(*self)
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        match value {
            ComponentValue::U64(v) => Ok(v),
            ComponentValue::I64(v) if v >= 0 => Ok(v as u64),
            ComponentValue::F64(v) if v >= 0.0 => Ok(v as u64),
            _ => Err(ComponentError::invalid_value(name, "u64")),
        }
    }
}

impl ComponentValueCodec for usize {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::U64(*self as u64)
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        expect_usize(value, name)
    }
}

impl ComponentValueCodec for u32 {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::U64(*self as u64)
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        // Clamp (not truncate) on overflow, matching the build-time `prop_u16`/`prop_*` rules.
        Ok(expect_usize(value, name)?.min(u32::MAX as usize) as u32)
    }
}

impl ComponentValueCodec for u16 {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::U64(*self as u64)
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        // Clamp (not truncate) on overflow, matching the build-time `prop_u16` rule.
        Ok(expect_usize(value, name)?.min(u16::MAX as usize) as u16)
    }
}

impl ComponentValueCodec for Vec<String> {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::StringList(self.clone())
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        expect_string_list(value, name)
    }
}

impl ComponentValueCodec for Vec<Vec<String>> {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::Table(self.clone())
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        expect_table(value, name)
    }
}

impl ComponentValueCodec for Rect {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::Rect(RuntimeRect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        })
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        expect_rect(value, name)
    }
}

impl ComponentValueCodec for EdgeInsets {
    fn to_component_value(&self) -> ComponentValue {
        let mut map = std::collections::BTreeMap::new();
        map.insert("top".to_string(), ComponentValue::U64(self.top as u64));
        map.insert("right".to_string(), ComponentValue::U64(self.right as u64));
        map.insert(
            "bottom".to_string(),
            ComponentValue::U64(self.bottom as u64),
        );
        map.insert("left".to_string(), ComponentValue::U64(self.left as u64));
        ComponentValue::Map(map)
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        match value {
            ComponentValue::U64(v) => {
                let val = v.min(u16::MAX as u64) as u16;
                Ok(EdgeInsets::all(val))
            }
            ComponentValue::I64(v) if v >= 0 => {
                let val = (v as u64).min(u16::MAX as u64) as u16;
                Ok(EdgeInsets::all(val))
            }
            ComponentValue::F64(v) if v >= 0.0 => {
                let val = (v as u64).min(u16::MAX as u64) as u16;
                Ok(EdgeInsets::all(val))
            }
            ComponentValue::List(values) => {
                if values.len() != 4 {
                    return Err(ComponentError::invalid_value(name, "edge insets list of 4"));
                }
                let to_u16 = |idx: usize| -> Result<u16, ComponentError> {
                    values
                        .get(idx)
                        .and_then(|v| v.as_u64())
                        .map(|v| v.min(u16::MAX as u64) as u16)
                        .ok_or_else(|| ComponentError::invalid_value(name, "edge insets"))
                };
                Ok(EdgeInsets {
                    top: to_u16(0)?,
                    right: to_u16(1)?,
                    bottom: to_u16(2)?,
                    left: to_u16(3)?,
                })
            }
            ComponentValue::Map(map) => {
                let to_u16 = |key: &str| -> u16 {
                    map.get(key)
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                        .min(u16::MAX as u64) as u16
                };
                Ok(EdgeInsets {
                    top: to_u16("top"),
                    right: to_u16("right"),
                    bottom: to_u16("bottom"),
                    left: to_u16("left"),
                })
            }
            _ => Err(ComponentError::invalid_value(name, "edge insets")),
        }
    }
}

impl ComponentValueCodec for crate::widgets::TabHeaderPosition {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::String(format!("{:?}", self))
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        let v = expect_string(value, name)?;
        crate::widgets::TabHeaderPosition::parse(&v)
            .ok_or_else(|| ComponentError::invalid_value(name, "Top/Bottom"))
    }
}

impl ComponentValueCodec for crate::widgets::DisclosureStatus {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::String(format!("{:?}", self))
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        let v = expect_string(value, name)?;
        crate::widgets::DisclosureStatus::parse(&v)
            .ok_or_else(|| ComponentError::invalid_value(name, "Idle/Running/Done/Error/Canceled"))
    }
}

impl ComponentValueCodec for crate::wm::WindowMinSizeMode {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::String(format!("{:?}", self))
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        let v = expect_string(value, name)?;
        crate::wm::WindowMinSizeMode::parse(&v)
            .ok_or_else(|| ComponentError::invalid_value(name, "WindowMinSizeMode"))
    }
}

impl ComponentValueCodec for crate::composable::DividerOrientation {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::String(format!("{:?}", self))
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        let v = expect_string(value, name)?;
        crate::composable::DividerOrientation::parse(&v)
            .ok_or_else(|| ComponentError::invalid_value(name, "Horizontal/Vertical"))
    }
}

impl ComponentValueCodec for crate::composable::SplitterOrientation {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::String(format!("{:?}", self))
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        let v = expect_string(value, name)?;
        crate::composable::SplitterOrientation::parse(&v)
            .ok_or_else(|| ComponentError::invalid_value(name, "Vertical/Horizontal"))
    }
}

fn expect_bool(value: ComponentValue, name: &str) -> Result<bool, ComponentError> {
    match value {
        ComponentValue::Bool(v) => Ok(v),
        _ => Err(ComponentError::invalid_value(name, "bool")),
    }
}

fn expect_f64(value: ComponentValue, name: &str) -> Result<f64, ComponentError> {
    match value {
        // Reject NaN/Inf at the boundary. `serde_json` encodes non-finite floats as `null`
        // *without* erroring, and `null` then fails to deserialize back into an `f64`. Letting one
        // in here means the write succeeds, the read-back succeeds, and the failure only surfaces
        // as an opaque decode error in whatever process is on the far side of the IPC socket.
        ComponentValue::F64(v) if !v.is_finite() => {
            Err(ComponentError::invalid_value(name, "finite number"))
        }
        ComponentValue::F64(v) => Ok(v),
        ComponentValue::I64(v) => Ok(v as f64),
        ComponentValue::U64(v) => Ok(v as f64),
        _ => Err(ComponentError::invalid_value(name, "number")),
    }
}

fn expect_usize(value: ComponentValue, name: &str) -> Result<usize, ComponentError> {
    match value {
        ComponentValue::U64(v) => Ok(v as usize),
        ComponentValue::I64(v) if v >= 0 => Ok(v as usize),
        ComponentValue::F64(v) if v >= 0.0 => Ok(v as usize),
        _ => Err(ComponentError::invalid_value(name, "usize")),
    }
}

fn expect_string(value: ComponentValue, name: &str) -> Result<String, ComponentError> {
    match value {
        ComponentValue::String(v) => Ok(v),
        _ => Err(ComponentError::invalid_value(name, "string")),
    }
}

fn expect_string_list(value: ComponentValue, name: &str) -> Result<Vec<String>, ComponentError> {
    match value {
        ComponentValue::StringList(v) => Ok(v),
        _ => Err(ComponentError::invalid_value(name, "string list")),
    }
}

fn expect_table(value: ComponentValue, name: &str) -> Result<Vec<Vec<String>>, ComponentError> {
    match value {
        ComponentValue::Table(v) => Ok(v),
        _ => Err(ComponentError::invalid_value(name, "table")),
    }
}

fn expect_rect(value: ComponentValue, name: &str) -> Result<Rect, ComponentError> {
    match value {
        ComponentValue::Rect(v) => Ok(Rect::new(v.x, v.y, v.width, v.height)),
        _ => Err(ComponentError::invalid_value(name, "rect")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A non-finite float encodes to JSON `null` without error and then refuses to decode back into
    /// an `f64`, so accepting one here would push the failure across the IPC boundary where its
    /// cause is no longer visible. Reject it at the point of entry instead.
    #[test]
    fn f64_codec_rejects_non_finite_values() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let err = f64::from_component_value(ComponentValue::F64(bad), "value")
                .expect_err("non-finite value must be rejected");
            assert_eq!(
                err,
                ComponentError::invalid_value("value", "finite number"),
                "unexpected error for {bad}"
            );

            assert!(
                f32::from_component_value(ComponentValue::F64(bad), "value").is_err(),
                "f32 codec must reject {bad} too"
            );
        }
    }

    #[test]
    fn f64_codec_accepts_finite_values_and_integer_widening() {
        assert_eq!(
            f64::from_component_value(ComponentValue::F64(1.5), "value"),
            Ok(1.5)
        );
        assert_eq!(
            f64::from_component_value(ComponentValue::I64(-3), "value"),
            Ok(-3.0)
        );
        assert_eq!(
            f64::from_component_value(ComponentValue::U64(7), "value"),
            Ok(7.0)
        );
    }
}
