use atto_ui::composable::{Component, ComponentContext, EventResult, LayoutParams, Size, VStack};
use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::input::ChatInputPanel;
use crate::list::ChatMessageList;

pub struct ChatPanel {
    view: VStack,
}

impl ChatPanel {
    pub fn new(list: ChatMessageList, input: ChatInputPanel) -> Self {
        let list_layout = LayoutParams {
            height: Size::Weight(1),
            ..LayoutParams::default()
        };
        let input_layout = LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        };

        let view = VStack::new()
            .with_spacing(1)
            .child_with_layout(list, list_layout)
            .child_with_layout(input, input_layout);

        Self { view }
    }
}

impl Component for ChatPanel {
    fn min_width(&self) -> u16 {
        self.view.min_width()
    }

    fn min_height(&self) -> u16 {
        self.view.min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.view.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.view.desired_height()
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.view.handle_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.view.draw(frame, area, ctx)
    }
}
