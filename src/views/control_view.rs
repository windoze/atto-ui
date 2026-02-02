use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::view::{EventOutcome, View, ViewContext, ViewEventResult};
use crate::widgets::{Control, ControlOutcome, FormAction};

/// Adapts a widget [`Control`] to the [`View`] trait so it can participate in layout containers.
pub struct ControlView {
    control: Box<dyn Control>,
}

impl ControlView {
    pub fn new(control: Box<dyn Control>) -> Self {
        Self { control }
    }
}

impl View for ControlView {
    fn is_focusable(&self) -> bool {
        self.control.is_focusable()
    }

    fn min_width(&self) -> u16 {
        self.control.min_width()
    }

    fn min_height(&self) -> u16 {
        self.control.min_height()
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.control.desired_height())
    }

    fn handle_event(&mut self, event: &Event, _ctx: ViewContext<'_>) -> ViewEventResult {
        let (outcome, action) = self.control.handle_event(event);

        let consumed = outcome == ControlOutcome::Consumed || action != FormAction::None;
        let outcome = if consumed {
            EventOutcome::Consumed
        } else {
            EventOutcome::Ignored
        };

        ViewEventResult {
            outcome,
            action: crate::view::ViewAction::None,
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        // `Control` receives events in its own local coordinate space, so its `area` for hit
        // testing should always be `(0,0,width,height)`.
        self.control.set_area(Rect {
            x: 0,
            y: 0,
            width: area.width,
            height: area.height,
        });
        self.control.set_focused(ctx.is_focused);
        self.control.draw(frame, area, ctx.theme);
    }
}
