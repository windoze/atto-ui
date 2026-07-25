//! Regression test for wide-character bleed from a terminal-emulator background
//! into a foreground dialog window.
//!
//! A `TerminalEmulator` marks the continuation cell of every wide glyph (CJK,
//! emoji) with ratatui's `skip` flag, and leaves the glyph head occupying two
//! columns. When a dialog opens over the terminal, the window compositor must
//! (a) clear those skip flags and (b) blank any wide-glyph head whose right half
//! the dialog covers, otherwise the render diff drops the dialog's content and
//! the terminal bleeds through — the jagged interior gaps seen in the bug report.

use atto_ui::app::{Desktop, MenuBar};
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_terminal::TerminalEmulator;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

fn screen() -> Rect {
    Rect::new(0, 0, 80, 24)
}

/// A minimal dialog body that fills its inner area with a distinctive glyph so we
/// can confirm the dialog's own content actually reaches the screen buffer.
struct FillView(char);

impl atto_ui::composable::Component for FillView {
    fn draw(
        &mut self,
        frame: &mut ratatui::Frame<'_>,
        area: Rect,
        _ctx: atto_ui::composable::ComponentContext<'_>,
    ) {
        let buf = frame.buffer_mut();
        let glyph = self.0.to_string();
        for y in area.y..area.y.saturating_add(area.height) {
            for x in area.x..area.x.saturating_add(area.width) {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(&glyph);
                }
            }
        }
    }
}

impl atto_ui::composable::EventHandling for FillView {
    fn handle_event(
        &mut self,
        _event: &crossterm::event::Event,
        _ctx: atto_ui::composable::ComponentContext<'_>,
    ) -> atto_ui::composable::EventResult {
        atto_ui::composable::EventResult::ignored()
    }
}

atto_ui::impl_component_default_traits!(FillView => Layout, Scrollable, FocusNav, DynamicTree);

#[test]
fn dialog_over_terminal_emoji_background_renders_without_gaps() {
    // Background: a terminal emulator filled with emoji + CJK so many rows carry
    // wide glyphs (skip-flagged continuation cells) across the screen.
    let terminal_widget = TerminalEmulator::new();
    let handle = terminal_widget.handle();
    // Several lines of wide glyphs at varying offsets, mimicking a shell prompt
    // with emoji (📦 🐍 🦀) and CJK, so wide-char columns land all over the grid.
    for row in 0..12 {
        // Leading spaces shift the wide glyphs a bit per row so their continuation
        // cells fall on different columns.
        let pad = " ".repeat(row % 4);
        handle.process_output_str(&format!("{pad}📦 v0.1.0 via 🐍 v3.14 via 🦀 你好世界\r\n"));
    }

    let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
    let term_rect = Rect::new(1, 1, 78, 20);
    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Terminal 1",
            term_rect,
            Box::new(terminal_widget),
        ),
        screen(),
    );

    // Draw twice on the SAME terminal so frame 2 is an incremental diff against
    // frame 1 — the condition under which the render diff would skip cells.
    let backend = TestBackend::new(screen().width, screen().height);
    let mut term = Terminal::new(backend).expect("terminal");
    term.draw(|f| desktop.draw(f)).expect("draw frame 1");

    // Frame 2: open a dialog over the terminal, filled with '#'.
    let dialog_rect = Rect::new(10, 4, 50, 14);
    desktop.add_window(
        Window::new(
            WindowKind::Tooltip,
            "Terminal Settings",
            dialog_rect,
            Box::new(FillView('#')),
        ),
        screen(),
    );
    term.draw(|f| desktop.draw(f)).expect("draw frame 2");

    // Every cell strictly inside the dialog (excluding its 1-cell border) must be
    // the dialog's own fill glyph, with no skip flag left over from the terminal.
    let buf = term.backend().buffer();
    let inner = Rect::new(
        dialog_rect.x + 1,
        dialog_rect.y + 1,
        dialog_rect.width - 2,
        dialog_rect.height - 2,
    );
    let mut bad = Vec::new();
    for y in inner.y..inner.y + inner.height {
        for x in inner.x..inner.x + inner.width {
            let cell = buf.cell((x, y)).expect("dialog cell");
            if cell.skip || cell.symbol() != "#" {
                bad.push((x, y, cell.symbol().to_string(), cell.skip));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "dialog interior has {} corrupted cells (skip flag or wrong glyph): {:?}\n{buf:?}",
        bad.len(),
        &bad[..bad.len().min(12)]
    );
}
