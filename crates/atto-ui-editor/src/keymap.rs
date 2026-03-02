use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyChord {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn from_key_event(event: KeyEvent) -> Option<Self> {
        if event.kind != KeyEventKind::Press {
            return None;
        }
        Some(Self {
            code: event.code,
            modifiers: event.modifiers,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditorAction {
    // --- Core editing
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    SelectAll,

    // --- Find / replace
    Find,
    Replace,
    FindNext,
    FindPrev,

    Backspace,
    DeleteForward,
    InsertNewline,
    InsertTab,

    // --- Cursor movement (non-selecting)
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveHome,
    MoveEnd,
    PageUp,
    PageDown,

    // --- Cursor movement (extending selection)
    SelectLeft,
    SelectRight,
    SelectUp,
    SelectDown,
    SelectHome,
    SelectEnd,
    SelectPageUp,
    SelectPageDown,

    // --- Multi-cursor / selection modes
    ClearSecondarySelections,
    ToggleRectSelection,

    // --- Folding
    ToggleFoldAtCursor,
    UnfoldAll,

    // --- Popups
    CancelPopup,

    // --- LSP
    LspRequestHover,
    LspRequestCompletion,
    LspGotoDefinition,
    LspGotoDeclaration,
    LspGotoTypeDefinition,
    LspGotoImplementation,
    LspGotoReferences,

    // --- UI toggles
    ToggleLineNumbers,
    ToggleFoldingMarkers,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorKeymap {
    bindings: HashMap<KeyChord, EditorAction>,
}

impl EditorKeymap {
    pub fn empty() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    pub fn default_bindings() -> Self {
        use EditorAction as A;

        let mut map = HashMap::<KeyChord, EditorAction>::new();

        // --- Editing
        map.insert(
            KeyChord::new(KeyCode::Char('z'), KeyModifiers::CONTROL),
            A::Undo,
        );
        map.insert(
            KeyChord::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
            A::Redo,
        );
        map.insert(
            KeyChord::new(
                KeyCode::Char('z'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
            A::Redo,
        );

        map.insert(
            KeyChord::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            A::Copy,
        );
        map.insert(
            KeyChord::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
            A::Cut,
        );
        map.insert(
            KeyChord::new(KeyCode::Char('v'), KeyModifiers::CONTROL),
            A::Paste,
        );
        map.insert(
            KeyChord::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
            A::SelectAll,
        );

        map.insert(
            KeyChord::new(KeyCode::Backspace, KeyModifiers::NONE),
            A::Backspace,
        );
        map.insert(
            KeyChord::new(KeyCode::Delete, KeyModifiers::NONE),
            A::DeleteForward,
        );
        map.insert(
            KeyChord::new(KeyCode::Enter, KeyModifiers::NONE),
            A::InsertNewline,
        );
        map.insert(
            KeyChord::new(KeyCode::Tab, KeyModifiers::NONE),
            A::InsertTab,
        );

        // --- Cursor movement
        map.insert(
            KeyChord::new(KeyCode::Left, KeyModifiers::NONE),
            A::MoveLeft,
        );
        map.insert(
            KeyChord::new(KeyCode::Right, KeyModifiers::NONE),
            A::MoveRight,
        );
        map.insert(KeyChord::new(KeyCode::Up, KeyModifiers::NONE), A::MoveUp);
        map.insert(
            KeyChord::new(KeyCode::Down, KeyModifiers::NONE),
            A::MoveDown,
        );
        map.insert(
            KeyChord::new(KeyCode::Home, KeyModifiers::NONE),
            A::MoveHome,
        );
        map.insert(KeyChord::new(KeyCode::End, KeyModifiers::NONE), A::MoveEnd);
        map.insert(
            KeyChord::new(KeyCode::PageUp, KeyModifiers::NONE),
            A::PageUp,
        );
        map.insert(
            KeyChord::new(KeyCode::PageDown, KeyModifiers::NONE),
            A::PageDown,
        );

        map.insert(
            KeyChord::new(KeyCode::Left, KeyModifiers::SHIFT),
            A::SelectLeft,
        );
        map.insert(
            KeyChord::new(KeyCode::Right, KeyModifiers::SHIFT),
            A::SelectRight,
        );
        map.insert(KeyChord::new(KeyCode::Up, KeyModifiers::SHIFT), A::SelectUp);
        map.insert(
            KeyChord::new(KeyCode::Down, KeyModifiers::SHIFT),
            A::SelectDown,
        );
        map.insert(
            KeyChord::new(KeyCode::Home, KeyModifiers::SHIFT),
            A::SelectHome,
        );
        map.insert(
            KeyChord::new(KeyCode::End, KeyModifiers::SHIFT),
            A::SelectEnd,
        );
        map.insert(
            KeyChord::new(KeyCode::PageUp, KeyModifiers::SHIFT),
            A::SelectPageUp,
        );
        map.insert(
            KeyChord::new(KeyCode::PageDown, KeyModifiers::SHIFT),
            A::SelectPageDown,
        );

        // --- Selection modes
        map.insert(
            KeyChord::new(KeyCode::Esc, KeyModifiers::NONE),
            A::CancelPopup,
        );

        // --- Find / replace
        map.insert(
            KeyChord::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
            A::Find,
        );
        map.insert(
            KeyChord::new(KeyCode::Char('h'), KeyModifiers::CONTROL),
            A::Replace,
        );
        map.insert(
            KeyChord::new(KeyCode::F(3), KeyModifiers::NONE),
            A::FindNext,
        );
        map.insert(
            KeyChord::new(KeyCode::F(3), KeyModifiers::SHIFT),
            A::FindPrev,
        );

        map.insert(
            KeyChord::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
            A::ToggleRectSelection,
        );
        map.insert(
            KeyChord::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            A::UnfoldAll,
        );
        map.insert(
            KeyChord::new(KeyCode::Char('l'), KeyModifiers::CONTROL),
            A::ToggleFoldAtCursor,
        );
        map.insert(
            KeyChord::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
            A::ClearSecondarySelections,
        );

        // --- LSP
        map.insert(
            KeyChord::new(KeyCode::Char(' '), KeyModifiers::CONTROL),
            A::LspRequestCompletion,
        );
        map.insert(
            KeyChord::new(KeyCode::F(12), KeyModifiers::NONE),
            A::LspGotoDefinition,
        );
        map.insert(
            KeyChord::new(KeyCode::F(12), KeyModifiers::SHIFT),
            A::LspGotoReferences,
        );
        map.insert(
            KeyChord::new(KeyCode::F(12), KeyModifiers::CONTROL),
            A::LspGotoDeclaration,
        );
        map.insert(
            KeyChord::new(KeyCode::F(12), KeyModifiers::CONTROL | KeyModifiers::SHIFT),
            A::LspGotoTypeDefinition,
        );
        map.insert(
            KeyChord::new(KeyCode::F(12), KeyModifiers::ALT),
            A::LspGotoImplementation,
        );

        Self { bindings: map }
    }

    pub fn get(&self, chord: KeyChord) -> Option<EditorAction> {
        self.bindings.get(&chord).copied()
    }

    pub fn insert(&mut self, chord: KeyChord, action: EditorAction) {
        self.bindings.insert(chord, action);
    }

    pub fn remove(&mut self, chord: KeyChord) -> Option<EditorAction> {
        self.bindings.remove(&chord)
    }

    pub fn bindings(&self) -> &HashMap<KeyChord, EditorAction> {
        &self.bindings
    }
}

impl Default for EditorKeymap {
    fn default() -> Self {
        Self::default_bindings()
    }
}
