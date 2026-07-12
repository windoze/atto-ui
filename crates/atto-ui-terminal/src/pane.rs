//! Split-pane container for terminal views.
//!
//! The pane group keeps tmux-like pane layout inside one `atto-ui` window. It deliberately stays
//! above `TerminalEmulator`: each pane is still a normal terminal component, while this container
//! owns the pane tree, pane focus, and pane-level prefix commands.

use std::sync::Arc;

use anyhow::Result;
use atto_ui::composable::{
    Component, ComponentContext, DynamicTree, EventHandling, EventResult, FocusNav, Layout,
    MouseCoordinateSpace, Scrollable, ScrollbarHost, TabMode,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent};
use parking_lot::Mutex;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::{TerminalEmulator, TerminalHandle, TerminalShortcut};

const DIVIDER_THICKNESS: u16 = 1;

/// Stable identifier for one terminal pane inside a [`TerminalPaneGroup`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TerminalPaneId(u64);

impl TerminalPaneId {
    /// Returns the numeric pane id for display and tests.
    pub const fn raw(self) -> u64 {
        self.0
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

/// Observable state for one pane.
#[derive(Clone)]
pub struct TerminalPaneSnapshot {
    pub id: TerminalPaneId,
    pub index: usize,
    pub is_active: bool,
    pub rect: Option<Rect>,
    pub handle: TerminalHandle,
}

#[derive(Clone, Default)]
struct TerminalPaneGroupShared {
    panes: Vec<TerminalPaneSnapshot>,
    active_id: Option<TerminalPaneId>,
    last_error: Option<String>,
}

/// Handle for inspecting a [`TerminalPaneGroup`] from the surrounding app shell.
#[derive(Clone, Default)]
pub struct TerminalPaneGroupHandle {
    shared: Arc<Mutex<TerminalPaneGroupShared>>,
}

impl TerminalPaneGroupHandle {
    /// Returns snapshots for all panes in stable pane order.
    pub fn panes(&self) -> Vec<TerminalPaneSnapshot> {
        self.shared.lock().panes.clone()
    }

    /// Returns the number of panes currently owned by the group.
    pub fn pane_count(&self) -> usize {
        self.shared.lock().panes.len()
    }

    /// Returns the active pane id, if any.
    pub fn active_pane_id(&self) -> Option<TerminalPaneId> {
        self.shared.lock().active_id
    }

    /// Returns the active pane snapshot, if any.
    pub fn active_pane(&self) -> Option<TerminalPaneSnapshot> {
        self.shared
            .lock()
            .panes
            .iter()
            .find(|pane| pane.is_active)
            .cloned()
    }

    /// Returns the active terminal handle, if any.
    pub fn active_terminal_handle(&self) -> Option<TerminalHandle> {
        self.active_pane().map(|pane| pane.handle)
    }

    /// Returns the pane covering an absolute screen coordinate.
    pub fn pane_at_screen_position(&self, x: u16, y: u16) -> Option<TerminalPaneSnapshot> {
        self.shared
            .lock()
            .panes
            .iter()
            .find(|pane| pane.rect.is_some_and(|rect| rect_contains(rect, x, y)))
            .cloned()
    }

    /// Returns and clears the last split creation error.
    pub fn take_last_error(&self) -> Option<String> {
        self.shared.lock().last_error.take()
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
        first: Box<TerminalPaneNode>,
        second: Box<TerminalPaneNode>,
    },
}

impl TerminalPaneNode {
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
}

#[derive(Clone, Copy, Debug)]
struct TerminalPaneLayout {
    id: TerminalPaneId,
    rect: Rect,
}

type TerminalPaneFactory = dyn Fn(usize) -> Result<TerminalEmulator> + Send + Sync;

/// A tmux-like split-pane container for terminal emulators.
pub struct TerminalPaneGroup {
    panes: Vec<TerminalPane>,
    tree: TerminalPaneNode,
    active_id: TerminalPaneId,
    next_id: u64,
    prefix_shortcut: TerminalShortcut,
    prefix_pending: bool,
    last_area: Option<Rect>,
    last_layouts: Vec<TerminalPaneLayout>,
    pane_factory: Option<Arc<TerminalPaneFactory>>,
    shared: Arc<Mutex<TerminalPaneGroupShared>>,
}

impl TerminalPaneGroup {
    /// Creates a pane group with one initial terminal pane.
    pub fn new(initial: TerminalEmulator) -> Self {
        let first_id = TerminalPaneId(1);
        let pane = TerminalPane::new(first_id, initial);
        let shared = Arc::new(Mutex::new(TerminalPaneGroupShared::default()));
        let mut group = Self {
            panes: vec![pane],
            tree: TerminalPaneNode::Leaf(first_id),
            active_id: first_id,
            next_id: 2,
            prefix_shortcut: TerminalShortcut::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
            prefix_pending: false,
            last_area: None,
            last_layouts: Vec::new(),
            pane_factory: None,
            shared,
        };
        group.sync_shared_state();
        group
    }

    /// Returns a handle for observing panes from the app shell.
    pub fn handle(&self) -> TerminalPaneGroupHandle {
        TerminalPaneGroupHandle {
            shared: Arc::clone(&self.shared),
        }
    }

    /// Configures the shortcut used for pane-level prefix commands.
    pub fn prefix_shortcut(mut self, shortcut: TerminalShortcut) -> Self {
        self.prefix_shortcut = shortcut;
        self.prefix_pending = false;
        self
    }

    /// Configures how new panes are created when a split command is invoked.
    pub fn pane_factory<F>(mut self, factory: F) -> Self
    where
        F: Fn(usize) -> Result<TerminalEmulator> + Send + Sync + 'static,
    {
        self.pane_factory = Some(Arc::new(factory));
        self
    }

    fn active_pane_index(&self) -> Option<usize> {
        self.panes.iter().position(|pane| pane.id == self.active_id)
    }

    fn pane_index(&self, id: TerminalPaneId) -> Option<usize> {
        self.panes.iter().position(|pane| pane.id == id)
    }

    fn focus_pane(&mut self, id: TerminalPaneId, capture: bool) -> bool {
        if self.pane_index(id).is_none() {
            return false;
        }
        self.active_id = id;
        for pane in &self.panes {
            pane.handle.set_capture(capture && pane.id == id);
        }
        self.sync_shared_state();
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

    fn create_terminal_for_new_pane(&self, pane_number: usize) -> Result<TerminalEmulator> {
        if let Some(factory) = &self.pane_factory {
            factory(pane_number)
        } else {
            Ok(TerminalEmulator::new())
        }
    }

    fn split_active(&mut self, direction: TerminalPaneSplit) -> bool {
        let Some(active_idx) = self.active_pane_index() else {
            return false;
        };
        let active_id = self.panes[active_idx].id;
        let new_id = TerminalPaneId(self.next_id);
        let pane_number = self.panes.len().saturating_add(1);
        let terminal = match self.create_terminal_for_new_pane(pane_number) {
            Ok(terminal) => terminal,
            Err(error) => {
                self.shared.lock().last_error = Some(error.to_string());
                return false;
            }
        };
        if !self.tree.split_leaf(active_id, new_id, direction) {
            return false;
        }
        self.next_id = self.next_id.saturating_add(1);
        self.panes.push(TerminalPane::new(new_id, terminal));
        self.focus_pane(new_id, true);
        true
    }

    fn sync_shared_state(&mut self) {
        let mut shared = self.shared.lock();
        shared.active_id = Some(self.active_id);
        shared.panes = self
            .panes
            .iter()
            .enumerate()
            .map(|(index, pane)| TerminalPaneSnapshot {
                id: pane.id,
                index,
                is_active: pane.id == self.active_id,
                rect: self
                    .last_layouts
                    .iter()
                    .find(|layout| layout.id == pane.id)
                    .map(|layout| layout.rect),
                handle: pane.handle.clone(),
            })
            .collect();
    }

    fn layout_for_area(
        &self,
        area: Rect,
    ) -> (Vec<TerminalPaneLayout>, Vec<(Rect, TerminalPaneSplit)>) {
        let mut panes = Vec::new();
        let mut dividers = Vec::new();
        layout_pane_node(&self.tree, area, &mut panes, &mut dividers);
        (panes, dividers)
    }

    fn pane_at_screen_position(&self, x: u16, y: u16) -> Option<(usize, Rect)> {
        let layout = self
            .last_layouts
            .iter()
            .find(|layout| rect_contains(layout.rect, x, y))?;
        let index = self.pane_index(layout.id)?;
        Some((index, layout.rect))
    }

    fn handle_pane_prefix_key(
        &mut self,
        key: KeyEvent,
        ctx: ComponentContext<'_>,
    ) -> Option<EventResult> {
        if !ctx.is_focused || key.kind == KeyEventKind::Release {
            return None;
        }
        if self.prefix_pending {
            self.prefix_pending = false;
            return Some(self.handle_prefixed_key(key, ctx));
        }
        let active = self.active_pane_index()?;
        if !self.panes[active].handle.capture() || !shortcut_matches(self.prefix_shortcut, key) {
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
            None => self.replay_prefix_to_active_pane(key, ctx),
        }
    }

    fn replay_prefix_to_active_pane(
        &mut self,
        key: KeyEvent,
        ctx: ComponentContext<'_>,
    ) -> EventResult {
        let Some(active_idx) = self.active_pane_index() else {
            return EventResult::ignored();
        };
        let prefix_event = Event::Key(KeyEvent::new(
            self.prefix_shortcut.code,
            self.prefix_shortcut.modifiers,
        ));
        let child_ctx = child_context(ctx, true);
        let prefix_result = self.panes[active_idx]
            .terminal
            .handle_event(&prefix_event, child_ctx);
        let key_result = self.panes[active_idx]
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
        self.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            self.last_layouts.clear();
            self.sync_shared_state();
            return;
        }

        let (layouts, dividers) = self.layout_for_area(area);
        self.last_layouts = layouts;
        self.sync_shared_state();

        for layout in self.last_layouts.clone() {
            if layout.rect.width == 0 || layout.rect.height == 0 {
                continue;
            }
            let Some(index) = self.pane_index(layout.id) else {
                continue;
            };
            let focused = ctx.is_focused && layout.id == self.active_id;
            let pane_ctx = ComponentContext {
                is_focused: focused,
                scrollbar_host: ScrollbarHost::Window,
                tab_mode: ctx.tab_mode.for_child(),
                mouse_coordinate_space: ctx.mouse_coordinate_space,
                drag: None,
                ..ctx
            };
            self.panes[index]
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
        let Some(active_idx) = self.active_pane_index() else {
            return EventResult::ignored();
        };
        let child_ctx = child_context(ctx, ctx.is_focused);
        self.panes[active_idx]
            .terminal
            .handle_event(event, child_ctx)
    }

    fn handle_mouse_event(&mut self, mouse: MouseEvent, ctx: ComponentContext<'_>) -> EventResult {
        let Some(area) = self.last_area else {
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
            let id = self.panes[pane_idx].id;
            let capture = self.panes[pane_idx].handle.capture();
            let _ = self.focus_pane(id, capture);
        }

        let child_event = Event::Mouse(MouseEvent {
            column: screen_x.saturating_sub(rect.x),
            row: screen_y.saturating_sub(rect.y),
            ..mouse
        });
        let child_focused = ctx.is_focused && self.panes[pane_idx].id == self.active_id;
        let child_ctx = ComponentContext {
            mouse_coordinate_space: MouseCoordinateSpace::Local,
            is_focused: child_focused,
            scrollbar_host: ScrollbarHost::Window,
            tab_mode: ctx.tab_mode.for_child(),
            drag: None,
            ..ctx
        };
        self.panes[pane_idx]
            .terminal
            .handle_event(&child_event, child_ctx)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneCommand {
    SplitVertical,
    SplitHorizontal,
    FocusNext,
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
            first,
            second,
        } => {
            let (first_rect, divider, second_rect) = split_rect(area, *direction);
            layout_pane_node(first, first_rect, panes, dividers);
            if divider.width > 0 && divider.height > 0 {
                dividers.push((divider, *direction));
            }
            layout_pane_node(second, second_rect, panes, dividers);
        }
    }
}

fn split_rect(area: Rect, direction: TerminalPaneSplit) -> (Rect, Rect, Rect) {
    match direction {
        TerminalPaneSplit::Vertical => split_rect_vertical(area),
        TerminalPaneSplit::Horizontal => split_rect_horizontal(area),
    }
}

fn split_rect_vertical(area: Rect) -> (Rect, Rect, Rect) {
    if area.width <= DIVIDER_THICKNESS {
        return (area, Rect::default(), Rect::default());
    }
    let available = area.width.saturating_sub(DIVIDER_THICKNESS);
    let first_width = available / 2;
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

fn split_rect_horizontal(area: Rect) -> (Rect, Rect, Rect) {
    if area.height <= DIVIDER_THICKNESS {
        return (area, Rect::default(), Rect::default());
    }
    let available = area.height.saturating_sub(DIVIDER_THICKNESS);
    let first_height = available / 2;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_rects_reserve_one_cell_divider() {
        let area = Rect::new(2, 3, 11, 7);
        let (left, divider, right) = split_rect_vertical(area);
        assert_eq!(left, Rect::new(2, 3, 5, 7));
        assert_eq!(divider, Rect::new(7, 3, 1, 7));
        assert_eq!(right, Rect::new(8, 3, 5, 7));

        let (top, divider, bottom) = split_rect_horizontal(area);
        assert_eq!(top, Rect::new(2, 3, 11, 3));
        assert_eq!(divider, Rect::new(2, 6, 11, 1));
        assert_eq!(bottom, Rect::new(2, 7, 11, 3));
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
    }
}
