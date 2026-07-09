#![forbid(unsafe_code)]

//! Application crate for the Atto TUI agent.
//!
//! The crate is intentionally thin at this stage: later milestones will compose
//! `atto-ui`, `atto-ui-chat`, and `atto-ui-async` here without adding network
//! dependencies to the reusable UI crates.

use anyhow::Result;

/// Runs the TUI agent application.
pub fn run() -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn skeleton_run_succeeds() {
        run().expect("agent app skeleton should start without work");
    }
}
