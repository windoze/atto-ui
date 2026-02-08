use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use atto_ui_macros::{ComponentProps, component_props};
use super::component::{Component, ComponentContext, EventResult, TitleBarContent, TitleBarContext};
use super::node::{ComponentId, ComponentNode};
use super::scroll::ScrollConfig;
use crate::{ComponentCommand, ComponentError, ComponentValue};

#[derive(ComponentProps)]
pub struct ComponentTag {
    id: String,
    inner: Box<dyn Component>,
}

impl ComponentTag {
    pub fn new(id: impl Into<String>, inner: impl Component + 'static) -> Self {
        Self {
            id: id.into(),
            inner: Box::new(inner),
        }
    }

    pub fn boxed(id: impl Into<String>, inner: Box<dyn Component>) -> Self {
        Self {
            id: id.into(),
            inner,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

pub trait ComponentTagExt: Component + Sized + 'static {
    fn tag(self, id: impl Into<String>) -> ComponentTag {
        ComponentTag::new(id, self)
    }
}

impl<T> ComponentTagExt for T where T: Component + Sized + 'static {}

#[component_props]
impl Component for ComponentTag {
    fn type_name(&self) -> &'static str {
        self.inner.type_name()
    }

    fn tag(&self) -> Option<&str> {
        Some(self.id.as_str())
    }

    fn focused_child(&self) -> Option<ComponentId> {
        self.inner.focused_child()
    }

    fn property_names(&self) -> Vec<&'static str> {
        self.inner.property_names()
    }

    fn get_property(&self, name: &str) -> Option<ComponentValue> {
        self.inner.get_property(name)
    }

    fn set_property(
        &mut self,
        name: &str,
        value: ComponentValue,
    ) -> Result<(), ComponentError> {
        self.inner.set_property(name, value)
    }

    fn apply_command(&mut self, command: ComponentCommand) -> EventResult {
        self.inner.apply_command(command)
    }

    fn is_focusable(&self) -> bool {
        self.inner.is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        self.inner.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.inner.focus_last()
    }

    fn min_width(&self) -> u16 {
        self.inner.min_width()
    }

    fn min_height(&self) -> u16 {
        self.inner.min_height()
    }

    fn min_size(&self) -> (u16, u16) {
        self.inner.min_size()
    }

    fn desired_width(&self) -> Option<u16> {
        self.inner.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.inner.desired_height()
    }

    fn children(&self) -> &[ComponentNode] {
        self.inner.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        self.inner.children_mut()
    }

    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.inner.handle_event_capture(event, ctx)
    }

    fn handle_event_bubble(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.inner.handle_event_bubble(event, ctx)
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.inner.handle_event(event, ctx)
    }

    fn titlebar(&mut self, ctx: TitleBarContext<'_>) -> Option<TitleBarContent> {
        self.inner.titlebar(ctx)
    }

    fn handle_titlebar_event(&mut self, event: &Event, ctx: TitleBarContext<'_>) -> EventResult {
        self.inner.handle_titlebar_event(event, ctx)
    }

    fn is_scrollable(&self) -> bool {
        self.inner.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.inner.content_size()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.inner.scroll_offset()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.inner.viewport_size()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.inner.scroll_config()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.inner.set_scroll_offset(x, y)
    }

    fn scroll_to(&mut self, x: u16, y: u16) {
        self.inner.scroll_to(x, y)
    }

    fn scroll_to_child(&mut self, child_id: ComponentId) {
        self.inner.scroll_to_child(child_id)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.inner.draw(frame, area, ctx)
    }
}

impl From<(String, Box<dyn Component>)> for ComponentTag {
    fn from(value: (String, Box<dyn Component>)) -> Self {
        ComponentTag::boxed(value.0, value.1)
    }
}

impl From<(&str, Box<dyn Component>)> for ComponentTag {
    fn from(value: (&str, Box<dyn Component>)) -> Self {
        ComponentTag::boxed(value.0.to_string(), value.1)
    }
}

impl From<(String, ComponentTag)> for ComponentTag {
    fn from(value: (String, ComponentTag)) -> Self {
        let (id, mut tag) = value;
        tag.id = id;
        tag
    }
}

impl From<(&str, ComponentTag)> for ComponentTag {
    fn from(value: (&str, ComponentTag)) -> Self {
        let (id, mut tag) = value;
        tag.id = id.to_string();
        tag
    }
}
