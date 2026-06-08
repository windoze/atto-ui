#![forbid(unsafe_code)]

//! Editor component built on top of `editor-core`.
//!
//! This module provides a `Component` implementation (`EditorView`) that integrates:
//! - syntax highlighting (Tree-sitter, Sublime `.sublime-syntax`, or simple regex fallback)
//! - code folding (Tree-sitter / Sublime / LSP folding ranges)
//! - LSP (semantic tokens, folding ranges, hover, completion, signature help, goto*)
//!
//! The editor maintains its own theming system (per-language) and does not depend on
//! `atto_ui::theme::Theme` for text styling.

mod artifact;
mod config;
mod diff;
mod dynamic;
mod keymap;
mod popup;
mod syntax;
mod theme;
mod view;

pub use artifact::RichArtifactViewer;
pub use config::{
    EditorCompletionConfig, EditorConfig, EditorHoverConfig, EditorIndentConfig,
    EditorInlayHintsConfig, EditorLspConfig, EditorLspGotoKind, EditorLspMode, EditorScrollConfig,
    EditorSyntaxConfig, EditorTreeSitterConfig,
};
pub use diff::{DiffView, DiffViewConfig, DiffViewHandle, DiffViewMode};
pub use dynamic::{editor_schema, register_editor, register_runtime_components};
pub use keymap::{EditorAction, EditorKeymap, KeyChord};
pub use popup::{
    CodeActionItemView, CodeActionPopupModel, CompletionItem, CompletionPopupModel,
    EditorPopupWindows, HoverPopupModel, LspCompletionItemEdit, LspHoverContents, RenamePopupModel,
    SignatureHelpPopupModel,
};
pub use theme::{
    EditorTheme, EditorThemeSet, LSP_DIAGNOSTIC_ERROR_STYLE_ID, LSP_DIAGNOSTIC_HINT_STYLE_ID,
    LSP_DIAGNOSTIC_INFO_STYLE_ID, LSP_DIAGNOSTIC_STYLE_BASE, LSP_DIAGNOSTIC_WARNING_STYLE_ID,
    SEARCH_CURRENT_STYLE_ID, SEARCH_MATCH_STYLE_ID, SemanticTokenTheme, TS_STYLE_COMMENT,
    TS_STYLE_CONSTANT, TS_STYLE_FUNCTION, TS_STYLE_KEYWORD, TS_STYLE_NUMBER, TS_STYLE_STRING,
    TS_STYLE_TYPE, TS_STYLE_VARIABLE,
};
pub use view::{DiagnosticsSummary, EditorEvent, EditorView, EditorViewHandle};
