use std::sync::Arc;
use std::time::Duration;

use crossterm::event::Event;
use parking_lot::RwLock;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use atto_ui_macros::{ComponentProps, component_props};
use crate::composable::{Component, ComponentContext, EventResult};
use crate::reactive::{Binding, TimerHandle, cancel_timer, register_timer_with_duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpinnerLayout {
    IconLeft,
    IconRight,
}

#[derive(Clone, Debug)]
pub enum SpinnerIconStyle {
    None,
    Dots,
    Bars,
    Circles,
    Braille,
    Custom(Vec<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowDirection {
    LeftToRight,
    RightToLeft,
}

#[derive(Clone, Debug)]
pub enum SpinnerTextEffect {
    None,
    Flow {
        colors: Vec<Color>,
        speed: Duration,
        direction: FlowDirection,
    },
}

#[derive(Clone, Debug, ComponentProps)]
struct SpinnerState {
    text: Binding<String>,
    enabled: Binding<bool>,
    running: Binding<bool>,
    icon_frames: Vec<String>,
    icon_speed: Duration,
    #[component(skip)]
    icon_index: Binding<usize>,
    text_effect: SpinnerTextEffect,
    #[component(skip)]
    text_offset: Binding<usize>,
    layout: SpinnerLayout,
    spacing: u16,
    icon_timer: Option<TimerHandle>,
    text_timer: Option<TimerHandle>,
    icon_timer_speed: Option<Duration>,
    text_timer_speed: Option<Duration>,
}

#[derive(Clone, Debug, ComponentProps)]
pub struct Spinner {
    #[component(delegate)]
    state: Arc<RwLock<SpinnerState>>,
}

impl Spinner {
    pub fn new(text: impl Into<Binding<String>>) -> Self {
        let state = SpinnerState {
            text: text.into(),
            enabled: true.into(),
            running: true.into(),
            icon_frames: SpinnerIconStyle::Dots.frames(),
            icon_speed: Duration::from_millis(120),
            icon_index: 0usize.into(),
            text_effect: SpinnerTextEffect::None,
            text_offset: 0usize.into(),
            layout: SpinnerLayout::IconLeft,
            spacing: 1,
            icon_timer: None,
            text_timer: None,
            icon_timer_speed: None,
            text_timer_speed: None,
        };

        Self {
            state: Arc::new(RwLock::new(state)),
        }
    }

    pub fn text(self, text: impl Into<Binding<String>>) -> Self {
        self.state.write().text = text.into();
        self
    }

    pub fn enabled(self, enabled: impl Into<Binding<bool>>) -> Self {
        self.state.write().enabled = enabled.into();
        self
    }

    pub fn running(self, running: impl Into<Binding<bool>>) -> Self {
        self.state.write().running = running.into();
        self.sync_timers();
        self
    }

    pub fn icon_style(self, style: SpinnerIconStyle) -> Self {
        let mut state = self.state.write();
        state.icon_frames = style.frames();
        state.icon_index.set(0);
        let icon_timer = state.icon_timer.take();
        drop(state);
        cancel_handle(icon_timer);
        self.sync_timers();
        self
    }

    pub fn icon_speed(self, speed: Duration) -> Self {
        let mut icon_timer = None;
        let mut changed = false;
        {
            let mut state = self.state.write();
            if state.icon_speed != speed {
                state.icon_speed = speed;
                icon_timer = state.icon_timer.take();
                changed = true;
            }
        }
        if changed {
            cancel_handle(icon_timer);
            self.sync_timers();
        }
        self
    }

    pub fn text_effect(self, effect: SpinnerTextEffect) -> Self {
        let mut state = self.state.write();
        state.text_effect = effect;
        state.text_offset.set(0);
        let text_timer = state.text_timer.take();
        drop(state);
        cancel_handle(text_timer);
        self.sync_timers();
        self
    }

    pub fn flow_text(self, colors: Vec<Color>, speed: Duration) -> Self {
        self.text_effect(SpinnerTextEffect::Flow {
            colors,
            speed,
            direction: FlowDirection::LeftToRight,
        })
    }

    pub fn flow_direction(self, direction: FlowDirection) -> Self {
        {
            let mut state = self.state.write();
            if let SpinnerTextEffect::Flow { direction: dir, .. } = &mut state.text_effect
                && *dir != direction
            {
                *dir = direction;
                state.text_offset.set(0);
            }
        }
        self
    }

    pub fn layout(self, layout: SpinnerLayout) -> Self {
        self.state.write().layout = layout;
        self
    }

    pub fn spacing(self, spacing: u16) -> Self {
        self.state.write().spacing = spacing;
        self
    }

    pub fn start(&self) {
        self.state.write().running.set(true);
        self.sync_timers();
    }

    pub fn stop(&self) {
        self.state.write().running.set(false);
        self.sync_timers();
    }

    fn sync_timers(&self) {
        let mut cancel_icon = None;
        let mut cancel_text = None;

        {
            let mut state = self.state.write();
            if !state.running.get() {
                cancel_icon = state.icon_timer.take();
                cancel_text = state.text_timer.take();
                state.icon_timer_speed = None;
                state.text_timer_speed = None;
            } else {
                if state.icon_frames.is_empty() {
                    cancel_icon = state.icon_timer.take();
                    state.icon_timer_speed = None;
                } else {
                    let desired_speed = state.icon_speed;
                    let speed_changed = state.icon_timer_speed != Some(desired_speed);
                    if state.icon_timer.is_none() || speed_changed {
                        cancel_icon = state.icon_timer.take();
                        let frames_len = state.icon_frames.len();
                        let icon_index = state.icon_index.clone();
                        state.icon_timer =
                            Some(register_timer_with_duration(desired_speed, move || {
                                icon_index.update(|v| {
                                    *v = (*v + 1) % frames_len;
                                });
                                true
                            }));
                        state.icon_timer_speed = Some(desired_speed);
                    }
                }

                match &state.text_effect {
                    SpinnerTextEffect::None => {
                        cancel_text = state.text_timer.take();
                        state.text_timer_speed = None;
                    }
                    SpinnerTextEffect::Flow { colors, speed, .. } => {
                        if colors.is_empty() {
                            cancel_text = state.text_timer.take();
                            state.text_timer_speed = None;
                        } else {
                            let desired_speed = *speed;
                            let speed_changed = state.text_timer_speed != Some(desired_speed);
                            if state.text_timer.is_none() || speed_changed {
                                cancel_text = state.text_timer.take();
                                let text_offset = state.text_offset.clone();
                                state.text_timer =
                                    Some(register_timer_with_duration(desired_speed, move || {
                                        text_offset.update(|v| {
                                            *v = v.wrapping_add(1);
                                        });
                                        true
                                    }));
                                state.text_timer_speed = Some(desired_speed);
                            }
                        }
                    }
                }
            }
        }

        cancel_handle(cancel_icon);
        cancel_handle(cancel_text);
    }

    fn frame_and_text(
        &self,
    ) -> (
        String,
        String,
        SpinnerLayout,
        u16,
        bool,
        SpinnerTextEffect,
        usize,
    ) {
        let state = self.state.read();
        let text = state.text.get();
        let layout = state.layout;
        let spacing = state.spacing;
        let enabled = state.enabled.get();
        let offset = state.text_offset.get();
        let icon = if state.icon_frames.is_empty() {
            String::new()
        } else {
            let idx = state.icon_index.get() % state.icon_frames.len();
            state.icon_frames[idx].clone()
        };
        (
            icon,
            text,
            layout,
            spacing,
            enabled,
            state.text_effect.clone(),
            offset,
        )
    }

    fn icon_style_for(&self, enabled: bool, ctx: ComponentContext<'_>) -> Style {
        if enabled {
            ctx.theme.widget.accent
        } else {
            ctx.theme.widget.disabled
        }
    }

    fn text_style_for(&self, enabled: bool, ctx: ComponentContext<'_>) -> Style {
        if enabled {
            ctx.theme.widget.dim
        } else {
            ctx.theme.widget.disabled
        }
    }
}

#[component_props]
impl Component for Spinner {
    fn set_property(
        &mut self,
        name: &str,
        value: crate::ComponentValue,
    ) -> Result<(), crate::ComponentError> {
        if name == "running" {
            let v = crate::ComponentValueCodec::from_component_value(value, name)?;
            self.state.write().running.set(v);
            self.sync_timers();
            return Ok(());
        }
        ::atto_ui::ComponentProps::set_property(self, name, value)
    }

    fn is_focusable(&self) -> bool {
        false
    }

    fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }

    fn desired_width(&self) -> Option<u16> {
        let state = self.state.read();
        let icon_width = state
            .icon_frames
            .iter()
            .map(|frame| frame.width())
            .max()
            .unwrap_or(0);
        let text_width = state.text.get().width();
        let spacing = if icon_width > 0 && text_width > 0 {
            state.spacing as usize
        } else {
            0
        };
        Some((icon_width + spacing + text_width).min(u16::MAX as usize) as u16)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        self.sync_timers();
        let (icon, text, layout, spacing, enabled, text_effect, text_offset) =
            self.frame_and_text();

        let icon_style = self.icon_style_for(enabled, ctx);
        let text_style = self.text_style_for(enabled, ctx);
        let text_effect = if enabled {
            text_effect
        } else {
            SpinnerTextEffect::None
        };
        let mut spans = Vec::new();

        let spacing_text = " ".repeat(spacing as usize);
        let has_icon = !icon.is_empty();
        let has_text = !text.is_empty();

        let (first_icon, second_icon) = match layout {
            SpinnerLayout::IconLeft => (has_icon, false),
            SpinnerLayout::IconRight => (false, has_icon),
        };

        if first_icon {
            spans.push(Span::styled(icon.clone(), icon_style));
        }

        if has_text {
            if first_icon && spacing > 0 {
                spans.push(Span::styled(spacing_text.clone(), text_style));
            }
            spans.extend(build_text_spans(
                &text,
                &text_effect,
                text_style,
                text_offset,
            ));
        }

        if second_icon {
            if has_text && spacing > 0 {
                spans.push(Span::styled(spacing_text.clone(), text_style));
            }
            spans.push(Span::styled(icon.clone(), icon_style));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

impl SpinnerIconStyle {
    fn frames(&self) -> Vec<String> {
        match self {
            SpinnerIconStyle::None => Vec::new(),
            SpinnerIconStyle::Dots | SpinnerIconStyle::Braille => {
                vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect()
            }
            SpinnerIconStyle::Bars => vec![
                "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃", "▂",
            ]
            .into_iter()
            .map(|s| s.to_string())
            .collect(),
            SpinnerIconStyle::Circles => vec!["◐", "◓", "◑", "◒"]
                .into_iter()
                .map(|s| s.to_string())
                .collect(),
            SpinnerIconStyle::Custom(frames) => frames.clone(),
        }
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new("".to_string())
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        if Arc::strong_count(&self.state) != 1 {
            return;
        }
        let (icon, text) = {
            let mut state = self.state.write();
            (state.icon_timer.take(), state.text_timer.take())
        };
        cancel_handle(icon);
        cancel_handle(text);
    }
}

fn build_text_spans<'a>(
    text: &'a str,
    effect: &'a SpinnerTextEffect,
    base: Style,
    offset: usize,
) -> Vec<Span<'a>> {
    match effect {
        SpinnerTextEffect::None => vec![Span::styled(text.to_string(), base)],
        SpinnerTextEffect::Flow {
            colors, direction, ..
        } => {
            if colors.is_empty() || text.is_empty() {
                return vec![Span::styled(text.to_string(), base)];
            }
            let text_len = text.chars().count();
            if text_len == 0 {
                return vec![Span::styled(text.to_string(), base)];
            }
            let cycle = text_len.max(1);
            let shift = offset % cycle;
            let mut spans = Vec::new();
            for (idx, ch) in text.chars().enumerate() {
                let pos = match direction {
                    FlowDirection::LeftToRight => (cycle + idx - shift) % cycle,
                    FlowDirection::RightToLeft => (idx + shift) % cycle,
                };
                let t = if cycle <= 1 {
                    0.0
                } else {
                    pos as f32 / (cycle.saturating_sub(1) as f32)
                };
                let color = gradient_color(colors, t).unwrap_or(colors[0]);
                let style = base.fg(color);
                spans.push(Span::styled(ch.to_string(), style));
            }
            spans
        }
    }
}

fn cancel_handle(handle: Option<TimerHandle>) {
    if let Some(handle) = handle {
        cancel_timer(handle);
    }
}

fn gradient_color(colors: &[Color], t: f32) -> Option<Color> {
    if colors.is_empty() {
        return None;
    }
    if colors.len() == 1 {
        return Some(colors[0]);
    }

    let t = t.clamp(0.0, 1.0);
    let segments = colors.len() - 1;
    let scaled = t * segments as f32;
    let idx = scaled.floor() as usize;
    let idx = idx.min(segments - 1);
    let local_t = scaled - idx as f32;
    let c0 = colors[idx];
    let c1 = colors[idx + 1];
    if let (Some(a), Some(b)) = (color_to_rgb(c0), color_to_rgb(c1)) {
        return Some(Color::Rgb(
            lerp_u8(a.0, b.0, local_t),
            lerp_u8(a.1, b.1, local_t),
            lerp_u8(a.2, b.2, local_t),
        ));
    }
    Some(if local_t < 0.5 { c0 } else { c1 })
}

fn color_to_rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Black => Some((0, 0, 0)),
        Color::Red => Some((205, 49, 49)),
        Color::Green => Some((13, 188, 121)),
        Color::Yellow => Some((229, 229, 16)),
        Color::Blue => Some((36, 114, 200)),
        Color::Magenta => Some((188, 63, 188)),
        Color::Cyan => Some((17, 168, 205)),
        Color::Gray => Some((128, 128, 128)),
        Color::DarkGray => Some((64, 64, 64)),
        Color::LightRed => Some((241, 76, 76)),
        Color::LightGreen => Some((35, 209, 139)),
        Color::LightYellow => Some((245, 245, 67)),
        Color::LightBlue => Some((59, 142, 234)),
        Color::LightMagenta => Some((214, 112, 214)),
        Color::LightCyan => Some((41, 184, 219)),
        Color::White => Some((255, 255, 255)),
        Color::Indexed(_) => None,
        Color::Reset => None,
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let a = a as f32;
    let b = b as f32;
    (a + (b - a) * t).round().clamp(0.0, 255.0) as u8
}
