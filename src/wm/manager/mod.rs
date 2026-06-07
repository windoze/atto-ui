mod chrome;
mod core;
mod docking;
mod draw;
mod events;
mod focus;
mod placement;
#[cfg(test)]
mod tests;
mod types;
mod z_order;

pub use types::{WindowManager, WindowManagerAction, WindowManagerInputMode};

pub(crate) use super::{
    DockAutoHide, DockSide, Window, WindowBorderStyle, WindowButtons, WindowDock, WindowId,
    WindowKind, WindowMinSizeMode, WindowState,
};
pub(crate) use types::{DragKind, DragState, GlobalDragState, HitRegion, HitTest, ResizeCorner};
