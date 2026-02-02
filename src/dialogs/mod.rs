//! Higher-level, reusable UI dialogs (modal window content).
//!
//! Dialogs are views that are typically hosted in a `WindowKind::Modal` window.

mod file_dialog;

pub use file_dialog::{FileDialog, FileDialogMode};
