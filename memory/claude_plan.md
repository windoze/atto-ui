## Current Invocation Plan

Selected task: **M7.6 真实请求取消** from `TODO.md`.

Goal: ensure Esc, `ChatMessageList::on_cancel`, and `/abort` cancel in-flight live DeepSeek HTTP/SSE requests, advance branch tokens, and prevent late live events from mutating a canceled or replaced branch. PTY coverage may remain mock-backed as required by the task.

### Steps

1. Check the latest commit message for unfinished work directly relevant to M7.6.
2. Inspect the agent app live provider, cancellation, branch-token, `/abort`, and existing DeepSeek provider tests.
3. Identify how live DeepSeek turns are spawned and how cancellation tokens/handles currently flow through mock and live paths.
4. Implement real request cancellation by wiring cancellation into the live HTTP/SSE future and ensuring all UI cancel entry points invoke it. **Done:** live DeepSeek turns now register a futures abort handle alongside the existing cancellation token, and both initial live turns and live tool-loop continuations are aborted by the existing cancel entry points.
5. Add or update focused tests for live request cancellation and late-event rejection; keep PTY tests mock-backed where applicable. **Done:** added a live local SSE test for `/abort` closing the in-flight connection and rejecting stale branch events.
6. Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets`. **Done:** also ran `cargo fmt --all -- --check`.
7. Mark M7.6 `[DONE]` in `TODO.md` with completion notes and validation commands. **Done.**
8. Update this progress file with the completed state. **Done.**
9. Commit all related changes with a clear M7.6 commit message and required co-author trailer, then stop. **Next.**
