use std::sync::Arc;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::view::{View, ViewContext};

use super::view::{DeclarativeView, EmptyView};

/// Text view (renders a single line of text; will clip if the area is too small).
#[derive(Clone)]
pub struct Text {
    content: TextContent,
    style: Option<Style>,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: TextContent::Static(content.into()),
            style: None,
        }
    }

    pub fn from_fn<F>(f: F) -> Self
    where
        F: Fn() -> String + Send + Sync + 'static,
    {
        Self {
            content: TextContent::Dynamic(Arc::new(f)),
            style: None,
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn fg(mut self, color: Color) -> Self {
        self.style = Some(self.style.unwrap_or_default().fg(color));
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.style = Some(self.style.unwrap_or_default().bg(color));
        self
    }

    fn resolve(&self) -> String {
        match &self.content {
            TextContent::Static(s) => s.clone(),
            TextContent::Dynamic(f) => (f)(),
        }
    }
}

impl DeclarativeView for Text {
    fn body(&self) -> Box<dyn DeclarativeView> {
        Box::new(EmptyView)
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let style = self.style.unwrap_or(ctx.theme.widget.normal);
        frame.render_widget(Paragraph::new(Line::styled(self.resolve(), style)), area);
    }

    fn build_view(&self) -> Box<dyn View> {
        Box::new(TextImperativeView {
            content: self.content.clone(),
            style: self.style,
        })
    }

    fn is_primitive(&self) -> bool {
        true
    }
}

/// Dynamic text view (constructed from a closure).
///
/// This exists primarily to make `Text::from_fn` usable from the `view_builder!` macro, which
/// expects a `Type::new(...)` constructor form.
#[derive(Clone)]
pub struct TextFn {
    inner: Text,
}

impl TextFn {
    pub fn new<F>(f: F) -> Self
    where
        F: Fn() -> String + Send + Sync + 'static,
    {
        Self {
            inner: Text::from_fn(f),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.inner = self.inner.style(style);
        self
    }

    pub fn fg(mut self, color: Color) -> Self {
        self.inner = self.inner.fg(color);
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.inner = self.inner.bg(color);
        self
    }
}

impl DeclarativeView for TextFn {
    fn body(&self) -> Box<dyn DeclarativeView> {
        self.inner.body()
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.inner.render(frame, area, ctx);
    }

    fn build_view(&self) -> Box<dyn View> {
        self.inner.build_view()
    }

    fn is_primitive(&self) -> bool {
        self.inner.is_primitive()
    }
}

#[derive(Clone)]
struct TextImperativeView {
    content: TextContent,
    style: Option<Style>,
}

impl View for TextImperativeView {
    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }

    fn desired_width(&self) -> Option<u16> {
        Some(self.resolve().len().min(u16::MAX as usize) as u16)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        let style = self.style.unwrap_or(ctx.theme.widget.normal);
        frame.render_widget(Paragraph::new(Line::styled(self.resolve(), style)), area);
    }
}

#[derive(Clone)]
enum TextContent {
    Static(String),
    Dynamic(Arc<dyn Fn() -> String + Send + Sync>),
}

impl TextImperativeView {
    fn resolve(&self) -> String {
        match &self.content {
            TextContent::Static(s) => s.clone(),
            TextContent::Dynamic(f) => (f)(),
        }
    }
}

/// Spacer view (takes space, renders nothing).
#[derive(Clone, Debug, Default)]
pub struct Spacer;

impl Spacer {
    pub fn new() -> Self {
        Self
    }
}

impl DeclarativeView for Spacer {
    fn body(&self) -> Box<dyn DeclarativeView> {
        Box::new(self.clone())
    }

    fn render(&self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ViewContext<'_>) {}

    fn build_view(&self) -> Box<dyn View> {
        Box::new(SpacerImperativeView)
    }

    fn is_primitive(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, Default)]
struct SpacerImperativeView;

impl View for SpacerImperativeView {
    fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ViewContext<'_>) {}
}

/// Divider view (horizontal or vertical line).
#[derive(Clone, Debug)]
pub struct Divider {
    horizontal: bool,
}

impl Divider {
    pub fn horizontal() -> Self {
        Self { horizontal: true }
    }

    pub fn vertical() -> Self {
        Self { horizontal: false }
    }
}

impl DeclarativeView for Divider {
    fn body(&self) -> Box<dyn DeclarativeView> {
        Box::new(self.clone())
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let style = ctx.theme.widget.normal;
        if self.horizontal {
            let line = "─".repeat(area.width as usize);
            frame.render_widget(Paragraph::new(Line::styled(line, style)), area);
            return;
        }

        let buf = frame.buffer_mut();
        for dy in 0..area.height {
            buf[(area.x, area.y.saturating_add(dy))]
                .set_symbol("│")
                .set_style(style);
        }
    }

    fn build_view(&self) -> Box<dyn View> {
        Box::new(DividerImperativeView {
            horizontal: self.horizontal,
        })
    }

    fn is_primitive(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug)]
struct DividerImperativeView {
    horizontal: bool,
}

impl View for DividerImperativeView {
    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }

    fn desired_width(&self) -> Option<u16> {
        Some(1)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let style = ctx.theme.widget.normal;
        if self.horizontal {
            let line = "─".repeat(area.width as usize);
            frame.render_widget(Paragraph::new(Line::styled(line, style)), area);
            return;
        }

        let buf = frame.buffer_mut();
        for dy in 0..area.height {
            buf[(area.x, area.y.saturating_add(dy))]
                .set_symbol("│")
                .set_style(style);
        }
    }
}
