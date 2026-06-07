//! Shared drag-and-drop data types for composable components.

use std::path::PathBuf;

use ratatui::layout::Rect;

use super::node::ComponentId;
use crate::wm::WindowId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DragPayloadType(pub &'static str);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DragPayload {
    Text(String),
    FilePath(PathBuf),
    ComponentId(ComponentId),
    WindowId(WindowId),
    Custom { ty: DragPayloadType, data: Vec<u8> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragOperation {
    Copy,
    Move,
    Link,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragSource {
    pub payload: DragPayload,
    pub operation: DragOperation,
    pub threshold: u16,
    pub ghost: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragOffer<'a> {
    pub payload: &'a DragPayload,
    pub operation: DragOperation,
    pub screen_x: u16,
    pub screen_y: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DropEffect {
    #[default]
    None,
    Copy,
    Move,
    Link,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DropFeedback {
    pub effect: DropEffect,
    pub rect: Option<Rect>,
    pub label: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragContext<'a> {
    pub payload: &'a DragPayload,
    pub operation: DragOperation,
    pub source_window: WindowId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composable::{
        ComponentContext, DragAndDrop, MouseCoordinateSpace, ScrollbarHost, TabMode,
    };
    use crate::theme::Theme;

    struct RejectingTarget;

    impl DragAndDrop for RejectingTarget {}

    #[test]
    fn default_drop_feedback_rejects_drop() {
        let feedback = DropFeedback::default();

        assert_eq!(feedback.effect, DropEffect::None);
        assert_eq!(feedback.rect, None);
        assert_eq!(feedback.label, None);
    }

    #[test]
    fn default_drag_over_returns_reject_feedback() {
        let theme = Theme::dark();
        let mut target = RejectingTarget;
        let payload = DragPayload::Text("hello".to_string());
        let offer = DragOffer {
            payload: &payload,
            operation: DragOperation::Copy,
            screen_x: 2,
            screen_y: 3,
        };
        let ctx = ComponentContext {
            theme: &theme,
            window_id: WindowId::from_raw(1),
            is_focused: false,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Bubble,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        };

        assert_eq!(target.drag_over(offer, ctx), DropFeedback::default());
    }
}
