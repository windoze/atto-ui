//! Persistent configuration model for the terminal app.
//!
//! The model stays serializable and validation-focused, while runtime conversion helpers feed live
//! terminal widgets and app settings windows.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail, ensure};
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::session::TerminalSessionSpec;
use crate::terminal::{TerminalCursorShape, TerminalShellIntegration, TerminalShortcut};

pub const DEFAULT_TERMINAL_SCROLLBACK_LEN: usize = 2000;
pub const DEFAULT_TERMINAL_SCROLL_STEP: u16 = 3;
pub const DEFAULT_TERMINAL_PROFILE_NAME: &str = "Shell";
pub const DEFAULT_TERMINAL_SHELL_FALLBACK: &str = "/bin/sh";

/// Top-level terminal app settings persisted as JSON or YAML.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalConfig {
    #[serde(default = "default_scrollback_len")]
    pub scrollback_len: usize,
    #[serde(default)]
    pub palette: TerminalPaletteConfig,
    #[serde(default = "default_prefix_key_config")]
    pub prefix_key: TerminalShortcutConfig,
    #[serde(default = "default_release_shortcut_config")]
    pub release_shortcut: TerminalShortcutConfig,
    #[serde(default)]
    pub alternate_screen_scroll: TerminalAlternateScreenScrollConfig,
    #[serde(default)]
    pub sessions: TerminalSessionsConfig,
    #[serde(default)]
    pub shell_integration: TerminalShellIntegrationConfig,
    #[serde(default)]
    pub tmux: TerminalTmuxEnvironmentConfig,
    #[serde(default)]
    pub close_window_on_shell_exit: bool,
    #[serde(default)]
    pub cursor: TerminalCursorConfig,
}

/// Supported terminal configuration file encodings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalConfigFormat {
    Json,
    Yaml,
}

impl TerminalConfig {
    /// Parses and validates a terminal config string using the selected format.
    pub fn from_str(input: &str, format: TerminalConfigFormat) -> Result<Self> {
        let config = match format {
            TerminalConfigFormat::Json => {
                serde_json::from_str(input).context("parse terminal config JSON")?
            }
            TerminalConfigFormat::Yaml => {
                serde_yaml::from_str(input).context("parse terminal config YAML")?
            }
        };
        validate_config(config)
    }

    /// Parses and validates terminal config bytes using the selected format.
    pub fn from_bytes(input: &[u8], format: TerminalConfigFormat) -> Result<Self> {
        let config = match format {
            TerminalConfigFormat::Json => {
                serde_json::from_slice(input).context("parse terminal config JSON")?
            }
            TerminalConfigFormat::Yaml => {
                serde_yaml::from_slice(input).context("parse terminal config YAML")?
            }
        };
        validate_config(config)
    }

    /// Serializes a validated terminal config to the selected format.
    pub fn to_string(&self, format: TerminalConfigFormat) -> Result<String> {
        self.validate()?;
        match format {
            TerminalConfigFormat::Json => {
                serde_json::to_string_pretty(self).context("serialize terminal config JSON")
            }
            TerminalConfigFormat::Yaml => {
                serde_yaml::to_string(self).context("serialize terminal config YAML")
            }
        }
    }

