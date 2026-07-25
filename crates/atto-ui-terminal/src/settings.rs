//! Visual settings editor for terminal configuration.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use atto_ui::composable::{
    Component, ComponentContext, ComponentNode, DragAndDrop, DynamicTree, EdgeInsets,
    EventHandling, EventResult, FocusNav, Grid, HStack, Layout, LayoutParams, ScrollConfig,
    Scrollable, Size, Text, VStack,
};
use atto_ui::reactive::Binding;
use atto_ui::widgets::{Button, Checkbox, Label, ListBox, RadioGroup, TextBox};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::{
    TerminalAlternateScreenScrollConfig, TerminalColorSpec, TerminalConfig, TerminalCursorConfig,
    TerminalCursorShapeConfig, TerminalPaletteConfig, TerminalProfileConfig,
    TerminalSessionsConfig, TerminalShellIntegrationConfig, TerminalShortcutConfig,
    TerminalShortcutModifier, TerminalTmuxEnvironmentConfig,
};

const PALETTE_LEN: usize = 16;
const CURSOR_SHAPES: [&str; 3] = ["Block", "Underline", "Bar"];

/// Returns the default path used by the terminal settings window for persistence.
pub fn default_terminal_config_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("ATTO_UI_TERMINAL_CONFIG")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }

    if let Some(path) = env::var_os("XDG_CONFIG_HOME")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path).join("atto-ui").join("terminal.yaml"));
    }

    env::var_os("HOME").and_then(|home| {
        (!home.is_empty()).then(|| {
            PathBuf::from(home)
                .join(".config")
                .join("atto-ui")
                .join("terminal.yaml")
        })
    })
}

/// Loads a terminal config from `path` when it exists, otherwise returns defaults.
pub fn load_terminal_config_or_default(path: Option<&Path>) -> Result<TerminalConfig> {
    match path {
        Some(path) if path.exists() => TerminalConfig::load_path(path),
        _ => Ok(TerminalConfig::default()),
    }
}

/// Editable, string-oriented representation of [`TerminalConfig`] used by the settings UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSettingsDraft {
    pub scrollback_len: String,
    pub prefix_key: String,
    pub release_shortcut: String,
    pub alternate_screen_scroll_enabled: bool,
    pub alternate_screen_scroll_step: String,
    pub alternate_screen_scroll_up_key: String,
    pub alternate_screen_scroll_down_key: String,
    pub palette_foreground: String,
    pub palette_background: String,
    pub palette_ansi: [String; PALETTE_LEN],
    pub profile_name: String,
    pub profile_command: String,
    pub profile_args_json: String,
    pub profile_cwd: String,
    pub preserved_profiles: Vec<TerminalProfileConfig>,
    pub shell_integration_inject: bool,
    pub tmux: TerminalTmuxEnvironmentConfig,
    pub close_window_on_shell_exit: bool,
    pub cursor_shape: TerminalCursorShapeConfig,
}

impl TerminalSettingsDraft {
    pub fn from_config(config: &TerminalConfig) -> Self {
        let default_profile = config
            .sessions
            .default_profile()
            .or_else(|| config.sessions.profiles.first())
            .cloned()
            .unwrap_or_else(TerminalProfileConfig::shell_from_env);
        let preserved_profiles = config
            .sessions
            .profiles
            .iter()
            .filter(|profile| profile.name != default_profile.name)
            .cloned()
            .collect();

        Self {
            scrollback_len: config.scrollback_len.to_string(),
            prefix_key: prefix_key_text(&config.prefix_key),
            release_shortcut: shortcut_text(&config.release_shortcut),
            alternate_screen_scroll_enabled: config.alternate_screen_scroll.enabled,
            alternate_screen_scroll_step: config.alternate_screen_scroll.step.to_string(),
            alternate_screen_scroll_up_key: shortcut_text(
                &config.alternate_screen_scroll.scroll_up_key,
            ),
            alternate_screen_scroll_down_key: shortcut_text(
                &config.alternate_screen_scroll.scroll_down_key,
            ),
            palette_foreground: optional_color_text(config.palette.foreground.as_ref()),
            palette_background: optional_color_text(config.palette.background.as_ref()),
            palette_ansi: std::array::from_fn(|idx| config.palette.ansi[idx].as_str().to_string()),
            profile_name: default_profile.name,
            profile_command: default_profile.command,
            profile_args_json: serde_json::to_string(&default_profile.args)
                .unwrap_or_else(|_| "[]".to_string()),
            profile_cwd: default_profile
                .cwd
                .map(|cwd| cwd.to_string_lossy().into_owned())
                .unwrap_or_default(),
            preserved_profiles,
            shell_integration_inject: config.shell_integration.inject,
            tmux: config.tmux.clone(),
            close_window_on_shell_exit: config.close_window_on_shell_exit,
            cursor_shape: config.cursor.default_shape,
        }
    }

