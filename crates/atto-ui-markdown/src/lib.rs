#![forbid(unsafe_code)]

//! Markdown viewer component.
//!
//! This crate provides [`MarkdownViewer`], a scrollable UI component for rendering markdown inside
//! `atto-ui` applications.

mod markdown;

pub use markdown::MarkdownViewer;
