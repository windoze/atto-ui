# Async Integration Plan

## Context

The current async pattern is demonstrated in `examples/async_progress.rs`. It uses a
manual event loop with `std::sync::mpsc::channel`, `try_recv`, and `event::poll` to
interleave terminal input with background updates. We also have `reactive::EventQueue`
and `EventQueue::channel()` plus `drain_channel()` in `src/reactive/queue.rs`, but
`run_crossterm_desktop()` does not integrate any background action channel.

## Observations (from async_progress)

- The example re-implements terminal setup/teardown and event loop logic that already
  exists in `src/app/run.rs`.
- Background work is bridged through an `AppAction` enum and `mpsc::Sender`.
- The event loop uses a dynamic timeout to avoid busy polling when idle.
- There is no first-class API for spawning async work or dispatching results into
  the main UI loop; each app rolls its own.

## Goals

- Keep the core runtime minimal and dependency-light (no forced tokio dependency).
- Provide a first-class, ergonomic path for background tasks to feed actions into
  the UI loop without copy-pasting a custom event loop.
- Maintain deterministic behavior for PTY tests (avoid nondeterministic scheduling
  or hidden threads unless explicitly requested by the app).

## Integration Options

### Option A: Channel-aware run loop (minimal, std-only)

- Extend `run_crossterm_desktop()` with an optional action receiver (or a small
  helper wrapper like `run_crossterm_desktop_with_actions`).
- Provide a tiny helper type (e.g., `AppActionQueue<T>`) that exposes
  `(sender, receiver)` and `drain()` for the loop.
- In the run loop: drain actions before draw, and use a timeout that is the min of
  `tick_rate` and an action-receiver timeout to avoid busy polling.

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

Start with Option A to provide a zero-dependency, channel-aware run loop that
matches the `async_progress` pattern but lives in `src/app/run.rs`. This makes
async updates first-class while keeping the crate lightweight. Keep Option B as a
feature-gated follow-up if there is strong demand for native async/await.

## Proposed Steps (No code changes yet)

1. Define the desired API surface:
   - A small action-queue type (or reuse `EventQueue::channel`) and a run-loop
     entry point that accepts a receiver and drains it each tick.
   - Decide whether actions are typed at the app level (`AppAction`) or whether
     the API stays generic (`T: Send + 'static`).

2. Update `run_crossterm_desktop()` (or add a new helper) to:
   - Drain pending actions before `terminal.draw()`.
   - Use a timeout strategy similar to `async_progress` (fast poll when actions
     are pending; normal tick rate when idle).
   - Expose a hook to let the app translate actions into UI state changes.

3. Provide a small runtime handle for background tasks:
   - `AppHandle::sender()` or `AppHandle::dispatch(action)`.
   - Optional helper `spawn_blocking` that uses `std::thread::spawn` and returns
     results via the sender.

4. Migrate `examples/async_progress.rs` to the new API to validate ergonomics.

5. Update documentation:
   - `docs/ASYNC_TASKS.md` to prefer the new run-loop integration.
   - Add a short section to `README.md` or a new doc that explains the default
     (std-only) async pattern and the optional tokio feature (if added later).

## Open Questions

- Should the action queue live in `app` (runtime concern) or `reactive`
  (state-management concern)?
- Do we want a single global action queue per app, or multiple queues per window/
  component?
- Is a feature-gated tokio integration worth the extra API surface, or should we
  stay with thread-based async only?

