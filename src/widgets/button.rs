use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::composable::{
    Component, ComponentContext, EventHandling, EventResult, FocusNav, Layout,
};
use crate::reactive::Binding;
use crate::runtime::CallbackHandle;
use atto_ui_macros::{ComponentProperties, component_properties};

use super::util::{mouse_coords_local_to_area, widget_style};

#[derive(Clone, ComponentProperties)]
pub struct Button {
    label: Binding<String>,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    on_click_callback: Option<CallbackHandle>,
    enabled: Binding<bool>,
    last_area: Option<Rect>,
}

impl Button {
    pub fn new(label: impl Into<Binding<String>>) -> Self {
        Self {
            label: label.into(),
            on_click: None,
            on_click_callback: None,
            enabled: true.into(),
            last_area: None,
        }
    }

    pub fn on_click<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_click = Some(Arc::new(callback));
        self
    }

    pub fn on_click_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_click_callback = Some(callback);
        self
    }

    pub fn label(mut self, label: impl Into<Binding<String>>) -> Self {
        self.label = label.into();
        self
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    fn trigger(&self) {
        if let Some(cb) = &self.on_click {
            cb();
        }
        if let Some(cb) = &self.on_click_callback {
            cb.emit();
        }
    }
}

#[component_properties]
impl Component for Button {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        let enabled = self.enabled.get();
        let style = widget_style(ctx.theme, enabled, ctx.is_focused);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(style)
            .border_set(ctx.theme.border_set(false));
        let text = Line::styled(format!(" {} ", self.label.get()), style);
        let p = Paragraph::new(text).block(block);
        frame.render_widget(p, area);
    }
}

impl Layout for Button {
    fn min_width(&self) -> u16 {
        3
    }

    fn min_height(&self) -> u16 {
        3
    }

    fn desired_height(&self) -> Option<u16> {
        Some(3)
    }
}

impl FocusNav for Button {
    fn is_focusable(&self) -> bool {
        self.enabled.get()
    }
}

impl EventHandling for Button {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }
        match event {
            Event::Mouse(m) => {
                use crossterm::event::MouseButton;
                use crossterm::event::MouseEventKind;

                if m.kind == MouseEventKind::Down(MouseButton::Left) {
                    let Some(area) = self.last_area else {
                        return EventResult::ignored();
                    };
                    if mouse_coords_local_to_area(area, *m, ctx.mouse_coordinate_space).is_none() {
                        return EventResult::ignored();
                    }
                    self.trigger();
                    return EventResult::submitted();
                }
                EventResult::ignored()
            }
            Event::Key(KeyEvent {
                code: KeyCode::Enter | KeyCode::Char(' '),
                ..
            }) => {
                self.trigger();
                EventResult::submitted()
            }
            Event::Key(KeyEvent { .. }) => EventResult::ignored(),
            _ => EventResult::ignored(),
        }
    }
}

crate::impl_component_default_traits!(Button => Scrollable, DynamicTree);

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::composable::{MouseCoordinateSpace, ScrollbarHost, TabMode};
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
        }
    }

    #[test]
    fn mouse_down_outside_last_area_does_not_click() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_button = Arc::clone(&calls);
        let mut button = Button::new("OK").on_click(move || {
            calls_for_button.fetch_add(1, Ordering::SeqCst);
        });
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(20, 10)).expect("terminal");
        terminal
            .draw(|f| button.draw(f, Rect::new(10, 5, 6, 3), context(&theme)))
            .expect("draw");

        let outside = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::empty(),
        });
        assert_eq!(
            button.handle_event(&outside, context(&theme)),
            EventResult::ignored()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        let inside = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 11,
            row: 6,
            modifiers: KeyModifiers::empty(),
        });
        assert_eq!(
            button.handle_event(&inside, context(&theme)),
            EventResult::submitted()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn disabled_button_ignores_keyboard_and_mouse() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_button = Arc::clone(&calls);
        let mut button = Button::new("OK").enabled(false).on_click(move || {
            calls_for_button.fetch_add(1, Ordering::SeqCst);
        });
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(20, 10)).expect("terminal");
        terminal
            .draw(|f| button.draw(f, Rect::new(2, 2, 6, 3), context(&theme)))
            .expect("draw");

        let key = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            button.handle_event(&key, context(&theme)),
            EventResult::ignored()
        );

        let mouse = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 3,
            modifiers: KeyModifiers::empty(),
        });
        assert_eq!(
            button.handle_event(&mouse, context(&theme)),
            EventResult::ignored()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
