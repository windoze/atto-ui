//! Split-pane container for terminal views.
//!
//! The pane group keeps tmux-like pane layout inside one `atto-ui` window. It deliberately stays
//! above `TerminalEmulator`: each pane is still a normal terminal component, while this container
//! owns the pane tree, pane focus, and pane-level prefix commands.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, anyhow, bail};
use atto_ui::composable::{
    Component, ComponentContext, DynamicTree, EventHandling, EventResult, FocusNav, Layout,
    MouseCoordinateSpace, Scrollable, ScrollbarHost, TabMode,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent};
use parking_lot::Mutex;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::{TerminalConfig, TerminalEmulator, TerminalHandle, TerminalShortcut};

const DIVIDER_THICKNESS: u16 = 1;
const PANE_RESIZE_STEP: u16 = 1;

/// Stable identifier for one terminal pane inside a [`TerminalPaneGroup`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TerminalPaneId(u64);

impl TerminalPaneId {
    /// Builds a pane id from its protocol/display value.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the numeric pane id for display and tests.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Allocates a globally-unique pane id.
    ///
    /// Pane ids must be unique across *all* pane groups in the process, not
    /// just within one group: the IPC layer ([`crate::TerminalPaneIpc`])
    /// addresses panes by a bare id and treats collisions as ambiguous. A
    /// process-wide monotonic counter (like tmux's `%N`) guarantees that.
    fn allocate() -> Self {
        static NEXT_PANE_ID: AtomicU64 = AtomicU64::new(1);
        Self(NEXT_PANE_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// Direction used when splitting the active terminal pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalPaneSplit {
    /// Split into left/right panes, placing the new pane on the right.
    Vertical,
    /// Split into top/bottom panes, placing the new pane below.
    Horizontal,
}

/// Direction used when selecting a neighboring terminal pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalPaneSelectDirection {
    Left,
    Right,
    Up,
    Down,
}

/// Result of splitting one pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalPaneSplitOutcome {
    pub pane_id: TerminalPaneId,
    pub new_pane_id: TerminalPaneId,
    pub pane_count: usize,
}

/// Result of selecting a neighboring pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalPaneSelectOutcome {
    pub previous_pane_id: TerminalPaneId,
    pub pane_id: TerminalPaneId,
}

/// Result of detaching one pane from a group.
pub struct TerminalPaneBreakOutcome {
    pub pane_id: TerminalPaneId,
    pub terminal: TerminalEmulator,
    pub rect: Option<Rect>,
    pub remaining_pane_count: usize,
}

/// Observable state for one pane.
#[derive(Clone)]
pub struct TerminalPaneSnapshot {
    pub id: TerminalPaneId,
    pub index: usize,
    pub is_active: bool,
    pub rect: Option<Rect>,
    pub handle: TerminalHandle,
}

struct TerminalPaneGroupShared {
    panes: Vec<TerminalPane>,
    tree: Option<TerminalPaneNode>,
    active_id: Option<TerminalPaneId>,
    prefix_shortcut: TerminalShortcut,
    last_error: Option<String>,
    last_area: Option<Rect>,
    last_layouts: Vec<TerminalPaneLayout>,
    zoomed_pane_id: Option<TerminalPaneId>,
    pane_factory: Option<Arc<TerminalPaneFactory>>,
}

impl Default for TerminalPaneGroupShared {
    fn default() -> Self {
        Self {
            panes: Vec::new(),
            tree: None,
            active_id: None,
            prefix_shortcut: TerminalConfig::default()
                .prefix_shortcut()
                .expect("default terminal prefix shortcut must be valid"),
            last_error: None,
            last_area: None,
            last_layouts: Vec::new(),
            zoomed_pane_id: None,
            pane_factory: None,
        }
    }
}

/// Handle for inspecting a [`TerminalPaneGroup`] from the surrounding app shell.
#[derive(Clone, Default)]
pub struct TerminalPaneGroupHandle {
    shared: Arc<Mutex<TerminalPaneGroupShared>>,
}

impl TerminalPaneGroupHandle {
    /// Returns snapshots for all panes in stable pane order.
    pub fn panes(&self) -> Vec<TerminalPaneSnapshot> {
        self.shared.lock().snapshots()
    }

    /// Returns the number of panes currently owned by the group.
    pub fn pane_count(&self) -> usize {
        self.shared.lock().panes.len()
    }

    /// Returns the active pane id, if any.
    pub fn active_pane_id(&self) -> Option<TerminalPaneId> {
        self.shared.lock().active_pane_id()
    }

    /// Returns the active pane snapshot, if any.
    pub fn active_pane(&self) -> Option<TerminalPaneSnapshot> {
        self.shared
            .lock()
            .snapshots()
            .into_iter()
            .find(|pane| pane.is_active)
    }

    /// Returns the active terminal handle, if any.
    pub fn active_terminal_handle(&self) -> Option<TerminalHandle> {
        self.active_pane().map(|pane| pane.handle)
    }

    /// Returns the pane covering an absolute screen coordinate.
    pub fn pane_at_screen_position(&self, x: u16, y: u16) -> Option<TerminalPaneSnapshot> {
        self.shared
            .lock()
            .snapshots()
            .into_iter()
            .find(|pane| pane.rect.is_some_and(|rect| rect_contains(rect, x, y)))
    }

    /// Returns and clears the last split creation error.
    pub fn take_last_error(&self) -> Option<String> {
        self.shared.lock().last_error.take()
    }

    /// Applies terminal configuration to all current panes and pane-level prefix handling.
    pub fn apply_config(&self, config: &TerminalConfig) -> Result<()> {
        config.validate()?;
        let prefix_shortcut = config.prefix_shortcut()?;
        let panes = {
            let mut shared = self.shared.lock();
            shared.prefix_shortcut = prefix_shortcut;
            shared.snapshots()
        };
        for pane in panes {
            pane.handle.apply_config(config)?;
        }
        Ok(())
    }

    /// Splits the addressed pane, or the active pane when `pane_id` is `None`.
    pub fn split_window(
        &self,
        pane_id: Option<TerminalPaneId>,
        direction: TerminalPaneSplit,
    ) -> Result<TerminalPaneSplitOutcome> {
        let mut shared = self.shared.lock();
        match pane_id {
            Some(pane_id) => shared.split_pane(pane_id, direction),
            None => shared.split_active(direction),
        }
    }

