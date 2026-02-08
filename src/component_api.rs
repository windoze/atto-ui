use std::sync::Arc;

use parking_lot::RwLock as ParkingRwLock;
use ratatui::layout::Rect;

use atto_ui_runtime::ComponentValue;

use crate::composable::EdgeInsets;

pub trait ComponentValueCodec: Sized {
    fn to_component_value(&self) -> ComponentValue;
    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError>;
}

pub trait ComponentPropertySchema {
    fn property_schema() -> Vec<atto_ui_runtime::PropertyMeta>;
}

impl<T: ComponentPropertySchema> ComponentPropertySchema for Box<T> {
    fn property_schema() -> Vec<atto_ui_runtime::PropertyMeta> {
        T::property_schema()
    }
}

impl<T: ComponentPropertySchema> ComponentPropertySchema for Arc<T> {
    fn property_schema() -> Vec<atto_ui_runtime::PropertyMeta> {
        T::property_schema()
    }
}

impl<T: ComponentPropertySchema> ComponentPropertySchema for ParkingRwLock<T> {
    fn property_schema() -> Vec<atto_ui_runtime::PropertyMeta> {
        T::property_schema()
    }
}

impl<T: ComponentPropertySchema> ComponentPropertySchema for std::sync::RwLock<T> {
    fn property_schema() -> Vec<atto_ui_runtime::PropertyMeta> {
        T::property_schema()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ComponentCommand {
    Click,
    Toggle,
    InputText(String),
    SelectIndex(usize),
    Submit,
    Custom { name: String, payload: Vec<u8> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentTarget {
    Id(String),
    Focused,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentError {
    NotFound(String),
    UnsupportedProperty(String),
    InvalidValue {
        name: String,
        expected: &'static str,
    },
    ActionNotSupported(String),
    RenderFailed(String),
}

impl ComponentError {
    pub fn not_found(id: impl Into<String>) -> Self {
        ComponentError::NotFound(id.into())
    }

    pub fn unsupported_property(name: impl Into<String>) -> Self {
        ComponentError::UnsupportedProperty(name.into())
    }

    pub fn invalid_value(name: impl Into<String>, expected: &'static str) -> Self {
        ComponentError::InvalidValue {
            name: name.into(),
            expected,
        }
    }

    pub fn action_not_supported(name: impl Into<String>) -> Self {
        ComponentError::ActionNotSupported(name.into())
    }

    pub(crate) fn render_failed(err: impl ToString) -> Self {
        ComponentError::RenderFailed(err.to_string())
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
        Ok(expect_usize(value, name)? as u32)
    }
}

impl ComponentValueCodec for u16 {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::U64(*self as u64)
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        Ok(expect_usize(value, name)? as u16)
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
        ComponentValue::Rect(atto_ui_runtime::Rect {
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
        map.insert("bottom".to_string(), ComponentValue::U64(self.bottom as u64));
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
                    return Err(ComponentError::invalid_value(
                        name,
                        "edge insets list of 4",
                    ));
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
