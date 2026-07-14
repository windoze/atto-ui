//! Collapsible content block used by core UI and higher-level crates.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::ComponentCommand;
use crate::composable::{
    Component, ComponentContext, ComponentId, ComponentNode, DynamicTree, EventHandling,
    EventResult, FocusNav, Layout, Scrollable, ScrollbarHost,
};
use crate::reactive::Binding;
use crate::runtime::CallbackHandle;
use atto_ui_macros::{ComponentProperties, component_properties};

use super::util::{contains, mouse_coords_local_to_area, widget_style};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisclosureStatus {
    #[default]
    Idle,
    Running,
    Done,
    Error,
    Canceled,
}

impl DisclosureStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "Idle" | "idle" => Some(Self::Idle),
            "Running" | "running" => Some(Self::Running),
            "Done" | "done" => Some(Self::Done),
            "Error" | "error" => Some(Self::Error),
            "Canceled" | "canceled" | "Cancelled" | "cancelled" => Some(Self::Canceled),
            _ => None,
        }
    }

    fn indicator_glyph(self) -> &'static str {
        match self {
            Self::Idle => "disclosure-idle-indicator",
            Self::Running => "disclosure-running-indicator",
            Self::Done => "disclosure-done-indicator",
            Self::Error => "disclosure-error-indicator",
            Self::Canceled => "disclosure-canceled-indicator",
        }
    }

    fn indicator_fallback(self) -> &'static str {
        match self {
            Self::Idle => "[ ]",
            Self::Running => "[~]",
            Self::Done => "[x]",
            Self::Error => "[!]",
            Self::Canceled => "[-]",
        }
    }

    fn style_name(self) -> &'static str {
        match self {
            Self::Idle => "disclosure-idle",
            Self::Running => "disclosure-running",
            Self::Done => "disclosure-done",
            Self::Error => "disclosure-error",
            Self::Canceled => "disclosure-canceled",
        }
    }
}

#[derive(ComponentProperties)]
pub struct Disclosure {
    id: ComponentId,
    title: Binding<String>,
    expanded: Binding<bool>,
    #[component(include)]
    status: Binding<DisclosureStatus>,
    content: Option<Binding<String>>,
    enabled: Binding<bool>,
    children: Vec<ComponentNode>,
    focused_child: Option<ComponentId>,
    last_area: Option<Rect>,
    #[component(skip)]
    on_toggle: Option<CallbackHandle>,
}

impl Disclosure {
    pub fn new(title: impl Into<Binding<String>>) -> Self {
        Self {
            id: ComponentId::next(),
            title: title.into(),
            expanded: false.into(),
            status: DisclosureStatus::Idle.into(),
            content: None,
            enabled: true.into(),
            children: Vec::new(),
            focused_child: None,
            last_area: None,
            on_toggle: None,
        }
    }

    pub fn title(mut self, title: impl Into<Binding<String>>) -> Self {
        self.title = title.into();
        self
    }

    pub fn expanded(mut self, expanded: impl Into<Binding<bool>>) -> Self {
        self.expanded = expanded.into();
        if !self.expanded.get() {
            self.focused_child = None;
        }
        self
    }

    pub fn status(mut self, status: impl Into<Binding<DisclosureStatus>>) -> Self {
        self.status = status.into();
        self
    }

    pub fn content(mut self, content: impl Into<Binding<String>>) -> Self {
        self.content = Some(content.into());
        self
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        if !self.enabled.get() {
            self.focused_child = None;
        }
        self
    }

    pub fn child(mut self, view: impl Component + 'static) -> Self {
        self.set_child(Box::new(view));
        self
    }

    pub fn boxed_child(mut self, view: Box<dyn Component>) -> Self {
        self.set_child(view);
        self
    }

