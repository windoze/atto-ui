#![forbid(unsafe_code)]

//! Markdown viewer component.
//!
//! This crate provides [`MarkdownViewer`], a scrollable UI component for rendering markdown inside
//! `atto-ui` applications.

mod dynamic;
mod markdown;
pub mod syntax;

pub use dynamic::{markdown_viewer_schema, register_markdown_viewer, register_runtime_components};
pub use markdown::MarkdownViewer;
