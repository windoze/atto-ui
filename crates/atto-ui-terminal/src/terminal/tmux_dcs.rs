//! tmux DCS passthrough decoder: strips the tmux wrapper around forwarded
//! DCS sequences (notably OSC 52 clipboard) and unescapes its body.

use super::*;

#[derive(Default)]
pub(crate) struct TmuxDcsPassthroughDecoder {
    pub(crate) state: TmuxDcsPassthroughState,
}

#[derive(Default)]
pub(crate) enum TmuxDcsPassthroughState {
    #[default]
    Ground,
    Esc,
    DcsPrefix {
        raw: Vec<u8>,
        matched: usize,
    },
    IgnoredDcs {
        pending_esc: bool,
    },
    TmuxBody {
        raw: Vec<u8>,
        body: Vec<u8>,
        pending_esc: bool,
    },
}

impl TmuxDcsPassthroughDecoder {
    /// Unwraps complete tmux DCS passthrough frames before vt100 sees the output stream.
    pub(crate) fn decode(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            self.push_byte(byte, &mut output);
        }
        output
    }

    pub(crate) fn push_byte(&mut self, byte: u8, output: &mut Vec<u8>) {
        let state = std::mem::take(&mut self.state);
        match state {
            TmuxDcsPassthroughState::Ground => {
                if byte == 0x1b {
                    self.state = TmuxDcsPassthroughState::Esc;
                } else {
                    output.push(byte);
                }
            }
            TmuxDcsPassthroughState::Esc => {
                if byte == b'P' {
                    self.state = TmuxDcsPassthroughState::DcsPrefix {
                        raw: vec![0x1b, b'P'],
                        matched: 0,
                    };
                } else {
                    output.push(0x1b);
                    if byte == 0x1b {
                        self.state = TmuxDcsPassthroughState::Esc;
                    } else {
                        output.push(byte);
                    }
                }
            }
            TmuxDcsPassthroughState::DcsPrefix { raw, matched } => {
                self.push_dcs_prefix_byte(raw, matched, byte);
            }
            TmuxDcsPassthroughState::IgnoredDcs { pending_esc } => {
                self.push_ignored_dcs_byte(pending_esc, byte);
            }
            TmuxDcsPassthroughState::TmuxBody {
                mut raw,
                mut body,
                pending_esc,
            } => {
                self.push_tmux_body_byte(&mut raw, &mut body, pending_esc, byte, output);
            }
        }
    }

    pub(crate) fn push_dcs_prefix_byte(&mut self, mut raw: Vec<u8>, matched: usize, byte: u8) {
        raw.push(byte);
        if byte == TMUX_DCS_PREFIX[matched] {
            let matched = matched + 1;
            if matched == TMUX_DCS_PREFIX.len() {
                self.state = TmuxDcsPassthroughState::TmuxBody {
                    raw,
                    body: Vec::new(),
                    pending_esc: false,
                };
            } else {
                self.state = TmuxDcsPassthroughState::DcsPrefix { raw, matched };
            }
        } else {
            // Unknown DCS content must remain non-executable. vt100 treats ESC
            // inside DCS too eagerly, so consume the control string instead of
            // exposing nested OSC bytes such as clipboard requests.
            self.state = TmuxDcsPassthroughState::IgnoredDcs {
                pending_esc: byte == 0x1b,
            };
        }
    }

    pub(crate) fn push_ignored_dcs_byte(&mut self, pending_esc: bool, byte: u8) {
        self.state = if pending_esc && byte == b'\\' {
            TmuxDcsPassthroughState::Ground
        } else {
            TmuxDcsPassthroughState::IgnoredDcs {
                pending_esc: byte == 0x1b,
            }
        };
    }

    pub(crate) fn push_tmux_body_byte(
        &mut self,
        raw: &mut Vec<u8>,
        body: &mut Vec<u8>,
        pending_esc: bool,
        byte: u8,
        output: &mut Vec<u8>,
    ) {
        raw.push(byte);
        if pending_esc {
            if byte == b'\\' {
                if let Some(decoded) = unescape_tmux_dcs_body(body) {
                    output.extend(decoded);
                }
                // Malformed tmux passthrough is not forwarded, because the raw
                // frame can contain nested OSC that must not execute.
                self.state = TmuxDcsPassthroughState::Ground;
                return;
            }
            body.push(0x1b);
            body.push(byte);
            self.state = TmuxDcsPassthroughState::TmuxBody {
                raw: std::mem::take(raw),
                body: std::mem::take(body),
                pending_esc: false,
            };
        } else if byte == 0x1b {
            self.state = TmuxDcsPassthroughState::TmuxBody {
                raw: std::mem::take(raw),
                body: std::mem::take(body),
                pending_esc: true,
            };
        } else {
            body.push(byte);
            self.state = TmuxDcsPassthroughState::TmuxBody {
                raw: std::mem::take(raw),
                body: std::mem::take(body),
                pending_esc: false,
            };
        }

        if self.buffered_len() > TMUX_DCS_MAX_BUFFERED {
            self.drop_pending_control_string();
        }
    }

    pub(crate) fn buffered_len(&self) -> usize {
        match &self.state {
            TmuxDcsPassthroughState::Ground => 0,
            TmuxDcsPassthroughState::Esc => 1,
            TmuxDcsPassthroughState::IgnoredDcs { .. } => 0,
            TmuxDcsPassthroughState::DcsPrefix { raw, .. }
            | TmuxDcsPassthroughState::TmuxBody { raw, .. } => raw.len(),
        }
    }

    pub(crate) fn drop_pending_control_string(&mut self) {
        match std::mem::take(&mut self.state) {
            TmuxDcsPassthroughState::Ground => {}
            TmuxDcsPassthroughState::Esc => {}
            TmuxDcsPassthroughState::IgnoredDcs { .. } => {}
            TmuxDcsPassthroughState::DcsPrefix { .. }
            | TmuxDcsPassthroughState::TmuxBody { .. } => {}
        }
        self.state = TmuxDcsPassthroughState::Ground;
    }
}

pub(crate) fn unescape_tmux_dcs_body(body: &[u8]) -> Option<Vec<u8>> {
    let mut decoded = Vec::with_capacity(body.len());
    let mut index = 0;
    while index < body.len() {
        if body[index] == 0x1b {
            if body.get(index + 1) != Some(&0x1b) {
                return None;
            }
            decoded.push(0x1b);
            index += 2;
        } else {
            decoded.push(body[index]);
            index += 1;
        }
    }
    Some(decoded)
}