    pub fn to_config(&self) -> Result<TerminalConfig> {
        let scrollback_len = parse_positive_usize(&self.scrollback_len, "scrollback_len")?;
        let alt_step = parse_positive_u16(
            &self.alternate_screen_scroll_step,
            "alternate screen scroll step",
        )?;
        // `palette_ansi` is a fixed `[String; PALETTE_LEN]`, so build the output
        // array directly — no fallible length check needed.
        let ansi: [TerminalColorSpec; PALETTE_LEN] =
            std::array::from_fn(|idx| TerminalColorSpec::new(self.palette_ansi[idx].trim()));

        let profile_name = self.profile_name.trim();
        let profile_command = self.profile_command.trim();
        ensure!(!profile_name.is_empty(), "profile name must not be empty");
        ensure!(
            !profile_command.is_empty(),
            "profile command must not be empty"
        );
        let profile_args = parse_profile_args(&self.profile_args_json)?;
        let mut profile = TerminalProfileConfig::new(profile_name, profile_command, profile_args);
        if !self.profile_cwd.trim().is_empty() {
            profile.cwd = Some(PathBuf::from(self.profile_cwd.trim()));
        }

        let mut profiles = vec![profile.clone()];
        profiles.extend(
            self.preserved_profiles
                .iter()
                .filter(|preserved| preserved.name != profile.name)
                .cloned(),
        );

        let config = TerminalConfig {
            scrollback_len,
            palette: TerminalPaletteConfig {
                foreground: optional_color_config(&self.palette_foreground),
                background: optional_color_config(&self.palette_background),
                ansi,
            },
            prefix_key: parse_prefix_key(&self.prefix_key).context("parse prefix key")?,
            release_shortcut: parse_shortcut_text(&self.release_shortcut)
                .context("parse release shortcut")?,
            alternate_screen_scroll: TerminalAlternateScreenScrollConfig {
                enabled: self.alternate_screen_scroll_enabled,
                step: alt_step,
                scroll_up_key: parse_shortcut_text(&self.alternate_screen_scroll_up_key)
                    .context("parse alternate scroll up key")?,
                scroll_down_key: parse_shortcut_text(&self.alternate_screen_scroll_down_key)
                    .context("parse alternate scroll down key")?,
            },
            sessions: TerminalSessionsConfig {
                default_profile: profile.name,
                profiles,
            },
            shell_integration: TerminalShellIntegrationConfig {
                inject: self.shell_integration_inject,
            },
            tmux: self.tmux.clone(),
            close_window_on_shell_exit: self.close_window_on_shell_exit,
            cursor: TerminalCursorConfig {
                default_shape: self.cursor_shape,
            },
        };
        config.validate()?;
        Ok(config)
    }
}

#[derive(Clone)]
struct TerminalSettingsBindings {
    scrollback_len: Binding<String>,
    prefix_key: Binding<String>,
    release_shortcut: Binding<String>,
    alt_scroll_enabled: Binding<bool>,
    alt_scroll_step: Binding<String>,
    alt_scroll_up_key: Binding<String>,
    alt_scroll_down_key: Binding<String>,
    palette_foreground: Binding<String>,
    palette_background: Binding<String>,
    palette_ansi: [Binding<String>; PALETTE_LEN],
    palette_items: Binding<Vec<String>>,
    selected_palette_index: Binding<usize>,
    selected_palette_value: Binding<String>,
    /// Palette index whose value currently lives in `selected_palette_value`.
    /// Used to detect selection changes so the outgoing edit is committed and
    /// the incoming value is loaded into the editor.
    committed_palette_index: Binding<usize>,
    profile_name: Binding<String>,
    profile_command: Binding<String>,
    profile_args_json: Binding<String>,
    profile_cwd: Binding<String>,
    preserved_profiles: Binding<Vec<TerminalProfileConfig>>,
    shell_integration_inject: Binding<bool>,
    tmux: Binding<TerminalTmuxEnvironmentConfig>,
    close_window_on_shell_exit: Binding<bool>,
    cursor_shape_index: Binding<usize>,
}

impl TerminalSettingsBindings {
    fn from_config(config: &TerminalConfig) -> Self {
        let draft = TerminalSettingsDraft::from_config(config);
        Self::from_draft(draft)
    }

    fn from_draft(draft: TerminalSettingsDraft) -> Self {
        let palette_items = Binding::new(palette_items_for(&draft.palette_ansi));
        let selected_palette_value = Binding::new(draft.palette_ansi[0].clone());
        Self {
            scrollback_len: Binding::new(draft.scrollback_len),
            prefix_key: Binding::new(draft.prefix_key),
            release_shortcut: Binding::new(draft.release_shortcut),
            alt_scroll_enabled: Binding::new(draft.alternate_screen_scroll_enabled),
            alt_scroll_step: Binding::new(draft.alternate_screen_scroll_step),
            alt_scroll_up_key: Binding::new(draft.alternate_screen_scroll_up_key),
            alt_scroll_down_key: Binding::new(draft.alternate_screen_scroll_down_key),
            palette_foreground: Binding::new(draft.palette_foreground),
            palette_background: Binding::new(draft.palette_background),
            palette_ansi: std::array::from_fn(|idx| Binding::new(draft.palette_ansi[idx].clone())),
            palette_items,
            selected_palette_index: Binding::new(0),
            selected_palette_value,
            committed_palette_index: Binding::new(0),
            profile_name: Binding::new(draft.profile_name),
            profile_command: Binding::new(draft.profile_command),
            profile_args_json: Binding::new(draft.profile_args_json),
            profile_cwd: Binding::new(draft.profile_cwd),
            preserved_profiles: Binding::new(draft.preserved_profiles),
            shell_integration_inject: Binding::new(draft.shell_integration_inject),
            tmux: Binding::new(draft.tmux),
            close_window_on_shell_exit: Binding::new(draft.close_window_on_shell_exit),
            cursor_shape_index: Binding::new(cursor_shape_index(draft.cursor_shape)),
        }
    }

