use std::cmp::Ordering;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use crate::declarative::{Align, Divider, HStack, LayoutParams, Size, VStack, ViewAdapter};
use crate::reactive::{Binding, DirtyObserver, Property};
use crate::view::{View, ViewContext, ViewEventResult};
use crate::widgets::{Button, Control, ControlOutcome, FormAction, Label, ListBox, TextBox};

#[derive(Clone)]
struct FocusBindingControl<T> {
    inner: T,
    focused: Binding<bool>,
}

impl<T> FocusBindingControl<T> {
    fn new(inner: T, focused: Binding<bool>) -> Self {
        Self { inner, focused }
    }
}

impl<T> Control for FocusBindingControl<T>
where
    T: Control + Clone + Send + 'static,
{
    fn is_focusable(&self) -> bool {
        self.inner.is_focusable()
    }

    fn min_width(&self) -> u16 {
        self.inner.min_width()
    }

    fn min_height(&self) -> u16 {
        self.inner.min_height()
    }

    fn min_size(&self) -> (u16, u16) {
        self.inner.min_size()
    }

    fn is_enabled(&self) -> bool {
        self.inner.is_enabled()
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused.set(focused);
        self.inner.set_focused(focused);
    }

    fn set_area(&mut self, area: Rect) {
        self.inner.set_area(area);
    }

    fn handle_event(&mut self, event: &Event) -> (ControlOutcome, FormAction) {
        self.inner.handle_event(event)
    }

    fn draw(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect, theme: &crate::theme::Theme) {
        self.inner.draw(frame, area, theme);
    }

    fn desired_height(&self) -> u16 {
        self.inner.desired_height()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileDialogMode {
    OpenFile,
    SaveFile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingAction {
    Cancel,
    Submit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntryKind {
    Parent,
    Directory,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    name: OsString,
    path: PathBuf,
    kind: EntryKind,
}

impl Entry {
    fn display_name(&self) -> String {
        match self.kind {
            EntryKind::Parent => "../".to_string(),
            EntryKind::Directory => format!("{}/", self.name.to_string_lossy()),
            EntryKind::File => self.name.to_string_lossy().to_string(),
        }
    }

    fn file_name_string(&self) -> Option<String> {
        match self.kind {
            EntryKind::File => Some(self.name.to_string_lossy().to_string()),
            EntryKind::Parent | EntryKind::Directory => None,
        }
    }
}

/// A modal-friendly file dialog view (open/save) with basic directory navigation.
///
/// Intended usage:
/// - Create a `Property<Option<PathBuf>>` (or `Binding<Option<PathBuf>>`) in your app state.
/// - Host this dialog in a `WindowKind::Modal` window.
/// - When the user selects a file and confirms, the dialog sets the binding and closes itself.
///
/// Keyboard behavior:
/// - `Tab`/`Shift+Tab`: cycle focus.
/// - `Up`/`Down`: move selection in the file list.
/// - `Enter`:
///   - In the file list: open directory / select file.
///   - In the file name field: submit.
/// - `Backspace`: go to parent directory (when the file list is focused).
/// - `Esc`: cancel and close.
pub struct FileDialog {
    mode: FileDialogMode,
    result: Binding<Option<PathBuf>>,

    current_dir: Property<PathBuf>,
    dir_entries: Property<Vec<Entry>>,
    dir_display_items: Property<Vec<String>>,
    dir_selection: Property<usize>,

    file_entries: Property<Vec<Entry>>,
    file_display_items: Property<Vec<String>>,
    file_selection: Property<usize>,

    file_name: Property<String>,
    submit_enabled: Property<bool>,
    location_text: Property<String>,
    status_text: Property<String>,

    pending_action: Property<Option<PendingAction>>,

    dir_list_focused: Binding<bool>,
    file_list_focused: Binding<bool>,
    file_name_focused: Binding<bool>,

    inner: ViewAdapter,

    file_selection_observer: DirtyObserver,
    file_name_observer: DirtyObserver,
}

impl FileDialog {
    pub fn open_file(result: Binding<Option<PathBuf>>) -> Self {
        Self::new(FileDialogMode::OpenFile, result)
    }

    pub fn save_file(result: Binding<Option<PathBuf>>) -> Self {
        Self::new(FileDialogMode::SaveFile, result)
    }

    pub fn initial_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.current_dir.set(dir.into());
        self.refresh_entries();
        self
    }

    pub fn initial_file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name.set(name.into());
        self.recompute_submit_enabled();
        self
    }

    fn new(mode: FileDialogMode, result: Binding<Option<PathBuf>>) -> Self {
        let initial_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        let current_dir = Property::new(initial_dir);
        let dir_entries: Property<Vec<Entry>> = Property::new(Vec::new());
        let dir_display_items: Property<Vec<String>> = Property::new(Vec::new());
        let dir_selection: Property<usize> = Property::new(0);

        let file_entries: Property<Vec<Entry>> = Property::new(Vec::new());
        let file_display_items: Property<Vec<String>> = Property::new(Vec::new());
        let file_selection: Property<usize> = Property::new(0);

        let file_name: Property<String> = Property::new(String::new());
        let submit_enabled: Property<bool> = Property::new(false);
        let location_text: Property<String> = Property::new(String::new());
        let help_text = Self::default_help_text();
        let status_text: Property<String> = Property::new(String::new());
        let pending_action: Property<Option<PendingAction>> = Property::new(None);

        let dir_list_focused = Binding::new(false);
        let file_list_focused = Binding::new(false);
        let file_name_focused = Binding::new(false);

        let submit_label = match mode {
            FileDialogMode::OpenFile => "Open",
            FileDialogMode::SaveFile => "Save",
        };

        let cancel_action = {
            let pending_action = pending_action.clone();
            move || pending_action.set(Some(PendingAction::Cancel))
        };
        let submit_action = {
            let pending_action = pending_action.clone();
            move || pending_action.set(Some(PendingAction::Submit))
        };

        let root = VStack::new()
            .spacing(0)
            .padding(1)
            .child_with_layout(
                Label::new(location_text.binding()),
                LayoutParams {
                    height: Size::Content,
                    ..LayoutParams::default()
                },
            )
            .child_with_layout(
                Label::new(help_text),
                LayoutParams {
                    height: Size::Content,
                    ..LayoutParams::default()
                },
            )
            .child_with_layout(
                Label::new(status_text.binding()),
                LayoutParams {
                    height: Size::Content,
                    ..LayoutParams::default()
                },
            )
            .child_with_layout(
                Divider::horizontal(),
                LayoutParams {
                    height: Size::Content,
                    align_x: Align::Stretch,
                    ..LayoutParams::default()
                },
            )
            .child_with_layout(
                HStack::new()
                    .spacing(0)
                    .child_with_layout(
                        FocusBindingControl::new(
                            ListBox::new(
                                "Directories",
                                dir_display_items.binding(),
                                dir_selection.binding(),
                            ),
                            dir_list_focused.clone(),
                        ),
                        LayoutParams {
                            width: Size::Weight(1),
                            height: Size::Fill,
                            align_x: Align::Stretch,
                            align_y: Align::Stretch,
                            tab_index: Some(0),
                            ..LayoutParams::default()
                        },
                    )
                    .child_with_layout(
                        Divider::vertical(),
                        LayoutParams {
                            width: Size::Content,
                            height: Size::Fill,
                            align_y: Align::Stretch,
                            ..LayoutParams::default()
                        },
                    )
                    .child_with_layout(
                        FocusBindingControl::new(
                            ListBox::new(
                                "Files",
                                file_display_items.binding(),
                                file_selection.binding(),
                            ),
                            file_list_focused.clone(),
                        ),
                        LayoutParams {
                            width: Size::Weight(2),
                            height: Size::Fill,
                            align_x: Align::Stretch,
                            align_y: Align::Stretch,
                            tab_index: Some(1),
                            ..LayoutParams::default()
                        },
                    ),
                LayoutParams {
                    height: Size::Fill,
                    align_y: Align::Stretch,
                    ..LayoutParams::default()
                },
            )
            .child_with_layout(
                Divider::horizontal(),
                LayoutParams {
                    height: Size::Content,
                    align_x: Align::Stretch,
                    ..LayoutParams::default()
                },
            )
            .child_with_layout(
                HStack::new()
                    .spacing(1)
                    .child_with_layout(
                        FocusBindingControl::new(
                            TextBox::new("File name", file_name.binding()),
                            file_name_focused.clone(),
                        ),
                        LayoutParams {
                            width: Size::Weight(1),
                            tab_index: Some(2),
                            ..LayoutParams::default()
                        },
                    )
                    .child_with_layout(
                        Button::new("Cancel").on_click(cancel_action),
                        LayoutParams {
                            width: Size::Fixed(12),
                            tab_index: Some(3),
                            ..LayoutParams::default()
                        },
                    )
                    .child_with_layout(
                        Button::new(submit_label)
                            .enabled(submit_enabled.binding())
                            .on_click(submit_action),
                        LayoutParams {
                            width: Size::Fixed(12),
                            tab_index: Some(4),
                            ..LayoutParams::default()
                        },
                    ),
                LayoutParams {
                    height: Size::Content,
                    ..LayoutParams::default()
                },
            );

        let inner = ViewAdapter::new(root);

        let file_selection_observer = file_selection.dirty_observer();
        let file_name_observer = file_name.dirty_observer();

        let mut dialog = Self {
            mode,
            result,
            current_dir,
            dir_entries,
            dir_display_items,
            dir_selection,
            file_entries,
            file_display_items,
            file_selection,
            file_name,
            submit_enabled,
            location_text,
            status_text,
            pending_action,
            dir_list_focused,
            file_list_focused,
            file_name_focused,
            inner,
            file_selection_observer,
            file_name_observer,
        };
        dialog.refresh_entries();
        dialog.recompute_submit_enabled();
        dialog
    }

    fn default_help_text() -> String {
        "Tab: focus  ↑/↓: select  Enter: open/select  Backspace: parent  Esc: cancel".to_string()
    }

    fn refresh_entries(&mut self) {
        let dir = self.current_dir.get();
        self.location_text
            .set(format!("Location: {}", dir.display()));

        match list_dir_entries(&dir) {
            Ok(entries) => {
                let dir_entries: Vec<Entry> = entries
                    .iter()
                    .filter(|e| matches!(e.kind, EntryKind::Parent | EntryKind::Directory))
                    .cloned()
                    .collect();
                let file_entries: Vec<Entry> = entries
                    .iter()
                    .filter(|e| matches!(e.kind, EntryKind::File))
                    .cloned()
                    .collect();

                let dir_display: Vec<String> =
                    dir_entries.iter().map(Entry::display_name).collect();
                let file_display: Vec<String> =
                    file_entries.iter().map(Entry::display_name).collect();

                self.dir_entries.set(dir_entries);
                self.dir_display_items.set(dir_display);
                self.file_entries.set(file_entries);
                self.file_display_items.set(file_display);
                self.status_text.set(String::new());
                self.dir_selection.set(0);
                self.file_selection.set(0);
            }
            Err(err) => {
                self.status_text
                    .set(format!("Error reading directory: {err}"));

                let parent = dir.parent().map(|p| Entry {
                    name: OsString::from(".."),
                    path: p.to_path_buf(),
                    kind: EntryKind::Parent,
                });
                let dir_entries: Vec<Entry> = parent.into_iter().collect();
                let dir_display: Vec<String> =
                    dir_entries.iter().map(Entry::display_name).collect();
                self.dir_entries.set(dir_entries);
                self.dir_display_items.set(dir_display);
                self.file_entries.set(Vec::new());
                self.file_display_items.set(Vec::new());
                self.dir_selection.set(0);
                self.file_selection.set(0);
            }
        }
    }

    fn recompute_submit_enabled(&mut self) {
        self.submit_enabled
            .set(!self.file_name.get().trim().is_empty());
    }

    fn sync_file_name_from_file_selection(&mut self) {
        let idx = self.file_selection.get();
        let entries = self.file_entries.get();
        let Some(entry) = entries.get(idx) else {
            return;
        };
        if let Some(name) = entry.file_name_string() {
            self.file_name.set(name);
            self.recompute_submit_enabled();
        }
    }

    fn navigate_to(&mut self, dir: PathBuf) {
        self.current_dir.set(dir);
        self.refresh_entries();
    }

    fn navigate_parent(&mut self) {
        let dir = self.current_dir.get();
        let Some(parent) = dir.parent() else {
            return;
        };
        self.navigate_to(parent.to_path_buf());
    }

    fn activate_dir_selection(&mut self) {
        let idx = self.dir_selection.get();
        let entries = self.dir_entries.get();
        let Some(entry) = entries.get(idx) else {
            return;
        };

        match entry.kind {
            EntryKind::Parent | EntryKind::Directory => self.navigate_to(entry.path.clone()),
            EntryKind::File => {}
        }
    }

    fn activate_file_selection(&mut self) -> Option<ViewEventResult> {
        let idx = self.file_selection.get();
        let entries = self.file_entries.get();
        let Some(entry) = entries.get(idx) else {
            return None;
        };

        if entry.kind != EntryKind::File {
            return None;
        }

        if let Some(name) = entry.file_name_string() {
            self.file_name.set(name);
            self.recompute_submit_enabled();
        }

        match self.mode {
            FileDialogMode::OpenFile => self.submit().then_some(ViewEventResult::close_window()),
            FileDialogMode::SaveFile => None,
        }
    }

    fn submit(&mut self) -> bool {
        let raw = self.file_name.get();
        let name = raw.trim();
        if name.is_empty() {
            self.status_text
                .set("Please enter a file name.".to_string());
            return false;
        }

        let dir = self.current_dir.get();
        let path = dir.join(name);

        match self.mode {
            FileDialogMode::OpenFile => {
                if path.is_file() {
                    self.result.set(Some(path));
                    true
                } else if path.is_dir() {
                    self.navigate_to(path);
                    false
                } else {
                    self.status_text
                        .set(format!("File not found: {}", path.display()));
                    false
                }
            }
            FileDialogMode::SaveFile => {
                if path.is_dir() {
                    self.status_text
                        .set(format!("Cannot save: is a directory: {}", path.display()));
                    return false;
                }
                self.result.set(Some(path));
                true
            }
        }
    }
}

impl View for FileDialog {
    fn is_focusable(&self) -> bool {
        self.inner.is_focusable()
    }

    fn min_width(&self) -> u16 {
        self.inner.min_width().max(44)
    }

    fn min_height(&self) -> u16 {
        self.inner.min_height().max(12)
    }

    fn desired_width(&self) -> Option<u16> {
        self.inner.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.inner.desired_height()
    }

    fn children(&self) -> &[crate::views::ViewNode] {
        self.inner.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<crate::views::ViewNode>> {
        self.inner.children_mut()
    }

    fn handle_event_capture(&mut self, event: &Event, ctx: ViewContext<'_>) -> ViewEventResult {
        self.inner.handle_event_capture(event, ctx)
    }

    fn handle_event_bubble(&mut self, event: &Event, ctx: ViewContext<'_>) -> ViewEventResult {
        self.inner.handle_event_bubble(event, ctx)
    }

    fn handle_event(&mut self, event: &Event, ctx: ViewContext<'_>) -> ViewEventResult {
        // Esc cancels regardless of focus.
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            self.pending_action.set(None);
            return ViewEventResult::close_window();
        }

        let inner_res = self.inner.handle_event(event, ctx);

        // File selection changed (consumed by ListBox), but we still want to sync filename.
        if self
            .file_selection
            .check_dirty(&mut self.file_selection_observer)
        {
            self.sync_file_name_from_file_selection();
        }

        // File name changed (consumed by TextBox), but we still want to recompute enabled state.
        if self.file_name.check_dirty(&mut self.file_name_observer) {
            self.recompute_submit_enabled();
        }

        // Button callbacks set a pending action during the inner dispatch; finalize here.
        if let Some(pending) = self.pending_action.get() {
            self.pending_action.set(None);
            match pending {
                PendingAction::Cancel => return ViewEventResult::close_window(),
                PendingAction::Submit => {
                    if self.submit() {
                        return ViewEventResult::close_window();
                    }
                    return ViewEventResult::consumed();
                }
            }
        }

        // If the inner view didn't consume the event, it is most likely coming from a ListBox
        // (ListBox doesn't handle Enter/Backspace).
        if !inner_res.is_consumed() {
            if let Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                kind: KeyEventKind::Press,
                ..
            }) = event
            {
                if self.dir_list_focused.get() || self.file_list_focused.get() {
                    self.navigate_parent();
                }
                return ViewEventResult::consumed();
            }

            if let Event::Key(KeyEvent {
                code: KeyCode::Enter,
                kind: KeyEventKind::Press,
                ..
            }) = event
            {
                if self.dir_list_focused.get() {
                    self.activate_dir_selection();
                    return ViewEventResult::consumed();
                }
                if self.file_list_focused.get() {
                    if let Some(res) = self.activate_file_selection() {
                        return res;
                    }
                    return ViewEventResult::consumed();
                }
                return ViewEventResult::consumed();
            }
        }

        // Enter in the file name field should submit (TextBox consumes Enter but doesn't act).
        if let Event::Key(KeyEvent {
            code: KeyCode::Enter,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            if self.file_name_focused.get() {
                if self.submit() {
                    return ViewEventResult::close_window();
                }
                return ViewEventResult::consumed();
            }
        }

        inner_res
    }

    fn draw(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        self.inner.draw(frame, area, ctx);
    }
}

fn list_dir_entries(dir: &Path) -> std::io::Result<Vec<Entry>> {
    let mut out: Vec<Entry> = Vec::new();

    if let Some(parent) = dir.parent() {
        out.push(Entry {
            name: OsString::from(".."),
            path: parent.to_path_buf(),
            kind: EntryKind::Parent,
        });
    }

    let read_dir = fs::read_dir(dir)?;
    let mut entries: Vec<Entry> = Vec::new();
    for entry in read_dir {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let kind = if file_type.is_dir() {
            EntryKind::Directory
        } else {
            EntryKind::File
        };
        entries.push(Entry {
            name: entry.file_name(),
            path: entry.path(),
            kind,
        });
    }

    entries.sort_by(|a, b| cmp_entries(a, b));
    out.extend(entries);
    Ok(out)
}

fn cmp_entries(a: &Entry, b: &Entry) -> Ordering {
    // Parent always first.
    if a.kind == EntryKind::Parent && b.kind != EntryKind::Parent {
        return Ordering::Less;
    }
    if b.kind == EntryKind::Parent && a.kind != EntryKind::Parent {
        return Ordering::Greater;
    }

    // Directories before files.
    let ak = match a.kind {
        EntryKind::Parent => 0,
        EntryKind::Directory => 1,
        EntryKind::File => 2,
    };
    let bk = match b.kind {
        EntryKind::Parent => 0,
        EntryKind::Directory => 1,
        EntryKind::File => 2,
    };
    match ak.cmp(&bk) {
        Ordering::Equal => {}
        other => return other,
    }

    // Case-insensitive name ordering (fallback to original for stability).
    let al = a.name.to_string_lossy();
    let bl = b.name.to_string_lossy();
    let al_lower = al.to_lowercase();
    let bl_lower = bl.to_lowercase();
    match al_lower.cmp(&bl_lower) {
        Ordering::Equal => al.cmp(&bl),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use crate::view::{ScrollbarHost, TabMode, ViewContext};
    use crate::wm::WindowId;
    use crossterm::event::{KeyEventState, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        dir.push(format!(
            "chatty-file-dialog-tests-{prefix}-{}-{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn list_dir_entries_sorts_parent_dirs_then_files() {
        let root = make_temp_dir("sort");
        let _cleanup = TempDirCleanup(root.clone());

        fs::create_dir_all(root.join("b_dir")).unwrap();
        fs::create_dir_all(root.join("a_dir")).unwrap();
        fs::write(root.join("z_file.txt"), "z").unwrap();
        fs::write(root.join("A_file.txt"), "a").unwrap();

        let entries = list_dir_entries(&root).unwrap();
        let names: Vec<String> = entries.iter().map(Entry::display_name).collect();

        // No parent entry for a temp dir's root? Actually it should have a parent.
        assert!(!names.is_empty());
        assert_eq!(names[0], "../");
        assert_eq!(names[1], "a_dir/");
        assert_eq!(names[2], "b_dir/");
        assert_eq!(names[3], "A_file.txt");
        assert_eq!(names[4], "z_file.txt");
    }

    fn draw_dialog(dialog: &mut FileDialog, area: Rect, ctx: ViewContext<'_>) {
        let backend = TestBackend::new(area.width.max(1), area.height.max(1));
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|f| dialog.draw(f, area, ctx))
            .expect("draw dialog");
    }

    #[test]
    fn file_dialog_tab_order_dir_list_then_file_list_then_name_then_buttons() {
        let root = make_temp_dir("tab-order");
        let _cleanup = TempDirCleanup(root.clone());

        fs::create_dir_all(root.join("subdir")).unwrap();
        fs::write(root.join("file.txt"), "x").unwrap();

        let result: Property<Option<PathBuf>> = Property::new(None);
        let mut dialog = FileDialog::open_file(result.binding())
            .initial_dir(root)
            .initial_file_name("file.txt");

        let theme = Theme::dark();
        let ctx = ViewContext {
            theme: &theme,
            window_id: WindowId(1),
            is_focused: true,
            scrollbar_host: ScrollbarHost::View,
            tab_mode: TabMode::Cycle,
        };

        let area = Rect::new(0, 0, 80, 24);
        draw_dialog(&mut dialog, area, ctx);
        assert!(dialog.dir_list_focused.get());
        assert!(!dialog.file_list_focused.get());
        assert!(!dialog.file_name_focused.get());

        let tab = Event::Key(KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });

        assert!(dialog.handle_event(&tab, ctx).is_consumed());
        draw_dialog(&mut dialog, area, ctx);
        assert!(!dialog.dir_list_focused.get());
        assert!(dialog.file_list_focused.get());
        assert!(!dialog.file_name_focused.get());

        assert!(dialog.handle_event(&tab, ctx).is_consumed());
        draw_dialog(&mut dialog, area, ctx);
        assert!(!dialog.dir_list_focused.get());
        assert!(!dialog.file_list_focused.get());
        assert!(dialog.file_name_focused.get());

        // Next tab should move focus into the button row (either Cancel or Submit).
        assert!(dialog.handle_event(&tab, ctx).is_consumed());
        draw_dialog(&mut dialog, area, ctx);
        assert!(!dialog.dir_list_focused.get());
        assert!(!dialog.file_list_focused.get());
        assert!(!dialog.file_name_focused.get());

        // Tab through the remaining button(s), then wrap back to the directory list.
        assert!(dialog.handle_event(&tab, ctx).is_consumed());
        draw_dialog(&mut dialog, area, ctx);

        assert!(dialog.handle_event(&tab, ctx).is_consumed());
        draw_dialog(&mut dialog, area, ctx);
        assert!(dialog.dir_list_focused.get());
    }

    struct TempDirCleanup(PathBuf);

    impl Drop for TempDirCleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
