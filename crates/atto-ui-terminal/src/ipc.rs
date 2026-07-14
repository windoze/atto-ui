//! IPC adapter for terminal pane control methods.
//!
//! The core `atto-ui` IPC server owns the Unix socket and UI-thread dispatch.
//! This module registers the terminal-specific method mapping without making
//! the core crate depend on terminal types.

use atto_ui::app::Desktop;
use atto_ui::protocol::{
    CapturePaneResult, PaneInfo, ProtocolMethod, ProtocolResult, SendKeysResult,
};
use atto_ui::runtime::Rect as ProtocolRect;
use atto_ui::{ComponentError, IpcMethodHandler};

use crate::{TerminalPaneGroupHandle, TerminalPaneSnapshot};

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
}

/// Builds a core IPC method handler for terminal pane control methods.
pub fn terminal_pane_ipc_handler(control: TerminalPaneIpc) -> Box<IpcMethodHandler> {
    Box::new(move |_desktop: &mut Desktop, method: &ProtocolMethod| control.handle_method(method))
}

fn protocol_rect(rect: ratatui::layout::Rect) -> ProtocolRect {
    ProtocolRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}
