use ratatui::Frame;
use ratatui::layout::Rect;

use crate::view::{View, ViewContext};

/// Declarative view trait (SwiftUI-inspired).
///
/// A declarative view is a pure description of UI derived from state: custom views typically
/// implement [`DeclarativeView::body`] and return composition of other declarative views.
///
/// To integrate with the rest of Chatty (window manager, focus, event routing), the declarative
/// tree can be converted into an imperative `Box<dyn View>` via [`DeclarativeView::build_view`].
pub trait DeclarativeView: Send {
    /// Return the view's body (composition).
    ///
    /// This is intentionally a `Box<dyn DeclarativeView>` rather than an associated type so we
    /// can store heterogeneous collections.
    fn body(&self) -> Box<dyn DeclarativeView>;

    /// Render the view into the given terminal area.
    ///
    /// Default implementation renders the view's [`DeclarativeView::body`].
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.body().render(frame, area, ctx);
    }

    /// Build an imperative [`View`] tree representing this declarative view.
    ///
    /// Default implementation delegates to the view's [`DeclarativeView::body`].
    fn build_view(&self) -> Box<dyn View> {
        self.body().build_view()
    }

    /// Returns whether this view is a primitive (cannot be decomposed further).
    fn is_primitive(&self) -> bool {
        false
    }
}

/// Empty view (renders nothing).
#[derive(Clone, Debug, Default)]
pub struct EmptyView;

impl DeclarativeView for EmptyView {
    fn body(&self) -> Box<dyn DeclarativeView> {
        Box::new(EmptyView)
    }

    fn render(&self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ViewContext<'_>) {}

    fn build_view(&self) -> Box<dyn View> {
        Box::new(EmptyImperativeView)
    }

    fn is_primitive(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, Default)]
struct EmptyImperativeView;

impl View for EmptyImperativeView {
    fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ViewContext<'_>) {}
}
