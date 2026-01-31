use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
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

    fn set_area(&mut self, _area: Rect) {}

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
    layout: Vec<Option<Rect>>,
}

impl Form {
    pub fn new(controls: Vec<Box<dyn Control>>) -> Self {
        let focused = controls
            .iter()
            .position(|c| c.is_focusable())
            .or(if controls.is_empty() { None } else { Some(0) });
        let layout = vec![None; controls.len()];
        Self {
            controls,
            focused,
            layout,
        }
    }

    pub fn handle_event(&mut self, event: &Event) -> (ControlOutcome, FormAction) {
        if let Event::Mouse(m) = event {
            return self.handle_mouse_event(m, event);
        }

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
            return (ControlOutcome::Consumed, FormAction::None);
        }

        let Some(idx) = self.focused else {
            return (ControlOutcome::Ignored, FormAction::None);
        };
        let Some(control) = self.controls.get_mut(idx) else {
            return (ControlOutcome::Ignored, FormAction::None);
        };
        control.handle_event(event)
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, theme: &Theme, window_focused: bool) {
        self.layout.clear();
        self.layout.resize(self.controls.len(), None);

        let mut y = area.y;
        for (idx, control) in self.controls.iter_mut().enumerate() {
            let h = control.desired_height().min(area.y + area.height - y);
            let r = Rect {
                x: area.x,
                y,
                width: area.width,
                height: h,
            };
            self.layout[idx] = Some(r);
            control.set_area(r);
            // Only set control as focused if both window is focused AND control has focus in form
            control.set_focused(window_focused && self.focused == Some(idx));
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

    fn handle_mouse_event(
        &mut self,
        m: &MouseEvent,
        event: &Event,
    ) -> (ControlOutcome, FormAction) {
        if m.kind != MouseEventKind::Down(MouseButton::Left) {
            return (ControlOutcome::Ignored, FormAction::None);
        }

        let Some(idx) = self.hit_test(m.column, m.row) else {
            return (ControlOutcome::Ignored, FormAction::None);
        };

        let focus_changed = self
            .controls
            .get(idx)
            .is_some_and(|c| c.is_focusable() && self.focused != Some(idx));
        if focus_changed {
            self.focused = Some(idx);
        }

        let Some(control) = self.controls.get_mut(idx) else {
            return (ControlOutcome::Ignored, FormAction::None);
        };
        let (outcome, action) = control.handle_event(event);

        let consumed = focus_changed || outcome == ControlOutcome::Consumed;
        let final_outcome = if consumed {
            ControlOutcome::Consumed
        } else {
            ControlOutcome::Ignored
        };

        (final_outcome, action)
    }

    fn hit_test(&self, x: u16, y: u16) -> Option<usize> {
        for (idx, rect) in self.layout.iter().enumerate() {
            let Some(r) = rect else {
                continue;
            };
            if x >= r.x
                && x < r.x.saturating_add(r.width)
                && y >= r.y
                && y < r.y.saturating_add(r.height)
            {
                return Some(idx);
            }
        }
        None
    }
}
