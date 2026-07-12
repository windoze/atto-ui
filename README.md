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
| `crates/atto-agent-app` | Single-window TUI agent app built on `atto-ui-chat`, with DeepSeek protocol/client modules, local tools, skills, plan mode, context compaction, and deterministic mock PTY fixtures. |
| `crates/atto-ui-terminal` | Full-featured terminal emulator component and `terminal_viewer` demo with multi-window sessions, split panes, copy-mode, command blocks, and a visual settings window. |

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

## Terminal App Quick Start

Launch the full terminal viewer demo with your login shell, or pass a command to make the initial terminal window run that command:

```sh
cargo run -p atto-ui-terminal --example terminal_viewer
cargo run -p atto-ui-terminal --example terminal_viewer -- top
```

The viewer supports floating terminal windows, tmux-style split panes, dead-session restart, OSC title sync, command-block navigation for OSC 133/7 shell integration, local selection/copy-mode, OSC 52/system clipboard integration, alt-screen wheel routing, and a File -> Settings window backed by JSON/YAML `TerminalConfig`.

Key defaults: `Ctrl+B` is the terminal prefix, `Ctrl+B [` enters copy-mode, `Ctrl+B %` / `Ctrl+B "` split panes, `Ctrl+B o` focuses the next pane, and `Ctrl+B F10` opens the menu while capture is active. Without a saved config, `terminal_viewer` uses `Ctrl+Shift+L` to release capture so plain `F10` can reach the menu; saved configs can change the release shortcut.

Configuration is loaded from `ATTO_UI_TERMINAL_CONFIG` when set, then `$XDG_CONFIG_HOME/atto-ui/terminal.yaml`, then `~/.config/atto-ui/terminal.yaml`. See `crates/atto-ui-terminal/README.md` for feature details, config examples, and focused validation commands.

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

## Agent App Quick Start

Launch the agent app from a checkout:

```sh
cargo run -p atto-agent-app -- --workspace .
```

The app selects `provider: deepseek` when an API key is configured and `--mock` is not set; otherwise it selects `provider: mock` and shows a startup notice with next steps unless mock mode was explicitly forced. Live DeepSeek turns stream over HTTP/SSE through the same UI action path as the deterministic mock provider, including structured error display; default tests still avoid external network access. The ignored real-API smoke test can be run manually with:

```sh
DEEPSEEK_API_KEY=... cargo test -p atto-agent-app --test deepseek_real_smoke -- --ignored
```

Useful runtime options include `--api-key`, `--base-url`, `--model`, `--temperature`, `--max-tokens`, `--workspace`, `--plan-mode`, `--config`, `--transcript`, and `--mock`. Configuration may also come from `DEEPSEEK_*` / `ATTO_AGENT_*` environment variables, workspace `.atto-agent.toml`, and `~/.config/atto-agent/config.toml`.

See `crates/atto-agent-app/README.md` for slash commands, tool and skill behavior, transcript persistence, and validation notes.

## JavaScript Quick Start

Build the local native binding before running JS examples or tests from a checkout:

```sh
npm run build --prefix crates/atto-ui-node
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
- `crates/atto-ui-terminal/README.md` documents the terminal emulator component, full terminal viewer demo, settings model, shortcuts, and terminal-specific validation commands.
- `docs/RELEASE.md` documents CI coverage, the tag-based npm release workflow, and workspace-only app release scope.
- `NODE_BINDING.md` is the design record for the Node binding and React host architecture.

## CI And Release

CI runs Rust formatting, clippy, full Rust tests, native N-API build, Node/Core/React tests, React PTY/e2e coverage, Bun/Deno compatibility smokes, and npm pack dry-runs.

Publishing is tag-based. Pushing a `v*` tag runs the release workflow, first repeats the full Linux CI gate, then builds platform `.node` artifacts on macOS/Linux/Windows, verifies package contents, and publishes platform packages, `@atto-ui/node`, `@atto-ui/core`, and `@atto-ui/react` using `NPM_TOKEN`.
