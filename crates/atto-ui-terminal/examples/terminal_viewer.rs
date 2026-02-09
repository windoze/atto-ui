use std::env;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Rect;

use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, MenuItem, MenuSpec,
    run_crossterm_desktop_with_actions,
};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowId, WindowKind, WindowState};
use atto_ui_terminal::{TerminalEmulator, TerminalShortcut};

const WINDOWS_MENU_ID: &str = "atto-ui-terminal:terminal_viewer:windows";
const WINDOWS_MENU_LIST_ID: &str = "atto-ui-terminal:terminal_viewer:windows:list";

#[derive(Clone, Copy, Debug)]
enum TerminalViewerAction {
    NewWindow,
    Quit,
    FocusNext,
    MinimizeFocused,
    ToggleMaximizeFocused,
    CloseFocused,
    FocusWindow(WindowId),
}

fn build_menu(action_tx: mpsc::Sender<TerminalViewerAction>) -> MenuBar {
    MenuBar::new(vec![
        MenuSpec::new(
            "File",
            vec![
                MenuItem::action("New terminal window", {
                    let action_tx = action_tx.clone();
                    move || {
                        let _ = action_tx.send(TerminalViewerAction::NewWindow);
                    }
                })
                .shortcut("n"),
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

fn terminal_release_shortcut() -> TerminalShortcut {
    TerminalShortcut::new(
        KeyCode::Char('l'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )
}

fn terminal_window_rect(work_area: Rect, index: usize) -> Rect {
    let base_width = work_area.width.saturating_sub(6).min(110).max(30);
    let base_height = work_area.height.saturating_sub(6).min(32).max(10);

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

fn spawn_terminal_window(
    desktop: &mut Desktop,
    screen: Rect,
    window_number: usize,
    command: &str,
    command_args: &[String],
) -> Result<WindowId> {
    let work_area = Desktop::layout(screen).work_area;
    let rect = terminal_window_rect(work_area, window_number);

    let mut terminal = TerminalEmulator::new().release_shortcut(terminal_release_shortcut());
    terminal.spawn_process(command, command_args)?;
    let handle = terminal.handle();

    let banner = format!("Terminal Emulator ({window_number})\r\n");
    handle.process_output_str(&banner);
    handle
        .process_output_str("Menu: click the menu bar, or press F10 after releasing capture.\r\n");
    handle.process_output_str("Ctrl+Shift+L: release capture; click terminal to recapture.\r\n");
    handle.process_output_str("\x1b[?1000h\x1b[?1006h");

    let title = format!("Terminal {window_number}");
    let window = Window::new(WindowKind::Normal, title, rect, Box::new(terminal));
    Ok(desktop.add_window(window, screen))
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
            let is_focused = Some(w.id) == focused;
            (title, w.id, is_focused)
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
    let mut args = env::args().skip(1);
    let command = args
        .next()
        .or_else(|| env::var("SHELL").ok())
        .unwrap_or_else(|| "/bin/sh".to_string());
    let command_args: Vec<String> = args.collect();

    let config = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(true)
        .cursor(CursorMode::Hide);

    let (action_tx, action_rx) = mpsc::channel::<TerminalViewerAction>();

    let command_for_build = command.clone();
    let command_args_for_build = command_args.clone();
    let action_tx_for_build = action_tx.clone();

    let action_tx_for_actions = action_tx.clone();
    let action_tx_for_tick = action_tx.clone();

    run_crossterm_desktop_with_actions(
        config,
        move |screen: Rect| {
            let theme = Theme::dark();
            let menu = build_menu(action_tx_for_build.clone());
            let mut desktop = Desktop::new(theme, menu);

            spawn_terminal_window(
                &mut desktop,
                screen,
                1,
                &command_for_build,
                &command_args_for_build,
            )?;
            refresh_windows_menu(&mut desktop, &action_tx_for_build);

            Ok(desktop)
        },
        action_rx,
        {
            let command = command.clone();
            let command_args = command_args.clone();
            let action_tx = action_tx_for_actions.clone();
            let mut next_window_number = 2usize;

            move |desktop: &mut Desktop, action: TerminalViewerAction, screen: Rect| {
                match action {
                    TerminalViewerAction::NewWindow => {
                        spawn_terminal_window(
                            desktop,
                            screen,
                            next_window_number,
                            &command,
                            &command_args,
                        )?;
                        next_window_number = next_window_number.saturating_add(1);
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
                }

                refresh_windows_menu(desktop, &action_tx);
                Ok(AppControl::Continue)
            }
        },
        move |desktop: &mut Desktop, _screen: Rect| {
            refresh_windows_menu(desktop, &action_tx_for_tick);
            Ok(AppControl::Continue)
        },
        |_desktop: &mut Desktop, _ev, _screen, _result| Ok(AppControl::Continue),
    )
}
