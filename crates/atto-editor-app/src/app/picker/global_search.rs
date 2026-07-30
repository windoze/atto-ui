//! Global-search picker: query picker, results picker, item building, and
//! display-path formatting.

use super::super::*;

pub(crate) fn process_global_search_picker_events(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    actions: &EventQueue<AppAction>,
) {
    let events = state.lock().global_search_picker_events.clone();
    let mut submitted_query = None;
    for event in events.drain() {
        match event {
            PickerEvent::Accepted(action) => {
                restore_global_search_picker_focus(desktop, state);
                actions.push(action);
            }
            PickerEvent::Submitted(query) => {
                restore_global_search_picker_focus(desktop, state);
                submitted_query = Some(query);
            }
            PickerEvent::Closed => restore_global_search_picker_focus(desktop, state),
        }
    }
    if let Some(query) = submitted_query {
        open_global_search_results_for_query(desktop, screen, state, query);
    }
}

pub(crate) fn restore_global_search_picker_focus(desktop: &mut Desktop, state: &Arc<Mutex<AppState>>) {
    let restore = {
        let mut s = state.lock();
        s.global_search_picker_window = None;
        s.global_search_picker_restore_focus.take()
    };

    if let Some(id) = restore
        && desktop.wm.window(id).is_some()
    {
        desktop.focus_window(id);
    }
}

pub(crate) fn open_global_search_query_picker(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
) {
    if let Some(id) = state.lock().global_search_picker_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().global_search_picker_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.global_search_picker_events.clone();
        let _ = events.drain();
        s.global_search_picker_restore_focus = desktop.wm.focused();
        events
    };
    let view = PickerView::<AppAction>::new("Global Search", Vec::new(), events.clone())
        .placeholder("Type search text and press Enter")
        .submit_query_on_empty(true);
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 82, 12);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "Global Search", rect, Box::new(view))
            .with_tag("atto-editor-app-global-search")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.global_search_picker_window == Some(id) {
                        s.global_search_picker_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().global_search_picker_window = Some(id);
}

pub(crate) fn open_global_search_results_for_query(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    query: String,
) {
    let roots = {
        let s = state.lock();
        canonical_workspace_roots(&s.workspace_roots)
    };
    if roots.is_empty() {
        set_status_message(state, "No workspace roots for global search");
        return;
    }

    let config = GlobalSearchConfig {
        max_total_matches: MAX_GLOBAL_SEARCH_RESULTS,
        ..GlobalSearchConfig::default()
    };
    match search_workspace(&roots, &query, SearchOptions::default(), config) {
        Ok(results) => {
            let count = results.len();
            open_global_search_results_picker(desktop, screen, state, &roots, results);
            set_status_message(state, format!("Global search found {count} result(s)"));
        }
        Err(err) => {
            set_status_message(state, format!("Global search failed: {err:#}"));
        }
    }
}

pub(crate) fn open_global_search_results_picker(
    desktop: &mut Desktop,
    screen: Rect,
    state: &Arc<Mutex<AppState>>,
    roots: &[PathBuf],
    results: Vec<GlobalSearchResult>,
) {
    if let Some(id) = state.lock().global_search_picker_window {
        if desktop.wm.window(id).is_some() {
            return;
        }
        state.lock().global_search_picker_window = None;
    }
    if desktop.wm.has_active_modal() {
        return;
    }

    let events = {
        let mut s = state.lock();
        let events = s.global_search_picker_events.clone();
        let _ = events.drain();
        s.global_search_picker_restore_focus = desktop.wm.focused();
        events
    };
    let view = PickerView::new(
        "Global Search",
        global_search_items(roots, &results),
        events.clone(),
    )
    .placeholder("Type to filter search results")
    .max_results(MAX_GLOBAL_SEARCH_RESULTS)
    .border(false);
    let work = Desktop::layout(screen).work_area;
    let rect = centered_rect(work, 90, 22);
    let id = desktop.add_window(
        Window::new(WindowKind::Modal, "Global Search", rect, Box::new(view))
            .with_tag("atto-editor-app-global-search")
            .with_close_hook({
                let state = state.clone();
                let events = events.clone();
                move |id| {
                    let mut s = state.lock();
                    if s.global_search_picker_window == Some(id) {
                        s.global_search_picker_window = None;
                    }
                    events.push(PickerEvent::Closed);
                    true
                }
            }),
        screen,
    );
    state.lock().global_search_picker_window = Some(id);
}

pub(crate) fn global_search_items(
    roots: &[PathBuf],
    results: &[GlobalSearchResult],
) -> Vec<PickerItem<AppAction>> {
    results
        .iter()
        .map(|result| {
            let display_path = display_path_for_roots(&result.path, roots);
            PickerItem::new(
                format!("{}:{}:{}", display_path, result.line + 1, result.column + 1),
                AppAction::OpenPathAndJump {
                    path: result.path.clone(),
                    target: JumpTarget::CharPosition {
                        line: result.line,
                        column: result.column,
                    },
                },
            )
            .subtitle(result.text.trim().to_string())
        })
        .collect()
}

pub(crate) fn display_path_for_roots(path: &Path, roots: &[PathBuf]) -> String {
    for root in roots {
        if let Ok(rel) = path.strip_prefix(root)
            && !rel.as_os_str().is_empty()
        {
            return rel.to_string_lossy().to_string();
        }
    }
    path.to_string_lossy().to_string()
}
