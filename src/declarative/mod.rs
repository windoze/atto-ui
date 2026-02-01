//! SwiftUI-inspired declarative view system.
//!
//! This module provides lightweight building blocks (`Text`, `Divider`, `VStack`, ...)
//! that can be composed via pure `body()` functions.
//!
//! The declarative layer can also be bridged into Chatty's imperative window/view manager
//! by calling [`DeclarativeView::build_view`], which produces a `Box<dyn crate::view::View>`.

mod primitives;
mod view;
mod view_adapter;
mod vstack;
mod widget_controls;

pub use primitives::{Divider, Spacer, Text, TextFn};
pub use view::{DeclarativeView, EmptyView};
pub use view_adapter::ViewAdapter;
pub use vstack::VStack;

pub use crate::views::EdgeInsets;
pub use crate::widgets::{Button, Checkbox, Label, ListBox, RadioGroup, TableView, TextBox};
