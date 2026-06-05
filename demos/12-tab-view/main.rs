// Demo: 12-tab-view
// 演示 TabView：页签切换、动态增删、程序化选中、头部位置切换。

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use atto_ui::app::{
    CrosstermAppConfig, CursorMode, Desktop, MenuBar, run_crossterm_desktop_simple,
};
use atto_ui::composable::{
    Checkbox, ComponentContext, EventHandling, EventResult, ListBox, TabHeaderPosition, TabView,
    Text, TextBox, VStack,
};
use atto_ui::reactive::Binding;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_macros::{ComponentProperties, component_properties};

const INFO_HEIGHT: u16 = 5;

fn build_form_tab() -> VStack {
    let name = Binding::new("".to_string());
    let note = Binding::new("".to_string());
    let agree = Binding::new(false);

    VStack::new()
        .spacing(1)
        .padding(1)
        .child(Text::new("表单示例："))
        .child(TextBox::new("姓名", name))
        .child(TextBox::new("备注", note))
        .child(Checkbox::new("我已阅读并同意", agree))
}

fn build_list_tab() -> VStack {
    let items = vec![
        "Rust".to_string(),
        "TypeScript".to_string(),
        "Go".to_string(),
        "Swift".to_string(),
        "Zig".to_string(),
    ];

    VStack::new()
        .spacing(1)
        .padding(1)
        .child(Text::new("列表示例："))
        .child(ListBox::new("语言", items, Binding::new(0usize)).height(6))
}

fn build_info_tab(title: String) -> VStack {
    VStack::new()
        .spacing(1)
        .padding(1)
        .child(Text::new(title))
        .child(Text::new("每个 Tab 都是独立容器，可放任意子组件。"))
}

#[derive(ComponentProperties)]
struct TabDemoView {
    tab_view: TabView,
    selection: Binding<usize>,
    header_position: Binding<TabHeaderPosition>,
    #[component(skip)]
    next_tab_index: usize,
    #[component(skip)]
    last_area: Option<Rect>,
}

impl TabDemoView {
    fn new() -> Self {
        let selection = Binding::new(0usize);
        let header_position = Binding::new(TabHeaderPosition::Top);

        let mut tab_view = TabView::new()
            .selection(selection.clone())
            .header_position(header_position.clone());

        tab_view.add_tab("Tab0", build_form_tab());
        tab_view.add_tab("Tab1", build_list_tab());
        tab_view.add_tab("Tab2", build_info_tab("这是 Tab2（当前高亮）".to_string()));
        tab_view.add_tab("Tab3", build_info_tab("这是 Tab3".to_string()));
        tab_view.set_selected(2);

        Self {
            tab_view,
            selection,
            header_position,
            next_tab_index: 4,
            last_area: None,
        }
    }

    fn layout_areas(&self, area: Rect) -> (Rect, Rect) {
        if area.height == 0 {
            return (Rect::default(), Rect::default());
        }
        let info_height = INFO_HEIGHT.min(area.height);
        let info_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: info_height,
        };
        let tab_area = if area.height > info_height {
            Rect {
                x: area.x,
                y: area.y + info_height,
                width: area.width,
                height: area.height.saturating_sub(info_height),
            }
        } else {
            Rect::default()
        };
        (info_area, tab_area)
    }

    fn add_tab(&mut self) {
        let index = self.next_tab_index;
        self.next_tab_index = self.next_tab_index.saturating_add(1);
        let title = format!("Tab{index}");
        let content = build_info_tab(format!("动态创建的标签页：{title}"));
        self.tab_view.add_tab(title, content);
        let new_idx = self.tab_view.len().saturating_sub(1);
        self.tab_view.set_selected(new_idx);
    }

    fn remove_active_tab(&mut self) -> bool {
        let Some(selected) = self.tab_view.selected() else {
            return false;
        };
        self.tab_view.remove_tab(selected)
    }

    fn select_next(&mut self) -> bool {
        let len = self.tab_view.len();
        if len == 0 {
            return false;
        }
        let current = self.selection.get();
        let next = (current + 1) % len;
        self.tab_view.set_selected(next);
        true
    }

    fn select_prev(&mut self) -> bool {
        let len = self.tab_view.len();
        if len == 0 {
            return false;
        }
        let current = self.selection.get();
        let prev = if current == 0 { len - 1 } else { current - 1 };
        self.tab_view.set_selected(prev);
        true
    }

    fn select_index(&mut self, idx: usize) -> bool {
        if idx >= self.tab_view.len() {
            return false;
        }
        self.tab_view.set_selected(idx);
        true
    }

    fn toggle_header_position(&mut self) {
        let next = match self.header_position.get() {
            TabHeaderPosition::Top => TabHeaderPosition::Bottom,
            TabHeaderPosition::Bottom => TabHeaderPosition::Top,
        };
        self.header_position.set(next);
    }

    fn handle_shortcuts(&mut self, key: KeyEvent) -> Option<EventResult> {
        if !key.modifiers.contains(KeyModifiers::CONTROL) {
            return None;
        }

        match key.code {
            KeyCode::Char('t') => {
                self.add_tab();
                Some(EventResult::changed())
            }
            KeyCode::Char('d') => {
                if self.remove_active_tab() {
                    Some(EventResult::changed())
                } else {
                    Some(EventResult::ignored())
                }
            }
            KeyCode::Char('h') => {
                self.toggle_header_position();
                Some(EventResult::changed())
            }
            KeyCode::Left | KeyCode::Char('p') => {
                if self.select_prev() {
                    Some(EventResult::changed())
                } else {
                    Some(EventResult::ignored())
                }
            }
            KeyCode::Right | KeyCode::Char('n') => {
                if self.select_next() {
                    Some(EventResult::changed())
                } else {
                    Some(EventResult::ignored())
                }
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let idx = c.to_digit(10).unwrap_or(0) as usize;
                if idx == 0 {
                    return Some(EventResult::ignored());
                }
                if self.select_index(idx - 1) {
                    Some(EventResult::changed())
                } else {
                    Some(EventResult::ignored())
                }
            }
            _ => Some(EventResult::ignored()),
        }
    }

    fn forward_mouse(
        &mut self,
        area: Rect,
        event: MouseEvent,
        ctx: ComponentContext<'_>,
    ) -> EventResult {
        let (_, tab_area) = self.layout_areas(area);
        if tab_area.width == 0 || tab_area.height == 0 {
            return EventResult::ignored();
        }

        let local_x = event.column.saturating_sub(tab_area.x);
        let local_y = event.row.saturating_sub(tab_area.y);
        if local_x >= tab_area.width || local_y >= tab_area.height {
            return EventResult::ignored();
        }

        let child_event = Event::Mouse(MouseEvent {
            column: local_x,
            row: local_y,
            ..event
        });

        self.tab_view.handle_event(
            &child_event,
            ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: ctx.is_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
            },
        )
    }
}

