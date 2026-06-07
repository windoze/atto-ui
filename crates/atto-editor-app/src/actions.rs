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

    /// Toggles the docked Explorer window (file tree) visibility.
    ToggleExplorer,
    ExplorerLeft,
    ExplorerRight,

    // Open-folder modal buttons.
    SubmitOpenFolderDialog,
    CancelOpenFolderDialog,

    // Requests emitted from within a window (e.g. explorer Enter / Ctrl+Enter).
    OpenPath {
        path: PathBuf,
        target: OpenTarget,
    },
}
