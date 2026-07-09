# Claude Execution Plan

## Scope

- Follow `TODO.md` as the authoritative task list.
- Identify the first incomplete task whose heading is not prefixed with `[DONE]`.
- Complete exactly that task in this invocation, then stop.
- Do not perform broad historical triage before selecting the current task.

## Execution Plan

1. Read `TODO.md` and identify the first incomplete task.
2. Check the latest commit only for directly relevant unfinished work tied to that task.
3. Inspect the files and tests relevant to the selected task.
4. Implement the task completely, using minimal targeted changes.
5. Run formatting first, then linting with `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite as required.
6. If a blocking prerequisite or unscheduled failing test appears, update `TODO.md` with the minimum required prerequisite task, commit that bookkeeping, and stop.
7. If the task is completed, update `TODO.md` by prefixing the task title with `[DONE]` and filling its completion record.
8. Commit all task-related changes with a clear task-specific message.
9. Stop without starting the next task.

## Progress

- Initial plan written before reading project task files or running commands.
- Read `TODO.md`; selected first incomplete task: `P4.1 输入排队`.
- Next steps are limited to recent-commit relevance check, `input.rs`/store/list related inspection, implementation, validation, TODO update, and commit.
- Inspection complete: input submission currently dispatches immediately through `ChatInputPanel::emit_response`; streaming status is tracked on chat messages, while input needs an explicit host-controlled streaming binding for queue behavior.
- Implementation approach: add `ChatInputHandle` streaming and queued response bindings, queue text submissions while streaming, show a one-line queue/status indicator, and let Enter send queued items after streaming completes.
- Code changes made: input handle/panel now expose streaming and queued-response bindings; text submissions queue while streaming; a status line shows streaming/queue state; empty Enter after streaming sends queued responses; snapshot app has a new `--input-queue` fixture and PTY coverage.
- First clippy attempt found an ambiguous integer type in `ChatInputPanel::min_height`; fixed by annotating the local height as `u16`.
- `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings` now pass.
- Targeted input tests exposed stale `TextArea` buffer state after programmatic draft clearing/replacement. Fixed by syncing the text area whenever `ChatInputPanel` writes `draft`, and added a slash replacement regression test.
- Targeted validation now passes: `cargo test -p atto-ui-chat input --lib` and the new `chat_input_queues_text_while_streaming_and_sends_after_prompt` PTY test.
- Full validation passes after the final changes: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace --all-targets`, and `cargo test --all --all-targets`.
- `TODO.md` updated: `P4.1 输入排队` is marked `[DONE]` with implementation, test coverage, and validation records.
- Pre-commit review complete: `git status`, `git diff`, recent log, and `git diff --check` reviewed; only P4.1-related files are modified.
