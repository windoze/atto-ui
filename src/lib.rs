#![forbid(unsafe_code)]

extern crate self as atto_ui;

pub mod app;
pub mod component_api;
pub mod composable;
pub mod dialogs;
mod drawing;
pub mod inspect;
pub mod reactive;
pub mod runtime;
pub mod text;
pub mod theme;
pub mod widgets;
pub mod wm;

pub use component_api::{
    ComponentCommand, ComponentError, ComponentPropertySchema, ComponentTarget, ComponentValueCodec,
};
pub use inspect::{DesktopInspector, InspectNode, InspectSnapshot};
pub use runtime::{
    ActionMeta, CallbackId, CallbackInvocation, CallbackRegistry, ComponentRegistry,
    ComponentSchema, ComponentSpec, ComponentSpecChild, ComponentValue, EventMeta, PropertyMeta,
    TreeError, TreeOp, ValueType,
};
pub use wm::{
    Window, WindowBorderStyle, WindowButtons, WindowDecorations, WindowId, WindowKind,
    WindowManager, WindowMinSizeMode, WindowState,
};
