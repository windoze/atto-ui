use std::collections::HashMap;

use editor_core::{
    DIFF_ADD_LINE_STYLE_ID, DIFF_REMOVE_LINE_STYLE_ID, DIFF_SPACER_STYLE_ID,
    FOLD_PLACEHOLDER_STYLE_ID,
};
use editor_core_highlight_simple::{
    SIMPLE_STYLE_BOOLEAN, SIMPLE_STYLE_COMMENT, SIMPLE_STYLE_KEY, SIMPLE_STYLE_NULL,
    SIMPLE_STYLE_NUMBER, SIMPLE_STYLE_SECTION, SIMPLE_STYLE_STRING,
};
use ratatui::style::{Color, Modifier, Style};

// --- Additional style ids used by atto-ui-editor
//
// These ids are intentionally kept in a separate numeric range from:
// - editor-core built-ins (e.g. `FOLD_PLACEHOLDER_STYLE_ID`)
// - editor-core-highlight-simple ids
// - editor-core-lsp semantic token encoding (< 0x0100_0000)

/// Tree-sitter: comment capture style id.
pub const TS_STYLE_COMMENT: u32 = 0x0500_0001;
/// Tree-sitter: string capture style id.
pub const TS_STYLE_STRING: u32 = 0x0500_0002;
/// Tree-sitter: number capture style id.
pub const TS_STYLE_NUMBER: u32 = 0x0500_0003;
/// Tree-sitter: keyword capture style id.
pub const TS_STYLE_KEYWORD: u32 = 0x0500_0004;
/// Tree-sitter: function/method capture style id.
pub const TS_STYLE_FUNCTION: u32 = 0x0500_0005;
/// Tree-sitter: type/struct/enum capture style id.
pub const TS_STYLE_TYPE: u32 = 0x0500_0006;
/// Tree-sitter: variable/identifier capture style id.
pub const TS_STYLE_VARIABLE: u32 = 0x0500_0007;
/// Tree-sitter: constant capture style id.
pub const TS_STYLE_CONSTANT: u32 = 0x0500_0008;

/// Find/replace: match highlight style id.
pub const SEARCH_MATCH_STYLE_ID: u32 = 0x0600_0001;
/// Find/replace: "current match" highlight style id (optional).
pub const SEARCH_CURRENT_STYLE_ID: u32 = 0x0600_0002;

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticTokenTheme {
    pub token_types: HashMap<String, Style>,
    pub token_modifiers: HashMap<String, Modifier>,
    pub unknown_token_type: Style,
    pub unknown_token_modifier: Modifier,
}

impl SemanticTokenTheme {
    pub fn dark_default() -> Self {
        let mut token_types = HashMap::<String, Style>::new();
        token_types.insert("comment".to_string(), Style::default().fg(Color::DarkGray));
        token_types.insert("string".to_string(), Style::default().fg(Color::Green));
        token_types.insert("number".to_string(), Style::default().fg(Color::Yellow));
        token_types.insert("keyword".to_string(), Style::default().fg(Color::LightBlue));
        token_types.insert("function".to_string(), Style::default().fg(Color::Cyan));
        token_types.insert("method".to_string(), Style::default().fg(Color::Cyan));
        token_types.insert("macro".to_string(), Style::default().fg(Color::Magenta));
        token_types.insert("operator".to_string(), Style::default().fg(Color::LightRed));
        token_types.insert(
            "parameter".to_string(),
            Style::default().fg(Color::LightYellow),
        );
        token_types.insert("variable".to_string(), Style::default().fg(Color::White));
        token_types.insert("property".to_string(), Style::default().fg(Color::White));
        token_types.insert("enumMember".to_string(), Style::default().fg(Color::White));
        token_types.insert(
            "namespace".to_string(),
            Style::default().fg(Color::LightMagenta),
        );

        // A conservative "type-ish" group.
        let type_style = Style::default().fg(Color::LightCyan);
        for name in [
            "type",
            "struct",
            "enum",
            "class",
            "interface",
            "typeParameter",
        ] {
            token_types.insert(name.to_string(), type_style);
        }

        let mut token_modifiers = HashMap::<String, Modifier>::new();
        token_modifiers.insert("declaration".to_string(), Modifier::BOLD);
        token_modifiers.insert("definition".to_string(), Modifier::BOLD);
        token_modifiers.insert("documentation".to_string(), Modifier::ITALIC);
        token_modifiers.insert("readonly".to_string(), Modifier::UNDERLINED);
        token_modifiers.insert("static".to_string(), Modifier::DIM);
        token_modifiers.insert("deprecated".to_string(), Modifier::UNDERLINED);
        token_modifiers.insert("async".to_string(), Modifier::ITALIC);

        Self {
            token_types,
            token_modifiers,
            unknown_token_type: Style::default().fg(Color::White),
            unknown_token_modifier: Modifier::empty(),
        }
    }
}

