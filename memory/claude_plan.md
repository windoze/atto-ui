<!-- Progress plan for the current autonomous task invocation. -->

# Current Invocation Plan

1. Read `TODO.md` first and identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit only for directly relevant unfinished work after the current task is identified.
3. Read the task body, dependencies, validation requirements, and any completion record in `TODO.md`.
4. Inspect only the code and tests needed to complete that task without broad unrelated triage.
5. Implement the task as written, adding a prerequisite task instead only if a concrete blocker makes correct execution impossible.
6. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required by the task and repository policy.
7. Update `TODO.md` by prefixing the completed task heading with `[DONE]` and filling in its completion record. Update `PLAN.md` only if the phase-level plan changes.
8. Review the worktree, commit all intended changes with a descriptive task-scoped commit message, and stop without starting the next task.

## Progress Log

- Initial plan written before running repository inspection commands.
- Identified first incomplete task from `TODO.md`: `NT15` (`@atto-ui/core` imperative builders, L.2) in `TODO-1.md`.
- Read `TODO-1.md` NT15 details. Scope: add type-safe imperative component spec builders under `packages/core/src/`, test equivalence with handwritten JSON, and pass TypeScript checks. Latest commit `NR14` did not surface a directly relevant unfinished blocker.
- Implemented first draft of `packages/core/src/builders.ts` and runtime `builders.js`, exported them from the core package, and added JS/type tests for builder output and type safety.
- Quick validation found a TypeScript 6 deprecation failure for `moduleResolution: "Node"`; fixed both TS package configs to `module`/`moduleResolution: "Node16"`. Core and React typechecks now pass, and `packages/core` tests pass.
- Completed full validation: `cargo fmt`, workspace clippy, full Rust tests, core/react typechecks, core/react JS tests, and Node binding JS tests all passed. Marked `NT15` as `[DONE]` in `TODO-1.md` and the root `TODO.md` index.
- Fixed an event alias merge edge case so explicit `events` entries are not cleared by omitted `on*` aliases; reran `packages/core` typecheck/test plus React typecheck/test successfully.
