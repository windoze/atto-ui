# Async Integration Notes

> 状态：部分已落地。当前已有标准库通道式后台动作入口：`EventQueue::channel()` 与 `run_crossterm_desktop_with_actions()`；`tokio` / native async-await 集成仍未落地，继续作为可选后续方向。

## Context

The current async pattern is demonstrated in `examples/async_progress.rs`. It uses
`reactive::EventQueue::channel()` and `run_crossterm_desktop_with_actions()` to bridge
background `std::thread` work into the main UI loop through `std::sync::mpsc` actions.
The default `run_crossterm_desktop()` remains input/tick driven; the `with_actions`
variant drains background actions before each draw.

## Observations (from async_progress)

- Background work is bridged through an app-specific action enum and `mpsc::Sender`.
- `run_crossterm_desktop_with_actions()` drains queued actions on the main UI thread
  before drawing, so background updates are visible immediately.
- `examples/async_progress.rs` now reuses the shared crossterm run-loop setup instead
  of owning terminal setup/teardown itself.
- `tests/pty_async_actions.rs` covers deterministic dispatch from a background thread
  into the main UI thread.
- There is still no tokio-backed event stream or native async-await runtime helper in
  `src`; users who need async-await can bring their own runtime and dispatch results
  through the standard-library channel bridge.

## Goals

- Keep the core runtime minimal and dependency-light (no forced tokio dependency).
- Provide a first-class, ergonomic path for background tasks to feed actions into
  the UI loop without copy-pasting a custom event loop.
- Maintain deterministic behavior for PTY tests (avoid nondeterministic scheduling
  or hidden threads unless explicitly requested by the app).

## Integration Options

### Option A: Channel-aware run loop (implemented, std-only)

- `run_crossterm_desktop_with_actions()` accepts an action receiver and an `on_action`
  callback.
- `EventQueue::channel()` exposes `(sender, receiver)` for background workers.
- The run loop drains actions before draw, then uses the existing `tick_rate` poll
  cadence for terminal input.

Pros:
- No new dependencies.
- Keeps async integration in a single place (the run loop).
- Easy migration path for `async_progress` and other examples.

Cons:
- Still thread-based; async/await requires users to bring their own runtime.

### Option B: Feature-gated async runtime (tokio)

- Add an optional `async` feature that provides a tokio runtime helper and a
  crossterm `EventStream` integration (select between terminal events and app
  actions).
- Provide `spawn_async()` / `spawn_blocking()` helpers in a runtime handle that
  dispatches results into the action queue.

Pros:
- Native async/await ergonomics.
- Unified event loop with `select!`-style await on terminal events and app actions.

Cons:
- Adds tokio as an optional dependency and increases API surface.
- Must ensure PTY tests remain deterministic and feature-flagged.

### Option C: Component-level async context

- Expose an `AsyncContext` in `ComponentContext` for spawning background work that
  posts actions back into the app loop.
- Built on Option A or B; primarily a UI-level ergonomic layer.

Pros:
- Components can kick off async tasks without plumbing channels through app code.

Cons:
- Requires careful API design to avoid hidden global state.

## Recommended Direction

Option A is the current implementation. It keeps the core crate dependency-light and
provides a first-class path for thread/background-task results to update UI state on
the main thread. Keep Option B as a feature-gated follow-up if there is strong demand
for native async/await.

## Remaining Follow-up Steps

1. Document the current std-only API in user-facing docs:
   - `EventQueue::channel()` for sender/receiver creation.
   - `run_crossterm_desktop_with_actions()` for main-thread dispatch.
   - App-specific action enums for typed UI updates.

2. Evaluate whether repeated application boilerplate warrants a small runtime handle:
   - `AppHandle::sender()` or `AppHandle::dispatch(action)`.
   - Optional helper `spawn_blocking` that uses `std::thread::spawn` and returns
     results through the existing sender.

3. If native async-await demand appears, design Option B behind a feature gate:
   - Optional tokio runtime helper.
   - Crossterm `EventStream` integration.
   - Deterministic PTY testing strategy for feature-gated async behavior.

## Open Questions

- Should the action queue live in `app` (runtime concern) or `reactive`
  (state-management concern)?
- Do we want a single global action queue per app, or multiple queues per window/
  component?
- Is a feature-gated tokio integration worth the extra API surface, or should we
  stay with thread-based async only?
