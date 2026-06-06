use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;

const DEFAULT_CAPACITY: usize = 5;
const DEFAULT_DURATION: Duration = Duration::from_secs(4);
const MAX_VISIBLE_TOASTS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastLevel {
    fn title(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Success => "Done",
            Self::Warning => "Warning",
            Self::Error => "Error",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Toast {
    message: String,
    level: ToastLevel,
    expires_at: Instant,
}

impl Toast {
    pub fn new(message: impl Into<String>, level: ToastLevel, duration: Duration) -> Self {
        Self {
            message: message.into(),
            level,
            expires_at: Instant::now() + duration,
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message, ToastLevel::Info, DEFAULT_DURATION)
    }

    pub fn success(message: impl Into<String>) -> Self {
        Self::new(message, ToastLevel::Success, DEFAULT_DURATION)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(message, ToastLevel::Warning, DEFAULT_DURATION)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(message, ToastLevel::Error, DEFAULT_DURATION)
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn level(&self) -> ToastLevel {
        self.level
    }
}

#[derive(Clone, Debug)]
pub struct ToastQueue {
    entries: VecDeque<Toast>,
    capacity: usize,
}

impl Default for ToastQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl ToastQueue {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            capacity: DEFAULT_CAPACITY,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn push(&mut self, toast: Toast) {
        self.prune_expired(Instant::now());
        while self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(toast);
    }

    pub fn push_message(
        &mut self,
        level: ToastLevel,
        message: impl Into<String>,
        duration: Duration,
    ) {
        self.push(Toast::new(message, level, duration));
    }

    pub fn notify_background_complete(&mut self, message: impl Into<String>) {
        self.push(Toast::success(message));
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&mut self) -> usize {
        self.prune_expired(Instant::now());
        self.entries.len()
    }

    pub fn is_empty(&mut self) -> bool {
        self.len() == 0
    }

    pub(crate) fn prune_expired(&mut self, now: Instant) {
        self.entries.retain(|toast| toast.expires_at > now);
    }

    pub(crate) fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        self.prune_expired(Instant::now());
        if area.width == 0 || area.height == 0 || self.entries.is_empty() {
            return;
        }

        let mut next_bottom = area.y.saturating_add(area.height);
        for toast in self.entries.iter().rev().take(MAX_VISIBLE_TOASTS) {
            if next_bottom <= area.y {
                break;
            }
            let width = toast_width(toast, area.width);
            let height = 3.min(area.height);
            if height == 0 || next_bottom.saturating_sub(height) < area.y {
                break;
            }
            let rect = Rect {
                x: area.x + area.width.saturating_sub(width),
                y: next_bottom.saturating_sub(height),
                width,
                height,
            };
            next_bottom = rect.y.saturating_sub(1);
            draw_toast(frame, rect, toast, theme);
        }
    }
}

fn draw_toast(frame: &mut Frame<'_>, area: Rect, toast: &Toast, theme: &Theme) {
    let style = toast_style(toast.level, theme);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(toast.level.title())
        .border_style(style);
    let inner_width = area.width.saturating_sub(2) as usize;
    let message = truncate_display_width(toast.message(), inner_width);
    let line = Line::from(vec![Span::styled(message, style)]);

    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn toast_style(level: ToastLevel, theme: &Theme) -> Style {
    let fallback = match level {
        ToastLevel::Info => theme.window_bg.patch(theme.widget.accent),
        ToastLevel::Success => theme.window_bg.patch(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        ToastLevel::Warning => theme.window_bg.patch(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        ToastLevel::Error => theme
            .window_bg
            .patch(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
    };
    let name = match level {
        ToastLevel::Info => "toast-info",
        ToastLevel::Success => "toast-success",
        ToastLevel::Warning => "toast-warning",
        ToastLevel::Error => "toast-error",
    };
    theme.named_style(name).unwrap_or(fallback)
}

fn toast_width(toast: &Toast, available: u16) -> u16 {
    let title_w = UnicodeWidthStr::width(toast.level.title());
    let message_w = UnicodeWidthStr::width(toast.message());
    let preferred = title_w.max(message_w).saturating_add(4);
    let max = available.clamp(1, 48) as usize;
    let min = 12.min(max);
    preferred.clamp(min, max) as u16
}

fn truncate_display_width(text: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }

    let suffix = "...";
    let suffix_width = suffix.len().min(max_width);
    let target = max_width.saturating_sub(suffix_width);
    let mut out = String::new();
    let mut used = 0usize;
    for g in text.graphemes(true) {
        let width = UnicodeWidthStr::width(g);
        if used.saturating_add(width) > target {
            break;
        }
        out.push_str(g);
        used += width;
    }
    out.push_str(&suffix[..suffix_width]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_queue_prunes_expired_entries() {
        let mut queue = ToastQueue::with_capacity(3);
        queue.push(Toast::new(
            "old",
            ToastLevel::Info,
            Duration::from_millis(1),
        ));
        queue.push(Toast::new("new", ToastLevel::Info, Duration::from_secs(1)));

        queue.prune_expired(Instant::now() + Duration::from_millis(10));

        assert_eq!(queue.entries.len(), 1);
        assert_eq!(queue.entries[0].message(), "new");
    }

    #[test]
    fn toast_queue_keeps_bounded_recent_entries() {
        let mut queue = ToastQueue::with_capacity(2);
        queue.push(Toast::info("one"));
        queue.push(Toast::info("two"));
        queue.push(Toast::info("three"));

        assert_eq!(queue.entries.len(), 2);
        assert_eq!(queue.entries[0].message(), "two");
        assert_eq!(queue.entries[1].message(), "three");
    }
}