    /// Loads a terminal config from a path whose extension selects JSON or YAML.
    pub fn load_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes =
            fs::read(path).with_context(|| format!("read terminal config {}", path.display()))?;
        Self::from_bytes_infer(&bytes, Some(path))
    }

    /// Saves a terminal config to a path using the requested format.
    pub fn save_path(&self, path: impl AsRef<Path>, format: TerminalConfigFormat) -> Result<()> {
        let path = path.as_ref();
        let content = self.to_string(format)?;
        fs::write(path, content)
            .with_context(|| format!("write terminal config {}", path.display()))
    }

    /// Saves a terminal config to a path whose extension selects JSON or YAML.
    pub fn save_path_infer(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let format = Self::infer_format_from_path(path)
            .with_context(|| format!("infer terminal config format from {}", path.display()))?;
        self.save_path(path, format)
    }

    /// Infers the config format from a `.json`, `.yaml`, or `.yml` path extension.
    pub fn infer_format_from_path(path: &Path) -> Option<TerminalConfigFormat> {
        let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
        match ext.as_str() {
            "json" => Some(TerminalConfigFormat::Json),
            "yaml" | "yml" => Some(TerminalConfigFormat::Yaml),
            _ => None,
        }
    }

    /// Parses bytes by extension when available, otherwise by trying JSON then YAML.
    pub fn from_bytes_infer(input: &[u8], path: Option<&Path>) -> Result<Self> {
        if let Some(path) = path
            && let Some(format) = Self::infer_format_from_path(path)
        {
            return Self::from_bytes(input, format)
                .with_context(|| format!("parse terminal config file {}", path.display()));
        }

        let json_err = match Self::from_bytes(input, TerminalConfigFormat::Json) {
            Ok(config) => return Ok(config),
            Err(error) => error,
        };
        match Self::from_bytes(input, TerminalConfigFormat::Yaml) {
            Ok(config) => Ok(config),
            Err(yaml_err) => Err(anyhow!(
                "failed to parse terminal config as JSON ({json_err}) or YAML ({yaml_err})"
            )),
        }
    }

    /// Validates cross-field invariants and all nested parseable values.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.scrollback_len > 0,
            "terminal scrollback_len must be greater than zero"
        );
        self.prefix_shortcut()
            .context("invalid terminal prefix_key")?;
        self.release_shortcut()
            .context("invalid terminal release_shortcut")?;
        self.alternate_screen_scroll
            .validate()
            .context("invalid terminal alternate_screen_scroll")?;
        self.palette
            .validate()
            .context("invalid terminal palette")?;
        self.sessions
            .validate()
            .context("invalid terminal sessions")?;
        self.tmux
            .validate()
            .context("invalid terminal tmux environment")?;
        Ok(())
    }

    /// Converts the persisted prefix key to the runtime shortcut type.
    pub fn prefix_shortcut(&self) -> Result<TerminalShortcut> {
        self.prefix_key.to_prefix_shortcut()
    }

    /// Converts the persisted release shortcut to the runtime shortcut type.
    pub fn release_shortcut(&self) -> Result<TerminalShortcut> {
        self.release_shortcut.to_shortcut()
    }

    /// Converts the shell integration toggle to the runtime spawn policy.
    pub fn shell_integration_policy(&self) -> TerminalShellIntegration {
        self.shell_integration.to_policy()
    }
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            scrollback_len: DEFAULT_TERMINAL_SCROLLBACK_LEN,
            palette: TerminalPaletteConfig::default(),
            prefix_key: default_prefix_key_config(),
            release_shortcut: default_release_shortcut_config(),
            alternate_screen_scroll: TerminalAlternateScreenScrollConfig::default(),
            sessions: TerminalSessionsConfig::default(),
            shell_integration: TerminalShellIntegrationConfig::default(),
            tmux: TerminalTmuxEnvironmentConfig::default(),
            close_window_on_shell_exit: false,
            cursor: TerminalCursorConfig::default(),
        }
    }
}

/// Serializable color spec used by terminal palette entries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TerminalColorSpec(String);

impl TerminalColorSpec {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parses the color into Ratatui's color type for future rendering integration.
    pub fn to_color(&self) -> Result<Color> {
        parse_color_spec(&self.0)
    }
}

impl From<&str> for TerminalColorSpec {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Configurable terminal foreground/background and ANSI 0-15 palette.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalPaletteConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground: Option<TerminalColorSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<TerminalColorSpec>,
    #[serde(default = "default_ansi_palette")]
    pub ansi: [TerminalColorSpec; 16],
}

impl TerminalPaletteConfig {
    /// Validates every color spec while keeping default foreground/background theme-driven.
    pub fn validate(&self) -> Result<()> {
        if let Some(color) = &self.foreground {
            color.to_color().context("invalid foreground color")?;
        }
        if let Some(color) = &self.background {
            color.to_color().context("invalid background color")?;
        }
        for (index, color) in self.ansi.iter().enumerate() {
            color
                .to_color()
                .with_context(|| format!("invalid ANSI color {index}"))?;
        }
        Ok(())
    }