    pub fn on_toggle_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_toggle = Some(callback);
        self
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded.get()
    }

    pub fn set_child(&mut self, view: Box<dyn Component>) -> ComponentId {
        let mut node = ComponentNode::new(view);
        node.parent = Some(self.id);
        let id = node.id;
        self.children.clear();
        self.children.push(node);
        self.focused_child = None;
        id
    }

    fn emit_toggle(&self) {
        if let Some(cb) = &self.on_toggle {
            cb.emit();
        }
    }

    fn toggle(&mut self) -> EventResult {
        let next = !self.expanded.get();
        self.expanded.set(next);
        if !next {
            self.focused_child = None;
        }
        self.emit_toggle();
        EventResult::changed()
    }

    fn header_line(&self, ctx: ComponentContext<'_>) -> Line<'static> {
        let enabled = self.enabled.get();
        let header_focused = ctx.is_focused && self.focused_child.is_none();
        let title_style = if enabled {
            ctx.theme
                .named_style(if header_focused {
                    "disclosure-title-focused"
                } else {
                    "disclosure-title"
                })
                .unwrap_or_else(|| widget_style(ctx.theme, enabled, header_focused))
        } else {
            ctx.theme.widget.disabled
        };
        let status = self.status.get();
        let indicator_style = if enabled {
            ctx.theme
                .named_style(status.style_name())
                .unwrap_or(ctx.theme.widget.accent)
        } else {
            ctx.theme.widget.disabled
        };
        let marker = if self.expanded.get() {
            ctx.theme.glyph("disclosure-expanded").unwrap_or("▼")
        } else {
            ctx.theme.glyph("disclosure-collapsed").unwrap_or("▶")
        };
        let indicator = ctx
            .theme
            .glyph(status.indicator_glyph())
            .unwrap_or_else(|| status.indicator_fallback());

        Line::from(vec![
            Span::styled(marker.to_string(), title_style),
            Span::styled(" ".to_string(), title_style),
            Span::styled(indicator.to_string(), indicator_style),
            Span::styled(" ".to_string(), title_style),
            Span::styled(self.title.get(), title_style),
        ])
    }

    fn content_style(&self, ctx: ComponentContext<'_>) -> Style {
        if self.enabled.get() {
            ctx.theme
                .named_style("disclosure-content")
                .unwrap_or(ctx.theme.widget.normal)
        } else {
            ctx.theme.widget.disabled
        }
    }

    fn content_local_area(area: Rect) -> Rect {
        if area.width == 0 || area.height <= 1 {
            return Rect::default();
        }
        let indent = area.width.min(2);
        Rect {
            x: indent,
            y: 1,
            width: area.width.saturating_sub(indent),
            height: area.height.saturating_sub(1),
        }
    }

    fn content_abs_area(area: Rect) -> Rect {
        let local = Self::content_local_area(area);
        Rect {
            x: area.x.saturating_add(local.x),
            y: area.y.saturating_add(local.y),
            width: local.width,
            height: local.height,
        }
    }

    fn content_text_height(&self) -> u16 {
        let Some(content) = &self.content else {
            return 0;
        };
        let text = content.get();
        text.lines().count().max(1).min(u16::MAX as usize) as u16
    }

    fn content_text_width(&self) -> u16 {
        let Some(content) = &self.content else {
            return 0;
        };
        content
            .get()
            .lines()
            .map(UnicodeWidthStr::width)
            .max()
            .unwrap_or(0)
            .min(u16::MAX as usize) as u16
    }

    fn header_width(&self) -> u16 {
        let status = self.status.get();
        let text = format!("> {} {}", status.indicator_fallback(), self.title.get());
        UnicodeWidthStr::width(text.as_str()).min(u16::MAX as usize) as u16
    }

    fn child_ctx<'a>(
        focused_child: Option<ComponentId>,
        child_id: ComponentId,
        ctx: ComponentContext<'a>,
    ) -> ComponentContext<'a> {
        ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused && focused_child == Some(child_id),
            scrollbar_host: match ctx.scrollbar_host {
                ScrollbarHost::Window => ScrollbarHost::Window,
                ScrollbarHost::Component => ctx.scrollbar_host.for_child(),
            },
            tab_mode: ctx.tab_mode.for_child(),
            mouse_coordinate_space: ctx.mouse_coordinate_space,
            drag: None,
        }
    }

    fn child_event_ctx<'a>(
        focused_child: Option<ComponentId>,
        child_id: ComponentId,
        ctx: ComponentContext<'a>,
    ) -> ComponentContext<'a> {
        ComponentContext {
            mouse_coordinate_space: ctx.mouse_coordinate_space.for_child(),
            ..Self::child_ctx(focused_child, child_id, ctx)
        }
    }

    fn handle_mouse_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Event::Mouse(m) = event else {
            return EventResult::ignored();
        };
        let Some(area) = self.last_area else {
            return EventResult::ignored();
        };
        let Some((local_x, local_y)) =
            mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space)
        else {
            return EventResult::ignored();
        };

        if local_y == 0 {
            if m.kind == MouseEventKind::Down(MouseButton::Left) {
                self.focused_child = None;
                return self.toggle();
            }
            return EventResult::ignored();
        }

        if !self.expanded.get() {
            return EventResult::ignored();
        }

        let content = Self::content_local_area(area);
        if !contains(content, local_x, local_y) {
            return EventResult::ignored();
        }

        let Some(child_id) = self.children.first().map(|child| child.id) else {
            return EventResult::ignored();
        };
        let child_idx = 0;
        if matches!(m.kind, MouseEventKind::Down(_)) && self.children[child_idx].view.is_focusable()
        {
            self.focused_child = Some(child_id);
        }

        let child_event = Event::Mouse(crossterm::event::MouseEvent {
            column: local_x.saturating_sub(content.x),
            row: local_y.saturating_sub(content.y),
            ..*m
        });
        let child_ctx = Self::child_event_ctx(self.focused_child, child_id, ctx);
        self.children[child_idx]
            .view
            .handle_event(&child_event, child_ctx)
    }

    fn handle_focused_child(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.expanded.get() {
            self.focused_child = None;
            return EventResult::ignored();
        }
        let Some(child_id) = self.focused_child else {
            return EventResult::ignored();
        };
        let Some(child_idx) = self.children.iter().position(|child| child.id == child_id) else {
            self.focused_child = None;
            return EventResult::ignored();
        };
        let child_ctx = Self::child_event_ctx(self.focused_child, child_id, ctx);
        self.children[child_idx].view.handle_event(event, child_ctx)
    }

    fn tab_direction(event: &Event) -> Option<bool> {
        match event {
            Event::Key(KeyEvent {
                code: KeyCode::Tab,
                modifiers,
                ..
            }) => Some(!modifiers.contains(KeyModifiers::SHIFT)),
            Event::Key(KeyEvent {
                code: KeyCode::BackTab,
                ..
            }) => Some(false),
            _ => None,
        }
    }
}

