//! Window placement and startup helpers: editor-window addition, open-folder
//! modal UI, centered rect / default editor rect, and initial-path splitting.

use super::*;

pub(crate) fn add_editor_window(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    actions: EventQueue<AppAction>,
    editor_theme: atto_ui::reactive::Binding<atto_ui_editor::EditorThemeSet>,
    clipboard: atto_ui::reactive::Binding<String>,
    initial_files: Vec<PathBuf>,
) -> EventQueue<EditorWindowCommand> {
    let work = Desktop::layout(screen).work_area;
    let offset = state.lock().next_window_offset;
    state.lock().next_window_offset = offset.saturating_add(2);

    let rect = default_editor_rect(work, offset);
    let commands = EventQueue::<EditorWindowCommand>::new();
    let editor_events = EventQueue::<atto_ui_editor::EditorEvent>::new();
    let diagnostics_summary: Binding<atto_ui_editor::DiagnosticsSummary> =
        atto_ui_editor::DiagnosticsSummary::default().into();
    let editor_status: Binding<EditorStatus> = EditorStatus::default().into();
    let tab_summaries: Binding<Vec<EditorTabSummary>> = Vec::new().into();
    let workspace_state = state.lock().workspace_state.clone();
    let view = EditorWindowView::new_with_workspace_bindings(
        actions,
        commands.clone(),
        editor_events.clone(),
        editor_theme,
        clipboard,
        workspace_state,
        EditorWindowBindings::new(
            diagnostics_summary.clone(),
            editor_status.clone(),
            tab_summaries.clone(),
        ),
    );
    let id = desktop.add_window(
        Window::new(WindowKind::Normal, "Atto Editor", rect, Box::new(view))
            .with_tag("atto-editor-app")
            .with_close_hook({
                let state = state.clone();
                move |id| {
                    remove_editor_window_state(&state, id);
                    true
                }
            }),
        screen,
    );
    {
        let mut s = state.lock();
        s.editor_windows.insert(id, commands.clone());
        s.editor_events.insert(id, editor_events);
        s.editor_diagnostics.insert(id, diagnostics_summary);
        s.editor_statuses.insert(id, editor_status);
        s.editor_tab_summaries.insert(id, tab_summaries);
        s.last_focused_editor = Some(id);
    }

    for file in initial_files {
        commands.push(EditorWindowCommand::OpenFile(file));
    }

    commands
}

pub(crate) fn build_open_folder_view(
    input: Property<String>,
    actions: EventQueue<AppAction>,
) -> Box<dyn Component> {
    let input_binding = input.binding();

    let open_button = Button::new("Open").on_click({
        let actions = actions.clone();
        move || actions.push(AppAction::SubmitOpenFolderDialog)
    });
    let cancel_button =
        Button::new("Cancel").on_click(move || actions.push(AppAction::CancelOpenFolderDialog));

    let help = TextFn::new(|| "Tip: paste a folder path and press Open.".to_string());

    let layout = VStack::new()
        .spacing(1)
        .padding(1)
        .child_with_layout(
            TextBox::new("Folder", input_binding),
            LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            help,
            LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            },
        )
        .child_with_layout(
            HStack::new()
                .spacing(1)
                .child(open_button)
                .child(cancel_button),
            LayoutParams {
                height: Size::Content,
                ..LayoutParams::default()
            },
        );

    Box::new(layout)
}

pub(crate) fn centered_rect(work: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(work.width.saturating_sub(2)).max(20);
    let h = height.min(work.height.saturating_sub(2)).max(8);
    Rect {
        x: work.x + (work.width.saturating_sub(w)) / 2,
        y: work.y + (work.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

pub(crate) fn default_editor_rect(work: Rect, offset: u16) -> Rect {
    let w = work.width.saturating_sub(6).max(40);
    let h = work.height.saturating_sub(4).max(12);
    Rect {
        x: work
            .x
            .saturating_add(3)
            .saturating_add(offset)
            .min(work.x + work.width.saturating_sub(1)),
        y: work
            .y
            .saturating_add(2)
            .saturating_add(offset)
            .min(work.y + work.height.saturating_sub(1)),
        width: w.min(work.width.saturating_sub(2)),
        height: h.min(work.height.saturating_sub(2)),
    }
}

pub(crate) fn split_initial_paths(paths: &[PathBuf]) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut roots = Vec::<PathBuf>::new();
    let mut files = Vec::<PathBuf>::new();

    if paths.is_empty() {
        roots.push(env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        return (roots, files);
    }

    for p in paths {
        if p.is_dir() {
            roots.push(p.clone());
        } else if p.is_file() {
            files.push(p.clone());
        }
    }

    if roots.is_empty() {
        if let Some(parent) = files
            .first()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
        {
            roots.push(parent);
        } else {
            roots.push(env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        }
    }

    (roots, files)
}

pub(crate) fn parent_dir_or_cwd(path: &Path) -> PathBuf {
    path.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}
