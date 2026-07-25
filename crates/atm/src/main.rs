#![forbid(unsafe_code)]

//! `atm` — the atto terminal multiplexer.
//!
//! A full multi-window terminal app built on the reusable `atto-ui-terminal`
//! components: local PTY sessions in atto-ui windows, split panes, copy-mode,
//! command blocks, prefix-key handling, a settings window, and IPC hooks for
//! the companion `tmux` shim.

use std::cell::RefCell;
use std::env;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, MenuItem, MenuSpec,
    popup_menu_window, run_crossterm_desktop_with_actions,
};
use atto_ui::composable::{Component, ComponentContext};
use atto_ui::reactive::{Binding, DirtyObserver};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowId, WindowKind, WindowState};
use atto_ui_terminal::{
    TerminalCommandBlockPresentation, TerminalConfig, TerminalEmulator, TerminalHandle,
    TerminalPaneGroup, TerminalPaneGroupHandle, TerminalPaneId, TerminalSessionSpec,
    TerminalSettingsView, TerminalShortcut, TerminalShortcutConfig, TerminalShortcutModifier,
    default_terminal_config_path, load_terminal_config_or_default,
};

const WINDOWS_MENU_ID: &str = "atm:windows";
const WINDOWS_MENU_LIST_ID: &str = "atm:windows:list";

#[derive(Clone, Copy, Debug)]
enum TerminalViewerAction {
    NewShellWindow,
    NewCommandWindow,
    OpenFeatureGuide,
    OpenSettings,
    Quit,
    FocusNext,
    MinimizeFocused,
    ToggleMaximizeFocused,
    CloseFocused,
    FocusWindow(WindowId),
    CommandContext(CommandContextMenuAction),
    SetTheme(&'static str),
}

#[derive(Clone, Copy, Debug)]
enum CommandContextMenuAction {
    Rerun,
    CopyCommand,
    CopyOutput,
}

struct TerminalWindowSession {
    id: WindowId,
    panes: TerminalPaneGroupHandle,
    window_number: usize,
    spec: TerminalSessionSpec,
    exit_prompted: bool,
}

impl TerminalWindowSession {
    fn new(
        id: WindowId,
        panes: TerminalPaneGroupHandle,
        window_number: usize,
        spec: TerminalSessionSpec,
    ) -> Self {
        Self {
            id,
            panes,
            window_number,
            spec,
            exit_prompted: false,
        }
    }

    fn active_handle(&self) -> Option<TerminalHandle> {
        self.panes.active_terminal_handle()
    }
}

#[derive(Clone, Copy, Debug)]
struct CommandContextState {
    menu_id: WindowId,
    terminal_id: WindowId,
    pane_id: TerminalPaneId,
    block_index: usize,
}

struct FeatureGuideView {
    lines: Vec<String>,
}

impl FeatureGuideView {
    fn new(lines: Vec<String>) -> Self {
        Self { lines }
    }
}

impl Component for FeatureGuideView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, _ctx: ComponentContext<'_>) {
        let lines: Vec<Line<'_>> = self
            .lines
            .iter()
            .map(|line| Line::from(line.as_str()))
            .collect();
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }
}

atto_ui::impl_component_default_traits!(
    FeatureGuideView => Layout, Scrollable, FocusNav, DynamicTree, EventHandling
);

fn build_menu(action_tx: mpsc::Sender<TerminalViewerAction>) -> MenuBar {
    MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![
                MenuItem::action("New shell window", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::NewShellWindow);
                    }
                })
                .shortcut("n"),
                MenuItem::action("New command window", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::NewCommandWindow);
                    }
                })
                .shortcut("c"),
                MenuItem::action("Settings", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::OpenSettings);
                    }
                })
                .shortcut(","),
                MenuItem::action("Quit", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::Quit);
                    }
                })
                .shortcut("q"),
            ],
        ),
        MenuSpec::new(
            "Help",
            vec![
                MenuItem::action("Feature guide", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::OpenFeatureGuide);
                    }
                })
                .shortcut("?"),
            ],
        ),
        MenuSpec::new(
            "View",
            vec![MenuItem::submenu(
                "Theme",
                vec![
                    MenuItem::action("Dark", {
                        let action_tx = action_tx.clone();
                        move || {
                            let _ = action_tx.send(TerminalViewerAction::SetTheme("dark"));
                        }
                    }),
                    MenuItem::action("Light", {
                        let action_tx = action_tx.clone();
                        move || {
                            let _ = action_tx.send(TerminalViewerAction::SetTheme("light"));
                        }
                    }),
                    MenuItem::action("Turbo", {
                        let action_tx = action_tx.clone();
                        move || {
                            let _ = action_tx.send(TerminalViewerAction::SetTheme("turbo"));
                        }
                    }),
                ],
            )],
        ),
        MenuSpec::new(
            "Windows",
            vec![
                MenuItem::action("Next window", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::FocusNext);
                    }
                })
                .shortcut("F6"),
                MenuItem::action("Minimize focused", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::MinimizeFocused);
                    }
                })
                .shortcut("m"),
                MenuItem::action("Maximize/restore focused", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::ToggleMaximizeFocused);
                    }
                })
                .shortcut("x"),
                MenuItem::action("Close focused", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::CloseFocused);
                    }
                })
                .shortcut("w"),
                MenuItem::minimized_windows("Minimized windows"),
                MenuItem::submenu("Switch to", Vec::new()).with_tag(WINDOWS_MENU_LIST_ID),
            ],
        )
        .with_tag(WINDOWS_MENU_ID),
    ])
}