    /// Reconciles the single "Color" editor with the per-index palette store.
    ///
    /// The palette UI edits one entry at a time through `selected_palette_value`
    /// while the ListBox drives `selected_palette_index`. This commits the
    /// currently-edited value back into `palette_ansi[committed]` every frame,
    /// and when the selection changes it loads the newly-selected index's value
    /// into the editor. Without this, only the last-selected entry would ever be
    /// saved and edits to every other index would be silently lost.
    ///
    /// Returns `true` when the selection changed (so callers can refresh derived
    /// state).
    fn reconcile_palette_selection(&self) -> bool {
        let committed = self
            .committed_palette_index
            .get()
            .min(PALETTE_LEN.saturating_sub(1));
        // Always flush the in-progress edit into the entry it belongs to.
        self.palette_ansi[committed].set(self.selected_palette_value.get());

        let selected = self
            .selected_palette_index
            .get()
            .min(PALETTE_LEN.saturating_sub(1));
        if selected == committed {
            return false;
        }
        // Selection moved: load the newly-selected entry into the editor.
        self.selected_palette_value
            .set(self.palette_ansi[selected].get());
        self.committed_palette_index.set(selected);
        true
    }

    fn to_draft(&self) -> TerminalSettingsDraft {
        // Ensure the in-progress edit is reflected before snapshotting.
        self.reconcile_palette_selection();
        let palette_ansi = std::array::from_fn(|idx| self.palette_ansi[idx].get());

        TerminalSettingsDraft {
            scrollback_len: self.scrollback_len.get(),
            prefix_key: self.prefix_key.get(),
            release_shortcut: self.release_shortcut.get(),
            alternate_screen_scroll_enabled: self.alt_scroll_enabled.get(),
            alternate_screen_scroll_step: self.alt_scroll_step.get(),
            alternate_screen_scroll_up_key: self.alt_scroll_up_key.get(),
            alternate_screen_scroll_down_key: self.alt_scroll_down_key.get(),
            palette_foreground: self.palette_foreground.get(),
            palette_background: self.palette_background.get(),
            palette_ansi,
            profile_name: self.profile_name.get(),
            profile_command: self.profile_command.get(),
            profile_args_json: self.profile_args_json.get(),
            profile_cwd: self.profile_cwd.get(),
            preserved_profiles: self.preserved_profiles.get(),
            shell_integration_inject: self.shell_integration_inject.get(),
            tmux: self.tmux.get(),
            close_window_on_shell_exit: self.close_window_on_shell_exit.get(),
            cursor_shape: cursor_shape_from_index(self.cursor_shape_index.get()),
        }
    }

    fn load_config(&self, config: &TerminalConfig) {
        let draft = TerminalSettingsDraft::from_config(config);
        self.scrollback_len.set(draft.scrollback_len);
        self.prefix_key.set(draft.prefix_key);
        self.release_shortcut.set(draft.release_shortcut);
        self.alt_scroll_enabled
            .set(draft.alternate_screen_scroll_enabled);
        self.alt_scroll_step.set(draft.alternate_screen_scroll_step);
        self.alt_scroll_up_key
            .set(draft.alternate_screen_scroll_up_key);
        self.alt_scroll_down_key
            .set(draft.alternate_screen_scroll_down_key);
        self.palette_foreground.set(draft.palette_foreground);
        self.palette_background.set(draft.palette_background);
        for (idx, value) in draft.palette_ansi.iter().enumerate() {
            self.palette_ansi[idx].set(value.clone());
        }
        self.palette_items
            .set(palette_items_for(&draft.palette_ansi));
        self.selected_palette_index.set(0);
        self.selected_palette_value
            .set(draft.palette_ansi[0].clone());
        self.committed_palette_index.set(0);
        self.profile_name.set(draft.profile_name);
        self.profile_command.set(draft.profile_command);
        self.profile_args_json.set(draft.profile_args_json);
        self.profile_cwd.set(draft.profile_cwd);
        self.preserved_profiles.set(draft.preserved_profiles);
        self.shell_integration_inject
            .set(draft.shell_integration_inject);
        self.tmux.set(draft.tmux);
        self.close_window_on_shell_exit
            .set(draft.close_window_on_shell_exit);
        self.cursor_shape_index
            .set(cursor_shape_index(draft.cursor_shape));
    }
}

/// Shared handle for applying, saving, and testing a terminal settings window.
#[derive(Clone)]
pub struct TerminalSettingsHandle {
    bindings: TerminalSettingsBindings,
    applied_config: Binding<TerminalConfig>,
    status: Binding<String>,
    save_path: Option<PathBuf>,
}

impl TerminalSettingsHandle {
    pub fn draft(&self) -> TerminalSettingsDraft {
        self.bindings.to_draft()
    }

    pub fn applied_config(&self) -> TerminalConfig {
        self.applied_config.get()
    }

    pub fn status_text(&self) -> String {
        self.status.get()
    }

    pub fn set_scrollback_len_text(&self, value: impl Into<String>) {
        self.bindings.scrollback_len.set(value.into());
    }

    pub fn set_prefix_key_text(&self, value: impl Into<String>) {
        self.bindings.prefix_key.set(value.into());
    }

    pub fn set_palette_color_text(&self, index: usize, value: impl Into<String>) {
        let index = index.min(PALETTE_LEN.saturating_sub(1));
        let value = value.into();
        self.bindings.palette_ansi[index].set(value.clone());
        if self.bindings.selected_palette_index.get() == index {
            self.bindings.selected_palette_value.set(value);
        }
    }