#[component_properties]
impl Component for Disclosure {
    fn supports_command(&self, command: &ComponentCommand) -> bool {
        matches!(command, ComponentCommand::Toggle)
    }

    fn apply_command(&mut self, command: ComponentCommand) -> EventResult {
        match command {
            ComponentCommand::Toggle if self.enabled.get() => self.toggle(),
            _ => EventResult::ignored(),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let header = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(self.header_line(ctx)), header);

        if !self.expanded.get() {
            return;
        }

        let content = Self::content_abs_area(area);
        if content.width == 0 || content.height == 0 {
            return;
        }

        if let Some(child) = self.children.first_mut() {
            child.set_bounds(Rect::new(0, 0, content.width, content.height));
            let child_ctx = Self::child_ctx(self.focused_child, child.id, ctx);
            child.view.draw(frame, content, child_ctx);
            return;
        }

        if let Some(content_text) = &self.content {
            frame.render_widget(
                Paragraph::new(content_text.get()).style(self.content_style(ctx)),
                content,
            );
        }
    }
}

impl crate::composable::DragAndDrop for Disclosure {}

impl Layout for Disclosure {
    fn min_width(&self) -> u16 {
        let child = self
            .children
            .first()
            .map_or(0, |child| child.view.min_width().saturating_add(2));
        self.header_width().max(child).max(1)
    }

    fn min_height(&self) -> u16 {
        1
    }

    fn desired_width(&self) -> Option<u16> {
        let child = self.children.first().and_then(|child| {
            child
                .view
                .desired_width()
                .map(|width| width.saturating_add(2))
        });
        Some(
            self.header_width()
                .max(child.unwrap_or_else(|| self.content_text_width().saturating_add(2))),
        )
    }

    fn desired_height(&self) -> Option<u16> {
        if !self.expanded.get() {
            return Some(1);
        }
        let content = self
            .children
            .first()
            .and_then(|child| child.view.desired_height())
            .unwrap_or_else(|| self.content_text_height());
        Some(1u16.saturating_add(content))
    }
}

