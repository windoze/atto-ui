# Chatty — Implementation Plan

This document breaks down development into milestones with concrete deliverables and test/validation steps.

## Principles

- **Incremental, testable slices:** each milestone must compile and have automated validation.
- **Deterministic rendering tests:** prefer PTY-backed, screen-buffer snapshot tests for end-to-end behavior.
- **Modular architecture:** low-level primitives (views/widgets/themes) build up to a window manager and desktop UI.
- **Turbo Vision-inspired UX:** keyboard-driven window/menu operations with mouse support layered on.

## Repository Layout (target)

- `src/` — `chatty` library crate
  - `app/` — run-loop helpers and top-level desktop integration
  - `wm/` — window manager (z-order, focus, move/resize, modal)
  - `views/` — view traits + common built-in views (optional)
  - `widgets/` — reusable widgets (button, textbox, checkbox, …)
  - `theme/` — theming/styling
  - `text/` — Unicode-aware text editing helpers (IME/paste-friendly)
- `crates/chatty-test-host/` — PTY test host library for integration tests
- `tests/` — end-to-end tests using PTY host + a test binary
- `examples/` — demo application showcasing capabilities

## Test Strategy

### Unit-level

- Pure logic tests for:
  - window manager state transitions (focus, z-order changes)
  - geometry clamping (move/resize within desktop bounds)
  - Unicode text buffer editing (insert/delete, cursor movement by grapheme)

### End-to-end (required)

Create a **test-host app** that:

1. Allocates a **PTY** with a fixed terminal size.
2. Runs a TUI binary connected to that PTY.
3. Captures output and parses it into a **screen buffer**.
4. Simulates user input:
   - keyboard sequences (arrows, function keys, modifiers where possible)
   - mouse sequences (xterm SGR mouse)
   - bracketed paste sequences to simulate IME/paste input

Tests assert against:

- **presence checks** (e.g., window titles visible),
- **state-driven snapshots** (stable expected screen text after scripted inputs),
- **behavior checks** (e.g., focused window changes after click).

### Manual validation

- `cargo run --example demo`
- Verify:
  - moving/resizing via keyboard + mouse drag
  - z-order changes on focus
  - modal windows block interaction
  - paste inserts Unicode text correctly

## Milestones

### M0 — Bootstrap (crate + tooling)

**Deliverables**

- Cargo crate initialized
- Basic CI-friendly commands documented (build/test)

**Validation**

- `cargo build`
- `cargo test`

**Progress**

- [x] Complete
- Notes:
  - Cargo workspace created for `chatty` + `crates/chatty-test-host`
  - Smoke builds/tests: `cargo build`, `cargo test`

---

### M1 — Core API + Theme + Text

**Deliverables**

- `View` trait (render + event handling)
- `Theme` struct with styles for desktop/window/widget primitives
- Unicode-aware `TextBuffer` for textbox editing and paste/IME-like input

**Tests**

- Unit tests for `TextBuffer` (grapheme cursor movement; delete/backspace behavior)

**Validation**

- `cargo test`

**Progress**

- [x] Complete
- Notes:
  - `src/view.rs`: `View` trait + `ViewAction`
  - `src/theme/mod.rs`: `Theme` (dark/light)
  - `src/text/buffer.rs`: grapheme-aware `TextBuffer` + unit tests

---

### M2 — Window System (z-order, focus, move/resize)

**Deliverables**

- `Window` model:
  - kinds: normal/floating/modal/tooltip
  - states: normal/minimized/maximized/closed
- `WindowManager`:
  - create/destroy windows
  - focus + z-order maintenance
  - keyboard-driven move/resize
  - mouse focus + titlebar dragging

**Tests**

- Unit tests for window manager state transitions (no rendering)

**Validation**

- `cargo test`

**Progress**

- [x] Complete
- Notes:
  - `src/wm/window.rs`: window model (kind/state/decorations)
  - `src/wm/manager.rs`: z-order, focus, move/resize, mouse drag/titlebar buttons + unit tests

---

### M3 — Rendering + Desktop Chrome (decorations, menu, status)

**Deliverables**

- Window decorations:
  - border + title bar
  - control buttons (min/max/close)
  - optional drop shadow
- Desktop chrome:
  - menubar (F10 activation, nested menus)
  - status bar (dynamic hints + state)

**Tests**

- PTY-backed snapshot tests for:
  - correct window layering / z-order
  - title bars and borders
  - menu activation and navigation

**Validation**

- `cargo test`

**Progress**

- [x] Complete
- Notes:
  - `src/app/desktop.rs`: desktop layout + mode switching (normal/menu/window)
  - `src/app/menu.rs`: nested menus (F10) + command dispatch
  - `src/app/status.rs`: status bar rendering

---

### M4 — Widgets (button, label, textbox, checkbox, radio, list, table)

**Deliverables**

- Reusable widgets with basic focus + keyboard interaction
- A simple container (`Form`) to manage focus traversal (Tab/Shift-Tab)

**Tests**

- PTY-backed snapshot tests verifying:
  - checkbox toggles
  - textbox accepts paste (including Unicode)
  - list selection moves with arrows

**Validation**

- `cargo test`

**Progress**

- [x] Complete
- Notes:
  - `src/widgets/`: `Label`, `TextBox` (paste + Unicode), `Checkbox`, `RadioGroup`, `ListBox`, `TableView`, `Button`
  - `src/widgets/primitives.rs`: `Form` for focus traversal (Tab/Shift-Tab)

---

### M5 — Test Host + Demo App

**Deliverables**

- `crates/chatty-test-host`:
  - spawn a binary in a PTY
  - read output + parse to screen buffer
  - send keyboard/mouse/paste sequences
- `examples/demo.rs`:
  - multi-window desktop
  - modal dialog + tooltip
  - menubar + status bar
  - theming toggle
  - widgets showcase

**Tests**

- At least 2 PTY end-to-end tests:
  - window movement/z-order and close
  - paste event into textbox and Unicode rendering

**Validation**

- `cargo test`
- `cargo run --example demo` (manual)

**Progress**

- [x] Complete
- Notes:
  - `crates/chatty-test-host`: PTY runner + vt100 screen parsing + keyboard/mouse/paste helpers
  - `tests/pty_desktop.rs`: PTY integration tests (paste, window move, mouse focus)
  - `src/bin/snapshot_app.rs`: deterministic test binary
  - `examples/demo.rs`: interactive demo showcasing features

---

## Versioning Targets (suggested)

- `0.1.0` — MVP desktop + windows + a few widgets + PTY tests
- `0.2.0` — richer widget set + improved mouse handling + better menu system
- `0.3.0` — layout containers, docking, more Turbo Vision-like dialogs
