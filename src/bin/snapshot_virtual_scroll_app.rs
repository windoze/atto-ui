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
use atto_ui::reactive::EventQueue;
use atto_ui::theme::Theme;
use atto_ui::views::{EdgeInsets, ScrollContent, ScrollContentContext, ScrollView, ScrollViewHost};
use atto_ui::wm::{Window, WindowKind};

#[derive(Clone, Debug)]
struct VirtualGridContent {
    rows: u16,
    cols: u16,
}

impl VirtualGridContent {
    fn new(rows: u16, cols: u16) -> Self {
        Self { rows, cols }
    }

    fn total_height(&self) -> u16 {
        // Row 0 is a header.
        1u16.saturating_add(self.rows)
    }

    fn line_for_row(&self, row: u16) -> String {
        if row == 0 {
            return "Virtual scroll test: wheel/drag/arrow buttons (Ctrl+Q quits)".to_string();
        }

        let idx = row - 1;
        let mut s = format!("{idx:04}:");
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

impl ScrollContent for VirtualGridContent {
    fn content_size(
        &mut self,
        _viewport: (u16, u16),
        _ctx: ScrollContentContext<'_>,
    ) -> (u16, u16) {
        // Ensure both scrollbars are visible in typical snapshot window sizes.
        //
        // - Height includes a header row.
        // - Width is based on a fixed virtual column count.
        let height = self.total_height();
        let width = 6u16.saturating_add(self.cols.saturating_mul(9));
        (width, height)
    }

    fn draw(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ScrollContentContext<'_>,
        _host: &mut ScrollViewHost,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let style = if ctx.view.is_focused {
            ctx.view.theme.widget.focused
        } else {
            ctx.view.theme.widget.normal
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

fn build_virtual_scroll_view() -> ScrollView {
    ScrollView::new(Box::new(VirtualGridContent::new(1000, 40)))
        .with_padding(EdgeInsets::symmetric(1, 1))
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
            "Virtual Scroll",
            Rect {
                x: work.x.saturating_add(2),
                y: work.y.saturating_add(2),
                width: 50.min(work.width.saturating_sub(2)).max(20),
                height: 14.min(work.height.saturating_sub(2)).max(8),
            },
            Box::new(build_virtual_scroll_view()),
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
