# atm — atto terminal multiplexer

`atm` is the atto terminal multiplexer: a full multi-window terminal app built on the reusable [`atto-ui-terminal`](../atto-ui-terminal) components. It runs local PTY sessions inside `atto-ui` windows, renders vt100 output with Ratatui, forwards keyboard and mouse input, and exposes handles for lifecycle, selection, command blocks, configuration, and split-pane orchestration.

It ships two binaries:

- `atm` — the multiplexer app itself.
- `tmux` — a client-side `tmux` shim that translates common tmux subcommands into atto-ui IPC calls against a running `atm`.

## Quick Start

Run the multiplexer with the default shell:

```sh
cargo run -p atm
```

Pass a command to make the first terminal window run that command instead of the shell:

```sh
cargo run -p atm -- top
```

Load a specific config file:

```sh
cargo run -p atm -- --config /path/to/terminal.yaml
```

The deterministic PTY fixtures used by `atto-ui-terminal` tests are still built from that crate:

```sh
cargo run -p atto-ui-terminal --bin snapshot_terminal_app
cargo run -p atto-ui-terminal --bin snapshot_terminal_window_app
```

## Features

| Area | Capabilities |
|---|---|
| Process lifecycle | Exit status tracking, `on_exit`, dead-session prompt, and `R` restart using the window's session profile/cwd. |
| Window shell | Floating terminal windows, title sync from OSC 0/2, Windows menu refresh, minimize/maximize/close, and File menu entries for shell/command windows. |
| Prefix commands | Configurable tmux-style plain `Ctrl+<letter>` prefix, default `Ctrl+B`, with shell actions and literal-prefix escape. |
| Split panes | `Ctrl+B %` splits right, `Ctrl+B "` splits below, `Ctrl+B o` / `Ctrl+B Tab` focuses the next pane, arrow keys select geometrically, `Ctrl+Arrow` resizes, `z` zooms, and `x` closes the active pane. |
| Selection/copy | Mouse selection, `Shift+drag` local selection when child mouse reporting is enabled, copy-mode via `Ctrl+B [`, internal copy buffer, OSC 52, tmux DCS passthrough for OSC 52, and system clipboard backend. |
| Scroll routing | Mouse-reporting apps receive SGR wheel events, alt-screen apps without mouse reporting receive configured scroll keys, and normal shell output uses local scrollback. |
| Command blocks | OSC 133/7 command marks drive command-block presentation, `Ctrl+Up` / `Ctrl+Down` navigation, right-click Rerun / Copy command / Copy output, command exit codes, and cwd inheritance. |
| Rendering fidelity | Wide-character-aware cells, selectable block/underline/bar cursor shapes, application cursor mode, and application keypad encoding. |
| IPC / tmux shim | `TerminalPaneIpc` (in `atto-ui-terminal`) maps pane protocol methods to terminal handles, and the `tmux` shim binary translates common tmux subcommands into those IPC calls. |
| Settings | Visual File -> Settings window for scrollback, palette, prefix/release shortcuts, alt-screen scrolling, profiles/cwd, shell integration, and default cursor shape. |

## Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+B F10` | Open the menu while the terminal is capturing input. |
| `Ctrl+B w` | Enter window-management mode. |
| `Ctrl+B [` | Enter copy-mode. |
| `Ctrl+B ]` | Paste the component copy buffer into the child process. |
| `Ctrl+B Ctrl+B` | Send one literal prefix key to the child process. |
| `Ctrl+B %` | Split the active terminal pane to the right. |
| `Ctrl+B "` | Split the active terminal pane below. |
| `Ctrl+B o` / `Ctrl+B Tab` | Focus the next pane. |
| `Ctrl+B Left/Right/Up/Down` | Select the nearest pane in that direction. |
| `Ctrl+B Ctrl+Left/Right/Up/Down` | Resize the nearest split around the active pane. |
| `Ctrl+B z` | Zoom or restore the active pane. |
| `Ctrl+B x` | Close the active pane when another pane remains. |
| `Ctrl+Up` / `Ctrl+Down` | Jump to the previous or next OSC 133 command block when command marks are available. |
| `R` | Restart a focused dead terminal session after the exit prompt appears. |

Copy-mode keys are `h`/`j`/`k`/`l` or arrows to move, `PageUp`/`PageDown` and `Home`/`End` for larger jumps, `v` or `Space` to start selection, `y` or `Enter` to copy, and `Esc` or `q` to cancel.

When no config file exists, `atm` uses `Ctrl+Shift+L` as its capture-release shortcut so plain `F10` can reach the menu after release. The reusable `TerminalConfig` default (in `atto-ui-terminal`) remains `Ctrl+Shift+Esc`; saved configs can override either shortcut.

## Configuration

`TerminalConfig` (defined in `atto-ui-terminal`) is serializable as JSON or YAML and can be edited visually from File -> Settings. `atm` loads the first configured path in this order:

1. `--config <path>` command-line argument
2. `ATTO_UI_TERMINAL_CONFIG`
3. `$XDG_CONFIG_HOME/atto-ui/terminal.yaml`
4. `~/.config/atto-ui/terminal.yaml`

See [`../atto-ui-terminal/README.md`](../atto-ui-terminal/README.md) for the full YAML schema and supported color specs.

### tmux shim

`tmux.inject` is opt-in. When enabled, spawned child processes receive `$TMUX` formatted as `socket_path,pid,session_id`, `$TMUX_PANE` formatted as `%pane_id`, and `shim_path` is prepended to `PATH` so a built `tmux` shim can intercept supported subcommands. `override_term=true` changes `TERM` to `tmux-256color`; otherwise the terminal keeps the default `xterm-256color`.

Build the shim with:

```sh
cargo build -p atm --bin tmux
```

The shim is a client translator, not a tmux server. It supports `send-keys`, `capture-pane`, `list-panes`, `split-window`, `select-pane`, `break-pane`, and `display-popup`; unsupported subcommands and control mode (`-CC`) fail explicitly.

## Validation

```sh
cargo test -p atm                              # unit + PTY viewer tests
cargo test -p atto-ui-terminal --lib           # component library
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```
