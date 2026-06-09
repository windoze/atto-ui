use std::fmt;
use std::sync::Arc;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use parking_lot::Mutex;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::composable::EventResult;
use crate::reactive::Binding;
use crate::theme::Theme;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatusSegmentAlign {
    #[default]
    Left,
    Right,
}

#[derive(Clone)]
pub struct StatusSegment {
    pub id: String,
    pub text: Binding<String>,
    pub style: Option<String>,
    pub align: StatusSegmentAlign,
    pub min_width: u16,
    pub priority: u16,
    pub on_click: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl StatusSegment {
    pub fn new(id: impl Into<String>, text: impl Into<Binding<String>>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            style: None,
            align: StatusSegmentAlign::Left,
            min_width: 0,
            priority: 0,
            on_click: None,
        }
    }

    pub fn style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }

    pub fn align(mut self, align: StatusSegmentAlign) -> Self {
        self.align = align;
        self
    }

    pub fn min_width(mut self, min_width: u16) -> Self {
        self.min_width = min_width;
        self
    }

    pub fn priority(mut self, priority: u16) -> Self {
        self.priority = priority;
        self
    }

    pub fn on_click<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_click = Some(Arc::new(callback));
        self
    }
}

impl fmt::Debug for StatusSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StatusSegment")
            .field("id", &self.id)
            .field("text", &self.text)
            .field("style", &self.style)
            .field("align", &self.align)
            .field("min_width", &self.min_width)
            .field("priority", &self.priority)
            .field("on_click", &self.on_click.as_ref().map(|_| "<callback>"))
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StatusSegmentHitBox {
    id: String,
    rect: Rect,
}

#[derive(Clone, Debug)]
pub struct StatusBar {
    left: String,
    right: String,
    custom: Option<(String, String)>,
    segments: Vec<StatusSegment>,
    hit_boxes: Arc<Mutex<Vec<StatusSegmentHitBox>>>,
    last_separator_width: Arc<Mutex<usize>>,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self {
            left: String::new(),
            right: String::new(),
            custom: None,
            segments: Vec::new(),
            hit_boxes: Arc::new(Mutex::new(Vec::new())),
            last_separator_width: Arc::new(Mutex::new(1)),
        }
    }
}

impl StatusBar {
    pub fn set_left(&mut self, text: impl Into<String>) {
        self.left = text.into();
    }

    pub fn set_right(&mut self, text: impl Into<String>) {
        self.right = text.into();
    }

    pub fn set_custom(&mut self, left: impl Into<String>, right: impl Into<String>) {
        self.custom = Some((left.into(), right.into()));
        self.clear_segments();
    }

    pub fn clear_custom(&mut self) {
        self.custom = None;
    }

    pub fn has_custom(&self) -> bool {
        self.custom.is_some()
    }

    pub fn has_segments(&self) -> bool {
        !self.segments.is_empty()
    }

    pub fn set_segments(&mut self, segments: Vec<StatusSegment>) {
        self.segments = segments;
        self.custom = None;
        self.hit_boxes.lock().clear();
    }

    pub fn push_segment(&mut self, segment: StatusSegment) {
        self.segments.push(segment);
        self.custom = None;
        self.hit_boxes.lock().clear();
    }

    pub fn clear_segments(&mut self) {
        self.segments.clear();
        self.hit_boxes.lock().clear();
    }

    pub fn handle_mouse(&self, event: &MouseEvent, area: Rect) -> EventResult {
        if area.height == 0
            || event.kind != MouseEventKind::Down(MouseButton::Left)
            || !contains(area, event.column, event.row)
        {
            return EventResult::ignored();
        }

        if let Some(id) = self
            .hit_boxes
            .lock()
            .iter()
            .find_map(|hit| contains(hit.rect, event.column, event.row).then(|| hit.id.clone()))
        {
            return self.trigger_segment(&id);
        }

        let separator_width = *self.last_separator_width.lock();
        let fallback_layout = layout_status_segments(&self.segments, area, separator_width);
        if let Some(id) = fallback_layout.segments.iter().find_map(|layout| {
            contains(layout.rect, event.column, event.row).then(|| layout.id.clone())
        }) {
            return self.trigger_segment(&id);
        }

        EventResult::ignored()
    }

