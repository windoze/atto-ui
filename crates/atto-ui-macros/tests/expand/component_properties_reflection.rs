#![allow(dead_code)]

extern crate self as atto_ui;

use atto_ui_macros::{ComponentProperties, component_properties};

pub mod reactive {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Debug)]
    pub struct DirtySignal;

    #[derive(Clone)]
    pub struct Binding<T> {
        value: Rc<RefCell<T>>,
    }

    impl<T: Clone + PartialEq> Binding<T> {
        pub fn new(value: T) -> Self {
            Self {
                value: Rc::new(RefCell::new(value)),
            }
        }

        pub fn get(&self) -> T {
            self.value.borrow().clone()
        }

        pub fn set(&self, value: T) {
            *self.value.borrow_mut() = value;
        }

        pub fn dirty_signal(&self) -> DirtySignal {
            DirtySignal
        }
    }

    impl<T: Clone + PartialEq> From<T> for Binding<T> {
        fn from(value: T) -> Self {
            Binding::new(value)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ComponentValue {
    Bool(bool),
    String(String),
    U64(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    Bool,
    String,
    U64,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyMeta {
    pub name: String,
    pub value_type: ValueType,
}

impl PropertyMeta {
    pub fn new(name: impl Into<String>, value_type: ValueType) -> Self {
        Self {
            name: name.into(),
            value_type,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentError {
    UnsupportedProperty(String),
    InvalidValue {
        name: String,
        expected: &'static str,
    },
}

impl ComponentError {
    pub fn unsupported_property(name: impl Into<String>) -> Self {
        ComponentError::UnsupportedProperty(name.into())
    }

    pub fn invalid_value(name: impl Into<String>, expected: &'static str) -> Self {
        ComponentError::InvalidValue {
            name: name.into(),
            expected,
        }
    }
}

pub trait ComponentValueCodec: Sized {
    fn to_component_value(&self) -> ComponentValue;
    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError>;
}

impl ComponentValueCodec for String {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::String(self.clone())
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        match value {
            ComponentValue::String(value) => Ok(value),
            _ => Err(ComponentError::invalid_value(name, "string")),
        }
    }
}

impl ComponentValueCodec for bool {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::Bool(*self)
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        match value {
            ComponentValue::Bool(value) => Ok(value),
            _ => Err(ComponentError::invalid_value(name, "bool")),
        }
    }
}

impl ComponentValueCodec for u64 {
    fn to_component_value(&self) -> ComponentValue {
        ComponentValue::U64(*self)
    }

    fn from_component_value(value: ComponentValue, name: &str) -> Result<Self, ComponentError> {
        match value {
            ComponentValue::U64(value) => Ok(value),
            _ => Err(ComponentError::invalid_value(name, "u64")),
        }
    }
}

pub trait ComponentPropertySchema {
    fn property_schema() -> Vec<PropertyMeta>;
}

pub trait Component {
    fn property_names(&self) -> Vec<&'static str>;
    fn get_property(&self, name: &str) -> Option<ComponentValue>;
    fn set_property(
        &mut self,
        name: &str,
        value: ComponentValue,
    ) -> Result<(), ComponentError>;
    fn dirty_signals(&self) -> Vec<reactive::DirtySignal>;
}

use reactive::Binding;

#[derive(ComponentProperties)]
struct Panel {
    title: Binding<String>,
    #[component(rename = "enabled")]
    active: Binding<bool>,
    count: Binding<u64>,
    #[component(skip)]
    internal: Binding<String>,
}

#[component_properties]
impl Component for Panel {}

fn main() {
    let mut panel = Panel {
        title: "hello".to_string().into(),
        active: true.into(),
        count: 3.into(),
        internal: "hidden".to_string().into(),
    };

    assert_eq!(panel.property_names(), vec!["title", "enabled", "count"]);
    assert_eq!(
        panel.get_property("title"),
        Some(ComponentValue::String("hello".to_string()))
    );
    assert_eq!(panel.get_property("internal"), None);

    panel
        .set_property("enabled", ComponentValue::Bool(false))
        .unwrap();
    assert_eq!(panel.get_property("enabled"), Some(ComponentValue::Bool(false)));

    let schema = Panel::property_schema();
    assert_eq!(schema[0].name, "title");
    assert_eq!(schema[0].value_type, ValueType::String);
    assert_eq!(schema[1].name, "enabled");
    assert_eq!(schema[1].value_type, ValueType::Bool);
}
