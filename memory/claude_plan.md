# Execution Plan

## Current Status

- Started this invocation.
- Read `TODO.md` and `TODO-1.md`.
- First incomplete task is `NR15 — 审阅 NT15`, which reviews the `@atto-ui/core` imperative builders from `NT15`.
- Root `PLAN.md` is not present; existing phase plans are `PLAN-1.md` and `PLAN-2.md`.
- Review found a concrete omission: `NODE_BINDING.md` §6.4 documents `ChatMessageList` as a low-level builder, and the Python wrapper exposes Markdown/Terminal/FileTree/Chat helpers, but `@atto-ui/core` builders currently cover only the core built-ins.

## Plan

1. Read `TODO.md` and identify the first task whose title is not prefixed with `[DONE]`.
2. Check recent git history only for directly relevant unfinished work after the current task is identified.
3. Add the missing `@atto-ui/core` command builders that are already documented or exposed by the Python low-level wrapper: MarkdownViewer, TerminalEmulator, FileTree/FileTreeNode, chat message helpers, ChatMessageList, and ChatInputPanel.
4. Keep the generated spec shapes thin and consistent with runtime schemas: snake_case prop names, string callback handles, undefined pruning, and plain object `ComponentSpec` output.
5. Extend core JS and TS tests to cover these builders and callback/string typing.
6. Run formatting first, then clippy with warnings denied, then the required Rust and TypeScript/JS validation for `packages/core` and affected packages.
7. Update `TODO.md` / `TODO-1.md` completion records and prefix `NR15` with `[DONE]` only after validation succeeds.
8. Commit all intended changes with a clear `NR15` review message.
9. Stop after completing this single task.

## Notes

- This file records the actionable plan and progress log, not private chain-of-thought.
- `TODO.md` remains the source of truth for task ordering and completion state.

## Progress Log

- Identified `NR15` as the first incomplete task.
- Reviewed `packages/core` builders against `TODO-1.md`, `PLAN-1.md`, `NODE_BINDING.md`, runtime schemas, and the Python low-level helper surface.
- Fixed the documented/Python-helper coverage gap by adding MarkdownViewer, TerminalEmulator, FileTree/FileTreeNode, chat message helpers, ChatMessageList, ChatInputMode, and ChatInputPanel builders.
- Extended `packages/core` JS output tests and TypeScript type tests for the added builders.
- Validation passed: `npm run typecheck --prefix packages/core`; `npm test --prefix packages/core`; `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --all --all-targets`; `npm run typecheck --prefix packages/react`; `npm test --prefix packages/react`; `npm test --prefix crates/atto-ui-node`; `git diff --check`.
- No `tools/run_fixtures.py` fixture runner exists in this workspace.
