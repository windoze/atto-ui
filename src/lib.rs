#![forbid(unsafe_code)]

pub mod app;
pub mod text;
pub mod theme;
pub mod view;
pub mod wm;

pub use wm::{Window, WindowDecorations, WindowId, WindowKind, WindowManager, WindowState};
