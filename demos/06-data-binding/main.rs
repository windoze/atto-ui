// Demo: 06-data-binding
// 演示 Atto-UI 的反应式数据绑定：Property / Binding + 双向同步 + 禁用状态。

use std::time::Duration;

use anyhow::Result;
use ratatui::layout::Rect;

use atto_ui::app::{
    CrosstermAppConfig, CursorMode, Desktop, MenuBar, run_crossterm_desktop_simple,
};
use atto_ui::composable::{
    Button, Checkbox, Component, Divider, EdgeInsets, HStack, Label, LayoutParams, RadioGroup,
    Size, Spacer, Text, TextBox, TextFn, VStack,
};
use atto_ui::reactive::Property;
use atto_ui::theme::Theme;
use atto_ui::wm::{Window, WindowKind};

fn content_height() -> LayoutParams {
    LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    }
}

#[derive(Clone)]
struct AppModel {
    editor_enabled: Property<bool>,
    name: Property<String>,
    email: Property<String>,
    notes: Property<String>,
    subscribed: Property<bool>,
    role: Property<usize>,
    counter: Property<u32>,
    status: Property<String>,
    sample_idx: Property<usize>,
}

impl AppModel {
    fn new() -> Self {
        Self {
            editor_enabled: Property::new(true),
            name: Property::new("Alice".to_string()),
            email: Property::new("alice@example.com".to_string()),
            notes: Property::new(String::new()),
            subscribed: Property::new(true),
            role: Property::new(0),
            counter: Property::new(0),
            status: Property::new("Ready. Try editing fields or click 'Load sample'.".to_string()),
            sample_idx: Property::new(0),
        }
    }

    fn load_sample(&self) {
        const SAMPLES: &[(&str, &str, bool, usize)] = &[
            ("Alice", "alice@example.com", true, 0),
            ("Bob", "bob@example.com", false, 1),
            ("Chen", "chen@example.com", true, 2),
            ("Dora", "dora@example.com", true, 0),
        ];

        let idx = self.sample_idx.get() % SAMPLES.len();
        let (name, email, subscribed, role) = SAMPLES[idx];

        self.name.set(name.to_string());
        self.email.set(email.to_string());
        self.subscribed.set(subscribed);
        self.role.set(role);
        self.status
            .set(format!("Loaded sample #{idx}: {name} ({email})"));

        self.sample_idx.update(|i| *i = i.saturating_add(1));
    }

    fn clear(&self) {
        self.name.set(String::new());
        self.email.set(String::new());
        self.notes.set(String::new());
        self.subscribed.set(false);
        self.role.set(0);
        self.status.set("Cleared fields.".to_string());
    }
}