fn viewer_release_shortcut_config() -> TerminalShortcutConfig {
    TerminalShortcutConfig::new(
        "l",
        [
            TerminalShortcutModifier::Control,
            TerminalShortcutModifier::Shift,
        ],
    )
}

fn load_viewer_terminal_config(path: Option<&Path>) -> Result<TerminalConfig> {
    let mut config = load_terminal_config_or_default(path)?;
    if path.is_none_or(|path| !path.exists()) {
        config.release_shortcut = viewer_release_shortcut_config();
    }
    config.validate()?;
    Ok(config)
}

fn shortcut_label(shortcut: TerminalShortcut) -> String {
    let mut parts = Vec::new();
    if shortcut.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("Ctrl".to_string());
    }
    if shortcut.modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("Shift".to_string());
    }
    if shortcut.modifiers.contains(KeyModifiers::ALT) {
        parts.push("Alt".to_string());
    }
    let key = match shortcut.code {
        KeyCode::Char(ch) => ch.to_ascii_uppercase().to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "BackTab".to_string(),
        KeyCode::F(n) => format!("F{n}"),
        other => format!("{other:?}"),
    };
    parts.push(key);
    parts.join("+")
}

fn terminal_window_rect(work_area: Rect, index: usize) -> Rect {
    let base_width = work_area.width.saturating_sub(6).clamp(30, 110);
    let base_height = work_area.height.saturating_sub(6).clamp(10, 32);

    let index = index.max(1) as u16;
    let offset_x = (index.saturating_sub(1) * 2) % 10;
    let offset_y = (index.saturating_sub(1)) % 6;

    Rect {
        x: work_area.x.saturating_add(2 + offset_x),
        y: work_area.y.saturating_add(1 + offset_y),
        width: base_width,
        height: base_height,
    }
}

fn terminal_window_title(window_number: usize) -> String {
    format!("Terminal {window_number}")
}

fn demo_command_session_spec() -> TerminalSessionSpec {
    TerminalSessionSpec::command(
        "Demo Command",
        "/bin/sh",
        vec![
            "-lc".to_string(),
            concat!(
                "printf 'Atto terminal demo command session\\n'; ",
                "printf 'This profile was launched from File > New command window.\\n'; ",
                "printf 'It inherits the active terminal cwd, then hands off to your shell.\\n'; ",
                "exec \"${SHELL:-/bin/sh}\""
            )
            .to_string(),
        ],
    )
}

fn is_plain_restart_key(event: &Event) -> bool {
    let Event::Key(key) = event else {
        return false;
    };
    if key.kind != KeyEventKind::Press || key.modifiers != KeyModifiers::NONE {
        return false;
    }
    matches!(key.code, KeyCode::Char(ch) if ch.eq_ignore_ascii_case(&'r'))
}

fn seed_terminal_banner(
    handle: &TerminalHandle,
    window_number: usize,
    pane_number: usize,
    config: &TerminalConfig,
) -> Result<()> {
    let banner = format!("Terminal Emulator ({window_number}.{pane_number})\r\n");
    handle.process_output_str(&banner);
    handle
        .process_output_str("Menu: click the menu bar, or press F10 after releasing capture.\r\n");
    let release = shortcut_label(config.release_shortcut()?);
    handle.process_output_str(&format!(
        "{release}: release capture; click terminal to recapture.\r\n"
    ));
    let prefix = shortcut_label(config.prefix_shortcut()?);
    handle.process_output_str(&format!(
        "{prefix} %/\": split; {prefix} arrows: select; {prefix} Ctrl+arrows: resize.\r\n"
    ));
    handle.process_output_str(&format!(
        "{prefix} z: zoom pane; {prefix} x: close pane; {prefix} o/Tab: next pane.\r\n"
    ));
    handle.process_output_str(&format!(
        "{prefix} [: copy-mode; {prefix} ]: paste copy buffer.\r\n"
    ));
    handle.process_output_str(
        "File > New shell/command: sessions; File > Settings: live config including close-on-exit.\r\n",
    );
    handle.process_output_str(
        "Right-click an OSC 133 command block for rerun/copy actions when shell integration is active.\r\n",
    );
    Ok(())
}

