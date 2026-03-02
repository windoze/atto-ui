use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenTarget {
    NewTab,
    NewWindow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppAction {
    Quit,

    OpenFileDialog(OpenTarget),
    OpenFolderDialog,
    Save,
    SaveAsDialog,

    CloseTab,
    SplitVertical,
    SplitHorizontal,
    CloseSplit,

    ToggleExplorer,
    ExplorerLeft,
    ExplorerRight,

    // Open-folder modal buttons.
    SubmitOpenFolderDialog,
    CancelOpenFolderDialog,

    // Requests emitted from within a window (e.g. file tree Ctrl+Enter).
    OpenFileInNewWindow(PathBuf),
}
