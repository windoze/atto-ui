# atto-ui-terminal

`atto-ui-terminal` provides the reusable terminal emulator component used by Atto UI apps. It runs local PTY sessions, renders vt100 output with Ratatui, forwards keyboard and mouse input, and exposes handles for lifecycle, selection, command blocks, configuration, and split-pane orchestration.

The component powers [`atm`](../atm), the atto terminal multiplexer app — see that crate for the end-user binary, the `tmux` shim, and the shortcut reference. This crate is the library plus deterministic PTY test fixtures; it no longer ships the viewer app or the `tmux` shim.

## Quick Start

```rust
use atto_ui_terminal::{TerminalConfig, TerminalEmulator, TerminalSessionSpec};

let config = TerminalConfig::default();
let mut terminal = TerminalEmulator::from_config(&config)?;
terminal.spawn_session(&TerminalSessionSpec::shell_from_env())?;
let handle = terminal.handle();
```

Use `TerminalEmulator::from_config(&config)?` for a single terminal widget, or wrap it in `TerminalPaneGroup` for split panes. `TerminalHandle` exposes running state, process exit status, title/bell/clipboard callbacks, scrollback, selection/copy helpers, command-block queries, live config application, and PTY resize.

The deterministic PTY fixtures used by tests are:

```sh
cargo run -p atto-ui-terminal --bin snapshot_terminal_app
cargo run -p atto-ui-terminal --bin snapshot_terminal_window_app
```

## Component Capabilities

| Area | Capabilities |
|---|---|
| Process lifecycle | Exit status tracking, `on_exit`, and session specs carrying profile/cwd for restart. |
| Prefix commands | Configurable tmux-style plain `Ctrl+<letter>` prefix with shell actions and literal-prefix escape. |
| Split panes | `TerminalPaneGroup` maintains a pane tree, layout, active pane, and pane handle snapshots inside a single `Window` view. |
| Selection/copy | Mouse selection, `Shift+drag` local selection when child mouse reporting is enabled, copy-mode, internal copy buffer, OSC 52, tmux DCS passthrough for OSC 52, and a system clipboard backend. |
| Scroll routing | Mouse-reporting apps receive SGR wheel events, alt-screen apps without mouse reporting receive configured scroll keys, and normal shell output uses local scrollback. |
| Command blocks | OSC 133/7 command marks drive command-block presentation, navigation, and exit codes. |
| Rendering fidelity | Wide-character-aware cells, selectable block/underline/bar cursor shapes, application cursor mode, and application keypad encoding. |
| IPC | `TerminalPaneIpc` maps pane protocol methods to terminal handles for exposure over the core IPC server. |

## Configuration

`TerminalConfig` is serializable as JSON or YAML and can be edited visually from a host app's settings UI. The host loads it from the first configured path in this order:

1. `ATTO_UI_TERMINAL_CONFIG`
2. `$XDG_CONFIG_HOME/atto-ui/terminal.yaml`
3. `~/.config/atto-ui/terminal.yaml`

Minimal YAML example:

```yaml
scrollback_len: 4000
prefix_key:
  key: a
  modifiers: [control]
release_shortcut:
  key: l
  modifiers: [control, shift]
alternate_screen_scroll:
  enabled: true
  step: 3
  scroll_up_key: { key: up }
  scroll_down_key: { key: down }
shell_integration:
  inject: false
tmux:
  inject: false
  socket_path: /tmp/atto-ui-tmux.sock
  shim_path: target/debug
  override_term: false
cursor:
  default_shape: block
sessions:
  default_profile: Shell
  profiles:
    - name: Shell
      command: /bin/sh
palette:
  foreground: white
  background: black
  ansi:
    - black
    - red
    - green
    - yellow
    - blue
    - magenta
    - cyan
    - gray
    - dark_gray
    - light_red
    - light_green
    - light_yellow
    - light_blue
    - light_magenta
    - light_cyan
    - white
```

Supported color specs are Ratatui color names, `#rgb`, `#rrggbb`, and `indexed:<n>`. Prefix keys must be plain `Ctrl+<ASCII letter>` so they work reliably in traditional terminal byte streams.

`tmux.inject` is opt-in. When enabled, spawned child processes receive `$TMUX` formatted as `socket_path,pid,session_id`, `$TMUX_PANE` formatted as `%pane_id`, and `shim_path` is prepended to `PATH` so a built `tmux` shim (shipped by [`atm`](../atm)) can intercept supported subcommands. `override_term=true` changes `TERM` to `tmux-256color`; otherwise the terminal keeps the default `xterm-256color`.

## IPC Integration

To expose pane operations over the core IPC server, register `terminal_pane_ipc_handler(TerminalPaneIpc::new(group_handle))` on the `IpcServer` that is drained by your app. The handler maps pane protocol methods to `send_input_bytes`, `snapshot`, `panes`, split/select, `break_pane`, and floating popup window creation. Without that handler, generic IPC methods such as `query`, `invoke`, and `tree` still work, but terminal pane methods return `ActionNotSupported`.

## Validation

Focused component validation:

```sh
cargo test -p atto-ui-terminal --lib
cargo test -p atto-ui-terminal --test input_encoding
cargo test -p atto-ui-terminal --test callbacks
cargo test -p atto-ui-terminal --test pty_terminal_window_interactions -- --nocapture
```

Workspace gate:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```
