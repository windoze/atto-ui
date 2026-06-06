# Claude Execution Plan

## Scope

- Work from `TODO.md` as the authoritative task list.
- Identify the first task whose title is not prefixed with `[DONE]`.
- Complete exactly that one task, then stop after committing.
- Do not perform unrelated historical triage before selecting the current task.

## Constraints

- Keep `TODO.md` as the routine task/completion log.
- Update `PLAN.md` only if phase-level sequencing, dependencies, assumptions, or completion criteria change.
- Do not use workarounds for spec mismatches. If a blocker prevents the task, add the minimum prerequisite task to `TODO.md`, commit, and stop.
- Treat any observed unscheduled test or fixture failure as a real project issue: fix it or schedule it before marking the current task done.
- Mark completion only by prefixing the task title with `[DONE]` and filling its completion record.

## Step-by-Step Plan

1. Read `TODO.md` and identify the first incomplete task by title prefix.
2. Inspect recent git history only enough to determine whether the latest commit mentions an unfinished issue directly relevant to that selected task.
3. Read the selected task details, dependencies, validation requirements, and completion-record format.
4. Inspect the relevant code and tests for that task, avoiding broad unrelated issue sweeps.
5. Implement the smallest correct change that satisfies the task requirements.
6. Run `cargo fmt`.
7. Run `cargo clippy --all-targets -- -D warnings`.
8. Run the relevant tests, then the full required test suite if code changes require it.
9. If failures appear, fix them if in scope or explicitly schedule prerequisite/follow-up tasks in `TODO.md` according to the policy.
10. Update `TODO.md` by marking the completed task title with `[DONE]` and adding a concise completion record with validation results.
11. Update this plan file as key steps complete or if the plan changes.
12. Review git status and diff to ensure only intended files are included.
13. Commit all required changes with a clear message referencing the task id.
14. Stop without starting the next task.

## Progress Log

- Initial execution plan recorded before repository inspection.
- Selected first incomplete task from `TODO.md`: `R11 — 审阅 T11`.
- R11 scope: review T11's visible-row parsing and borrow changes, verify boundary coverage and large dataset rendering, fix only directly related issues if found, then mark R11 done and commit.
- Review found that existing virtual-scroll PTY tests exercise the shared scroll container but not the changed `ListBox`/`TableView` row-slicing render paths directly.
- Added focused unit tests for `ListBox` and `TableView` to verify vertical scroll slicing includes the first and last visible boundary rows and excludes adjacent offscreen rows.
- Validation completed: `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test draw_slices_visible_rows_after_vertical_scroll --lib`; `cargo test visible_row_range --lib`; `cargo test --test pty_virtual_scrolling`; `cargo test --all --all-targets`.
- Completion record added to `TODO.md` and R11 marked `[DONE]`.
- Pre-commit diff reviewed. Intended files: `TODO.md`, `memory/claude_plan.md`, `src/widgets/list.rs`, `src/widgets/table.rs`. Untracked `notification.sh` and `run_agent.sh` are unrelated and will not be staged.
