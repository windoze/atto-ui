# Claude Execution Plan

## Scope

- Follow `TODO.md` as the authoritative task list.
- Complete exactly the first task whose heading is not prefixed with `[DONE]`, then stop.
- Do not perform broad historical triage before selecting the current task.
- Record only a concise, auditable plan and progress notes here; hidden reasoning is not recorded.

## Step-by-step Plan

1. Read `TODO.md` and identify the first incomplete task by heading prefix.
2. Check recent Git context only as needed, including whether the latest commit mentions an unfinished issue directly relevant to the selected task.
3. Inspect the code and tests relevant to the selected task.
4. Implement the task completely, or add the minimum prerequisite task to `TODO.md` if a concrete blocker makes implementation impossible.
5. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required by the task and validation policy.
6. Update `TODO.md` by prefixing the completed task title with `[DONE]` and filling its completion record, or record the blocker/prerequisite if blocked.
7. Update this plan file after key milestones or plan changes.
8. Inspect Git status, diff, and recent log; commit all intended changes with a clear task-specific message.
9. Stop without starting the next task.

## Progress

- Initial plan written before reading task files or running commands.
- Selected first incomplete task: `P3.1 store 截断-fork API`.
- Recent Git context checked: latest commit is `[P2.R] Complete P2 review`; no directly relevant unfinished P3.1 issue found.
- Worktree before implementation only contained this plan file change.
- Implemented `ChatMessageStore::truncate_from` and `ChatMessageStore::fork_at` with version-map cleanup for removed messages/blocks.
- Added store unit tests for first/middle/last truncation, fork no-op behavior, and truncating a streaming turn.
- Validation note: first full `cargo test --all --all-targets` run observed `chat_artifact_code_link_opens_text_viewer_window` timing out with an empty screen. The exact test and then full `pty_chat` test file both passed on immediate rerun; rerunning full suite before marking P3.1 complete.
- Validation completed: `cargo fmt --all`, targeted store tests, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `cargo build --workspace --all-targets`, and final `cargo test --all --all-targets` passed.
- Updated `TODO.md` with `[DONE] P3.1` and completion record. Next step is to inspect the diff and commit the P3.1 changes only.