    pub fn foreground_color(&self) -> Result<Option<Color>> {
        self.foreground
            .as_ref()
            .map(TerminalColorSpec::to_color)
            .transpose()
    }

    pub fn background_color(&self) -> Result<Option<Color>> {
        self.background
            .as_ref()
            .map(TerminalColorSpec::to_color)
            .transpose()
    }

    pub fn color_for_index(&self, index: u8) -> Result<Color> {
        if let Some(color) = self.ansi.get(usize::from(index)) {
            color.to_color()
        } else {
            Ok(Color::Indexed(index))
        }
    }
}

impl Default for TerminalPaletteConfig {
    fn default() -> Self {
        Self {
            foreground: None,
            background: None,
            ansi: default_ansi_palette(),
        }
    }
}

/// Modifier names supported by persisted terminal shortcuts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalShortcutModifier {
    #[serde(alias = "ctrl")]
    Control,
    Shift,
    #[serde(alias = "option")]
    Alt,
}

impl TerminalShortcutModifier {
    fn flag(self) -> KeyModifiers {
        match self {
            Self::Control => KeyModifiers::CONTROL,
            Self::Shift => KeyModifiers::SHIFT,
            Self::Alt => KeyModifiers::ALT,
        }
    }
}

/// Serializable keyboard shortcut.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalShortcutConfig {
    pub key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<TerminalShortcutModifier>,
}

impl TerminalShortcutConfig {
    pub fn new(
        key: impl Into<String>,
        modifiers: impl IntoIterator<Item = TerminalShortcutModifier>,
    ) -> Self {
        Self {
            key: key.into(),
            modifiers: modifiers.into_iter().collect(),
        }
    }

    pub fn control_letter(letter: char) -> Self {
        Self::new(
            letter.to_ascii_lowercase().to_string(),
            [TerminalShortcutModifier::Control],
        )
    }

    /// Converts the persisted key/modifier pair to the runtime shortcut type.
    pub fn to_shortcut(&self) -> Result<TerminalShortcut> {
        Ok(TerminalShortcut::new(
            parse_key_code(&self.key)?,
            self.modifiers()?,
        ))
    }

    /// Converts and validates a prefix key as plain Ctrl+ASCII-letter.
    pub fn to_prefix_shortcut(&self) -> Result<TerminalShortcut> {
        let shortcut = self.to_shortcut()?;
        ensure!(
            shortcut.modifiers == KeyModifiers::CONTROL,
            "terminal prefix shortcut must be plain Ctrl+<ASCII letter>"
        );
        let KeyCode::Char(letter) = shortcut.code else {
            bail!("terminal prefix shortcut must be plain Ctrl+<ASCII letter>");
        };
        ensure!(
            letter.is_ascii_alphabetic(),
            "terminal prefix shortcut must be plain Ctrl+<ASCII letter>"
        );
        Ok(TerminalShortcut::new(
            KeyCode::Char(letter.to_ascii_lowercase()),
            KeyModifiers::CONTROL,
        ))
    }

    fn modifiers(&self) -> Result<KeyModifiers> {
        let mut seen = BTreeSet::new();
        let mut out = KeyModifiers::NONE;
        for modifier in &self.modifiers {
            ensure!(
                seen.insert(*modifier),
                "duplicate shortcut modifier {modifier:?}"
            );
            out |= modifier.flag();
        }
        Ok(out)
    }
}

/// How mouse-wheel events are translated while a child app owns the alternate screen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalAlternateScreenScrollConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_scroll_step")]
    pub step: u16,
    #[serde(default = "default_alt_scroll_up_key")]
    pub scroll_up_key: TerminalShortcutConfig,
    #[serde(default = "default_alt_scroll_down_key")]
    pub scroll_down_key: TerminalShortcutConfig,
}

impl TerminalAlternateScreenScrollConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.step > 0,
            "alternate screen scroll step must be greater than zero"
        );
        self.scroll_up_key
            .to_shortcut()
            .context("invalid scroll_up_key")?;
        self.scroll_down_key
            .to_shortcut()
            .context("invalid scroll_down_key")?;
        Ok(())
    }
}

impl Default for TerminalAlternateScreenScrollConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            step: DEFAULT_TERMINAL_SCROLL_STEP,
            scroll_up_key: default_alt_scroll_up_key(),
            scroll_down_key: default_alt_scroll_down_key(),
        }
    }
}

/// Spawn profiles and the selected default profile for terminal windows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSessionsConfig {
    #[serde(default = "default_profile_name")]
    pub default_profile: String,
    #[serde(default = "default_profiles")]
    pub profiles: Vec<TerminalProfileConfig>,
}

impl TerminalSessionsConfig {
    /// Validates names, commands, uniqueness, and default-profile membership.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.profiles.is_empty(),
            "terminal sessions must contain at least one profile"
        );
        ensure!(
            !self.default_profile.trim().is_empty(),
            "terminal default_profile must not be empty"
        );
        let mut names = BTreeSet::new();
        let mut has_default = false;
        for profile in &self.profiles {
            profile.validate()?;
            ensure!(
                names.insert(profile.name.as_str()),
                "duplicate terminal profile {:?}",
                profile.name
            );
            if profile.name == self.default_profile {
                has_default = true;
            }
        }
        ensure!(
            has_default,
            "terminal default_profile {:?} must match a configured profile",
            self.default_profile
        );
        Ok(())
    }

    pub fn default_profile(&self) -> Option<&TerminalProfileConfig> {
        self.profiles
            .iter()
            .find(|profile| profile.name == self.default_profile)
    }
}

impl Default for TerminalSessionsConfig {
    fn default() -> Self {
        Self {
            default_profile: default_profile_name(),
            profiles: default_profiles(),
        }
    }
}

/// One terminal session profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalProfileConfig {
    pub name: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
}

impl TerminalProfileConfig {
    pub fn new(
        name: impl Into<String>,
        command: impl Into<String>,
        args: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            args: args.into_iter().collect(),
            cwd: None,
        }
    }

    pub fn shell_from_env() -> Self {
        Self::new(
            DEFAULT_TERMINAL_PROFILE_NAME,
            env::var("SHELL").unwrap_or_else(|_| DEFAULT_TERMINAL_SHELL_FALLBACK.to_string()),
            Vec::new(),
        )
    }

    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.trim().is_empty(),
            "terminal profile name must not be empty"
        );
        ensure!(
            !self.command.trim().is_empty(),
            "terminal profile {:?} command must not be empty",
            self.name
        );
        Ok(())
    }

    pub fn to_session_spec(&self) -> TerminalSessionSpec {
        let mut spec =
            TerminalSessionSpec::new(self.name.clone(), self.command.clone(), self.args.clone());
        if let Some(cwd) = &self.cwd {
            spec.set_cwd(cwd);
        }
        spec
    }
}

/// Shell integration injection settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalShellIntegrationConfig {
    #[serde(default)]
    pub inject: bool,
}

impl TerminalShellIntegrationConfig {
    pub fn to_policy(self) -> TerminalShellIntegration {
        if self.inject {
            TerminalShellIntegration::enabled()
        } else {
            TerminalShellIntegration::Disabled
        }
    }
}

/// Optional tmux-compatible environment variables injected when spawning a child process.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalTmuxEnvironmentConfig {
    #[serde(default)]
    pub inject: bool,
    #[serde(default = "default_tmux_socket_path")]
    pub socket_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_pid: Option<u32>,
    #[serde(default)]
    pub session_id: u64,
    #[serde(default)]
    pub pane_id: u64,
    #[serde(default)]
    pub override_term: bool,
}

impl TerminalTmuxEnvironmentConfig {
    /// Validates the pieces that become the comma-delimited `$TMUX` value.
    pub fn validate(&self) -> Result<()> {
        if !self.inject {
            return Ok(());
        }

        ensure!(
            !self.socket_path.trim().is_empty(),
            "tmux socket_path must not be empty when injection is enabled"
        );
        ensure!(
            !self.socket_path.contains(','),
            "tmux socket_path must not contain commas"
        );
        ensure!(
            !self.socket_path.contains('\0'),
            "tmux socket_path must not contain NUL bytes"
        );
        Ok(())
    }

