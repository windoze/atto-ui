//! Composable component system (single component model).

mod border;
mod component;
mod for_each;
mod grid;
mod identifiable;
mod layout;
mod node;
mod primitives;
pub(crate) mod scroll;
mod scroll_container;
mod stack;

pub use border::Border;
pub use component::{
    Component, ComponentAction, ComponentContext, EventOutcome, EventResult, ScrollbarHost,
    TabMode,
};
pub use for_each::{ForEach, ForEachIdentifiable};
pub use grid::Grid;
pub use identifiable::Identifiable;
pub use layout::{Align, Anchor, AnchorPlacement, EdgeInsets, LayoutParams, Size};
pub use node::{ComponentId, ComponentNode};
pub use primitives::{Divider, Spacer, Text, TextFn};
pub use scroll::{ScrollConfig, ScrollOffset, ScrollbarVisibility};
pub use scroll_container::{
    ScrollContainer, ScrollContent, ScrollContentContext, ScrollContainerHost, ScrollContainerInfo,
    ScrollContainerScrollbars, ScrollbarLayout, ScrollbarPlacement,
};
pub use stack::{HStack, VStack};
pub use crate::widgets::{Button, Checkbox, Label, ListBox, RadioGroup, TableView, TextBox};

#[cfg(test)]
mod tests;