    fn trigger_segment(&self, id: &str) -> EventResult {
        let Some(segment) = self.segments.iter().find(|segment| segment.id == id) else {
            return EventResult::ignored();
        };
        if let Some(callback) = &segment.on_click {
            callback();
            return EventResult::submitted();
        }
        EventResult::consumed()
    }

    pub fn draw(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }

        frame.render_widget(
            Fill {
                style: theme.status_bar,
                ch: ' ',
            },
            area,
        );

        if !self.segments.is_empty() {
            self.draw_segments(frame, area, theme);
            return;
        }

        let width = area.width as usize;
        let (left, right) = self
            .custom
            .as_ref()
            .map(|(left, right)| (left.as_str(), right.as_str()))
            .unwrap_or((self.left.as_str(), self.right.as_str()));
        let line = build_status_line(left, right, width);

        frame
            .buffer_mut()
            .set_stringn(area.x, area.y, line, width, theme.status_bar);
    }

    fn draw_segments(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let separator = theme.glyph("status-separator").unwrap_or(" ");
        let separator_width = UnicodeWidthStr::width(separator).max(1);
        *self.last_separator_width.lock() = separator_width;
        let layout = layout_status_segments(&self.segments, area, separator_width);

        let buf = frame.buffer_mut();
        let separator_style = theme
            .named_style("status-segment")
            .unwrap_or(theme.status_bar);
        for separator_layout in &layout.separators {
            draw_text_fragment(buf, separator_layout.rect, separator, separator_style);
        }

        for segment_layout in &layout.segments {
            let style = self.segments[segment_layout.index]
                .style
                .as_deref()
                .and_then(|name| theme.named_style(name))
                .unwrap_or_else(|| {
                    theme
                        .named_style("status-segment")
                        .unwrap_or(theme.status_bar)
                });
            draw_text_fragment(buf, segment_layout.rect, &segment_layout.text, style);
        }

        *self.hit_boxes.lock() = layout
            .segments
            .iter()
            .map(|layout| StatusSegmentHitBox {
                id: layout.id.clone(),
                rect: layout.rect,
            })
            .collect();
    }
}

#[derive(Clone, Debug)]
struct StatusCandidate {
    index: usize,
    id: String,
    text: String,
    align: StatusSegmentAlign,
    priority: u16,
    width: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StatusSegmentLayout {
    index: usize,
    id: String,
    text: String,
    rect: Rect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StatusSeparatorLayout {
    rect: Rect,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct StatusLayout {
    segments: Vec<StatusSegmentLayout>,
    separators: Vec<StatusSeparatorLayout>,
}

fn layout_status_segments(
    segments: &[StatusSegment],
    area: Rect,
    separator_width: usize,
) -> StatusLayout {
    let width = area.width as usize;
    if width == 0 || area.height == 0 {
        return StatusLayout::default();
    }

    let separator_width = separator_width.max(1);
    let candidates = status_candidates(segments);
    let mut visible: Vec<usize> = candidates
        .iter()
        .filter(|candidate| candidate.width > 0)
        .map(|candidate| candidate.index)
        .collect();

    while required_width(&visible, &candidates, separator_width) > width && visible.len() > 1 {
        let remove_pos = lowest_priority_position(&visible, &candidates);
        visible.remove(remove_pos);
    }

    let left: Vec<usize> = visible
        .iter()
        .copied()
        .filter(|idx| candidates[*idx].align == StatusSegmentAlign::Left)
        .collect();
    let right: Vec<usize> = visible
        .iter()
        .copied()
        .filter(|idx| candidates[*idx].align == StatusSegmentAlign::Right)
        .collect();

    let right_full_width = group_width(&right, &candidates, separator_width);
    let right_width = right_full_width.min(width);
    let left_width = width.saturating_sub(right_width);

    let mut layout = StatusLayout::default();
    push_group_layout(
        &mut layout,
        &left,
        &candidates,
        area.x,
        area.y,
        area.height,
        left_width,
        separator_width,
    );

    let right_visual: Vec<usize> = right.iter().rev().copied().collect();
    let right_x = area
        .x
        .saturating_add((width.saturating_sub(right_width)) as u16);
    push_group_layout(
        &mut layout,
        &right_visual,
        &candidates,
        right_x,
        area.y,
        area.height,
        right_width,
        separator_width,
    );

    layout
}

fn status_candidates(segments: &[StatusSegment]) -> Vec<StatusCandidate> {
    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let text = segment.text.get();
            let text_width = UnicodeWidthStr::width(text.as_str());
            StatusCandidate {
                index,
                id: segment.id.clone(),
                text,
                align: segment.align,
                priority: segment.priority,
                width: text_width.max(segment.min_width as usize),
            }
        })
        .collect()
}

fn required_width(
    indices: &[usize],
    candidates: &[StatusCandidate],
    separator_width: usize,
) -> usize {
    let left: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|idx| candidates[*idx].align == StatusSegmentAlign::Left)
        .collect();
    let right: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|idx| candidates[*idx].align == StatusSegmentAlign::Right)
        .collect();
    group_width(&left, candidates, separator_width)
        + group_width(&right, candidates, separator_width)
}

