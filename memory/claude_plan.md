# Claude Execution Plan

## Scope

- Work from `TODO.md` as the authoritative task list.
- Complete exactly the first task whose title is not prefixed with `[DONE]`.
- Do not proceed to later tasks after completing or scheduling a blocker for the current task.
- Keep this file updated when the plan changes or key steps complete.

## Execution Steps

1. Read `TODO.md` and identify the first incomplete task by title prefix.
2. Inspect the latest commit only for unfinished work directly relevant to that task.
3. Read the files and tests needed to understand the selected task.
4. Implement the task directly, without narrowing scope or using workaround behavior.
5. If a concrete prerequisite or blocking spec mismatch is found, update `TODO.md` with the minimum required prerequisite task, keep the current task incomplete, commit, and stop.
6. Run required validation in the requested order: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required.
7. Address any unscheduled failing tests or fixtures before marking the task complete.
8. Mark the completed task title in `TODO.md` with `[DONE]` and update its completion record.
9. Review `git status`, `git diff`, and recent log, then commit all intended changes with a clear task-specific message.
10. Stop after the single task is completed and committed.

## Current Status

- `TODO.md` read. The first incomplete task is `NT13` from `TODO-1.md`.
- `NT13` scope: implement virtual `DesktopContainer`, `<Window>` host nodes, window lifecycle routing, window prop updates, per-window op bucketing, and `singleWindow:true` auto wrapping for `@atto-ui/react`.
- Latest commit checked: `[NR12] Review React text components`; no unfinished NT13 note was found.
- Existing React code is currently single-window: `createRoot(host, windowId)` owns one runtime window and `flushStaticTree()` sends all ops to that one `windowId`.
- Native AppHost already supports window lifecycle methods, but does not expose desktop chrome setters for MenuBar/StatusBar; to avoid a spec-deviating no-op, NT13 implementation will include minimal native `setMenuBar` / `setStatusBar` support and matching TS types.

## Detailed Implementation Plan

1. Extend `packages/react/src/host.ts` with desktop/window container modes, virtual host node detection, window root `set_tree` flushes, and per-window pending-op buckets.
2. Add desktop root lifecycle: direct root children must be `Window`, `MenuBar`, or `StatusBar`; `Window` add/remove maps to `addDynamicWindow`/`closeWindow`; `Window` prop updates map to `setTitle`/`moveWindow`/`resizeWindow`.
3. Add React wrappers in a new `packages/react/src/desktop.ts` for `Window`, `MenuBar`, `Menu`, `MenuItem`, and `StatusBar`.
4. Add `createDesktopRoot()` and update `render()` so default/single-window mode auto-wraps the user tree in a full-screen `Window`; `singleWindow:false` renders the user-provided desktop tree directly.
5. Add native/core support for `setMenuBar` and `setStatusBar`, including callback emission from menu items.
6. Add reconciler/headless/render tests for multi-window op bucketing, window lifecycle/title/rect updates, context sharing, single-window auto wrapping, and basic MenuBar/StatusBar lowering.
7. Run the required validation sequence, update `TODO.md`, commit, and stop.

## Progress Update

- Implemented desktop/window container modes in `packages/react/src/host.ts` with virtual `Window`, `MenuBar`, `Menu`, `MenuItem`, and `StatusBar` handling.
- Added `packages/react/src/desktop.ts` wrappers and `createDesktopRoot()`; `render()` now uses the virtual desktop root and auto-wraps single-window apps.
- Added native/core `setMenuBar` and `setStatusBar`; custom status text no longer gets overwritten by default desktop status text.
- Added JS/native/type tests for multi-window routing, context across windows, window prop updates, desktop child validation, and chrome lowering.
- Validation completed so far: `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --all --all-targets`; core/react TypeScript checks; napi build; node/core/react npm tests; `git diff --check`. No `tools/run_fixtures.py` fixture runner exists in this repo.
- Next step: update `TODO.md` / `TODO-1.md` completion records, then commit intended changes.
