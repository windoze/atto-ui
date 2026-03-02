#![forbid(unsafe_code)]

use anyhow::Result;

fn main() -> Result<()> {
    atto_editor::run(atto_editor::AttoEditorConfig::from_env_args())
}
