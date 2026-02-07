use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::automation::{AutomationAction, AutomationError, AutomationValue};
use atto_ui_macros::{Automatable, automate_component};
use crate::composable::{
    Component, ComponentContext, ComponentId, ComponentNode, EventResult, ScrollConfig, TitleBarContent,
    TitleBarContext,
};
use crate::reactive::Binding;

#[derive(Automatable)]
pub struct Visibility {
    visible: Binding<bool>,
    inner: Box<dyn Component>,
}

impl Visibility {
    pub fn new(visible: impl Into<Binding<bool>>, inner: impl Component + 'static) -> Self {
        Self {
            visible: visible.into(),
            inner: Box::new(inner),
        }
    }

    pub fn boxed(visible: impl Into<Binding<bool>>, inner: Box<dyn Component>) -> Self {
        Self {
            visible: visible.into(),
            inner,
        }
    }

    pub fn visible(self, visible: impl Into<Binding<bool>>) -> Self {
        Self {
            visible: visible.into(),
            ..self
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }
}

pub trait VisibilityExt: Component + Sized + 'static {
    fn visible(self, visible: impl Into<Binding<bool>>) -> Visibility {
        Visibility::new(visible, self)
    }
}

impl<T> VisibilityExt for T where T: Component + Sized + 'static {}

#[automate_component]
impl Component for Visibility {
    fn automation_properties(&self) -> Vec<&'static str> {
        let mut props = self.inner.automation_properties();
        props.push("visible");
        props
    }

    fn automation_get_property(&self, name: &str) -> Option<AutomationValue> {
        match name {
            "visible" => Some(AutomationValue::Bool(self.visible.get())),
            _ => self.inner.automation_get_property(name),
        }
    }

    fn automation_set_property(
        &mut self,
        name: &str,
        value: AutomationValue,
    ) -> Result<(), AutomationError> {
        match name {
            "visible" => {
                let v = value.try_into_bool(name)?;
                self.visible.set(v);
                Ok(())
            }
            _ => self.inner.automation_set_property(name, value),
        }
    }

    fn automation_action(&mut self, action: AutomationAction) -> EventResult {
        if !self.visible.get() {
            return EventResult::ignored();
        }
        self.inner.automation_action(action)
    }

    fn automation_focused_child(&self) -> Option<ComponentId> {
        if !self.visible.get() {
            return None;
        }
        self.inner.automation_focused_child()
    }

    fn is_focusable(&self) -> bool {
        self.visible.get() && self.inner.is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        if !self.visible.get() {
            return false;
        }
        self.inner.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        if !self.visible.get() {
            return false;
        }
        self.inner.focus_last()
    }

    fn min_width(&self) -> u16 {
        if self.visible.get() {
            self.inner.min_width()
        } else {
            0
        }
    }

    fn min_height(&self) -> u16 {
        if self.visible.get() {
            self.inner.min_height()
        } else {
            0
        }
    }

    fn desired_width(&self) -> Option<u16> {
        if self.visible.get() {
            self.inner.desired_width()
        } else {
            Some(0)
        }
    }

    fn desired_height(&self) -> Option<u16> {
        if self.visible.get() {
            self.inner.desired_height()
        } else {
            Some(0)
        }
    }

    fn children(&self) -> &[ComponentNode] {
        self.inner.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        self.inner.children_mut()
    }

    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.visible.get() {
            return EventResult::ignored();
        }
        self.inner.handle_event_capture(event, ctx)
    }

    fn handle_event_bubble(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.visible.get() {
            return EventResult::ignored();
        }
        self.inner.handle_event_bubble(event, ctx)
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.visible.get() {
            return EventResult::ignored();
        }
        self.inner.handle_event(event, ctx)
    }

    fn titlebar(&mut self, ctx: TitleBarContext<'_>) -> Option<TitleBarContent> {
        if !self.visible.get() {
            return None;
        }
        self.inner.titlebar(ctx)
    }

    fn handle_titlebar_event(&mut self, event: &Event, ctx: TitleBarContext<'_>) -> EventResult {
        if !self.visible.get() {
            return EventResult::ignored();
        }
        self.inner.handle_titlebar_event(event, ctx)
    }

    fn is_scrollable(&self) -> bool {
        self.visible.get() && self.inner.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        if self.visible.get() {
            self.inner.content_size()
        } else {
            (0, 0)
        }
    }

    fn viewport_size(&self) -> (u16, u16) {
        if self.visible.get() {
            self.inner.viewport_size()
        } else {
            (0, 0)
        }
    }

    fn scroll_offset(&self) -> (u16, u16) {
        if self.visible.get() {
            self.inner.scroll_offset()
        } else {
            (0, 0)
        }
    }

    fn scroll_config(&self) -> ScrollConfig {
        if self.visible.get() {
            self.inner.scroll_config()
        } else {
            ScrollConfig::default()
        }
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        if self.visible.get() {
            self.inner.set_scroll_offset(x, y);
        }
    }

    fn scroll_to(&mut self, x: u16, y: u16) {
        if self.visible.get() {
            self.inner.scroll_to(x, y);
        }
    }

    fn scroll_to_child(&mut self, child_id: crate::composable::ComponentId) {
        if self.visible.get() {
            self.inner.scroll_to_child(child_id);
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if !self.visible.get() {
            return;
        }
        self.inner.draw(frame, area, ctx);
    }
}
