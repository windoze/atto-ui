//! System-clipboard sink (OSC 52 + native `arboard`) used by terminal copy
//! operations, and the [`TerminalClipboardCopy`] payload type.

use super::*;

/// System clipboard sink used by [`TerminalEmulator`] copy operations.
///
/// The default implementation sends an OSC 52 clipboard request to the host terminal first and
/// then tries `arboard`, so remote-capable terminal clipboard support takes priority while native
/// clipboard APIs still cover hosts that ignore OSC 52.
pub trait TerminalSystemClipboard: Send + Sync {
    fn copy_text(&self, text: &str) -> Result<()>;
}

impl<F> TerminalSystemClipboard for F
where
    F: Fn(&str) -> Result<()> + Send + Sync,
{
    fn copy_text(&self, text: &str) -> Result<()> {
        self(text)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DefaultTerminalSystemClipboard;

impl TerminalSystemClipboard for DefaultTerminalSystemClipboard {
    fn copy_text(&self, text: &str) -> Result<()> {
        copy_text_with_backends(
            text,
            |text| atto_ui::clipboard::copy_to_system_clipboard(text).map_err(Into::into),
            copy_text_with_arboard,
        )
    }
}

/// OSC 52 clipboard-copy request observed in the terminal output stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalClipboardCopy {
    /// Clipboard selector from the OSC 52 sequence, for example `c`.
    pub selector: Vec<u8>,
    /// Base64-encoded clipboard payload from the OSC 52 sequence.
    pub data: Vec<u8>,
}

impl TerminalClipboardCopy {
    /// Returns whether this OSC 52 request targets the standard clipboard selection.
    pub fn targets_system_clipboard(&self) -> bool {
        self.selector.is_empty() || self.selector.contains(&b'c')
    }

    /// Decodes the OSC 52 base64 payload as UTF-8 clipboard text.
    pub fn decoded_text(&self) -> Result<String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&self.data)
            .map_err(|error| anyhow!("invalid OSC 52 clipboard payload: {error}"))?;
        String::from_utf8(bytes)
            .map_err(|error| anyhow!("OSC 52 clipboard payload is not UTF-8 text: {error}"))
    }
}