fn build_terminal_view(
    window_number: usize,
    pane_number: usize,
    spec: &TerminalSessionSpec,
    config: &TerminalConfig,
) -> Result<(TerminalEmulator, TerminalHandle)> {
    let mut terminal = TerminalEmulator::from_config(config)?
        .command_block_presentation(TerminalCommandBlockPresentation::enabled());
    terminal.spawn_session(spec)?;
    let handle = terminal.handle();
    seed_terminal_banner(&handle, window_number, pane_number, config)?;
    Ok((terminal, handle))
}

fn build_terminal_pane_group(
    window_number: usize,
    spec: &TerminalSessionSpec,
    config: Binding<TerminalConfig>,
) -> Result<(TerminalPaneGroup, TerminalPaneGroupHandle)> {
    let initial_config = config.get();
    let (terminal, _) = build_terminal_view(window_number, 1, spec, &initial_config)?;
    let spec = spec.clone();
    let config_for_factory = config.clone();
    let group = TerminalPaneGroup::new(terminal)
        .config(&initial_config)?
        .pane_factory(move |pane_number| {
            let config = config_for_factory.get();
            build_terminal_view(window_number, pane_number, &spec, &config)
                .map(|(terminal, _)| terminal)
        });
    let handle = group.handle();
    Ok((group, handle))
}

fn prune_terminal_sessions(desktop: &Desktop, sessions: &mut Vec<TerminalWindowSession>) {
    sessions.retain(|session| desktop.wm.windows().iter().any(|w| w.id() == session.id));
}

fn sync_session_cwd_from_active_pane(session: &mut TerminalWindowSession) -> bool {
    let Some(cwd) = session
        .active_handle()
        .and_then(|handle| handle.current_cwd())
    else {
        return false;
    };
    let cwd = PathBuf::from(cwd);
    if session.spec.cwd() == Some(cwd.as_path()) {
        return false;
    }
    session.spec.set_cwd(cwd);
    true
}

fn sync_session_cwds(sessions: &mut [TerminalWindowSession]) -> bool {
    let mut changed = false;
    for session in sessions {
        changed |= sync_session_cwd_from_active_pane(session);
    }
    changed
}

fn focused_session_cwd(desktop: &Desktop, sessions: &[TerminalWindowSession]) -> Option<PathBuf> {
    let focused = desktop.wm.focused()?;
    sessions
        .iter()
        .find(|session| session.id == focused)
        .and_then(|session| session.spec.cwd().map(PathBuf::from))
}

fn spec_for_new_window(
    desktop: &Desktop,
    sessions: &[TerminalWindowSession],
    base: &TerminalSessionSpec,
) -> TerminalSessionSpec {
    let mut spec = base.clone();
    if let Some(cwd) = focused_session_cwd(desktop, sessions) {
        spec.set_cwd(cwd);
    }
    spec
}

fn show_exit_prompt_if_needed(
    desktop: &mut Desktop,
    session: &mut TerminalWindowSession,
    config: &TerminalConfig,
) -> bool {
    if session.exit_prompted || desktop.wm.window(session.id).is_none() {
        return false;
    }
    let Some(handle) = session.active_handle() else {
        return false;
    };
    let Some(status) = handle.exit_status() else {
        return false;
    };

    if config.close_window_on_shell_exit {
        desktop.wm.close(session.id);
        return true;
    }

    handle.set_capture(false);
    handle.process_output_str(&format!(
        "\r\n[Process exited: code {} — press R to restart]\r\n",
        status.exit_code()
    ));
    session.exit_prompted = true;
    true
}

fn update_terminal_exit_prompts(
    desktop: &mut Desktop,
    sessions: &mut Vec<TerminalWindowSession>,
    config: &TerminalConfig,
) -> bool {
    prune_terminal_sessions(desktop, sessions);
    let mut changed = false;
    for session in sessions.iter_mut() {
        changed |= show_exit_prompt_if_needed(desktop, session, config);
    }
    prune_terminal_sessions(desktop, sessions);
    changed
}

fn sync_terminal_window_titles(desktop: &mut Desktop, sessions: &[TerminalWindowSession]) -> bool {
    let mut changed = false;
    for session in sessions {
        let Some(handle) = session.active_handle() else {
            continue;
        };
        // 应用清空标题时 window_title() 会返回 None,此时回退到窗口的初始标题,
        // 而不是显示成空标题。
        let title = handle
            .window_title()
            .unwrap_or_else(|| terminal_window_title(session.window_number));
        let current_title = desktop.wm.window(session.id).map(|w| w.title.get());
        if current_title.as_deref() == Some(title.as_str()) {
            continue;
        }
        changed |= desktop.set_title(session.id, title);
    }
    changed
}

