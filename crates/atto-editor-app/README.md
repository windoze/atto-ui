# atto-editor-app

`atto-editor-app` is a terminal editor application built on top of:

- `atto-ui` (window manager + widgets + desktop chrome)
- `atto-ui-editor` (an `editor-core`-powered editor widget)
- `editor-core-treesitter` (syntax highlighting + code folding)
- `editor-core-lsp` (diagnostics, code actions, rename, signature help, formatting, inlay hints, symbols, completion, hover, and goto via language servers)
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

# Open a file in a tab; if no folder was supplied, its parent becomes the workspace root
cargo run -p atto-editor-app -- path/to/file.rs

# Mix folders + files
cargo run -p atto-editor-app -- path/to/project path/to/other/file.py
```

## Features

- Workspace roots (multiple folders) + file explorer window
  - Toggle Explorer visibility (View menu or command palette)
  - Dock Explorer left or right (View menu)
  - Open files from the explorer into tabs
  - `Ctrl+Enter` opens a file in a new window
  - Git status badges, multi-select, context menu actions, inline new/rename, cut/copy/paste, and drag move
- Multi-file editing with window tabs
  - Dirty tabs show a `*` suffix (`file.rs*`)
- Split view for the same file (two views backed by the same document text)
  - Vertical split
  - Horizontal split
- Editing features (from `atto-ui-editor`)
  - Undo/redo, copy/cut/paste, multi-cursor, rectangle selection
  - Find/replace UI with match highlighting
  - Tree-sitter syntax highlighting + code folding
  - Auto-pairs, auto-indent, line movement/duplication, toggle comment, trim trailing whitespace on save
  - LSP diagnostics, code actions, rename, signature help, formatting, inlay hints, document/workspace symbols, completion, hover, and goto when a language server is configured

## Keybindings

### App / Window

- `F10` — Open the menu bar
- `Ctrl+Q` — Quit
- `Ctrl+W` — Enter framework window-management mode
- `F6` — Focus next window
- `Ctrl+Shift+P` — Command Palette
- `Ctrl+P` — File Picker
- `Ctrl+Shift+F` — Global Search
- `Ctrl+Alt+K` — App command prefix / which-key popup

Common command-prefix sequences:

- `Ctrl+Alt+K Ctrl+Alt+A` — Save
- `Ctrl+Alt+K Ctrl+Alt+O` — Open File
- `Ctrl+Alt+K Ctrl+Alt+D` — Open Folder
- `Ctrl+Alt+K Ctrl+Alt+E` — Toggle Explorer
- `Ctrl+Alt+K Ctrl+Alt+L` / `Ctrl+Alt+K Ctrl+Alt+R` — Dock Explorer left / right
- `Ctrl+Alt+K Ctrl+Alt+B` / `Ctrl+Alt+K Ctrl+Alt+H` — Split vertical / horizontal
- `Ctrl+K Ctrl+F` — Format active document

### Explorer Window

- `Enter` — Open the selected file in a new tab
- `Ctrl+Enter` — Open the selected file in a new window
- `Ctrl+Click` / `Shift+Click` — Toggle or extend multi-selection
- Mouse double-click — Open the selected file in a new tab

### Editor (atto-ui-editor)

- `Ctrl+Z` — Undo
- `Ctrl+Y` / `Ctrl+Shift+Z` — Redo
- `Ctrl+C` / `Ctrl+X` / `Ctrl+V` — Copy / Cut / Paste
- `Ctrl+F` — Find
- `Ctrl+H` — Replace
- `F3` / `Shift+F3` — Find next / previous
- `Ctrl+/` — Toggle comment
- `Alt+Up` / `Alt+Down` — Move line up / down
- `Shift+Alt+Down` — Duplicate line
- `Ctrl+Alt+Up` / `Ctrl+Alt+Down` — Add cursor above / below
- `Ctrl+D` / `Ctrl+Shift+L` — Add next / all occurrences
- `Ctrl+Space` — Request completion (when LSP is enabled)
- `Ctrl+Shift+Space` — Signature help
- `F8` / `Shift+F8` — Next / previous diagnostic
- `Ctrl+.` — Code action
- `F2` — Rename symbol
- `F7` — Toggle inlay hints
- `F12` — Go to definition
- `Shift+F12` — Go to references
- `Ctrl+L` — Toggle fold at cursor (Tree-sitter)
- `Ctrl+U` — Unfold all

Tip: use `Ctrl+Shift+P` or the `Ctrl+Alt+K` command prefix to discover commands and their current shortcut labels.

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

### LSP

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
