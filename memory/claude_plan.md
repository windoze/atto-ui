# Execution plan

I will follow `TODO.md` as the source of truth and complete only the first task whose heading is not prefixed with `[DONE]`.

1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Check the latest commit message only for directly relevant unfinished work.
3. Inspect the files required by that task and avoid unrelated triage.
4. Implement the task completely, or add the minimum prerequisite task if a concrete blocker makes direct completion impossible.
5. Run formatting, linting, and the relevant/full tests required by the task.
6. Update `TODO.md` by prefixing the completed task title with `[DONE]` and recording the completion result. Update `PLAN.md` only if phase-level sequencing changed.
7. Commit all task-related changes with a descriptive message and stop without starting the next task.

## Current task

First incomplete task: `M7.3 配置模型`.

Task-specific steps:

1. Inspect the terminal crate structure, existing terminal constants/options, and `src/theme/config.rs` persistence style.
2. Add a centralized `TerminalConfig` model covering scrollback length, palette, prefix key, release shortcut, alt-screen scroll behavior, shell/command, cwd/profile, shell-integration injection, and default cursor shape.
3. Provide behavior-preserving defaults plus JSON/YAML load/save helpers.
4. Add focused unit tests for defaults, validation, and JSON/YAML roundtrips.
5. Run formatting, clippy with warnings denied, and tests; fix any failures.
6. Mark M7.3 `[DONE]` in `TODO.md`, record validation, commit, and stop.

Progress:

- Added the initial `config` module, serde dependencies, public exports, defaults, persistence helpers, validation, and unit coverage for M7.3.
- Validation completed: formatting, focused config tests, workspace clippy with warnings denied, and full workspace tests passed.
- `TODO.md` now marks M7.3 as `[DONE]` with a completion record.
