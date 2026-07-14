use std::collections::VecDeque;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow, bail, ensure};
use base64::Engine;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, ExitStatus, PtySize, native_pty_system};
use ratatui::Frame;
use ratatui::buffer::Cell;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use atto_ui::composable::{
    Capture, ComponentAction, ComponentContext, EventOutcome, EventResult, MouseCoordinateSpace,
    ScrollConfig,
};
use atto_ui::theme::Theme;

use crate::selection::{
    TerminalSelectionPosition, TerminalSelectionRange, TerminalSelectionState,
    position_for_view_cell, selected_cell_ranges_for_screen_row, selected_text_from_screen,
    visible_top_row,
};
use crate::session::TerminalSessionSpec;
use crate::{
    TerminalAlternateScreenScrollConfig, TerminalConfig, TerminalPaletteConfig,
    TerminalTmuxEnvironmentConfig,
};

const DEFAULT_TERM_ENV: &str = "xterm-256color";
const DEFAULT_COLORTERM_ENV: &str = "truecolor";
const TMUX_TERM_ENV: &str = "tmux-256color";
const COMMAND_SEPARATOR_SYMBOL: &str = "─";
const COMMAND_FAILURE_SYMBOL: &str = "!";
const CURSOR_BAR_SYMBOL: &str = "▏";
static SHELL_INTEGRATION_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// Keyboard shortcut used to release terminal input capture.
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

    const fn is_enabled(self) -> bool {
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

    const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalRuntimeConfig {
    scrollback_len: usize,
    palette: TerminalPalette,
    release_shortcut: TerminalShortcut,
    prefix_shortcut: TerminalShortcut,
    alternate_screen_scroll: TerminalAlternateScreenScroll,
    shell_integration: TerminalShellIntegration,
    tmux_environment: TerminalTmuxEnvironmentConfig,
    cursor_shape: TerminalCursorShape,
}

impl TerminalRuntimeConfig {
    fn from_config(config: &TerminalConfig) -> Result<Self> {
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalPalette {
    foreground: Option<Color>,
    background: Option<Color>,
    ansi: [Color; 16],
}

impl TerminalPalette {
    fn from_config(config: &TerminalPaletteConfig) -> Result<Self> {
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

    fn color_for_index(&self, index: u8) -> Color {
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
struct TerminalAlternateScreenScroll {
    enabled: bool,
    step: u16,
    scroll_up_key: TerminalShortcut,
    scroll_down_key: TerminalShortcut,
}

impl TerminalAlternateScreenScroll {
    fn from_config(config: &TerminalAlternateScreenScrollConfig) -> Result<Self> {
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

    fn matches(&self, event: KeyEvent) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_shared() -> TerminalShared {
        let runtime_config = TerminalRuntimeConfig::from_config(&TerminalConfig::default())
            .expect("default terminal config");
        TerminalShared {
            parser: terminal_parser(24, 80, runtime_config.scrollback_len),
            scrollback_len: runtime_config.scrollback_len,
            palette: runtime_config.palette,
            alternate_screen_scroll: runtime_config.alternate_screen_scroll,
            input: VecDeque::new(),
            on_input: None,
            input_forward: None,
            on_exit: None,
            on_window_title: None,
            on_window_icon_name: None,
            on_audible_bell: None,
            on_clipboard_copy: None,
            on_command_finished: None,
            system_clipboard: None,
            pty_resize: None,
            shell_integration: runtime_config.shell_integration,
            tmux_environment: runtime_config.tmux_environment,
            last_shell_integration_error: None,
            exit_status: None,
            process_running: false,
            window_title: None,
            window_icon_name: None,
            audible_bell_count: 0,
            last_clipboard_copy: None,
            last_system_clipboard_text: None,
            last_system_clipboard_error: None,
            capture: true,
            capture_suspended_by_blur: false,
            release_shortcut: runtime_config.release_shortcut,
            prefix_shortcut: runtime_config.prefix_shortcut,
            prefix_bindings: default_prefix_bindings(),
            prefix_pending: false,
            copy_mode: None,
            copy_buffer: None,
            selection: TerminalSelectionState::default(),
            command_marks: Vec::new(),
            current_cwd: None,
            cursor_shape: TerminalCursorShape::default(),
            dsr_tail: Vec::new(),
            tmux_dcs_passthrough: TmuxDcsPassthroughDecoder::default(),
        }
    }

    fn mouse_at(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn mouse_coords_local_uses_explicit_coordinate_space() {
        let area = Some(Rect::new(10, 5, 4, 3));

        assert_eq!(
            mouse_coords_local(area, mouse_at(11, 6), MouseCoordinateSpace::Absolute),
            Some((1, 1))
        );
        assert_eq!(
            mouse_coords_local(area, mouse_at(1, 1), MouseCoordinateSpace::Absolute),
            None
        );
        assert_eq!(
            mouse_coords_local(area, mouse_at(1, 1), MouseCoordinateSpace::Local),
            Some((1, 1))
        );
        assert_eq!(
            mouse_coords_local(area, mouse_at(11, 6), MouseCoordinateSpace::Local),
            None
        );
    }

    #[test]
    fn dsr_responses_handle_split_packets() {
        let mut shared = test_shared();

        assert!(collect_dsr_responses(&mut shared, b"\x1b[?6").is_empty());
        let responses = collect_dsr_responses(&mut shared, b"n");
        assert_eq!(responses, vec![b"\x1b[?1;1R".to_vec()]);

        assert!(collect_dsr_responses(&mut shared, b"\x1b[?").is_empty());
        let responses = collect_dsr_responses(&mut shared, b"5n");
        assert_eq!(responses, vec![b"\x1b[?0n".to_vec()]);
    }

    #[test]
    fn dsr_complete_packets_do_not_repeat_on_later_output() {
        let mut shared = test_shared();

        let responses = collect_dsr_responses(&mut shared, b"\x1b[6n");
        assert_eq!(responses, vec![b"\x1b[1;1R".to_vec()]);

        assert!(collect_dsr_responses(&mut shared, b"x").is_empty());
        assert!(shared.dsr_tail.is_empty());
    }

    #[test]
    fn device_attribute_and_keyboard_queries_are_answered() {
        let mut shared = test_shared();

        // DA1: both `CSI c` and `CSI 0 c` forms.
        assert_eq!(
            collect_dsr_responses(&mut shared, b"\x1b[c"),
            vec![b"\x1b[?62c".to_vec()]
        );
        assert_eq!(
            collect_dsr_responses(&mut shared, b"\x1b[0c"),
            vec![b"\x1b[?62c".to_vec()]
        );
        // DA2.
        assert_eq!(
            collect_dsr_responses(&mut shared, b"\x1b[>c"),
            vec![b"\x1b[>0;0;0c".to_vec()]
        );
        // Kitty keyboard-protocol flags query.
        assert_eq!(
            collect_dsr_responses(&mut shared, b"\x1b[?u"),
            vec![b"\x1b[?0u".to_vec()]
        );
    }

    #[test]
    fn neovim_startup_query_batch_is_fully_answered() {
        let mut shared = test_shared();

        // The exact batch Neovim emits on startup (kitty flags, DA1, OSC 11
        // background query, DSR status). We must answer the keyboard query,
        // DA1, and DSR — OSC 11 is not a CSI query and is left to the parser.
        let responses = collect_dsr_responses(&mut shared, b"\x1b[?u\x1b[c\x1b]11;?\x07\x1b[5n");
        assert_eq!(
            responses,
            vec![
                b"\x1b[?0u".to_vec(),
                b"\x1b[?62c".to_vec(),
                b"\x1b[0n".to_vec(),
            ]
        );
    }

    #[test]
    fn da_query_split_across_chunks_is_buffered() {
        let mut shared = test_shared();

        assert!(collect_dsr_responses(&mut shared, b"\x1b[").is_empty());
        assert_eq!(
            collect_dsr_responses(&mut shared, b"c"),
            vec![b"\x1b[?62c".to_vec()]
        );
    }

    #[test]
    fn osc133_and_osc7_record_command_block_marks() {
        let terminal = TerminalEmulator::new();
        let handle = terminal.handle();

        handle.process_output_str(
            "\x1b]7;file://host/tmp/project%20one\x07\
             \x1b]133;A\x07$ echo ok\
             \x1b]133;B\x07\r\n\
             \x1b]133;C\x07ok\r\n\
             \x1b]133;D;7\x07",
        );

        let shared = terminal.shared.lock();
        assert_eq!(shared.current_cwd.as_deref(), Some("/tmp/project one"));
        assert_eq!(shared.command_marks.len(), 1);
        assert_eq!(
            shared.command_marks[0],
            TerminalCommandBlock {
                prompt_start: Some(0),
                prompt_start_col: Some(0),
                command_start: Some(0),
                command_start_col: Some(9),
                output_start: Some(1),
                output_start_col: Some(0),
                end: Some(2),
                end_col: Some(0),
                exit_code: Some(7),
                cwd: Some("/tmp/project one".to_string()),
            }
        );
    }

    #[test]
    fn osc133_zsh_preexec_order_still_captures_command_text() {
        // Real zsh/bash shell integration emits B (command-start) and C
        // (output-start) together from preexec/PS0, i.e. AFTER the user pressed
        // Enter — so both land at the output position, not inline after the
        // typed command. The typed command lives on the prompt line between A
        // and the newline. Copy command / Rerun must still recover it.
        let terminal = TerminalEmulator::new();
        let handle = terminal.handle();

        handle.process_output_str(
            "\x1b]133;A\x07$ echo hi\r\n\
             \x1b]133;B\x07\x1b]133;C\x07hi\r\n\
             \x1b]133;D;0\x07",
        );

        let command = handle
            .copy_command_block_command(0)
            .expect("command text recoverable");
        assert_eq!(command, "$ echo hi");

        let output = handle
            .copy_command_block_output(0)
            .expect("output text recoverable");
        assert_eq!(output, "hi");
    }

    #[test]
    fn osc133_missing_markers_degrade_to_partial_blocks() {
        let terminal = TerminalEmulator::new();
        let handle = terminal.handle();

        handle.process_output_str("plain output\r\n\x1b]133;D;0\x07");

        let shared = terminal.shared.lock();
        assert_eq!(shared.command_marks.len(), 1);
        assert_eq!(
            shared.command_marks[0],
            TerminalCommandBlock {
                prompt_start: Some(1),
                prompt_start_col: Some(0),
                end: Some(1),
                end_col: Some(0),
                exit_code: Some(0),
                ..TerminalCommandBlock::default()
            }
        );
    }

    #[test]
    fn osc7_decodes_file_uri_paths() {
        assert_eq!(
            parse_osc7_cwd(b"file://localhost/Users/test/project%20one").as_deref(),
            Some("/Users/test/project one")
        );
        assert_eq!(
            parse_osc7_cwd(b"file:///tmp/%E4%BD%A0%E5%A5%BD").as_deref(),
            Some("/tmp/你好")
        );
        assert_eq!(parse_osc7_cwd(b"https://example.invalid/tmp"), None);
    }

    #[test]
    fn shell_integration_defaults_to_zero_intrusion() {
        let terminal = TerminalEmulator::new();
        let handle = terminal.handle();
        let mut cmd = CommandBuilder::new("/bin/bash");

        let files = prepare_shell_integration(&mut cmd, handle.shell_integration())
            .expect("disabled shell integration is infallible");

        assert_eq!(
            handle.shell_integration(),
            TerminalShellIntegration::Disabled
        );
        assert!(files.is_none());
        assert_eq!(cmd.get_argv().as_slice(), [OsString::from("/bin/bash")]);
    }

    #[test]
    fn shell_integration_wraps_interactive_bash() {
        let mut cmd = CommandBuilder::new("/bin/bash");

        let files = prepare_shell_integration(&mut cmd, TerminalShellIntegration::enabled())
            .expect("prepare integration")
            .expect("bash integration files");

        assert_eq!(
            cmd.get_argv().as_slice(),
            [
                OsString::from("/bin/bash"),
                OsString::from("--rcfile"),
                files.entrypoint().as_os_str().to_os_string(),
                OsString::from("-i")
            ]
        );
        assert_eq!(
            cmd.get_env("ATTO_UI_SHELL_INTEGRATION"),
            Some(OsStr::new("1"))
        );
        assert!(
            fs::read_to_string(files.entrypoint())
                .expect("read bash integration script")
                .contains("OSC 133/7")
        );

        let root = files.root().to_path_buf();
        drop(files);
        assert!(!root.exists());
    }

    #[test]
    fn shell_integration_leaves_non_interactive_shell_commands_unchanged() {
        let mut cmd = CommandBuilder::new("/bin/bash");
        cmd.arg("-c");
        cmd.arg("echo unchanged");

        let files = prepare_shell_integration(&mut cmd, TerminalShellIntegration::enabled())
            .expect("prepare integration");

        assert!(files.is_none());
        assert_eq!(
            cmd.get_argv().as_slice(),
            [
                OsString::from("/bin/bash"),
                OsString::from("-c"),
                OsString::from("echo unchanged")
            ]
        );
        assert_eq!(cmd.get_env("ATTO_UI_SHELL_INTEGRATION"), None);
    }

    #[test]
    fn shell_integration_mode_is_queryable_and_mutable() {
        let terminal =
            TerminalEmulator::new().shell_integration(TerminalShellIntegration::enabled());
        let handle = terminal.handle();

        assert_eq!(
            handle.shell_integration(),
            TerminalShellIntegration::enabled()
        );
        handle.set_shell_integration(TerminalShellIntegration::Disabled);
        assert_eq!(
            handle.shell_integration(),
            TerminalShellIntegration::Disabled
        );
        assert_eq!(handle.last_shell_integration_error(), None);
    }

    #[test]
    fn tmux_environment_mode_is_queryable_and_mutable() {
        let terminal = TerminalEmulator::new().tmux_environment(TerminalTmuxEnvironmentConfig {
            inject: true,
            socket_path: "/tmp/atto-ui-builder.sock".to_string(),
            server_pid: Some(1111),
            session_id: 2,
            pane_id: 4,
            override_term: false,
        });
        let handle = terminal.handle();

        assert_eq!(
            handle.tmux_environment().tmux_env_value(),
            "/tmp/atto-ui-builder.sock,1111,2"
        );
        handle.set_tmux_environment(TerminalTmuxEnvironmentConfig::default());
        assert!(!handle.tmux_environment().inject);
    }

    #[test]
    fn spawn_command_preparation_sets_terminal_env_and_default_cwd() {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.env("TERM", "host-term");
        cmd.env("COLORTERM", "host-colorterm");
        cmd.env_remove("TMUX");
        cmd.env_remove("TMUX_PANE");

        prepare_spawn_command(&mut cmd, &TerminalTmuxEnvironmentConfig::default())
            .expect("prepare spawn command");

        assert_eq!(cmd.get_env("TERM"), Some(OsStr::new(DEFAULT_TERM_ENV)));
        assert_eq!(
            cmd.get_env("COLORTERM"),
            Some(OsStr::new(DEFAULT_COLORTERM_ENV))
        );
        assert_eq!(cmd.get_env("TMUX"), None);
        assert_eq!(cmd.get_env("TMUX_PANE"), None);
        assert_eq!(
            cmd.get_cwd().and_then(|cwd| cwd.to_str()),
            env::current_dir().expect("current dir").as_path().to_str()
        );
    }

    #[test]
    fn spawn_command_preparation_injects_tmux_probe_environment_when_enabled() {
        let mut cmd = CommandBuilder::new("/bin/sh");
        let tmux = TerminalTmuxEnvironmentConfig {
            inject: true,
            socket_path: "/tmp/atto-ui-test.sock".to_string(),
            server_pid: Some(4242),
            session_id: 7,
            pane_id: 3,
            override_term: false,
        };

        prepare_spawn_command(&mut cmd, &tmux).expect("prepare spawn command");

        assert_eq!(cmd.get_env("TERM"), Some(OsStr::new(DEFAULT_TERM_ENV)));
        assert_eq!(
            cmd.get_env("TMUX"),
            Some(OsStr::new("/tmp/atto-ui-test.sock,4242,7"))
        );
        assert_eq!(cmd.get_env("TMUX_PANE"), Some(OsStr::new("%3")));
    }

    #[test]
    fn spawn_command_preparation_can_use_tmux_term_when_enabled() {
        let mut cmd = CommandBuilder::new("/bin/sh");
        let tmux = TerminalTmuxEnvironmentConfig {
            inject: true,
            override_term: true,
            ..TerminalTmuxEnvironmentConfig::default()
        };

        prepare_spawn_command(&mut cmd, &tmux).expect("prepare spawn command");

        assert_eq!(cmd.get_env("TERM"), Some(OsStr::new(TMUX_TERM_ENV)));
        assert_eq!(cmd.get_env("TMUX_PANE"), Some(OsStr::new("%0")));
    }

    #[test]
    fn spawn_command_preparation_preserves_explicit_cwd() {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.cwd(OsStr::new("/tmp"));

        prepare_spawn_command(&mut cmd, &TerminalTmuxEnvironmentConfig::default())
            .expect("prepare spawn command");

        assert_eq!(cmd.get_cwd().and_then(|cwd| cwd.to_str()), Some("/tmp"));
    }

    #[test]
    fn system_clipboard_backend_uses_arboard_when_osc52_fails() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let osc52_calls = Arc::clone(&calls);
        let arboard_calls = Arc::clone(&calls);

        copy_text_with_backends(
            "copy me",
            move |text| {
                osc52_calls.lock().push(format!("osc52:{text}"));
                Err(anyhow!("osc52 failed"))
            },
            move |text| {
                arboard_calls.lock().push(format!("arboard:{text}"));
                Ok(())
            },
        )
        .expect("arboard fallback should succeed");

        assert_eq!(
            calls.lock().as_slice(),
            ["osc52:copy me", "arboard:copy me"]
        );
    }

    #[test]
    fn system_clipboard_backend_reports_when_both_paths_fail() {
        let error = copy_text_with_backends(
            "copy me",
            |_| Err(anyhow!("osc52 failed")),
            |_| Err(anyhow!("arboard failed")),
        )
        .expect_err("both backends should fail")
        .to_string();

        assert!(error.contains("osc52 failed"));
        assert!(error.contains("arboard failed"));
    }
}

fn default_prefix_bindings() -> Vec<TerminalPrefixBinding> {
    vec![
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::F(10), KeyModifiers::NONE),
            TerminalPrefixCommand::ActivateMenu,
        ),
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::Char('w'), KeyModifiers::NONE),
            TerminalPrefixCommand::ToggleWindowManagement,
        ),
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::Char('z'), KeyModifiers::NONE),
            TerminalPrefixCommand::ToggleMaximize,
        ),
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::Char('['), KeyModifiers::NONE),
            TerminalPrefixCommand::EnterCopyMode,
        ),
        TerminalPrefixBinding::new(
            TerminalShortcut::new(KeyCode::Char(']'), KeyModifiers::NONE),
            TerminalPrefixCommand::PasteCopyBuffer,
        ),
    ]
}

