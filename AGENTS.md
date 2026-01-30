# Repository Guidelines

## Project Structure & Module Organization

- `src/` — main `chatty` library crate
  - `src/app/` — desktop chrome (menubar/status bar) + app orchestration
  - `src/wm/` — window model + window manager (focus/z-order/move/resize)
  - `src/widgets/` — built-in widgets (`TextBox`, `Button`, `ListBox`, etc.)
  - `src/text/` — Unicode/grapheme-aware text editing primitives
  - `src/theme/` — theming/styling
  - `src/view.rs` — `View` trait for window content
- `src/bin/snapshot_app.rs` — deterministic test target used by PTY tests
- `crates/chatty-test-host/` — PTY runner + `vt100` screen parser for integration tests
- `tests/` — end-to-end/PTY integration tests
- `examples/demo.rs` — interactive demo showcasing multi-window behavior

## Build, Test, and Development Commands

- `cargo build` — compile library and all targets.
- `cargo test` — run unit tests + PTY integration tests (`tests/pty_*.rs`).
- `cargo run --example demo` — launch the interactive demo.
- `cargo run --bin snapshot_app` — run the deterministic app used by PTY tests.
- `cargo fmt` / `cargo clippy` — formatting and linting (recommended before PRs).

## Coding Style & Naming Conventions

- Rust edition: 2024. Keep `unsafe` out of the codebase (`#![forbid(unsafe_code)]`).
- Follow `rustfmt` defaults; prefer small, focused modules.
- Naming: `snake_case` for files/modules/functions, `CamelCase` for types/traits.
- Keep the public API intentional: prefer re-exports from `src/lib.rs` and avoid leaking internals.

## Testing Guidelines

- Unit tests live next to code (`#[cfg(test)]`).
- UI behavior should be validated via PTY tests using `chatty_test_host::PtyTestHost`.
- Use fixed terminal sizes, deterministic input scripts, and `wait_for_text(...)` instead of ad-hoc sleeps.

## Commit & Pull Request Guidelines

- Commit messages use imperative present tense (examples in history: “Add …”, “Fix …”, “Implement …”).
- Make small commits per bug/feature; each commit should pass `cargo test`.
- PRs should include: what changed, how to test, and (for UI changes) a PTY buffer snippet or screenshot.
- Update `IMPLEMENTATION_PLAN.md` when milestone status changes.

