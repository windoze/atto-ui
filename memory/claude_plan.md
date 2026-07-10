# Current Invocation Plan

Selected task: **M7.7 错误映射接线** from `TODO.md`.

Goal: ensure the live DeepSeek provider path reuses the existing M2.5 `ChatError` mapping so missing/invalid API keys, 401/403, 429, 5xx, network disconnects, and no-key live-provider attempts surface clear, actionable UI errors.

## Steps

1. Confirm the latest commit has no unfinished work directly relevant to M7.7. **Done:** latest commit is M7.6 request cancellation and does not list unfinished M7.7 work.
2. Inspect DeepSeek client error mapping, live provider startup/selection, app action failure handling, and existing tests.
3. Identify any mismatch between M2.5 mapping and the M7 live provider path, especially around preflight API-key validation and stream interruption.
4. Implement missing wiring so live provider failures become structured `ChatError` UI failures and status summaries instead of silent exits or generic errors. **Done:** live errors now surface through structured failed turns, and no-key mock fallback gets an actionable startup notice.
5. Add focused tests for missing/invalid API key behavior, HTTP status mappings, and live stream disconnect/error display where coverage is missing. **Done:** added live-provider coverage for missing key, 401, 429, 502, stream disconnect, and startup notice suppression.
6. Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --all-targets`, and `cargo fmt --all -- --check`. **Done.**
7. Mark M7.7 `[DONE]` in `TODO.md` with completion notes and validation commands. **Done.**
8. Update this progress file with completion status, commit all related changes with the required co-author trailer, and stop. **Next.**
