use std::sync::Arc;

use crossterm::event::{Event, MouseButton, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::composable::{
    Component, ComponentContext, ComponentId, ComponentNode, DynamicTree, EventHandling,
    EventResult, Layout, LayoutParams,
};
use crate::reactive::Binding;
use crate::runtime::{CallbackHandle, ComponentValue, PropertyMeta, ValueType};
use crate::text::styled_text::{
    StyledTextSegment, hit_test_link, normalize_segments, parse_text_color, segments_display_width,
    spans_from_segments,
};
use crate::widgets::util::mouse_coords_local_to_area;
use crate::{ComponentError, ComponentPropertySchema};

type LinkCallback = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct TextSpanColor {
    raw: String,
    color: Color,
}

impl TextSpanColor {
    fn parse(raw: impl Into<String>) -> Result<Self, String> {
        let raw = raw.into();
        let color = parse_text_color(&raw)?;
        Ok(Self { raw, color })
    }

    fn from_color(color: Color) -> Self {
        Self {
            raw: color_to_raw(color),
            color,
        }
    }
}

/// A semantic child node for [`RichText`] with structured style flags.
#[derive(Clone, Debug)]
pub struct TextSpan {
    text: Binding<String>,
    bold: Binding<bool>,
    italic: Binding<bool>,
    underline: Binding<bool>,
    strike: Binding<bool>,
    color: Binding<Option<TextSpanColor>>,
    href: Binding<Option<String>>,
}

impl TextSpan {
    pub fn new(text: impl Into<Binding<String>>) -> Self {
        Self {
            text: text.into(),
            bold: false.into(),
            italic: false.into(),
            underline: false.into(),
            strike: false.into(),
            color: Binding::new(None),
            href: Binding::new(None),
        }
    }

    pub fn text(mut self, text: impl Into<Binding<String>>) -> Self {
        self.text = text.into();
        self
    }

    pub fn bold(mut self, bold: impl Into<Binding<bool>>) -> Self {
        self.bold = bold.into();
        self
    }

    pub fn italic(mut self, italic: impl Into<Binding<bool>>) -> Self {
        self.italic = italic.into();
        self
    }

    pub fn underline(mut self, underline: impl Into<Binding<bool>>) -> Self {
        self.underline = underline.into();
        self
    }

    pub fn strike(mut self, strike: impl Into<Binding<bool>>) -> Self {
        self.strike = strike.into();
        self
    }

    pub fn color(self, color: Color) -> Self {
        self.color.set(Some(TextSpanColor::from_color(color)));
        self
    }

    pub fn color_name(mut self, color: impl Into<String>) -> Result<Self, String> {
        self.set_color_name(color.into())?;
        Ok(self)
    }

    pub fn href(self, href: impl Into<String>) -> Self {
        self.href.set(Some(href.into()));
        self
    }

    pub(crate) fn segment(&self) -> StyledTextSegment {
        StyledTextSegment::structured(
            self.text.get(),
            self.bold.get(),
            self.italic.get(),
            self.underline.get(),
            self.strike.get(),
            self.color.get().map(|color| color.color),
            self.href.get(),
        )
    }

    fn set_color_name(&mut self, color: String) -> Result<(), String> {
        self.color.set(Some(TextSpanColor::parse(color)?));
        Ok(())
    }
}

impl ComponentPropertySchema for TextSpan {
    fn property_schema() -> Vec<PropertyMeta> {
        vec![
            PropertyMeta::new("text", ValueType::String),
            PropertyMeta::new("bold", ValueType::Bool),
            PropertyMeta::new("italic", ValueType::Bool),
            PropertyMeta::new("underline", ValueType::Bool),
            PropertyMeta::new("strike", ValueType::Bool),
            PropertyMeta::new("color", ValueType::String),
            PropertyMeta::new("href", ValueType::String),
        ]
    }
}

impl Component for TextSpan {
    fn property_names(&self) -> Vec<&'static str> {
        vec![
            "text",
            "bold",
            "italic",
            "underline",
            "strike",
            "color",
            "href",
        ]
    }

    fn get_property(&self, name: &str) -> Option<ComponentValue> {
        match name {
            "text" => Some(ComponentValue::String(self.text.get())),
            "bold" => Some(ComponentValue::Bool(self.bold.get())),
            "italic" => Some(ComponentValue::Bool(self.italic.get())),
            "underline" => Some(ComponentValue::Bool(self.underline.get())),
            "strike" => Some(ComponentValue::Bool(self.strike.get())),
            "color" => self
                .color
                .get()
                .map(|color| ComponentValue::String(color.raw)),
            "href" => self.href.get().map(ComponentValue::String),
            _ => None,
        }
    }

    fn set_property(&mut self, name: &str, value: ComponentValue) -> Result<(), ComponentError> {
        match name {
            "text" => self.text.set(expect_string(value, name)?),
            "bold" => self.bold.set(expect_bool(value, name)?),
            "italic" => self.italic.set(expect_bool(value, name)?),
            "underline" => self.underline.set(expect_bool(value, name)?),
            "strike" => self.strike.set(expect_bool(value, name)?),
            "color" => match value {
                ComponentValue::Null => self.color.set(None),
                ComponentValue::String(value) => self.set_color_name(value).map_err(|_| {
                    ComponentError::invalid_value(name, "color name or #RGB/#RRGGBB")
                })?,
                _ => {
                    return Err(ComponentError::invalid_value(
                        name,
                        "color name or #RGB/#RRGGBB",
                    ));
                }
            },
            "href" => match value {
                ComponentValue::Null => self.href.set(None),
                ComponentValue::String(value) => self.href.set(Some(value)),
                _ => return Err(ComponentError::invalid_value(name, "string")),
            },
            _ => return Err(ComponentError::unsupported_property(name)),
        }
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let segment = self.segment();
        let spans = spans_from_segments(&[segment], ctx.theme.widget.dim, None);
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

impl Layout for TextSpan {
    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }

    fn desired_width(&self) -> Option<u16> {
        Some(segments_display_width(&[self.segment()]))
    }
}

crate::impl_component_default_traits!(TextSpan => Scrollable, FocusNav, DynamicTree, EventHandling);

/// A single-line rich text container composed from structured [`TextSpan`] children.
pub struct RichText {
    id: ComponentId,
    children: Vec<ComponentNode>,
    on_link: Option<LinkCallback>,
    on_link_callback: Option<CallbackHandle>,
    last_area: Option<Rect>,
}

impl RichText {
    pub fn new() -> Self {
        Self {
            id: ComponentId::next(),
            children: Vec::new(),
            on_link: None,
            on_link_callback: None,
            last_area: None,
        }
    }

    pub fn span(mut self, span: TextSpan) -> Self {
        self.add_span(span);
        self
    }

    pub fn add_span(&mut self, span: TextSpan) {
        self.add_child_with_layout(Box::new(span), LayoutParams::default());
    }

    pub fn on_link<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.on_link = Some(Arc::new(callback));
        self
    }

    pub fn on_link_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_link_callback = Some(callback);
        self
    }

    pub(crate) fn add_child_with_layout(&mut self, view: Box<dyn Component>, layout: LayoutParams) {
        let mut node = ComponentNode::new(view).with_layout(layout);
        node.parent = Some(self.id);
        self.children.push(node);
    }

    fn segments(&self) -> Vec<StyledTextSegment> {
        normalize_segments(
            self.children
                .iter()
                .filter_map(|child| text_span_segment(child.view.as_ref())),
        )
    }
}

