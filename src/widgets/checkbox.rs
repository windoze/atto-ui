use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use atto_ui_macros::{Automatable, automate_component};
use crate::composable::{Component, ComponentContext, EventResult};
use crate::dynamic::CallbackHandle;
use crate::reactive::Binding;

#[derive(Clone, Debug, Automatable)]
pub struct Checkbox {
    label: Binding<String>,
    #[automation(rename = "checked")]
    binding: Binding<bool>,
    enabled: Binding<bool>,
    on_change_callback: Option<CallbackHandle>,
}

impl Checkbox {
    pub fn new(label: impl Into<Binding<String>>, binding: Binding<bool>) -> Self {
        Self {
            label: label.into(),
            binding,
            enabled: true.into(),
            on_change_callback: None,
        }
    }

    pub fn label(mut self, label: impl Into<Binding<String>>) -> Self {
        self.label = label.into();
        self
    }

    pub fn enabled(mut self, enabled: impl Into<Binding<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    pub fn on_change_callback(mut self, callback: CallbackHandle) -> Self {
        self.on_change_callback = Some(callback);
        self
    }

    fn emit_change(&self) {
        if let Some(cb) = &self.on_change_callback {
            cb.emit();
        }
    }
}

#[automate_component]
impl Component for Checkbox {
    fn min_width(&self) -> u16 {
        3
    }

    fn min_height(&self) -> u16 {
        1
    }

    fn is_focusable(&self) -> bool {
        self.enabled.get()
    }

    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        if !self.enabled.get() {
            return EventResult::ignored();
        }
        match event {
            Event::Mouse(m) => {
                use crossterm::event::MouseButton;
                use crossterm::event::MouseEventKind;

                if m.kind == MouseEventKind::Down(MouseButton::Left) {
                    self.binding.update(|v| *v = !*v);
                    self.emit_change();
                    return EventResult::changed();
                }
                EventResult::ignored()
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char(' ') | KeyCode::Enter,
                ..
            }) => {
                self.binding.update(|v| *v = !*v);
                self.emit_change();
                EventResult::changed()
            }
            Event::Key(KeyEvent { .. }) => EventResult::ignored(),
            _ => EventResult::ignored(),
        }
    }

    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        let enabled = self.enabled.get();
        let style = if !enabled {
            ctx.theme.widget.disabled
        } else if ctx.is_focused {
            ctx.theme.widget.focused
        } else {
            ctx.theme.widget.normal
        };
        let mark = if self.binding.get() {
            ctx.theme.glyph("checkbox-checked").unwrap_or("[x]")
        } else {
            ctx.theme.glyph("checkbox-unchecked").unwrap_or("[ ]")
        };
        let text = format!("{mark} {}", self.label.get());
        frame.render_widget(Paragraph::new(Line::styled(text, style)), area);
    }
}
