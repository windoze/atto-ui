# Execution Plan

I will use `TODO.md` as the authoritative task list and complete exactly the first task whose heading is not prefixed with `[DONE]`. I will avoid broad triage before selecting that task.

1. Read `TODO.md` to identify the first incomplete task and its validation requirements. Done: the first incomplete task is `#3 按钮重绘`.
2. Check the current repository state and latest commit only as needed to detect directly relevant unfinished work. Done: latest commit is `#2b` and does not add a blocker for `#3`; unrelated untracked files will be left untouched.
3. Inspect the code and tests related to that task. Done: `src/widgets/button.rs`, theme style tokens, dynamic runtime construction, and PTY widget coverage are the relevant surfaces.
4. Implement the task without workarounds or spec deviations. Done: buttons now draw as borderless single-line color blocks with right/bottom shadow and default/focused emphasis while keeping current layout height stable because `#3b` explicitly owns the height change and dependent layout regression.
5. Run formatting, linting, and relevant tests, escalating to the required full validation when code changes require it. Done: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, focused button/PTY tests, and `cargo test --all --all-targets` passed.
6. Update this file at key milestones and update `TODO.md` by adding `[DONE]` and a completion record when the task is complete. Done.
7. Commit all task-related changes with a clear message and stop without starting the next task. Pending.
