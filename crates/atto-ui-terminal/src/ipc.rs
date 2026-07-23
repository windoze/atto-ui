//! IPC adapter for terminal pane control methods.
//!
//! The core `atto-ui` IPC server owns the Unix socket and UI-thread dispatch.
//! This module registers the terminal-specific method mapping without making
//! the core crate depend on terminal types.

use atto_ui::app::Desktop;
use atto_ui::protocol::{
    BreakPaneResult, CapturePaneResult, DisplayPopupResult, PaneInfo, PaneSelectDirection,
    PaneSplitDirection, ProtocolMethod, ProtocolResult, SelectPaneResult, SendKeysResult,
    SplitWindowResult,
};
use atto_ui::runtime::Rect as ProtocolRect;
use atto_ui::wm::{Window, WindowId, WindowKind};
use atto_ui::{ComponentError, IpcMethodHandler};
use ratatui::layout::Rect;

use crate::{
    TerminalEmulator, TerminalPaneGroupHandle, TerminalPaneId, TerminalPaneSelectDirection,
    TerminalPaneSnapshot, TerminalPaneSplit,
};

/// Terminal pane IPC method dispatcher backed by one or more pane groups.
#[derive(Clone, Default)]
pub struct TerminalPaneIpc {
    groups: Vec<TerminalPaneGroupHandle>,
}

impl TerminalPaneIpc {
    /// Creates a dispatcher for a single terminal pane group.
    pub fn new(group: TerminalPaneGroupHandle) -> Self {
        Self {
            groups: vec![group],
        }
    }

    /// Creates a dispatcher for multiple terminal pane groups.
    pub fn from_groups(groups: impl IntoIterator<Item = TerminalPaneGroupHandle>) -> Self {
        Self {
            groups: groups.into_iter().collect(),
        }
    }

    /// Adds a pane group to the dispatcher.
    pub fn add_group(&mut self, group: TerminalPaneGroupHandle) {
        self.groups.push(group);
    }

    /// Handles terminal pane protocol methods. Non-terminal methods are ignored.
    pub fn handle_method(
        &self,
        desktop: &mut Desktop,
        screen: Rect,
        method: &ProtocolMethod,
    ) -> Result<Option<ProtocolResult>, ComponentError> {
        match method {
            ProtocolMethod::SendKeys(params) => {
                let pane = self.find_pane(params.pane_id)?;
                pane.handle.send_input_bytes(&params.bytes);
                Ok(Some(ProtocolResult::SendKeys(SendKeysResult {
                    pane_id: params.pane_id,
                    byte_count: params.bytes.len(),
                })))
            }
            ProtocolMethod::CapturePane(params) => {
                let pane = self.find_pane(params.pane_id)?;
                let snapshot = pane.handle.snapshot();
                Ok(Some(ProtocolResult::CapturePane(CapturePaneResult {
                    pane_id: params.pane_id,
                    lines: snapshot.lines,
                    cols: snapshot.cols,
                    rows: snapshot.rows,
                    scrollback: snapshot.scrollback,
                })))
            }
            ProtocolMethod::ListPanes(_) => Ok(Some(ProtocolResult::ListPanes(self.list_panes()))),
            ProtocolMethod::SplitWindow(params) => {
                let group = self.resolve_group(params.pane_id)?;
                let outcome = group
                    .split_window(
                        params.pane_id.map(TerminalPaneId::from_raw),
                        terminal_split_direction(params.direction),
                    )
                    .map_err(pane_control_error)?;
                Ok(Some(ProtocolResult::SplitWindow(SplitWindowResult {
                    pane_id: outcome.pane_id.raw(),
                    new_pane_id: outcome.new_pane_id.raw(),
                    pane_count: outcome.pane_count,
                })))
            }
            ProtocolMethod::SelectPane(params) => {
                let group = self.resolve_group(params.pane_id)?;
                let outcome = group
                    .select_pane(
                        params.pane_id.map(TerminalPaneId::from_raw),
                        terminal_select_direction(params.direction),
                    )
                    .map_err(pane_control_error)?;
                Ok(Some(ProtocolResult::SelectPane(SelectPaneResult {
                    previous_pane_id: outcome.previous_pane_id.raw(),
                    pane_id: outcome.pane_id.raw(),
                })))
            }
            ProtocolMethod::BreakPane(params) => {
                let group = self.resolve_group(Some(params.pane_id))?;
                let outcome = group
                    .break_pane(TerminalPaneId::from_raw(params.pane_id))
                    .map_err(pane_control_error)?;
                let pane_id = outcome.pane_id.raw();
                let remaining_pane_count = outcome.remaining_pane_count;
                let window_id = add_detached_pane_window(desktop, screen, outcome)?;
                Ok(Some(ProtocolResult::BreakPane(BreakPaneResult {
                    pane_id,
                    window_id: window_id.raw(),
                    remaining_pane_count,
                })))
            }
            ProtocolMethod::DisplayPopup(params) => {
                let window_id = add_popup_terminal_window(
                    desktop,
                    screen,
                    params.title.clone(),
                    params.rect,
                    params.command.clone(),
                )?;
                Ok(Some(ProtocolResult::DisplayPopup(DisplayPopupResult {
                    window_id: window_id.raw(),
                })))
            }
            _ => Ok(None),
        }
    }

    fn list_panes(&self) -> Vec<PaneInfo> {
        self.groups
            .iter()
            .flat_map(TerminalPaneGroupHandle::panes)
            .map(|pane| PaneInfo {
                pane_id: pane.id.raw(),
                index: pane.index,
                is_active: pane.is_active,
                rect: pane.rect.map(protocol_rect),
            })
            .collect()
    }

