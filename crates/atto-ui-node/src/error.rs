//! Error conversion helpers for the Node binding.

use atto_ui::TreeError;
use napi::{Error, Status};

/// Convert invalid JavaScript-facing input into a `TypeError`-like napi error.
pub fn invalid_arg(message: impl Into<String>) -> Error {
    Error::new(Status::InvalidArg, message.into())
}

/// Convert runtime tree errors into JavaScript `Error` objects without dropping context.
pub fn tree_error(error: TreeError) -> Error {
    Error::new(Status::GenericFailure, error.to_string())
}

/// Convert arbitrary host errors into JavaScript `Error` objects without dropping context.
pub fn anyhow_error(error: anyhow::Error) -> Error {
    Error::new(Status::GenericFailure, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_error_preserves_display_message() {
        let error = tree_error(TreeError::NotFound("missing".to_string()));
        assert_eq!(error.reason, "node not found: missing");
    }

    #[test]
    fn anyhow_error_preserves_display_message() {
        let error = anyhow_error(anyhow::anyhow!("host failed: {code}", code = 7));
        assert_eq!(error.reason, "host failed: 7");
    }
}
