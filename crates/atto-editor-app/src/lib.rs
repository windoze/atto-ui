#![forbid(unsafe_code)]

//! `atto-editor-app` — a terminal editor app built on `atto-ui` + `editor-core`.
//!
//! This crate provides an interactive editor application with:
//! - workspace roots (folders + individual files)
//! - a standalone file explorer window (toggle + dock left/right)
//! - multi-file editing with window tabs (via `atto_ui::composable::TabWindow`)
//! - split views for the same document (vertical/horizontal)
//! - syntax highlighting + folding via Tree-sitter (`editor-core-treesitter` through `atto-ui-editor`)
//! - LSP-powered completion/hover/goto via `editor-core-lsp` (through `atto-ui-editor`)
//!
//! The implementation is intentionally structured as a small set of modules (`app`, `window`,
//! `workspace`, `language`) so consumers can reuse pieces or swap behaviors (e.g. language / LSP
//! configuration) without forking the entire UI.

pub mod actions;
pub mod app;
pub mod commands;
pub mod explorer_window;
pub mod language;
pub mod picker;
pub mod search;
pub mod window;
pub mod workspace;

pub use app::{AttoEditorConfig, run};
