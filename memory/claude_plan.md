# Execution Plan

Status: selected task `P5.2 Turn 级折叠`.

Plan:
1. Read `TODO.md` and identify the first task whose heading is not prefixed with `[DONE]`.
2. Review only the task-relevant context, including `PLAN.md` and recent Git history if it directly affects the selected task.
3. Implement the selected task completely, unless a concrete prerequisite blocker must be inserted into `TODO.md`.
4. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required.
5. Update `TODO.md` with the `[DONE]` prefix and a completion record, or record a blocker/prerequisite if the task cannot be completed.
6. Commit the resulting changes with a task-specific message and stop without starting the next task.

Progress log:
- Initialized this plan before reading task details.
- Read `TODO.md`; the first heading without `[DONE]` is `P5.2 Turn 级折叠`.
- Next step is to inspect only P5.2-relevant plan/source context and recent Git state.
- Confirmed latest commit is `[P5.1] Add chat session search` with no unfinished issue in the subject.
- Implementation approach: keep turn collapse as local `ChatMessageList` UI state, filter virtual `row_keys` so collapsed turns keep only their header row, add a header action button for `Collapse`/`Expand`, show a collapsed placeholder line in the header, and queue a virtual scroll adjustment when expanding to restore the pre-collapse offset.
- Initial `cargo test -p atto-ui-chat --lib` exposed one directly affected unit test whose viewport was too short after adding the header fold control; update that test fixture rather than changing behavior.
- Implemented P5.2 unit coverage and confirmed `cargo test -p atto-ui-chat --lib` passes.
- Next validation sequence: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `cargo build --workspace --all-targets`, then `cargo test --all --all-targets`.
- `cargo clippy --workspace --all-targets -- -D warnings` first run found the old no-arg row-key helper is now test-only; marked it `#[cfg(test)]` and will rerun validation from formatting.
- Full `cargo test --all --all-targets` reached PTY chat and found two existing PTY cases that depended on old vertical layout. The added header fold control makes their target text offscreen, so update those tests to drive scrolling/visibility explicitly.
- Updated the affected PTY fixtures/tests: `--tool-call` and `--turn-meta-error` now allocate enough window height for the new fold row, and the tool-call test uses explicit key events plus disclosure-aware waiting. Both exact PTY cases now pass.
- Final validation passed: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `cargo build --workspace --all-targets`, and `cargo test --all --all-targets`.
- Updated `TODO.md` to mark `P5.2 Turn 级折叠` as `[DONE]` with completion notes.