    /// Formats `$TMUX` as `socket_path,pid,session_id`.
    pub fn tmux_env_value(&self) -> String {
        format!(
            "{},{},{}",
            self.socket_path,
            self.server_pid.unwrap_or_else(std::process::id),
            self.session_id
        )
    }

    /// Formats `$TMUX_PANE` using tmux's `%<id>` pane identifier shape.
    pub fn tmux_pane_env_value(&self) -> String {
        format!("%{}", self.pane_id)
    }
}

impl Default for TerminalTmuxEnvironmentConfig {
    fn default() -> Self {
        Self {
            inject: false,
            socket_path: default_tmux_socket_path(),
            server_pid: None,
            session_id: 0,
            pane_id: 0,
            override_term: false,
        }
    }
}

/// Cursor defaults applied before a child process sends DECSCUSR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalCursorConfig {
    #[serde(default)]
    pub default_shape: TerminalCursorShapeConfig,
}

impl Default for TerminalCursorConfig {
    fn default() -> Self {
        Self {
            default_shape: TerminalCursorShapeConfig::Block,
        }
    }
}

/// Serializable cursor shape names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalCursorShapeConfig {
    #[default]
    Block,
    Underline,
    Bar,
}

impl From<TerminalCursorShapeConfig> for TerminalCursorShape {
    fn from(value: TerminalCursorShapeConfig) -> Self {
        match value {
            TerminalCursorShapeConfig::Block => Self::Block,
            TerminalCursorShapeConfig::Underline => Self::Underline,
            TerminalCursorShapeConfig::Bar => Self::Bar,
        }
    }
}

impl From<TerminalCursorShape> for TerminalCursorShapeConfig {
    fn from(value: TerminalCursorShape) -> Self {
        match value {
            TerminalCursorShape::Block => Self::Block,
            TerminalCursorShape::Underline => Self::Underline,
            TerminalCursorShape::Bar => Self::Bar,
        }
    }
}

fn validate_config(config: TerminalConfig) -> Result<TerminalConfig> {
    config.validate()?;
    Ok(config)
}

fn default_scrollback_len() -> usize {
    DEFAULT_TERMINAL_SCROLLBACK_LEN
}

fn default_scroll_step() -> u16 {
    DEFAULT_TERMINAL_SCROLL_STEP
}

fn default_true() -> bool {
    true
}

fn default_profile_name() -> String {
    DEFAULT_TERMINAL_PROFILE_NAME.to_string()
}

fn default_profiles() -> Vec<TerminalProfileConfig> {
    vec![TerminalProfileConfig::shell_from_env()]
}

fn default_tmux_socket_path() -> String {
    "/tmp/atto-ui-tmux.sock".to_string()
}

fn default_prefix_key_config() -> TerminalShortcutConfig {
    TerminalShortcutConfig::control_letter('b')
}

fn default_release_shortcut_config() -> TerminalShortcutConfig {
    TerminalShortcutConfig::new(
        "escape",
        [
            TerminalShortcutModifier::Control,
            TerminalShortcutModifier::Shift,
        ],
    )
}

fn default_alt_scroll_up_key() -> TerminalShortcutConfig {
    TerminalShortcutConfig::new("up", [])
}

fn default_alt_scroll_down_key() -> TerminalShortcutConfig {
    TerminalShortcutConfig::new("down", [])
}

fn default_ansi_palette() -> [TerminalColorSpec; 16] {
    [
        "black".into(),
        "red".into(),
        "green".into(),
        "yellow".into(),
        "blue".into(),
        "magenta".into(),
        "cyan".into(),
        "gray".into(),
        "dark_gray".into(),
        "light_red".into(),
        "light_green".into(),
        "light_yellow".into(),
        "light_blue".into(),
        "light_magenta".into(),
        "light_cyan".into(),
        "white".into(),
    ]
}

