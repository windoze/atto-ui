//! Session descriptions used by terminal app shells.
//!
//! A session spec is intentionally small: it records which profile was chosen,
//! which program should be spawned, and the cwd that belongs to that window.

use std::env;
use std::path::{Path, PathBuf};

use portable_pty::CommandBuilder;

/// Spawn profile and command metadata for one terminal session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSessionSpec {
    profile: String,
    program: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
}

impl TerminalSessionSpec {
    /// Creates a session spec for an explicit program and argument vector.
    pub fn new(
        profile: impl Into<String>,
        program: impl Into<String>,
        args: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            profile: profile.into(),
            program: program.into(),
            args: args.into_iter().collect(),
            cwd: None,
        }
    }

    /// Creates the default shell profile from `$SHELL`, falling back to `/bin/sh`.
    pub fn shell_from_env() -> Self {
        Self::new(
            "Shell",
            env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
            Vec::new(),
        )
    }

    /// Creates a named command profile.
    pub fn command(
        profile: impl Into<String>,
        program: impl Into<String>,
        args: impl IntoIterator<Item = String>,
    ) -> Self {
        Self::new(profile, program, args)
    }

    /// Returns the user-visible profile name that selected this session.
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// Returns the executable path or name.
    pub fn program(&self) -> &str {
        &self.program
    }

    /// Returns the configured command arguments.
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Returns the cwd owned by this session, if one has been selected or observed.
    pub fn cwd(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }

    /// Sets the cwd for future spawns of this session.
    pub fn set_cwd(&mut self, cwd: impl Into<PathBuf>) {
        self.cwd = Some(cwd.into());
    }

    /// Clears the cwd so the subprocess inherits the app process cwd.
    pub fn clear_cwd(&mut self) {
        self.cwd = None;
    }

    /// Returns a copy of this session with the provided cwd.
    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.set_cwd(cwd);
        self
    }

    /// Builds a PTY command for this session.
    pub fn command_builder(&self) -> CommandBuilder {
        let mut cmd = CommandBuilder::new(&self.program);
        for arg in &self.args {
            cmd.arg(arg);
        }
        if let Some(cwd) = &self.cwd {
            cmd.cwd(cwd.as_os_str());
        }
        cmd
    }

    /// Formats the command for status text and menus.
    pub fn command_line(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_spec_builds_command_with_args_and_cwd() {
        let spec = TerminalSessionSpec::command(
            "Command",
            "/bin/sh",
            vec!["-c".to_string(), "pwd".to_string()],
        )
        .with_cwd("/tmp");

        let cmd = spec.command_builder();
        assert_eq!(cmd.get_argv().len(), 3);
        assert_eq!(cmd.get_argv()[0], "/bin/sh");
        assert_eq!(cmd.get_argv()[1], "-c");
        assert_eq!(cmd.get_argv()[2], "pwd");
        assert_eq!(cmd.get_cwd().and_then(|cwd| cwd.to_str()), Some("/tmp"));
    }

    #[test]
    fn session_spec_tracks_profile_independently_from_command() {
        let mut spec = TerminalSessionSpec::new("Project", "/bin/zsh", Vec::new());
        assert_eq!(spec.profile(), "Project");
        assert_eq!(spec.program(), "/bin/zsh");
        assert_eq!(spec.cwd(), None);

        spec.set_cwd("/workspace");
        assert_eq!(spec.cwd(), Some(Path::new("/workspace")));
        spec.clear_cwd();
        assert_eq!(spec.cwd(), None);
    }
}
