use ratatui::Frame;
use ratatui::layout::Rect;

use crate::view::{View, ViewContext};
use crate::views::ControlView;
use crate::widgets::Control;

use super::view::{DeclarativeView, EmptyView};

/// Any cloneable widget [`Control`] can participate in the declarative layer.
///
/// This is the main bridge that allows composing existing interactive widgets (`TextBox`,
/// `Checkbox`, `Button`, ...) inside declarative layout containers such as [`super::VStack`].
impl<T> DeclarativeView for T
where
    T: Control + Clone + Send + 'static,
{
    fn body(&self) -> Box<dyn DeclarativeView> {
        Box::new(EmptyView)
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        // Rendering controls from an immutable reference requires a temporary mutable clone.
        // This is primarily useful for tests and quick one-off rendering.
        let mut control = self.clone();
        control.set_area(Rect {
            x: 0,
            y: 0,
            width: area.width,
            height: area.height,
        });
        control.set_focused(ctx.is_focused);
        control.draw(frame, area, ctx.theme);
    }

    fn build_view(&self) -> Box<dyn View> {
        Box::new(ControlView::new(Box::new(self.clone())))
    }

    fn is_primitive(&self) -> bool {
        true
    }
}
