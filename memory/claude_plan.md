# Execution Plan

I will follow the task order in `TODO.md` and complete only the first task whose heading is not prefixed with `[DONE]`. This file records the actionable plan and progress updates; it intentionally avoids private reasoning details.

1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Check the latest commit only for directly relevant unfinished work related to that task.
3. Inspect the relevant implementation and tests.
4. Implement the task completely, without narrowing scope or using workarounds.
5. Run formatting, linting, and the required tests in the requested order.
6. Update `TODO.md` with a `[DONE]` prefix and a completion record for the task.
7. Commit all task-related changes with a clear message and stop.

## Progress

- Started task selection setup.
- Read `TODO.md`; every listed task is already prefixed with `[DONE]`, so this invocation proceeds with the final review and release-tag flow.
- Latest commit is an M7.R review commit and does not mention an unfinished issue requiring prerequisite work.
- Beginning final validation in the required order: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace --all-targets`.
- Final validation completed successfully: formatting check, clippy with warnings denied, and the full workspace test suite all passed.
- Next step: commit the final review progress file and create the `endtag` Git tag.
