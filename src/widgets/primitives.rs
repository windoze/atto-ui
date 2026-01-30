use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlOutcome {
    Consumed,
    Ignored,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormAction {
    None,
    Changed,
    Submitted,
}

pub trait Control: Send {
    fn is_focusable(&self) -> bool {
        true
    }

    fn set_focused(&mut self, _focused: bool) {}

    fn handle_event(&mut self, _event: &Event) -> (ControlOutcome, FormAction) {
        (ControlOutcome::Ignored, FormAction::None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme);

    fn desired_height(&self) -> u16 {
        1
    }
}

pub struct Form {
    controls: Vec<Box<dyn Control>>,
    focused: Option<usize>,
}

impl Form {
    pub fn new(controls: Vec<Box<dyn Control>>) -> Self {
        let focused = controls
            .iter()
            .position(|c| c.is_focusable())
            .or(if controls.is_empty() { None } else { Some(0) });
        Self { controls, focused }
    }

    pub fn handle_event(&mut self, event: &Event) -> FormAction {
        if let Event::Key(KeyEvent {
            code: KeyCode::Tab,
            modifiers,
            ..
        }) = event
        {
            if modifiers.contains(KeyModifiers::SHIFT) {
                self.focus_prev();
            } else {
                self.focus_next();
            }
            return FormAction::None;
        }

        let Some(idx) = self.focused else {
            return FormAction::None;
        };
        let Some(control) = self.controls.get_mut(idx) else {
            return FormAction::None;
        };
        let (_outcome, action) = control.handle_event(event);
        action
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let mut y = area.y;
        for (idx, control) in self.controls.iter_mut().enumerate() {
            let h = control.desired_height().min(area.y + area.height - y);
            let r = Rect {
                x: area.x,
                y,
                width: area.width,
                height: h,
            };
            control.set_focused(self.focused == Some(idx));
            control.draw(frame, r, theme);
            y = y.saturating_add(h);
            if y >= area.y.saturating_add(area.height) {
                break;
            }
        }
    }

    fn focus_next(&mut self) {
        if self.controls.is_empty() {
            self.focused = None;
            return;
        }
        let start = self.focused.unwrap_or(0);
        for i in 1..=self.controls.len() {
            let idx = (start + i) % self.controls.len();
            if self.controls[idx].is_focusable() {
                self.focused = Some(idx);
                return;
            }
        }
    }

    fn focus_prev(&mut self) {
        if self.controls.is_empty() {
            self.focused = None;
            return;
        }
        let start = self.focused.unwrap_or(0);
        for i in 1..=self.controls.len() {
            let idx = (start + self.controls.len() - i) % self.controls.len();
            if self.controls[idx].is_focusable() {
                self.focused = Some(idx);
                return;
            }
        }
    }
}
