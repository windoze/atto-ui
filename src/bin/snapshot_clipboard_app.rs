use anyhow::Result;
use ratatui::layout::Rect;

use atto_ui::app::{AppHost, CrosstermAppConfig, CursorMode, Desktop, MenuBar};
use atto_ui::composable::Text;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};

fn main() -> Result<()> {
    let config = CrosstermAppConfig::default().cursor(CursorMode::Show);
    let mut host = AppHost::new(config, |screen| {
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
        let work = Desktop::layout(screen).work_area;
        let window = Window::new(
            WindowKind::Normal,
            "Clipboard",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(2),
                width: 36.min(work.width.saturating_sub(2)).max(24),
                height: 8.min(work.height.saturating_sub(2)).max(6),
            },
            Box::new(Text::new("alpha beta\ngamma delta\nomega").selectable(true)),
        );
        let id = desktop.add_window(window, screen);
        desktop.wm.focus(id);
        Ok(desktop)
    })?;
    host.run()
}
