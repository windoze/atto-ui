use std::sync::atomic::{AtomicU64, Ordering};

use ratatui::layout::Rect;

use super::component::Component;
use super::layout::LayoutParams;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ComponentId(pub(crate) u64);

static NEXT_COMPONENT_ID: AtomicU64 = AtomicU64::new(1);

impl ComponentId {
    pub fn next() -> Self {
        Self(NEXT_COMPONENT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

pub struct ComponentNode {
    pub id: ComponentId,
    pub parent: Option<ComponentId>,
    pub view: Box<dyn Component>,
    pub layout: LayoutParams,
    bounds: Rect,
}

impl ComponentNode {
    pub fn new(view: Box<dyn Component>) -> Self {
        Self {
            id: ComponentId::next(),
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
