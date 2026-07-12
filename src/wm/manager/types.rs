use std::collections::HashMap;

use ratatui::layout::Rect;

use crate::composable::scroll::ScrollbarDrag;
use crate::composable::{ComponentId, DragSource, DropFeedback, EventResult};

use super::{DockSide, Window, WindowId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowManagerInputMode {
    Normal,
    WindowManagement,
}

#[derive(Debug, Default)]
pub struct WindowManagerAction {
    pub consumed: bool,
    pub close: Option<WindowId>,
    pub component_result: Option<(WindowId, EventResult)>,
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
    DockResize {
        start_size: u16,
        side: DockSide,
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

#[derive(Clone, Debug)]
pub(crate) struct GlobalDragState {
    pub(crate) source_window: WindowId,
    pub(crate) source_component: Option<ComponentId>,
    pub(crate) start_x: u16,
    pub(crate) start_y: u16,
    pub(crate) last_x: u16,
    pub(crate) last_y: u16,
    pub(crate) source: DragSource,
    pub(crate) active: bool,
    pub(crate) feedback: Option<DropFeedback>,
    pub(crate) target_window: Option<WindowId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HitRegion {
    TitleBar,
    MinimizeButton,
    MaximizeButton,
    CloseButton,
    ResizeHandle(ResizeCorner),
    DockResizeEdge(DockSide),
    DockAutoHideHandle,
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
    pub(super) window_index: HashMap<WindowId, usize>,
    pub(super) focused: Option<WindowId>,
    pub(super) drag: Option<DragState>,
    pub(super) global_drag: Option<GlobalDragState>,
    pub(super) mouse_capture: bool,
    /// Window whose content has captured the pointer (a component inside it
    /// requested capture on mouse down). While set, mouse events are routed
    /// straight to that window's view, bypassing chrome/hit-test, until release.
    pub(super) pointer_capture: Option<WindowId>,
}