    pub fn set_cursor_shape(&self, shape: TerminalCursorShapeConfig) {
        self.bindings
            .cursor_shape_index
            .set(cursor_shape_index(shape));
    }

    pub fn set_close_window_on_shell_exit(&self, enabled: bool) {
        self.bindings.close_window_on_shell_exit.set(enabled);
    }

    pub fn apply(&self) -> Result<TerminalConfig> {
        match self.draft().to_config() {
            Ok(config) => {
                self.applied_config.set(config.clone());
                self.status.set(format!(
                    "Applied: scrollback={} prefix={}",
                    config.scrollback_len,
                    prefix_key_text(&config.prefix_key)
                ));
                Ok(config)
            }
            Err(error) => {
                self.status
                    .set(format!("Error: {}", first_error_line(&error)));
                Err(error)
            }
        }
    }

    pub fn save(&self) -> Result<TerminalConfig> {
        // Validate and persist to disk *before* mutating the live config, so a
        // failed write (permissions, read-only FS, full disk) never leaves the
        // running terminal config diverged from what is on disk.
        let config = match self.draft().to_config() {
            Ok(config) => config,
            Err(error) => {
                self.status
                    .set(format!("Error: {}", first_error_line(&error)));
                return Err(error);
            }
        };
        let path = self
            .save_path
            .as_ref()
            .context("terminal settings save path is not configured")?;
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && let Err(error) = fs::create_dir_all(parent)
                .with_context(|| format!("create terminal config directory {}", parent.display()))
        {
            self.status
                .set(format!("Error: {}", first_error_line(&error)));
            return Err(error);
        }
        if let Err(error) = config.save_path_infer(path) {
            self.status
                .set(format!("Error: {}", first_error_line(&error)));
            return Err(error);
        }

        // Only now that the file is safely written do we adopt it as the live
        // config.
        self.applied_config.set(config.clone());
        self.status
            .set(format!("Saved terminal config to {}", path.display()));
        Ok(config)
    }

    pub fn reset_from_applied(&self) {
        let config = self.applied_config.get();
        self.bindings.load_config(&config);
        self.status
            .set("Reset draft from applied config".to_string());
    }

    fn preview_text(&self) -> String {
        match self.draft().to_config() {
            Ok(config) => format!(
                "Preview: scrollback={} prefix={} cursor={} profile={} alt-scroll={}x{} close-on-exit={}",
                config.scrollback_len,
                prefix_key_text(&config.prefix_key),
                cursor_shape_text(config.cursor.default_shape),
                config.sessions.default_profile,
                if config.alternate_screen_scroll.enabled {
                    "on"
                } else {
                    "off"
                },
                config.alternate_screen_scroll.step,
                if config.close_window_on_shell_exit {
                    "on"
                } else {
                    "off"
                }
            ),
            Err(error) => format!("Preview error: {}", first_error_line(&error)),
        }
    }
}

/// Declarative settings panel for editing [`TerminalConfig`].
pub struct TerminalSettingsView {
    handle: TerminalSettingsHandle,
    root: VStack,
}

impl TerminalSettingsView {
    pub fn new(config: Binding<TerminalConfig>, save_path: Option<PathBuf>) -> Self {
        let handle = TerminalSettingsHandle {
            bindings: TerminalSettingsBindings::from_config(&config.get()),
            applied_config: config,
            status: Binding::new("Edit settings, then Apply or Save.".to_string()),
            save_path,
        };
        let root = build_settings_root(&handle);
        Self { handle, root }
    }

    pub fn from_config(config: TerminalConfig) -> Self {
        Self::new(Binding::new(config), None)
    }

    pub fn handle(&self) -> TerminalSettingsHandle {
        self.handle.clone()
    }

    fn refresh_palette_items(&self) {
        // Commit the in-progress palette edit and load the newly-selected entry
        // before recomputing the list labels, so the ListBox shows live values
        // and no edit is dropped when the selection moves.
        self.handle.bindings.reconcile_palette_selection();
        let palette_ansi: [String; PALETTE_LEN] =
            std::array::from_fn(|idx| self.handle.bindings.palette_ansi[idx].get());
        self.handle
            .bindings
            .palette_items
            .set(palette_items_for(&palette_ansi));
    }
}

impl Component for TerminalSettingsView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.refresh_palette_items();
        self.root.draw(frame, area, ctx);
    }
}

impl Layout for TerminalSettingsView {
    fn min_width(&self) -> u16 {
        40
    }

    fn min_height(&self) -> u16 {
        12
    }

    fn desired_width(&self) -> Option<u16> {
        Some(72)
    }

    fn desired_height(&self) -> Option<u16> {
        Some(24)
    }
}

impl Scrollable for TerminalSettingsView {
    fn is_scrollable(&self) -> bool {
        self.root.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.root.content_size()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.root.scroll_offset()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.root.viewport_size()
    }

    fn scroll_config(&self) -> ScrollConfig {
        Scrollable::scroll_config(&self.root)
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.root.set_scroll_offset(x, y);
    }

    fn scroll_to(&mut self, x: u16, y: u16) {
        self.root.scroll_to(x, y);
    }
}

impl FocusNav for TerminalSettingsView {
    fn focused_child(&self) -> Option<atto_ui::composable::ComponentId> {
        self.root.focused_child()
    }

    fn focus_first(&mut self) -> bool {
        self.root.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.root.focus_last()
    }
}

