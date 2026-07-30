//! DSR / device-report query-reply handling (cursor position, DA1/DA2, kitty
//! keyboard protocol capability) collected from the parser callbacks.

use super::*;

pub(crate) enum DsrResponse {
    Cursor {
        private: bool,
    },
    Status {
        private: bool,
    },
    /// Primary Device Attributes (DA1): `CSI c` / `CSI 0 c`.
    PrimaryDeviceAttributes,
    /// Secondary Device Attributes (DA2): `CSI > c` / `CSI > 0 c`.
    SecondaryDeviceAttributes,
    /// Kitty keyboard-protocol flags query: `CSI ? u`. We do not implement the
    /// protocol, so we report flags `0`.
    KittyKeyboardFlags,
}

/// Scans program output for terminal queries that expect a synchronous reply and
/// returns the reply byte sequences. Handling these prevents full-screen apps
/// (notably Neovim) from blocking on a ~1s startup/teardown timeout while they
/// wait for Device Attributes / keyboard-protocol answers that never arrive.
///
/// A trailing partial escape sequence is buffered in `dsr_tail` and re-scanned
/// on the next chunk.
pub(crate) fn collect_dsr_responses(shared: &mut TerminalShared, bytes: &[u8]) -> Vec<Vec<u8>> {
    if bytes.is_empty() {
        return Vec::new();
    }

    let mut combined = Vec::with_capacity(shared.dsr_tail.len() + bytes.len());
    combined.extend_from_slice(&shared.dsr_tail);
    combined.extend_from_slice(bytes);

    let mut responses = Vec::new();
    let mut idx = 0;
    let mut tail_start = combined.len();
    while idx < combined.len() {
        if combined[idx] != 0x1b {
            idx += 1;
            continue;
        }
        // Need at least `ESC [` to be a CSI.
        if idx + 1 >= combined.len() {
            tail_start = idx;
            break;
        }
        if combined[idx + 1] != b'[' {
            idx += 1;
            continue;
        }

        // Parse the CSI body: an optional leading private marker (`?` or `>`),
        // then parameter bytes, then a final byte.
        let mut j = idx + 2;
        let prefix = match combined.get(j) {
            None => {
                tail_start = idx;
                break;
            }
            Some(&b'?') => {
                j += 1;
                Some(b'?')
            }
            Some(&b'>') => {
                j += 1;
                Some(b'>')
            }
            Some(_) => None,
        };
        let params_start = j;
        while j < combined.len() && combined[j].is_ascii_digit() {
            j += 1;
        }
        let Some(&final_byte) = combined.get(j) else {
            // Incomplete sequence; buffer and wait for more input.
            tail_start = idx;
            break;
        };
        let params = &combined[params_start..j];

        let matched = match (prefix, final_byte) {
            // DSR — device status report.
            (None, b'n') => match params {
                b"6" => Some(DsrResponse::Cursor { private: false }),
                b"5" => Some(DsrResponse::Status { private: false }),
                _ => None,
            },
            (Some(b'?'), b'n') => match params {
                b"6" => Some(DsrResponse::Cursor { private: true }),
                b"5" => Some(DsrResponse::Status { private: true }),
                _ => None,
            },
            // Primary Device Attributes (DA1): `CSI c` or `CSI 0 c`.
            (None, b'c') if params.is_empty() || params == b"0" => {
                Some(DsrResponse::PrimaryDeviceAttributes)
            }
            // Secondary Device Attributes (DA2): `CSI > c` or `CSI > 0 c`.
            (Some(b'>'), b'c') if params.is_empty() || params == b"0" => {
                Some(DsrResponse::SecondaryDeviceAttributes)
            }
            // Kitty keyboard-protocol flags query: `CSI ? u`.
            (Some(b'?'), b'u') if params.is_empty() => Some(DsrResponse::KittyKeyboardFlags),
            _ => None,
        };

        if let Some(response) = matched {
            responses.push(response);
            idx = j + 1;
            continue;
        }
        idx += 1;
    }

    shared.dsr_tail.clear();
    let tail = &combined[tail_start..];
    // A pending query is tiny; an over-long tail is an unterminated/garbage CSI
    // that we will never complete, so drop it instead of buffering unboundedly.
    if tail.len() <= DSR_TAIL_MAX {
        shared.dsr_tail.extend_from_slice(tail);
    }

    if responses.is_empty() {
        return Vec::new();
    }

    responses
        .into_iter()
        .map(|response| match response {
            DsrResponse::Cursor { private } => {
                let (row, col) = shared.parser.screen().cursor_position();
                let row = row.saturating_add(1);
                let col = col.saturating_add(1);
                if private {
                    format!("\x1b[?{row};{col}R").into_bytes()
                } else {
                    format!("\x1b[{row};{col}R").into_bytes()
                }
            }
            DsrResponse::Status { private } => {
                if private {
                    b"\x1b[?0n".to_vec()
                } else {
                    b"\x1b[0n".to_vec()
                }
            }
            // Report as a VT220 with no optional extensions. This is enough to
            // satisfy programs that gate startup on a DA1 reply.
            DsrResponse::PrimaryDeviceAttributes => b"\x1b[?62c".to_vec(),
            // Terminal id 0, firmware version 0, ROM 0 (`CSI > 0 ; 0 ; 0 c`).
            DsrResponse::SecondaryDeviceAttributes => b"\x1b[>0;0;0c".to_vec(),
            // Kitty keyboard protocol unsupported: report flags 0.
            DsrResponse::KittyKeyboardFlags => b"\x1b[?0u".to_vec(),
        })
        .collect()
}
