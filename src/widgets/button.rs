use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::composable::{
    Component, ComponentContext, EventHandling, EventResult, FocusNav, Layout,
};
use crate::reactive::Binding;
use crate::runtime::CallbackHandle;
use crate::theme::Theme;
use atto_ui_macros::{ComponentProperties, component_properties};

use super::util::mouse_coords_local_to_area;

const BUTTON_HORIZONTAL_PADDING: u16 = 2;

#[derive(Clone, ComponentProperties)]
pub struct Button {
    label: Binding<String>,
    #[component(rename = "default")]
    default_button: Binding<bool>,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    on_click_callback: Option<CallbackHandle>,
    enabled: Binding<bool>,
    last_area: Option<Rect>,
}

impl Button {
    pub fn new(label: impl Into<Binding<String>>) -> Self {
        Self {
            label: label.into(),
            default_button: false.into(),
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

    pub fn default_button(mut self, default_button: impl Into<Binding<bool>>) -> Self {
        self.default_button = default_button.into();
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
        if area.width == 0 || area.height == 0 {
            return;
        }

        let enabled = self.enabled.get();
        let style = button_style(
            ctx.theme,
            enabled,
            ctx.is_focused,
            self.default_button.get(),
        );
        let shadow_style = button_shadow_style(ctx.theme);
        let button_rect = Rect::new(area.x, button_row(area), area.width, 1);
        let bounds = frame.area();
        let buf = frame.buffer_mut();

        draw_button_shadow(buf, button_rect, bounds, shadow_style);
        fill_button_row(buf, button_rect, bounds, style);
        draw_button_label(buf, button_rect, bounds, &self.label.get(), style);
    }
}

impl Layout for Button {
    fn min_width(&self) -> u16 {
        3
    }

    fn min_height(&self) -> u16 {
        1
    }

    fn desired_height(&self) -> Option<u16> {
        Some(1)
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

fn button_row(area: Rect) -> u16 {
    area.y.saturating_add(area.height.saturating_sub(1) / 2)
}

fn button_style(theme: &Theme, enabled: bool, focused: bool, default_button: bool) -> Style {
    if !enabled {
        return theme
            .named_style("button-disabled")
            .unwrap_or_else(|| theme.menu_bar.patch(theme.widget.disabled));
    }

    if focused {
        return theme
            .named_style("button-focused")
            .or_else(|| theme.named_style("button-default"))
            .unwrap_or(theme.selection);
    }

    if default_button {
        return theme
            .named_style("button-default")
            .or_else(|| theme.named_style("button-focused"))
            .unwrap_or(theme.selection);
    }

    theme
        .named_style("button")
        .unwrap_or_else(|| theme.menu_bar.patch(theme.widget.normal))
}

fn button_shadow_style(theme: &Theme) -> Style {
    theme
        .named_style("button-shadow")
        .unwrap_or(theme.window_shadow)
}

fn fill_button_row(buf: &mut Buffer, rect: Rect, bounds: Rect, style: Style) {
    if rect.width == 0 || !contains_y(bounds, rect.y) {
        return;
    }

    let style = reset_style(style);
    let end_x = rect.x.saturating_add(rect.width).min(bounds_right(bounds));
    for x in rect.x.max(bounds.x)..end_x {
        if let Some(cell) = buf.cell_mut((x, rect.y)) {
            cell.set_symbol(" ");
            cell.set_style(style);
        }
    }
}

fn draw_button_label(buf: &mut Buffer, rect: Rect, bounds: Rect, label: &str, style: Style) {
    if rect.width == 0 || !contains_y(bounds, rect.y) {
        return;
    }

    let max_label_width = if rect.width > BUTTON_HORIZONTAL_PADDING.saturating_mul(2) {
        rect.width
            .saturating_sub(BUTTON_HORIZONTAL_PADDING.saturating_mul(2))
    } else {
        rect.width
    };
    let label = truncate_display_width(label, usize::from(max_label_width));
    let label_width = display_width(&label).min(rect.width);
    let x = rect
        .x
        .saturating_add(rect.width.saturating_sub(label_width) / 2);
    let max_width = rect
        .x
        .saturating_add(rect.width)
        .min(bounds_right(bounds))
        .saturating_sub(x);

    if max_width > 0 {
        buf.set_stringn(x, rect.y, label, usize::from(max_width), reset_style(style));
    }
}

fn draw_button_shadow(buf: &mut Buffer, rect: Rect, bounds: Rect, style: Style) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    let style = reset_style(style);
    let right_x = rect.x.saturating_add(rect.width);
    let bottom_y = rect.y.saturating_add(rect.height);

    if right_x < bounds_right(bounds)
        && contains_y(bounds, rect.y)
        && let Some(cell) = buf.cell_mut((right_x, rect.y))
    {
        cell.set_symbol(" ");
        cell.set_style(style);
    }

    if !contains_y(bounds, bottom_y) {
        return;
    }

    let start_x = rect.x.saturating_add(1).max(bounds.x);
    let end_x = right_x.saturating_add(1).min(bounds_right(bounds));
    for x in start_x..end_x {
        if let Some(cell) = buf.cell_mut((x, bottom_y)) {
            cell.set_symbol(" ");
            cell.set_style(style);
        }
    }
}

fn truncate_display_width(text: &str, max_width: usize) -> String {
    let mut out = String::new();
    let mut width = 0usize;
    for grapheme in text.graphemes(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if grapheme_width == 0 {
            out.push_str(grapheme);
            continue;
        }
        if width.saturating_add(grapheme_width) > max_width {
            break;
        }
        out.push_str(grapheme);
        width = width.saturating_add(grapheme_width);
    }
    out
}

fn display_width(text: &str) -> u16 {
    UnicodeWidthStr::width(text).min(usize::from(u16::MAX)) as u16
}

fn contains_y(rect: Rect, y: u16) -> bool {
    y >= rect.y && y < bounds_bottom(rect)
}

fn bounds_right(rect: Rect) -> u16 {
    rect.x.saturating_add(rect.width)
}

fn bounds_bottom(rect: Rect) -> u16 {
    rect.y.saturating_add(rect.height)
}

fn reset_style(style: Style) -> Style {
    Style::reset().patch(style)
}

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
        context_with_focus(theme, true)
    }

    fn context_with_focus(theme: &Theme, focused: bool) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: focused,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        }
    }

    fn region_symbols(terminal: &Terminal<TestBackend>, area: Rect) -> String {
        let buf = terminal.backend().buffer();
        let mut out = String::new();
        for y in area.y..area.y.saturating_add(area.height) {
            for x in area.x..area.x.saturating_add(area.width) {
                out.push_str(buf[(x, y)].symbol());
            }
        }
        out
    }

    fn assert_visible_style(actual: Style, expected: Style) {
        assert_eq!(actual.fg, expected.fg);
        assert_eq!(actual.bg, expected.bg);
        assert_eq!(actual.add_modifier, expected.add_modifier);
    }

    #[test]
    fn draw_uses_flat_single_row_with_shadow_instead_of_border() {
        let mut button = Button::new("OK");
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(10, 5)).expect("terminal");
        terminal
            .draw(|f| button.draw(f, Rect::new(1, 1, 6, 1), context_with_focus(&theme, false)))
            .expect("draw");

        let buf = terminal.backend().buffer();
        let button_style = reset_style(theme.named_style("button").expect("button style"));
        let shadow_style = reset_style(
            theme
                .named_style("button-shadow")
                .expect("button shadow style"),
        );

        assert_eq!(buf[(3, 1)].symbol(), "O");
        assert_eq!(buf[(4, 1)].symbol(), "K");
        for x in 1..7 {
            assert_visible_style(buf[(x, 1)].style(), button_style);
        }
        assert_visible_style(buf[(7, 1)].style(), shadow_style);
        for x in 2..8 {
            assert_visible_style(buf[(x, 2)].style(), shadow_style);
        }

        let drawn = region_symbols(&terminal, Rect::new(1, 1, 7, 2));
        for border in ['┌', '─', '┐', '│', '└', '┘'] {
            assert!(
                !drawn.contains(border),
                "button should be borderless, got {drawn:?}"
            );
        }
    }

    #[test]
    fn layout_reports_single_row_height() {
        let button = Button::new("OK");

        assert_eq!(button.min_height(), 1);
        assert_eq!(button.desired_height(), Some(1));
    }

    #[test]
    fn default_button_uses_emphasis_without_focus() {
        let mut button = Button::new("OK").default_button(true);
        let theme = Theme::dark();
        let mut terminal = Terminal::new(TestBackend::new(10, 3)).expect("terminal");
        terminal
            .draw(|f| button.draw(f, Rect::new(1, 0, 6, 1), context_with_focus(&theme, false)))
            .expect("draw");

        let emphasized_style = reset_style(
            theme
                .named_style("button-default")
                .expect("button default style"),
        );
        let buf = terminal.backend().buffer();
        assert_eq!(buf[(3, 0)].symbol(), "O");
        assert_visible_style(buf[(3, 0)].style(), emphasized_style);
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
            .draw(|f| button.draw(f, Rect::new(10, 5, 6, 1), context(&theme)))
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
            row: 5,
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
            .draw(|f| button.draw(f, Rect::new(2, 2, 6, 1), context(&theme)))
            .expect("draw");

        let key = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            button.handle_event(&key, context(&theme)),
            EventResult::ignored()
        );

        let mouse = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 2,
            modifiers: KeyModifiers::empty(),
        });
        assert_eq!(
            button.handle_event(&mouse, context(&theme)),
            EventResult::ignored()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
