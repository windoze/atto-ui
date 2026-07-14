//! Error conversion helpers for the Node binding.

use atto_ui::{ComponentError, TreeError};
use napi::{Error, Status};

/// Convert invalid JavaScript-facing input into a `TypeError`-like napi error.
pub fn invalid_arg(message: impl Into<String>) -> Error {
    Error::new(Status::InvalidArg, message.into())
}

/// Convert runtime tree errors into JavaScript `Error` objects without dropping context.
pub fn tree_error(error: TreeError) -> Error {
    Error::new(Status::GenericFailure, error.to_string())
}

/// Convert component inspection/property errors into JavaScript `Error` objects.
pub fn component_error(error: ComponentError) -> Error {
    let message = match error {
        ComponentError::NotFound(id) => format!("component not found: {id}"),
        ComponentError::UnsupportedProperty(name) => format!("unsupported property: {name}"),
        ComponentError::InvalidValue { name, expected } => {
            format!("invalid value for {name}: expected {expected}")
        }
        ComponentError::ActionNotSupported(name) => format!("action not supported: {name}"),
        ComponentError::RenderFailed(message) => format!("render failed: {message}"),
        ComponentError::Timeout(message) => format!("timeout: {message}"),
    };
    Error::new(Status::GenericFailure, message)
}

/// Convert arbitrary host errors into JavaScript `Error` objects without dropping context.
pub fn anyhow_error(error: anyhow::Error) -> Error {
    Error::new(Status::GenericFailure, format_anyhow_chain(&error))
}

fn format_anyhow_chain(error: &anyhow::Error) -> String {
    let mut chain = error.chain().map(ToString::to_string);
    let Some(mut message) = chain.next() else {
        return error.to_string();
    };

    for cause in chain {
        message.push_str(": ");
        message.push_str(&cause);
    }
    message
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

    #[test]
    fn anyhow_error_preserves_source_chain_without_debug_details() {
        let source = std::io::Error::new(std::io::ErrorKind::NotFound, "config missing");
        let error = anyhow::Error::new(source).context("could not start host");
        let error = anyhow_error(error);

        assert_eq!(error.reason, "could not start host: config missing");
        assert!(!error.reason.contains("backtrace"));
    }

    #[test]
    fn component_error_uses_stable_messages() {
        let error = component_error(ComponentError::invalid_value("text", "string"));
        assert_eq!(error.reason, "invalid value for text: expected string");
    }
}
