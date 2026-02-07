#![forbid(unsafe_code)]

extern crate self as atto_ui;

pub mod app;
pub mod automation;
pub mod cache;
pub mod composable;
pub mod dialogs;
pub mod reactive;
pub mod text;
pub mod theme;
pub mod widgets;
pub mod wm;

pub use wm::{
    Window, WindowBorderStyle, WindowButtons, WindowDecorations, WindowId, WindowKind,
    WindowManager, WindowMinSizeMode, WindowState,
};