impl Default for RichText {
    fn default() -> Self {
        Self::new()
    }
}

impl ComponentPropertySchema for RichText {
    fn property_schema() -> Vec<PropertyMeta> {
        Vec::new()
    }
}

impl Component for RichText {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let segments = self.segments();
        let spans = spans_from_segments(
            &segments,
            ctx.theme.widget.dim,
            ctx.theme.named_style("markdown-link"),
        );
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

impl Layout for RichText {
    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }

    fn desired_width(&self) -> Option<u16> {
        Some(segments_display_width(&self.segments()))
    }
}

impl DynamicTree for RichText {
    fn children(&self) -> &[ComponentNode] {
        &self.children
    }
}

impl EventHandling for RichText {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Event::Mouse(m) = event else {
            return EventResult::ignored();
        };
        if m.kind != MouseEventKind::Down(MouseButton::Left) {
            return EventResult::ignored();
        }

        let Some(area) = self.last_area else {
            return EventResult::ignored();
        };
        let Some((local_x, local_y)) =
            mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
        else {
            return EventResult::ignored();
        };
        if local_y != 0 {
            return EventResult::ignored();
        }

        let segments = self.segments();
        if let Some(url) = hit_test_link(&segments, local_x) {
            if let Some(cb) = &self.on_link {
                cb(url);
            }
            if let Some(cb) = &self.on_link_callback {
                cb.emit_with(Some(ComponentValue::String(url.to_string())));
            }
            return EventResult::consumed();
        }

