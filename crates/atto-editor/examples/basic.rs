#![forbid(unsafe_code)]

use std::path::PathBuf;

use anyhow::Result;

fn main() -> Result<()> {
    atto_editor::run(atto_editor::AttoEditorConfig {
        initial_paths: vec![PathBuf::from(".")],
    })
}