impl Default for SemanticTokenTheme {
    fn default() -> Self {
        Self::dark_default()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditorTheme {
    /// Default text style (foreground).
    pub text: Style,
    /// Editor background fill (should include `bg`).
    pub background: Style,
    /// Selection style (should include `bg`).
    pub selection: Style,

    /// Gutter background/foreground (line numbers, fold markers).
    pub gutter: Style,
    /// Active line number style.
    pub gutter_active: Style,
    /// Folding marker style.
    pub fold_marker: Style,

    /// Popup background/foreground.
    pub popup: Style,
    pub popup_border: Style,
    pub popup_selected: Style,

    /// Used when a Sublime scope is unknown.
    pub unknown_scope: Style,
    /// Used when a `StyleId` is unknown and cannot be decoded (or mapped) by any configured
    /// provider.
    pub unknown_style_id: Style,

    /// Direct `StyleId -> Style` mapping.
    pub style_ids: HashMap<u32, Style>,
    /// Sublime scope -> style mapping (exact matches + hierarchical fallback).
    pub sublime_scopes: HashMap<String, Style>,
    /// LSP semantic tokens theming.
    pub semantic_tokens: SemanticTokenTheme,
}

impl EditorTheme {
    pub fn dark_default() -> Self {
        let background = Style::default().bg(Color::Black);
        let text = Style::default().fg(Color::White);

        let mut style_ids = HashMap::<u32, Style>::new();
        style_ids.insert(SIMPLE_STYLE_STRING, Style::default().fg(Color::Green));
        style_ids.insert(SIMPLE_STYLE_NUMBER, Style::default().fg(Color::Yellow));
        style_ids.insert(SIMPLE_STYLE_BOOLEAN, Style::default().fg(Color::Magenta));
        style_ids.insert(SIMPLE_STYLE_NULL, Style::default().fg(Color::DarkGray));
        style_ids.insert(
            SIMPLE_STYLE_SECTION,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
        style_ids.insert(SIMPLE_STYLE_KEY, Style::default().fg(Color::Blue));
        style_ids.insert(
            SIMPLE_STYLE_COMMENT,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        );
        style_ids.insert(
            FOLD_PLACEHOLDER_STYLE_ID,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        );

        // Tree-sitter styles (host-provided capture -> these ids).
        style_ids.insert(
            TS_STYLE_COMMENT,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        );
        style_ids.insert(TS_STYLE_STRING, Style::default().fg(Color::Green));
        style_ids.insert(TS_STYLE_NUMBER, Style::default().fg(Color::Yellow));
        style_ids.insert(TS_STYLE_KEYWORD, Style::default().fg(Color::LightBlue));
        style_ids.insert(TS_STYLE_FUNCTION, Style::default().fg(Color::Cyan));
        style_ids.insert(
            TS_STYLE_TYPE,
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        );
        style_ids.insert(TS_STYLE_VARIABLE, Style::default().fg(Color::White));
        style_ids.insert(TS_STYLE_CONSTANT, Style::default().fg(Color::Magenta));

        // Diff line backgrounds (added / removed / alignment spacer).
        style_ids.insert(
            DIFF_ADD_LINE_STYLE_ID,
            Style::default().bg(Color::Rgb(0, 48, 0)),
        );
        style_ids.insert(
            DIFF_REMOVE_LINE_STYLE_ID,
            Style::default().bg(Color::Rgb(64, 0, 0)),
        );
        style_ids.insert(
            DIFF_SPACER_STYLE_ID,
            Style::default().bg(Color::Rgb(18, 18, 18)),
        );

        // Find/replace match highlights.
        style_ids.insert(
            SEARCH_MATCH_STYLE_ID,
            Style::default().bg(Color::DarkGray).fg(Color::White),
        );
        style_ids.insert(
            SEARCH_CURRENT_STYLE_ID,
            Style::default().bg(Color::LightYellow).fg(Color::Black),
        );

        // A small out-of-the-box Sublime scope theme. This intentionally uses broad prefix keys
        // (e.g. "comment", "string") because `EditorView` applies hierarchical fallback
        // (`comment.line` -> `comment`) when resolving scopes.
        let mut sublime_scopes = HashMap::<String, Style>::new();
        sublime_scopes.insert(
            "comment".to_string(),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        );
        sublime_scopes.insert("string".to_string(), Style::default().fg(Color::Green));
        sublime_scopes.insert(
            "constant.numeric".to_string(),
            Style::default().fg(Color::Yellow),
        );
        sublime_scopes.insert("keyword".to_string(), Style::default().fg(Color::LightBlue));
        sublime_scopes.insert("storage".to_string(), Style::default().fg(Color::LightBlue));
        sublime_scopes.insert(
            "entity.name.function".to_string(),
            Style::default().fg(Color::Cyan),
        );
        sublime_scopes.insert(
            "entity.name.type".to_string(),
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        );

        Self {
            text,
            background,
            selection: Style::default().bg(Color::Blue).fg(Color::White),
            gutter: Style::default().bg(Color::Black).fg(Color::DarkGray),
            gutter_active: Style::default().bg(Color::Black).fg(Color::White),
            fold_marker: Style::default().bg(Color::Black).fg(Color::LightCyan),
            popup: Style::default().bg(Color::Black).fg(Color::White),
            popup_border: Style::default().fg(Color::DarkGray),
            popup_selected: Style::default().bg(Color::Blue).fg(Color::White),
            unknown_scope: Style::default().fg(Color::White),
            unknown_style_id: Style::default().fg(Color::White),
            style_ids,
            sublime_scopes,
            semantic_tokens: SemanticTokenTheme::dark_default(),
        }
    }
}

impl Default for EditorTheme {
    fn default() -> Self {
        Self::dark_default()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditorThemeSet {
    pub default: EditorTheme,
    pub by_language: HashMap<String, EditorTheme>,
}

impl EditorThemeSet {
    pub fn new(default: EditorTheme) -> Self {
        Self {
            default,
            by_language: HashMap::new(),
        }
    }

    pub fn for_language(&self, language_id: &str) -> &EditorTheme {
        self.by_language.get(language_id).unwrap_or(&self.default)
    }

    pub fn insert_language(&mut self, language_id: impl Into<String>, theme: EditorTheme) {
        self.by_language.insert(language_id.into(), theme);
    }
}

impl Default for EditorThemeSet {
    fn default() -> Self {
        Self::new(EditorTheme::dark_default())
    }
}