        EventResult::ignored()
    }
}

crate::impl_component_default_traits!(RichText => Scrollable, FocusNav);

fn text_span_segment(view: &dyn Component) -> Option<StyledTextSegment> {
    if view.type_name().rsplit("::").next() != Some("TextSpan") {
        return None;
    }

    let text = match view.get_property("text") {
        Some(ComponentValue::String(value)) => value,
        _ => String::new(),
    };
    let color = match view.get_property("color") {
        Some(ComponentValue::String(value)) => parse_text_color(&value).ok(),
        _ => None,
    };
    let href = match view.get_property("href") {
        Some(ComponentValue::String(value)) => Some(value),
        _ => None,
    };

    Some(StyledTextSegment::structured(
        text,
        bool_prop(view, "bold"),
        bool_prop(view, "italic"),
        bool_prop(view, "underline"),
        bool_prop(view, "strike"),
        color,
        href,
    ))
}

fn bool_prop(view: &dyn Component, name: &str) -> bool {
    matches!(view.get_property(name), Some(ComponentValue::Bool(true)))
}

fn expect_string(value: ComponentValue, name: &str) -> Result<String, ComponentError> {
    match value {
        ComponentValue::String(value) => Ok(value),
        _ => Err(ComponentError::invalid_value(name, "string")),
    }
}

fn expect_bool(value: ComponentValue, name: &str) -> Result<bool, ComponentError> {
    match value {
        ComponentValue::Bool(value) => Ok(value),
        _ => Err(ComponentError::invalid_value(name, "bool")),
    }
}

