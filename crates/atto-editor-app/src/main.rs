#![forbid(unsafe_code)]

use anyhow::Result;

fn main() -> Result<()> {
    atto_editor_app::run(atto_editor_app::AttoEditorConfig::from_env_args())
}
