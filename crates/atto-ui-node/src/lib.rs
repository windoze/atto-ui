#![deny(unsafe_code)]
#![allow(unsafe_op_in_unsafe_fn)]

//! Native Node.js binding entry points for atto-ui.

use napi_derive::napi;

/// Return the native package version exposed to JavaScript smoke tests.
#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::version;

    #[test]
    fn version_matches_crate_version() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }
}