    /// Selects the nearest pane in `direction` from the addressed or active pane.
    pub fn select_pane(
        &self,
        pane_id: Option<TerminalPaneId>,
        direction: TerminalPaneSelectDirection,
    ) -> Result<TerminalPaneSelectOutcome> {
        let mut shared = self.shared.lock();
        let source = pane_id
            .or_else(|| shared.active_pane_id())
            .ok_or_else(|| anyhow!("no active pane"))?;
        shared.select_pane(source, direction)
    }

    /// Removes a pane from this group and returns its terminal for hosting elsewhere.
    pub fn break_pane(&self, pane_id: TerminalPaneId) -> Result<TerminalPaneBreakOutcome> {
        self.shared.lock().break_pane(pane_id)
    }
}

struct TerminalPane {
    id: TerminalPaneId,
    terminal: TerminalEmulator,
    handle: TerminalHandle,
}

impl TerminalPane {
    fn new(id: TerminalPaneId, terminal: TerminalEmulator) -> Self {
        let handle = terminal.handle();
        Self {
            id,
            terminal,
            handle,
        }
    }
}

#[derive(Clone, Debug)]
enum TerminalPaneNode {
    Leaf(TerminalPaneId),
    Split {
        direction: TerminalPaneSplit,
        first_len: Option<u16>,
        first: Box<TerminalPaneNode>,
        second: Box<TerminalPaneNode>,
    },
}

impl TerminalPaneNode {
    fn contains_leaf(&self, target: TerminalPaneId) -> bool {
        match self {
            Self::Leaf(id) => *id == target,
            Self::Split { first, second, .. } => {
                first.contains_leaf(target) || second.contains_leaf(target)
            }
        }
    }

