//! View hierarchy infrastructure and shared view helpers.
//!
//! This module introduces a "view tree" layer on top of the core [`crate::view::View`] trait.
//! It also provides shared layout/scroll primitives plus a few helper views (`BorderView`,
//! `ScrollView`, ...).
//!
//! Most apps should prefer the declarative containers in [`crate::declarative`] (`VStack`, `HStack`,
//! `Grid`) instead of building view trees imperatively.

mod border;
mod control_view;
pub(crate) mod layout;
mod markdown_viewer;
mod node;
pub(crate) mod scroll;
mod scroll_view;

pub use border::BorderView;
pub use control_view::ControlView;
pub use layout::{Align, Anchor, AnchorPlacement, EdgeInsets, LayoutParams, Size};
pub use markdown_viewer::MarkdownViewer;
pub use node::{ViewId, ViewNode};
pub use scroll::{ScrollConfig, ScrollOffset, ScrollbarVisibility};
pub use scroll_view::{
    ScrollContent, ScrollContentContext, ScrollView, ScrollViewHost, ScrollViewInfo,
    ScrollViewScrollbars, ScrollbarLayout, ScrollbarPlacement,
};

pub(crate) use scroll::{
    ScrollbarDrag, ScrollbarHit, scroll_offset_from_thumb_start, scrollbar_hit_test,
    scrollbar_layout_1d, should_show_scrollbar,
};