impl FocusNav for Disclosure {
    fn focused_child(&self) -> Option<ComponentId> {
        self.focused_child
    }

    fn is_focusable(&self) -> bool {
        self.enabled.get()
    }

    fn focus_first(&mut self) -> bool {
        if !self.enabled.get() {
            self.focused_child = None;
            return false;
        }
        self.focused_child = None;
        true
    }

    fn focus_last(&mut self) -> bool {
        if !self.enabled.get() {
            self.focused_child = None;
            return false;
        }
        if self.expanded.get()
            && let Some(child) = self.children.first_mut()
            && child.view.is_focusable()
        {
            self.focused_child = Some(child.id);
            let _ = child.view.focus_last();
            return true;
        }
        self.focused_child = None;
        true
    }
}

impl DynamicTree for Disclosure {
    fn children(&self) -> &[ComponentNode] {
        &self.children
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        Some(&mut self.children)
    }
}

impl EventHandling for Disclosure {
    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !ctx.is_focused || !self.enabled.get() || !self.expanded.get() {
            return EventResult::ignored();
        }
        let Some(next) = Self::tab_direction(event) else {
            return EventResult::ignored();
        };
        let Some(child_id) = self.children.first().map(|child| child.id) else {
            return EventResult::ignored();
        };
        if !self.children[0].view.is_focusable() {
            return EventResult::ignored();
        }

        if next {
            if self.focused_child.is_none() {
                self.focused_child = Some(child_id);
                let _ = self.children[0].view.focus_first();
                return EventResult::consumed();
            }
            let child_ctx = Self::child_event_ctx(self.focused_child, child_id, ctx);
            let res = self.children[0].view.handle_event_capture(event, child_ctx);
            if res.is_consumed() {
                return res;
            }
            self.focused_child = None;
            return EventResult::ignored();
        }

        if self.focused_child.is_some() {
            self.focused_child = None;
            return EventResult::consumed();
        }
        EventResult::ignored()
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }
        if matches!(event, Event::Mouse(_)) {
            return self.handle_mouse_event(event, ctx);
        }

        let child = self.handle_focused_child(event, ctx);
        if child.is_consumed() {
            return child;
        }

        match event {
            Event::Key(KeyEvent {
                code: KeyCode::Enter | KeyCode::Char(' '),
                ..
            }) => self.toggle(),
            _ => EventResult::ignored(),
        }
    }
}

impl Scrollable for Disclosure {}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyEventKind, KeyModifiers, MouseEvent};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::composable::{MouseCoordinateSpace, TabMode};
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
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        }
    }

    #[test]
    fn enter_toggles_expanded_state() {
        let theme = Theme::dark();
        let mut disclosure = Disclosure::new("Details");
        let event = Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::empty(),
        });

        assert_eq!(
            disclosure.handle_event(&event, context(&theme)),
            EventResult::changed()
        );
        assert!(disclosure.is_expanded());
    }

    #[test]
    fn canceled_status_parses_and_has_distinct_indicator() {
        assert_eq!(
            DisclosureStatus::parse("canceled"),
            Some(DisclosureStatus::Canceled)
        );
        assert_eq!(DisclosureStatus::Canceled.indicator_fallback(), "[-]");
    }

    #[test]
    fn mouse_click_only_toggles_header() {
        let theme = Theme::dark();
        let mut disclosure = Disclosure::new("Details").content("line").expanded(true);
        let mut terminal = Terminal::new(TestBackend::new(20, 5)).expect("terminal");
        terminal
            .draw(|f| disclosure.draw(f, Rect::new(2, 1, 16, 3), context(&theme)))
            .expect("draw");

        let content_click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 4,
            row: 2,
            modifiers: KeyModifiers::empty(),
        });
        assert_eq!(
            disclosure.handle_event(&content_click, context(&theme)),
            EventResult::ignored()
        );
        assert!(disclosure.is_expanded());

        let header_click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 4,
            row: 1,
            modifiers: KeyModifiers::empty(),
        });
        assert_eq!(
            disclosure.handle_event(&header_click, context(&theme)),
            EventResult::changed()
        );
        assert!(!disclosure.is_expanded());
    }
}