fn group_width(indices: &[usize], candidates: &[StatusCandidate], separator_width: usize) -> usize {
    if indices.is_empty() {
        return 0;
    }
    indices
        .iter()
        .map(|idx| candidates[*idx].width)
        .sum::<usize>()
        + separator_width.saturating_mul(indices.len().saturating_sub(1))
}

fn lowest_priority_position(indices: &[usize], candidates: &[StatusCandidate]) -> usize {
    let mut remove_pos = 0;
    for (pos, idx) in indices.iter().enumerate().skip(1) {
        let current = &candidates[*idx];
        let selected = &candidates[indices[remove_pos]];
        if current.priority < selected.priority
            || (current.priority == selected.priority && pos > remove_pos)
        {
            remove_pos = pos;
        }
    }
    remove_pos
}

#[allow(clippy::too_many_arguments)]
fn push_group_layout(
    layout: &mut StatusLayout,
    indices: &[usize],
    candidates: &[StatusCandidate],
    start_x: u16,
    y: u16,
    height: u16,
    max_width: usize,
    separator_width: usize,
) {
    if max_width == 0 {
        return;
    }

    let mut cursor = 0usize;
    for (pos, idx) in indices.iter().enumerate() {
        if pos > 0 {
            let available = max_width.saturating_sub(cursor);
            if available == 0 {
                break;
            }
            let width = separator_width.min(available);
            layout.separators.push(StatusSeparatorLayout {
                rect: Rect {
                    x: start_x.saturating_add(cursor as u16),
                    y,
                    width: width as u16,
                    height,
                },
            });
            cursor = cursor.saturating_add(width);
        }

        let available = max_width.saturating_sub(cursor);
        if available == 0 {
            break;
        }
        let candidate = &candidates[*idx];
        let width = candidate.width.min(available);
        if width == 0 {
            continue;
        }
        layout.segments.push(StatusSegmentLayout {
            index: candidate.index,
            id: candidate.id.clone(),
            text: candidate.text.clone(),
            rect: Rect {
                x: start_x.saturating_add(cursor as u16),
                y,
                width: width as u16,
                height,
            },
        });
        cursor = cursor.saturating_add(width);
    }
}

fn contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x
        && x < area.x.saturating_add(area.width)
        && y >= area.y
        && y < area.y.saturating_add(area.height)
}

fn draw_text_fragment(buf: &mut Buffer, rect: Rect, text: &str, style: Style) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let rendered = text_to_width(text, rect.width as usize);
    let right = rect.x.saturating_add(rect.width);
    let mut x = rect.x;
    for grapheme in rendered.graphemes(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme).max(1) as u16;
        if x.saturating_add(grapheme_width) > right {
            break;
        }

        if let Some(cell) = buf.cell_mut((x, rect.y)) {
            cell.set_symbol(grapheme).set_style(style).set_skip(false);
        }
        for dx in 1..grapheme_width {
            if let Some(cell) = buf.cell_mut((x.saturating_add(dx), rect.y)) {
                cell.set_symbol(" ").set_skip(true).set_style(style);
            }
        }
        x = x.saturating_add(grapheme_width);
    }

    while x < right {
        if let Some(cell) = buf.cell_mut((x, rect.y)) {
            cell.set_symbol(" ").set_style(style).set_skip(false);
        }
        x = x.saturating_add(1);
    }
}

