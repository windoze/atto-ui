# Claude Execution Plan

## Scope
- Follow `TODO.md` as the authoritative task list.
- Complete exactly the first task whose heading is not prefixed with `[DONE]`, then stop.
- Do not perform broad historical triage before identifying the current task.

## Plan
1. Read `TODO.md` and identify the first incomplete task by heading prefix.
2. Check recent git context only as needed to see whether the latest commit mentions an unfinished issue directly relevant to that task.
3. Inspect only the files and tests relevant to the selected task.
4. Implement the task as specified, adding no workaround or scope narrowing.
5. Run formatting, linting, and relevant tests; run the full suite if compiled behavior changed and no narrower validation is sufficient.
6. If tests expose an unscheduled failure, either fix it or add the minimum required prerequisite task before marking the current task complete.
7. Update `TODO.md` by prefixing the completed task heading with `[DONE]` and filling the completion record.
8. Update this file whenever the plan changes or a key step completes.
9. Commit all intended changes with a descriptive message, then stop.

## Progress
- Initial plan written before inspecting the task list.
- Read `TODO.md`; the first incomplete task is `T16` from `TODO-2.md`.
- Next step is to read the detailed `T16` entry and inspect only directly relevant git/code context.
- Read the detailed `T16` entry: implement Document symbols, Workspace symbols, and Global search pickers.
- Latest commit is `[R15] Review file and buffer pickers`; it does not mention an unfinished issue directly blocking `T16`.
- Current worktree also contains unrelated untracked helper scripts; I will not modify them unless they become directly relevant.
- Completed targeted exploration: T16 will reuse existing modal picker lifecycle, add editor LSP symbol events, bridge editor events through `EditorWindowView`, add open/jump commands that preserve UTF-16 conversion in the editor, and add a local global-search helper that respects ignore files and size limits.
- Implemented T16 surfaces and validation is green after `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --all --all-targets` (the full suite passed after rerunning one transient PTY timeout).
