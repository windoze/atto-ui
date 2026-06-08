use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenTarget {
    NewTab,
    NewWindow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JumpTarget {
    CharOffset { offset: usize },
    CharPosition { line: usize, column: usize },
    Utf16Position { line: u32, character: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppAction {
    Quit,

    OpenFileDialog(OpenTarget),
    OpenFolderDialog,
    Save,
    SaveAsDialog,
    OpenCommandPalette,
    OpenFilePicker,
    OpenBufferPicker,
    OpenDocumentSymbolPicker,
    OpenWorkspaceSymbolPicker,
    OpenGlobalSearch,

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
    OpenPathAndJump {
        path: PathBuf,
        target: JumpTarget,
    },
    JumpTo(JumpTarget),
    SelectEditorTab {
        window: atto_ui::wm::WindowId,
        tab_id: u64,
    },
    ShowStatusMessage(String),
}
