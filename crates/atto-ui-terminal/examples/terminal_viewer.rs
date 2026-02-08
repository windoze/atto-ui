use std::env;
use std::time::Duration;

use anyhow::Result;
use ratatui::layout::Rect;

use atto_ui::app::{
    CrosstermAppConfig, CursorMode, Desktop, MenuBar, run_crossterm_desktop_simple,
};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_terminal::{TerminalEmulator, TerminalShortcut};
use crossterm::event::{KeyCode, KeyModifiers};

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

    run_crossterm_desktop_simple(config, move |screen: Rect| {
        let theme = Theme::dark();
        let menu = MenuBar::new(vec![]);
        let mut desktop = Desktop::new(theme, menu);

        let mut terminal = TerminalEmulator::new().release_shortcut(TerminalShortcut::new(
            KeyCode::Char('l'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ));
        terminal.spawn_process(&command, &command_args)?;
        let handle = terminal.handle();

        handle.process_output_str("Terminal Emulator\r\n");
        handle.process_output_str("Ctrl+Shift+L 释放捕获，点击终端重新捕获。\r\n");
        handle.process_output_str("\x1b[?1000h\x1b[?1006h");

        let work = Desktop::layout(screen).work_area;
        let window = Window::new(WindowKind::Normal, "Terminal", work, Box::new(terminal));
        desktop.add_window(window, screen);

        Ok(desktop)
    })
}
