// Demo: 10-file-dialog
// 演示 FileDialog（Open/Save）的基本用法。

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use ratatui::layout::Rect;

use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, run_crossterm_desktop,
};
use atto_ui::composable::{Component, LayoutParams, Size, TextFn, VStack};
use atto_ui::dialogs::FileDialog;
use atto_ui::reactive::{EventQueue, Property};
use atto_ui::theme::Theme;
use atto_ui::widgets::Button;
use atto_ui::wm::{Window, WindowKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DemoAction {
    OpenDialog,
    SaveDialog,
    Clear,
}

#[derive(Clone)]
struct DemoModel {
    actions: EventQueue<DemoAction>,
    last_open: Property<Option<PathBuf>>,
    last_save: Property<Option<PathBuf>>,
}

impl DemoModel {
    fn new(actions: EventQueue<DemoAction>) -> Self {
        Self {
            actions,
            last_open: Property::new(None),
            last_save: Property::new(None),
        }
    }
}

fn build_main_view(model: DemoModel) -> Box<dyn Component> {
    let actions = model.actions.clone();
    let last_open = model.last_open.clone();
    let last_save = model.last_save.clone();

    let open_button = Button::new("Open File...").on_click({
        let actions = actions.clone();
        move || actions.push(DemoAction::OpenDialog)
    });
    let save_button = Button::new("Save File...").on_click({
        let actions = actions.clone();
        move || actions.push(DemoAction::SaveDialog)
    });
    let clear_button = Button::new("Clear").on_click(move || actions.push(DemoAction::Clear));

    let root = VStack::new()
        .spacing(1)
        .padding(1)
        .child_with_layout(open_button, LayoutParams::default())
        .child_with_layout(save_button, LayoutParams::default())
        .child_with_layout(clear_button, LayoutParams::default())
        .child_with_layout(
            TextFn::new(move || match last_open.get() {
                Some(p) => format!("Last open: {}", p.display()),
                None => "Last open: <none>".to_string(),
            }),
            LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            TextFn::new(move || match last_save.get() {
                Some(p) => format!("Last save: {}", p.display()),
                None => "Last save: <none>".to_string(),
            }),
            LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            },
        );

    Box::new(root)
}

fn centered_rect(work: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(work.width.saturating_sub(2)).max(20);
    let h = height.min(work.height.saturating_sub(2)).max(8);
    Rect {
        x: work.x + (work.width.saturating_sub(w)) / 2,
        y: work.y + (work.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

fn main() -> Result<()> {
    let config = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .cursor(CursorMode::Show)
        .bracketed_paste(true);

    let actions: EventQueue<DemoAction> = EventQueue::new();
    let model = DemoModel::new(actions.clone());

    run_crossterm_desktop(
        config,
        {
            let model = model.clone();
            move |screen| {
                let menu = MenuBar::new(vec![]);
                let mut desktop = Desktop::new(Theme::dark(), menu);

                let work = Desktop::layout(screen).work_area;
                let rect = Rect {
                    x: work.x + 2,
                    y: work.y + 1,
                    width: work.width.saturating_sub(4).max(30),
                    height: work.height.saturating_sub(2).max(12),
                };
                desktop.add_window(
                    Window::new(
                        WindowKind::Normal,
                        "FileDialog Demo",
                        rect,
                        build_main_view(model),
                    ),
                    screen,
                );

                Ok(desktop)
            }
        },
        {
            let model = model.clone();
            move |desktop, screen| {
                for action in model.actions.drain() {
                    match action {
                        DemoAction::Clear => {
                            model.last_open.set(None);
                            model.last_save.set(None);
                        }
                        DemoAction::OpenDialog => {
                            if desktop.wm.has_active_modal() {
                                continue;
                            }
                            model.last_open.set(None);
                            let work = Desktop::layout(screen).work_area;
                            let rect = centered_rect(work, 72, 22);
                            desktop.add_window(
                                Window::new(
                                    WindowKind::Modal,
                                    "Open File",
                                    rect,
                                    Box::new(FileDialog::open_file(model.last_open.binding())),
                                ),
                                screen,
                            );
                        }
                        DemoAction::SaveDialog => {
                            if desktop.wm.has_active_modal() {
                                continue;
                            }
                            model.last_save.set(None);
                            let work = Desktop::layout(screen).work_area;
                            let rect = centered_rect(work, 72, 22);
                            desktop.add_window(
                                Window::new(
                                    WindowKind::Modal,
                                    "Save File",
                                    rect,
                                    Box::new(
                                        FileDialog::save_file(model.last_save.binding())
                                            .initial_file_name("output.txt"),
                                    ),
                                ),
                                screen,
                            );
                        }
                    }
                }

                Ok(AppControl::Continue)
            }
        },
        |_desktop, _event, _screen, _result| Ok(AppControl::Continue),
    )
}