    fn find_pane(&self, pane_id: u64) -> Result<TerminalPaneSnapshot, ComponentError> {
        let mut matches = self
            .groups
            .iter()
            .flat_map(TerminalPaneGroupHandle::panes)
            .filter(|pane| pane.id.raw() == pane_id);
        let Some(pane) = matches.next() else {
            return Err(ComponentError::not_found(format!("pane:{pane_id}")));
        };
        if matches.next().is_some() {
            return Err(ComponentError::invalid_value(
                "pane_id",
                "a pane id unique among registered pane groups",
            ));
        }
        Ok(pane)
    }

    fn resolve_group(
        &self,
        pane_id: Option<u64>,
    ) -> Result<TerminalPaneGroupHandle, ComponentError> {
        match pane_id {
            Some(pane_id) => {
                let mut matches = self
                    .groups
                    .iter()
                    .filter(|group| group.panes().iter().any(|pane| pane.id.raw() == pane_id));
                let Some(group) = matches.next() else {
                    return Err(ComponentError::not_found(format!("pane:{pane_id}")));
                };
                if matches.next().is_some() {
                    return Err(ComponentError::invalid_value(
                        "pane_id",
                        "a pane id unique among registered pane groups",
                    ));
                }
                Ok(group.clone())
            }
            None if self.groups.len() == 1 => Ok(self.groups[0].clone()),
            None if self.groups.is_empty() => Err(ComponentError::not_found("pane_group")),
            None => Err(ComponentError::invalid_value(
                "pane_id",
                "an explicit pane id when multiple pane groups are registered",
            )),
        }
    }
}

/// Builds a core IPC method handler for terminal pane control methods.
pub fn terminal_pane_ipc_handler(control: TerminalPaneIpc) -> Box<IpcMethodHandler> {
    Box::new(
        move |desktop: &mut Desktop, screen: Rect, method: &ProtocolMethod| {
            control.handle_method(desktop, screen, method)
        },
    )
}

fn protocol_rect(rect: ratatui::layout::Rect) -> ProtocolRect {
    ProtocolRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

fn terminal_split_direction(direction: PaneSplitDirection) -> TerminalPaneSplit {
    match direction {
        PaneSplitDirection::Vertical => TerminalPaneSplit::Vertical,
        PaneSplitDirection::Horizontal => TerminalPaneSplit::Horizontal,
    }
}

fn terminal_select_direction(direction: PaneSelectDirection) -> TerminalPaneSelectDirection {
    match direction {
        PaneSelectDirection::Left => TerminalPaneSelectDirection::Left,
        PaneSelectDirection::Right => TerminalPaneSelectDirection::Right,
        PaneSelectDirection::Up => TerminalPaneSelectDirection::Up,
        PaneSelectDirection::Down => TerminalPaneSelectDirection::Down,
    }
}

fn pane_control_error(error: anyhow::Error) -> ComponentError {
    ComponentError::invalid_value("pane", error.to_string())
}

fn add_detached_pane_window(
    desktop: &mut Desktop,
    screen: Rect,
    outcome: crate::TerminalPaneBreakOutcome,
) -> Result<WindowId, ComponentError> {
    let title = outcome
        .terminal
        .handle()
        .window_title()
        .unwrap_or_else(|| format!("Pane {}", outcome.pane_id.raw()));
    let rect = outcome
        .rect
        .map(detached_window_rect)
        .unwrap_or_else(|| default_floating_rect(screen));
    let window_id = desktop.add_window(
        Window::new(WindowKind::Normal, title, rect, Box::new(outcome.terminal)),
        screen,
    );
    desktop.wm.focus(window_id);
    Ok(window_id)
}

fn add_popup_terminal_window(
    desktop: &mut Desktop,
    screen: Rect,
    title: Option<String>,
    rect: Option<ProtocolRect>,
    command: Option<Vec<String>>,
) -> Result<WindowId, ComponentError> {
    let mut terminal = TerminalEmulator::new();
    if let Some(command) = command {
        let Some((program, args)) = command.split_first() else {
            return Err(ComponentError::invalid_value("command", "non-empty argv"));
        };
        terminal
            .spawn_process(program, args)
            .map_err(|error| ComponentError::RenderFailed(error.to_string()))?;
    }

    let rect = rect
        .map(ratatui_rect)
        .unwrap_or_else(|| default_popup_rect(screen));
    let window_id = desktop.add_window(
        Window::new(
            WindowKind::Floating,
            title.unwrap_or_else(|| "tmux popup".to_string()),
            rect,
            Box::new(terminal),
        ),
        screen,
    );
    desktop.wm.focus(window_id);
    Ok(window_id)
}

fn ratatui_rect(rect: ProtocolRect) -> Rect {
    Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

fn detached_window_rect(rect: Rect) -> Rect {
    Rect {
        x: rect.x.saturating_sub(1),
        y: rect.y.saturating_sub(1),
        width: rect.width.saturating_add(2),
        height: rect.height.saturating_add(2),
    }
}

fn default_floating_rect(screen: Rect) -> Rect {
    let work = Desktop::layout(screen).work_area;
    Rect {
        x: work.x.saturating_add(2),
        y: work.y.saturating_add(1),
        width: (work.width / 2).max(20).min(work.width.max(1)),
        height: (work.height / 2).max(8).min(work.height.max(1)),
    }
}

fn default_popup_rect(screen: Rect) -> Rect {
    let work = Desktop::layout(screen).work_area;
    let width = (work.width.saturating_mul(3) / 4)
        .max(20)
        .min(work.width.max(1));
    let height = (work.height.saturating_mul(2) / 3)
        .max(8)
        .min(work.height.max(1));
    Rect {
        x: work.x + work.width.saturating_sub(width) / 2,
        y: work.y + work.height.saturating_sub(height) / 2,
        width,
        height,
    }
}
