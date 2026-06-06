# atto-editor-app

`atto-editor-app` is a terminal editor application built on top of:

- `atto-ui` (window manager + widgets + desktop chrome)
- `atto-ui-editor` (an `editor-core`-powered editor widget)
- `editor-core-treesitter` (syntax highlighting + code folding)
- `editor-core-lsp` (completion/hover/goto via language servers)
- `editor-core-sublime` (optional fallback highlighting via `.sublime-syntax`)

This crate is designed as an application crate (`cargo run -p atto-editor-app`) but is also usable as a
library via `atto_editor_app::run(AttoEditorConfig)`.

## Quick Start

Run the editor:

```sh
cargo run -p atto-editor-app
```

Open one or more files and/or folders at startup:

```sh
# Open a folder as a workspace root
cargo run -p atto-editor-app -- path/to/project

# Open a file in a tab (and auto-add its parent folder as a workspace root)
cargo run -p atto-editor-app -- path/to/file.rs

# Mix folders + files
cargo run -p atto-editor-app -- path/to/project path/to/other/file.py
```

## Features

- Workspace roots (multiple folders) + file explorer window
  - Toggle Explorer visibility (`Ctrl+E` / View menu)
  - Dock Explorer left or right (View menu)
  - Open files from the explorer into tabs
  - `Ctrl+Enter` opens a file in a new window
- Multi-file editing via window tabs (`atto_ui::composable::TabWindow`)
  - Dirty tabs show a `*` suffix (`file.rs*`)
- Split view for the same file (two views backed by the same document text)
  - Vertical split
  - Horizontal split
- Editing features (from `atto-ui-editor`)
  - Undo/redo, copy/cut/paste, multi-cursor, rectangle selection
  - Find/replace UI with match highlighting
  - Tree-sitter syntax highlighting + code folding
  - LSP completion/hover/goto-* when a language server is configured

## Keybindings

### App / Window

- `Ctrl+O` — Open File… (opens in a new tab in the focused editor window)
- `Ctrl+S` — Save active tab
- `Ctrl+W` — Close active tab
- `Ctrl+E` — Toggle Explorer window

### Explorer Window

- `Enter` — Open the selected file in a new tab
- `Ctrl+Enter` — Open the selected file in a new window
- Mouse double-click — Open the selected file in a new tab

### Editor (atto-ui-editor)

- `Ctrl+Z` — Undo
- `Ctrl+Y` / `Ctrl+Shift+Z` — Redo
- `Ctrl+C` / `Ctrl+X` / `Ctrl+V` — Copy / Cut / Paste
- `Ctrl+F` — Find
- `Ctrl+H` — Replace
- `F3` / `Shift+F3` — Find next / previous
- `Ctrl+Space` — Request completion (when LSP is enabled)
- `F12` — Go to definition
- `Ctrl+L` — Toggle fold at cursor (Tree-sitter)
- `Ctrl+U` — Unfold all

Tip: open the top menu bar with the mouse to discover split commands and additional actions.

## Language Support

### Tree-sitter (syntax highlighting + folding)

Built-in Tree-sitter grammars are included for:

- Rust (`.rs`)
- TOML (`.toml`)
- JSON (`.json`)
- YAML (`.yml`, `.yaml`)
- Python (`.py`)
- JavaScript / JSX (`.js`, `.jsx`)
- TypeScript / TSX (`.ts`, `.tsx`)

Other file types fall back to no syntax highlighting by default.

### LSP (completion/hover/goto)

LSP is opt-in via environment variables.

- Per-language command:
  - `ATTO_EDITOR_LSP_CMD_<LANGID>` (example: `ATTO_EDITOR_LSP_CMD_RUST="rust-analyzer"`)
- Global default command:
  - `ATTO_EDITOR_LSP_CMD` (used when the per-language variable is not set)

Commands are parsed by whitespace (e.g. `rust-analyzer` or `typescript-language-server --stdio`).

### Sublime fallback highlighting

If you have a `.sublime-syntax` file available, you can opt into a fallback syntax engine:

- `ATTO_EDITOR_SUBLIME_SYNTAX_FILE=/absolute/path/to/Rust.sublime-syntax`
- `ATTO_EDITOR_SUBLIME_SYNTAX_INCLUDE_PATHS=/path/one:/path/two` (optional, `:`-separated)

When set, Sublime highlighting is used only for file types that do not have a built-in Tree-sitter
configuration.

## Library Usage

Launch the editor from your own binary:

```rust
use anyhow::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    atto_editor_app::run(atto_editor_app::AttoEditorConfig {
        initial_paths: vec![PathBuf::from(".")],
    })
}
```
