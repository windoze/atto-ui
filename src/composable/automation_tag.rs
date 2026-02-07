use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use atto_ui_macros::{Automatable, automate_component};
use super::component::{Component, ComponentContext, EventResult, TitleBarContent, TitleBarContext};
use super::node::{ComponentId, ComponentNode};
use super::scroll::ScrollConfig;
use crate::automation::{AutomationAction, AutomationError, AutomationValue};

#[derive(Automatable)]
pub struct AutomationTag {
    id: String,
    inner: Box<dyn Component>,
}

impl AutomationTag {
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

pub trait AutomationIdExt: Component + Sized + 'static {
    fn automation_id(self, id: impl Into<String>) -> AutomationTag {
        AutomationTag::new(id, self)
    }
}

impl<T> AutomationIdExt for T where T: Component + Sized + 'static {}

#[automate_component]
impl Component for AutomationTag {
    fn automation_type_name(&self) -> &'static str {
        self.inner.automation_type_name()
    }

    fn automation_id(&self) -> Option<&str> {
        Some(self.id.as_str())
    }

    fn automation_focused_child(&self) -> Option<ComponentId> {
        self.inner.automation_focused_child()
    }

    fn automation_properties(&self) -> Vec<&'static str> {
        self.inner.automation_properties()
    }

    fn automation_get_property(&self, name: &str) -> Option<AutomationValue> {
        self.inner.automation_get_property(name)
    }

    fn automation_set_property(
        &mut self,
        name: &str,
        value: AutomationValue,
    ) -> Result<(), AutomationError> {
        self.inner.automation_set_property(name, value)
    }

    fn automation_action(&mut self, action: AutomationAction) -> EventResult {
        self.inner.automation_action(action)
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

impl From<(String, Box<dyn Component>)> for AutomationTag {
    fn from(value: (String, Box<dyn Component>)) -> Self {
        AutomationTag::boxed(value.0, value.1)
    }
}

impl From<(&str, Box<dyn Component>)> for AutomationTag {
    fn from(value: (&str, Box<dyn Component>)) -> Self {
        AutomationTag::boxed(value.0.to_string(), value.1)
    }
}

impl From<(String, AutomationTag)> for AutomationTag {
    fn from(value: (String, AutomationTag)) -> Self {
        let (id, mut tag) = value;
        tag.id = id;
        tag
    }
}

impl From<(&str, AutomationTag)> for AutomationTag {
    fn from(value: (&str, AutomationTag)) -> Self {
        let (id, mut tag) = value;
        tag.id = id.to_string();
        tag
    }
}
