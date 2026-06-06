#![forbid(unsafe_code)]

use std::path::PathBuf;

use anyhow::Result;

fn main() -> Result<()> {
    atto_editor_app::run(atto_editor_app::AttoEditorConfig {
        initial_paths: vec![PathBuf::from(".")],
    })
}