    fn split_leaf(
        &mut self,
        target: TerminalPaneId,
        new_id: TerminalPaneId,
        direction: TerminalPaneSplit,
    ) -> bool {
        match self {
            Self::Leaf(id) if *id == target => {
                *self = Self::Split {
                    direction,
                    first_len: None,
                    first: Box::new(Self::Leaf(target)),
                    second: Box::new(Self::Leaf(new_id)),
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                first.split_leaf(target, new_id, direction)
                    || second.split_leaf(target, new_id, direction)
            }
        }
    }

    fn resize_leaf(
        &mut self,
        target: TerminalPaneId,
        direction: TerminalPaneSelectDirection,
        amount: u16,
        area: Rect,
    ) -> bool {
        match self {
            Self::Leaf(_) => false,
            Self::Split {
                direction: split_direction,
                first_len,
                first,
                second,
            } => {
                let target_in_first = first.contains_leaf(target);
                let target_in_second = second.contains_leaf(target);
                if !target_in_first && !target_in_second {
                    return false;
                }

                let (first_rect, _, second_rect) = split_rect(area, *split_direction, *first_len);
                let child_resized = if target_in_first {
                    first.resize_leaf(target, direction, amount, first_rect)
                } else {
                    second.resize_leaf(target, direction, amount, second_rect)
                };
                if child_resized {
                    return true;
                }

                let delta = match (
                    *split_direction,
                    direction,
                    target_in_first,
                    target_in_second,
                ) {
                    (
                        TerminalPaneSplit::Vertical,
                        TerminalPaneSelectDirection::Right,
                        true,
                        false,
                    )
                    | (
                        TerminalPaneSplit::Horizontal,
                        TerminalPaneSelectDirection::Down,
                        true,
                        false,
                    ) => amount as i16,
                    (
                        TerminalPaneSplit::Vertical,
                        TerminalPaneSelectDirection::Left,
                        false,
                        true,
                    )
                    | (
                        TerminalPaneSplit::Horizontal,
                        TerminalPaneSelectDirection::Up,
                        false,
                        true,
                    ) => -(amount as i16),
                    _ => return false,
                };

                let axis_len = match split_direction {
                    TerminalPaneSplit::Vertical => area.width,
                    TerminalPaneSplit::Horizontal => area.height,
                };
                let current = split_first_len(axis_len, *first_len);
                let next = adjust_split_first_len(axis_len, current, delta);
                if next == current {
                    return false;
                }
                *first_len = Some(next);
                true
            }
        }
    }

    fn without_leaf(self, target: TerminalPaneId) -> Option<Self> {
        match self {
            Self::Leaf(id) if id == target => None,
            Self::Leaf(id) => Some(Self::Leaf(id)),
            Self::Split {
                direction,
                first_len,
                first,
                second,
            } => match (first.without_leaf(target), second.without_leaf(target)) {
                (Some(first), Some(second)) => Some(Self::Split {
                    direction,
                    first_len,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (Some(only), None) | (None, Some(only)) => Some(only),
                (None, None) => None,
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TerminalPaneLayout {
    id: TerminalPaneId,
    rect: Rect,
}

type TerminalPaneFactory = dyn Fn(usize) -> Result<TerminalEmulator> + Send + Sync;

impl TerminalPaneGroupShared {
    fn new(initial: TerminalPane) -> Self {
        let first_id = initial.id;
        Self {
            panes: vec![initial],
            tree: Some(TerminalPaneNode::Leaf(first_id)),
            active_id: Some(first_id),
            ..Self::default()
        }
    }

    fn pane_index(&self, id: TerminalPaneId) -> Option<usize> {
        self.panes.iter().position(|pane| pane.id == id)
    }

    fn active_pane_index(&self) -> Option<usize> {
        self.panes
            .iter()
            .position(|pane| Some(pane.id) == self.active_id)
    }

    fn active_pane_id(&self) -> Option<TerminalPaneId> {
        self.active_id
    }

    fn snapshots(&self) -> Vec<TerminalPaneSnapshot> {
        self.panes
            .iter()
            .enumerate()
            .map(|(index, pane)| TerminalPaneSnapshot {
                id: pane.id,
                index,
                is_active: Some(pane.id) == self.active_id,
                rect: self
                    .last_layouts
                    .iter()
                    .find(|layout| layout.id == pane.id)
                    .map(|layout| layout.rect),
                handle: pane.handle.clone(),
            })
            .collect()
    }

    fn focus_pane(&mut self, id: TerminalPaneId, capture: bool) -> bool {
        if self.pane_index(id).is_none() {
            return false;
        }
        self.active_id = Some(id);
        if self.zoomed_pane_id.is_some() {
            self.zoomed_pane_id = Some(id);
        }
        for pane in &self.panes {
            pane.handle.set_capture(capture && pane.id == id);
        }
        true
    }

    fn focus_next_pane(&mut self) -> bool {
        let Some(active_idx) = self.active_pane_index() else {
            return false;
        };
        let next_idx = (active_idx + 1) % self.panes.len().max(1);
        let next_id = self.panes[next_idx].id;
        self.focus_pane(next_id, true)
    }

    fn refresh_layouts_from_last_area(&mut self) {
        let Some(area) = self.last_area else {
            return;
        };
        let (layouts, _) = self.visible_layouts_for_area(area);
        self.last_layouts = layouts;
    }

    fn full_layouts_for_area(
        &self,
        area: Rect,
    ) -> (Vec<TerminalPaneLayout>, Vec<(Rect, TerminalPaneSplit)>) {
        let Some(tree) = &self.tree else {
            return (Vec::new(), Vec::new());
        };
        let mut layouts = Vec::new();
        let mut dividers = Vec::new();
        layout_pane_node(tree, area, &mut layouts, &mut dividers);
        (layouts, dividers)
    }

    fn visible_layouts_for_area(
        &self,
        area: Rect,
    ) -> (Vec<TerminalPaneLayout>, Vec<(Rect, TerminalPaneSplit)>) {
        let (layouts, dividers) = self.full_layouts_for_area(area);
        if let Some(id) = self.zoomed_pane_id
            && self.pane_index(id).is_some()
        {
            return (vec![TerminalPaneLayout { id, rect: area }], Vec::new());
        }
        (layouts, dividers)
    }

    fn create_terminal_for_new_pane(&self, pane_number: usize) -> Result<TerminalEmulator> {
        if let Some(factory) = &self.pane_factory {
            factory(pane_number)
        } else {
            Ok(TerminalEmulator::new())
        }
    }

    fn split_pane(
        &mut self,
        pane_id: TerminalPaneId,
        direction: TerminalPaneSplit,
    ) -> Result<TerminalPaneSplitOutcome> {
        if self.pane_index(pane_id).is_none() {
            bail!("pane {} does not exist", pane_id.raw());
        }
        let new_id = TerminalPaneId::allocate();
        let pane_number = self.panes.len().saturating_add(1);
        let terminal = match self.create_terminal_for_new_pane(pane_number) {
            Ok(terminal) => terminal,
            Err(error) => {
                self.last_error = Some(error.to_string());
                return Err(error);
            }
        };
        let Some(tree) = self.tree.as_mut() else {
            bail!("pane tree is empty");
        };
        if !tree.split_leaf(pane_id, new_id, direction) {
            bail!("pane {} is not present in the pane tree", pane_id.raw());
        }
        self.panes.push(TerminalPane::new(new_id, terminal));
        self.focus_pane(new_id, true);
        self.refresh_layouts_from_last_area();
        Ok(TerminalPaneSplitOutcome {
            pane_id,
            new_pane_id: new_id,
            pane_count: self.panes.len(),
        })
    }

    fn split_active(&mut self, direction: TerminalPaneSplit) -> Result<TerminalPaneSplitOutcome> {
        let Some(active_id) = self.active_id else {
            bail!("no active pane");
        };
        self.split_pane(active_id, direction)
    }

    fn select_pane(
        &mut self,
        source: TerminalPaneId,
        direction: TerminalPaneSelectDirection,
    ) -> Result<TerminalPaneSelectOutcome> {
        let target = self
            .neighbor_pane(source, direction)
            .ok_or_else(|| anyhow!("no pane {:?} of pane {}", direction, source.raw()))?;
        self.focus_pane(target, true);
        Ok(TerminalPaneSelectOutcome {
            previous_pane_id: source,
            pane_id: target,
        })
    }

    fn resize_active_pane(&mut self, direction: TerminalPaneSelectDirection, amount: u16) -> bool {
        let Some(active_id) = self.active_id else {
            return false;
        };
        let Some(area) = self.last_area else {
            return false;
        };
        let Some(tree) = self.tree.as_mut() else {
            return false;
        };
        if !tree.resize_leaf(active_id, direction, amount, area) {
            return false;
        }
        self.refresh_layouts_from_last_area();
        true
    }

    fn toggle_zoom_active_pane(&mut self) -> bool {
        let Some(active_id) = self.active_id else {
            return false;
        };
        self.zoomed_pane_id = if self.zoomed_pane_id == Some(active_id) {
            None
        } else {
            Some(active_id)
        };
        self.refresh_layouts_from_last_area();
        true
    }

    fn close_active_pane(&mut self) -> bool {
        let Some(active_id) = self.active_id else {
            return false;
        };
        self.close_pane(active_id).is_ok()
    }

    fn neighbor_pane(
        &self,
        source: TerminalPaneId,
        direction: TerminalPaneSelectDirection,
    ) -> Option<TerminalPaneId> {
        let layouts = if let Some(area) = self.last_area {
            self.full_layouts_for_area(area).0
        } else {
            self.last_layouts.clone()
        };
        let active = layouts.iter().find(|layout| layout.id == source)?;
        let active_center = rect_center(active.rect);
        layouts
            .iter()
            .filter(|layout| layout.id != source)
            .filter(|layout| pane_is_in_direction(active.rect, layout.rect, direction))
            .min_by_key(|layout| pane_direction_score(active_center, layout.rect, direction))
            .map(|layout| layout.id)
    }

    /// Returns the surviving pane whose layout center is closest to `source`'s,
    /// used to transfer focus spatially when a pane is closed.
    fn nearest_pane_to(&self, source: TerminalPaneId) -> Option<TerminalPaneId> {
        let layouts = if let Some(area) = self.last_area {
            self.full_layouts_for_area(area).0
        } else {
            self.last_layouts.clone()
        };
        let source_center = rect_center(layouts.iter().find(|l| l.id == source)?.rect);
        layouts
            .iter()
            .filter(|layout| layout.id != source)
            .min_by_key(|layout| {
                let c = rect_center(layout.rect);
                let dx = c.0.abs_diff(source_center.0);
                let dy = c.1.abs_diff(source_center.1);
                dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy))
            })
            .map(|layout| layout.id)
    }

    fn break_pane(&mut self, pane_id: TerminalPaneId) -> Result<TerminalPaneBreakOutcome> {
        let Some((pane, rect)) = self.remove_pane(pane_id)? else {
            bail!("cannot break the last pane in a group");
        };
        Ok(TerminalPaneBreakOutcome {
            pane_id,
            terminal: pane.terminal,
            rect,
            remaining_pane_count: self.panes.len(),
        })
    }

    fn close_pane(&mut self, pane_id: TerminalPaneId) -> Result<()> {
        let Some((_pane, _rect)) = self.remove_pane(pane_id)? else {
            bail!("cannot close the last pane in a group");
        };
        Ok(())
    }

    fn remove_pane(
        &mut self,
        pane_id: TerminalPaneId,
    ) -> Result<Option<(TerminalPane, Option<Rect>)>> {
        if self.panes.len() <= 1 {
            return Ok(None);
        }
        let Some(index) = self.pane_index(pane_id) else {
            bail!("pane {} does not exist", pane_id.raw());
        };
        let Some(tree) = self.tree.take() else {
            bail!("pane tree is empty");
        };
        let Some(next_tree) = tree.without_leaf(pane_id) else {
            bail!("cannot break the last pane in a group");
        };
        self.tree = Some(next_tree);

        let removing_active = self.active_id == Some(pane_id);
        // Preserve the outgoing active pane's capture state so closing a pane
        // doesn't silently flip the survivor back into capture mode.
        let preserved_capture = removing_active
            .then(|| self.panes[index].handle.capture())
            .unwrap_or(true);
        // Pick the spatially-nearest surviving pane to inherit focus, rather
        // than always jumping to the first-created pane.
        let successor = removing_active
            .then(|| self.nearest_pane_to(pane_id))
            .flatten();

        let pane = self.panes.remove(index);
        let rect = self
            .last_layouts
            .iter()
            .find(|layout| layout.id == pane_id)
            .map(|layout| layout.rect);
        self.last_layouts.retain(|layout| layout.id != pane_id);
        if self.zoomed_pane_id == Some(pane_id) {
            self.zoomed_pane_id = None;
        }
        if removing_active {
            // Fall back to the first remaining pane only if no spatial neighbor
            // was found (e.g. layouts not yet computed).
            let next = successor.or_else(|| self.panes.first().map(|pane| pane.id));
            self.active_id = next;
            if let Some(active_id) = next {
                self.focus_pane(active_id, preserved_capture);
            }
        }
        self.refresh_layouts_from_last_area();
        Ok(Some((pane, rect)))
    }
}

/// A tmux-like split-pane container for terminal emulators.
pub struct TerminalPaneGroup {
    prefix_shortcut: TerminalShortcut,
    prefix_pending: bool,
    shared: Arc<Mutex<TerminalPaneGroupShared>>,
}

impl TerminalPaneGroup {
    /// Creates a pane group with one initial terminal pane.
    pub fn new(initial: TerminalEmulator) -> Self {
        let first_id = TerminalPaneId::allocate();
        let pane = TerminalPane::new(first_id, initial);
        let shared = Arc::new(Mutex::new(TerminalPaneGroupShared::new(pane)));
        let prefix_shortcut = shared.lock().prefix_shortcut;
        Self {
            prefix_shortcut,
            prefix_pending: false,
            shared,
        }
    }

    /// Returns a handle for observing panes from the app shell.
    pub fn handle(&self) -> TerminalPaneGroupHandle {
        TerminalPaneGroupHandle {
            shared: Arc::clone(&self.shared),
        }
    }

    /// Grants mutable access to the active pane's terminal, returning the
    /// closure's result.
    ///
    /// Pane ids are allocated when the group is created, so callers that need
    /// the terminal's `$TMUX_PANE` (or any other id-dependent setup) to match
    /// the pane id must create the group first and then configure/spawn the
    /// initial terminal through this method. Returns `None` when the group has
    /// no active pane.
    pub fn with_active_terminal_mut<R>(
        &mut self,
        f: impl FnOnce(&mut TerminalEmulator) -> R,
    ) -> Option<R> {
        let mut shared = self.shared.lock();
        let index = shared.active_pane_index()?;
        Some(f(&mut shared.panes[index].terminal))
    }

    /// Configures the shortcut used for pane-level prefix commands.
    pub fn prefix_shortcut(mut self, shortcut: TerminalShortcut) -> Self {
        self.prefix_shortcut = shortcut;
        self.prefix_pending = false;
        self.shared.lock().prefix_shortcut = shortcut;
        self
    }

    /// Applies terminal configuration to existing panes and future pane-level prefix handling.
    pub fn config(self, config: &TerminalConfig) -> Result<Self> {
        self.handle().apply_config(config)?;
        Ok(self)
    }

    /// Configures how new panes are created when a split command is invoked.
    pub fn pane_factory<F>(self, factory: F) -> Self
    where
        F: Fn(usize) -> Result<TerminalEmulator> + Send + Sync + 'static,
    {
        self.shared.lock().pane_factory = Some(Arc::new(factory));
        self
    }

    fn sync_prefix_from_shared(&mut self) {
        let prefix_shortcut = self.shared.lock().prefix_shortcut;
        if self.prefix_shortcut != prefix_shortcut {
            self.prefix_shortcut = prefix_shortcut;
            self.prefix_pending = false;
        }
    }

    fn focus_pane(&mut self, id: TerminalPaneId, capture: bool) -> bool {
        self.shared.lock().focus_pane(id, capture)
    }

    fn focus_next_pane(&mut self) -> bool {
        self.shared.lock().focus_next_pane()
    }

    fn split_active(&mut self, direction: TerminalPaneSplit) -> bool {
        self.shared.lock().split_active(direction).is_ok()
    }

    fn select_active_neighbor(&mut self, direction: TerminalPaneSelectDirection) -> bool {
        let mut shared = self.shared.lock();
        let Some(active_id) = shared.active_pane_id() else {
            return false;
        };
        shared.select_pane(active_id, direction).is_ok()
    }

    fn resize_active_pane(&mut self, direction: TerminalPaneSelectDirection) -> bool {
        self.shared
            .lock()
            .resize_active_pane(direction, PANE_RESIZE_STEP)
    }

    fn toggle_zoom_active_pane(&mut self) -> bool {
        self.shared.lock().toggle_zoom_active_pane()
    }

    fn close_active_pane(&mut self) -> bool {
        self.shared.lock().close_active_pane()
    }

    fn pane_at_screen_position(&self, x: u16, y: u16) -> Option<(usize, Rect)> {
        let shared = self.shared.lock();
        let layout = shared
            .last_layouts
            .iter()
            .find(|layout| rect_contains(layout.rect, x, y))?;
        let index = shared.pane_index(layout.id)?;
        Some((index, layout.rect))
    }

    fn handle_pane_prefix_key(
        &mut self,
        key: KeyEvent,
        ctx: ComponentContext<'_>,
    ) -> Option<EventResult> {
        self.sync_prefix_from_shared();
        if !ctx.is_focused || key.kind == KeyEventKind::Release {
            return None;
        }
        if self.prefix_pending {
            self.prefix_pending = false;
            return Some(self.handle_prefixed_key(key, ctx));
        }
        let capture = {
            let shared = self.shared.lock();
            let active = shared.active_pane_index()?;
            shared.panes[active].handle.capture()
        };
        if !capture || !shortcut_matches(self.prefix_shortcut, key) {
            return None;
        }
        self.prefix_pending = true;
        Some(EventResult::consumed())
    }

    fn handle_prefixed_key(&mut self, key: KeyEvent, ctx: ComponentContext<'_>) -> EventResult {
        match pane_command_for_key(key) {
            Some(PaneCommand::SplitVertical) => {
                let _ = self.split_active(TerminalPaneSplit::Vertical);
                EventResult::consumed()
            }
            Some(PaneCommand::SplitHorizontal) => {
                let _ = self.split_active(TerminalPaneSplit::Horizontal);
                EventResult::consumed()
            }
            Some(PaneCommand::FocusNext) => {
                let _ = self.focus_next_pane();
                EventResult::consumed()
            }
            Some(PaneCommand::Select(direction)) => {
                let _ = self.select_active_neighbor(direction);
                EventResult::consumed()
            }
            Some(PaneCommand::Resize(direction)) => {
                let _ = self.resize_active_pane(direction);
                EventResult::consumed()
            }
            Some(PaneCommand::ToggleZoom) => {
                let _ = self.toggle_zoom_active_pane();
                EventResult::consumed()
            }
            Some(PaneCommand::Close) => {
                let _ = self.close_active_pane();
                EventResult::consumed()
            }
            None => self.replay_prefix_to_active_pane(key, ctx),
        }
    }

    fn replay_prefix_to_active_pane(
        &mut self,
        key: KeyEvent,
        ctx: ComponentContext<'_>,
    ) -> EventResult {
        let mut shared = self.shared.lock();
        let Some(active_idx) = shared.active_pane_index() else {
            return EventResult::ignored();
        };
        let prefix_event = Event::Key(KeyEvent::new(
            self.prefix_shortcut.code,
            self.prefix_shortcut.modifiers,
        ));
        let child_ctx = child_context(ctx, true);
        let prefix_result = shared.panes[active_idx]
            .terminal
            .handle_event(&prefix_event, child_ctx);
        let key_result = shared.panes[active_idx]
            .terminal
            .handle_event(&Event::Key(key), child_ctx);
        if key_result.is_consumed() {
            key_result
        } else if prefix_result.is_consumed() {
            prefix_result
        } else {
            key_result
        }
    }
}

impl Component for TerminalPaneGroup {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let mut shared = self.shared.lock();
        shared.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            shared.last_layouts.clear();
            return;
        }

        let (layouts, dividers) = shared.visible_layouts_for_area(area);
        shared.last_layouts = layouts;

        for layout in shared.last_layouts.clone() {
            if layout.rect.width == 0 || layout.rect.height == 0 {
                continue;
            }
            let Some(index) = shared.pane_index(layout.id) else {
                continue;
            };
            let focused = ctx.is_focused && Some(layout.id) == shared.active_id;
            let pane_ctx = ComponentContext {
                is_focused: focused,
                scrollbar_host: ScrollbarHost::Window,
                tab_mode: ctx.tab_mode.for_child(),
                mouse_coordinate_space: ctx.mouse_coordinate_space,
                drag: None,
                ..ctx
            };
            shared.panes[index]
                .terminal
                .draw(frame, layout.rect, pane_ctx);
        }

        draw_dividers(frame, &dividers, ctx);
    }
}

impl Layout for TerminalPaneGroup {
    fn min_width(&self) -> u16 {
        1
    }

    fn min_height(&self) -> u16 {
        1
    }
}

impl Scrollable for TerminalPaneGroup {}

impl FocusNav for TerminalPaneGroup {
    fn is_focusable(&self) -> bool {
        true
    }
}

impl DynamicTree for TerminalPaneGroup {}

impl EventHandling for TerminalPaneGroup {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if let Event::Key(key) = event
            && let Some(result) = self.handle_pane_prefix_key(*key, ctx)
        {
            return result;
        }

        match event {
            Event::Mouse(mouse) => self.handle_mouse_event(*mouse, ctx),
            _ => self.handle_active_pane_event(event, ctx),
        }
    }
}

impl atto_ui::composable::DragAndDrop for TerminalPaneGroup {}

impl TerminalPaneGroup {
    fn handle_active_pane_event(
        &mut self,
        event: &Event,
        ctx: ComponentContext<'_>,
    ) -> EventResult {
        let mut shared = self.shared.lock();
        let Some(active_idx) = shared.active_pane_index() else {
            return EventResult::ignored();
        };
        let child_ctx = child_context(ctx, ctx.is_focused);
        shared.panes[active_idx]
            .terminal
            .handle_event(event, child_ctx)
    }

    fn handle_mouse_event(&mut self, mouse: MouseEvent, ctx: ComponentContext<'_>) -> EventResult {
        let Some(area) = self.shared.lock().last_area else {
            return EventResult::ignored();
        };
        let Some((screen_x, screen_y)) =
            mouse_screen_position(area, mouse, ctx.mouse_coordinate_space)
        else {
            return EventResult::ignored();
        };
        let Some((pane_idx, rect)) = self.pane_at_screen_position(screen_x, screen_y) else {
            return EventResult::ignored();
        };

        if matches!(mouse.kind, crossterm::event::MouseEventKind::Down(_)) {
            let Some((id, capture)) = ({
                let shared = self.shared.lock();
                shared
                    .panes
                    .get(pane_idx)
                    .map(|pane| (pane.id, pane.handle.capture()))
            }) else {
                return EventResult::ignored();
            };
            let _ = self.focus_pane(id, capture);
        }

        let child_event = Event::Mouse(MouseEvent {
            column: screen_x.saturating_sub(rect.x),
            row: screen_y.saturating_sub(rect.y),
            ..mouse
        });
        let mut shared = self.shared.lock();
        let active_id = shared.active_id;
        let Some(pane) = shared.panes.get_mut(pane_idx) else {
            return EventResult::ignored();
        };
        let child_focused = ctx.is_focused && Some(pane.id) == active_id;
        let child_ctx = ComponentContext {
            mouse_coordinate_space: MouseCoordinateSpace::Local,
            is_focused: child_focused,
            scrollbar_host: ScrollbarHost::Window,
            tab_mode: ctx.tab_mode.for_child(),
            drag: None,
            ..ctx
        };
        pane.terminal.handle_event(&child_event, child_ctx)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneCommand {
    SplitVertical,
    SplitHorizontal,
    FocusNext,
    Select(TerminalPaneSelectDirection),
    Resize(TerminalPaneSelectDirection),
    ToggleZoom,
    Close,
}

fn pane_command_for_key(key: KeyEvent) -> Option<PaneCommand> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    match key.code {
        KeyCode::Char('%') if key.modifiers == KeyModifiers::NONE => {
            Some(PaneCommand::SplitVertical)
        }
        KeyCode::Char('"') if key.modifiers == KeyModifiers::NONE => {
            Some(PaneCommand::SplitHorizontal)
        }
        KeyCode::Char(ch)
            if key.modifiers == KeyModifiers::NONE && ch.eq_ignore_ascii_case(&'o') =>
        {
            Some(PaneCommand::FocusNext)
        }
        KeyCode::Tab if key.modifiers == KeyModifiers::NONE => Some(PaneCommand::FocusNext),
        KeyCode::Left if key.modifiers == KeyModifiers::NONE => {
            Some(PaneCommand::Select(TerminalPaneSelectDirection::Left))
        }
        KeyCode::Right if key.modifiers == KeyModifiers::NONE => {
            Some(PaneCommand::Select(TerminalPaneSelectDirection::Right))
        }
        KeyCode::Up if key.modifiers == KeyModifiers::NONE => {
            Some(PaneCommand::Select(TerminalPaneSelectDirection::Up))
        }
        KeyCode::Down if key.modifiers == KeyModifiers::NONE => {
            Some(PaneCommand::Select(TerminalPaneSelectDirection::Down))
        }
        KeyCode::Left if key.modifiers == KeyModifiers::CONTROL => {
            Some(PaneCommand::Resize(TerminalPaneSelectDirection::Left))
        }
        KeyCode::Right if key.modifiers == KeyModifiers::CONTROL => {
            Some(PaneCommand::Resize(TerminalPaneSelectDirection::Right))
        }
        KeyCode::Up if key.modifiers == KeyModifiers::CONTROL => {
            Some(PaneCommand::Resize(TerminalPaneSelectDirection::Up))
        }
        KeyCode::Down if key.modifiers == KeyModifiers::CONTROL => {
            Some(PaneCommand::Resize(TerminalPaneSelectDirection::Down))
        }
        KeyCode::Char(ch)
            if key.modifiers == KeyModifiers::NONE && ch.eq_ignore_ascii_case(&'z') =>
        {
            Some(PaneCommand::ToggleZoom)
        }
        KeyCode::Char(ch)
            if key.modifiers == KeyModifiers::NONE && ch.eq_ignore_ascii_case(&'x') =>
        {
            Some(PaneCommand::Close)
        }
        _ => None,
    }
}

fn child_context(ctx: ComponentContext<'_>, is_focused: bool) -> ComponentContext<'_> {
    ComponentContext {
        is_focused,
        scrollbar_host: ScrollbarHost::Window,
        tab_mode: TabMode::Bubble,
        drag: None,
        ..ctx
    }
}

fn shortcut_matches(shortcut: TerminalShortcut, key: KeyEvent) -> bool {
    if key.kind == KeyEventKind::Release || key.modifiers != shortcut.modifiers {
        return false;
    }
    if key.code == shortcut.code {
        return true;
    }
    matches!(
        (key.code, shortcut.code),
        (KeyCode::Char(a), KeyCode::Char(b)) if a.eq_ignore_ascii_case(&b)
    )
}

fn layout_pane_node(
    node: &TerminalPaneNode,
    area: Rect,
    panes: &mut Vec<TerminalPaneLayout>,
    dividers: &mut Vec<(Rect, TerminalPaneSplit)>,
) {
    match node {
        TerminalPaneNode::Leaf(id) => panes.push(TerminalPaneLayout {
            id: *id,
            rect: area,
        }),
        TerminalPaneNode::Split {
            direction,
            first_len,
            first,
            second,
        } => {
            let (first_rect, divider, second_rect) = split_rect(area, *direction, *first_len);
            layout_pane_node(first, first_rect, panes, dividers);
            if divider.width > 0 && divider.height > 0 {
                dividers.push((divider, *direction));
            }
            layout_pane_node(second, second_rect, panes, dividers);
        }
    }
}

fn split_rect(
    area: Rect,
    direction: TerminalPaneSplit,
    first_len: Option<u16>,
) -> (Rect, Rect, Rect) {
    match direction {
        TerminalPaneSplit::Vertical => split_rect_vertical(area, first_len),
        TerminalPaneSplit::Horizontal => split_rect_horizontal(area, first_len),
    }
}

fn split_first_len(axis_len: u16, requested: Option<u16>) -> u16 {
    let available = axis_len.saturating_sub(DIVIDER_THICKNESS);
    if available <= 1 {
        return available;
    }
    requested
        .unwrap_or(available / 2)
        .clamp(1, available.saturating_sub(1))
}

fn adjust_split_first_len(axis_len: u16, current: u16, delta: i16) -> u16 {
    let available = axis_len.saturating_sub(DIVIDER_THICKNESS);
    if available <= 1 {
        return available;
    }
    let min = 1i32;
    let max = available.saturating_sub(1) as i32;
    (current as i32 + delta as i32).clamp(min, max) as u16
}

fn split_rect_vertical(area: Rect, first_len: Option<u16>) -> (Rect, Rect, Rect) {
    if area.width <= DIVIDER_THICKNESS {
        return (area, Rect::default(), Rect::default());
    }
    let available = area.width.saturating_sub(DIVIDER_THICKNESS);
    let first_width = split_first_len(area.width, first_len);
    let second_width = available.saturating_sub(first_width);
    let first = Rect {
        width: first_width,
        ..area
    };
    let divider = Rect {
        x: area.x.saturating_add(first_width),
        y: area.y,
        width: DIVIDER_THICKNESS,
        height: area.height,
    };
    let second = Rect {
        x: divider.x.saturating_add(DIVIDER_THICKNESS),
        y: area.y,
        width: second_width,
        height: area.height,
    };
    (first, divider, second)
}

fn split_rect_horizontal(area: Rect, first_len: Option<u16>) -> (Rect, Rect, Rect) {
    if area.height <= DIVIDER_THICKNESS {
        return (area, Rect::default(), Rect::default());
    }
    let available = area.height.saturating_sub(DIVIDER_THICKNESS);
    let first_height = split_first_len(area.height, first_len);
    let second_height = available.saturating_sub(first_height);
    let first = Rect {
        height: first_height,
        ..area
    };
    let divider = Rect {
        x: area.x,
        y: area.y.saturating_add(first_height),
        width: area.width,
        height: DIVIDER_THICKNESS,
    };
    let second = Rect {
        x: area.x,
        y: divider.y.saturating_add(DIVIDER_THICKNESS),
        width: area.width,
        height: second_height,
    };
    (first, divider, second)
}

fn draw_dividers(
    frame: &mut Frame<'_>,
    dividers: &[(Rect, TerminalPaneSplit)],
    ctx: ComponentContext<'_>,
) {
    let buf = frame.buffer_mut();
    let style = ctx.theme.widget.dim;
    let border = ctx.theme.border_set(false);
    for (rect, direction) in dividers {
        let symbol = match direction {
            TerminalPaneSplit::Vertical => border.vertical_left,
            TerminalPaneSplit::Horizontal => border.horizontal_top,
        };
        for dy in 0..rect.height {
            for dx in 0..rect.width {
                let x = rect.x.saturating_add(dx);
                let y = rect.y.saturating_add(dy);
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(symbol);
                    cell.set_style(style);
                }
            }
        }
    }
}

fn mouse_screen_position(
    area: Rect,
    mouse: MouseEvent,
    coordinate_space: MouseCoordinateSpace,
) -> Option<(u16, u16)> {
    match coordinate_space {
        MouseCoordinateSpace::Absolute => {
            rect_contains(area, mouse.column, mouse.row).then_some((mouse.column, mouse.row))
        }
        MouseCoordinateSpace::Local => {
            if mouse.column < area.width && mouse.row < area.height {
                Some((
                    area.x.saturating_add(mouse.column),
                    area.y.saturating_add(mouse.row),
                ))
            } else {
                None
            }
        }
    }
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && y >= rect.y
        && x < rect.x.saturating_add(rect.width)
        && y < rect.y.saturating_add(rect.height)
}

fn rect_center(rect: Rect) -> (u32, u32) {
    (
        rect.x as u32 * 2 + rect.width as u32,
        rect.y as u32 * 2 + rect.height as u32,
    )
}

fn pane_is_in_direction(
    active: Rect,
    candidate: Rect,
    direction: TerminalPaneSelectDirection,
) -> bool {
    let (active_x, active_y) = rect_center(active);
    let (candidate_x, candidate_y) = rect_center(candidate);
    match direction {
        TerminalPaneSelectDirection::Left => {
            candidate_x < active_x
                && ranges_overlap(active.y, active.height, candidate.y, candidate.height)
        }
        TerminalPaneSelectDirection::Right => {
            candidate_x > active_x
                && ranges_overlap(active.y, active.height, candidate.y, candidate.height)
        }
        TerminalPaneSelectDirection::Up => {
            candidate_y < active_y
                && ranges_overlap(active.x, active.width, candidate.x, candidate.width)
        }
        TerminalPaneSelectDirection::Down => {
            candidate_y > active_y
                && ranges_overlap(active.x, active.width, candidate.x, candidate.width)
        }
    }
}

fn pane_direction_score(
    active_center: (u32, u32),
    candidate: Rect,
    direction: TerminalPaneSelectDirection,
) -> (u32, u32) {
    let candidate_center = rect_center(candidate);
    let primary = match direction {
        TerminalPaneSelectDirection::Left | TerminalPaneSelectDirection::Right => {
            active_center.0.abs_diff(candidate_center.0)
        }
        TerminalPaneSelectDirection::Up | TerminalPaneSelectDirection::Down => {
            active_center.1.abs_diff(candidate_center.1)
        }
    };
    let secondary = match direction {
        TerminalPaneSelectDirection::Left | TerminalPaneSelectDirection::Right => {
            active_center.1.abs_diff(candidate_center.1)
        }
        TerminalPaneSelectDirection::Up | TerminalPaneSelectDirection::Down => {
            active_center.0.abs_diff(candidate_center.0)
        }
    };
    (primary, secondary)
}

fn ranges_overlap(a_start: u16, a_len: u16, b_start: u16, b_len: u16) -> bool {
    let a_end = a_start.saturating_add(a_len);
    let b_end = b_start.saturating_add(b_len);
    a_start < b_end && b_start < a_end
}

#[cfg(test)]
mod tests {
    use super::*;
    use atto_ui::theme::Theme;
    use atto_ui::wm::WindowId;

    fn context(theme: &Theme) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        }
    }

    #[test]
    fn split_rects_reserve_one_cell_divider() {
        let area = Rect::new(2, 3, 11, 7);
        let (left, divider, right) = split_rect_vertical(area, None);
        assert_eq!(left, Rect::new(2, 3, 5, 7));
        assert_eq!(divider, Rect::new(7, 3, 1, 7));
        assert_eq!(right, Rect::new(8, 3, 5, 7));

        let (top, divider, bottom) = split_rect_horizontal(area, None);
        assert_eq!(top, Rect::new(2, 3, 11, 3));
        assert_eq!(divider, Rect::new(2, 6, 11, 1));
        assert_eq!(bottom, Rect::new(2, 7, 11, 3));

        let (left, divider, right) = split_rect_vertical(area, Some(3));
        assert_eq!(left, Rect::new(2, 3, 3, 7));
        assert_eq!(divider, Rect::new(5, 3, 1, 7));
        assert_eq!(right, Rect::new(6, 3, 7, 7));
    }

    #[test]
    fn pane_prefix_commands_match_tmux_defaults() {
        assert_eq!(
            pane_command_for_key(KeyEvent::new(KeyCode::Char('%'), KeyModifiers::NONE)),
            Some(PaneCommand::SplitVertical)
        );
        assert_eq!(
            pane_command_for_key(KeyEvent::new(KeyCode::Char('"'), KeyModifiers::NONE)),
            Some(PaneCommand::SplitHorizontal)
        );
        assert_eq!(
            pane_command_for_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)),
            Some(PaneCommand::FocusNext)
        );
        assert_eq!(
            pane_command_for_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            Some(PaneCommand::FocusNext)
        );
        assert_eq!(
            pane_command_for_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            Some(PaneCommand::Select(TerminalPaneSelectDirection::Left))
        );
        assert_eq!(
            pane_command_for_key(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL)),
            Some(PaneCommand::Resize(TerminalPaneSelectDirection::Right))
        );
        assert_eq!(
            pane_command_for_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE)),
            Some(PaneCommand::ToggleZoom)
        );
        assert_eq!(
            pane_command_for_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
            Some(PaneCommand::Close)
        );
    }

    #[test]
    fn pane_resize_adjusts_nearest_split_for_active_pane() {
        let group = TerminalPaneGroup::new(TerminalEmulator::new());
        let handle = group.handle();
        {
            let mut shared = group.shared.lock();
            shared.last_area = Some(Rect::new(0, 0, 21, 8));
            shared.refresh_layouts_from_last_area();
        }

        let left_id = handle.panes()[0].id;
        let split = handle
            .split_window(None, TerminalPaneSplit::Vertical)
            .expect("split right");
        let right_id = split.new_pane_id;
        let before = handle.panes();
        let left_before = before
            .iter()
            .find(|pane| pane.id == left_id)
            .and_then(|pane| pane.rect)
            .expect("left rect before resize");
        let right_before = before
            .iter()
            .find(|pane| pane.id == right_id)
            .and_then(|pane| pane.rect)
            .expect("right rect before resize");

        assert!(
            group
                .shared
                .lock()
                .resize_active_pane(TerminalPaneSelectDirection::Left, 1)
        );

        let after = handle.panes();
        let left_after = after
            .iter()
            .find(|pane| pane.id == left_id)
            .and_then(|pane| pane.rect)
            .expect("left rect after resize");
        let right_after = after
            .iter()
            .find(|pane| pane.id == right_id)
            .and_then(|pane| pane.rect)
            .expect("right rect after resize");
        assert_eq!(left_after.width, left_before.width.saturating_sub(1));
        assert_eq!(right_after.width, right_before.width.saturating_add(1));
    }

    #[test]
    fn pane_zoom_exposes_only_active_pane_until_restored() {
        let group = TerminalPaneGroup::new(TerminalEmulator::new());
        let handle = group.handle();
        let area = Rect::new(3, 4, 30, 10);
        {
            let mut shared = group.shared.lock();
            shared.last_area = Some(area);
            shared.refresh_layouts_from_last_area();
        }
        let left_id = handle.panes()[0].id;
        let split = handle
            .split_window(None, TerminalPaneSplit::Vertical)
            .expect("split right");
        let right_id = split.new_pane_id;

        assert!(group.shared.lock().toggle_zoom_active_pane());
        let zoomed = handle.panes();
        assert_eq!(
            zoomed
                .iter()
                .find(|pane| pane.id == right_id)
                .and_then(|pane| pane.rect),
            Some(area)
        );
        assert_eq!(
            zoomed
                .iter()
                .find(|pane| pane.id == left_id)
                .and_then(|pane| pane.rect),
            None
        );

        assert!(group.shared.lock().toggle_zoom_active_pane());
        let restored = handle.panes();
        assert!(
            restored.iter().all(|pane| pane.rect.is_some()),
            "all panes should have visible rects after restoring zoom"
        );
    }

    #[test]
    fn pane_close_active_removes_pane_and_reflows_layout() {
        let group = TerminalPaneGroup::new(TerminalEmulator::new());
        let handle = group.handle();
        {
            let mut shared = group.shared.lock();
            shared.last_area = Some(Rect::new(0, 0, 21, 8));
            shared.refresh_layouts_from_last_area();
        }
        let left_id = handle.panes()[0].id;
        let split = handle
            .split_window(None, TerminalPaneSplit::Vertical)
            .expect("split right");
        assert_ne!(split.new_pane_id, left_id);
        assert!(group.shared.lock().close_active_pane());

        let panes = handle.panes();
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].id, left_id);
        assert!(panes[0].is_active);
        assert_eq!(panes[0].rect, Some(Rect::new(0, 0, 21, 8)));
    }

    #[test]
    fn closing_non_active_pane_preserves_active_focus_and_capture() {
        let group = TerminalPaneGroup::new(TerminalEmulator::new());
        let handle = group.handle();
        {
            let mut shared = group.shared.lock();
            shared.last_area = Some(Rect::new(0, 0, 40, 10));
            shared.refresh_layouts_from_last_area();
        }
        let first_id = handle.panes()[0].id;
        let split = handle
            .split_window(Some(first_id), TerminalPaneSplit::Vertical)
            .expect("split");
        let second_id = split.new_pane_id;

        // Make the first pane active and explicitly non-capturing.
        {
            let mut shared = group.shared.lock();
            shared.focus_pane(first_id, false);
        }
        let active_handle = handle
            .panes()
            .into_iter()
            .find(|p| p.id == first_id)
            .expect("first pane")
            .handle;
        assert!(!active_handle.capture());

        // Close the *other* (non-active) pane.
        group.shared.lock().close_pane(second_id).expect("close");

        // Active pane is unchanged and its non-capturing state is preserved.
        let panes = handle.panes();
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].id, first_id);
        assert!(panes[0].is_active);
        assert!(
            !panes[0].handle.capture(),
            "closing an unrelated pane must not force the active pane into capture mode"
        );
    }

    #[test]
    fn closing_active_pane_transfers_focus_to_spatial_neighbor() {
        // Layout: three panes split left-to-right. Closing the middle pane
        // should move focus to an adjacent pane, not always to the first.
        let group = TerminalPaneGroup::new(TerminalEmulator::new());
        let handle = group.handle();
        {
            let mut shared = group.shared.lock();
            shared.last_area = Some(Rect::new(0, 0, 60, 10));
            shared.refresh_layouts_from_last_area();
        }
        let left_id = handle.panes()[0].id;
        let mid = handle
            .split_window(Some(left_id), TerminalPaneSplit::Vertical)
            .expect("split mid");
        let mid_id = mid.new_pane_id;
        let right = handle
            .split_window(Some(mid_id), TerminalPaneSplit::Vertical)
            .expect("split right");
        let right_id = right.new_pane_id;

        // Focus the middle pane, then close it.
        {
            let mut shared = group.shared.lock();
            shared.focus_pane(mid_id, true);
        }
        group.shared.lock().close_pane(mid_id).expect("close mid");

        let active = handle.active_pane_id().expect("an active pane remains");
        assert_ne!(active.raw(), mid_id.raw());
        assert!(
            active.raw() == left_id.raw() || active.raw() == right_id.raw(),
            "focus should move to a spatial neighbor of the closed pane"
        );
    }

    #[test]
    fn pane_group_apply_config_updates_prefix_shortcut() {
        let theme = Theme::dark();
        let config = TerminalConfig {
            prefix_key: crate::TerminalShortcutConfig::control_letter('a'),
            ..TerminalConfig::default()
        };
        config.validate().expect("valid config");
        let mut group = TerminalPaneGroup::new(TerminalEmulator::new());
        let handle = group.handle();

        handle.apply_config(&config).expect("apply config");
        let prefix = group.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)),
            context(&theme),
        );
        assert!(prefix.is_consumed());
        let split = group.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('%'), KeyModifiers::NONE)),
            context(&theme),
        );

        assert!(split.is_consumed());
        assert_eq!(handle.pane_count(), 2);
    }
}
