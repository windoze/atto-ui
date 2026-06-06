use ratatui::layout::Rect;

use crate::composable::scroll::ScrollbarDrag;

use super::{Window, WindowId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowManagerInputMode {
    Normal,
    WindowManagement,
}

#[derive(Debug, Default)]
pub struct WindowManagerAction {
    pub consumed: bool,
    pub close: Option<WindowId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResizeCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum DragKind {
    Move {
        offset_x: u16,
        offset_y: u16,
    },
    Resize {
        start_rect: Rect,
        corner: ResizeCorner,
    },
    Scrollbar {
        drag: ScrollbarDrag,
    },
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DragState {
    pub(crate) window_id: WindowId,
    pub(crate) kind: DragKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HitRegion {
    TitleBar,
    MinimizeButton,
    MaximizeButton,
    CloseButton,
    ResizeHandle(ResizeCorner),
    VScrollbar,
    HScrollbar,
    Body,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct HitTest {
    pub(crate) window_id: WindowId,
    pub(crate) region: HitRegion,
}

#[derive(Default)]
pub struct WindowManager {
    pub(super) next_id: u64,
    pub(super) windows: Vec<Window>,
    pub(super) focused: Option<WindowId>,
    pub(super) drag: Option<DragState>,
    pub(super) mouse_capture: bool,
}
