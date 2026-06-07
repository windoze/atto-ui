use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use crate::app::status::Fill;
use crate::composable::{ComponentAction, EventOutcome};
use crate::theme::Theme;
use crate::wm::{Window, WindowId, WindowKind, WindowManager, WindowManagerInputMode, WindowState};
use crate::{CallbackRegistry, ComponentSpec, ComponentValue, TreeError, TreeOp};

use super::menu::{MenuAction, MenuBar};
use super::status::StatusBar;
use super::toast::{Toast, ToastQueue};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopMode {
    Normal,
    Menu,
    WindowManagement,
}

#[derive(Clone, Debug)]
pub enum DesktopAction {
    None,
    CloseWindow(WindowId),
}

#[derive(Clone, Debug)]
pub struct DesktopEventResult {
    pub outcome: EventOutcome,
    pub action: DesktopAction,
}

impl DesktopEventResult {
    pub const fn ignored() -> Self {
        Self {
            outcome: EventOutcome::Ignored,
            action: DesktopAction::None,
        }
    }

    pub const fn consumed() -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: DesktopAction::None,
        }
    }

    pub const fn close_window(id: WindowId) -> Self {
        Self {
            outcome: EventOutcome::Consumed,
            action: DesktopAction::CloseWindow(id),
        }
    }

    pub const fn is_consumed(&self) -> bool {
        matches!(self.outcome, EventOutcome::Consumed)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DesktopLayout {
    pub menu_bar: Rect,
    pub work_area: Rect,
    pub status_bar: Rect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: WindowId,
    pub tag: Option<String>,
    pub title: String,
    pub kind: WindowKind,
    pub state: WindowState,
    pub rect: Rect,
    pub is_focused: bool,
}

pub struct Desktop {
    pub theme: Theme,
    pub wm: WindowManager,
    pub menu: MenuBar,
    pub status: StatusBar,
    pub toasts: ToastQueue,
    pub mode: DesktopMode,
}

impl Desktop {
    pub fn new(theme: Theme, menu: MenuBar) -> Self {
        Self {
            theme,
            wm: WindowManager::new(),
            menu,
            status: StatusBar::default(),
            toasts: ToastQueue::default(),
            mode: DesktopMode::Normal,
        }
    }

    pub fn push_toast(&mut self, toast: Toast) {
        self.toasts.push(toast);
    }

    pub fn notify_background_complete(&mut self, message: impl Into<String>) {
        self.toasts.notify_background_complete(message);
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

    pub fn add_dynamic_window(
        &mut self,
        kind: WindowKind,
        title: impl Into<crate::reactive::Binding<String>>,
        rect: impl Into<crate::reactive::Binding<Rect>>,
        root: ComponentSpec,
        callbacks: CallbackRegistry,
        screen: Rect,
    ) -> Result<WindowId, TreeError> {
        let window = Window::new_dynamic(kind, title, rect, root, callbacks)?;
        Ok(self.add_window(window, screen))
    }

    pub fn apply_tree_ops(
        &mut self,
        window_id: WindowId,
        ops: &[TreeOp],
    ) -> Result<bool, TreeError> {
        self.wm.apply_tree_ops(window_id, ops)
    }

    pub fn rebuild_dynamic_window(&mut self, window_id: WindowId) -> Result<(), TreeError> {
        self.wm.rebuild_dynamic_window(window_id)
    }

    pub fn send_event_to_window(
        &mut self,
        window_id: WindowId,
        event: Event,
        screen: Rect,
    ) -> DesktopEventResult {
        if self.wm.window(window_id).is_none() {
            return DesktopEventResult::ignored();
        }

        if self.wm.has_active_modal() && self.wm.focused() != Some(window_id) {
            return DesktopEventResult::consumed();
        }

        if !self.wm.has_active_modal() {
            self.focus_window(window_id);
        }

        let Some(event) = self.window_relative_event(window_id, event) else {
            return DesktopEventResult::ignored();
        };

        let layout = Self::layout(screen);
        let Some((id, res)) =
            self.wm
                .dispatch_to_window_view(window_id, &event, layout.work_area, &self.theme)
        else {
            return DesktopEventResult::ignored();
        };

        if res.action == ComponentAction::CloseWindow {
            if self.wm.request_close(id) {
                return DesktopEventResult::close_window(id);
            }
            return DesktopEventResult::consumed();
        }
        if res.is_consumed() {
            DesktopEventResult::consumed()
        } else {
            DesktopEventResult::ignored()
        }
    }

    pub fn close_window(&mut self, id: WindowId) -> bool {
        self.wm.request_close(id)
    }

    pub fn focus_window(&mut self, id: WindowId) -> bool {
        if self.wm.has_active_modal() {
            return self.wm.focused() == Some(id);
        }

        if !self
            .wm
            .window(id)
            .is_some_and(|w| w.kind.is_focusable() && w.state.get() != WindowState::Minimized)
        {
            return false;
        }

        self.wm.focus(id);
        self.wm.focused() == Some(id)
    }

    pub fn move_window(&mut self, id: WindowId, x: u16, y: u16, screen: Rect) -> bool {
        let layout = Self::layout(screen);
        self.wm.move_window_to(id, x, y, layout.work_area)
    }

    pub fn resize_window(&mut self, id: WindowId, width: u16, height: u16, screen: Rect) -> bool {
        let layout = Self::layout(screen);
        self.wm
            .resize_window_to(id, width, height, layout.work_area)
    }

    pub fn list_windows(&self) -> Vec<WindowInfo> {
        let focused = self.wm.focused();
        self.wm
            .windows()
            .iter()
            .map(|w| WindowInfo {
                id: w.id(),
                tag: w.tag.clone(),
                title: w.title.get(),
                kind: w.kind,
                state: w.state.get(),
                rect: w.rect.get(),
                is_focused: focused == Some(w.id()),
            })
            .collect()
    }

    pub fn set_title(&mut self, id: WindowId, title: impl Into<String>) -> bool {
        let Some(window) = self.wm.window_mut(id) else {
            return false;
        };
        window.title.set(title.into());
        true
    }

    pub fn set_property(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        value: ComponentValue,
    ) -> Result<(), TreeError> {
        let id = id.into();
        let name = name.into();
        let window_ids: Vec<WindowId> = self
            .wm
            .windows()
            .iter()
            .filter(|w| w.dynamic_root_spec().is_some())
            .map(Window::id)
            .collect();
        let op = TreeOp::SetProp {
            id: id.clone(),
            name,
            value,
        };

        for window_id in window_ids {
            match self.apply_tree_ops(window_id, std::slice::from_ref(&op)) {
                Ok(_) => return Ok(()),
                Err(TreeError::NotFound(_)) => {}
                Err(err) => return Err(err),
            }
        }

        Err(TreeError::NotFound(id))
    }

    fn window_relative_event(&self, window_id: WindowId, event: Event) -> Option<Event> {
        let Event::Mouse(mouse) = event else {
            return Some(event);
        };
        let rect = self.wm.window(window_id)?.rect.get();
        Some(Event::Mouse(MouseEvent {
            column: rect.x.saturating_add(mouse.column),
            row: rect.y.saturating_add(mouse.row),
            ..mouse
        }))
    }

    pub fn handle_event(&mut self, event: &Event, screen: Rect) -> DesktopEventResult {
        let layout = Self::layout(screen);
        self.menu.refresh_minimized_windows(&self.wm);

        let input_mode = if self.mode == DesktopMode::WindowManagement {
            WindowManagerInputMode::WindowManagement
        } else {
            WindowManagerInputMode::Normal
        };

        if self.wm.has_global_drag()
            && matches!(
                event,
                Event::Mouse(_)
                    | Event::Key(KeyEvent {
                        code: KeyCode::Esc,
                        ..
                    })
            )
        {
            let wm_action = self
                .wm
                .handle_event(event, layout.work_area, input_mode, &self.theme);
            if let Some(id) = wm_action.close {
                if self.wm.request_close(id) {
                    return DesktopEventResult::close_window(id);
                }
                return DesktopEventResult::consumed();
            }
            if wm_action.consumed {
                return DesktopEventResult::consumed();
            }
        }

        // Desktop chrome mouse routing (menu bar / status bar) comes first so clicks don't
        // accidentally fall through to the focused view.
        if let Event::Mouse(m) = event {
            if layout.status_bar.height > 0 && m.row == layout.status_bar.y {
                return DesktopEventResult::consumed();
            }
            if layout.menu_bar.height > 0 && m.row == layout.menu_bar.y {
                match self.menu.handle_mouse(m, layout.menu_bar) {
                    MenuAction::None => {
                        if self.menu.is_active() {
                            self.mode = DesktopMode::Menu;
                        }
                        return DesktopEventResult::consumed();
                    }
                    MenuAction::Closed => {
                        self.mode = DesktopMode::Normal;
                        self.menu.deactivate();
                        return DesktopEventResult::consumed();
                    }
                    MenuAction::RestoreWindow(id) => {
                        self.wm.restore_window(id);
                        self.mode = DesktopMode::Normal;
                        self.menu.deactivate();
                        return DesktopEventResult::consumed();
                    }
                }
            }
        }

        // Menu captures all input while active.
        if self.mode == DesktopMode::Menu || self.menu.is_active() {
            if self.mode != DesktopMode::Menu {
                self.mode = DesktopMode::Menu;
            }
            let action = match event {
                Event::Mouse(m) => self.menu.handle_mouse(m, layout.menu_bar),
                _ => self.menu.handle_event(event),
            };
            let action = match action {
                MenuAction::None => DesktopAction::None,
                MenuAction::Closed => {
                    self.mode = DesktopMode::Normal;
                    self.menu.deactivate();
                    DesktopAction::None
                }
                MenuAction::RestoreWindow(id) => {
                    self.wm.restore_window(id);
                    self.mode = DesktopMode::Normal;
                    self.menu.deactivate();
                    DesktopAction::None
                }
            };
            return DesktopEventResult {
                outcome: EventOutcome::Consumed,
                action,
            };
        }

        let modal_active = self.wm.has_active_modal();

        let mut view_dispatched = false;

        // Layered input:
        //  1. Focused view receives the event (normal mode only; keys/paste/etc).
        //  2. Focused window (WindowManager) receives the event.
        //  3. Desktop receives the event (global shortcuts), unless a modal is open.
        if input_mode == WindowManagerInputMode::Normal && !matches!(event, Event::Mouse(_)) {
            view_dispatched = true;
            if let Some((id, res)) =
                self.wm
                    .dispatch_to_focused_view(event, layout.work_area, &self.theme)
            {
                if res.action == ComponentAction::CloseWindow {
                    if self.wm.request_close(id) {
                        return DesktopEventResult::close_window(id);
                    }
                    return DesktopEventResult::consumed();
                }
                if res.is_consumed() {
                    return DesktopEventResult::consumed();
                }
            }
        }

        let wm_action = self
            .wm
            .handle_event(event, layout.work_area, input_mode, &self.theme);
        if let Some(id) = wm_action.close {
            if self.wm.request_close(id) {
                return DesktopEventResult::close_window(id);
            }
            return DesktopEventResult::consumed();
        }
        if wm_action.consumed {
            return DesktopEventResult::consumed();
        }

        // Mouse events need to hit-test and potentially change focus before dispatching to the view,
        // so we dispatch them after the WindowManager.
        if input_mode == WindowManagerInputMode::Normal
            && !view_dispatched
            && let Event::Mouse(m) = event
            && let Some(target_id) = self.wm.window_at(m.column, m.row)
            && self.wm.window_kind(target_id) == Some(WindowKind::Tooltip)
            && let Some((id, res)) =
                self.wm
                    .dispatch_to_window_view(target_id, event, layout.work_area, &self.theme)
        {
            if res.action == ComponentAction::CloseWindow {
                if self.wm.request_close(id) {
                    return DesktopEventResult::close_window(id);
                }
                return DesktopEventResult::consumed();
            }
            if res.is_consumed() {
                return DesktopEventResult::consumed();
            }
        }

        if input_mode == WindowManagerInputMode::Normal
            && !view_dispatched
            && let Some((id, res)) =
                self.wm
                    .dispatch_to_focused_view(event, layout.work_area, &self.theme)
        {
            if res.action == ComponentAction::CloseWindow {
                if self.wm.request_close(id) {
                    return DesktopEventResult::close_window(id);
                }
                return DesktopEventResult::consumed();
            }
            if res.is_consumed() {
                return DesktopEventResult::consumed();
            }
        }

        // Modals act as an event sink: even if the modal view ignores an event, it should not
        // propagate to desktop-level shortcuts.
        if modal_active {
            return DesktopEventResult::consumed();
        }

        // Desktop-level shortcuts (press only).
        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            if *code == KeyCode::F(10) {
                self.mode = DesktopMode::Menu;
                self.menu.activate();
                return DesktopEventResult::consumed();
            }

            if modifiers.contains(KeyModifiers::ALT)
                && let KeyCode::Char(c) = *code
                && let Some(menu_idx) = self.menu.menu_index_for_shortcut(c)
            {
                self.mode = DesktopMode::Menu;
                self.menu.activate_menu(menu_idx);
                return DesktopEventResult::consumed();
            }

            if *code == KeyCode::Char('w') && modifiers.contains(KeyModifiers::CONTROL) {
                self.menu.deactivate();
                self.mode = if self.mode == DesktopMode::WindowManagement {
                    DesktopMode::Normal
                } else {
                    DesktopMode::WindowManagement
                };
                return DesktopEventResult::consumed();
            }

            if *code == KeyCode::Esc && self.mode != DesktopMode::Normal {
                self.mode = DesktopMode::Normal;
                self.menu.deactivate();
                return DesktopEventResult::consumed();
            }
        }

        DesktopEventResult::ignored()
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let layout = Self::layout(area);
        self.menu.refresh_minimized_windows(&self.wm);

        frame.render_widget(
            Fill {
                style: self.theme.desktop,
                ch: ' ',
            },
            area,
        );

        // Draw windows before chrome overlays so dropdown menus/tooltips can render on top.
        self.wm.draw(frame, layout.work_area, &self.theme);

        self.menu.draw(frame, layout.menu_bar, &self.theme);

        if !self.status.has_custom() {
            let status_left = match self.mode {
                DesktopMode::Normal => "F10 Menu  Ctrl+W Window  F6 Next",
                DesktopMode::Menu => "Menu: ←/→/↑/↓ Enter  Esc Close",
                DesktopMode::WindowManagement => {
                    "Window: arrows move  Shift+arrows resize  c close  x max  m min  Esc exit"
                }
            };
            self.status.set_left(status_left);
            let focused = self
                .wm
                .focused()
                .map(|id| format!("Focus: {:?}", id.0))
                .unwrap_or_else(|| "Focus: none".to_string());
            self.status.set_right(focused);
        }
        self.status.draw(frame, layout.status_bar, &self.theme);
        self.toasts.draw(frame, layout.work_area, &self.theme);

        // ratatui's buffer diff assumes buffers are "well-formed": a multi-width glyph is
        // followed only by blank cells. When layered UI elements (e.g., window borders) overwrite
        // the trailing cell of a wide glyph, the buffer becomes ill-formed and ratatui may skip
        // emitting updates for the overwritten cell(s). Normalize the final frame buffer to
        // ensure wide glyphs never straddle non-blank cells.
        sanitize_wide_glyph_overlaps(frame.buffer_mut());
    }
}

fn sanitize_wide_glyph_overlaps(buf: &mut Buffer) {
    let w = buf.area.width as usize;
    let h = buf.area.height as usize;
    if w == 0 || h == 0 {
        return;
    }

    for y in 0..h {
        let row_start = y * w;
        for x in 0..w {
            let idx = row_start + x;
            let symbol = buf.content[idx].symbol();
            let glyph_w = UnicodeWidthStr::width(symbol).max(1);
            if glyph_w <= 1 {
                continue;
            }

            let mut trailing_cells_blank = true;
            for k in 1..glyph_w {
                let nx = x + k;
                if nx >= w {
                    trailing_cells_blank = false;
                    break;
                }
                let nidx = row_start + nx;
                if buf.content[nidx].symbol() != " " {
                    trailing_cells_blank = false;
                    break;
                }
            }

            if trailing_cells_blank {
                continue;
            }

            // Hide the wide glyph completely by clearing its starting cell while preserving style.
            buf.content[idx].set_symbol(" ").set_skip(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composable::{
        Component, ComponentContext, DragAndDrop, DragOperation, DragPayload, DragSource,
        EventHandling, EventResult,
    };
    use crate::theme::Theme;
    use crate::wm::{Window, WindowKind};
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct ConsumeF6View;

    impl Component for ConsumeF6View {
        fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
    }

    impl EventHandling for ConsumeF6View {
        fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
            if let Event::Key(KeyEvent { code, .. }) = event
                && *code == KeyCode::F(6)
            {
                return EventResult::consumed();
            }
            EventResult::ignored()
        }
    }

    crate::impl_component_default_traits!(ConsumeF6View => Layout, Scrollable, FocusNav, DynamicTree);

    #[derive(Clone)]
    struct CountingMouseView {
        downs: Arc<AtomicUsize>,
    }

    impl CountingMouseView {
        fn new(downs: Arc<AtomicUsize>) -> Self {
            Self { downs }
        }
    }

    impl Component for CountingMouseView {
        fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
    }

    impl EventHandling for CountingMouseView {
        fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
            if let Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                ..
            }) = event
            {
                self.downs.fetch_add(1, Ordering::SeqCst);
                return EventResult::consumed();
            }
            EventResult::ignored()
        }
    }

    crate::impl_component_default_traits!(CountingMouseView => Layout, Scrollable, FocusNav, DynamicTree);

    #[derive(Clone)]
    struct DesktopDragSourceView {
        cancels: Arc<AtomicUsize>,
    }

    impl DesktopDragSourceView {
        fn new(cancels: Arc<AtomicUsize>) -> Self {
            Self { cancels }
        }
    }

    impl Component for DesktopDragSourceView {
        fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
    }

    impl DragAndDrop for DesktopDragSourceView {
        fn drag_source_at(
            &self,
            _screen_x: u16,
            _screen_y: u16,
            _ctx: ComponentContext<'_>,
        ) -> Option<DragSource> {
            Some(DragSource {
                payload: DragPayload::Text("desktop-drag".to_string()),
                operation: DragOperation::Copy,
                threshold: 1,
                ghost: None,
            })
        }

        fn drag_cancelled(&mut self, _ctx: ComponentContext<'_>) {
            self.cancels.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl crate::composable::Layout for DesktopDragSourceView {}
    impl crate::composable::Scrollable for DesktopDragSourceView {}
    impl crate::composable::FocusNav for DesktopDragSourceView {}
    impl crate::composable::DynamicTree for DesktopDragSourceView {}
    impl EventHandling for DesktopDragSourceView {}

    #[derive(Clone)]
    struct RecordingView {
        events: Arc<Mutex<Vec<String>>>,
    }

    impl RecordingView {
        fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
            Self { events }
        }
    }

    impl Component for RecordingView {
        fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
    }

    impl EventHandling for RecordingView {
        fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
            let entry = match event {
                Event::Key(KeyEvent { code, .. }) => format!("key:{code:?}"),
                Event::Mouse(mouse) => format!("mouse:{},{}", mouse.column, mouse.row),
                Event::Paste(text) => format!("paste:{text}"),
                _ => return EventResult::ignored(),
            };
            self.events.lock().expect("events lock").push(entry);
            EventResult::consumed()
        }
    }

    crate::impl_component_default_traits!(RecordingView => Layout, Scrollable, FocusNav, DynamicTree);

    #[test]
    fn focused_view_can_consume_event_before_window_manager() {
        let screen = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));

        let _id1 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "One",
                Rect {
                    x: 2,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(ConsumeF6View),
            ),
            screen,
        );
        let id2 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Two",
                Rect {
                    x: 25,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(ConsumeF6View),
            ),
            screen,
        );

        assert_eq!(desktop.wm.focused(), Some(id2));
        let result = desktop.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)),
            screen,
        );
        assert!(result.is_consumed());
        assert_eq!(
            desktop.wm.focused(),
            Some(id2),
            "expected view consumption to prevent WindowManager focus cycling"
        );
    }

    #[test]
    fn ignored_view_event_bubbles_to_window_manager() {
        struct IgnoreAllView;

        impl Component for IgnoreAllView {
            fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
        }

        impl EventHandling for IgnoreAllView {
            fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
                EventResult::ignored()
            }
        }

        crate::impl_component_default_traits!(IgnoreAllView => Layout, Scrollable, FocusNav, DynamicTree);

        let screen = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));

        let id1 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "One",
                Rect {
                    x: 2,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(IgnoreAllView),
            ),
            screen,
        );
        let id2 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Two",
                Rect {
                    x: 25,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(IgnoreAllView),
            ),
            screen,
        );

        assert_eq!(desktop.wm.focused(), Some(id2));
        let result = desktop.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)),
            screen,
        );
        assert!(result.is_consumed());
        assert_eq!(
            desktop.wm.focused(),
            Some(id1),
            "expected unhandled F6 to bubble to WindowManager focus_next"
        );
    }

    #[test]
    fn modal_window_blocks_desktop_shortcuts() {
        struct IgnoreAllView;

        impl Component for IgnoreAllView {
            fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
        }

        impl EventHandling for IgnoreAllView {
            fn handle_event(&mut self, _event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
                EventResult::ignored()
            }
        }

        crate::impl_component_default_traits!(IgnoreAllView => Layout, Scrollable, FocusNav, DynamicTree);

        let screen = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));

        let _normal_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Normal",
                Rect {
                    x: 2,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(IgnoreAllView),
            ),
            screen,
        );
        let modal_id = desktop.add_window(
            Window::new(
                WindowKind::Modal,
                "Modal",
                Rect {
                    x: 10,
                    y: 8,
                    width: 30,
                    height: 8,
                },
                Box::new(IgnoreAllView),
            ),
            screen,
        );

        assert!(desktop.wm.has_active_modal());
        assert_eq!(desktop.wm.focused(), Some(modal_id));

        let result = desktop.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            screen,
        );
        assert!(result.is_consumed());
        assert_eq!(
            desktop.mode,
            DesktopMode::Normal,
            "expected Ctrl+W to be blocked while a modal is open"
        );

        let result = desktop.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE)),
            screen,
        );
        assert!(result.is_consumed());
        assert_eq!(
            desktop.mode,
            DesktopMode::Normal,
            "expected F10 to be blocked while a modal is open"
        );
        assert!(
            !desktop.menu.is_active(),
            "expected menu to remain inactive"
        );
    }

    #[test]
    fn mouse_body_click_dispatches_to_focused_view() {
        let screen = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));

        let clicks_one = Arc::new(AtomicUsize::new(0));
        let clicks_two = Arc::new(AtomicUsize::new(0));

        let id1 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "One",
                Rect {
                    x: 2,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(CountingMouseView::new(Arc::clone(&clicks_one))),
            ),
            screen,
        );
        let id2 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Two",
                Rect {
                    x: 25,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(CountingMouseView::new(Arc::clone(&clicks_two))),
            ),
            screen,
        );

        assert_eq!(desktop.wm.focused(), Some(id2));

        // Click inside window "One" body (not the title bar).
        let result = desktop.handle_event(
            &Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 3,
                row: 3,
                modifiers: KeyModifiers::NONE,
            }),
            screen,
        );
        assert!(result.is_consumed());

        assert_eq!(desktop.wm.focused(), Some(id1));
        assert_eq!(clicks_one.load(Ordering::SeqCst), 1);
        assert_eq!(clicks_two.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn global_drag_mouse_up_on_desktop_chrome_clears_drag() {
        let screen = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
        let cancels = Arc::new(AtomicUsize::new(0));

        desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Drag",
                Rect {
                    x: 2,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(DesktopDragSourceView::new(Arc::clone(&cancels))),
            ),
            screen,
        );

        let down = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });
        desktop.handle_event(&down, screen);
        assert!(desktop.wm.has_global_drag());

        let drag_to_status_bar = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 3,
            row: screen.height.saturating_sub(1),
            modifiers: KeyModifiers::NONE,
        });
        let drag_result = desktop.handle_event(&drag_to_status_bar, screen);
        assert!(drag_result.is_consumed());
        assert!(desktop.wm.has_global_drag());

        let up_on_status_bar = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: 3,
            row: screen.height.saturating_sub(1),
            modifiers: KeyModifiers::NONE,
        });
        let up_result = desktop.handle_event(&up_on_status_bar, screen);

        assert!(up_result.is_consumed());
        assert!(!desktop.wm.has_global_drag());
        assert_eq!(cancels.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn send_event_to_window_routes_to_target_with_relative_mouse_coords() {
        let screen = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));

        let events_one = Arc::new(Mutex::new(Vec::new()));
        let events_two = Arc::new(Mutex::new(Vec::new()));

        let id1 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "One",
                Rect {
                    x: 2,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(RecordingView::new(Arc::clone(&events_one))),
            ),
            screen,
        );
        let id2 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Two",
                Rect {
                    x: 30,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(RecordingView::new(Arc::clone(&events_two))),
            ),
            screen,
        );

        assert_eq!(desktop.wm.focused(), Some(id2));

        let key_result = desktop.send_event_to_window(
            id1,
            Event::Key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE)),
            screen,
        );
        assert!(key_result.is_consumed());
        assert_eq!(desktop.wm.focused(), Some(id1));

        let paste_result = desktop.send_event_to_window(id1, Event::Paste("hello".into()), screen);
        assert!(paste_result.is_consumed());

        let mouse_result = desktop.send_event_to_window(
            id1,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: 2,
                modifiers: KeyModifiers::NONE,
            }),
            screen,
        );
        assert!(mouse_result.is_consumed());

        assert_eq!(
            events_one.lock().expect("events one").as_slice(),
            ["key:Char('k')", "paste:hello", "mouse:4,4"]
        );
        assert!(events_two.lock().expect("events two").is_empty());
    }

    #[test]
    fn window_management_methods_update_listed_state() {
        struct IgnoreAllView;

        impl Component for IgnoreAllView {
            fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
        }

        impl EventHandling for IgnoreAllView {}

        crate::impl_component_default_traits!(IgnoreAllView => Layout, Scrollable, FocusNav, DynamicTree);

        let screen = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
        let id1 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "One",
                Rect {
                    x: 2,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(IgnoreAllView),
            ),
            screen,
        );
        let id2 = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Two",
                Rect {
                    x: 30,
                    y: 2,
                    width: 20,
                    height: 6,
                },
                Box::new(IgnoreAllView),
            ),
            screen,
        );

        assert!(desktop.focus_window(id1));
        assert!(desktop.move_window(id1, 5, 6, screen));
        assert!(desktop.resize_window(id1, 30, 9, screen));
        assert!(desktop.set_title(id1, "Renamed"));

        let windows = desktop.list_windows();
        assert_eq!(windows.last().map(|w| w.id), Some(id1));
        let first = windows.iter().find(|w| w.id == id1).expect("id1 info");
        assert_eq!(first.title, "Renamed");
        assert_eq!(first.rect, Rect::new(5, 6, 30, 9));
        assert!(first.is_focused);

        assert!(desktop.close_window(id2));
        assert!(!desktop.list_windows().iter().any(|w| w.id == id2));
    }

    #[test]
    fn focus_window_respects_modal_and_minimized_state() {
        struct IgnoreAllView;

        impl Component for IgnoreAllView {
            fn draw(&mut self, _frame: &mut Frame<'_>, _area: Rect, _ctx: ComponentContext<'_>) {}
        }

        impl EventHandling for IgnoreAllView {}

        crate::impl_component_default_traits!(IgnoreAllView => Layout, Scrollable, FocusNav, DynamicTree);

        let screen = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
        let normal_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Normal",
                Rect::new(2, 2, 20, 6),
                Box::new(IgnoreAllView),
            ),
            screen,
        );
        let modal_id = desktop.add_window(
            Window::new(
                WindowKind::Modal,
                "Modal",
                Rect::new(10, 8, 30, 8),
                Box::new(IgnoreAllView),
            ),
            screen,
        );

        assert!(!desktop.focus_window(normal_id));
        assert_eq!(desktop.wm.focused(), Some(modal_id));
        assert!(desktop.focus_window(modal_id));

        desktop.close_window(modal_id);
        desktop
            .wm
            .window_mut(normal_id)
            .expect("normal window")
            .state
            .set(WindowState::Minimized);
        assert!(!desktop.focus_window(normal_id));
    }

    #[test]
    fn set_property_applies_tree_op_to_dynamic_window() {
        let screen = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
        let root = ComponentSpec::new("Label")
            .with_id("message")
            .with_prop("text", ComponentValue::String("before".into()));
        let window_id = desktop
            .add_dynamic_window(
                WindowKind::Normal,
                "Dynamic",
                Rect::new(2, 2, 20, 6),
                root,
                CallbackRegistry::new(),
                screen,
            )
            .expect("dynamic window");

        desktop
            .set_property("message", "text", ComponentValue::String("after".into()))
            .expect("set property");

        let value = desktop
            .wm
            .window(window_id)
            .and_then(|w| w.view.get_property("text"));
        assert_eq!(value, Some(ComponentValue::String("after".into())));
    }
}
