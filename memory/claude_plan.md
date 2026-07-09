# Claude Execution Plan

## Scope

- Follow `TODO.md` as the authoritative task list.
- Identify the first task whose heading is not prefixed with `[DONE]`.
- Complete exactly that one task, then stop after committing the result.

## Step-by-Step Plan

1. Read `TODO.md` and identify the first incomplete task without doing broad unrelated issue triage.
2. Check recent git context only as needed to determine whether the latest commit mentions an unfinished issue directly relevant to that task.
3. Inspect the files and tests relevant to the selected task.
4. Implement the selected task completely, or add the minimum prerequisite task in `TODO.md` if a concrete blocker makes direct completion impossible.
5. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the required tests; if code changed and full validation is required, run the full suite with an extended timeout.
6. Fix any observed unscheduled test failures or record the minimum prerequisite task before marking the current task complete.
7. Update `TODO.md` by prefixing the completed task heading with `[DONE]` and filling its completion record. Update `PLAN.md` only if phase-level sequencing changes.
8. Review git status and diff, then commit all changes relevant to this invocation with a descriptive task commit message.
9. Stop without starting the next task.

## Progress Log

- Initial plan written before reading project task details.
- Read `TODO.md`; selected first incomplete task: `P6.4 运行时/JS 侧同步`.
- Current task scope: sync P6 model changes through `crates/atto-ui-chat/src/dynamic.rs`, `crates/atto-ui-node`, `packages/core`, `packages/react`, and `docs/NODE_API.md`; preserve legacy serialization where required; run Rust/Node smoke and runtime compatibility validation.
- Inspection result: Rust dynamic conversion already has minimal compact block and approval action/level support, while raw N-API d.ts, core builders/types, React re-exports, React hook helpers, docs, and JS/TS tests still reflect the old approval/notice-only shape.
- Implementation approach: add optional structured approval fields without removing legacy `resolved`; add compact block types/builders; update React hook approval resolution to use action/level when present and infer legacy otherwise.
- Implemented JS-facing model sync in core/react/node type layers and added targeted core/react type checks; `npm run typecheck --prefix packages/core` and `npm run typecheck --prefix packages/react` pass.
- Completed full validation for P6.4: Rust fmt/clippy/build/test, core/runtime Node-Bun-Deno compatibility, raw Node build/test, React build/test, example smoke, and `git diff --check` all pass.
- Marked `P6.4` as `[DONE]` in `TODO.md` with completion notes. Next step is final diff review and a task-specific git commit.
