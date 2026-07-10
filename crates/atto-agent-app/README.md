# Atto Agent App

`atto-agent-app` is a single-window terminal agent application built on `atto-ui`, `atto-ui-chat`, and `atto-ui-async`. It hosts the app-layer agent pieces without adding network dependencies to the reusable UI crates.

## Implementation Status

| Area | Status |
|---|---|
| TUI shell | Desktop, status bar, one `ChatPanel`, slash commands, cancellation, retry, regenerate, and edit/resubmit are wired. |
| Provider selection | The app selects `provider: deepseek` when a resolved API key is present and `--mock` is not set; otherwise it selects `provider: mock`. |
| Interactive turn loop | `provider: deepseek` runs live HTTP/SSE turns through the same UI action path as the deterministic mock provider, including tool-loop continuation, cancellation, and structured error display. |
| DeepSeek protocol | OpenAI-compatible request/response models, SSE parser, UI stream mapper, error mapping, and HTTP streaming client are implemented in this crate. |
| Real DeepSeek smoke | Available as an ignored test that requires `DEEPSEEK_API_KEY`; default tests do not use the network. |
| Tools | Built-in `read_file`, `list_files`, `search_text`, `apply_patch`, and `run_command` are registered with workspace path checks and approval policy. |
| Skills | `SKILL.md` discovery, manual loading, deterministic auto matching, bounded prompt injection, and permission isolation are implemented. |
| Plan mode | `off`, `on`, and `auto` modes are implemented; mutating tools are blocked until a pending plan is accepted. |
| Context | Transcript to DeepSeek message conversion, `@path` file mention expansion, tool output truncation, compact blocks, and JSONL transcript persistence are implemented. |

## Run

```sh
cargo run -p atto-agent-app -- --workspace .
```

Without a configured API key the app selects the mock provider and shows a startup notice with next steps. Set `DEEPSEEK_API_KEY` or `--api-key` to select DeepSeek, or pass `--mock` to explicitly force the mock provider and suppress the notice. The deterministic PTY fixture always uses mock provider state.

Run the deterministic PTY fixture binary directly with:

```sh
cargo run -p atto-agent-app --bin snapshot_agent_app
```

Run the manual real DeepSeek smoke test with:

```sh
DEEPSEEK_API_KEY=... cargo test -p atto-agent-app --test deepseek_real_smoke -- --ignored
```

## Configuration

Configuration is resolved from defaults, then user config, workspace config, environment variables, and CLI flags. Later sources override earlier sources.

| Source | Path or prefix |
|---|---|
| User config | `~/.config/atto-agent/config.toml` |
| Workspace config | `.atto-agent.toml` under the selected workspace, or the path from `--config` |
| Environment | `DEEPSEEK_*` and `ATTO_AGENT_*` |
| CLI | Flags passed to `cargo run -p atto-agent-app -- ...` |

Supported CLI flags:

| Flag | Meaning |
|---|---|
| `--api-key`, `--deepseek-api-key` | DeepSeek API key. Prefer `DEEPSEEK_API_KEY` locally. |
| `--base-url` | API base URL. Defaults to `https://api.deepseek.com/v1`. |
| `--model` | Model name. Defaults to `deepseek-chat`. |
| `--temperature` | Non-negative sampling temperature. Defaults to `0.2`. |
| `--max-tokens` | Positive max token count. Defaults to `4096`. |
| `--workspace` | Workspace root. Defaults to the current directory. |
| `--plan-mode`, `--plan` | `off`, `on`, or `auto`. Defaults to `auto`. |
| `--transcript` | Optional JSONL transcript path. Relative paths resolve under the workspace. |
| `--config` | Explicit TOML config path. |
| `--mock` | Force the mock provider even when an API key is configured. |

Supported environment variables:

| Variable | Meaning |
|---|---|
| `DEEPSEEK_API_KEY` | DeepSeek API key; selects the DeepSeek provider unless `--mock` is set. |
| `DEEPSEEK_BASE_URL` | API base URL. |
| `DEEPSEEK_MODEL` | Model name. |
| `DEEPSEEK_TEMPERATURE` | Non-negative temperature. |
| `DEEPSEEK_MAX_TOKENS` | Positive max token count. |
| `ATTO_AGENT_WORKSPACE` | Workspace root. |
| `ATTO_AGENT_PLAN_MODE` | `off`, `on`, or `auto`. |
| `ATTO_AGENT_TRANSCRIPT` | Optional JSONL transcript path. |

Example `.atto-agent.toml`:

```toml
base_url = "https://api.deepseek.com/v1"
model = "deepseek-chat"
temperature = 0.2
max_tokens = 4096
workspace = "."
plan_mode = "auto"
transcript_path = ".atto/transcript.jsonl"
```

## Slash Commands

| Command | Behavior |
|---|---|
| `/help` | Show command help. |
| `/clear` | Clear the current transcript, queued input, file references, active turn, and turn budgets. |
| `/plan [on|off|auto]` | Cycle plan mode with no argument, or set a specific mode. |
| `/skills` | List discovered skills, loaded skills, and non-fatal discovery issues. |
| `/skill <name>` | Load a discovered skill into the current session. |
| `/tools` | List built-in tools, output kinds, and approval policy. |
| `/abort` | Cancel the active turn, including live DeepSeek HTTP/SSE requests. |

## Tools

| Tool | Permission | Output | Notes |
|---|---|---|---|
| `read_file` | Always allow | Markdown | Reads UTF-8 files under the workspace, up to 256 KiB. |
| `list_files` | Always allow | Markdown | Lists workspace files with a relative glob pattern. |
| `search_text` | Always allow | Markdown | Searches UTF-8 files with literal text matching. |
| `apply_patch` | Approve for project | Diff | Applies text unified diffs through `git apply` after path and text checks. |
| `run_command` | Approve for project | ANSI | Runs an argv array in a workspace-contained cwd without shell string parsing. |

All built-in tools canonicalize workspace paths. Symlink targets must remain inside the workspace. Mutating tools are blocked by plan mode until the user accepts a pending plan, even if a project-level approval was granted earlier in the process.

## Skills

Skills are local Markdown instruction files with YAML frontmatter. They are prompt packages, not executable plugins, and they cannot grant tool permissions.

Default search roots:

```text
.atto/skills/<skill-id>/SKILL.md
~/.config/atto-agent/skills/<skill-id>/SKILL.md
```

Example `SKILL.md`:

```markdown
---
name: rust-review
description: Review Rust changes for correctness, API regressions, and tests.
triggers: ["rust", "review", "clippy"]
tools: ["read_file", "search_text"]
mode: auto
---

Inspect changed Rust files first. Prefer concrete findings with file and line references.
```

Manual skills are loaded with `/skill <name>`. Auto skills are selected by deterministic word matching across the user prompt and the skill `name`, `description`, and `triggers`, with a per-prompt auto-load limit of 4. Loaded skills are injected as a bounded `<skills>` system block; each skill body is capped at 6 KiB and the total skill prompt at 20 KiB.

## Context And Transcript

`ContextBuilder` converts UI transcript blocks into OpenAI-compatible messages. User `@path` tokens are expanded into a bounded `<context_files>` block when they resolve to UTF-8 files inside the workspace.

Default context budgets:

| Item | Limit |
|---|---|
| File mention | 32 KiB per file, 128 KiB total per user message, 128 mentions max |
| Tool result sent to model | 16 KiB per tool result |
| Compact threshold | Estimated 70 percent of a 64 Ki token window |
| Recent messages kept during compact | 20 |

Transcript persistence is off by default. Set `--transcript path.jsonl`, `ATTO_AGENT_TRANSCRIPT`, or `transcript_path` in TOML to save and restore JSONL transcript rows. Streaming turns restored from disk are marked `Canceled` because active network/tool work cannot be resumed safely.

## Validation

Default validation should not require `DEEPSEEK_API_KEY` or network access.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

Focused app validation:

```sh
cargo test -p atto-agent-app --all-targets
cargo test -p atto-agent-app --test pty_agent
cargo test -p atto-agent-app --test deepseek_real_smoke -- --ignored
```
