# Execution Plan

## Current objective
Complete exactly the first incomplete task listed in `TODO.md`, then stop after updating task records and committing the result.

## Planned steps
1. Read `TODO.md` to identify the first task whose heading is not prefixed with `[DONE]`.
2. Check the latest commit message only for directly relevant unfinished work tied to that selected task.
3. Inspect the files and tests relevant to the selected task.
4. Implement the task completely, preserving existing behavior and codebase conventions.
5. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant tests and full suite as required.
6. If an unscheduled failing test or blocker appears, either fix it now or add the minimum prerequisite task to `TODO.md` before the blocked task and stop.
7. Mark the completed task title in `TODO.md` with `[DONE]` and update its completion record.
8. Commit all task-related changes with a descriptive message and the required co-author trailer.

## Progress log
- Initialized plan file before repository inspection.
- Selected first incomplete task: `M5.5 第 3 层 shell integration【配置面】`.
- Latest commit is `M5.4` command block interactions, directly preceding this task with no separate unfinished prerequisite noted.
- Next step: inspect terminal spawn/config surfaces and tests before implementing zero-intrusion/default-off shell integration injection.
- Implementation direction: add a public default-disabled shell-integration mode, expose it through builder/handle/dynamic properties, and inject OSC 133/7 startup scripts only for supported interactive shell spawns while leaving unsupported or non-interactive commands unchanged.
- Implemented the shell-integration mode, dynamic property, bash/zsh spawn-time script preparation, temp-file lifetime cleanup, and focused unit/spawn tests. Clippy and focused shell-integration tests have passed.
- Next step: run the full workspace test suite, then update `TODO.md` and commit if it stays green.
- Full workspace tests and the explicit format check passed. `TODO.md` now marks M5.5 as `[DONE]` with completion and validation records.
- Next step: inspect the final diff and commit all task-related files.
