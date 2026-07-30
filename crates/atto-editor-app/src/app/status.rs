//! Status-bar segment formatting: diagnostics summary, mode label, path text,
//! and the per-mode status segment builder.

use super::*;

pub(crate) fn update_diagnostics_statusbar(desktop: &mut Desktop, state: &Arc<Mutex<AppState>>) {
    let summary = active_editor_diagnostics_summary(desktop, state).unwrap_or_default();
    let editor_status = active_editor_status(desktop, state).unwrap_or_default();
    let status_message = state.lock().status_message.clone();
    desktop.status.set_segments(status_segments_for(
        desktop.mode,
        editor_status,
        summary,
        status_message,
    ));
}

pub(crate) fn set_status_message(state: &Arc<Mutex<AppState>>, message: impl Into<String>) {
    state.lock().status_message = Some(message.into());
}

pub(crate) fn status_left_for_mode(mode: DesktopMode) -> &'static str {
    match mode {
        DesktopMode::Normal => "F10 Menu  Ctrl+W Window  F6 Next",
        DesktopMode::Menu => "Menu: ←/→/↑/↓ Enter  Esc Close",
        DesktopMode::WindowManagement => {
            "Window: arrows move  Shift+arrows resize  c close  x max  m min  Esc exit"
        }
    }
}

pub(crate) fn format_diagnostics_summary(summary: atto_ui_editor::DiagnosticsSummary) -> String {
    format!("E:{} W:{}", summary.errors, summary.warnings)
}

pub(crate) fn status_segments_for(
    mode: DesktopMode,
    editor_status: EditorStatus,
    summary: atto_ui_editor::DiagnosticsSummary,
    status_message: Option<String>,
) -> Vec<StatusSegment> {
    let mut segments = vec![
        StatusSegment::new("app", "Atto Editor")
            .style("status-bar-key")
            .priority(100)
            .min_width(11),
        StatusSegment::new("path", status_path_text(editor_status.path.as_ref()))
            .priority(80)
            .min_width(8),
    ];

    if editor_status.dirty {
        segments.push(
            StatusSegment::new("dirty", "*")
                .style("status-segment-warning")
                .priority(90),
        );
    }

    if let Some(message) = status_message
        && !message.is_empty()
    {
        segments.push(
            StatusSegment::new("message", message)
                .style("status-segment-warning")
                .priority(95)
                .min_width(8),
        );
    }

    segments.push(
        StatusSegment::new("mode", status_left_for_mode(mode))
            .priority(10)
            .min_width(8),
    );

    let diagnostics_style = if summary.errors > 0 {
        "status-segment-error"
    } else if summary.warnings > 0 {
        "status-segment-warning"
    } else {
        "status-segment"
    };
    segments.push(
        StatusSegment::new("diagnostics", format_diagnostics_summary(summary))
            .style(diagnostics_style)
            .align(StatusSegmentAlign::Right)
            .priority(90)
            .min_width(7),
    );

    let language = if editor_status.language.is_empty() {
        "plaintext".to_string()
    } else {
        editor_status.language
    };
    segments.push(
        StatusSegment::new("language", language)
            .align(StatusSegmentAlign::Right)
            .priority(70)
            .min_width(4),
    );

    segments
}

pub(crate) fn status_path_text(path: Option<&PathBuf>) -> String {
    path.map(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| path.to_string_lossy().to_string())
    })
    .unwrap_or_else(|| "[No file]".to_string())
}

pub(crate) fn active_editor_diagnostics_summary(
    desktop: &Desktop,
    state: &Arc<Mutex<AppState>>,
) -> Option<atto_ui_editor::DiagnosticsSummary> {
    let focused = desktop.wm.focused();

    {
        let guard = state.lock();
        if let Some(id) = focused
            && desktop.wm.window(id).is_some()
            && let Some(summary) = guard.editor_diagnostics.get(&id)
        {
            return Some(summary.get());
        }

        if let Some(id) = guard.last_focused_editor
            && desktop.wm.window(id).is_some()
            && let Some(summary) = guard.editor_diagnostics.get(&id)
        {
            return Some(summary.get());
        }
    }

    let guard = state.lock();
    for w in desktop.wm.windows().iter().rev() {
        if let Some(summary) = guard.editor_diagnostics.get(&w.id()) {
            return Some(summary.get());
        }
    }

    None
}

pub(crate) fn active_editor_status(desktop: &Desktop, state: &Arc<Mutex<AppState>>) -> Option<EditorStatus> {
    let focused = desktop.wm.focused();

    {
        let guard = state.lock();
        if let Some(id) = focused
            && desktop.wm.window(id).is_some()
            && let Some(status) = guard.editor_statuses.get(&id)
        {
            return Some(status.get());
        }

        if let Some(id) = guard.last_focused_editor
            && desktop.wm.window(id).is_some()
            && let Some(status) = guard.editor_statuses.get(&id)
        {
            return Some(status.get());
        }
    }

    let guard = state.lock();
    for w in desktop.wm.windows().iter().rev() {
        if let Some(status) = guard.editor_statuses.get(&w.id()) {
            return Some(status.get());
        }
    }

    None
}
