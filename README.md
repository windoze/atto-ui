# atto-ui

`atto-ui` is a multi-window terminal UI framework built on Crossterm and Ratatui. The repository contains the Rust runtime, a Node N-API binding, a typed `@atto-ui/core` JavaScript facade, and a React reconciler package for JSX-driven terminal apps.

## Packages

| Package | Purpose |
|---|---|
| `atto-ui` | Core Rust crate with desktop chrome, window management, widgets, runtime component trees, themes, and PTY-testable app hosting. |
| `crates/atto-ui-node` / `@atto-ui/node` | Native N-API binding exposing `AppHost` to JavaScript runtimes. |
| `packages/core` / `@atto-ui/core` | Typed CommonJS facade, native loader, low-level spec builders, and runtime types. |
| `packages/react` / `@atto-ui/react` | React reconciler, JSX host components, event bridge, and `render()` loop. |
| `crates/atto-ui-node/npm/*` | Platform binary npm packages used by optional dependencies. |
| `crates/atto-editor-app` | Multi-window terminal editor app with Explorer, tabs, split views, command palette, file/symbol/search pickers, and LSP-backed editor features. |

## Requirements

- Rust stable with edition 2024 support.
- Node.js 22 or newer for local JavaScript tests.
- Python 3 for PTY integration tests.
- Bun and Deno are optional locally, but CI validates both runtimes.

## Rust Quick Start

```sh
cargo build
cargo run --example demo
cargo test --all --all-targets
```

Useful validation commands:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --all --all-targets
```

The deterministic test target used by PTY tests is available with:

```sh
cargo run --bin snapshot_app
```

### Themes

The built-in theme presets are `dark`, `light`, and `turbo`. `turbo` provides the classic Turbo Vision palette with a blue desktop, gray dialog surfaces, cyan menu/status bars, and green selection highlights. Theme JSON/YAML files can opt into a preset before applying overlays:

```yaml
base: turbo
colors:
  widget-accent:
    fg: yellow
```

## Editor App Quick Start

Launch the editor app with optional files and folders. Folders become workspace roots; files open as tabs. If no folder is supplied, the first file's parent folder becomes the workspace root.

```sh
cargo run -p atto-editor-app -- .
cargo run -p atto-editor-app -- path/to/file.rs path/to/project
```

Key entry points:

| Shortcut | Action |
|---|---|
| `F10` | Open the menu bar. |
| `Ctrl+Q` | Quit the terminal app. |
| `Ctrl+Shift+P` | Open the command palette. |
| `Ctrl+P` | Open the file picker. |
| `Ctrl+Shift+F` | Open global workspace search. |
| `Ctrl+Alt+K` | Start the app command prefix and show which-key choices. |
| `F8` / `Shift+F8` | Jump to next / previous diagnostic. |
| `Ctrl+.` | Request LSP code actions. |
| `F2` | Rename symbol. |
| `Ctrl+Shift+Space` | Request signature help. |
| `Ctrl+K Ctrl+F` | Format the active document. |
| `F7` | Toggle inlay hints. |

LSP support is opt-in. Set `ATTO_EDITOR_LSP_CMD_<LANGID>` (for example `ATTO_EDITOR_LSP_CMD_RUST="rust-analyzer"`) or the fallback `ATTO_EDITOR_LSP_CMD`; commands are split on whitespace.

## JavaScript Quick Start

Build the local native binding before running JS examples or tests from a checkout:

```sh
npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --cwd crates/atto-ui-node --platform
```

Low-level `@atto-ui/core` usage:

```js
const { AppHost, Button, Text, VStack } = require('@atto-ui/core')

const host = new AppHost({ headless: true, cols: 60, rows: 16 })
const callback = host.allocCallback()
const windowId = host.addDynamicWindow('Hello', [1, 1, 40, 8], VStack({ id: 'root' }, [
  Text('Hello from atto-ui', { id: 'title' }),
  Button({ id: 'ok', text: 'OK', onClick: callback }),
]))

host.step()
host.sendEvent(windowId, { type: 'key', key: 'enter' })
console.log(host.drainCallbacks())
host.dispose()
```

React usage:

```js
const React = require('react')
const { Button, Text, VStack, render } = require('@atto-ui/react')

function App() {
  const [count, setCount] = React.useState(0)
  return React.createElement(VStack, null,
    React.createElement(Text, null, `Count: ${count}`),
    React.createElement(Button, { onClick: () => setCount((value) => value + 1) }, 'Increment'),
  )
}

const handle = render(React.createElement(App), { singleWindow: true })
process.once('SIGINT', () => handle.stop())
```

## JavaScript Validation

```sh
npm run typecheck --prefix packages/core
npm test --prefix packages/core
npm run typecheck --prefix packages/react
npm test --prefix packages/react
```

Runtime compatibility smoke tests:

```sh
npm run test:runtime:node --prefix packages/core
npm run test:runtime:bun --prefix packages/core
npm run test:runtime:deno --prefix packages/core
```

The Deno smoke requires `--allow-read --allow-env --allow-run --allow-ffi`; the package script supplies those permissions. On POSIX platforms, the Bun and Deno PTY smokes start a real raw-mode terminal app and assert that alternate screen, cursor, mouse capture, and terminal flags are restored on exit. The PTY raw-mode smoke is skipped on Windows.

## Documentation

- `docs/NODE_API.md` documents the Node binding, `@atto-ui/core`, React package, component spec shape, events, and runtime compatibility notes.
- `docs/RELEASE.md` documents CI coverage and the tag-based npm release workflow.
- `NODE_BINDING.md` is the design record for the Node binding and React host architecture.

## CI And Release

CI runs Rust formatting, clippy, full Rust tests, native N-API build, Node/Core/React tests, React PTY/e2e coverage, Bun/Deno compatibility smokes, and npm pack dry-runs.

Publishing is tag-based. Pushing a `v*` tag runs the release workflow, first repeats the full Linux CI gate, then builds platform `.node` artifacts on macOS/Linux/Windows, verifies package contents, and publishes platform packages, `@atto-ui/node`, `@atto-ui/core`, and `@atto-ui/react` using `NPM_TOKEN`.
