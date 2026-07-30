//! Public configuration / value types for the terminal emulator: shortcuts,
//! command-block presentation, cursor shape, shell-integration policy, runtime
//! config, palette, and alternate-screen scroll behavior.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalShortcut {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

/// Visual treatment for OSC 133 command blocks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalCommandBlockPresentation {
    #[default]
    Disabled,
    Enabled,
}

/// Shape used for the synthetic terminal cursor rendered into the Ratatui buffer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalCursorShape {
    #[default]
    Block,
    Underline,
    Bar,
}

impl TerminalCommandBlockPresentation {
    pub const fn enabled() -> Self {
        Self::Enabled
    }

    pub(crate) const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// Spawn-time shell integration policy for emitting OSC 133/7 command markers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalShellIntegration {
    /// Do not mutate spawned shells. User-provided shell integration still works.
    #[default]
    Disabled,
    /// Inject startup snippets for supported interactive shells.
    Enabled,
}

impl TerminalShellIntegration {
    pub const fn enabled() -> Self {
        Self::Enabled
    }

    pub(crate) const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TerminalRuntimeConfig {
    pub(crate) scrollback_len: usize,
    pub(crate) palette: TerminalPalette,
    pub(crate) release_shortcut: TerminalShortcut,
    pub(crate) prefix_shortcut: TerminalShortcut,
    pub(crate) alternate_screen_scroll: TerminalAlternateScreenScroll,
    pub(crate) shell_integration: TerminalShellIntegration,
    pub(crate) tmux_environment: TerminalTmuxEnvironmentConfig,
    pub(crate) cursor_shape: TerminalCursorShape,
}

impl TerminalRuntimeConfig {
    pub(crate) fn from_config(config: &TerminalConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            scrollback_len: config.scrollback_len,
            palette: TerminalPalette::from_config(&config.palette)?,
            release_shortcut: config.release_shortcut()?,
            prefix_shortcut: config.prefix_shortcut()?,
            alternate_screen_scroll: TerminalAlternateScreenScroll::from_config(
                &config.alternate_screen_scroll,
            )?,
            shell_integration: config.shell_integration_policy(),
            tmux_environment: config.tmux.clone(),
            cursor_shape: config.cursor.default_shape.into(),
        })
    }
}

/// Resolved terminal ANSI palette plus optional default fg/bg overrides.
///
/// `foreground`/`background` are `Some` only when explicitly configured; when
/// `None`, rendering falls back to the theme's terminal colors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalPalette {
    pub(crate) foreground: Option<Color>,
    pub(crate) background: Option<Color>,
    pub(crate) ansi: [Color; 16],
}

impl TerminalPalette {
    /// Derives a palette from a theme's [`TerminalTheme`], pinning fg/bg to the
    /// theme's terminal defaults.
    pub fn from_theme(theme: &Theme) -> Self {
        let t = &theme.terminal;
        Self {
            foreground: Some(t.foreground),
            background: Some(t.background),
            ansi: t.ansi,
        }
    }

    pub(crate) fn from_config(config: &TerminalPaletteConfig) -> Result<Self> {
        let ansi = config
            .ansi
            .iter()
            .map(|color| color.to_color())
            .collect::<Result<Vec<_>>>()?
            .try_into()
            .map_err(|_| anyhow!("terminal palette must contain 16 ANSI colors"))?;
        Ok(Self {
            foreground: config.foreground_color()?,
            background: config.background_color()?,
            ansi,
        })
    }

    pub(crate) fn color_for_index(&self, index: u8) -> Color {
        self.ansi
            .get(usize::from(index))
            .copied()
            .unwrap_or(Color::Indexed(index))
    }
}

impl Default for TerminalPalette {
    fn default() -> Self {
        TerminalPalette::from_config(&TerminalPaletteConfig::default())
            .expect("default terminal palette must be valid")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerminalAlternateScreenScroll {
    pub(crate) enabled: bool,
    pub(crate) step: u16,
    pub(crate) scroll_up_key: TerminalShortcut,
    pub(crate) scroll_down_key: TerminalShortcut,
}

impl TerminalAlternateScreenScroll {
    pub(crate) fn from_config(config: &TerminalAlternateScreenScrollConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            enabled: config.enabled,
            step: config.step.max(1),
            scroll_up_key: config.scroll_up_key.to_shortcut()?,
            scroll_down_key: config.scroll_down_key.to_shortcut()?,
        })
    }
}

impl Default for TerminalAlternateScreenScroll {
    fn default() -> Self {
        TerminalAlternateScreenScroll::from_config(&TerminalAlternateScreenScrollConfig::default())
            .expect("default terminal alternate-screen scroll config must be valid")
    }
}

impl TerminalShortcut {
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub(crate) fn matches(&self, event: KeyEvent) -> bool {
        if event.code != self.code {
            match (event.code, self.code) {
                (KeyCode::Char(a), KeyCode::Char(b)) if a.eq_ignore_ascii_case(&b) => {}
                _ => return false,
            }
        }
        if event.kind == KeyEventKind::Release {
            return false;
        }
        event.modifiers == self.modifiers
    }
}
