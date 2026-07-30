//! File picker: event processing, focus restore, open, background indexing,
//! and item listing from the workspace file index.

use super::super::*;

pub(crate) fn process_file_picker_events(
    desktop: &mut Desktop,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
) {
    let events = state.lock().file_picker_events.clone();
    for event in events.drain() {
        match event {
            PickerEvent::Accepted(action) => {
                restore_file_picker_focus(desktop, state);
                actions.push(action);
            }
            PickerEvent::Submitted(_) => restore_file_picker_focus(desktop, state),
            PickerEvent::Closed => restore_file_picker_focus(desktop, state),
        }
    }
}

pub(crate) fn restore_file_picker_focus(desktop: &mut Desktop, state: &Arc<Mutex<AppState>>) {
    let restore = {
        let mut s = state.lock();
        s.file_picker_window = None;
        s.file_picker_restore_focus.take()
    };

    if let Some(id) = restore
        && desktop.wm.window(id).is_some()
    {
        desktop.focus_window(id);
    }
}

pub(crate) fn open_file_picker(desktop: &mut Desktop, screen: Rect, state: &Arc<Mutex<AppState>>) {
    if let Some(id) = state.lock().file_picker_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().file_picker_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.file_picker_events.clone();
        let _ = events.drain();
        s.file_picker_restore_focus = desktop.wm.focused();
        events
    };
    let mut view = PickerView::new("File Picker", Vec::new(), events.clone())
        .placeholder("Type a file path")
        .max_results(300)
        .border(false);
    match file_picker_cached_items(state) {
        // Cache hit: fill synchronously so the list is ready immediately.
        Some(items) => view.set_items(items),
        // Cache miss: build the index on a background thread to keep the UI
        // responsive; items stream in once ready.
        None => view = view.items_source(spawn_file_picker_index(state)),
    }
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 82, 20);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "File Picker", rect, Box::new(view))
            .with_tag("atto-editor-app-file-picker")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.file_picker_window == Some(id) {
                        s.file_picker_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().file_picker_window = Some(id);
}

/// Returns picker items from the cache when it matches the current workspace
/// roots, otherwise `None` (a background build is needed).
pub(crate) fn file_picker_cached_items(
    state: &Arc<Mutex<AppState>>,
) -> Option<Vec<PickerItem<AppAction>>> {
    let s = state.lock();
    let roots = canonical_workspace_roots(&s.workspace_roots);
    s.file_picker_cache
        .as_ref()
        .filter(|cache| cache.roots == roots)
        .map(file_picker_items_from_index)
}

/// Builds the workspace file index on a background thread and returns a receiver
/// the picker polls for the resulting items.
pub(crate) fn spawn_file_picker_index(
    state: &Arc<Mutex<AppState>>,
) -> Receiver<Vec<PickerItem<AppAction>>> {
    let (tx, rx) = mpsc::channel();
    let state = state.clone();
    thread::spawn(move || {
        let items = file_picker_items(&state);
        let _ = tx.send(items);
    });
    rx
}

pub(crate) fn file_picker_items(state: &Arc<Mutex<AppState>>) -> Vec<PickerItem<AppAction>> {
    let roots = {
        let s = state.lock();
        let roots = canonical_workspace_roots(&s.workspace_roots);
        if let Some(cache) = &s.file_picker_cache
            && cache.roots == roots
        {
            return file_picker_items_from_index(cache);
        }
        roots
    };

    let index = build_workspace_file_index(&roots, MAX_FILE_PICKER_ENTRIES);
    let items = file_picker_items_from_index(&index);
    {
        let mut s = state.lock();
        if canonical_workspace_roots(&s.workspace_roots) == index.roots {
            s.file_picker_cache = Some(index);
        } else {
            s.file_picker_cache = None;
        }
    }
    items
}

pub(crate) fn canonical_workspace_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .map(|root| canonicalize_best_effort(root))
        .collect()
}

pub(crate) fn file_picker_items_from_index(
    index: &WorkspaceFileIndex,
) -> Vec<PickerItem<AppAction>> {
    index
        .entries
        .iter()
        .map(|entry| {
            PickerItem::new(
                entry.display_path.clone(),
                AppAction::OpenPath {
                    path: entry.path.clone(),
                    target: OpenTarget::NewTab,
                },
            )
            .subtitle(entry.path.to_string_lossy())
        })
        .collect()
}
