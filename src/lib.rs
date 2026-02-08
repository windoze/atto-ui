#![forbid(unsafe_code)]

extern crate self as atto_ui;

pub mod app;
pub mod inspect;
pub mod cache;
pub mod composable;
pub mod dialogs;
pub mod runtime;
pub mod reactive;
pub mod text;
pub mod theme;
pub mod widgets;
pub mod wm;

pub use atto_ui_runtime::ComponentValue;
pub use inspect::{
    ComponentCommand, ComponentError, ComponentProps, ComponentTarget, ComponentValueCodec,
    ComponentValueExt, DesktopInspector, InspectNode, InspectSnapshot,
};
pub use wm::{
    Window, WindowBorderStyle, WindowButtons, WindowDecorations, WindowId, WindowKind,
    WindowManager, WindowMinSizeMode, WindowState,
};
