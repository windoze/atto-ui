# atto-ui-terminal

`atto-ui-terminal` provides the terminal emulator component used by Atto UI apps plus a full `terminal_viewer` demo. It runs local PTY sessions inside `atto-ui` windows, renders vt100 output with Ratatui, forwards keyboard and mouse input, and exposes handles for lifecycle, selection, command blocks, configuration, and split-pane orchestration.

## Quick Start

Run the terminal viewer with the default shell:

```sh
cargo run -p atto-ui-terminal --example terminal_viewer
```

Pass a command to make the first terminal window run that command instead of the shell:

```sh
cargo run -p atto-ui-terminal --example terminal_viewer -- top
```

The deterministic PTY fixtures used by tests are:

```sh
cargo run -p atto-ui-terminal --bin snapshot_terminal_app
cargo run -p atto-ui-terminal --bin snapshot_terminal_window_app
```

## Terminal Viewer Features

The viewer is a complete multi-window terminal app built on the reusable `TerminalEmulator` and `TerminalPaneGroup` components:

| Area | Capabilities |
|---|---|
| Process lifecycle | Exit status tracking, `on_exit`, dead-session prompt, and `R` restart using the window's session profile/cwd. |
| Window shell | Floating terminal windows, title sync from OSC 0/2, Windows menu refresh, minimize/maximize/close, and File menu entries for shell/command windows. |
| Prefix commands | Configurable tmux-style plain `Ctrl+<letter>` prefix, default `Ctrl+B`, with shell actions and literal-prefix escape. |
| Split panes | `Ctrl+B %` splits right, `Ctrl+B "` splits below, and `Ctrl+B o` / `Ctrl+B Tab` focuses the next pane inside the current window. |
| Selection/copy | Mouse selection, `Shift+drag` local selection when child mouse reporting is enabled, copy-mode via `Ctrl+B [`, internal copy buffer, OSC 52, and system clipboard backend. |
| Scroll routing | Mouse-reporting apps receive SGR wheel events, alt-screen apps without mouse reporting receive configured scroll keys, and normal shell output uses local scrollback. |
| Command blocks | OSC 133/7 command marks drive command-block presentation, `Ctrl+Up` / `Ctrl+Down` navigation, right-click Rerun / Copy command / Copy output, command exit codes, and cwd inheritance. |
| Rendering fidelity | Wide-character-aware cells, selectable block/underline/bar cursor shapes, application cursor mode, and application keypad encoding. |
| Settings | Visual File -> Settings window for scrollback, palette, prefix/release shortcuts, alt-screen scrolling, profiles/cwd, shell integration, and default cursor shape. |

## Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+B F10` | Open the menu while the terminal is capturing input. |
| `Ctrl+B w` | Enter window-management mode. |
| `Ctrl+B z` | Maximize or restore the focused terminal window. |
| `Ctrl+B [` | Enter copy-mode. |
| `Ctrl+B ]` | Paste the component copy buffer into the child process. |
| `Ctrl+B Ctrl+B` | Send one literal prefix key to the child process. |
| `Ctrl+B %` | Split the active terminal pane to the right. |
| `Ctrl+B "` | Split the active terminal pane below. |
| `Ctrl+B o` / `Ctrl+B Tab` | Focus the next pane. |
| `Ctrl+Up` / `Ctrl+Down` | Jump to the previous or next OSC 133 command block when command marks are available. |
| `R` | Restart a focused dead terminal session after the exit prompt appears. |

Copy-mode keys are `h`/`j`/`k`/`l` or arrows to move, `PageUp`/`PageDown` and `Home`/`End` for larger jumps, `v` or `Space` to start selection, `y` or `Enter` to copy, and `Esc` or `q` to cancel.

When no config file exists, `terminal_viewer` uses `Ctrl+Shift+L` as its capture-release shortcut so plain `F10` can reach the menu after release. The reusable `TerminalConfig` default remains `Ctrl+Shift+Esc`; saved viewer configs can override either shortcut.

## Configuration

`TerminalConfig` is serializable as JSON or YAML and can be edited visually from File -> Settings. The viewer loads the first configured path in this order:

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

## Component Usage

Use `TerminalEmulator::from_config(&config)?` for a single terminal widget, or wrap it in `TerminalPaneGroup` for split panes. `TerminalHandle` exposes running state, process exit status, title/bell/clipboard callbacks, scrollback, selection/copy helpers, command-block queries, live config application, and PTY resize.

```rust
use atto_ui_terminal::{TerminalConfig, TerminalEmulator, TerminalSessionSpec};

let config = TerminalConfig::default();
let mut terminal = TerminalEmulator::from_config(&config)?;
terminal.spawn_session(&TerminalSessionSpec::shell_from_env())?;
let handle = terminal.handle();
```

## Validation

Focused terminal validation:

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