fn restart_terminal_window(
    desktop: &mut Desktop,
    session: &mut TerminalWindowSession,
    config: Binding<TerminalConfig>,
) -> Result<bool> {
    if !desktop.wm.windows().iter().any(|w| w.id() == session.id) {
        return Ok(false);
    }

    let (terminal, panes) =
        build_terminal_pane_group(session.window_number, &session.spec, config)?;
    if !desktop.wm.set_view(session.id, Box::new(terminal)) {
        return Ok(false);
    }
    session.panes = panes;
    session.exit_prompted = false;
    desktop.set_title(session.id, terminal_window_title(session.window_number));
    desktop.wm.focus(session.id);
    Ok(true)
}

fn restart_focused_terminal(
    desktop: &mut Desktop,
    sessions: &mut [TerminalWindowSession],
    config: Binding<TerminalConfig>,
) -> Result<bool> {
    if desktop.menu.is_active() {
        return Ok(false);
    }
    let Some(focused) = desktop.wm.focused() else {
        return Ok(false);
    };
    let Some(session) = sessions
        .iter_mut()
        .find(|session| session.id == focused && session.exit_prompted)
    else {
        return Ok(false);
    };
    restart_terminal_window(desktop, session, config)
}

fn apply_terminal_config_to_sessions(
    config: &TerminalConfig,
    sessions: &[TerminalWindowSession],
) -> Result<()> {
    for session in sessions {
        session.panes.apply_config(config)?;
    }
    Ok(())
}

fn apply_terminal_config_if_dirty(
    config: &Binding<TerminalConfig>,
    observer: &mut DirtyObserver,
    sessions: &[TerminalWindowSession],
) -> Result<()> {
    if config.check_dirty(observer) {
        apply_terminal_config_to_sessions(&config.get(), sessions)?;
    }
    Ok(())
}

fn close_command_context_menu(desktop: &mut Desktop, context: &mut Option<CommandContextState>) {
    if let Some(context) = context.take() {
        desktop.wm.close(context.menu_id);
    }
}

fn command_block_at_mouse(
    desktop: &Desktop,
    sessions: &[TerminalWindowSession],
    mouse: &MouseEvent,
) -> Option<(WindowId, TerminalPaneId, usize)> {
    for session in sessions {
        // A session may briefly outlive its window (pruned only on the next
        // tick). Skip such sessions instead of aborting the whole scan, which
        // would miss command blocks in later, still-valid sessions.
        let Some(window) = desktop.wm.window(session.id) else {
            continue;
        };
        let inner = window.inner_rect();
        if mouse.column < inner.x
            || mouse.row < inner.y
            || mouse.column >= inner.x.saturating_add(inner.width)
            || mouse.row >= inner.y.saturating_add(inner.height)
        {
            continue;
        }
        let Some(pane) = session
            .panes
            .pane_at_screen_position(mouse.column, mouse.row)
        else {
            continue;
        };
        let Some(rect) = pane.rect else {
            continue;
        };
        let row = mouse.row.saturating_sub(rect.y);
        let col = mouse.column.saturating_sub(rect.x);
        let position = pane.handle.selection_position_for_view_cell(row, col);
        if let Some(block_index) = pane.handle.command_block_index_at_position(position) {
            return Some((session.id, pane.id, block_index));
        }
    }
    None
}

fn command_context_menu_items(action_tx: &mpsc::Sender<TerminalViewerAction>) -> Vec<MenuItem> {
    let send = |action: CommandContextMenuAction, tx: mpsc::Sender<TerminalViewerAction>| {
        move || {
            let _ = tx.send(TerminalViewerAction::CommandContext(action));
        }
    };
    vec![
        MenuItem::action(
            "Rerun",
            send(CommandContextMenuAction::Rerun, action_tx.clone()),
        ),
        MenuItem::action(
            "Copy command",
            send(CommandContextMenuAction::CopyCommand, action_tx.clone()),
        ),
        MenuItem::action(
            "Copy output",
            send(CommandContextMenuAction::CopyOutput, action_tx.clone()),
        ),
    ]
}

fn open_command_context_menu(
    desktop: &mut Desktop,
    sessions: &[TerminalWindowSession],
    screen: Rect,
    mouse: &MouseEvent,
    action_tx: &mpsc::Sender<TerminalViewerAction>,
    context: &mut Option<CommandContextState>,
) -> bool {
    close_command_context_menu(desktop, context);
    let Some((terminal_id, pane_id, block_index)) =
        command_block_at_mouse(desktop, sessions, mouse)
    else {
        return false;
    };
    if let Some(session) = sessions.iter().find(|session| session.id == terminal_id)
        && let Some(pane) = session
            .panes
            .panes()
            .into_iter()
            .find(|pane| pane.id == pane_id)
    {
        let _ = pane.handle.select_command_block_output(block_index);
    }
    let menu_id = desktop.add_window(
        popup_menu_window(
            command_context_menu_items(action_tx),
            (mouse.column, mouse.row),
            screen,
            "Command",
        ),
        screen,
    );
    *context = Some(CommandContextState {
        menu_id,
        terminal_id,
        pane_id,
        block_index,
    });
    true
}

