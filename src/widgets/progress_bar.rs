use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::composable::{Component, ComponentContext, Layout};
use crate::reactive::Binding;
use atto_ui_macros::{ComponentProperties, component_properties};

#[derive(Clone, Debug, ComponentProperties)]
pub struct ProgressBar {
    min: Binding<f64>,
    max: Binding<f64>,
    value: Binding<f64>,
    enabled: Binding<bool>,
    show_text: Binding<bool>,
    text: Option<Binding<String>>,
    fill_char: char,
    empty_char: char,
}

impl ProgressBar {
    pub fn new(
        min: impl Into<Binding<f64>>,
        max: impl Into<Binding<f64>>,
        value: Binding<f64>,
    ) -> Self {
        Self {
            min: min.into(),
            max: max.into(),
            value,
            enabled: true.into(),
            show_text: false.into(),
            text: None,
            fill_char: '=',
            empty_char: '-',
        }
    }

    pub fn min(mut self, min: impl Into<Binding<f64>>) -> Self {
        self.min = min.into();
        self
    }

    pub fn max(mut self, max: impl Into<Binding<f64>>) -> Self {
        self.max = max.into();
        self
    }

    pub fn value(mut self, value: Binding<f64>) -> Self {
        self.value = value;
        self
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    pub fn show_text(mut self, show_text: impl Into<Binding<bool>>) -> Self {
        self.show_text = show_text.into();
        self
    }

    pub fn text(mut self, text: impl Into<Binding<String>>) -> Self {
        self.text = Some(text.into());
        self.show_text = true.into();
        self
    }

    pub fn fill_char(mut self, ch: char) -> Self {
        self.fill_char = ch;
        self
    }

    pub fn empty_char(mut self, ch: char) -> Self {
        self.empty_char = ch;
        self
    }

    fn normalized_range(&self) -> (f64, f64, f64) {
        let mut min = self.min.get();
        let mut max = self.max.get();
        if max < min {
            std::mem::swap(&mut min, &mut max);
        }
        let mut value = self.value.get();
        if value < min {
            value = min;
        } else if value > max {
            value = max;
        }
        let range = max - min;
        let ratio = if range.abs() <= f64::EPSILON {
            0.0
        } else {
            (value - min) / range
        };
        (min, max, ratio)
    }
}

#[component_properties]
impl Component for ProgressBar {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let enabled = self.enabled.get();
        let fill_style = if enabled {
            ctx.theme.widget.accent
        } else {
            ctx.theme.widget.disabled
        };
        let empty_style = if enabled {
            ctx.theme.widget.dim
        } else {
            ctx.theme.widget.disabled
        };
        let text_style = if enabled {
            ctx.theme.widget.normal
        } else {
            ctx.theme.widget.disabled
        };

        let (_min, _max, ratio) = self.normalized_range();
        let width = area.width as usize;
        let filled = ((ratio * width as f64).floor() as usize).min(width);

        let show_text = self.show_text.get();
        let text = self.text.as_ref().map(|t| t.get()).unwrap_or_default();
        let text_chars: Vec<char> = if show_text {
            text.chars().collect()
        } else {
            Vec::new()
        };
        let text_len = text_chars.len().min(width);
        let text_start = if text_len > 0 {
            width.saturating_sub(text_len) / 2
        } else {
            0
        };

        let mut spans = Vec::with_capacity(width);
        for idx in 0..width {
            let (ch, style) =
                if text_len > 0 && idx >= text_start && idx < text_start.saturating_add(text_len) {
                    let ch = text_chars[idx - text_start];
                    (ch, text_style)
                } else if idx < filled {
                    (self.fill_char, fill_style)
                } else {
                    (self.empty_char, empty_style)
                };

            spans.push(Span::styled(ch.to_string(), style));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

impl Layout for ProgressBar {
    fn min_width(&self) -> u16 {
        3
    }

    fn min_height(&self) -> u16 {
        1
    }

    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }
}

crate::impl_component_default_traits!(ProgressBar => Scrollable, FocusNav, DynamicTree, EventHandling);

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::composable::{ComponentContext, MouseCoordinateSpace, ScrollbarHost, TabMode};
    use crate::theme::Theme;
    use crate::wm::WindowId;

    use super::*;

    fn context(theme: &Theme) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: false,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        }
    }

    fn screen_contents(terminal: &Terminal<TestBackend>, width: u16) -> String {
        let buf = terminal.backend().buffer();
        let mut out = String::new();
        for x in 0..width {
            out.push_str(buf[(x, 0)].symbol());
        }
        out
    }

    #[test]
    fn draw_clamps_range_and_centers_text() {
        let mut progress = ProgressBar::new(10.0, 0.0, Binding::new(15.0))
            .fill_char('#')
            .empty_char('.')
            .text("Done");
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(12, 1)).expect("terminal");

        terminal
            .draw(|f| progress.draw(f, Rect::new(0, 0, 12, 1), context(&theme)))
            .expect("draw");

        let screen = screen_contents(&terminal, 12);
        assert_eq!(screen, "####Done####");
    }
}
