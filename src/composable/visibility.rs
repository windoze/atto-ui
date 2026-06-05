use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::composable::{
    Component, ComponentContext, ComponentId, ComponentNode, EventResult, ScrollConfig,
    TitleBarContent, TitleBarContext,
};
use crate::reactive::Binding;
use crate::{
    CallbackRegistry, ComponentCommand, ComponentError, ComponentSpec, ComponentValue,
    ComponentValueCodec, TreeError, TreeOp,
};
use atto_ui_macros::{ComponentProperties, component_properties};

#[derive(ComponentProperties)]
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

#[component_properties]
impl ::atto_ui::composable::Component for Visibility {
    fn property_names(&self) -> Vec<&'static str> {
        let mut props = self.inner.property_names();
        props.push("visible");
        props
    }

    fn get_property(&self, name: &str) -> Option<ComponentValue> {
        match name {
            "visible" => Some(ComponentValue::Bool(self.visible.get())),
            _ => self.inner.get_property(name),
        }
    }

    fn set_property(&mut self, name: &str, value: ComponentValue) -> Result<(), ComponentError> {
        match name {
            "visible" => {
                let v: bool = ComponentValueCodec::from_component_value(value, name)?;
                self.visible.set(v);
                Ok(())
            }
            _ => self.inner.set_property(name, value),
        }
    }

    fn apply_command(&mut self, action: ComponentCommand) -> EventResult {
        if !self.visible.get() {
            return EventResult::ignored();
        }
        self.inner.apply_command(action)
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

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if !self.visible.get() {
            return;
        }
        self.inner.draw(frame, area, ctx);
    }
}

impl ::atto_ui::composable::Layout for Visibility {
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
}

impl ::atto_ui::composable::Scrollable for Visibility {
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
}

impl ::atto_ui::composable::FocusNav for Visibility {
    fn focused_child(&self) -> Option<ComponentId> {
        if !self.visible.get() {
            return None;
        }
        self.inner.focused_child()
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
}

impl ::atto_ui::composable::DynamicTree for Visibility {
    fn children(&self) -> &[ComponentNode] {
        self.inner.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        self.inner.children_mut()
    }

    fn apply_tree_ops(&mut self, ops: &[TreeOp]) -> Result<bool, TreeError> {
        self.inner.apply_tree_ops(ops)
    }

    fn rebuild_tree(&mut self) -> Result<(), TreeError> {
        self.inner.rebuild_tree()
    }

    fn dynamic_root_spec(&self) -> Option<&ComponentSpec> {
        self.inner.dynamic_root_spec()
    }

    fn dynamic_callbacks(&self) -> Option<&CallbackRegistry> {
        self.inner.dynamic_callbacks()
    }
}

impl ::atto_ui::composable::EventHandling for Visibility {
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
}