impl DynamicTree for TerminalSettingsView {
    fn children(&self) -> &[ComponentNode] {
        self.root.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        self.root.children_mut()
    }
}

impl EventHandling for TerminalSettingsView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            return EventResult::close_window();
        }
        self.root.handle_event(event, ctx)
    }
}

impl DragAndDrop for TerminalSettingsView {}

fn build_settings_root(handle: &TerminalSettingsHandle) -> VStack {
    let row = content_row();
    VStack::new()
        .padding_insets(EdgeInsets::all(1))
        .spacing(1)
        .scrollable(true)
        .child_with_layout(Text::new("Terminal Settings"), row)
        .child_with_layout(Text::from_fn(preview_fn(handle)), row)
        .child_with_layout(general_section(handle), row)
        .child_with_layout(palette_section(handle), row)
        .child_with_layout(session_section(handle), row)
        .child_with_layout(button_row(handle), row)
        .child_with_layout(Label::new(handle.status.clone()), row)
}

fn general_section(handle: &TerminalSettingsHandle) -> VStack {
    let row = content_row();
    let grid = Grid::new()
        .columns(2usize)
        .column_gap(1u16)
        .row_gap(0u16)
        .child(Label::new("Scrollback rows"))
        .child(TextBox::new(
            "Scrollback",
            handle.bindings.scrollback_len.clone(),
        ))
        .child(Label::new("Prefix key"))
        .child(TextBox::new(
            "Ctrl+letter",
            handle.bindings.prefix_key.clone(),
        ))
        .child(Label::new("Release shortcut"))
        .child(TextBox::new(
            "Shortcut",
            handle.bindings.release_shortcut.clone(),
        ))
        .child(Label::new("Alt-screen scroll"))
        .child(Checkbox::new(
            "Translate wheel to keys",
            handle.bindings.alt_scroll_enabled.clone(),
        ))
        .child(Label::new("Alt scroll step"))
        .child(TextBox::new(
            "Wheel step",
            handle.bindings.alt_scroll_step.clone(),
        ))
        .child(Label::new("Alt scroll up/down"))
        .child(
            HStack::new()
                .spacing(1u16)
                .child(TextBox::new(
                    "Up",
                    handle.bindings.alt_scroll_up_key.clone(),
                ))
                .child(TextBox::new(
                    "Down",
                    handle.bindings.alt_scroll_down_key.clone(),
                )),
        )
        .child(Label::new("Cursor shape"))
        .child(RadioGroup::new(
            "Default cursor",
            CURSOR_SHAPES
                .iter()
                .map(|value| (*value).to_string())
                .collect::<Vec<_>>(),
            handle.bindings.cursor_shape_index.clone(),
        ));

    VStack::new()
        .spacing(0u16)
        .child_with_layout(Text::new("General"), row)
        .child_with_layout(grid, row)
}

fn palette_section(handle: &TerminalSettingsHandle) -> VStack {
    let row = content_row();
    let grid = Grid::new()
        .columns(2usize)
        .column_gap(1u16)
        .row_gap(0u16)
        .child(Label::new("Foreground"))
        .child(TextBox::new(
            "Foreground",
            handle.bindings.palette_foreground.clone(),
        ))
        .child(Label::new("Background"))
        .child(TextBox::new(
            "Background",
            handle.bindings.palette_background.clone(),
        ))
        .child(Label::new("ANSI palette"))
        .child(
            HStack::new()
                .spacing(1u16)
                .child(
                    ListBox::new(
                        "Index",
                        handle.bindings.palette_items.clone(),
                        handle.bindings.selected_palette_index.clone(),
                    )
                    .height(5u16),
                )
                .child(TextBox::new(
                    "Color",
                    handle.bindings.selected_palette_value.clone(),
                )),
        );

    VStack::new()
        .spacing(0u16)
        .child_with_layout(Text::new("Palette"), row)
        .child_with_layout(grid, row)
}

fn session_section(handle: &TerminalSettingsHandle) -> VStack {
    let row = content_row();
    let grid = Grid::new()
        .columns(2usize)
        .column_gap(1u16)
        .row_gap(0u16)
        .child(Label::new("Profile name"))
        .child(TextBox::new(
            "Profile",
            handle.bindings.profile_name.clone(),
        ))
        .child(Label::new("Shell / command"))
        .child(TextBox::new(
            "Command",
            handle.bindings.profile_command.clone(),
        ))
        .child(Label::new("Args JSON"))
        .child(TextBox::new(
            "Args",
            handle.bindings.profile_args_json.clone(),
        ))
        .child(Label::new("Working directory"))
        .child(TextBox::new("Cwd", handle.bindings.profile_cwd.clone()))
        .child(Label::new("Shell integration"))
        .child(Checkbox::new(
            "Inject OSC 133/7 hooks",
            handle.bindings.shell_integration_inject.clone(),
        ))
        .child(Label::new("Shell exit"))
        .child(Checkbox::new(
            "Close window on shell exit",
            handle.bindings.close_window_on_shell_exit.clone(),
        ));

    VStack::new()
        .spacing(0u16)
        .child_with_layout(Text::new("Session"), row)
        .child_with_layout(grid, row)
}

fn button_row(handle: &TerminalSettingsHandle) -> HStack {
    let apply_handle = handle.clone();
    let save_handle = handle.clone();
    let reset_handle = handle.clone();
    HStack::new()
        .spacing(1u16)
        .child(Button::new("Apply").default_button(true).on_click(move || {
            let _ = apply_handle.apply();
        }))
        .child(Button::new("Save").on_click(move || {
            let _ = save_handle.save();
        }))
        .child(Button::new("Reset").on_click(move || {
            reset_handle.reset_from_applied();
        }))
}

