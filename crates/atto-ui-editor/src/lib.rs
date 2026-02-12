#![forbid(unsafe_code)]

//! Editor component built on top of `editor-core`.
//!
//! This module provides a `Component` implementation (`EditorView`) that integrates:
//! - syntax highlighting (Sublime `.sublime-syntax` or simple regex fallback)
//! - code folding
//! - LSP (semantic tokens, folding ranges, hover, completion, goto*)
//!
//! The editor maintains its own theming system (per-language) and does not depend on
//! `atto_ui::theme::Theme` for text styling.

mod config;
mod dynamic;
mod keymap;
mod popup;
mod theme;
mod view;

pub use config::{
    EditorCompletionConfig, EditorConfig, EditorHoverConfig, EditorIndentConfig, EditorLspConfig,
    EditorLspGotoKind, EditorLspMode, EditorScrollConfig, EditorSyntaxConfig,
};
pub use dynamic::{editor_schema, register_editor, register_runtime_components};
pub use keymap::{EditorAction, EditorKeymap, KeyChord};
pub use popup::{
    CompletionItem, CompletionPopupModel, EditorPopupWindows, HoverPopupModel,
    LspCompletionItemEdit, LspHoverContents,
};
pub use theme::{EditorTheme, EditorThemeSet, SemanticTokenTheme};
pub use view::{EditorEvent, EditorView, EditorViewHandle};
