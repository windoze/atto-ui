use std::sync::atomic::{AtomicU64, Ordering};

use ratatui::layout::Rect;

use crate::view::View;

use super::LayoutParams;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ViewId(pub(crate) u64);

static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(1);

impl ViewId {
    pub fn next() -> Self {
        Self(NEXT_VIEW_ID.fetch_add(1, Ordering::Relaxed))
    }
}

pub struct ViewNode {
    pub id: ViewId,
    pub parent: Option<ViewId>,
    pub view: Box<dyn View>,
    pub layout: LayoutParams,
    bounds: Rect,
}

impl ViewNode {
    pub fn new(view: Box<dyn View>) -> Self {
        Self {
            id: ViewId::next(),
            parent: None,
            view,
            layout: LayoutParams::default(),
            bounds: Rect::default(),
        }
    }

    pub fn with_layout(mut self, layout: LayoutParams) -> Self {
        self.layout = layout;
        self
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }
}
