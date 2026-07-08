# P1.3 执行计划

## 当前任务

- TODO 首个未完成任务：`P1.3 快照与测试`。
- 任务要求：为 `snapshot_markdown_app` / `snapshot_chat_app` 增加多语言代码块与带语法高亮的 diff 场景；在 `tests/` 补充 PTY 覆盖，能够通过屏幕内容或样式断言验证高亮着色。
- 阶段约束：保持 P1.1 代码块高亮和 P1.2 diff 高亮既有语义；不能破坏代码块水平滚动、纯文本 fallback、diff `+`/`-` 语义色。

## 执行计划

1. 检查最近提交与当前工作树状态，确认是否有与 P1.3 直接相关的未完成改动或冲突；只处理当前任务相关内容。
2. 定位 `snapshot_markdown_app`、`snapshot_chat_app`、现有 PTY 测试与测试宿主样式断言能力，明确最小可验证改动点。
3. 扩展 markdown snapshot 场景，加入至少两种语言的 fenced code block，并保留未知或普通文本路径不受影响。
4. 扩展 chat snapshot 场景，加入带文件路径或 unified header 的 diff，用于触发 diff payload 语法高亮，同时覆盖新增/删除语义色。
5. 新增或扩展 PTY 测试：验证屏幕内容存在，并通过 vt100 样式/颜色断言确认代码块语法高亮与 diff 语义色/语法色合成按预期生效。
6. 先运行聚焦测试；修复发现的问题后执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
7. 更新 `TODO.md`，将 P1.3 标题加 `[DONE]` 并填写完成记录与验证结果；仅当阶段计划确实变化时才更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、`git log --oneline -10`，提交本任务全部相关改动，提交信息使用 `[P1.3] ...` 格式，然后停止。

## 进度记录

- 已确认 `TODO.md` 中 P1.3 是首个未完成任务。
- 已读取 `PLAN.md` 中 P1 验收要求；当前无需修改阶段级计划。
- 已检查工作树与最近提交：最新提交为 P1.2 diff 高亮实现，无显式未完成阻塞项；当前只有本计划文件变更。
- 已定位实现点：`crates/atto-ui-markdown/src/bin/snapshot_markdown_app.rs`、`crates/atto-ui-chat/src/bin/snapshot_chat_app.rs`、`crates/atto-ui-markdown/tests/pty_markdown_viewer_blocks.rs`、`crates/atto-ui-chat/tests/pty_chat.rs`。
- 测试策略更新：新增独立 fixture，不修改默认滚动/交互场景；PTY 测试使用 `cell_fgcolor` / `cell_bgcolor` 比较屏幕内颜色关系，避免硬编码具体 ANSI/vt100 映射。
- 已实现 snapshot 场景：markdown 新增 `--syntax-highlighting`（Rust/Python/未知语言 fallback）；chat 新增 `--syntax-diff`（Rust unified diff）。
- 已新增 PTY 覆盖：markdown 断言 Rust/Python 关键字与 fallback 前景色不同；chat 断言 context payload 语法色生效，且新增/删除行保留语义前景/背景。
- 已运行 `cargo fmt --all`。
- 聚焦验证通过：`cargo test -p atto-ui-markdown --test pty_markdown_viewer_blocks pty_markdown_viewer_renders_syntax_highlighted_code_blocks`、`cargo test -p atto-ui-chat --test pty_chat chat_syntax_diff_highlights_context_and_preserves_semantic_lines`。
- 完整验证通过：`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 已更新 `TODO.md`：P1.3 标题已加 `[DONE]`，完成记录包含新增 fixture、PTY 样式断言和验证命令。测试后仅更新文档记录，无需重跑完整套件。
- 已检查最终 `git status` / `git diff` / `git log --oneline -10`；改动均为 P1.3 相关文件，下一步提交。
