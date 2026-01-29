use crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::Frame;

use crate::theme::Theme;
use crate::wm::WindowId;

#[derive(Clone, Copy, Debug)]
pub struct ViewContext<'a> {
    pub theme: &'a Theme,
    pub window_id: WindowId,
    pub is_focused: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewAction {
    #[default]
    None,
    CloseWindow,
}

pub trait View: Send {
    fn handle_event(&mut self, _event: &Event, _ctx: ViewContext<'_>) -> ViewAction {
        ViewAction::None
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>);
}