fn apply_command_context_action(
    desktop: &mut Desktop,
    sessions: &[TerminalWindowSession],
    context: &mut Option<CommandContextState>,
    action: CommandContextMenuAction,
) {
    let Some(active) = *context else {
        return;
    };
    let Some(session) = sessions
        .iter()
        .find(|session| session.id == active.terminal_id)
    else {
        close_command_context_menu(desktop, context);
        return;
    };
    let Some(pane) = session
        .panes
        .panes()
        .into_iter()
        .find(|pane| pane.id == active.pane_id)
    else {
        close_command_context_menu(desktop, context);
        return;
    };
    match action {
        CommandContextMenuAction::Rerun => {
            pane.handle.rerun_command_block(active.block_index);
        }
        CommandContextMenuAction::CopyCommand => {
            pane.handle.copy_command_block_command(active.block_index);
        }
        CommandContextMenuAction::CopyOutput => {
            pane.handle.copy_command_block_output(active.block_index);
        }
    }
    close_command_context_menu(desktop, context);
}

fn is_right_mouse_down(event: &Event) -> Option<&MouseEvent> {
    match event {
        Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Right)) => {
            Some(mouse)
        }
        _ => None,
    }
}

fn non_right_mouse_down(event: &Event) -> Option<&MouseEvent> {
    match event {
        Event::Mouse(
            mouse @ MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left | MouseButton::Middle),
                ..
            },
        ) => Some(mouse),
        _ => None,
    }
}

/// Dismisses the context menu when a non-right click lands outside the menu
/// window. Clicks inside the menu are handled by the menu view itself (a row
/// activation closes the window and its action is applied next frame, so the
/// stored context must survive this call).
fn dismiss_context_menu_on_outside_click(
    desktop: &mut Desktop,
    event: &Event,
    context: &mut Option<CommandContextState>,
) {
    let Some(mouse) = non_right_mouse_down(event) else {
        return;
    };
    let Some(state) = *context else {
        return;
    };
    // If the menu window is gone, the click already activated an item; keep the
    // context so `on_action` can apply it next frame.
    let outside = match desktop.wm.window(state.menu_id) {
        Some(window) => {
            let rect = window.rect.get();
            mouse.column < rect.x
                || mouse.row < rect.y
                || mouse.column >= rect.x.saturating_add(rect.width)
                || mouse.row >= rect.y.saturating_add(rect.height)
        }
        None => false,
    };
    if outside {
        close_command_context_menu(desktop, context);
    }
}

fn spawn_terminal_window(
    desktop: &mut Desktop,
    screen: Rect,
    window_number: usize,
    spec: TerminalSessionSpec,
    config: Binding<TerminalConfig>,
) -> Result<TerminalWindowSession> {
    let work_area = Desktop::layout(screen).work_area;
    let rect = terminal_window_rect(work_area, window_number);

    let (terminal, panes) = build_terminal_pane_group(window_number, &spec, config)?;
    let title = terminal_window_title(window_number);
    let window = Window::new(WindowKind::Normal, title, rect, Box::new(terminal));
    let id = desktop.add_window(window, screen);
    Ok(TerminalWindowSession::new(id, panes, window_number, spec))
}