fn parse_key_code(key: &str) -> Result<KeyCode> {
    let trimmed = key.trim();
    ensure!(!trimmed.is_empty(), "shortcut key must not be empty");
    let mut chars = trimmed.chars();
    if let Some(ch) = chars.next()
        && chars.next().is_none()
    {
        return Ok(KeyCode::Char(ch));
    }

    let normalized = trimmed.to_ascii_lowercase().replace(['_', '-', ' '], "");
    let code = match normalized.as_str() {
        "backspace" => KeyCode::Backspace,
        "enter" | "return" => KeyCode::Enter,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        "tab" => KeyCode::Tab,
        "backtab" | "shifttab" => KeyCode::BackTab,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "escape" | "esc" => KeyCode::Esc,
        "keypadbegin" => KeyCode::KeypadBegin,
        "null" => KeyCode::Null,
        name if name.starts_with('f') => parse_function_key(name)?,
        _ => bail!("unknown shortcut key {key:?}"),
    };
    Ok(code)
}

fn parse_function_key(name: &str) -> Result<KeyCode> {
    let number = name
        .strip_prefix('f')
        .and_then(|value| value.parse::<u8>().ok())
        .with_context(|| format!("invalid function key {name:?}"))?;
    ensure!(
        (1..=24).contains(&number),
        "function key must be in the range F1..F24"
    );
    Ok(KeyCode::F(number))
}

fn parse_color_spec(input: &str) -> Result<Color> {
    let trimmed = input.trim();
    ensure!(!trimmed.is_empty(), "color spec must not be empty");
    if let Some(hex) = trimmed.strip_prefix('#') {
        return parse_hex_color(hex);
    }
    if let Some(index) = trimmed.strip_prefix("indexed:") {
        let index = index
            .parse::<u8>()
            .with_context(|| format!("invalid indexed color {trimmed:?}"))?;
        return Ok(Color::Indexed(index));
    }

    let name = trimmed.to_ascii_lowercase().replace(['-', ' '], "_");
    let color = match name.as_str() {
        "reset" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "dark_gray" | "darkgrey" | "dark_grey" => Color::DarkGray,
        "lightred" | "light_red" => Color::LightRed,
        "lightgreen" | "light_green" => Color::LightGreen,
        "lightyellow" | "light_yellow" => Color::LightYellow,
        "lightblue" | "light_blue" => Color::LightBlue,
        "lightmagenta" | "light_magenta" => Color::LightMagenta,
        "lightcyan" | "light_cyan" => Color::LightCyan,
        "white" => Color::White,
        _ => bail!("unknown color {input:?}"),
    };
    Ok(color)
}

fn parse_hex_color(hex: &str) -> Result<Color> {
    let bytes = hex.as_bytes();
    let (r, g, b) = match bytes.len() {
        3 => (
            parse_short_hex_channel(bytes[0]).context("parse red channel")?,
            parse_short_hex_channel(bytes[1]).context("parse green channel")?,
            parse_short_hex_channel(bytes[2]).context("parse blue channel")?,
        ),
        6 => (
            parse_hex_channel(bytes[0], bytes[1]).context("parse red channel")?,
            parse_hex_channel(bytes[2], bytes[3]).context("parse green channel")?,
            parse_hex_channel(bytes[4], bytes[5]).context("parse blue channel")?,
        ),
        _ => bail!("expected #RGB or #RRGGBB color"),
    };
    Ok(Color::Rgb(r, g, b))
}

fn parse_short_hex_channel(hex: u8) -> Result<u8> {
    let value = hex_value(hex)?;
    Ok((value << 4) | value)
}

fn parse_hex_channel(high: u8, low: u8) -> Result<u8> {
    Ok((hex_value(high)? << 4) | hex_value(low)?)
}

