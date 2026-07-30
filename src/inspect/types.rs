//! Snapshot/inspect node types and result value types for the inspector.

use std::collections::BTreeMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

use crate::composable::EventResult;
use crate::reactive::{DirtySignal, DirtySignalSet};
use crate::runtime::{ComponentValue, Rect as RuntimeRect};
use crate::wm::WindowId;
use crate::ComponentTarget;

use super::{buffer_to_string, crop_buffer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Desktop,
    MenuBar,
    Menu,
    MenuItem,
    StatusBar,
    Window,
    Component,
}

#[derive(Clone, Debug)]
pub struct InspectNode {
    pub kind: NodeKind,
    pub id: Option<String>,
    pub name: String,
    pub type_id: String,
    pub bounds: Option<Rect>,
    pub properties: Vec<String>,
    pub focusable: bool,
    pub window_id: Option<WindowId>,
    pub children: Vec<InspectNode>,
}

impl InspectNode {
    pub fn find_by_id(&self, id: &str) -> Option<&InspectNode> {
        if self.id.as_deref() == Some(id) {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_by_id(id) {
                return Some(found);
            }
        }
        None
    }
}

#[derive(Clone, Debug)]
pub struct InspectSnapshot {
    pub buffer: Buffer,
    pub tree: InspectNode,
}

/// Serializable desktop snapshot for host-language assertions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesktopSnapshot {
    pub bounds: RuntimeRect,
    pub tree: DesktopSnapshotNode,
}

/// Serializable node in a desktop snapshot tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesktopSnapshotNode {
    pub kind: NodeKind,
    pub id: Option<String>,
    pub tag: Option<String>,
    pub name: String,
    pub type_name: String,
    pub bounds: Option<RuntimeRect>,
    pub text: Option<String>,
    pub state: Option<String>,
    pub window_id: Option<u64>,
    pub properties: BTreeMap<String, ComponentValue>,
    pub children: Vec<DesktopSnapshotNode>,
}

impl DesktopSnapshotNode {
    pub fn find_by_id(&self, id: &str) -> Option<&DesktopSnapshotNode> {
        if self.id.as_deref() == Some(id) {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_by_id(id) {
                return Some(found);
            }
        }
        None
    }
}

impl InspectSnapshot {
    pub fn contents(&self) -> String {
        buffer_to_string(&self.buffer)
    }

    pub fn component_buffer(&self, id: &str) -> Option<Buffer> {
        let node = self.tree.find_by_id(id)?;
        let area = node.bounds?;
        Some(crop_buffer(&self.buffer, area))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvokeDispatch {
    Semantic,
    CoordinateFallback,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InvokeResult {
    pub dispatch: InvokeDispatch,
    pub result: EventResult,
}

impl InvokeResult {
    pub(super) fn new(dispatch: InvokeDispatch, result: EventResult) -> Self {
        Self { dispatch, result }
    }

    pub(super) fn semantic(result: EventResult) -> Self {
        Self::new(InvokeDispatch::Semantic, result)
    }

    pub(super) fn coordinate_fallback(result: EventResult) -> Self {
        Self::new(InvokeDispatch::CoordinateFallback, result)
    }

    pub(super) fn unsupported() -> Self {
        Self::new(InvokeDispatch::Unsupported, EventResult::ignored())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WaitCondition {
    PropertyEquals {
        target: ComponentTarget,
        property: String,
        expected: ComponentValue,
    },
}

impl WaitCondition {
    pub fn property_equals(
        target: ComponentTarget,
        property: impl Into<String>,
        expected: ComponentValue,
    ) -> Self {
        Self::PropertyEquals {
            target,
            property: property.into(),
            expected,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WaitResult {
    pub polls: u64,
    pub value: Option<ComponentValue>,
}

#[derive(Clone, Debug, Default)]
pub struct DesktopChangeTracker {
    signals: DirtySignalSet,
}

impl DesktopChangeTracker {
    pub fn new(signals: Vec<DirtySignal>) -> Self {
        Self {
            signals: DirtySignalSet::new(signals),
        }
    }

    pub fn changed_since_last_poll(&mut self) -> bool {
        self.signals.changed_since_last_poll()
    }

    pub fn refresh(&mut self, signals: Vec<DirtySignal>) {
        self.signals.refresh(signals);
    }

    pub fn signal_count(&self) -> usize {
        self.signals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }
}
