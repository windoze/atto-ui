use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::{Frame, Terminal};

use atto_ui::app::{Desktop, MenuBar, MenuItem, MenuSpec};
use atto_ui::composable::{
    Component, EdgeInsets, ScrollContainer, ScrollContainerHost, ScrollContent,
    ScrollContentContext, Splitter,
};
use atto_ui::reactive::EventQueue;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};

#[derive(Clone, Debug)]
struct PaneContent {
    prefix: char,
    rows: u16,
    cols: u16,
}

impl PaneContent {
    fn new(prefix: char, rows: u16, cols: u16) -> Self {
        Self { prefix, rows, cols }
    }

    fn total_height(&self) -> u16 {
        self.rows
    }

    fn line_for_row(&self, row: u16) -> String {
        let mut s = format!("{}{row:03}:", self.prefix);
        for c in 0..self.cols {
            s.push(' ');
            s.push_str(&format!("[col-{c:02}]"));
        }
        s
    }

    fn draw_row(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        style: Style,
        dy: u16,
        row: Option<u16>,
        scroll_x: u16,
    ) {
        let buf = frame.buffer_mut();
        let y = area.y.saturating_add(dy);

        // Clear the row so old content doesn't smear when scrolling.
        for dx in 0..area.width {
            buf[(area.x.saturating_add(dx), y)]
                .set_symbol(" ")
                .set_style(style);
        }

        let Some(row) = row else {
            return;
        };

        let line = self.line_for_row(row);
        let start = scroll_x as usize;
        let visible = if start < line.len() {
            &line[start..]
        } else {
            ""
        };
        buf.set_stringn(area.x, y, visible, area.width as usize, style);
    }
}

impl ScrollContent for PaneContent {
    fn content_size(
        &mut self,
        _viewport: (u16, u16),
        _ctx: ScrollContentContext<'_>,
    ) -> (u16, u16) {
        // Ensure both scrollbars are visible in typical snapshot window sizes.
        let height = self.total_height();
        let width = 5u16.saturating_add(self.cols.saturating_mul(9));
        (width, height)
    }

    fn draw(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ScrollContentContext<'_>,
        _host: &mut ScrollContainerHost,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let style = if ctx.component.is_focused {
            ctx.component.theme.widget.focused
        } else {
            ctx.component.theme.widget.normal
        };

        let scroll = ctx.info.scroll_offset;
        let content_h = self.total_height();

        for dy in 0..area.height {
            let row = scroll.y.saturating_add(dy);
            let row = (row < content_h).then_some(row);
            self.draw_row(frame, area, style, dy, row, scroll.x);
        }
    }
}

fn build_pane(prefix: char) -> ScrollContainer {
    ScrollContainer::new(Box::new(PaneContent::new(prefix, 200, 40))).with_padding(EdgeInsets::ZERO)
}

fn build_splitter_view() -> Box<dyn Component> {
    let left = build_pane('L');
    let right = build_pane('R');
    Box::new(Splitter::vertical(left, right))
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        event::EnableMouseCapture,
        cursor::Show
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let actions: EventQueue<()> = EventQueue::new();
    let menu = MenuBar::new(vec![MenuSpec::new(
        "File",
        vec![
            MenuItem::action("Quit", {
                let actions = actions.clone();
                move || actions.push(())
            })
            .shortcut("q"),
        ],
    )]);
    let mut desktop = Desktop::new(Theme::dark(), menu);

    let screen: Rect = terminal.size()?.into();
    let work = Desktop::layout(screen).work_area;

    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Splitter Scrollbars",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(2),
                width: 50.min(work.width.saturating_sub(2)).max(20),
                height: 14.min(work.height.saturating_sub(2)).max(8),
            },
            build_splitter_view(),
        ),
        screen,
    );

    loop {
        terminal.draw(|f| desktop.draw(f))?;

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let ev = event::read()?;

        let screen: Rect = terminal.size()?.into();
        let _res = desktop.handle_event(&ev, screen);

        if let Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = ev
            && modifiers.contains(KeyModifiers::CONTROL)
        {
            break;
        }

        if actions.pop().is_some() {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}