#[component_properties]
impl ::atto_ui::composable::Component for TabDemoView {
    fn draw(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        let (info_area, tab_area) = self.layout_areas(area);

        let status = format!(
            "当前 Tab: {} / 总数: {} / 头部: {:?}",
            self.tab_view.selected().unwrap_or(0),
            self.tab_view.len(),
            self.header_position.get()
        );

        let lines = vec![
            Line::styled("TabView Demo", ctx.theme.widget.focused),
            Line::styled(
                "Ctrl+T 新增 | Ctrl+D 删除 | Ctrl+←/→ 切换 | Ctrl+1..9 选中",
                ctx.theme.widget.dim,
            ),
            Line::styled("Ctrl+H 切换头部位置（上/下）", ctx.theme.widget.dim),
            Line::styled(status, ctx.theme.widget.normal),
            Line::from(vec![
                Span::styled("提示: ", ctx.theme.widget.dim),
                Span::styled("点击标题切换 / 点击内容区交互", ctx.theme.widget.normal),
            ]),
        ];

        let info = Paragraph::new(lines).style(ctx.theme.window_bg);
        frame.render_widget(info, info_area);

        if tab_area.width > 0 && tab_area.height > 0 {
            self.tab_view.draw(
                frame,
                tab_area,
                ComponentContext {
                    theme: ctx.theme,
                    window_id: ctx.window_id,
                    is_focused: ctx.is_focused,
                    scrollbar_host: ctx.scrollbar_host.for_child(),
                    tab_mode: ctx.tab_mode.for_child(),
                },
            );
        }
    }
}

impl ::atto_ui::composable::Layout for TabDemoView {
    fn min_width(&self) -> u16 {
        self.tab_view.min_width()
    }

    fn min_height(&self) -> u16 {
        INFO_HEIGHT.saturating_add(self.tab_view.min_height())
    }

    fn desired_width(&self) -> Option<u16> {
        self.tab_view.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.tab_view
            .desired_height()
            .map(|h| h.saturating_add(INFO_HEIGHT))
    }
}

impl ::atto_ui::composable::Scrollable for TabDemoView {}

impl ::atto_ui::composable::FocusNav for TabDemoView {
    fn is_focusable(&self) -> bool {
        self.tab_view.is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        self.tab_view.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.tab_view.focus_last()
    }
}

impl ::atto_ui::composable::DynamicTree for TabDemoView {}

impl ::atto_ui::composable::EventHandling for TabDemoView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Some(area) = self.last_area else {
            return EventResult::ignored();
        };

        if let Event::Key(key) = event
            && let Some(result) = self.handle_shortcuts(*key)
            && result.is_consumed()
        {
            return result;
        }

        if let Event::Mouse(m) = event {
            return self.forward_mouse(area, *m, ctx);
        }

        self.tab_view.handle_event(
            event,
            ComponentContext {
                theme: ctx.theme,
                window_id: ctx.window_id,
                is_focused: ctx.is_focused,
                scrollbar_host: ctx.scrollbar_host.for_child(),
                tab_mode: ctx.tab_mode.for_child(),
            },
        )
    }
}

fn main() -> Result<()> {
    let config = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(true)
        .cursor(CursorMode::Hide);

    run_crossterm_desktop_simple(config, |screen| {
        let theme = Theme::dark();
        let menu = MenuBar::new(vec![]);
        let mut desktop = Desktop::new(theme, menu);

        let window = Window::new(
            WindowKind::Normal,
            "TabView Demo",
            Rect {
                x: 6,
                y: 3,
                width: 84,
                height: 28,
            },
            Box::new(TabDemoView::new()),
        );
        desktop.add_window(window, screen);

        Ok(desktop)
    })
}