fn content_row() -> LayoutParams {
    LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    }
}

fn preview_fn(handle: &TerminalSettingsHandle) -> impl Fn() -> String + Send + Sync + 'static {
    let handle = handle.clone();
    move || handle.preview_text()
}

fn optional_color_text(color: Option<&TerminalColorSpec>) -> String {
    color
        .map(TerminalColorSpec::as_str)
        .unwrap_or_default()
        .to_string()
}

fn optional_color_config(input: &str) -> Option<TerminalColorSpec> {
    let trimmed = input.trim();
    (!trimmed.is_empty()).then(|| TerminalColorSpec::new(trimmed))
}

fn parse_positive_usize(input: &str, label: &str) -> Result<usize> {
    let value = input
        .trim()
        .parse::<usize>()
        .with_context(|| format!("parse {label}"))?;
    ensure!(value > 0, "{label} must be greater than zero");
    Ok(value)
}

fn parse_positive_u16(input: &str, label: &str) -> Result<u16> {
    let value = input
        .trim()
        .parse::<u16>()
        .with_context(|| format!("parse {label}"))?;
    ensure!(value > 0, "{label} must be greater than zero");
    Ok(value)
}

fn parse_profile_args(input: &str) -> Result<Vec<String>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(trimmed).context("profile args must be a JSON string array")
}

fn parse_prefix_key(input: &str) -> Result<TerminalShortcutConfig> {
    let normalized = input.trim().to_ascii_lowercase().replace(' ', "");
    let key = normalized
        .strip_prefix("ctrl+")
        .or_else(|| normalized.strip_prefix("control+"))
        .unwrap_or(normalized.as_str());
    let mut chars = key.chars();
    let Some(letter) = chars.next() else {
        bail!("prefix key must be Ctrl+<ASCII letter>");
    };
    ensure!(
        chars.next().is_none() && letter.is_ascii_alphabetic(),
        "prefix key must be Ctrl+<ASCII letter>"
    );
    Ok(TerminalShortcutConfig::control_letter(letter))
}

fn parse_shortcut_text(input: &str) -> Result<TerminalShortcutConfig> {
    let parts = input
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    ensure!(!parts.is_empty(), "shortcut must not be empty");
    let (key, modifier_parts) = parts
        .split_last()
        .expect("parts is not empty after validation");
    let modifiers = modifier_parts
        .iter()
        .map(|part| parse_shortcut_modifier(part))
        .collect::<Result<Vec<_>>>()?;
    let shortcut = TerminalShortcutConfig::new((*key).to_ascii_lowercase(), modifiers);
    shortcut.to_shortcut()?;
    Ok(shortcut)
}

fn parse_shortcut_modifier(input: &str) -> Result<TerminalShortcutModifier> {
    match input.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Ok(TerminalShortcutModifier::Control),
        "shift" => Ok(TerminalShortcutModifier::Shift),
        "alt" | "option" => Ok(TerminalShortcutModifier::Alt),
        _ => bail!("unknown shortcut modifier {input:?}"),
    }
}