fn color_to_raw(color: Color) -> String {
    match color {
        Color::Reset => "reset".to_string(),
        Color::Black => "black".to_string(),
        Color::Red => "red".to_string(),
        Color::Green => "green".to_string(),
        Color::Yellow => "yellow".to_string(),
        Color::Blue => "blue".to_string(),
        Color::Magenta => "magenta".to_string(),
        Color::Cyan => "cyan".to_string(),
        Color::Gray => "gray".to_string(),
        Color::DarkGray => "darkgray".to_string(),
        Color::LightRed => "lightred".to_string(),
        Color::LightGreen => "lightgreen".to_string(),
        Color::LightYellow => "lightyellow".to_string(),
        Color::LightBlue => "lightblue".to_string(),
        Color::LightMagenta => "lightmagenta".to_string(),
        Color::LightCyan => "lightcyan".to_string(),
        Color::White => "white".to_string(),
        Color::Indexed(value) => format!("indexed:{value}"),
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Modifier;

    use crate::composable::{MouseCoordinateSpace, ScrollbarHost, TabMode};
    use crate::runtime::{CallbackRegistry, ComponentSpec, ComponentSpecChild, ComponentTree};
    use crate::theme::Theme;
    use crate::wm::WindowId;

    use super::*;

    fn context(theme: &Theme) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Local,
            drag: None,
        }
    }

    #[test]
    fn rich_text_draws_structured_styles() {
        let mut rich = RichText::new()
            .span(TextSpan::new("B").bold(true))
            .span(TextSpan::new("I").italic(true))
            .span(TextSpan::new("U").underline(true))
            .span(TextSpan::new("S").strike(true))
            .span(TextSpan::new("C").color(Color::Red))
            .span(TextSpan::new("L").href("https://example.com"));
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(8, 1)).expect("terminal");

        terminal
            .draw(|f| rich.draw(f, Rect::new(0, 0, 8, 1), context(&theme)))
            .expect("draw");
        let buf = terminal.backend().buffer();

        let bold = buf.cell((0, 0)).expect("bold cell");
        assert_eq!(bold.symbol(), "B");
        assert!(bold.modifier.contains(Modifier::BOLD));

        let italic = buf.cell((1, 0)).expect("italic cell");
        assert_eq!(italic.symbol(), "I");
        assert!(italic.modifier.contains(Modifier::ITALIC));

        let underline = buf.cell((2, 0)).expect("underline cell");
        assert_eq!(underline.symbol(), "U");
        assert!(underline.modifier.contains(Modifier::UNDERLINED));

        let strike = buf.cell((3, 0)).expect("strike cell");
        assert_eq!(strike.symbol(), "S");
        assert!(strike.modifier.contains(Modifier::CROSSED_OUT));

        let color = buf.cell((4, 0)).expect("color cell");
        assert_eq!(color.symbol(), "C");
        assert_eq!(color.fg, Color::Red);

        let link = buf.cell((5, 0)).expect("link cell");
        assert_eq!(link.symbol(), "L");
        assert!(link.modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn rich_text_link_event_emits_callback_payload() {
        let callbacks = CallbackRegistry::new();
        let callback = callbacks.register();
        let root = ComponentSpec::new("RichText")
            .with_id("rich")
            .with_event("link", callback)
            .with_child(ComponentSpecChild::new(
                ComponentSpec::new("TextSpan")
                    .with_id("plain")
                    .with_prop("text", ComponentValue::String("go ".to_string())),
            ))
            .with_child(ComponentSpecChild::new(
                ComponentSpec::new("TextSpan")
                    .with_id("link")
                    .with_prop("text", ComponentValue::String("LINK".to_string()))
                    .with_prop(
                        "href",
                        ComponentValue::String("https://example.com".to_string()),
                    ),
            ));
        let mut tree = ComponentTree::new(root, callbacks.clone()).expect("tree");
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(12, 1)).expect("terminal");

        terminal
            .draw(|f| tree.draw(f, Rect::new(0, 0, 12, 1), context(&theme)))
            .expect("draw");
        let event = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        let result = tree.handle_event(&event, context(&theme));

        assert!(result.is_consumed());
        let events = callbacks.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].callback_id, callback);
        assert_eq!(events[0].target_id.as_deref(), Some("rich"));
        assert_eq!(events[0].event, "link");
        assert_eq!(
            events[0].payload,
            Some(ComponentValue::String("https://example.com".to_string()))
        );
    }

    #[test]
    fn text_span_rejects_invalid_color_property() {
        let mut span = TextSpan::new("text");
        let err = span
            .set_property("color", ComponentValue::String("not-a-color".to_string()))
            .expect_err("invalid color");

        assert_eq!(
            err,
            ComponentError::invalid_value("color", "color name or #RGB/#RRGGBB")
        );
    }

    #[test]
    fn rich_text_cleans_empty_spans_and_merges_adjacent_styles() {
        let rich = RichText::new()
            .span(TextSpan::new("a").bold(true))
            .span(TextSpan::new(""))
            .span(TextSpan::new("b").bold(true));

        let segments = rich.segments();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments_display_width(&segments), 2);
    }
}