fn text_to_width(text: &str, width: usize) -> String {
    let mut out = String::new();
    let used = push_graphemes_up_to_width(&mut out, text, width);
    out.push_str(&" ".repeat(width.saturating_sub(used)));
    out
}

fn build_status_line(left: &str, right: &str, width: usize) -> String {
    let left_w = UnicodeWidthStr::width(left);
    let right_w = UnicodeWidthStr::width(right);
    let mut line = String::new();

    if left_w >= width {
        let used = push_graphemes_up_to_width(&mut line, left, width);
        line.push_str(&" ".repeat(width.saturating_sub(used)));
        return line;
    }

    line.push_str(left);
    let remaining = width.saturating_sub(left_w);
    if right_w <= remaining {
        line.push_str(&" ".repeat(remaining.saturating_sub(right_w)));
        line.push_str(right);
    } else {
        line.push_str(&" ".repeat(remaining));
    }
    line
}

fn push_graphemes_up_to_width(out: &mut String, text: &str, max_width: usize) -> usize {
    if max_width == 0 {
        return 0;
    }

    let mut used = 0usize;
    for (_, grapheme) in text.grapheme_indices(true) {
        let grapheme_w = UnicodeWidthStr::width(grapheme);
        let next = used.saturating_add(grapheme_w);
        if next > max_width {
            break;
        }
        out.push_str(grapheme);
        used = next;
    }
    used
}

/// Utility widget for filling areas with a style.
pub(crate) struct Fill {
    pub style: ratatui::style::Style,
    pub ch: char,
}

impl Widget for Fill {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let s = self.ch.to_string();
        let style = ratatui::style::Style::reset().patch(self.style);
        for y in area.y..area.y.saturating_add(area.height) {
            for x in area.x..area.x.saturating_add(area.width) {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(style);
                    cell.set_symbol(&s);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn status_line_aligns_right_text_by_display_width() {
        let line = build_status_line("状态", "🦀", 12);

        assert_eq!(UnicodeWidthStr::width(line.as_str()), 12);
        assert!(line.starts_with("状态"));
        assert!(line.ends_with("🦀"));
    }

    #[test]
    fn status_line_right_aligns_ascii() {
        let line = build_status_line("left", "right", 12);

        assert_eq!(line, "left   right");
        assert_eq!(UnicodeWidthStr::width(line.as_str()), 12);
    }

    #[test]
    fn status_line_right_aligns_cjk() {
        let line = build_status_line("左", "右", 10);

        assert_eq!(line, "左      右");
        assert_eq!(UnicodeWidthStr::width(line.as_str()), 10);
    }

    #[test]
    fn status_line_right_aligns_mixed_width_text() {
        let line = build_status_line("A你", "B🦀", 10);

        assert_eq!(line, "A你    B🦀");
        assert_eq!(UnicodeWidthStr::width(line.as_str()), 10);
    }

    #[test]
    fn status_line_truncates_left_on_grapheme_boundary() {
        let line = build_status_line("你好你好", "", 5);

        assert_eq!(line, "你好 ");
        assert_eq!(UnicodeWidthStr::width(line.as_str()), 5);
    }

    #[test]
    fn status_line_truncates_emoji_sequence_on_grapheme_boundary() {
        let line = build_status_line("👨‍👩‍👧‍👦abc", "", 1);

        assert_eq!(line, " ");
        assert_eq!(UnicodeWidthStr::width(line.as_str()), 1);
    }

    #[test]
    fn segments_align_mixed_width_text_left_and_right() {
        let mut status = StatusBar::default();
        status.set_segments(vec![
            StatusSegment::new("left", "状态"),
            StatusSegment::new("emoji", "🦀").align(StatusSegmentAlign::Right),
        ]);
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(12, 1)).expect("terminal");

        terminal
            .draw(|frame| status.draw(frame, Rect::new(0, 0, 12, 1), &theme))
            .expect("draw status");

        let buf = terminal.backend().buffer();
        assert_eq!(buf[(0, 0)].symbol(), "状");
        assert_eq!(buf[(2, 0)].symbol(), "态");
        assert_eq!(buf[(10, 0)].symbol(), "🦀");
        assert_eq!(buf[(9, 0)].style().fg, theme.status_bar.fg);
        assert_eq!(buf[(9, 0)].style().bg, theme.status_bar.bg);
    }

    #[test]
    fn segments_hide_low_priority_before_truncating() {
        let mut status = StatusBar::default();
        status.set_segments(vec![
            StatusSegment::new("keep", "KEEP").priority(10),
            StatusSegment::new("drop", "DROP").priority(1),
        ]);
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(6, 1)).expect("terminal");

        terminal
            .draw(|frame| status.draw(frame, Rect::new(0, 0, 6, 1), &theme))
            .expect("draw status");

        let row = row_contents(terminal.backend().buffer(), 6);
        assert!(
            row.contains("KEEP"),
            "expected high priority segment in {row:?}"
        );
        assert!(
            !row.contains("DROP"),
            "expected low priority segment hidden in {row:?}"
        );
    }