fn settings_window_rect(screen: Rect) -> Rect {
    let work_area = Desktop::layout(screen).work_area;
    let width = 76.min(work_area.width.saturating_sub(4)).max(40);
    let height = 22.min(work_area.height.saturating_sub(2)).max(12);
    Rect {
        x: work_area.x + work_area.width.saturating_sub(width) / 2,
        y: work_area.y + work_area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn feature_guide_window_rect(screen: Rect) -> Rect {
    let work_area = Desktop::layout(screen).work_area;
    let width = 76.min(work_area.width.saturating_sub(4)).max(44);
    let height = 18.min(work_area.height.saturating_sub(2)).max(12);
    Rect {
        x: work_area
            .x
            .saturating_add(work_area.width.saturating_sub(width).saturating_sub(1)),
        y: work_area.y.saturating_add(1),
        width,
        height,
    }
}

fn config_path_label(config_path: Option<&Path>) -> String {
    config_path
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "default config path unavailable".to_string())
}

fn feature_guide_lines(
    config: &TerminalConfig,
    config_path: Option<&Path>,
    shell_spec: &TerminalSessionSpec,
    command_spec: &TerminalSessionSpec,
) -> Result<Vec<String>> {
    let prefix = shortcut_label(config.prefix_shortcut()?);
    let release = shortcut_label(config.release_shortcut()?);
    Ok(vec![
        "Atto Terminal Viewer full-feature demo".to_string(),
        String::new(),
        format!("Capture: {release} releases keyboard capture; click a terminal to recapture."),
        format!(
            "Prefix: {prefix} F10 menu, {prefix} w window mode, {prefix} z zooms a pane, {prefix} {prefix} sends a literal prefix."
        ),
        format!(
            "Copy-mode: {prefix} [ enters; arrows/hjkl move; v or Space starts selection; y or Enter copies; {prefix} ] pastes."
        ),
        format!("Splits: {prefix} % splits right, {prefix} \" splits below, arrows select panes, Ctrl+arrows resize, {prefix} x closes a pane."),
        format!(
            "Sessions: File > New shell window uses profile '{}'; File > New command window uses profile '{}'.",
            shell_spec.profile(),
            command_spec.profile()
        ),
        if config.close_window_on_shell_exit {
            "Shell exit: close-window-on-exit is enabled, so an exited shell closes its terminal window.".to_string()
        } else {
            "Restart: when a session exits, the terminal shows a dead-process prompt; plain R restarts with that session profile and cwd.".to_string()
        },
        "Command blocks: OSC 133/shell-integration marks commands; Ctrl+Up/Down navigates; right-click a block to rerun or copy command/output.".to_string(),
        format!(
            "Settings: File > Settings edits scrollback, prefix/release keys, palette, profiles, shell integration, close-on-exit, and saves to {}.",
            config_path_label(config_path)
        ),
        "Windows: OSC 0/2 titles update window titles and the Windows > Switch to list.".to_string(),
        String::new(),
        "Close this guide to use the terminal; reopen it from Help > Feature guide.".to_string(),
    ])
}

fn open_feature_guide_window(
    desktop: &mut Desktop,
    screen: Rect,
    config: &TerminalConfig,
    config_path: Option<&Path>,
    shell_spec: &TerminalSessionSpec,
    command_spec: &TerminalSessionSpec,
    feature_guide_window_id: &mut Option<WindowId>,
) -> Result<()> {
    if let Some(id) = *feature_guide_window_id
        && (desktop.wm.restore_window(id) || desktop.wm.window(id).is_some())
    {
        desktop.wm.focus(id);
        return Ok(());
    }

    let id = desktop.add_window(
        Window::new(
            WindowKind::Floating,
            "Terminal Feature Guide",
            feature_guide_window_rect(screen),
            Box::new(FeatureGuideView::new(feature_guide_lines(
                config,
                config_path,
                shell_spec,
                command_spec,
            )?)),
        )
        .with_min_size(44, 12),
        screen,
    );
    desktop.wm.focus(id);
    *feature_guide_window_id = Some(id);
    Ok(())
}

fn open_settings_window(
    desktop: &mut Desktop,
    screen: Rect,
    config: Binding<TerminalConfig>,
    config_path: Option<PathBuf>,
    settings_window_id: &mut Option<WindowId>,
) {
    if let Some(id) = *settings_window_id
        && (desktop.wm.restore_window(id) || desktop.wm.window(id).is_some())
    {
        desktop.wm.focus(id);
        return;
    }

    let id = desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Terminal Settings",
            settings_window_rect(screen),
            Box::new(TerminalSettingsView::new(config, config_path)),
        )
        .with_min_size(40, 12),
        screen,
    );
    desktop.wm.focus(id);
    *settings_window_id = Some(id);
}

fn refresh_windows_menu(desktop: &mut Desktop, action_tx: &mpsc::Sender<TerminalViewerAction>) {
    if desktop.menu.is_active() {
        return;
    }

    let focused = desktop.wm.focused();
    let mut windows: Vec<(String, WindowId, bool)> = desktop
        .wm
        .windows()
        .iter()
        .filter(|w| w.kind.is_focusable())
        .filter(|w| w.state.get() != WindowState::Minimized)
        .map(|w| {
            let title = w.title.get();
            let id = w.id();
            let is_focused = Some(id) == focused;
            (title, id, is_focused)
        })
        .collect();
    windows.sort_by(|a, b| a.0.cmp(&b.0));

    let mut items = Vec::new();
    for (title, id, is_focused) in windows {
        let label = if is_focused {
            format!("* {title}")
        } else {
            title
        };
        let action_tx = action_tx.clone();
        items.push(MenuItem::action(label, move || {
            let _ = action_tx.send(TerminalViewerAction::FocusWindow(id));
        }));
    }
    if items.is_empty() {
        items.push(MenuItem::submenu("No terminal windows", Vec::new()).enabled(false));
    }

    for menu in desktop.menu.menus_mut() {
        if menu.tag.as_deref() != Some(WINDOWS_MENU_ID) {
            continue;
        }
        for item in &mut menu.items {
            if item.tag.as_deref() == Some(WINDOWS_MENU_LIST_ID) {
                item.submenu = items;
                return;
            }
        }
    }
}

