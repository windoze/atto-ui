//! View hierarchy and layout containers.
//!
//! This module introduces a "view tree" layer on top of the core [`crate::view::View`] trait.
//! Containers like [`VBox`] and [`HBox`] own child views, compute layout in `draw()`, and route
//! events to the appropriate child view in `handle_event()`.

mod control_view;
mod grid;
mod layout;
mod node;
mod vbox;

pub use control_view::ControlView;
pub use grid::Grid;
pub use layout::{Align, Anchor, AnchorPlacement, EdgeInsets, LayoutParams, Size};
pub use node::{ViewId, ViewNode};
pub use vbox::{HBox, VBox};

#[cfg(test)]
mod tests;