fn prefix_shortcut_from_letter(letter: char) -> Result<TerminalShortcut> {
    normalize_prefix_shortcut(TerminalShortcut::new(
        KeyCode::Char(letter),
        KeyModifiers::CONTROL,
    ))
}

fn normalize_prefix_shortcut(shortcut: TerminalShortcut) -> Result<TerminalShortcut> {
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
    Ok(TerminalShortcut {
        code: KeyCode::Char(letter.to_ascii_lowercase()),
        modifiers: KeyModifiers::CONTROL,
    })
}

fn normalize_prefix_binding_shortcut(shortcut: TerminalShortcut) -> TerminalShortcut {
    let code = match shortcut.code {
        KeyCode::Char(letter) => KeyCode::Char(letter.to_ascii_lowercase()),
        code => code,
    };
    TerminalShortcut {
        code,
        modifiers: shortcut.modifiers,
    }
}

type InputCallback = Arc<dyn Fn(&[u8]) + Send + Sync>;
type ExitCallback = Arc<dyn Fn(ExitStatus) + Send + Sync>;
type TextCallback = Arc<dyn Fn(&str) + Send + Sync>;
type BellCallback = Arc<dyn Fn() + Send + Sync>;
type ClipboardCopyCallback = Arc<dyn Fn(&TerminalClipboardCopy) + Send + Sync>;
type CommandFinishedCallback = Arc<dyn Fn(&TerminalCommandBlock) + Send + Sync>;
type SystemClipboard = Arc<dyn TerminalSystemClipboard>;
type TerminalParser = vt100::Parser<TerminalCallbacks>;
const TMUX_DCS_PREFIX: &[u8] = b"tmux;";
const TMUX_DCS_MAX_BUFFERED: usize = 1024 * 1024;

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
struct DefaultTerminalSystemClipboard;

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

#[derive(Default)]
struct TmuxDcsPassthroughDecoder {
    state: TmuxDcsPassthroughState,
}

