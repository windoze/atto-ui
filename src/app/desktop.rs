use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::Frame;

use crate::app::status::Fill;
use crate::theme::Theme;
use crate::wm::{Window, WindowId, WindowManager, WindowManagerInputMode};

use super::menu::{MenuAction, MenuBar};
use super::status::StatusBar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopMode {
    Normal,
    Menu,
    WindowManagement,
}

#[derive(Clone, Debug)]
pub enum DesktopAction {
    None,
    MenuCommand(String),
    CloseWindow(WindowId),
}

#[derive(Clone, Copy, Debug)]
pub struct DesktopLayout {
    pub menu_bar: Rect,
    pub work_area: Rect,
    pub status_bar: Rect,
}

pub struct Desktop {
    pub theme: Theme,
    pub wm: WindowManager,
    pub menu: MenuBar,
    pub status: StatusBar,
    pub mode: DesktopMode,
}

impl Desktop {
    pub fn new(theme: Theme, menu: MenuBar) -> Self {
        Self {
            theme,
            wm: WindowManager::new(),
            menu,
            status: StatusBar::default(),
            mode: DesktopMode::Normal,
        }
    }

    pub fn layout(area: Rect) -> DesktopLayout {
        let menu_bar = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1.min(area.height),
        };
        let status_bar = if area.height >= 2 {
            Rect {
                x: area.x,
                y: area.y + area.height - 1,
                width: area.width,
                height: 1,
            }
        } else {
            Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 0,
            }
        };
        let work_area = if area.height >= 3 {
            Rect {
                x: area.x,
                y: area.y + 1,
                width: area.width,
                height: area.height - 2,
            }
        } else {
            Rect {
                x: area.x,
                y: area.y.saturating_add(1),
                width: area.width,
                height: area.height.saturating_sub(1),
            }
        };
        DesktopLayout {
            menu_bar,
            work_area,
            status_bar,
        }
    }

    pub fn add_window(&mut self, window: Window, screen: Rect) -> WindowId {
        let layout = Self::layout(screen);
        self.wm.add_window(window, layout.work_area)
    }

    pub fn handle_event(&mut self, event: &Event, screen: Rect) -> DesktopAction {
        let layout = Self::layout(screen);

        // Global toggles.
        if let Event::Key(KeyEvent { code, modifiers, .. }) = event {
            if *code == KeyCode::F(10) && self.mode != DesktopMode::Menu {
                self.mode = DesktopMode::Menu;
                self.menu.activate();
                return DesktopAction::None;
            }
            if *code == KeyCode::Char('w') && modifiers.contains(KeyModifiers::CONTROL) {
                self.mode = if self.mode == DesktopMode::WindowManagement {
                    DesktopMode::Normal
                } else {
                    DesktopMode::WindowManagement
                };
                return DesktopAction::None;
            }
            if *code == KeyCode::Esc && self.mode != DesktopMode::Normal {
                self.mode = DesktopMode::Normal;
                self.menu.deactivate();
                return DesktopAction::None;
            }
        }

        match self.mode {
            DesktopMode::Menu => match self.menu.handle_event(event) {
                MenuAction::None => DesktopAction::None,
                MenuAction::Closed => {
                    self.mode = DesktopMode::Normal;
                    DesktopAction::None
                }
                MenuAction::Command(cmd) => {
                    self.mode = DesktopMode::Normal;
                    DesktopAction::MenuCommand(cmd)
                }
            },
            DesktopMode::WindowManagement | DesktopMode::Normal => {
                let input_mode = if self.mode == DesktopMode::WindowManagement {
                    WindowManagerInputMode::WindowManagement
                } else {
                    WindowManagerInputMode::Normal
                };
                let wm_action = self.wm.handle_event(event, layout.work_area, input_mode);
                if let Some(id) = wm_action.close {
                    self.wm.close(id);
                    return DesktopAction::CloseWindow(id);
                }
                if wm_action.consumed {
                    return DesktopAction::None;
                }

                if let Some((id, action)) =
                    self.wm
                        .dispatch_to_focused_view(event, layout.work_area, &self.theme)
                {
                    if action == crate::view::ViewAction::CloseWindow {
                        self.wm.close(id);
                        return DesktopAction::CloseWindow(id);
                    }
                }
                DesktopAction::None
            }
        }
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let layout = Self::layout(area);

        frame.render_widget(
            Fill {
                style: self.theme.desktop,
                ch: ' ',
            },
            area,
        );

        self.menu.draw(frame, layout.menu_bar, &self.theme);

        let status_left = match self.mode {
            DesktopMode::Normal => "F10 Menu  Ctrl+W Window  F6 Next",
            DesktopMode::Menu => "Menu: ←/→/↑/↓ Enter  Esc Close",
            DesktopMode::WindowManagement => "Window: arrows move  Shift+arrows resize  c close  x max  m min  Esc exit",
        };
        self.status.set_left(status_left);
        let focused = self
            .wm
            .focused()
            .map(|id| format!("Focus: {:?}", id.0))
            .unwrap_or_else(|| "Focus: none".to_string());
        self.status.set_right(focused);
        self.status.draw(frame, layout.status_bar, &self.theme);

        self.wm.draw(frame, layout.work_area, &self.theme);
    }
}
