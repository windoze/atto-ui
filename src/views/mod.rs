//! View hierarchy and layout containers.
//!
//! This module introduces a "view tree" layer on top of the core [`crate::view::View`] trait.
//! Containers like [`VBox`] and [`HBox`] own child views, compute layout in `draw()`, and route
//! events to the appropriate child view in `handle_event()`.

mod border;
mod control_view;
mod grid;
mod layout;
mod node;
mod scroll;
mod scroll_view;
mod vbox;

pub use border::BorderView;
pub use control_view::ControlView;
pub use grid::Grid;
pub use layout::{Align, Anchor, AnchorPlacement, EdgeInsets, LayoutParams, Size};
pub use node::{ViewId, ViewNode};
pub use scroll::{ScrollConfig, ScrollOffset, ScrollbarVisibility};
pub use scroll_view::{
    ScrollContent, ScrollContentContext, ScrollView, ScrollViewHost, ScrollViewInfo,
    ScrollViewScrollbars, ScrollbarLayout, ScrollbarPlacement,
};
pub use vbox::{HBox, VBox};

pub(crate) use scroll::{
    ScrollbarDrag, ScrollbarHit, scroll_offset_from_thumb_start, scrollbar_hit_test,
    scrollbar_layout_1d, should_show_scrollbar,
};

#[cfg(test)]
mod tests;