#[derive(Default)]
enum TmuxDcsPassthroughState {
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
    fn decode(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            self.push_byte(byte, &mut output);
        }
        output
    }

    fn push_byte(&mut self, byte: u8, output: &mut Vec<u8>) {
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

    fn push_dcs_prefix_byte(&mut self, mut raw: Vec<u8>, matched: usize, byte: u8) {
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

    fn push_ignored_dcs_byte(&mut self, pending_esc: bool, byte: u8) {
        self.state = if pending_esc && byte == b'\\' {
            TmuxDcsPassthroughState::Ground
        } else {
            TmuxDcsPassthroughState::IgnoredDcs {
                pending_esc: byte == 0x1b,
            }
        };
    }

    fn push_tmux_body_byte(
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

    fn buffered_len(&self) -> usize {
        match &self.state {
            TmuxDcsPassthroughState::Ground => 0,
            TmuxDcsPassthroughState::Esc => 1,
            TmuxDcsPassthroughState::IgnoredDcs { .. } => 0,
            TmuxDcsPassthroughState::DcsPrefix { raw, .. }
            | TmuxDcsPassthroughState::TmuxBody { raw, .. } => raw.len(),
        }
    }

    fn drop_pending_control_string(&mut self) {
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

fn unescape_tmux_dcs_body(body: &[u8]) -> Option<Vec<u8>> {
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

/// Command selected by the terminal prefix key table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalPrefixCommand {
    ActivateMenu,
    ToggleWindowManagement,
    ToggleMaximize,
    EnterCopyMode,
    PasteCopyBuffer,
    SendPrefix,
}

/// One configurable binding in the prefix command table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalPrefixBinding {
    pub shortcut: TerminalShortcut,
    pub command: TerminalPrefixCommand,
}

impl TerminalPrefixBinding {
    pub fn new(shortcut: TerminalShortcut, command: TerminalPrefixCommand) -> Self {
        Self {
            shortcut: normalize_prefix_binding_shortcut(shortcut),
            command,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalCopyModeState {
    cursor: TerminalSelectionPosition,
    selecting: bool,
}

impl TerminalCopyModeState {
    fn new(cursor: TerminalSelectionPosition) -> Self {
        Self {
            cursor,
            selecting: false,
        }
    }
}

/// OSC 133/7 command block markers observed in terminal output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalCommandBlock {
    /// Absolute terminal row where the prompt started.
    pub prompt_start: Option<usize>,
    /// Terminal column where the prompt-start marker was observed.
    pub prompt_start_col: Option<u16>,
    /// Absolute terminal row where the command text started.
    pub command_start: Option<usize>,
    /// Terminal column where the command-start marker was observed.
    pub command_start_col: Option<u16>,
    /// Absolute terminal row where command output started.
    pub output_start: Option<usize>,
    /// Terminal column where the output-start marker was observed.
    pub output_start_col: Option<u16>,
    /// Absolute terminal row where the command finished.
    pub end: Option<usize>,
    /// Terminal column where the command-finished marker was observed.
    pub end_col: Option<u16>,
    /// Command-level exit code reported by OSC 133 `D`, if present.
    pub exit_code: Option<i32>,
    /// Current working directory reported by OSC 7 for this block.
    pub cwd: Option<String>,
}

impl TerminalCommandBlock {
    fn at_prompt(row: usize, col: u16, cwd: Option<String>) -> Self {
        Self {
            prompt_start: Some(row),
            prompt_start_col: Some(col),
            cwd,
            ..Self::default()
        }
    }

    fn is_open(&self) -> bool {
        self.end.is_none()
    }

    fn has_command_activity(&self) -> bool {
        self.command_start.is_some() || self.output_start.is_some()
    }

    fn anchor_row(&self) -> Option<usize> {
        self.prompt_start
            .or(self.command_start)
            .or(self.output_start)
            .or(self.end)
    }

    fn last_row(&self) -> Option<usize> {
        [
            self.prompt_start,
            self.command_start,
            self.output_start,
            self.end,
        ]
        .into_iter()
        .flatten()
        .max()
    }

    fn contains_row(&self, row: usize) -> bool {
        let Some(start) = self.anchor_row() else {
            return false;
        };
        let end = self.last_row().unwrap_or(start);
        row >= start && row <= end
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalCallbackEvent {
    WindowTitle(String),
    WindowIconName(String),
    AudibleBell,
    ClipboardCopy(TerminalClipboardCopy),
    UnhandledOsc {
        params: Vec<Vec<u8>>,
        row: usize,
        col: u16,
    },
    CursorShape(TerminalCursorShape),
}

#[derive(Default)]
struct TerminalCallbacks {
    events: Vec<TerminalCallbackEvent>,
}

impl TerminalCallbacks {
    fn take_events(&mut self) -> Vec<TerminalCallbackEvent> {
        std::mem::take(&mut self.events)
    }
}

impl vt100::Callbacks for TerminalCallbacks {
    fn audible_bell(&mut self, _: &mut vt100::Screen) {
        self.events.push(TerminalCallbackEvent::AudibleBell);
    }

    fn set_window_icon_name(&mut self, _: &mut vt100::Screen, icon_name: &[u8]) {
        self.events.push(TerminalCallbackEvent::WindowIconName(
            string_from_terminal_bytes(icon_name),
        ));
    }

    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.events.push(TerminalCallbackEvent::WindowTitle(
            string_from_terminal_bytes(title),
        ));
    }

    fn copy_to_clipboard(&mut self, _: &mut vt100::Screen, selector: &[u8], data: &[u8]) {
        self.events.push(TerminalCallbackEvent::ClipboardCopy(
            TerminalClipboardCopy {
                selector: selector.to_vec(),
                data: data.to_vec(),
            },
        ));
    }

    fn unhandled_osc(&mut self, screen: &mut vt100::Screen, params: &[&[u8]]) {
        let (row, col) = current_absolute_position_for_screen(screen);
        self.events.push(TerminalCallbackEvent::UnhandledOsc {
            params: params.iter().map(|param| param.to_vec()).collect(),
            row,
            col,
        });
    }

    fn unhandled_csi(
        &mut self,
        _: &mut vt100::Screen,
        i1: Option<u8>,
        i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        if let Some(shape) = parse_decscusr_cursor_shape(i1, i2, params, c) {
            self.events.push(TerminalCallbackEvent::CursorShape(shape));
        }
    }
}

enum TerminalCallbackDispatch {
    WindowTitle(TextCallback, String),
    WindowIconName(TextCallback, String),
    AudibleBell(BellCallback),
    ClipboardCopy(ClipboardCopyCallback, TerminalClipboardCopy),
    CommandFinished(CommandFinishedCallback, TerminalCommandBlock),
    SystemClipboardCopy(String),
}

fn terminal_parser(rows: u16, cols: u16, scrollback_len: usize) -> TerminalParser {
    vt100::Parser::new_with_callbacks(rows, cols, scrollback_len, TerminalCallbacks::default())
}

fn current_absolute_position_for_screen(screen: &mut vt100::Screen) -> (usize, u16) {
    let current_scrollback = screen.scrollback();
    screen.set_scrollback(usize::MAX);
    let max_scrollback = screen.scrollback();
    screen.set_scrollback(current_scrollback);
    let (row, col) = screen.cursor_position();
    (max_scrollback.saturating_add(usize::from(row)), col)
}

fn string_from_terminal_bytes(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn parse_osc133_exit_code(bytes: &[u8]) -> Option<i32> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

fn parse_decscusr_cursor_shape(
    i1: Option<u8>,
    i2: Option<u8>,
    params: &[&[u16]],
    c: char,
) -> Option<TerminalCursorShape> {
    if i1 != Some(b' ') || i2.is_some() || c != 'q' {
        return None;
    }
    let style = params
        .first()
        .and_then(|param| param.first())
        .copied()
        .unwrap_or(0);
    match style {
        0..=2 => Some(TerminalCursorShape::Block),
        3 | 4 => Some(TerminalCursorShape::Underline),
        5 | 6 => Some(TerminalCursorShape::Bar),
        _ => None,
    }
}

fn parse_osc7_cwd(bytes: &[u8]) -> Option<String> {
    let uri = std::str::from_utf8(bytes).ok()?;
    let rest = uri.strip_prefix("file://")?;
    let path = if rest.starts_with('/') {
        rest
    } else {
        rest.get(rest.find('/')?..)?
    };
    if path.is_empty() {
        return None;
    }
    Some(percent_decode_uri_path(path))
}

fn percent_decode_uri_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            decoded.push((hi << 4) | lo);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn encode_paste_text(screen: &vt100::Screen, text: &str) -> Vec<u8> {
    if screen.bracketed_paste() {
        let mut buf = Vec::with_capacity(text.len() + 16);
        buf.extend_from_slice(b"\x1b[200~");
        buf.extend_from_slice(text.as_bytes());
        buf.extend_from_slice(b"\x1b[201~");
        buf
    } else {
        text.as_bytes().to_vec()
    }
}

fn copy_text_with_arboard(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(text.to_owned())?;
    Ok(())
}

fn copy_text_with_backends<O, A>(text: &str, osc52: O, arboard: A) -> Result<()>
where
    O: FnOnce(&str) -> Result<()>,
    A: FnOnce(&str) -> Result<()>,
{
    let osc52_result = osc52(text);
    let arboard_result = arboard(text);
    if osc52_result.is_ok() || arboard_result.is_ok() {
        return Ok(());
    }

    let osc52_error = osc52_result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_else(|| "unknown OSC 52 error".to_string());
    let arboard_error = arboard_result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_else(|| "unknown arboard error".to_string());
    Err(anyhow!(
        "failed to copy text to system clipboard via OSC 52 ({osc52_error}) or arboard ({arboard_error})"
    ))
}

const BASH_SHELL_INTEGRATION_SCRIPT: &str = r#"# atto-ui OSC 133/7 shell integration for bash.
if [ -z "${ATTO_UI_SHELL_INTEGRATION_NO_USER_RC:-}" ] && [ -r "${HOME:-}/.bashrc" ]; then
  . "${HOME}/.bashrc"
fi

__atto_ui_emit_cwd() {
  printf '\033]7;file://%s%s\a' "${HOSTNAME:-localhost}" "${PWD}"
}

__atto_ui_precmd() {
  local __atto_ui_status=$?
  if [ "${__atto_ui_prompt_seen:-0}" = 1 ]; then
    printf '\033]133;D;%s\a' "${__atto_ui_status}"
  fi
  __atto_ui_prompt_seen=1
  __atto_ui_emit_cwd
  printf '\033]133;A\a'
  # Mark command-start (OSC 133 B) at the end of the prompt so it lands right
  # before the user's typed command, not on the output line. Appended
  # idempotently (this runs last in PROMPT_COMMAND) so prompt rebuilds keep it.
  case "$PS1" in
    *$'\[\033]133;B\a\]'*) ;;
    *) PS1="${PS1}"$'\[\033]133;B\a\]' ;;
  esac
}

# Output-start (OSC 133 C) is emitted after the user submits the command.
PS0=$'\[\033]133;C\a\]'
if [ -n "${PROMPT_COMMAND:-}" ]; then
  PROMPT_COMMAND="${PROMPT_COMMAND}; __atto_ui_precmd"
else
  PROMPT_COMMAND="__atto_ui_precmd"
fi
"#;

const ZSH_SHELL_INTEGRATION_SCRIPT: &str = r#"# atto-ui OSC 133/7 shell integration for zsh.
if [ -z "${ATTO_UI_SHELL_INTEGRATION_NO_USER_RC:-}" ] && [ -n "${ATTO_UI_ORIGINAL_ZDOTDIR:-}" ] && [ -r "${ATTO_UI_ORIGINAL_ZDOTDIR}/.zshrc" ]; then
  . "${ATTO_UI_ORIGINAL_ZDOTDIR}/.zshrc"
fi

__atto_ui_emit_cwd() {
  printf '\033]7;file://%s%s\a' "${HOST:-${HOSTNAME:-localhost}}" "${PWD}"
}

__atto_ui_precmd() {
  local __atto_ui_status=$?
  if [[ "${__atto_ui_prompt_seen:-0}" == 1 ]]; then
    printf '\033]133;D;%d\a' "${__atto_ui_status}"
  fi
  __atto_ui_prompt_seen=1
  __atto_ui_emit_cwd
  printf '\033]133;A\a'
  # Mark command-start (OSC 133 B) at the very end of the prompt so it lands
  # right before the user's typed command, not on the output line. Appended
  # idempotently after any user/framework precmd (this hook is registered last)
  # so prompt rebuilds keep the mark.
  case "$PROMPT" in
    *$'\033]133;B\a'*) ;;
    *) PROMPT="${PROMPT}"$'%{\033]133;B\a%}' ;;
  esac
}

__atto_ui_preexec() {
  printf '\033]133;C\a'
}

autoload -Uz add-zsh-hook
add-zsh-hook precmd __atto_ui_precmd
add-zsh-hook preexec __atto_ui_preexec
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShellIntegrationKind {
    Bash,
    Zsh,
}

#[derive(Debug)]
struct TerminalShellIntegrationFiles {
    root: PathBuf,
    entrypoint: PathBuf,
}

impl TerminalShellIntegrationFiles {
    fn create(kind: ShellIntegrationKind) -> Result<Self> {
        let root = create_shell_integration_temp_dir()?;
        let (entrypoint, script) = match kind {
            ShellIntegrationKind::Bash => (root.join("bashrc"), BASH_SHELL_INTEGRATION_SCRIPT),
            ShellIntegrationKind::Zsh => (root.join(".zshrc"), ZSH_SHELL_INTEGRATION_SCRIPT),
        };
        fs::write(&entrypoint, script)?;
        Ok(Self { root, entrypoint })
    }

    fn entrypoint(&self) -> &Path {
        &self.entrypoint
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for TerminalShellIntegrationFiles {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.entrypoint);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn create_shell_integration_temp_dir() -> Result<PathBuf> {
    for _ in 0..16 {
        let id = SHELL_INTEGRATION_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            env::temp_dir().join(format!("atto-ui-shell-integration-{}-{id}", process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    bail!("failed to allocate a unique shell integration temp directory")
}

fn prepare_spawn_command(
    cmd: &mut CommandBuilder,
    tmux_environment: &TerminalTmuxEnvironmentConfig,
) -> Result<()> {
    tmux_environment.validate()?;
    let term = if tmux_environment.inject && tmux_environment.override_term {
        TMUX_TERM_ENV
    } else {
        DEFAULT_TERM_ENV
    };

    cmd.env("TERM", term);
    cmd.env("COLORTERM", DEFAULT_COLORTERM_ENV);
    if tmux_environment.inject {
        cmd.env("TMUX", tmux_environment.tmux_env_value());
        cmd.env("TMUX_PANE", tmux_environment.tmux_pane_env_value());
    }
    if cmd.get_cwd().is_none() {
        let cwd = env::current_dir()?;
        cmd.cwd(cwd.as_os_str());
    }
    Ok(())
}

fn prepare_shell_integration(
    cmd: &mut CommandBuilder,
    integration: TerminalShellIntegration,
) -> Result<Option<TerminalShellIntegrationFiles>> {
    if !integration.is_enabled() {
        return Ok(None);
    }

    let argv = cmd.get_argv();
    let Some(program) = argv.first() else {
        return Ok(None);
    };
    let Some(kind) = shell_integration_kind(program) else {
        return Ok(None);
    };
    if !shell_integration_accepts_args(argv) {
        return Ok(None);
    }

    let files = TerminalShellIntegrationFiles::create(kind)?;
    match kind {
        ShellIntegrationKind::Bash => {
            let program = cmd
                .get_argv()
                .first()
                .cloned()
                .expect("program exists for bash shell integration");
            let argv = cmd.get_argv_mut();
            argv.clear();
            argv.push(program);
            argv.push(OsString::from("--rcfile"));
            argv.push(files.entrypoint().as_os_str().to_os_string());
            argv.push(OsString::from("-i"));
        }
        ShellIntegrationKind::Zsh => {
            if let Some(original_zdotdir) = cmd
                .get_env("ZDOTDIR")
                .map(OsStr::to_os_string)
                .or_else(|| env::var_os("ZDOTDIR"))
                .or_else(|| env::var_os("HOME"))
            {
                cmd.env("ATTO_UI_ORIGINAL_ZDOTDIR", original_zdotdir);
            }
            cmd.env("ZDOTDIR", files.root().as_os_str());
            ensure_interactive_shell_arg(cmd);
        }
    }
    cmd.env("ATTO_UI_SHELL_INTEGRATION", "1");
    Ok(Some(files))
}

fn shell_integration_kind(program: &OsStr) -> Option<ShellIntegrationKind> {
    let name = Path::new(program).file_name()?.to_string_lossy();
    let name = name.strip_prefix('-').unwrap_or(&name);
    match name {
        "bash" => Some(ShellIntegrationKind::Bash),
        "zsh" => Some(ShellIntegrationKind::Zsh),
        _ => None,
    }
}

fn shell_integration_accepts_args(argv: &[OsString]) -> bool {
    match &argv[1..] {
        [] => true,
        [arg] => arg.as_os_str() == OsStr::new("-i"),
        _ => false,
    }
}

fn ensure_interactive_shell_arg(cmd: &mut CommandBuilder) {
    if cmd
        .get_argv()
        .iter()
        .skip(1)
        .any(|arg| arg.as_os_str() == OsStr::new("-i"))
    {
        return;
    }
    cmd.arg("-i");
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CommandRowPresentation {
    separator: bool,
    output: bool,
    failed_marker: bool,
}

#[derive(Clone, Copy)]
enum CommandBlockTextKind {
    Command,
    Output,
}

fn command_row_presentation(blocks: &[TerminalCommandBlock], row: usize) -> CommandRowPresentation {
    let mut presentation = CommandRowPresentation::default();
    for block in blocks {
        if block.prompt_start == Some(row) {
            presentation.separator = true;
        }

        if let Some(output_start) = block.output_start {
            let output_end = block.end.unwrap_or(usize::MAX);
            if row >= output_start && row < output_end {
                presentation.output = true;
            }
        }

        if block.exit_code.is_some_and(|code| code != 0)
            && block
                .end
                .or(block.output_start)
                .or(block.command_start)
                .or(block.prompt_start)
                == Some(row)
        {
            presentation.failed_marker = true;
        }
    }
    presentation
}

fn command_separator_start(screen: &vt100::Screen, row: u16, width: u16) -> u16 {
    let mut content_end = 0;
    for x in 0..width {
        let Some(cell) = screen.cell(row, x) else {
            continue;
        };
        if cell.is_wide_continuation() || cell.contents().is_empty() {
            continue;
        }
        content_end = x.saturating_add(1);
    }
    if content_end == 0 {
        0
    } else {
        content_end.saturating_add(1).min(width)
    }
}

fn command_output_style(theme: &Theme) -> Style {
    theme
        .named_style("terminal-command-output")
        .unwrap_or_else(|| {
            let mut style = Style::default();
            if let Some(bg) = theme.status_bar.bg {
                style = style.bg(bg);
            }
            style
        })
}

fn command_separator_style(theme: &Theme) -> Style {
    theme
        .named_style("terminal-command-separator")
        .unwrap_or_else(|| theme.status_bar_key.add_modifier(Modifier::BOLD))
}

fn command_failure_style(theme: &Theme) -> Style {
    theme
        .named_style("terminal-command-failure")
        .or_else(|| theme.named_style("status-segment-error"))
        .unwrap_or_else(|| Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
}

fn trim_terminal_block_text(mut text: String) -> Option<String> {
    while text.ends_with('\n') || text.ends_with('\r') {
        text.pop();
    }
    (!text.is_empty()).then_some(text)
}

struct TerminalShared {
    parser: TerminalParser,
    scrollback_len: usize,
    palette: TerminalPalette,
    alternate_screen_scroll: TerminalAlternateScreenScroll,
    input: VecDeque<u8>,
    on_input: Option<InputCallback>,
    input_forward: Option<InputCallback>,
    on_exit: Option<ExitCallback>,
    on_window_title: Option<TextCallback>,
    on_window_icon_name: Option<TextCallback>,
    on_audible_bell: Option<BellCallback>,
    on_clipboard_copy: Option<ClipboardCopyCallback>,
    on_command_finished: Option<CommandFinishedCallback>,
    system_clipboard: Option<SystemClipboard>,
    pty_resize: Option<TerminalPtyResize>,
    shell_integration: TerminalShellIntegration,
    tmux_environment: TerminalTmuxEnvironmentConfig,
    last_shell_integration_error: Option<String>,
    exit_status: Option<ExitStatus>,
    process_running: bool,
    window_title: Option<String>,
    window_icon_name: Option<String>,
    audible_bell_count: u64,
    last_clipboard_copy: Option<TerminalClipboardCopy>,
    last_system_clipboard_text: Option<String>,
    last_system_clipboard_error: Option<String>,
    capture: bool,
    /// Set when keyboard capture was auto-released because the terminal window
    /// lost focus (e.g. a modal popup opened). Distinguishes that transient loss
    /// from an intentional release via the release shortcut, so capture can be
    /// restored automatically once focus returns.
    capture_suspended_by_blur: bool,
    release_shortcut: TerminalShortcut,
    prefix_shortcut: TerminalShortcut,
    prefix_bindings: Vec<TerminalPrefixBinding>,
    prefix_pending: bool,
    copy_mode: Option<TerminalCopyModeState>,
    copy_buffer: Option<String>,
    selection: TerminalSelectionState,
    command_marks: Vec<TerminalCommandBlock>,
    current_cwd: Option<String>,
    cursor_shape: TerminalCursorShape,
    dsr_tail: Vec<u8>,
    tmux_dcs_passthrough: TmuxDcsPassthroughDecoder,
}

impl TerminalShared {
    fn apply_runtime_config(&mut self, config: TerminalRuntimeConfig) {
        if self.scrollback_len != config.scrollback_len {
            self.scrollback_len = config.scrollback_len;
            let (rows, cols) = self.parser.screen().size();
            self.parser = terminal_parser(rows, cols, config.scrollback_len);
            self.selection.clear();
            self.copy_mode = None;
            self.command_marks.clear();
        }
        self.palette = config.palette;
        self.release_shortcut = config.release_shortcut;
        self.set_prefix_shortcut(config.prefix_shortcut);
        self.alternate_screen_scroll = config.alternate_screen_scroll;
        self.shell_integration = config.shell_integration;
        self.tmux_environment = config.tmux_environment;
        self.cursor_shape = config.cursor_shape;
    }

    fn set_scrollback_len(&mut self, len: usize) {
        if self.scrollback_len == len {
            return;
        }
        let (rows, cols) = self.parser.screen().size();
        self.scrollback_len = len;
        self.parser = terminal_parser(rows, cols, len);
        self.cursor_shape = TerminalCursorShape::default();
        self.selection.clear();
        self.copy_mode = None;
        self.command_marks.clear();
    }

    fn set_capture(&mut self, capture: bool) {
        self.capture = capture;
        if !capture {
            self.prefix_pending = false;
            self.copy_mode = None;
        }
    }

    fn set_prefix_shortcut(&mut self, shortcut: TerminalShortcut) {
        self.prefix_shortcut = shortcut;
        self.prefix_pending = false;
    }

    fn set_prefix_binding(&mut self, binding: TerminalPrefixBinding) {
        if let Some(existing) = self
            .prefix_bindings
            .iter_mut()
            .find(|existing| existing.shortcut == binding.shortcut)
        {
            *existing = binding;
        } else {
            self.prefix_bindings.push(binding);
        }
        self.prefix_pending = false;
    }

    fn set_prefix_bindings(&mut self, bindings: impl IntoIterator<Item = TerminalPrefixBinding>) {
        self.prefix_bindings.clear();
        for binding in bindings {
            self.set_prefix_binding(binding);
        }
        self.prefix_pending = false;
    }

    fn prefix_command_for_event(&self, event: KeyEvent) -> Option<TerminalPrefixCommand> {
        if self.prefix_shortcut.matches(event) {
            return Some(TerminalPrefixCommand::SendPrefix);
        }
        if event.kind == KeyEventKind::Release {
            return None;
        }
        self.prefix_bindings
            .iter()
            .find(|binding| binding.shortcut.matches(event))
            .map(|binding| binding.command)
    }

    fn apply_callback_events(
        &mut self,
        events: Vec<TerminalCallbackEvent>,
    ) -> Vec<TerminalCallbackDispatch> {
        let mut dispatches = Vec::new();
        for event in events {
            match event {
                TerminalCallbackEvent::WindowTitle(title) => {
                    self.window_title = Some(title.clone());
                    if let Some(callback) = self.on_window_title.clone() {
                        dispatches.push(TerminalCallbackDispatch::WindowTitle(callback, title));
                    }
                }
                TerminalCallbackEvent::WindowIconName(icon_name) => {
                    self.window_icon_name = Some(icon_name.clone());
                    if let Some(callback) = self.on_window_icon_name.clone() {
                        dispatches.push(TerminalCallbackDispatch::WindowIconName(
                            callback, icon_name,
                        ));
                    }
                }
                TerminalCallbackEvent::AudibleBell => {
                    self.audible_bell_count = self.audible_bell_count.saturating_add(1);
                    if let Some(callback) = self.on_audible_bell.clone() {
                        dispatches.push(TerminalCallbackDispatch::AudibleBell(callback));
                    }
                }
                TerminalCallbackEvent::ClipboardCopy(copy) => {
                    self.last_clipboard_copy = Some(copy.clone());
                    if copy.targets_system_clipboard()
                        && let Ok(text) = copy.decoded_text()
                    {
                        self.copy_buffer = Some(text.clone());
                        dispatches.push(TerminalCallbackDispatch::SystemClipboardCopy(text));
                    }
                    if let Some(callback) = self.on_clipboard_copy.clone() {
                        dispatches.push(TerminalCallbackDispatch::ClipboardCopy(callback, copy));
                    }
                }
                TerminalCallbackEvent::UnhandledOsc { params, row, col } => {
                    if let Some(block) = self.apply_unhandled_osc(&params, row, col)
                        && let Some(callback) = self.on_command_finished.clone()
                    {
                        dispatches.push(TerminalCallbackDispatch::CommandFinished(callback, block));
                    }
                }
                TerminalCallbackEvent::CursorShape(shape) => {
                    self.cursor_shape = shape;
                }
            }
        }
        dispatches
    }

    fn apply_unhandled_osc(
        &mut self,
        params: &[Vec<u8>],
        row: usize,
        col: u16,
    ) -> Option<TerminalCommandBlock> {
        match params {
            [kind, rest @ ..] if kind.as_slice() == b"133" => {
                self.apply_osc133_marker(rest, row, col)
            }
            [kind, cwd] if kind.as_slice() == b"7" => {
                if let Some(cwd) = parse_osc7_cwd(cwd) {
                    self.current_cwd = Some(cwd.clone());
                    if let Some(block) = self.current_command_block_mut() {
                        block.cwd = Some(cwd);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn apply_osc133_marker(
        &mut self,
        params: &[Vec<u8>],
        row: usize,
        col: u16,
    ) -> Option<TerminalCommandBlock> {
        let marker = params.first().and_then(|marker| marker.first()).copied()?;
        match marker {
            b'A' => {
                self.record_prompt_start(row, col);
                None
            }
            b'B' => {
                let cwd = self.current_cwd.clone();
                let block = self.open_command_block(row, col, cwd);
                block.command_start = Some(row);
                block.command_start_col = Some(col);
                None
            }
            b'C' => {
                let cwd = self.current_cwd.clone();
                let block = self.open_command_block(row, col, cwd);
                block.output_start = Some(row);
                block.output_start_col = Some(col);
                None
            }
            b'D' => {
                let exit_code = params.get(1).and_then(|code| parse_osc133_exit_code(code));
                let cwd = self.current_cwd.clone();
                let block = self.open_command_block(row, col, cwd);
                block.end = Some(row);
                block.end_col = Some(col);
                block.exit_code = exit_code;
                Some(block.clone())
            }
            _ => None,
        }
    }

    fn record_prompt_start(&mut self, row: usize, col: u16) {
        let cwd = self.current_cwd.clone();
        match self.command_marks.last_mut() {
            Some(block) if block.is_open() && !block.has_command_activity() => {
                block.prompt_start = Some(row);
                block.prompt_start_col = Some(col);
                block.cwd = cwd;
            }
            _ => self
                .command_marks
                .push(TerminalCommandBlock::at_prompt(row, col, cwd)),
        }
    }

    fn open_command_block(
        &mut self,
        row: usize,
        col: u16,
        cwd: Option<String>,
    ) -> &mut TerminalCommandBlock {
        let needs_new_block = self
            .command_marks
            .last()
            .is_none_or(|block| !block.is_open());
        if needs_new_block {
            self.command_marks.push(TerminalCommandBlock {
                prompt_start: Some(row),
                prompt_start_col: Some(col),
                cwd,
                ..TerminalCommandBlock::default()
            });
        }
        self.command_marks.last_mut().expect("command block exists")
    }

    fn current_command_block_mut(&mut self) -> Option<&mut TerminalCommandBlock> {
        self.command_marks
            .last_mut()
            .filter(|block| block.is_open())
    }

    fn queue_input(&mut self, bytes: &[u8]) {
        self.input.extend(bytes);
    }

    fn max_scrollback(&mut self) -> usize {
        let screen = self.parser.screen_mut();
        let current = screen.scrollback();
        screen.set_scrollback(usize::MAX);
        let max = screen.scrollback();
        screen.set_scrollback(current);
        max
    }

    fn scrollback_offset(&self) -> usize {
        self.parser.screen().scrollback()
    }

    fn set_scrollback_offset(&mut self, offset: usize) {
        self.parser.screen_mut().set_scrollback(offset);
    }

    fn resize_screen(&mut self, rows: u16, cols: u16) -> bool {
        let screen = self.parser.screen_mut();
        if screen.size() == (rows, cols) {
            return false;
        }
        screen.set_size(rows, cols);
        true
    }

    fn set_scrollback_from_scroll_offset(&mut self, scroll_offset: u16) {
        let max = self.max_scrollback().min(u16::MAX as usize);
        let y = scroll_offset.min(max as u16) as usize;
        let offset = max.saturating_sub(y);
        self.set_scrollback_offset(offset);
    }

    fn enter_copy_mode(&mut self) {
        let cursor = self.current_copy_mode_position();
        self.copy_mode = Some(TerminalCopyModeState::new(cursor));
        self.prefix_pending = false;
        self.selection.clear();
        self.ensure_copy_mode_cursor_visible();
    }

    fn cancel_copy_mode(&mut self) {
        self.copy_mode = None;
        self.selection.clear();
    }

    fn finish_copy_mode_copy(&mut self) -> Option<String> {
        let text = self.copy_selection();
        self.copy_mode = None;
        self.selection.clear();
        text
    }

    fn copy_selection(&mut self) -> Option<String> {
        let text = self.selected_text()?;
        self.copy_buffer = Some(text.clone());
        Some(text)
    }

    fn paste_copy_buffer_bytes(&self) -> Option<Vec<u8>> {
        self.copy_buffer
            .as_deref()
            .map(|text| encode_paste_text(self.parser.screen(), text))
    }

    fn selected_text(&mut self) -> Option<String> {
        let range = self.selection.range()?;
        let max_scrollback = self.max_scrollback();
        selected_text_from_screen(self.parser.screen_mut(), max_scrollback, range)
    }

    fn command_block_index_at_position(
        &self,
        position: TerminalSelectionPosition,
    ) -> Option<usize> {
        self.command_marks
            .iter()
            .enumerate()
            .rev()
            .find(|(_, block)| block.contains_row(position.row))
            .map(|(index, _)| index)
    }

    fn scroll_to_command_block(&mut self, index: usize) -> bool {
        let Some(anchor_row) = self
            .command_marks
            .get(index)
            .and_then(TerminalCommandBlock::anchor_row)
        else {
            return false;
        };
        let max = self.max_scrollback();
        let target_top = anchor_row.min(max);
        let desired = max.saturating_sub(target_top);
        if self.parser.screen().scrollback() == desired {
            return false;
        }
        self.parser.screen_mut().set_scrollback(desired);
        true
    }

    fn scroll_to_previous_command_block(&mut self) -> Option<usize> {
        let max = self.max_scrollback();
        let current_top = visible_top_row(max, self.parser.screen().scrollback());
        let target = if self.parser.screen().scrollback() == 0 {
            self.command_marks
                .iter()
                .enumerate()
                .rev()
                .find(|(_, block)| block.anchor_row().is_some_and(|row| row < max))
        } else {
            self.command_marks
                .iter()
                .enumerate()
                .rev()
                .find(|(_, block)| block.anchor_row().is_some_and(|row| row < current_top))
        };
        let (index, _) = target?;
        self.scroll_to_command_block(index).then_some(index)
    }

    fn scroll_to_next_command_block(&mut self) -> Option<usize> {
        let max = self.max_scrollback();
        let current_top = visible_top_row(max, self.parser.screen().scrollback());
        let (index, _) = self
            .command_marks
            .iter()
            .enumerate()
            .find(|(_, block)| block.anchor_row().is_some_and(|row| row > current_top))?;
        self.scroll_to_command_block(index).then_some(index)
    }

    fn select_command_block_output(&mut self, index: usize) -> Option<TerminalSelectionRange> {
        let range = self.command_block_text_range(index, CommandBlockTextKind::Output)?;
        self.selection.start_keyboard(range.start);
        self.selection.update(range.end);
        Some(range)
    }

    fn copy_command_block_text(
        &mut self,
        index: usize,
        kind: CommandBlockTextKind,
    ) -> Option<String> {
        let text = self.command_block_text(index, kind)?;
        self.copy_buffer = Some(text.clone());
        Some(text)
    }

    fn command_block_rerun_bytes(&mut self, index: usize) -> Option<Vec<u8>> {
        let command = self.command_block_text(index, CommandBlockTextKind::Command)?;
        let mut bytes = command.into_bytes();
        bytes.push(b'\n');
        Some(bytes)
    }

    fn command_block_text(&mut self, index: usize, kind: CommandBlockTextKind) -> Option<String> {
        let range = self.command_block_text_range(index, kind)?;
        let max_scrollback = self.max_scrollback();
        let text = selected_text_from_screen(self.parser.screen_mut(), max_scrollback, range)?;
        trim_terminal_block_text(text)
    }

    fn command_block_text_range(
        &mut self,
        index: usize,
        kind: CommandBlockTextKind,
    ) -> Option<TerminalSelectionRange> {
        let block = self.command_marks.get(index)?.clone();
        let (rows, cols) = self.parser.screen().size();
        let max_scrollback = self.max_scrollback();
        let bottom_row = max_scrollback
            .saturating_add(usize::from(rows))
            .saturating_sub(1);
        match kind {
            CommandBlockTextKind::Command => {
                let end_row = block.output_start.or(block.end)?;
                let end_col = if block.output_start.is_some() {
                    block.output_start_col.unwrap_or(cols)
                } else {
                    block.end_col.unwrap_or(cols)
                };

                // The command-start marker (OSC 133 `B`) is only a usable start
                // when it precedes the output-start marker (`C`). Real shell
                // integrations (zsh preexec, bash PS0) emit `B` and `C` together
                // *after* the user submits the command, so they land at the same
                // position and the `B..C` range collapses to empty. In that case
                // the typed command lives on the prompt line, so fall back to the
                // prompt-start marker.
                let command_start = block.command_start.filter(|&cmd_row| {
                    cmd_row < end_row
                        || (cmd_row == end_row && block.command_start_col.unwrap_or(0) < end_col)
                });
                let (start_row, start_col) = match command_start {
                    Some(row) => (row, block.command_start_col.unwrap_or(0)),
                    None => (block.prompt_start?, block.prompt_start_col.unwrap_or(0)),
                };

                TerminalSelectionRange::new(
                    TerminalSelectionPosition::new(start_row, start_col),
                    TerminalSelectionPosition::new(end_row, end_col),
                )
            }
            CommandBlockTextKind::Output => {
                let start_row = block.output_start?;
                let start_col = block.output_start_col.unwrap_or(0);
                let end_row = block.end.unwrap_or(bottom_row);
                let end_col = block.end_col.unwrap_or(cols);
                TerminalSelectionRange::new(
                    TerminalSelectionPosition::new(start_row, start_col),
                    TerminalSelectionPosition::new(end_row, end_col),
                )
            }
        }
    }

    fn current_copy_mode_position(&mut self) -> TerminalSelectionPosition {
        let max_scrollback = self.max_scrollback();
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let (row, col) = screen.cursor_position();
        position_for_view_cell(
            max_scrollback,
            screen.scrollback(),
            rows,
            cols,
            row.min(rows.saturating_sub(1)),
            col.min(cols),
        )
    }

    fn begin_copy_mode_selection(&mut self) {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return;
        };
        self.selection.start_keyboard(cursor);
        if let Some(mode) = &mut self.copy_mode {
            mode.selecting = true;
        }
    }

    fn move_copy_mode_cursor(&mut self, row_delta: isize, col_delta: isize) -> bool {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return false;
        };
        let row = if row_delta.is_negative() {
            cursor.row.saturating_sub(row_delta.unsigned_abs())
        } else {
            cursor.row.saturating_add(row_delta as usize)
        };
        let col = if col_delta.is_negative() {
            cursor
                .col
                .saturating_sub(col_delta.unsigned_abs().min(u16::MAX as usize) as u16)
        } else {
            cursor
                .col
                .saturating_add((col_delta as usize).min(u16::MAX as usize) as u16)
        };
        self.set_copy_mode_cursor(TerminalSelectionPosition::new(row, col))
    }

    fn set_copy_mode_cursor(&mut self, position: TerminalSelectionPosition) -> bool {
        let position = self.clamp_copy_mode_position(position);
        let Some(mode) = &mut self.copy_mode else {
            return false;
        };
        if mode.cursor == position {
            return false;
        }
        mode.cursor = position;
        if mode.selecting {
            self.selection.update(position);
        }
        self.ensure_copy_mode_cursor_visible();
        true
    }

    fn move_copy_mode_cursor_to_column(&mut self, col: u16) -> bool {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return false;
        };
        self.set_copy_mode_cursor(TerminalSelectionPosition::new(cursor.row, col))
    }

    fn move_copy_mode_cursor_by_page(&mut self, page_delta: isize) -> bool {
        let rows = self.parser.screen().size().0.max(1) as isize;
        self.move_copy_mode_cursor(page_delta.saturating_mul(rows), 0)
    }

    fn clamp_copy_mode_position(
        &mut self,
        position: TerminalSelectionPosition,
    ) -> TerminalSelectionPosition {
        let max_scrollback = self.max_scrollback();
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let last_row = max_scrollback.saturating_add(usize::from(rows.saturating_sub(1)));
        TerminalSelectionPosition::new(position.row.min(last_row), position.col.min(cols))
    }

    fn ensure_copy_mode_cursor_visible(&mut self) {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return;
        };
        let max_scrollback = self.max_scrollback();
        let screen = self.parser.screen();
        let (rows, _) = screen.size();
        if rows == 0 {
            return;
        }
        let current = screen.scrollback();
        let top = visible_top_row(max_scrollback, current);
        let height = usize::from(rows);
        let bottom = top.saturating_add(height.saturating_sub(1));
        let desired = if cursor.row < top {
            max_scrollback.saturating_sub(cursor.row)
        } else if cursor.row > bottom {
            max_scrollback.saturating_sub(cursor.row.saturating_sub(height.saturating_sub(1)))
        } else {
            current
        };
        if desired != current {
            self.parser
                .screen_mut()
                .set_scrollback(desired.min(max_scrollback));
        }
    }

    fn scroll_copy_mode_view(&mut self, line_delta: isize) -> bool {
        if self.copy_mode.is_none() {
            return false;
        }
        let max = self.max_scrollback();
        let current = self.parser.screen().scrollback();
        let desired = if line_delta.is_negative() {
            current.saturating_sub(line_delta.unsigned_abs())
        } else {
            current.saturating_add(line_delta as usize).min(max)
        };
        if desired != current {
            self.parser.screen_mut().set_scrollback(desired);
            self.clamp_copy_mode_cursor_to_visible();
            return true;
        }
        false
    }

    fn clamp_copy_mode_cursor_to_visible(&mut self) {
        let Some(cursor) = self.copy_mode.as_ref().map(|mode| mode.cursor) else {
            return;
        };
        let max_scrollback = self.max_scrollback();
        let screen = self.parser.screen();
        let (rows, _) = screen.size();
        if rows == 0 {
            return;
        }
        let top = visible_top_row(max_scrollback, screen.scrollback());
        let bottom = top.saturating_add(usize::from(rows.saturating_sub(1)));
        let row = cursor.row.clamp(top, bottom);
        if row != cursor.row {
            let _ = self.set_copy_mode_cursor(TerminalSelectionPosition::new(row, cursor.col));
        }
    }
}

enum CapturedKeyAction {
    Consumed,
    Dispatch(Vec<u8>),
    Component(ComponentAction),
    SystemClipboardCopy(String),
}

fn handle_captured_key(shared: &mut TerminalShared, event: KeyEvent) -> CapturedKeyAction {
    if shared.copy_mode.is_some() {
        return handle_copy_mode_key(shared, event);
    }
    if shared.release_shortcut.matches(event) {
        shared.set_capture(false);
        // Intentional release: do not auto-restore capture on refocus.
        shared.capture_suspended_by_blur = false;
        return CapturedKeyAction::Consumed;
    }
    if event.kind == KeyEventKind::Release {
        return CapturedKeyAction::Consumed;
    }
    if handle_command_navigation_key(shared, event) {
        return CapturedKeyAction::Consumed;
    }
    if shared.prefix_pending {
        shared.prefix_pending = false;
        if let Some(command) = shared.prefix_command_for_event(event) {
            return handle_prefix_command(shared, command);
        }
        return encode_prefix_fallback(shared, event)
            .map(CapturedKeyAction::Dispatch)
            .unwrap_or(CapturedKeyAction::Consumed);
    }
    if shared.prefix_shortcut.matches(event) {
        shared.prefix_pending = true;
        return CapturedKeyAction::Consumed;
    }
    encode_key_event(shared.parser.screen(), event)
        .map(CapturedKeyAction::Dispatch)
        .unwrap_or(CapturedKeyAction::Consumed)
}

fn handle_command_navigation_key(shared: &mut TerminalShared, event: KeyEvent) -> bool {
    if event.kind == KeyEventKind::Release || event.modifiers != KeyModifiers::CONTROL {
        return false;
    }
    match event.code {
        KeyCode::Up => shared.scroll_to_previous_command_block().is_some(),
        KeyCode::Down => shared.scroll_to_next_command_block().is_some(),
        _ => false,
    }
}

fn handle_copy_mode_key(shared: &mut TerminalShared, event: KeyEvent) -> CapturedKeyAction {
    if event.kind == KeyEventKind::Release {
        return CapturedKeyAction::Consumed;
    }
    match event.code {
        KeyCode::Esc => {
            shared.cancel_copy_mode();
        }
        KeyCode::Enter => {
            if let Some(text) = shared.finish_copy_mode_copy() {
                return CapturedKeyAction::SystemClipboardCopy(text);
            }
        }
        KeyCode::Up => {
            let _ = shared.move_copy_mode_cursor(-1, 0);
        }
        KeyCode::Down => {
            let _ = shared.move_copy_mode_cursor(1, 0);
        }
        KeyCode::Left => {
            let _ = shared.move_copy_mode_cursor(0, -1);
        }
        KeyCode::Right => {
            let _ = shared.move_copy_mode_cursor(0, 1);
        }
        KeyCode::PageUp => {
            let _ = shared.move_copy_mode_cursor_by_page(-1);
        }
        KeyCode::PageDown => {
            let _ = shared.move_copy_mode_cursor_by_page(1);
        }
        KeyCode::Home => {
            let _ = shared.move_copy_mode_cursor_to_column(0);
        }
        KeyCode::End => {
            let cols = shared.parser.screen().size().1;
            let _ = shared.move_copy_mode_cursor_to_column(cols);
        }
        KeyCode::Char(ch) if event.modifiers == KeyModifiers::NONE => {
            match ch.to_ascii_lowercase() {
                'q' => shared.cancel_copy_mode(),
                'v' | ' ' => shared.begin_copy_mode_selection(),
                'y' => {
                    if let Some(text) = shared.finish_copy_mode_copy() {
                        return CapturedKeyAction::SystemClipboardCopy(text);
                    }
                }
                'h' => {
                    let _ = shared.move_copy_mode_cursor(0, -1);
                }
                'j' => {
                    let _ = shared.move_copy_mode_cursor(1, 0);
                }
                'k' => {
                    let _ = shared.move_copy_mode_cursor(-1, 0);
                }
                'l' => {
                    let _ = shared.move_copy_mode_cursor(0, 1);
                }
                _ => {}
            }
        }
        _ => {}
    }
    CapturedKeyAction::Consumed
}

fn handle_prefix_command(
    shared: &mut TerminalShared,
    command: TerminalPrefixCommand,
) -> CapturedKeyAction {
    match command {
        TerminalPrefixCommand::ActivateMenu => {
            CapturedKeyAction::Component(ComponentAction::ActivateMenu)
        }
        TerminalPrefixCommand::ToggleWindowManagement => {
            CapturedKeyAction::Component(ComponentAction::ToggleWindowManagement)
        }
        TerminalPrefixCommand::ToggleMaximize => {
            CapturedKeyAction::Component(ComponentAction::ToggleMaximizeWindow)
        }
        TerminalPrefixCommand::EnterCopyMode => {
            shared.enter_copy_mode();
            CapturedKeyAction::Consumed
        }
        TerminalPrefixCommand::PasteCopyBuffer => shared
            .paste_copy_buffer_bytes()
            .map(CapturedKeyAction::Dispatch)
            .unwrap_or(CapturedKeyAction::Consumed),
        TerminalPrefixCommand::SendPrefix => encode_prefix_literal(shared)
            .map(CapturedKeyAction::Dispatch)
            .unwrap_or(CapturedKeyAction::Consumed),
    }
}

fn encode_prefix_literal(shared: &TerminalShared) -> Option<Vec<u8>> {
    encode_key_event(
        shared.parser.screen(),
        KeyEvent::new(
            shared.prefix_shortcut.code,
            shared.prefix_shortcut.modifiers,
        ),
    )
}

fn encode_prefix_fallback(shared: &TerminalShared, event: KeyEvent) -> Option<Vec<u8>> {
    let screen = shared.parser.screen();
    let mut bytes = encode_key_event(
        screen,
        KeyEvent::new(
            shared.prefix_shortcut.code,
            shared.prefix_shortcut.modifiers,
        ),
    )
    .unwrap_or_default();
    if let Some(mut event_bytes) = encode_key_event(screen, event) {
        bytes.append(&mut event_bytes);
    }
    if bytes.is_empty() { None } else { Some(bytes) }
}

type PtyChild = Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>;

#[derive(Clone)]
struct TerminalPtyResize {
    master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    last_size: Arc<Mutex<(u16, u16)>>,
}

impl TerminalPtyResize {
    fn new(master: Box<dyn portable_pty::MasterPty + Send>, rows: u16, cols: u16) -> Self {
        Self {
            master: Arc::new(Mutex::new(master)),
            last_size: Arc::new(Mutex::new((rows, cols))),
        }
    }

    fn resize_if_needed(&self, rows: u16, cols: u16) -> bool {
        let mut last_size = self.last_size.lock();
        if *last_size == (rows, cols) {
            return false;
        }
        let _ = self.master.lock().resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        *last_size = (rows, cols);
        true
    }
}

struct TerminalProcess {
    _pty_resize: TerminalPtyResize,
    child: PtyChild,
    reader_alive: Arc<AtomicBool>,
    reader_thread: Option<thread::JoinHandle<()>>,
    exit_watcher_thread: Option<thread::JoinHandle<()>>,
    _shell_integration_files: Option<TerminalShellIntegrationFiles>,
}

impl TerminalProcess {
    fn record_exit_if_ready(&mut self, shared: &Arc<Mutex<TerminalShared>>) -> bool {
        try_record_child_exit(shared, &self.child)
    }

    fn shutdown(&mut self, shared: &Arc<Mutex<TerminalShared>>) {
        let already_exited = self.record_exit_if_ready(shared);
        self.reader_alive.store(false, Ordering::Relaxed);
        if !already_exited {
            let _ = self.child.lock().kill();
        }
        if let Some(handle) = self.reader_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.exit_watcher_thread.take() {
            let _ = handle.join();
        }
    }
}

fn try_record_child_exit(shared: &Arc<Mutex<TerminalShared>>, child: &PtyChild) -> bool {
    let status = match child.lock().try_wait() {
        Ok(Some(status)) => status,
        Ok(None) | Err(_) => return false,
    };
    record_exit_status(shared, status);
    true
}

fn record_exit_status(shared: &Arc<Mutex<TerminalShared>>, status: ExitStatus) {
    let callback = {
        let mut shared = shared.lock();
        if shared.exit_status.is_some() {
            return;
        }
        shared.exit_status = Some(status.clone());
        shared.process_running = false;
        shared.input_forward = None;
        shared.pty_resize = None;
        shared.on_exit.clone()
    };

    if let Some(callback) = callback {
        callback(status);
    }
}

fn dispatch_input(shared: &Arc<Mutex<TerminalShared>>, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let callbacks = {
        let mut shared = shared.lock();
        shared.queue_input(bytes);
        let mut callbacks = Vec::new();
        if let Some(cb) = shared.on_input.clone() {
            callbacks.push(cb);
        }
        if let Some(cb) = shared.input_forward.clone() {
            callbacks.push(cb);
        }
        callbacks
    };
    for cb in callbacks {
        cb(bytes);
    }
}

fn forward_input(shared: &Arc<Mutex<TerminalShared>>, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let cb = { shared.lock().input_forward.clone() };
    if let Some(cb) = cb {
        cb(bytes);
    }
}

fn resize_terminal(shared: &Arc<Mutex<TerminalShared>>, rows: u16, cols: u16) -> bool {
    if rows == 0 || cols == 0 {
        return false;
    }
    let (screen_changed, pty_resize) = {
        let mut shared = shared.lock();
        let screen_changed = shared.resize_screen(rows, cols);
        (screen_changed, shared.pty_resize.clone())
    };
    let pty_changed = pty_resize
        .map(|resize| resize.resize_if_needed(rows, cols))
        .unwrap_or(false);
    screen_changed || pty_changed
}

fn dispatch_system_clipboard_copy(shared: &Arc<Mutex<TerminalShared>>, text: &str) {
    let clipboard = { shared.lock().system_clipboard.clone() };
    let Some(clipboard) = clipboard else {
        return;
    };
    let result = clipboard.copy_text(text);
    let mut shared = shared.lock();
    shared.last_system_clipboard_text = Some(text.to_string());
    shared.last_system_clipboard_error = result.err().map(|error| error.to_string());
}

fn dispatch_terminal_callback_events(
    shared: &Arc<Mutex<TerminalShared>>,
    dispatches: Vec<TerminalCallbackDispatch>,
) {
    for dispatch in dispatches {
        match dispatch {
            TerminalCallbackDispatch::WindowTitle(callback, title) => callback(&title),
            TerminalCallbackDispatch::WindowIconName(callback, icon_name) => callback(&icon_name),
            TerminalCallbackDispatch::AudibleBell(callback) => callback(),
            TerminalCallbackDispatch::ClipboardCopy(callback, copy) => callback(&copy),
            TerminalCallbackDispatch::CommandFinished(callback, block) => callback(&block),
            TerminalCallbackDispatch::SystemClipboardCopy(text) => {
                dispatch_system_clipboard_copy(shared, &text);
            }
        }
    }
}

enum DsrResponse {
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
fn collect_dsr_responses(shared: &mut TerminalShared, bytes: &[u8]) -> Vec<Vec<u8>> {
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
    shared.dsr_tail.extend_from_slice(&combined[tail_start..]);

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

/// A terminal emulator widget.
pub struct TerminalEmulator {
    shared: Arc<Mutex<TerminalShared>>,
    last_area: Option<Rect>,
    capture_on_click: bool,
    command_block_presentation: TerminalCommandBlockPresentation,
    process: Option<TerminalProcess>,
    on_close: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl TerminalEmulator {
    pub fn new() -> Self {
        let runtime_config = TerminalRuntimeConfig::from_config(&TerminalConfig::default())
            .expect("default terminal config must be valid");
        let parser = terminal_parser(24, 80, runtime_config.scrollback_len);
        let shared = TerminalShared {
            parser,
            scrollback_len: runtime_config.scrollback_len,
            palette: runtime_config.palette,
            alternate_screen_scroll: runtime_config.alternate_screen_scroll,
            input: VecDeque::new(),
            on_input: None,
            input_forward: None,
            on_exit: None,
            on_window_title: None,
            on_window_icon_name: None,
            on_audible_bell: None,
            on_clipboard_copy: None,
            on_command_finished: None,
            system_clipboard: Some(Arc::new(DefaultTerminalSystemClipboard)),
            pty_resize: None,
            shell_integration: runtime_config.shell_integration,
            tmux_environment: runtime_config.tmux_environment,
            last_shell_integration_error: None,
            exit_status: None,
            process_running: false,
            window_title: None,
            window_icon_name: None,
            audible_bell_count: 0,
            last_clipboard_copy: None,
            last_system_clipboard_text: None,
            last_system_clipboard_error: None,
            capture: true,
            capture_suspended_by_blur: false,
            release_shortcut: runtime_config.release_shortcut,
            prefix_shortcut: runtime_config.prefix_shortcut,
            prefix_bindings: default_prefix_bindings(),
            prefix_pending: false,
            copy_mode: None,
            copy_buffer: None,
            selection: TerminalSelectionState::default(),
            command_marks: Vec::new(),
            current_cwd: None,
            cursor_shape: runtime_config.cursor_shape,
            dsr_tail: Vec::with_capacity(4),
            tmux_dcs_passthrough: TmuxDcsPassthroughDecoder::default(),
        };

        Self {
            shared: Arc::new(Mutex::new(shared)),
            last_area: None,
            capture_on_click: true,
            command_block_presentation: TerminalCommandBlockPresentation::default(),
            process: None,
            on_close: None,
        }
    }

    pub fn from_config(config: &TerminalConfig) -> Result<Self> {
        Self::new().config(config)
    }

    pub fn handle(&self) -> TerminalHandle {
        TerminalHandle {
            shared: Arc::clone(&self.shared),
        }
    }

    /// Explicitly resizes the parser screen and attached PTY, if a subprocess is running.
    pub fn resize(&mut self, rows: u16, cols: u16) -> bool {
        resize_terminal(&self.shared, rows, cols)
    }

    pub fn scrollback_len(self, len: usize) -> Self {
        self.shared.lock().set_scrollback_len(len);
        self
    }

    pub fn config(self, config: &TerminalConfig) -> Result<Self> {
        self.handle().apply_config(config)?;
        Ok(self)
    }

    pub fn capture(self, capture: bool) -> Self {
        self.shared.lock().set_capture(capture);
        self
    }

    pub fn release_shortcut(self, shortcut: TerminalShortcut) -> Self {
        self.shared.lock().release_shortcut = shortcut;
        self
    }

    /// Sets the terminal prefix shortcut. Only plain `Ctrl+<ASCII letter>` is accepted.
    pub fn prefix_shortcut(self, shortcut: TerminalShortcut) -> Result<Self> {
        let shortcut = normalize_prefix_shortcut(shortcut)?;
        self.shared.lock().set_prefix_shortcut(shortcut);
        Ok(self)
    }

    /// Sets the terminal prefix key letter, using `Ctrl+letter` as the actual shortcut.
    pub fn prefix_key(self, letter: char) -> Result<Self> {
        self.prefix_shortcut(prefix_shortcut_from_letter(letter)?)
    }

    /// Adds or replaces one prefix command binding.
    pub fn prefix_binding(
        self,
        shortcut: TerminalShortcut,
        command: TerminalPrefixCommand,
    ) -> Self {
        self.shared
            .lock()
            .set_prefix_binding(TerminalPrefixBinding::new(shortcut, command));
        self
    }

    /// Replaces the prefix command table.
    pub fn prefix_bindings(
        self,
        bindings: impl IntoIterator<Item = TerminalPrefixBinding>,
    ) -> Self {
        self.shared.lock().set_prefix_bindings(bindings);
        self
    }

    pub fn scroll_step(self, step: u16) -> Self {
        self.shared.lock().alternate_screen_scroll.step = step.max(1);
        self
    }

    pub fn capture_on_click(mut self, enabled: bool) -> Self {
        self.capture_on_click = enabled;
        self
    }

    /// Enables or disables visual presentation for OSC 133 command blocks.
    pub fn command_block_presentation(
        mut self,
        presentation: TerminalCommandBlockPresentation,
    ) -> Self {
        self.command_block_presentation = presentation;
        self
    }

    /// Replaces the system clipboard backend used by selection, copy-mode, and OSC 52 copies.
    pub fn system_clipboard<C>(self, clipboard: C) -> Self
    where
        C: TerminalSystemClipboard + 'static,
    {
        self.shared.lock().system_clipboard = Some(Arc::new(clipboard));
        self
    }

    /// Disables system clipboard writes while preserving the terminal-local copy buffer.
    pub fn without_system_clipboard(self) -> Self {
        self.shared.lock().system_clipboard = None;
        self
    }

    /// Configures whether supported interactive shell spawns receive OSC 133/7 integration.
    pub fn shell_integration(self, integration: TerminalShellIntegration) -> Self {
        self.shared.lock().shell_integration = integration;
        self
    }

    /// Configures tmux-compatible probe variables for future subprocess spawns.
    pub fn tmux_environment(self, config: TerminalTmuxEnvironmentConfig) -> Self {
        self.shared.lock().tmux_environment = config;
        self
    }

    pub fn on_input<F>(self, callback: F) -> Self
    where
        F: Fn(&[u8]) + Send + Sync + 'static,
    {
        self.shared.lock().on_input = Some(Arc::new(callback));
        self
    }

    pub fn on_close<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_close = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires once when the attached subprocess exits.
    pub fn on_exit<F>(self, callback: F) -> Self
    where
        F: Fn(ExitStatus) + Send + Sync + 'static,
    {
        self.shared.lock().on_exit = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when OSC 0/2 updates the window title.
    pub fn on_window_title<F>(self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.shared.lock().on_window_title = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when OSC 0/1 updates the window icon name.
    pub fn on_window_icon_name<F>(self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.shared.lock().on_window_icon_name = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when BEL requests an audible bell.
    pub fn on_audible_bell<F>(self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.shared.lock().on_audible_bell = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when OSC 52 requests a clipboard copy.
    pub fn on_clipboard_copy<F>(self, callback: F) -> Self
    where
        F: Fn(&TerminalClipboardCopy) + Send + Sync + 'static,
    {
        self.shared.lock().on_clipboard_copy = Some(Arc::new(callback));
        self
    }

    /// Registers a callback that fires when OSC 133 marks a command as finished.
    pub fn on_command_finished<F>(self, callback: F) -> Self
    where
        F: Fn(&TerminalCommandBlock) + Send + Sync + 'static,
    {
        self.shared.lock().on_command_finished = Some(Arc::new(callback));
        self
    }

    /// Spawns a subprocess attached to the terminal's PTY.
    pub fn spawn_process(&mut self, command: &str, args: &[String]) -> Result<()> {
        let mut cmd = CommandBuilder::new(command);
        for arg in args {
            cmd.arg(arg);
        }
        self.spawn_command(cmd)
    }

    /// Spawns a subprocess from a reusable terminal session spec.
    pub fn spawn_session(&mut self, session: &TerminalSessionSpec) -> Result<()> {
        self.spawn_command(session.command_builder())
    }

    /// Spawns a subprocess using a custom command builder.
    pub fn spawn_command(&mut self, mut cmd: CommandBuilder) -> Result<()> {
        let (shell_integration, tmux_environment) = {
            let shared = self.shared.lock();
            (shared.shell_integration, shared.tmux_environment.clone())
        };
        prepare_spawn_command(&mut cmd, &tmux_environment)?;
        let shell_integration_files = match prepare_shell_integration(&mut cmd, shell_integration) {
            Ok(files) => {
                self.shared.lock().last_shell_integration_error = None;
                files
            }
            Err(error) => {
                self.shared.lock().last_shell_integration_error = Some(error.to_string());
                None
            }
        };

        self.stop_process();
        {
            let mut shared = self.shared.lock();
            shared.exit_status = None;
            shared.process_running = false;
            shared.pty_resize = None;
        }

        let (rows, cols) = {
            let shared = self.shared.lock();
            shared.parser.screen().size()
        };

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let child = pair.slave.spawn_command(cmd)?;
        let child = Arc::new(Mutex::new(child));
        let master = pair.master;
        let writer = master.take_writer()?;
        let reader = master.try_clone_reader()?;
        let pty_resize = TerminalPtyResize::new(master, rows, cols);
        {
            let mut shared = self.shared.lock();
            shared.process_running = true;
            shared.pty_resize = Some(pty_resize.clone());
        }

        let handle = self.handle();
        let shared_for_reader = Arc::clone(&self.shared);
        let child_for_reader = Arc::clone(&child);
        let reader_alive = Arc::new(AtomicBool::new(true));
        let reader_alive_thread = Arc::clone(&reader_alive);
        let reader_thread = thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 8192];
            while reader_alive_thread.load(Ordering::Relaxed) {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => handle.process_output(&buf[..n]),
                    Err(_) => break,
                }
            }
            if reader_alive_thread.load(Ordering::Relaxed) {
                try_record_child_exit(&shared_for_reader, &child_for_reader);
            }
        });
        let shared_for_watcher = Arc::clone(&self.shared);
        let child_for_watcher = Arc::clone(&child);
        let exit_watcher_alive = Arc::clone(&reader_alive);
        let exit_watcher_thread = thread::spawn(move || {
            while exit_watcher_alive.load(Ordering::Relaxed) {
                if try_record_child_exit(&shared_for_watcher, &child_for_watcher) {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        });

        let forward_writer = Arc::new(Mutex::new(writer));
        self.shared.lock().input_forward = Some(Arc::new(move |bytes| {
            if bytes.is_empty() {
                return;
            }
            let mut writer = forward_writer.lock();
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }));

        self.process = Some(TerminalProcess {
            _pty_resize: pty_resize,
            child,
            reader_alive,
            reader_thread: Some(reader_thread),
            exit_watcher_thread: Some(exit_watcher_thread),
            _shell_integration_files: shell_integration_files,
        });

        Ok(())
    }

    /// Stops the currently attached subprocess, if any.
    pub fn stop_process(&mut self) {
        {
            let mut shared = self.shared.lock();
            shared.input_forward = None;
            shared.process_running = false;
            shared.pty_resize = None;
        }
        if let Some(mut process) = self.process.take() {
            process.shutdown(&self.shared);
        }
    }

    fn handle_scrollback_wheel(&mut self, event: MouseEvent) -> bool {
        let mut shared = self.shared.lock();
        let step = shared.alternate_screen_scroll.step;
        let delta = match event.kind {
            MouseEventKind::ScrollUp => -(step as i16),
            MouseEventKind::ScrollDown => step as i16,
            _ => return false,
        };
        let max = shared.max_scrollback();
        let current = shared.parser.screen().scrollback();
        let desired = if delta.is_negative() {
            let amount = i32::from(delta).unsigned_abs() as usize;
            current.saturating_add(amount).min(max)
        } else {
            current.saturating_sub(delta as usize)
        };
        if desired != current {
            shared.parser.screen_mut().set_scrollback(desired);
            return true;
        }
        false
    }

    fn handle_alternate_screen_wheel(&mut self, event: MouseEvent) -> bool {
        let shared = self.shared.lock();
        let config = shared.alternate_screen_scroll;
        if !config.enabled {
            return false;
        }
        let shortcut = match event.kind {
            MouseEventKind::ScrollUp => config.scroll_up_key,
            MouseEventKind::ScrollDown => config.scroll_down_key,
            _ => return false,
        };
        let screen = shared.parser.screen();
        if !matches!(screen.mouse_protocol_mode(), vt100::MouseProtocolMode::None)
            || !screen.alternate_screen()
        {
            return false;
        }

        let Some(key_bytes) =
            encode_key_event(screen, KeyEvent::new(shortcut.code, shortcut.modifiers))
        else {
            return true;
        };
        let mut bytes = Vec::with_capacity(key_bytes.len() * usize::from(config.step));
        for _ in 0..config.step {
            bytes.extend_from_slice(&key_bytes);
        }
        drop(shared);
        dispatch_input(&self.shared, &bytes);
        true
    }

    fn handle_scrollback_key(&mut self, event: KeyEvent) -> bool {
        if event.kind == KeyEventKind::Release {
            return false;
        }
        let mut shared = self.shared.lock();
        if handle_command_navigation_key(&mut shared, event) {
            return true;
        }
        let max = shared.max_scrollback();
        let current = shared.parser.screen().scrollback();
        let rows = shared.parser.screen().size().0 as usize;
        let desired = match event.code {
            KeyCode::PageUp => current.saturating_add(rows).min(max),
            KeyCode::PageDown => current.saturating_sub(rows),
            KeyCode::Home => max,
            KeyCode::End => 0,
            _ => return false,
        };
        if desired != current {
            shared.parser.screen_mut().set_scrollback(desired);
            return true;
        }
        false
    }

    fn handle_local_mouse_selection(
        &mut self,
        event: MouseEvent,
        coordinate_space: MouseCoordinateSpace,
    ) -> bool {
        if !matches!(
            event.kind,
            MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left)
        ) {
            return false;
        }

        let mut shared = self.shared.lock();
        let mouse_reporting_enabled = !matches!(
            shared.parser.screen().mouse_protocol_mode(),
            vt100::MouseProtocolMode::None
        );
        let selection_requested = !mouse_reporting_enabled
            || event.modifiers.contains(KeyModifiers::SHIFT)
            || shared.selection.is_dragging();
        if !selection_requested {
            return false;
        }

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(position) = mouse_selection_position(
                    &mut shared,
                    self.last_area,
                    event,
                    coordinate_space,
                    false,
                ) else {
                    return false;
                };
                shared.selection.start(position);
                true
            }
            MouseEventKind::Drag(MouseButton::Left) if shared.selection.is_dragging() => {
                let Some(position) = mouse_selection_position(
                    &mut shared,
                    self.last_area,
                    event,
                    coordinate_space,
                    true,
                ) else {
                    return false;
                };
                shared.selection.update(position);
                true
            }
            MouseEventKind::Up(MouseButton::Left) if shared.selection.is_dragging() => {
                let include_cell = shared.selection.range().is_some();
                let Some(position) = mouse_selection_position(
                    &mut shared,
                    self.last_area,
                    event,
                    coordinate_space,
                    include_cell,
                ) else {
                    return false;
                };
                shared.selection.finish(position);
                let text = shared.copy_selection();
                drop(shared);
                if let Some(text) = text {
                    dispatch_system_clipboard_copy(&self.shared, &text);
                }
                true
            }
            _ => false,
        }
    }

    fn handle_copy_mode_mouse(&mut self, event: MouseEvent) -> bool {
        let mut shared = self.shared.lock();
        if shared.copy_mode.is_none() {
            return false;
        }
        let step = shared.alternate_screen_scroll.step as isize;
        match event.kind {
            MouseEventKind::ScrollUp => {
                let _ = shared.scroll_copy_mode_view(step);
            }
            MouseEventKind::ScrollDown => {
                let _ = shared.scroll_copy_mode_view(-step);
            }
            _ => {}
        }
        true
    }
}

impl Default for TerminalEmulator {
    fn default() -> Self {
        Self::new()
    }
}

impl ::atto_ui::composable::Component for TerminalEmulator {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            return;
        }
        if let Some(process) = &mut self.process {
            process.record_exit_if_ready(&self.shared);
        }

        let (rows, cols) = (area.height, area.width);
        self.resize(rows, cols);

        let mut shared = self.shared.lock();
        if !ctx.is_focused {
            // Losing focus (e.g. a modal popup opened) auto-releases keyboard
            // capture. Remember that this was a transient, focus-driven release
            // so it can be restored when focus returns — unlike an intentional
            // release via the release shortcut.
            if shared.capture {
                shared.set_capture(false);
                shared.capture_suspended_by_blur = true;
            }
        } else if shared.capture_suspended_by_blur {
            // Focus came back after a focus-driven release: restore capture so
            // the keyboard keeps working without requiring a click.
            shared.set_capture(true);
            shared.capture_suspended_by_blur = false;
        }
        let selection_range = shared.selection.range();
        let copy_mode_cursor = shared.copy_mode.as_ref().map(|mode| mode.cursor);
        let command_blocks = if self.command_block_presentation.is_enabled() {
            shared.command_marks.clone()
        } else {
            Vec::new()
        };
        let max_scrollback = shared.max_scrollback();
        let cursor_shape = shared.cursor_shape;
        let palette = shared.palette.clone();
        let screen = shared.parser.screen_mut();
        let visible_top = visible_top_row(max_scrollback, screen.scrollback());

        let base_style = ctx.theme.window_bg;
        let base_fg = palette.foreground.or(base_style.fg);
        let base_bg = palette.background.or(base_style.bg);
        let command_output_style = command_output_style(ctx.theme);
        let command_separator_style = command_separator_style(ctx.theme);
        let command_failure_style = command_failure_style(ctx.theme);

        let buf = frame.buffer_mut();
        for y in 0..area.height {
            let absolute_row = visible_top.saturating_add(usize::from(y));
            let row_presentation = command_row_presentation(&command_blocks, absolute_row);
            let separator_start = if row_presentation.separator {
                command_separator_start(screen, y, area.width)
            } else {
                area.width
            };
            let selected_ranges = selection_range
                .map(|range| {
                    selected_cell_ranges_for_screen_row(screen, y, absolute_row, area.width, range)
                })
                .unwrap_or_default();
            for x in 0..area.width {
                let cell = screen.cell(y, x);
                let mut is_wide_cont = cell.is_some_and(vt100::Cell::is_wide_continuation);
                let mut symbol = cell
                    .map(|c| {
                        if c.is_wide_continuation() || c.contents().is_empty() {
                            " "
                        } else {
                            c.contents()
                        }
                    })
                    .unwrap_or(" ");

                let style = cell
                    .map(|c| cell_style(c, base_fg, base_bg, &palette))
                    .unwrap_or(base_style);
                let style = if row_presentation.output {
                    style.patch(command_output_style)
                } else {
                    style
                };
                let style = if row_presentation.separator && x >= separator_start && !is_wide_cont {
                    symbol = COMMAND_SEPARATOR_SYMBOL;
                    style.patch(command_separator_style)
                } else {
                    style
                };
                let style = if row_presentation.failed_marker && x == area.width.saturating_sub(1) {
                    symbol = COMMAND_FAILURE_SYMBOL;
                    is_wide_cont = false;
                    style.patch(command_failure_style)
                } else {
                    style
                };
                let style = if selected_ranges
                    .iter()
                    .any(|(start, end)| x >= *start && x < *end)
                {
                    ctx.theme.selection
                } else {
                    style
                };
                let style = if copy_mode_cursor.is_some_and(|cursor| {
                    absolute_row == cursor.row && x == cursor.col.min(area.width.saturating_sub(1))
                }) {
                    style.add_modifier(Modifier::REVERSED)
                } else {
                    style
                };

                let dst_x = area.x.saturating_add(x);
                let dst_y = area.y.saturating_add(y);
                if let Some(dst) = buf.cell_mut((dst_x, dst_y)) {
                    dst.set_symbol(symbol);
                    dst.set_style(style);
                    dst.set_skip(is_wide_cont);
                }
            }
        }

        if !screen.hide_cursor() && screen.scrollback() == 0 {
            let (cur_row, cur_col) = screen.cursor_position();
            if cur_row < area.height && cur_col < area.width {
                let dst_x = area.x.saturating_add(cur_col);
                let dst_y = area.y.saturating_add(cur_row);
                if let Some(dst) = buf.cell_mut((dst_x, dst_y)) {
                    apply_cursor_shape(dst, cursor_shape);
                }
            }
        }
    }
}

impl ::atto_ui::composable::DragAndDrop for TerminalEmulator {}

impl ::atto_ui::composable::Layout for TerminalEmulator {
    fn min_width(&self) -> u16 {
        1
    }

    fn min_height(&self) -> u16 {
        1
    }
}

impl ::atto_ui::composable::Scrollable for TerminalEmulator {
    fn is_scrollable(&self) -> bool {
        let mut shared = self.shared.lock();
        let max = shared.max_scrollback();
        max > 0
    }

    fn content_size(&self) -> (u16, u16) {
        let mut shared = self.shared.lock();
        let (rows, cols) = shared.parser.screen().size();
        let max = shared.max_scrollback().min(u16::MAX as usize);
        let height = rows.saturating_add(max as u16);
        (cols, height)
    }

    fn viewport_size(&self) -> (u16, u16) {
        let shared = self.shared.lock();
        let (rows, cols) = shared.parser.screen().size();
        (cols, rows)
    }

    fn scroll_config(&self) -> ScrollConfig {
        ScrollConfig::default()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        let mut shared = self.shared.lock();
        let max = shared.max_scrollback().min(u16::MAX as usize);
        let offset = shared.scrollback_offset().min(max);
        let y = max.saturating_sub(offset) as u16;
        (0, y)
    }

    fn set_scroll_offset(&mut self, _x: u16, y: u16) {
        let mut shared = self.shared.lock();
        shared.set_scrollback_from_scroll_offset(y);
    }
}

impl ::atto_ui::composable::FocusNav for TerminalEmulator {
    fn is_focusable(&self) -> bool {
        true
    }
}

impl ::atto_ui::composable::DynamicTree for TerminalEmulator {}

impl ::atto_ui::composable::EventHandling for TerminalEmulator {
    fn handle_event_capture(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        let Event::Key(key) = event else {
            return EventResult::ignored();
        };
        let mut shared = self.shared.lock();
        if !shared.capture {
            return EventResult::ignored();
        }
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => match handle_captured_key(&mut shared, *key) {
                CapturedKeyAction::Consumed => EventResult::consumed(),
                CapturedKeyAction::Component(action) => EventResult {
                    outcome: EventOutcome::Consumed,
                    action,
                    capture: Capture::None,
                },
                CapturedKeyAction::Dispatch(bytes) => {
                    drop(shared);
                    dispatch_input(&self.shared, &bytes);
                    EventResult::consumed()
                }
                CapturedKeyAction::SystemClipboardCopy(text) => {
                    drop(shared);
                    dispatch_system_clipboard_copy(&self.shared, &text);
                    EventResult::consumed()
                }
            },
            _ => EventResult::ignored(),
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        match event {
            Event::Key(key) => {
                let mut shared = self.shared.lock();
                if shared.capture {
                    match handle_captured_key(&mut shared, *key) {
                        CapturedKeyAction::Consumed => return EventResult::consumed(),
                        CapturedKeyAction::Component(action) => {
                            return EventResult {
                                outcome: EventOutcome::Consumed,
                                action,
                                capture: Capture::None,
                            };
                        }
                        CapturedKeyAction::Dispatch(bytes) => {
                            drop(shared);
                            dispatch_input(&self.shared, &bytes);
                            return EventResult::consumed();
                        }
                        CapturedKeyAction::SystemClipboardCopy(text) => {
                            drop(shared);
                            dispatch_system_clipboard_copy(&self.shared, &text);
                            return EventResult::consumed();
                        }
                    }
                }
                drop(shared);
                if self.handle_scrollback_key(*key) {
                    return EventResult::consumed();
                }
                EventResult::ignored()
            }
            Event::Paste(text) => {
                let shared = self.shared.lock();
                if !shared.capture {
                    return EventResult::ignored();
                }
                let bytes = encode_paste_text(shared.parser.screen(), text);
                drop(shared);
                dispatch_input(&self.shared, &bytes);
                EventResult::consumed()
            }
            Event::Mouse(m) => {
                let inside =
                    mouse_coords_local(self.last_area, *m, ctx.mouse_coordinate_space).is_some();
                if !inside {
                    return EventResult::ignored();
                }

                let mut shared = self.shared.lock();
                if !shared.capture {
                    if matches!(m.kind, MouseEventKind::Down(_)) && self.capture_on_click {
                        shared.set_capture(true);
                    } else {
                        drop(shared);
                        if self.handle_scrollback_wheel(*m) {
                            return EventResult::consumed();
                        }
                        return EventResult::ignored();
                    }
                }
                drop(shared);

                if self.handle_copy_mode_mouse(*m) {
                    return EventResult::consumed();
                }

                if self.handle_local_mouse_selection(*m, ctx.mouse_coordinate_space) {
                    return EventResult::consumed();
                }

                let shared = self.shared.lock();
                let screen = shared.parser.screen();
                if let Some(bytes) =
                    encode_mouse_event(screen, *m, self.last_area, ctx.mouse_coordinate_space)
                {
                    drop(shared);
                    dispatch_input(&self.shared, &bytes);
                    return EventResult::consumed();
                }
                drop(shared);
                if self.handle_alternate_screen_wheel(*m) {
                    return EventResult::consumed();
                }
                if self.handle_scrollback_wheel(*m) {
                    return EventResult::consumed();
                }
                EventResult::consumed()
            }
            _ => EventResult::ignored(),
        }
    }
}

impl Drop for TerminalEmulator {
    fn drop(&mut self) {
        self.stop_process();
        if let Some(cb) = self.on_close.take() {
            cb();
        }
    }
}

/// Handle for interacting with a [`TerminalEmulator`] from outside the UI tree.
#[derive(Clone)]
pub struct TerminalHandle {
    shared: Arc<Mutex<TerminalShared>>,
}

impl TerminalHandle {
    /// Feeds bytes into the terminal emulator (ANSI output stream).
    pub fn process_output(&self, bytes: &[u8]) {
        let (responses, dispatches) = {
            let mut shared = self.shared.lock();
            let decoded = shared.tmux_dcs_passthrough.decode(bytes);
            shared.parser.process(&decoded);
            let events = shared.parser.callbacks_mut().take_events();
            let responses = collect_dsr_responses(&mut shared, &decoded);
            let dispatches = shared.apply_callback_events(events);
            (responses, dispatches)
        };
        for response in responses {
            forward_input(&self.shared, &response);
        }
        dispatch_terminal_callback_events(&self.shared, dispatches);
    }

    pub fn process_output_str(&self, text: &str) {
        self.process_output(text.as_bytes());
    }

    /// Sends a user input event to the terminal (encoded to bytes).
    pub fn send_event(&self, event: &Event) {
        let shared = self.shared.lock();
        let screen = shared.parser.screen();
        let bytes = match event {
            Event::Key(key) => encode_key_event(screen, *key),
            Event::Paste(text) => Some(encode_paste_text(screen, text)),
            Event::Mouse(m) => encode_mouse_event(screen, *m, None, MouseCoordinateSpace::Absolute),
            _ => None,
        };
        if let Some(bytes) = bytes {
            drop(shared);
            dispatch_input(&self.shared, &bytes);
        }
    }

    /// Pushes raw input bytes to the terminal input stream.
    pub fn send_input_bytes(&self, bytes: &[u8]) {
        dispatch_input(&self.shared, bytes);
    }

    /// Explicitly resizes the parser screen and attached PTY, if a subprocess is running.
    pub fn resize(&self, rows: u16, cols: u16) -> bool {
        resize_terminal(&self.shared, rows, cols)
    }

    /// Returns and clears the queued input bytes.
    pub fn take_input(&self) -> Vec<u8> {
        let mut shared = self.shared.lock();
        let mut out = Vec::with_capacity(shared.input.len());
        while let Some(b) = shared.input.pop_front() {
            out.push(b);
        }
        out
    }

    pub fn set_capture(&self, capture: bool) {
        self.shared.lock().set_capture(capture);
    }

    pub fn capture(&self) -> bool {
        self.shared.lock().capture
    }

    /// Applies a validated terminal configuration to this live terminal instance.
    pub fn apply_config(&self, config: &TerminalConfig) -> Result<()> {
        let runtime_config = TerminalRuntimeConfig::from_config(config)?;
        self.shared.lock().apply_runtime_config(runtime_config);
        Ok(())
    }

    pub fn set_scrollback_len(&self, len: usize) {
        self.shared.lock().set_scrollback_len(len);
    }

    pub fn scrollback_len(&self) -> usize {
        self.shared.lock().scrollback_len
    }

    pub fn set_release_shortcut(&self, shortcut: TerminalShortcut) {
        self.shared.lock().release_shortcut = shortcut;
    }

    pub fn release_shortcut(&self) -> TerminalShortcut {
        self.shared.lock().release_shortcut
    }

    /// Updates the spawn-time shell integration policy.
    pub fn set_shell_integration(&self, integration: TerminalShellIntegration) {
        self.shared.lock().shell_integration = integration;
    }

    /// Returns the current spawn-time shell integration policy.
    pub fn shell_integration(&self) -> TerminalShellIntegration {
        self.shared.lock().shell_integration
    }

    /// Updates tmux-compatible probe variables used by future subprocess spawns.
    pub fn set_tmux_environment(&self, config: TerminalTmuxEnvironmentConfig) {
        self.shared.lock().tmux_environment = config;
    }

    /// Returns the current tmux-compatible probe environment configuration.
    pub fn tmux_environment(&self) -> TerminalTmuxEnvironmentConfig {
        self.shared.lock().tmux_environment.clone()
    }

    /// Returns the last non-fatal shell integration injection error, if any.
    pub fn last_shell_integration_error(&self) -> Option<String> {
        self.shared.lock().last_shell_integration_error.clone()
    }

    /// Returns the latest cwd reported by OSC 7 shell integration, if any.
    pub fn current_cwd(&self) -> Option<String> {
        self.shared.lock().current_cwd.clone()
    }

    /// Returns the cursor shape most recently requested through DECSCUSR.
    pub fn cursor_shape(&self) -> TerminalCursorShape {
        self.shared.lock().cursor_shape
    }

    /// Updates the terminal prefix shortcut. Only plain `Ctrl+<ASCII letter>` is accepted.
    pub fn set_prefix_shortcut(&self, shortcut: TerminalShortcut) -> Result<()> {
        let shortcut = normalize_prefix_shortcut(shortcut)?;
        self.shared.lock().set_prefix_shortcut(shortcut);
        Ok(())
    }

    pub fn prefix_shortcut(&self) -> TerminalShortcut {
        self.shared.lock().prefix_shortcut
    }

    /// Adds or replaces one prefix command binding at runtime.
    pub fn set_prefix_binding(&self, shortcut: TerminalShortcut, command: TerminalPrefixCommand) {
        self.shared
            .lock()
            .set_prefix_binding(TerminalPrefixBinding::new(shortcut, command));
    }

    /// Replaces the full prefix command table at runtime.
    pub fn set_prefix_bindings(&self, bindings: impl IntoIterator<Item = TerminalPrefixBinding>) {
        self.shared.lock().set_prefix_bindings(bindings);
    }

    pub fn prefix_bindings(&self) -> Vec<TerminalPrefixBinding> {
        self.shared.lock().prefix_bindings.clone()
    }

    /// Returns whether copy-mode is currently active.
    pub fn copy_mode(&self) -> bool {
        self.shared.lock().copy_mode.is_some()
    }

    /// Returns the current copy-mode cursor position, if copy-mode is active.
    pub fn copy_mode_cursor(&self) -> Option<TerminalSelectionPosition> {
        self.shared
            .lock()
            .copy_mode
            .as_ref()
            .map(|mode| mode.cursor)
    }

    /// Returns the last text copied into the terminal-local copy buffer.
    pub fn copied_text(&self) -> Option<String> {
        self.shared.lock().copy_buffer.clone()
    }

    /// Copies the active selection into the terminal-local copy buffer.
    pub fn copy_selection(&self) -> Option<String> {
        let text = { self.shared.lock().copy_selection() };
        if let Some(text) = &text {
            dispatch_system_clipboard_copy(&self.shared, text);
        }
        text
    }

    /// Pastes the terminal-local copy buffer back into the subprocess input stream.
    pub fn paste_copied_text(&self) -> bool {
        let bytes = { self.shared.lock().paste_copy_buffer_bytes() };
        let Some(bytes) = bytes else {
            return false;
        };
        dispatch_input(&self.shared, &bytes);
        true
    }

    /// Starts a terminal text selection at an absolute scrollback/screen position.
    pub fn begin_selection(&self, position: TerminalSelectionPosition) {
        self.shared.lock().selection.start(position);
    }

    /// Extends the active terminal text selection to an absolute position.
    pub fn update_selection(&self, position: TerminalSelectionPosition) {
        self.shared.lock().selection.update(position);
    }

    /// Clears the active terminal text selection.
    pub fn clear_selection(&self) -> bool {
        self.shared.lock().selection.clear()
    }

    /// Returns the normalized active terminal text selection range.
    pub fn selection_range(&self) -> Option<TerminalSelectionRange> {
        self.shared.lock().selection.range()
    }

    /// Converts a visible terminal cell into the absolute coordinate used by selections.
    pub fn selection_position_for_view_cell(
        &self,
        row: u16,
        col: u16,
    ) -> TerminalSelectionPosition {
        let mut shared = self.shared.lock();
        let max_scrollback = shared.max_scrollback();
        let screen = shared.parser.screen();
        let (rows, cols) = screen.size();
        position_for_view_cell(max_scrollback, screen.scrollback(), rows, cols, row, col)
    }

    /// Returns text currently covered by the active selection.
    pub fn selected_text(&self) -> Option<String> {
        let mut shared = self.shared.lock();
        shared.selected_text()
    }

    /// Returns the latest OSC 0/2 window title, if one has been observed.
    pub fn window_title(&self) -> Option<String> {
        self.shared.lock().window_title.clone()
    }

    /// Returns the latest OSC 0/1 window icon name, if one has been observed.
    pub fn window_icon_name(&self) -> Option<String> {
        self.shared.lock().window_icon_name.clone()
    }

    /// Returns the number of audible bell requests observed in terminal output.
    pub fn audible_bell_count(&self) -> u64 {
        self.shared.lock().audible_bell_count
    }

    /// Returns the latest OSC 52 clipboard-copy request, if one has been observed.
    pub fn last_clipboard_copy(&self) -> Option<TerminalClipboardCopy> {
        self.shared.lock().last_clipboard_copy.clone()
    }

    /// Returns the last text sent to the configured system clipboard backend.
    pub fn last_system_clipboard_text(&self) -> Option<String> {
        self.shared.lock().last_system_clipboard_text.clone()
    }

    /// Returns the last system clipboard sync error, if the most recent sync failed.
    pub fn last_system_clipboard_error(&self) -> Option<String> {
        self.shared.lock().last_system_clipboard_error.clone()
    }

    /// Returns OSC 133/7 command blocks observed in terminal output.
    pub fn command_blocks(&self) -> Vec<TerminalCommandBlock> {
        self.shared.lock().command_marks.clone()
    }

    /// Returns the command block index covering an absolute terminal position.
    pub fn command_block_index_at_position(
        &self,
        position: TerminalSelectionPosition,
    ) -> Option<usize> {
        self.shared.lock().command_block_index_at_position(position)
    }

    /// Scrolls so the previous command block is visible at the top of the viewport.
    pub fn scroll_to_previous_command_block(&self) -> Option<usize> {
        self.shared.lock().scroll_to_previous_command_block()
    }

    /// Scrolls so the next command block is visible at the top of the viewport.
    pub fn scroll_to_next_command_block(&self) -> Option<usize> {
        self.shared.lock().scroll_to_next_command_block()
    }

    /// Selects the complete output range for a command block.
    pub fn select_command_block_output(&self, index: usize) -> Option<TerminalSelectionRange> {
        self.shared.lock().select_command_block_output(index)
    }

    /// Copies the command text for a command block into the terminal-local copy buffer.
    pub fn copy_command_block_command(&self, index: usize) -> Option<String> {
        let text = {
            self.shared
                .lock()
                .copy_command_block_text(index, CommandBlockTextKind::Command)
        };
        if let Some(text) = &text {
            dispatch_system_clipboard_copy(&self.shared, text);
        }
        text
    }

    /// Copies the output text for a command block into the terminal-local copy buffer.
    pub fn copy_command_block_output(&self, index: usize) -> Option<String> {
        let text = {
            self.shared
                .lock()
                .copy_command_block_text(index, CommandBlockTextKind::Output)
        };
        if let Some(text) = &text {
            dispatch_system_clipboard_copy(&self.shared, text);
        }
        text
    }

    /// Sends a command block's command text back to the subprocess as a new command.
    pub fn rerun_command_block(&self, index: usize) -> bool {
        let bytes = { self.shared.lock().command_block_rerun_bytes(index) };
        let Some(bytes) = bytes else {
            return false;
        };
        dispatch_input(&self.shared, &bytes);
        true
    }

    /// Returns the exit code for the most recently completed command block, if reported.
    pub fn last_exit_code(&self) -> Option<i32> {
        self.shared
            .lock()
            .command_marks
            .iter()
            .rev()
            .find(|block| block.end.is_some())
            .and_then(|block| block.exit_code)
    }

    /// Returns whether a subprocess is currently attached and has not reported exit.
    pub fn is_running(&self) -> bool {
        self.shared.lock().process_running
    }

    /// Returns the last recorded subprocess exit status, if the process has exited.
    pub fn exit_status(&self) -> Option<ExitStatus> {
        self.shared.lock().exit_status.clone()
    }

    /// Snapshot of the terminal contents including scrollback.
    pub fn snapshot(&self) -> TerminalSnapshot {
        let mut shared = self.shared.lock();
        let max_scrollback = shared.max_scrollback();
        let (rows, cols, current_offset) = {
            let screen = shared.parser.screen();
            let (rows, cols) = screen.size();
            let current_offset = screen.scrollback();
            (rows, cols, current_offset)
        };
        let screen = shared.parser.screen_mut();

        let mut lines = Vec::with_capacity(max_scrollback + rows as usize);
        let mut start = 0;
        while start < max_scrollback {
            let offset = max_scrollback - start;
            screen.set_scrollback(offset);
            let chunk: Vec<String> = screen.rows(0, cols).collect();
            let take = (max_scrollback - start).min(rows as usize);
            lines.extend(chunk.into_iter().take(take));
            start += take;
        }

        screen.set_scrollback(0);
        lines.extend(screen.rows(0, cols));

        screen.set_scrollback(current_offset);

        TerminalSnapshot {
            lines,
            cols,
            rows,
            scrollback: max_scrollback,
        }
    }
}

/// Full text snapshot of the terminal contents including scrollback.
#[derive(Clone, Debug)]
pub struct TerminalSnapshot {
    pub lines: Vec<String>,
    pub cols: u16,
    pub rows: u16,
    pub scrollback: usize,
}

impl TerminalSnapshot {
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

fn mouse_coords_local(
    area: Option<Rect>,
    m: MouseEvent,
    coordinate_space: MouseCoordinateSpace,
) -> Option<(u16, u16)> {
    let Some(area) = area else {
        return Some((m.column, m.row));
    };

    if area.width == 0 || area.height == 0 {
        return None;
    }

    match coordinate_space {
        MouseCoordinateSpace::Absolute => {
            if m.column >= area.x
                && m.column < area.x.saturating_add(area.width)
                && m.row >= area.y
                && m.row < area.y.saturating_add(area.height)
            {
                return Some((
                    m.column.saturating_sub(area.x),
                    m.row.saturating_sub(area.y),
                ));
            }
        }
        MouseCoordinateSpace::Local => {
            if m.column < area.width && m.row < area.height {
                return Some((m.column, m.row));
            }
        }
    }

    None
}

fn mouse_selection_position(
    shared: &mut TerminalShared,
    area: Option<Rect>,
    event: MouseEvent,
    coordinate_space: MouseCoordinateSpace,
    include_cell: bool,
) -> Option<TerminalSelectionPosition> {
    let (col, row) = mouse_coords_local(area, event, coordinate_space)?;
    let max_scrollback = shared.max_scrollback();
    let screen = shared.parser.screen();
    let scrollback = screen.scrollback();
    let (rows, cols) = screen.size();
    if rows == 0 || cols == 0 || row >= rows || col >= cols {
        return None;
    }

    let cell_start = position_for_view_cell(max_scrollback, scrollback, rows, cols, row, col);
    let include_right_edge = if include_cell {
        match shared.selection.anchor() {
            Some(anchor) => cell_start >= anchor,
            None => true,
        }
    } else {
        false
    };
    let selection_col = if include_right_edge {
        col.saturating_add(1).min(cols)
    } else {
        col
    };

    Some(position_for_view_cell(
        max_scrollback,
        scrollback,
        rows,
        cols,
        row,
        selection_col,
    ))
}

fn cell_style(
    cell: &vt100::Cell,
    base_fg: Option<Color>,
    base_bg: Option<Color>,
    palette: &TerminalPalette,
) -> Style {
    let mut fg = resolve_color(cell.fgcolor(), base_fg, palette);
    let mut bg = resolve_color(cell.bgcolor(), base_bg, palette);
    if cell.inverse() {
        std::mem::swap(&mut fg, &mut bg);
    }

    let mut style = Style::default();
    if let Some(fg) = fg {
        style = style.fg(fg);
    }
    if let Some(bg) = bg {
        style = style.bg(bg);
    }

    let mut mods = Modifier::empty();
    if cell.bold() {
        mods |= Modifier::BOLD;
    }
    if cell.dim() {
        mods |= Modifier::DIM;
    }
    if cell.italic() {
        mods |= Modifier::ITALIC;
    }
    if cell.underline() {
        mods |= Modifier::UNDERLINED;
    }

    style.add_modifier(mods)
}

fn apply_cursor_shape(cell: &mut Cell, shape: TerminalCursorShape) {
    match shape {
        TerminalCursorShape::Block => {
            cell.set_style(cell.style().add_modifier(Modifier::REVERSED));
        }
        TerminalCursorShape::Underline => {
            cell.set_style(cell.style().add_modifier(Modifier::UNDERLINED));
        }
        TerminalCursorShape::Bar => {
            cell.set_symbol(CURSOR_BAR_SYMBOL);
            cell.set_skip(false);
        }
    }
}

fn resolve_color(
    color: vt100::Color,
    default: Option<Color>,
    palette: &TerminalPalette,
) -> Option<Color> {
    match color {
        vt100::Color::Default => default,
        vt100::Color::Idx(i) => Some(palette.color_for_index(i)),
        vt100::Color::Rgb(r, g, b) => Some(Color::Rgb(r, g, b)),
    }
}

fn encode_key_event(screen: &vt100::Screen, event: KeyEvent) -> Option<Vec<u8>> {
    if event.kind == KeyEventKind::Release {
        return None;
    }

    let mods = event.modifiers;
    let shift = mods.contains(KeyModifiers::SHIFT);
    let alt = mods.contains(KeyModifiers::ALT);
    let ctrl = mods.contains(KeyModifiers::CONTROL);

    let mut out = Vec::new();

    if screen.application_keypad()
        && is_keypad_event(event)
        && let Some(seq) = encode_application_keypad_key(event.code, mods)
    {
        out.extend_from_slice(seq.as_bytes());
        return Some(out);
    }

    match event.code {
        KeyCode::Char(c) => {
            if ctrl {
                if let Some(b) = ctrl_char(c) {
                    out.push(b);
                } else {
                    out.extend_from_slice(c.to_string().as_bytes());
                }
            } else {
                out.extend_from_slice(c.to_string().as_bytes());
            }
            if alt {
                out.insert(0, 0x1b);
            }
        }
        KeyCode::Enter => {
            out.push(b'\r');
            if alt {
                out.insert(0, 0x1b);
            }
        }
        KeyCode::Backspace => {
            out.push(0x7f);
            if alt {
                out.insert(0, 0x1b);
            }
        }
        KeyCode::Tab => {
            if shift {
                out.extend_from_slice(b"\x1b[Z");
            } else {
                out.push(b'\t');
            }
            if alt {
                out.insert(0, 0x1b);
            }
        }
        KeyCode::BackTab => {
            out.extend_from_slice(b"\x1b[Z");
        }
        KeyCode::Esc => {
            out.push(0x1b);
        }
        KeyCode::Up => {
            out.extend_from_slice(encode_cursor_key(screen, 'A', mods).as_bytes());
        }
        KeyCode::Down => {
            out.extend_from_slice(encode_cursor_key(screen, 'B', mods).as_bytes());
        }
        KeyCode::Right => {
            out.extend_from_slice(encode_cursor_key(screen, 'C', mods).as_bytes());
        }
        KeyCode::Left => {
            out.extend_from_slice(encode_cursor_key(screen, 'D', mods).as_bytes());
        }
        KeyCode::Home => {
            out.extend_from_slice(encode_home_end_key(screen, 'H', mods).as_bytes());
        }
        KeyCode::End => {
            out.extend_from_slice(encode_home_end_key(screen, 'F', mods).as_bytes());
        }
        KeyCode::PageUp => {
            out.extend_from_slice(encode_csi_tilde(5, mods).as_bytes());
        }
        KeyCode::PageDown => {
            out.extend_from_slice(encode_csi_tilde(6, mods).as_bytes());
        }
        KeyCode::Insert => {
            out.extend_from_slice(encode_csi_tilde(2, mods).as_bytes());
        }
        KeyCode::Delete => {
            out.extend_from_slice(encode_csi_tilde(3, mods).as_bytes());
        }
        KeyCode::F(n) => {
            if let Some(seq) = encode_function_key(n, mods) {
                out.extend_from_slice(seq.as_bytes());
            }
        }
        _ => return None,
    }

    Some(out)
}

fn is_keypad_event(event: KeyEvent) -> bool {
    event.state.contains(KeyEventState::KEYPAD) || matches!(event.code, KeyCode::KeypadBegin)
}

fn encode_application_keypad_key(code: KeyCode, mods: KeyModifiers) -> Option<&'static str> {
    if modifier_value(mods) != 1 {
        return None;
    }

    match code {
        KeyCode::Char('0') | KeyCode::Insert => Some("\x1bOp"),
        KeyCode::Char('1') | KeyCode::End => Some("\x1bOq"),
        KeyCode::Char('2') | KeyCode::Down => Some("\x1bOr"),
        KeyCode::Char('3') | KeyCode::PageDown => Some("\x1bOs"),
        KeyCode::Char('4') | KeyCode::Left => Some("\x1bOt"),
        KeyCode::Char('5') => Some("\x1bOu"),
        KeyCode::Char('6') | KeyCode::Right => Some("\x1bOv"),
        KeyCode::Char('7') | KeyCode::Home => Some("\x1bOw"),
        KeyCode::Char('8') | KeyCode::Up => Some("\x1bOx"),
        KeyCode::Char('9') | KeyCode::PageUp => Some("\x1bOy"),
        KeyCode::Char('*') => Some("\x1bOj"),
        KeyCode::Char('+') => Some("\x1bOk"),
        KeyCode::Char(',') => Some("\x1bOl"),
        KeyCode::Char('-') => Some("\x1bOm"),
        KeyCode::Char('.') | KeyCode::Delete => Some("\x1bOn"),
        KeyCode::Char('/') => Some("\x1bOo"),
        KeyCode::Enter => Some("\x1bOM"),
        KeyCode::Char('=') => Some("\x1bOX"),
        KeyCode::KeypadBegin => Some("\x1bOE"),
        _ => None,
    }
}

fn encode_cursor_key(screen: &vt100::Screen, suffix: char, mods: KeyModifiers) -> String {
    let mod_value = modifier_value(mods);
    if mod_value == 1 {
        if screen.application_cursor() {
            format!("\x1bO{suffix}")
        } else {
            format!("\x1b[{suffix}")
        }
    } else {
        format!("\x1b[1;{mod_value}{suffix}")
    }
}

fn encode_home_end_key(screen: &vt100::Screen, suffix: char, mods: KeyModifiers) -> String {
    let mod_value = modifier_value(mods);
    if mod_value == 1 {
        if screen.application_cursor() {
            format!("\x1bO{suffix}")
        } else {
            format!("\x1b[{suffix}")
        }
    } else {
        format!("\x1b[1;{mod_value}{suffix}")
    }
}

fn encode_csi_tilde(n: u8, mods: KeyModifiers) -> String {
    let mod_value = modifier_value(mods);
    if mod_value == 1 {
        format!("\x1b[{n}~")
    } else {
        format!("\x1b[{n};{mod_value}~")
    }
}

fn encode_function_key(n: u8, mods: KeyModifiers) -> Option<String> {
    let mod_value = modifier_value(mods);
    let seq = match n {
        1 => {
            if mod_value == 1 {
                "\x1bOP".to_string()
            } else {
                format!("\x1b[1;{mod_value}P")
            }
        }
        2 => {
            if mod_value == 1 {
                "\x1bOQ".to_string()
            } else {
                format!("\x1b[1;{mod_value}Q")
            }
        }
        3 => {
            if mod_value == 1 {
                "\x1bOR".to_string()
            } else {
                format!("\x1b[1;{mod_value}R")
            }
        }
        4 => {
            if mod_value == 1 {
                "\x1bOS".to_string()
            } else {
                format!("\x1b[1;{mod_value}S")
            }
        }
        5 => encode_csi_tilde(15, mods),
        6 => encode_csi_tilde(17, mods),
        7 => encode_csi_tilde(18, mods),
        8 => encode_csi_tilde(19, mods),
        9 => encode_csi_tilde(20, mods),
        10 => encode_csi_tilde(21, mods),
        11 => encode_csi_tilde(23, mods),
        12 => encode_csi_tilde(24, mods),
        _ => return None,
    };
    Some(seq)
}

fn modifier_value(mods: KeyModifiers) -> u8 {
    let mut value = 1;
    if mods.contains(KeyModifiers::SHIFT) {
        value += 1;
    }
    if mods.contains(KeyModifiers::ALT) {
        value += 2;
    }
    if mods.contains(KeyModifiers::CONTROL) {
        value += 4;
    }
    value
}

fn ctrl_char(c: char) -> Option<u8> {
    let c = c.to_ascii_uppercase();
    match c {
        '@' => Some(0),
        'A'..='Z' => Some((c as u8) - b'A' + 1),
        '[' => Some(27),
        '\\' => Some(28),
        ']' => Some(29),
        '^' => Some(30),
        '_' => Some(31),
        '?' => Some(127),
        _ => None,
    }
}

fn encode_mouse_event(
    screen: &vt100::Screen,
    event: MouseEvent,
    area: Option<Rect>,
    coordinate_space: MouseCoordinateSpace,
) -> Option<Vec<u8>> {
    if matches!(screen.mouse_protocol_mode(), vt100::MouseProtocolMode::None) {
        return None;
    }

    let (col, row) = mouse_coords_local(area, event, coordinate_space)?;
    let (rows, cols) = screen.size();
    if row >= rows || col >= cols {
        return None;
    }

    let cb = match event.kind {
        MouseEventKind::Down(button) => match button {
            MouseButton::Left => Some(0),
            MouseButton::Middle => Some(1),
            MouseButton::Right => Some(2),
        },
        MouseEventKind::Up(button) => match screen.mouse_protocol_mode() {
            vt100::MouseProtocolMode::PressRelease
            | vt100::MouseProtocolMode::ButtonMotion
            | vt100::MouseProtocolMode::AnyMotion => match button {
                MouseButton::Left => Some(0),
                MouseButton::Middle => Some(1),
                MouseButton::Right => Some(2),
            },
            _ => None,
        },
        MouseEventKind::Drag(button) => match screen.mouse_protocol_mode() {
            vt100::MouseProtocolMode::ButtonMotion | vt100::MouseProtocolMode::AnyMotion => {
                match button {
                    MouseButton::Left => Some(32),
                    MouseButton::Middle => Some(33),
                    MouseButton::Right => Some(34),
                }
            }
            _ => None,
        },
        MouseEventKind::Moved => match screen.mouse_protocol_mode() {
            vt100::MouseProtocolMode::AnyMotion => Some(35),
            _ => None,
        },
        MouseEventKind::ScrollUp => Some(64),
        MouseEventKind::ScrollDown => Some(65),
        MouseEventKind::ScrollLeft => Some(66),
        MouseEventKind::ScrollRight => Some(67),
    }?;

    let mut modifier_bits: u16 = 0;
    if event.modifiers.contains(KeyModifiers::SHIFT) {
        modifier_bits += 4;
    }
    if event.modifiers.contains(KeyModifiers::ALT) {
        modifier_bits += 8;
    }
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        modifier_bits += 16;
    }
    let cb = cb + modifier_bits;

    let x = col.saturating_add(1);
    let y = row.saturating_add(1);

    match screen.mouse_protocol_encoding() {
        vt100::MouseProtocolEncoding::Sgr => {
            let suffix = match event.kind {
                MouseEventKind::Up(_) => 'm',
                _ => 'M',
            };
            let seq = format!("\x1b[<{cb};{x};{y}{suffix}");
            Some(seq.into_bytes())
        }
        vt100::MouseProtocolEncoding::Utf8 | vt100::MouseProtocolEncoding::Default => {
            let cb = if matches!(event.kind, MouseEventKind::Up(_)) {
                3 + modifier_bits
            } else {
                cb
            };
            let cb = (cb + 32).min(255);
            let x = (x + 32).min(255);
            let y = (y + 32).min(255);
            let mut seq = Vec::with_capacity(6);
            seq.extend_from_slice(b"\x1b[M");
            seq.push(cb as u8);
            seq.push(x as u8);
            seq.push(y as u8);
            Some(seq)
        }
    }
}