fn build_editor_view(model: AppModel) -> Box<dyn Component> {
    let editor_enabled = model.editor_enabled.clone();

    let buttons = {
        let enabled = editor_enabled.binding();
        let model_load = model.clone();
        let model_clear = model.clone();
        let model_count = model.clone();

        HStack::new()
            .spacing(1)
            .child(
                Button::new("Load sample")
                    .enabled(enabled.clone())
                    .on_click(move || model_load.load_sample()),
            )
            .child(
                Button::new("Clear")
                    .enabled(enabled.clone())
                    .on_click(move || model_clear.clear()),
            )
            .child(Spacer::new())
            .child(
                Button::new("Count +1")
                    .enabled(enabled.clone())
                    .on_click(move || {
                        model_count.counter.update(|c| *c = c.saturating_add(1));
                        model_count
                            .status
                            .set(format!("Counter = {}", model_count.counter.get()));
                    }),
            )
    };

    let status_line = {
        let status = model.status.clone();
        TextFn::new(move || format!("Status: {}", status.get()))
    };

    let root = VStack::new()
        .spacing(1)
        .padding(1)
        .child_with_layout(Text::new("Data Binding Demo (Editor)"), content_height())
        .child_with_layout(
            Text::new("Tip: 'q' quits only when the focused widget did not consume the key; Ctrl+Q always quits."),
            content_height(),
        )
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(
            Checkbox::new(
                "Enable editor (disables inputs/buttons below)",
                model.editor_enabled.binding(),
            ),
            content_height(),
        )
        .child_with_layout(
            VStack::new()
                .spacing(1)
                .child(TextBox::new("Name", model.name.binding()).enabled(
                    editor_enabled.binding(),
                ))
                .child(TextBox::new("Email", model.email.binding()).enabled(
                    editor_enabled.binding(),
                ))
                .child(Checkbox::new("Subscribed", model.subscribed.binding()).enabled(
                    editor_enabled.binding(),
                ))
                .child(
                    RadioGroup::new(
                        "Role",
                        vec!["User".into(), "Admin".into(), "Guest".into()],
                        model.role.binding(),
                    )
                    .enabled(editor_enabled.binding()),
                )
                .child(TextBox::new("Notes (single-line)", model.notes.binding()).enabled(
                    editor_enabled.binding(),
                )),
            LayoutParams {
                height: Size::Fill,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(
            buttons,
            LayoutParams {
                height: Size::Fixed(3),
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            status_line,
            LayoutParams {
                height: Size::Fixed(1),
                ..LayoutParams::default()
            },
        );

    Box::new(root)
}

fn build_mirror_view(model: AppModel) -> Box<dyn Component> {
    let name = model.name.clone();
    let email = model.email.clone();
    let notes = model.notes.clone();
    let subscribed = model.subscribed.clone();
    let role = model.role.clone();
    let counter = model.counter.clone();

    let summary = TextFn::new(move || {
        let role_label = match role.get() {
            0 => "User",
            1 => "Admin",
            2 => "Guest",
            _ => "Unknown",
        };
        format!(
            "Summary: name=\"{}\"  email=\"{}\"  notes=\"{}\"  subscribed={}  role={}  counter={}",
            name.get(),
            email.get(),
            notes.get(),
            subscribed.get(),
            role_label,
            counter.get(),
        )
    });

    let root = VStack::new()
        .spacing(1)
        .padding_insets(EdgeInsets::all(1))
        .child_with_layout(Text::new("Data Binding Demo (Mirror)"), content_height())
        .child_with_layout(
            Text::new("These widgets share the same bindings as the Editor window."),
            content_height(),
        )
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(Label::new("Try editing on either side:"), content_height())
        .child(TextBox::new("Name (mirror)", model.name.binding()))
        .child(Checkbox::new(
            "Subscribed (mirror)",
            model.subscribed.binding(),
        ))
        .child_with_layout(Divider::horizontal(), content_height())
        .child_with_layout(
            summary,
            LayoutParams {
                height: Size::Fixed(1),
                ..LayoutParams::default()
            },
        );

    Box::new(root)
}

fn main() -> Result<()> {
    let config = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(50))
        .mouse_capture(true)
        .bracketed_paste(true)
        .cursor(CursorMode::Show);

    run_crossterm_desktop_simple(config, |screen| {
        let model = AppModel::new();

        let menu = MenuBar::new(vec![]);
        let mut desktop = Desktop::new(Theme::dark(), menu);

        let work = Desktop::layout(screen).work_area;
        let gutter = 2;
        let half = work.width / 2;

        let left = Rect {
            x: work.x.saturating_add(gutter),
            y: work.y.saturating_add(1),
            width: half.saturating_sub(gutter.saturating_add(1)).max(20),
            height: work.height.saturating_sub(2).max(10),
        };
        let right = Rect {
            x: work.x.saturating_add(half).saturating_add(1),
            y: work.y.saturating_add(1),
            width: work
                .width
                .saturating_sub(half)
                .saturating_sub(gutter.saturating_add(1))
                .max(20),
            height: work.height.saturating_sub(2).max(10),
        };

        let editor_id = desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Data Binding - Editor",
                left,
                build_editor_view(model.clone()),
            ),
            screen,
        );
        desktop.add_window(
            Window::new(
                WindowKind::Normal,
                "Data Binding - Mirror",
                right,
                build_mirror_view(model.clone()),
            ),
            screen,
        );
        desktop.wm.focus(editor_id);

        Ok(desktop)
    })
}
