# Boilerplate Simplification Plan (chatty)

## Problem

All demo binaries under `demos/` repeat a large amount of setup/loop code:

- Terminal setup/cleanup (`enable_raw_mode`, alt screen, mouse capture, bracketed paste, cursor show/hide).
- A draw + poll + read loop.
- Repeated quit logic (`Ctrl+Q` always, `q` only when `EventOutcome::Ignored`).
- Repeated “close window” handling (`DesktopAction::CloseWindow(id) => desktop.wm.close(id)`).
- Per-demo “tick” logic (e.g. draining `EventQueue` for menu actions) has no shared home.

This makes demos noisy and increases the chance of subtle inconsistencies between examples.

## Goals

- Provide a small, public helper API inside `chatty::app` that:
  - Owns terminal setup + cleanup (RAII / best-effort cleanup on drop).
  - Runs a standard desktop event loop.
  - Implements the crate’s default quit behavior.
  - Handles `DesktopAction::CloseWindow` by default.
  - Allows demos (and real apps) to attach:
    - a per-frame `on_tick` hook (for action queues, timers, etc.)
    - a per-event `on_event` hook (for app-level shortcuts after `Desktop` processing)
- Keep the API ergonomic and low-ceremony (demos should shrink significantly).
- Avoid changing existing core behavior: `Desktop` stays the canonical event router; the runner is just glue.

## Proposed API

Add a new module: `chatty::app::run`:

- `CrosstermAppConfig`:
  - `tick_rate: Duration`
  - `enable_mouse_capture: bool`
  - `enable_bracketed_paste: bool`
  - `cursor: CursorMode` (`Show` | `Hide`)
- `AppControl` enum:
  - `Continue`
  - `Exit`
- `run_crossterm_desktop(...)`:
  - Initializes terminal according to config
  - Calls `build(screen) -> Desktop`
  - Main loop:
    1. `on_tick(&mut Desktop, screen)`
    2. `desktop.draw(...)`
    3. poll for input (`tick_rate`)
    4. read event, call `desktop.handle_event(&event, screen)`
    5. auto-handle `DesktopAction::CloseWindow`
    6. apply default quit keys
    7. call `on_event(&mut Desktop, &event, screen, &result)`
- `run_crossterm_desktop_simple(config, build)` convenience wrapper with no hooks.

## Migration Plan (Demos)

For each demo `demos/*/main.rs`:

- Replace the explicit terminal init/cleanup and event loop with a single call to
  `chatty::app::run_crossterm_desktop{_simple}`.
- Keep demo-specific logic in:
  - the `build` closure (add windows, focus initial window)
  - `on_tick` (drain action queues like demo-04)
  - `on_event` (app-level shortcuts like demo-02 and demo-07)
- Preserve existing behavior per demo by setting `CrosstermAppConfig`:
  - demos that currently hide cursor keep `CursorMode::Hide`
  - demos that enable bracketed paste keep it enabled
  - demos that previously used a 50ms poll keep a 50ms tick rate

## Validation

- `cargo fmt`
- `cargo test` (demos compile as bins; PTY tests still run)