fn hex_value(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => bail!("invalid hex digit"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_config_defaults_preserve_existing_behavior() {
        let config = TerminalConfig::default();

        assert_eq!(config.scrollback_len, DEFAULT_TERMINAL_SCROLLBACK_LEN);
        assert_eq!(config.palette.foreground_color().unwrap(), None);
        assert_eq!(config.palette.background_color().unwrap(), None);
        assert_eq!(
            config.prefix_shortcut().unwrap(),
            TerminalShortcut::new(KeyCode::Char('b'), KeyModifiers::CONTROL)
        );
        assert_eq!(
            config.release_shortcut().unwrap(),
            TerminalShortcut::new(KeyCode::Esc, KeyModifiers::CONTROL | KeyModifiers::SHIFT)
        );
        assert!(config.alternate_screen_scroll.enabled);
        assert_eq!(
            config.alternate_screen_scroll.step,
            DEFAULT_TERMINAL_SCROLL_STEP
        );
        assert_eq!(
            config
                .alternate_screen_scroll
                .scroll_up_key
                .to_shortcut()
                .unwrap(),
            TerminalShortcut::new(KeyCode::Up, KeyModifiers::NONE)
        );
        assert_eq!(
            config
                .alternate_screen_scroll
                .scroll_down_key
                .to_shortcut()
                .unwrap(),
            TerminalShortcut::new(KeyCode::Down, KeyModifiers::NONE)
        );
        let expected_palette = [
            Color::Black,
            Color::Red,
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::Gray,
            Color::DarkGray,
            Color::LightRed,
            Color::LightGreen,
            Color::LightYellow,
            Color::LightBlue,
            Color::LightMagenta,
            Color::LightCyan,
            Color::White,
        ];
        for (index, expected) in expected_palette.into_iter().enumerate() {
            assert_eq!(
                config.palette.color_for_index(index as u8).unwrap(),
                expected
            );
        }
        assert_eq!(
            config.shell_integration_policy(),
            TerminalShellIntegration::Disabled
        );
        assert!(!config.tmux.inject);
        assert!(!config.tmux.override_term);
        assert!(!config.close_window_on_shell_exit);
        assert_eq!(
            TerminalCursorShape::from(config.cursor.default_shape),
            TerminalCursorShape::Block
        );
        assert_eq!(
            config.sessions.default_profile().unwrap().name,
            DEFAULT_TERMINAL_PROFILE_NAME
        );
        assert!(
            !config
                .sessions
                .default_profile()
                .unwrap()
                .command
                .is_empty()
        );
    }

    #[test]
    fn terminal_config_loads_partial_legacy_files_with_defaults() {
        let minimal = TerminalConfig::from_str("{}", TerminalConfigFormat::Json).unwrap();
        assert_eq!(minimal, TerminalConfig::default());

        let legacy_json = r#"{
            "scrollback_len": 1234,
            "palette": {},
            "alternate_screen_scroll": {},
            "sessions": {},
            "shell_integration": {},
            "cursor": {},
            "future_field": true
        }"#;
        let config = TerminalConfig::from_str(legacy_json, TerminalConfigFormat::Json).unwrap();

        assert_eq!(config.scrollback_len, 1234);
        assert_eq!(
            config.prefix_shortcut().unwrap(),
            TerminalShortcut::new(KeyCode::Char('b'), KeyModifiers::CONTROL)
        );
        assert_eq!(
            config.release_shortcut().unwrap(),
            TerminalShortcut::new(KeyCode::Esc, KeyModifiers::CONTROL | KeyModifiers::SHIFT)
        );
        assert!(config.alternate_screen_scroll.enabled);
        assert_eq!(
            config.alternate_screen_scroll.step,
            DEFAULT_TERMINAL_SCROLL_STEP
        );
        assert_eq!(config.palette.color_for_index(1).unwrap(), Color::Red);
        assert_eq!(
            TerminalCursorShape::from(config.cursor.default_shape),
            TerminalCursorShape::Block
        );
        assert!(!config.close_window_on_shell_exit);
        assert!(!config.tmux.inject);
        assert_eq!(
            config.sessions.default_profile().unwrap().name,
            DEFAULT_TERMINAL_PROFILE_NAME
        );
    }

    #[test]
    fn terminal_config_round_trips_json_and_yaml() {
        let config = TerminalConfig {
            scrollback_len: 4096,
            palette: TerminalPaletteConfig {
                foreground: Some("#f0f".into()),
                background: Some("indexed:235".into()),
                ..Default::default()
            },
            prefix_key: TerminalShortcutConfig::control_letter('a'),
            alternate_screen_scroll: TerminalAlternateScreenScrollConfig {
                step: 5,
                ..Default::default()
            },
            sessions: TerminalSessionsConfig {
                default_profile: "Project".to_string(),
                profiles: vec![
                    TerminalProfileConfig::new(
                        "Project",
                        "/bin/sh",
                        ["-lc".to_string(), "pwd".to_string()],
                    )
                    .with_cwd("/tmp"),
                ],
            },
            shell_integration: TerminalShellIntegrationConfig { inject: true },
            tmux: TerminalTmuxEnvironmentConfig {
                inject: true,
                socket_path: "/tmp/atto-ui-test.sock".to_string(),
                server_pid: Some(4242),
                session_id: 7,
                pane_id: 3,
                override_term: true,
            },
            close_window_on_shell_exit: true,
            cursor: TerminalCursorConfig {
                default_shape: TerminalCursorShapeConfig::Bar,
            },
            ..Default::default()
        };

        let json = config.to_string(TerminalConfigFormat::Json).unwrap();
        let yaml = config.to_string(TerminalConfigFormat::Yaml).unwrap();

        assert_eq!(
            TerminalConfig::from_str(&json, TerminalConfigFormat::Json).unwrap(),
            config
        );
        assert_eq!(
            TerminalConfig::from_str(&yaml, TerminalConfigFormat::Yaml).unwrap(),
            config
        );
        assert_eq!(
            TerminalConfig::from_bytes_infer(json.as_bytes(), Some(Path::new("terminal.json")))
                .unwrap(),
            config
        );
        assert_eq!(
            TerminalConfig::from_bytes_infer(yaml.as_bytes(), Some(Path::new("terminal.yaml")))
                .unwrap(),
            config
        );
    }

    #[test]
    fn terminal_config_rejects_invalid_values() {
        let invalid_prefix = r#"{"prefix_key":{"key":"f10","modifiers":["control"]}}"#;
        assert!(
            TerminalConfig::from_str(invalid_prefix, TerminalConfigFormat::Json)
                .unwrap_err()
                .to_string()
                .contains("prefix_key")
        );

        let invalid_scrollback = r#"{"scrollback_len":0}"#;
        assert!(
            TerminalConfig::from_str(invalid_scrollback, TerminalConfigFormat::Json)
                .unwrap_err()
                .to_string()
                .contains("scrollback_len")
        );

        let invalid_palette = r#"{"palette":{"ansi":["nope","red","green","yellow","blue","magenta","cyan","gray","dark_gray","light_red","light_green","light_yellow","light_blue","light_magenta","light_cyan","white"]}}"#;
        assert!(
            TerminalConfig::from_str(invalid_palette, TerminalConfigFormat::Json)
                .unwrap_err()
                .to_string()
                .contains("palette")
        );

        let invalid_sessions = r#"{"sessions":{"default_profile":"Missing","profiles":[{"name":"Shell","command":"/bin/sh"}]}}"#;
        assert!(
            TerminalConfig::from_str(invalid_sessions, TerminalConfigFormat::Json)
                .unwrap_err()
                .to_string()
                .contains("sessions")
        );

        let invalid_tmux = r#"{"tmux":{"inject":true,"socket_path":"/tmp/with,comma"}}"#;
        assert!(
            TerminalConfig::from_str(invalid_tmux, TerminalConfigFormat::Json)
                .unwrap_err()
                .to_string()
                .contains("tmux")
        );
    }

    #[test]
    fn terminal_profile_config_builds_session_spec() {
        let profile = TerminalProfileConfig::new(
            "Project",
            "/bin/sh",
            ["-lc".to_string(), "echo ok".to_string()],
        )
        .with_cwd("/workspace");

        let spec = profile.to_session_spec();

        assert_eq!(spec.profile(), "Project");
        assert_eq!(spec.program(), "/bin/sh");
        assert_eq!(spec.args(), ["-lc", "echo ok"]);
        assert_eq!(spec.cwd(), Some(Path::new("/workspace")));
    }
}
