# Execution plan

## Current task

First incomplete task from `TODO.md`: **M7.1 Provider 选择**.

Task requirements: introduce `AgentProvider` (`Mock` / `DeepSeek`) and parsing/selection logic; select DeepSeek when a valid `DEEPSEEK_API_KEY` exists and `--mock` is not set, otherwise select mock; keep snapshot fixtures always mock; make the status bar `provider` segment reflect the actual provider instead of a hard-coded mock value.

Latest commit message was `Update doc`; it does not mention an unfinished issue directly relevant to M7.1.

## Steps

1. Inspect `atto-agent-app` config, runtime construction, snapshot fixture, and status-bar code paths that currently handle `--mock`, API keys, and hard-coded `provider: mock`.
2. Add a small provider model and deterministic selection logic close to configuration/runtime setup, reusing existing CLI/env/config parsing where possible. **Done:** `AgentProvider` now selects DeepSeek from a non-blank resolved API key unless `--mock` is set.
3. Wire the selected provider into `AgentApp` state and status rendering; keep current turn execution mock-backed for M7.1, because live DeepSeek turn execution is explicitly scheduled for later M7 tasks. **Done:** provider status is now a runtime status binding instead of a hard-coded `provider: mock` segment.
4. Add or update focused tests for provider selection, CLI/env behavior, snapshot mock behavior, and status-bar text. **Done:** config and status unit coverage was added/updated; snapshot PTY still asserts `provider: mock`.
5. Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, and relevant tests, then the workspace test suite if required. **Done:** formatting, clippy, focused app tests, full workspace tests, and final format check passed.
6. Mark M7.1 `[DONE]` in `TODO.md` with the validation record, commit all task changes, and stop.
