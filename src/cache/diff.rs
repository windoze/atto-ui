use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;

/// Represents a region that has changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyRegion {
    pub rect: Rect,
}

/// Diff two buffers and return dirty regions.
pub fn diff_buffers(previous: &Buffer, current: &Buffer) -> Vec<DirtyRegion> {
    if previous.area != current.area {
        return vec![DirtyRegion { rect: current.area }];
    }

    if previous.content == current.content {
        return Vec::new();
    }

    let w = current.area.width;
    let h = current.area.height;
    if w == 0 || h == 0 {
        return Vec::new();
    }

    let mut regions = Vec::new();

    for dy in 0..h {
        let mut run_start: Option<u16> = None;
        let mut run_len: u16 = 0;

        for dx in 0..w {
            let idx = (dy as usize) * (w as usize) + (dx as usize);

            if idx >= previous.content.len() || idx >= current.content.len() {
                continue;
            }

            let changed = cells_differ(&previous.content[idx], &current.content[idx]);
            match (run_start, changed) {
                (None, true) => {
                    run_start = Some(dx);
                    run_len = 1;
                }
                (Some(_), true) => {
                    run_len = run_len.saturating_add(1);
                }
                (Some(start), false) => {
                    regions.push(DirtyRegion {
                        rect: Rect {
                            x: current.area.x.saturating_add(start),
                            y: current.area.y.saturating_add(dy),
                            width: run_len,
                            height: 1,
                        },
                    });
                    run_start = None;
                    run_len = 0;
                }
                (None, false) => {}
            }
        }

        if let Some(start) = run_start.take() {
            regions.push(DirtyRegion {
                rect: Rect {
                    x: current.area.x.saturating_add(start),
                    y: current.area.y.saturating_add(dy),
                    width: run_len,
                    height: 1,
                },
            });
        }
    }

    regions
}

/// Calculate dirty percentage (0.0 = no changes, 1.0 = full redraw).
pub fn dirty_percentage(previous: &Buffer, current: &Buffer) -> f32 {
    if previous.area != current.area {
        return 1.0;
    }

    let total_cells = (current.area.width as usize) * (current.area.height as usize);
    if total_cells == 0 {
        return 0.0;
    }

    let dirty_cells = previous
        .content
        .iter()
        .zip(current.content.iter())
        .filter(|(a, b)| cells_differ(a, b))
        .count();

    dirty_cells as f32 / total_cells as f32
}

fn cells_differ(a: &Cell, b: &Cell) -> bool {
    a.symbol() != b.symbol() || a.style() != b.style()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Style;

    #[test]
    fn diff_no_changes() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 5,
        };
        let buf1 = Buffer::empty(area);
        let buf2 = Buffer::empty(area);

        let diff = diff_buffers(&buf1, &buf2);
        assert!(diff.is_empty(), "no changes should yield empty diff");
    }

    #[test]
    fn diff_size_change_full_redraw() {
        let area1 = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 5,
        };
        let area2 = Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 10,
        };

        let buf1 = Buffer::empty(area1);
        let buf2 = Buffer::empty(area2);

        let diff = diff_buffers(&buf1, &buf2);
        assert_eq!(diff.len(), 1, "size change should trigger full redraw");
        assert_eq!(diff[0].rect, area2);
    }

    #[test]
    fn diff_single_change() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 5,
        };
        let buf1 = Buffer::empty(area);
        let mut buf2 = Buffer::empty(area);

        buf2.set_string(2, 1, "X", Style::default());

        let diff = diff_buffers(&buf1, &buf2);
        assert!(!diff.is_empty(), "should detect change");
        assert!(diff.iter().any(|r| r.rect.y == 1), "should include row 1");
    }

    #[test]
    fn dirty_percentage_no_change() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 5,
        };
        let buf1 = Buffer::empty(area);
        let buf2 = Buffer::empty(area);

        let pct = dirty_percentage(&buf1, &buf2);
        assert_eq!(pct, 0.0);
    }

    #[test]
    fn dirty_percentage_full_change() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 5,
        };
        let buf1 = Buffer::empty(area);
        let mut buf2 = Buffer::empty(area);

        for y in 0..5 {
            for x in 0..10 {
                buf2.set_string(x, y, "X", Style::default());
            }
        }

        let pct = dirty_percentage(&buf1, &buf2);
        assert!(pct > 0.99, "should be nearly 100% dirty");
    }
}