fn shortcut_text(shortcut: &TerminalShortcutConfig) -> String {
    let mut parts = shortcut
        .modifiers
        .iter()
        .map(|modifier| match modifier {
            TerminalShortcutModifier::Control => "ctrl",
            TerminalShortcutModifier::Shift => "shift",
            TerminalShortcutModifier::Alt => "alt",
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    parts.push(shortcut.key.clone());
    parts.join("+")
}

fn prefix_key_text(shortcut: &TerminalShortcutConfig) -> String {
    format!("ctrl+{}", shortcut.key.to_ascii_lowercase())
}

fn cursor_shape_index(shape: TerminalCursorShapeConfig) -> usize {
    match shape {
        TerminalCursorShapeConfig::Block => 0,
        TerminalCursorShapeConfig::Underline => 1,
        TerminalCursorShapeConfig::Bar => 2,
    }
}

fn cursor_shape_from_index(index: usize) -> TerminalCursorShapeConfig {
    match index {
        1 => TerminalCursorShapeConfig::Underline,
        2 => TerminalCursorShapeConfig::Bar,
        _ => TerminalCursorShapeConfig::Block,
    }
}

fn cursor_shape_text(shape: TerminalCursorShapeConfig) -> &'static str {
    match shape {
        TerminalCursorShapeConfig::Block => "block",
        TerminalCursorShapeConfig::Underline => "underline",
        TerminalCursorShapeConfig::Bar => "bar",
    }
}

fn palette_items_for(colors: &[String; PALETTE_LEN]) -> Vec<String> {
    colors
        .iter()
        .enumerate()
        .map(|(idx, value)| format!("{idx:02} {value}"))
        .collect()
}

fn first_error_line(error: &anyhow::Error) -> String {
    error
        .to_string()
        .lines()
        .next()
        .unwrap_or("unknown error")
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    use atto_ui::composable::{ComponentContext, MouseCoordinateSpace, ScrollbarHost, TabMode};
    use atto_ui::theme::Theme;
    use atto_ui::wm::WindowId;
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn settings_ctx(theme: &Theme) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        }
    }

    fn mouse_at(kind: MouseEventKind, col: u16, row: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column: col,
            row,
            modifiers: crossterm::event::KeyModifiers::NONE,
        })
    }

    fn dump_screen(terminal: &Terminal<TestBackend>, area: Rect) -> String {
        let buf = terminal.backend().buffer();
        let mut s = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                s.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
            }
            s.push('\n');
        }
        s
    }

    /// Clicks the input line of the field whose title contains `title`, then
    /// returns the focus cursor row (the row the newly focused TextBox parked
    /// its cursor at). A correctly routed click lands the cursor on the clicked
    /// field's own input line.
    fn click_field_and_cursor_row(
        view: &mut TerminalSettingsView,
        terminal: &mut Terminal<TestBackend>,
        theme: &Theme,
        area: Rect,
        title: &str,
    ) -> Option<u16> {
        let screen = dump_screen(terminal, area);
        let (label_row, _) = screen
            .lines()
            .enumerate()
            .find(|(_, l)| l.contains(title))?;
        let click_row = label_row as u16 + 1; // input box line sits below its title
        view.handle_event(
            &mouse_at(MouseEventKind::Down(MouseButton::Left), 40, click_row),
            settings_ctx(theme),
        );
        terminal
            .draw(|f| view.draw(f, area, settings_ctx(theme)))
            .expect("draw");
        terminal.get_cursor_position().ok().map(|p| p.y)
    }

    #[test]
    fn settings_checkbox_click_must_not_break_later_hit_testing() {
        let mut view = TerminalSettingsView::from_config(TerminalConfig::default());
        let theme = Theme::dark();

        // Small viewport forces the scrollable root to clip and scroll.
        let area = Rect::new(0, 0, 72, 16);
        let backend = TestBackend::new(72, 16);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let render = |view: &mut TerminalSettingsView, terminal: &mut Terminal<TestBackend>| {
            terminal
                .draw(|f| view.draw(f, area, settings_ctx(&theme)))
                .expect("draw");
        };

        // Draw, then scroll to the bottom so the checkbox row is visible.
        render(&mut view, &mut terminal);
        let (_cw, ch) = view.content_size();
        view.set_scroll_offset(0, ch);
        render(&mut view, &mut terminal);

        // Baseline: clicking the "Cwd" input parks the cursor on the Cwd line.
        let baseline = click_field_and_cursor_row(&mut view, &mut terminal, &theme, area, "Cwd");
        let screen = dump_screen(&terminal, area);
        let cwd_input_row = screen
            .lines()
            .enumerate()
            .find(|(_, l)| l.contains("Cwd"))
            .map(|(r, _)| r as u16 + 1)
            .expect("Cwd field visible");
        assert_eq!(
            baseline,
            Some(cwd_input_row),
            "baseline click on Cwd should focus the Cwd input"
        );

        // Click the "Close window on shell exit" checkbox (down + up).
        let screen = dump_screen(&terminal, area);
        let (cb_col, cb_row) = screen
            .lines()
            .enumerate()
            .find_map(|(row, line)| {
                line.find("Close window on shell exit")
                    .map(|idx| (idx as u16, row as u16))
            })
            .expect("checkbox visible after scroll");
        view.handle_event(
            &mouse_at(MouseEventKind::Down(MouseButton::Left), cb_col, cb_row),
            settings_ctx(&theme),
        );
        view.handle_event(
            &mouse_at(MouseEventKind::Up(MouseButton::Left), cb_col, cb_row),
            settings_ctx(&theme),
        );
        render(&mut view, &mut terminal);

        // The checkbox should have toggled on.
        assert!(
            view.handle().draft().close_window_on_shell_exit,
            "checkbox click should toggle the binding on"
        );

        // Regression: after the checkbox click, clicking the Cwd input must
        // still focus the Cwd input — not some other field.
        let after = click_field_and_cursor_row(&mut view, &mut terminal, &theme, area, "Cwd");
        assert_eq!(
            after,
            Some(cwd_input_row),
            "after a checkbox click, clicking Cwd must still focus the Cwd input \
             (got cursor row {after:?}, expected {cwd_input_row})"
        );
    }

    fn sample_config() -> TerminalConfig {
        TerminalConfig {
            scrollback_len: 4096,
            palette: TerminalPaletteConfig {
                foreground: Some("#eeeeee".into()),
                background: Some("indexed:235".into()),
                ..Default::default()
            },
            prefix_key: TerminalShortcutConfig::control_letter('a'),
            release_shortcut: TerminalShortcutConfig::new(
                "escape",
                [
                    TerminalShortcutModifier::Control,
                    TerminalShortcutModifier::Shift,
                ],
            ),
            alternate_screen_scroll: TerminalAlternateScreenScrollConfig {
                enabled: true,
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
                socket_path: "/tmp/atto-ui-settings.sock".to_string(),
                shim_path: Some("/tmp/atto-ui-shim".to_string()),
                server_pid: Some(5150),
                session_id: 8,
                pane_id: 13,
                override_term: true,
            },
            close_window_on_shell_exit: true,
            cursor: TerminalCursorConfig {
                default_shape: TerminalCursorShapeConfig::Bar,
            },
        }
    }

    #[test]
    fn terminal_settings_draft_round_trips_config() {
        let config = sample_config();
        let draft = TerminalSettingsDraft::from_config(&config);

        assert_eq!(draft.to_config().unwrap(), config);
    }

    #[test]
    fn terminal_settings_apply_updates_shared_config() {
        let config = Binding::new(TerminalConfig::default());
        let view = TerminalSettingsView::new(config.clone(), None);
        let handle = view.handle();

        handle.set_scrollback_len_text("8192");
        handle.set_prefix_key_text("ctrl+a");
        handle.set_palette_color_text(1, "#123456");
        handle.set_cursor_shape(TerminalCursorShapeConfig::Underline);
        handle.set_close_window_on_shell_exit(true);

        let applied = handle.apply().unwrap();
        assert_eq!(applied.scrollback_len, 8192);
        assert_eq!(
            applied.prefix_key,
            TerminalShortcutConfig::control_letter('a')
        );
        assert_eq!(applied.palette.ansi[1].as_str(), "#123456");
        assert_eq!(
            applied.cursor.default_shape,
            TerminalCursorShapeConfig::Underline
        );
        assert!(applied.close_window_on_shell_exit);
        assert_eq!(config.get(), applied);
        assert!(handle.status_text().contains("Applied"));
    }

    #[test]
    fn terminal_settings_invalid_input_updates_status() {
        let view = TerminalSettingsView::from_config(TerminalConfig::default());
        let handle = view.handle();

        handle.set_scrollback_len_text("0");

        assert!(handle.apply().is_err());
        assert!(handle.status_text().contains("Error"));
    }

    #[test]
    fn terminal_settings_palette_editor_commits_all_edited_indices() {
        // Mirrors the real UI path: the ListBox drives `selected_palette_index`
        // and the single "Color" TextBox drives `selected_palette_value`. A
        // reconcile runs each frame (via `refresh_palette_items`). This must
        // preserve edits to *every* index the user visits, not just the last.
        let config = Binding::new(TerminalConfig::default());
        let view = TerminalSettingsView::new(config.clone(), None);
        let handle = view.handle();
        let bindings = &handle.bindings;

        // Edit index 0 in the Color box.
        bindings.selected_palette_value.set("#111111".to_string());
        // Move the ListBox selection to index 3 (a frame renders in between).
        bindings.selected_palette_index.set(3);
        view.refresh_palette_items();
        // The editor now shows index 3's value; edit it.
        bindings.selected_palette_value.set("#333333".to_string());
        // Move to index 7 and edit.
        bindings.selected_palette_index.set(7);
        view.refresh_palette_items();
        bindings.selected_palette_value.set("#777777".to_string());

        let applied = handle.apply().unwrap();
        assert_eq!(applied.palette.ansi[0].as_str(), "#111111");
        assert_eq!(applied.palette.ansi[3].as_str(), "#333333");
        assert_eq!(applied.palette.ansi[7].as_str(), "#777777");
    }

    #[test]
    fn terminal_settings_rejects_invalid_edits_without_mutating_config() {
        let config = Binding::new(TerminalConfig::default());
        let original = config.get();
        let view = TerminalSettingsView::new(config.clone(), None);
        let handle = view.handle();

        handle.set_prefix_key_text("ctrl+f10");
        assert!(handle.apply().is_err());
        assert_eq!(config.get(), original);
        assert!(handle.status_text().contains("Error"));

        handle.reset_from_applied();
        handle.set_palette_color_text(0, "not-a-color");
        assert!(handle.apply().is_err());
        assert_eq!(config.get(), original);
        assert!(handle.status_text().contains("Error"));

        handle.reset_from_applied();
        handle
            .bindings
            .profile_args_json
            .set(r#"["ok", 7]"#.to_string());
        assert!(handle.apply().is_err());
        assert_eq!(config.get(), original);
        assert!(handle.status_text().contains("Error"));
    }

    #[test]
    fn terminal_settings_save_rejects_invalid_draft_without_writing_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "atto-ui-terminal-settings-invalid-{}-{unique}.yaml",
            process::id()
        ));
        let config = Binding::new(TerminalConfig::default());
        let original = config.get();
        let view = TerminalSettingsView::new(config.clone(), Some(path.clone()));
        let handle = view.handle();

        handle.set_scrollback_len_text("0");

        assert!(handle.save().is_err());
        assert_eq!(config.get(), original);
        assert!(!path.exists());
        assert!(handle.status_text().contains("Error"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn terminal_settings_save_persists_yaml() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "atto-ui-terminal-settings-{}-{unique}.yaml",
            process::id()
        ));
        let config = Binding::new(TerminalConfig::default());
        let view = TerminalSettingsView::new(config, Some(path.clone()));
        let handle = view.handle();

        handle.set_scrollback_len_text("1234");
        handle.save().unwrap();

        let loaded = TerminalConfig::load_path(&path).unwrap();
        assert_eq!(loaded.scrollback_len, 1234);
        assert!(handle.status_text().contains("Saved terminal config"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn terminal_settings_save_failure_does_not_mutate_live_config() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        // Make the save path's parent a regular file so create_dir_all / write
        // fails even though the draft itself is valid.
        let blocker = env::temp_dir().join(format!(
            "atto-ui-terminal-settings-blocker-{}-{unique}",
            process::id()
        ));
        fs::write(&blocker, b"not a directory").expect("create blocker file");
        let path = blocker.join("terminal.yaml");

        let config = Binding::new(TerminalConfig::default());
        let original = config.get();
        let view = TerminalSettingsView::new(config.clone(), Some(path));
        let handle = view.handle();

        // Valid edit, but the write must fail.
        handle.set_scrollback_len_text("4321");
        assert!(handle.save().is_err());
        // The live config must be untouched when the write fails.
        assert_eq!(config.get(), original);
        assert!(handle.status_text().contains("Error"));

        let _ = fs::remove_file(blocker);
    }
}