    #[test]
    fn segment_truncation_preserves_grapheme_boundaries() {
        let mut status = StatusBar::default();
        status.set_segments(vec![StatusSegment::new("family", "👨‍👩‍👧‍👦abc").priority(10)]);
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(1, 1)).expect("terminal");

        terminal
            .draw(|frame| status.draw(frame, Rect::new(0, 0, 1, 1), &theme))
            .expect("draw status");

        let row = row_contents(terminal.backend().buffer(), 1);
        assert_eq!(row, " ");
    }

    #[test]
    fn segment_click_hit_test_invokes_callback() {
        let clicks = Arc::new(AtomicUsize::new(0));
        let mut status = StatusBar::default();
        status.set_segments(vec![StatusSegment::new("click", "Click").on_click({
            let clicks = Arc::clone(&clicks);
            move || {
                clicks.fetch_add(1, Ordering::SeqCst);
            }
        })]);
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(20, 1)).expect("terminal");
        terminal
            .draw(|frame| status.draw(frame, Rect::new(0, 0, 20, 1), &theme))
            .expect("draw status");

        let result = status.handle_mouse(
            &MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            Rect::new(0, 0, 20, 1),
        );

        assert!(result.is_consumed());
        assert_eq!(clicks.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn segment_click_fallback_uses_last_drawn_separator_width() {
        let clicks = Arc::new(AtomicUsize::new(0));
        let mut status = StatusBar::default();
        let mut theme = Theme::dark();
        theme.set_glyph("status-separator", "||");

        let segments = |clicks: &Arc<AtomicUsize>| {
            vec![
                StatusSegment::new("left", "A"),
                StatusSegment::new("right", "B").on_click({
                    let clicks = Arc::clone(clicks);
                    move || {
                        clicks.fetch_add(1, Ordering::SeqCst);
                    }
                }),
            ]
        };

        status.set_segments(segments(&clicks));
        let mut terminal = Terminal::new(TestBackend::new(8, 1)).expect("terminal");
        terminal
            .draw(|frame| status.draw(frame, Rect::new(0, 0, 8, 1), &theme))
            .expect("draw status");
        assert!(row_contents(terminal.backend().buffer(), 8).starts_with("A||B"));

        // Updating segments clears cached hit boxes; fallback layout must still match the
        // last drawn separator width until the next frame repopulates hit boxes.
        status.set_segments(segments(&clicks));

        let separator_click = status.handle_mouse(
            &MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            Rect::new(0, 0, 8, 1),
        );
        assert!(!separator_click.is_consumed());
        assert_eq!(clicks.load(Ordering::SeqCst), 0);

        let segment_click = status.handle_mouse(
            &MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 3,
                row: 0,
                modifiers: KeyModifiers::NONE,
            },
            Rect::new(0, 0, 8, 1),
        );

        assert!(segment_click.is_consumed());
        assert_eq!(clicks.load(Ordering::SeqCst), 1);
    }

    fn row_contents(buf: &Buffer, width: u16) -> String {
        let mut row = String::new();
        for x in 0..width {
            row.push_str(buf[(x, 0)].symbol());
        }
        row
    }
}
