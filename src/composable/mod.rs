//! Composable component system (single component model).

mod border;
mod component;
mod for_each;
mod geom;
mod grid;
mod identifiable;
mod layout;
mod node;
mod primitives;
pub(crate) mod scroll;
mod scroll_container;
mod splitter;
mod stack;
mod tab_window;

pub use crate::widgets::{
    Button, Checkbox, Label, ListBox, RadioGroup, StyledLabel, TabHeaderPosition, TabView,
    TableView, TextBox,
};
pub use border::Border;
pub use component::{
    Component, ComponentAction, ComponentContext, EventOutcome, EventResult, ScrollbarHost, TabMode,
    TitleBarContent, TitleBarContext, TitleBarSpan,
};
pub use for_each::{ForEach, ForEachIdentifiable};
pub use grid::Grid;
pub use identifiable::Identifiable;
pub use layout::{Align, Anchor, AnchorPlacement, EdgeInsets, LayoutParams, Size};
pub use node::{ComponentId, ComponentNode};
pub use primitives::{Divider, Spacer, Text, TextFn};
pub use scroll::{
    ScrollConfig, ScrollOffset, ScrollbarDrag, ScrollbarHit, ScrollbarLayout1D, ScrollbarVisibility,
    scroll_offset_from_thumb_start, scrollbar_hit_test, scrollbar_layout_1d, should_show_scrollbar,
};
pub use scroll_container::{
    ScrollContainer, ScrollContainerHost, ScrollContainerInfo, ScrollContainerScrollbars,
    ScrollContent, ScrollContentContext, ScrollbarLayout, ScrollbarPlacement,
};
pub use splitter::{Splitter, SplitterOrientation};
pub use stack::{HStack, VStack};
pub use tab_window::TabWindow;

#[cfg(test)]
mod tests;
