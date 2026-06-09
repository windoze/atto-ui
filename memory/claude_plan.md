# Claude Execution Plan

## Objective
Complete exactly the first incomplete task listed in TODO.md, then stop after committing the completed task.

## Step-by-step plan
1. Read TODO.md first to identify the first task heading that is not explicitly prefixed with [DONE].
2. Check recent git state and the latest commit only as needed to understand whether there is unfinished work directly relevant to that selected task.
3. Inspect the files and tests directly relevant to the selected task.
4. Implement the task without changing unrelated behavior or working around specification gaps.
5. Run formatting, linting, and the relevant validation required by the task; if full validation is needed, run it after fmt and clippy pass.
6. If a blocking prerequisite is discovered, update TODO.md with the minimum prerequisite task, leave the current task incomplete, commit the bookkeeping change, and stop.
7. If implementation succeeds, update TODO.md by prefixing the task title with [DONE] and filling in its completion record.
8. Commit all changes for this invocation with a descriptive message and the required co-author trailer.

## Progress
- Plan file created before task execution.

## Progress update
- Selected first incomplete TODO task: #2 close/zoom button relocation and glyph normalization.
- Scope note: #2b mouse hit-test synchronization is a separate TODO item and will not be advanced in this invocation unless required by validation.
- Implemented titlebar layout direction: close control on left, zoom control on right, with bracket rendering around themed glyphs.
- Updated tests and PTY helper for bracketed left close/right zoom controls.
- Starting validation: cargo fmt, then clippy, then full tests.
- cargo fmt and cargo clippy --all-targets -- -D warnings passed.
- Running full workspace test suite: cargo test --all --all-targets.
- Fixed titlebar text_area to start after the left close control, eliminating title/control overlap.
- Re-running validation after the fix.
- cargo fmt and cargo clippy passed after titlebar text_area fix.
- Re-running full workspace test suite.
- Widened the AppHost API fixture window so the expected callback-count title remains visible with wider titlebar controls.
- Re-running cargo fmt and clippy.
- cargo fmt and cargo clippy passed after fixture width adjustment.
- Re-running full workspace test suite.
- Updated pty_desktop expectations for new title position and left close button hit area.
- Re-running cargo fmt and clippy.
- cargo fmt and cargo clippy passed after PTY coordinate updates.
- Re-running full workspace test suite.
- Fixed pty_desktop text lookup to convert UTF-8 byte offsets to terminal cell columns.
- Re-running cargo fmt and clippy.
- cargo fmt and cargo clippy passed after terminal-cell coordinate fix.
- Re-running full workspace test suite.
- Updated terminal PTY titlebar drag/click coordinate to avoid the new left close control.
- Re-running cargo fmt and clippy.
- cargo fmt and cargo clippy passed after terminal PTY coordinate update.
- Re-running full workspace test suite.
- Updated terminal PTY close click to the new left titlebar close control.
- Re-running cargo fmt and clippy.
- cargo fmt and cargo clippy passed after terminal close coordinate update.
- Re-running full workspace test suite.
- Final validation passed: cargo test --all --all-targets.
- Marked TODO #2 as [DONE] with completion record and validation results.
- Preparing final diff/status review before commit.