fn main() -> Result<()> {
    let mut argv: Vec<String> = env::args().skip(1).collect();
    let mut terminal_config_path = default_terminal_config_path();
    if argv.first().is_some_and(|arg| arg == "--config") && argv.len() >= 2 {
        terminal_config_path = Some(PathBuf::from(argv.remove(1)));
        argv.remove(0);
    }

    let shell_spec = TerminalSessionSpec::shell_from_env();
    let command_spec = argv
        .split_first()
        .map(|(program, args)| {
            TerminalSessionSpec::command("Command", program.clone(), args.to_vec())
        })
        .unwrap_or_else(demo_command_session_spec);
    let initial_spec = if argv.is_empty() {
        shell_spec.clone()
    } else {
        command_spec.clone()
    };
    let terminal_config = Binding::new(load_viewer_terminal_config(
        terminal_config_path.as_deref(),
    )?);
    let terminal_config_observer: Rc<RefCell<DirtyObserver>> =
        Rc::new(RefCell::new(terminal_config.dirty_observer()));

    let config = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(true)
        .cursor(CursorMode::Hide);

    let (action_tx, action_rx) = mpsc::channel::<TerminalViewerAction>();
    let terminal_sessions: Rc<RefCell<Vec<TerminalWindowSession>>> =
        Rc::new(RefCell::new(Vec::new()));

    let initial_spec_for_build = initial_spec.clone();
    let shell_spec_for_build = shell_spec.clone();
    let command_spec_for_build = command_spec.clone();
    let action_tx_for_build = action_tx.clone();
    let terminal_sessions_for_build = Rc::clone(&terminal_sessions);
    let terminal_config_for_build = terminal_config.clone();
    let terminal_config_path_for_build = terminal_config_path.clone();

    let action_tx_for_actions = action_tx.clone();
    let action_tx_for_tick = action_tx.clone();
    let action_tx_for_event = action_tx.clone();
    let terminal_sessions_for_actions = Rc::clone(&terminal_sessions);
    let terminal_sessions_for_tick = Rc::clone(&terminal_sessions);
    let terminal_sessions_for_event = Rc::clone(&terminal_sessions);
    let terminal_config_for_tick = terminal_config.clone();
    let terminal_config_for_event = terminal_config.clone();
    let terminal_config_observer_for_actions = Rc::clone(&terminal_config_observer);
    let terminal_config_observer_for_tick = Rc::clone(&terminal_config_observer);
    let command_context: Rc<RefCell<Option<CommandContextState>>> = Rc::new(RefCell::new(None));
    let command_context_for_actions = Rc::clone(&command_context);
    let command_context_for_event = Rc::clone(&command_context);
    let settings_window_id: Rc<RefCell<Option<WindowId>>> = Rc::new(RefCell::new(None));
    let settings_window_id_for_actions = Rc::clone(&settings_window_id);
    let feature_guide_window_id: Rc<RefCell<Option<WindowId>>> = Rc::new(RefCell::new(None));
    let feature_guide_window_id_for_build = Rc::clone(&feature_guide_window_id);
    let feature_guide_window_id_for_actions = Rc::clone(&feature_guide_window_id);
    let terminal_config_for_actions = terminal_config.clone();
    let terminal_config_path_for_actions = terminal_config_path.clone();

    run_crossterm_desktop_with_actions(
        config,
        move |screen: Rect| {
            let theme = Theme::dark();
            let menu = build_menu(action_tx_for_build.clone());
            let mut desktop = Desktop::new(theme, menu);

            let session = spawn_terminal_window(
                &mut desktop,
                screen,
                1,
                initial_spec_for_build.clone(),
                terminal_config_for_build.clone(),
            )?;
            session.panes.apply_theme(&desktop.theme);
            let terminal_id = session.id;
            terminal_sessions_for_build.borrow_mut().push(session);
            open_feature_guide_window(
                &mut desktop,
                screen,
                &terminal_config_for_build.get(),
                terminal_config_path_for_build.as_deref(),
                &shell_spec_for_build,
                &command_spec_for_build,
                &mut feature_guide_window_id_for_build.borrow_mut(),
            )?;
            desktop.wm.focus(terminal_id);
            refresh_windows_menu(&mut desktop, &action_tx_for_build);

            Ok(desktop)
        },
        action_rx,
        {
            let shell_spec = shell_spec.clone();
            let command_spec = command_spec.clone();
            let action_tx = action_tx_for_actions.clone();
            let terminal_sessions = Rc::clone(&terminal_sessions_for_actions);
            let mut next_window_number = 2usize;

            move |desktop: &mut Desktop, action: TerminalViewerAction, screen: Rect| {
                match action {
                    TerminalViewerAction::NewShellWindow => {
                        let spec = {
                            let mut sessions = terminal_sessions.borrow_mut();
                            sync_session_cwds(&mut sessions);
                            spec_for_new_window(desktop, &sessions, &shell_spec)
                        };
                        let session = spawn_terminal_window(
                            desktop,
                            screen,
                            next_window_number,
                            spec,
                            terminal_config_for_actions.clone(),
                        )?;
                        session.panes.apply_theme(&desktop.theme);
                        terminal_sessions.borrow_mut().push(session);
                        next_window_number = next_window_number.saturating_add(1);
                    }
                    TerminalViewerAction::NewCommandWindow => {
                        let spec = {
                            let mut sessions = terminal_sessions.borrow_mut();
                            sync_session_cwds(&mut sessions);
                            spec_for_new_window(desktop, &sessions, &command_spec)
                        };
                        let session = spawn_terminal_window(
                            desktop,
                            screen,
                            next_window_number,
                            spec,
                            terminal_config_for_actions.clone(),
                        )?;
                        session.panes.apply_theme(&desktop.theme);
                        terminal_sessions.borrow_mut().push(session);
                        next_window_number = next_window_number.saturating_add(1);
                    }
                    TerminalViewerAction::OpenFeatureGuide => {
                        open_feature_guide_window(
                            desktop,
                            screen,
                            &terminal_config_for_actions.get(),
                            terminal_config_path_for_actions.as_deref(),
                            &shell_spec,
                            &command_spec,
                            &mut feature_guide_window_id_for_actions.borrow_mut(),
                        )?;
                    }
                    TerminalViewerAction::OpenSettings => {
                        open_settings_window(
                            desktop,
                            screen,
                            terminal_config_for_actions.clone(),
                            terminal_config_path_for_actions.clone(),
                            &mut settings_window_id_for_actions.borrow_mut(),
                        );
                    }
                    TerminalViewerAction::Quit => return Ok(AppControl::Exit),
                    TerminalViewerAction::FocusNext => desktop.wm.focus_next(),
                    TerminalViewerAction::MinimizeFocused => desktop.wm.minimize_focused(),
                    TerminalViewerAction::ToggleMaximizeFocused => {
                        let work_area = Desktop::layout(screen).work_area;
                        desktop.wm.toggle_maximize_focused(work_area);
                    }
                    TerminalViewerAction::CloseFocused => {
                        if let Some(id) = desktop.wm.focused() {
                            desktop.wm.request_close(id);
                        }
                    }
                    TerminalViewerAction::FocusWindow(id) => {
                        if !desktop.wm.restore_window(id) {
                            desktop.wm.focus(id);
                        }
                    }
                    TerminalViewerAction::CommandContext(action) => {
                        let sessions = terminal_sessions.borrow();
                        apply_command_context_action(
                            desktop,
                            &sessions,
                            &mut command_context_for_actions.borrow_mut(),
                            action,
                        );
                    }
                    TerminalViewerAction::SetTheme(name) => {
                        // Swap the desktop theme (affects window chrome, next
                        // frame) and push the theme's terminal colors into every
                        // live terminal session so ANSI colors track the theme.
                        desktop.theme = Theme::named(name)?;
                        let sessions = terminal_sessions.borrow();
                        for session in sessions.iter() {
                            session.panes.apply_theme(&desktop.theme);
                        }
                    }
                }

                let mut sessions = terminal_sessions.borrow_mut();
                prune_terminal_sessions(desktop, &mut sessions);
                {
                    let mut observer = terminal_config_observer_for_actions.borrow_mut();
                    apply_terminal_config_if_dirty(
                        &terminal_config_for_actions,
                        &mut observer,
                        &sessions,
                    )?;
                }
                sync_session_cwds(&mut sessions);
                sync_terminal_window_titles(desktop, &sessions);
                refresh_windows_menu(desktop, &action_tx);
                Ok(AppControl::Continue)
            }
        },
        move |desktop: &mut Desktop, _screen: Rect| {
            let mut sessions = terminal_sessions_for_tick.borrow_mut();
            update_terminal_exit_prompts(desktop, &mut sessions, &terminal_config_for_tick.get());
            {
                let mut observer = terminal_config_observer_for_tick.borrow_mut();
                apply_terminal_config_if_dirty(
                    &terminal_config_for_tick,
                    &mut observer,
                    &sessions,
                )?;
            }
            sync_session_cwds(&mut sessions);
            sync_terminal_window_titles(desktop, &sessions);
            refresh_windows_menu(desktop, &action_tx_for_tick);
            Ok(AppControl::Continue)
        },
        {
            move |desktop: &mut Desktop, ev, screen, _result| {
                if let Some(mouse) = is_right_mouse_down(ev) {
                    let sessions = terminal_sessions_for_event.borrow();
                    open_command_context_menu(
                        desktop,
                        &sessions,
                        screen,
                        mouse,
                        &action_tx_for_event,
                        &mut command_context_for_event.borrow_mut(),
                    );
                } else {
                    dismiss_context_menu_on_outside_click(
                        desktop,
                        ev,
                        &mut command_context_for_event.borrow_mut(),
                    );
                }
                if is_plain_restart_key(ev) {
                    let mut sessions = terminal_sessions_for_event.borrow_mut();
                    sync_session_cwds(&mut sessions);
                    let restarted = restart_focused_terminal(
                        desktop,
                        &mut sessions,
                        terminal_config_for_event.clone(),
                    )?;
                    if restarted {
                        refresh_windows_menu(desktop, &action_tx_for_event);
                    }
                }
                Ok(AppControl::Continue)
            }
        },
    )
}
