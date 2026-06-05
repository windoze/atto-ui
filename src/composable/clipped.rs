use core::convert::Infallible;

use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Rect, Size};
use ratatui::{Frame, Terminal};

use super::component::{Component, ComponentContext};
use super::scroll::ScrollOffset;

/// Child-local and frame-absolute rectangles for a scrolled child region that is visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScrolledRegion {
    pub(crate) source: Rect,
    pub(crate) dest: Rect,
}

#[derive(Debug)]
struct OffscreenBackend {
    buffer: Buffer,
    cursor_visible: bool,
    cursor_pos: Position,
}

impl OffscreenBackend {
    fn new(width: u16, height: u16) -> Self {
        Self {
            buffer: Buffer::empty(Rect::new(0, 0, width, height)),
            cursor_visible: false,
            cursor_pos: Position::new(0, 0),
        }
    }

    fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    fn cursor_position(&self) -> Position {
        self.cursor_pos
    }
}

impl Backend for OffscreenBackend {
    type Error = Infallible;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        for (x, y, cell) in content {
            if x < self.buffer.area.width && y < self.buffer.area.height {
                self.buffer[(x, y)] = cell.clone();
            }
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.cursor_visible = false;
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.cursor_visible = true;
        Ok(())
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        Ok(self.cursor_pos)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.cursor_pos = position.into();
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.buffer = Buffer::empty(self.buffer.area);
        Ok(())
    }

    fn clear_region(&mut self, _clear_type: ClearType) -> Result<(), Self::Error> {
        self.clear()
    }

    fn size(&self) -> Result<Size, Self::Error> {
        Ok(self.buffer.area.as_size())
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        Ok(WindowSize {
            columns_rows: self.buffer.area.as_size(),
            pixels: Size::new(0, 0),
        })
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub(crate) fn bounds_intersects_viewport(
    bounds: Rect,
    scroll: ScrollOffset,
    viewport: (u16, u16),
) -> bool {
    scrolled_region(
        bounds,
        scroll,
        viewport,
        Rect::new(0, 0, viewport.0, viewport.1),
    )
    .is_some()
}

pub(crate) fn scrolled_region(
    bounds: Rect,
    scroll: ScrollOffset,
    viewport: (u16, u16),
    inner: Rect,
) -> Option<ScrolledRegion> {
    if bounds.width == 0 || bounds.height == 0 || viewport.0 == 0 || viewport.1 == 0 {
        return None;
    }

    let bx0 = bounds.x as u32;
    let by0 = bounds.y as u32;
    let bx1 = bx0.saturating_add(bounds.width as u32);
    let by1 = by0.saturating_add(bounds.height as u32);

    let vx0 = scroll.x as u32;
    let vy0 = scroll.y as u32;
    let vx1 = vx0.saturating_add(viewport.0 as u32);
    let vy1 = vy0.saturating_add(viewport.1 as u32);

    let ix0 = bx0.max(vx0);
    let iy0 = by0.max(vy0);
    let ix1 = bx1.min(vx1);
    let iy1 = by1.min(vy1);

    if ix0 >= ix1 || iy0 >= iy1 {
        return None;
    }

    let width = (ix1 - ix0).min(u16::MAX as u32) as u16;
    let height = (iy1 - iy0).min(u16::MAX as u32) as u16;
    let source_x = (ix0 - bx0).min(u16::MAX as u32) as u16;
    let source_y = (iy0 - by0).min(u16::MAX as u32) as u16;
    let dest_x = inner
        .x
        .saturating_add((ix0 - vx0).min(u16::MAX as u32) as u16);
    let dest_y = inner
        .y
        .saturating_add((iy0 - vy0).min(u16::MAX as u32) as u16);

    Some(ScrolledRegion {
        source: Rect::new(source_x, source_y, width, height),
        dest: Rect::new(dest_x, dest_y, width, height),
    })
}

/// Draw a child into an offscreen buffer and copy only the requested child-local region.
///
/// This keeps parent scroll containers from painting outside their viewport while still allowing
/// the child to render as if it had its full layout rectangle.
pub(crate) fn draw_component_region(
    frame: &mut Frame<'_>,
    component: &mut dyn Component,
    component_area: Rect,
    source: Rect,
    dest: Rect,
    ctx: ComponentContext<'_>,
) {
    if component_area.width == 0
        || component_area.height == 0
        || source.width == 0
        || source.height == 0
        || dest.width == 0
        || dest.height == 0
    {
        return;
    }

    let background = visible_background(frame, source, dest);
    let backend = OffscreenBackend::new(component_area.width, component_area.height);
    let mut terminal = Terminal::new(backend).expect("create offscreen terminal");
    terminal
        .try_draw(|f| {
            seed_visible_background(f.buffer_mut(), &background);
            component.draw(f, component_area, ctx);
            Ok::<(), Infallible>(())
        })
        .expect("draw clipped component");

    let backend = terminal.backend();
    copy_visible_region(frame, backend.buffer(), source, dest);
    if backend.cursor_visible() && ctx.is_focused {
        copy_visible_cursor(frame, backend.cursor_position(), source, dest);
    }
}

fn visible_background(frame: &mut Frame<'_>, source: Rect, dest: Rect) -> Vec<(u16, u16, Cell)> {
    let mut cells = Vec::with_capacity(source.width as usize * source.height as usize);
    let buf = frame.buffer_mut();
    for dy in 0..source.height {
        for dx in 0..source.width {
            let dst_x = dest.x.saturating_add(dx);
            let dst_y = dest.y.saturating_add(dy);
            let Some(cell) = buf.cell((dst_x, dst_y)) else {
                continue;
            };
            cells.push((
                source.x.saturating_add(dx),
                source.y.saturating_add(dy),
                cell.clone(),
            ));
        }
    }
    cells
}

fn seed_visible_background(buf: &mut Buffer, cells: &[(u16, u16, Cell)]) {
    for (x, y, cell) in cells {
        if let Some(dst) = buf.cell_mut((*x, *y)) {
            *dst = cell.clone();
        }
    }
}

fn copy_visible_region(frame: &mut Frame<'_>, source_buf: &Buffer, source: Rect, dest: Rect) {
    let dst_buf = frame.buffer_mut();
    for dy in 0..source.height {
        for dx in 0..source.width {
            let src_x = source.x.saturating_add(dx);
            let src_y = source.y.saturating_add(dy);
            let dst_x = dest.x.saturating_add(dx);
            let dst_y = dest.y.saturating_add(dy);
            let Some(src) = source_buf.cell((src_x, src_y)) else {
                continue;
            };
            if let Some(dst) = dst_buf.cell_mut((dst_x, dst_y)) {
                *dst = src.clone();
            }
        }
    }
}

fn copy_visible_cursor(frame: &mut Frame<'_>, cursor: Position, source: Rect, dest: Rect) {
    let within_x = cursor.x >= source.x && cursor.x < source.x.saturating_add(source.width);
    let within_y = cursor.y >= source.y && cursor.y < source.y.saturating_add(source.height);
    if within_x && within_y {
        let x = dest.x.saturating_add(cursor.x.saturating_sub(source.x));
        let y = dest.y.saturating_add(cursor.y.saturating_sub(source.y));
        frame.set_cursor_position((x, y));
    }
}
