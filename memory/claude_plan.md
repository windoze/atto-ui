# 执行计划

## 约束
- 以 `TODO.md` 为唯一任务来源，先找第一个标题未带 `[DONE]` 的任务。
- 本次只完成一个任务；完成后更新 `TODO.md`、验证、提交并停止。
- 如遇阻塞当前任务的缺陷或未排期失败测试，先修复或在 `TODO.md` 中添加最小前置任务并停止。
- 不披露隐藏推理；本文件记录可审计的操作计划、关键决策和进度。

## 初始步骤
1. 读取 `TODO.md`，识别第一个未完成任务及其验证要求。
2. 查看最新提交是否明确提到与该任务直接相关的未完成事项。
3. 按任务要求检查相关代码和测试，避免无关历史问题扫查。
4. 实施最小正确变更。
5. 运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再按需运行完整测试；如代码未变且仅文档变更，按要求复用上次绿色结果并记录。
6. 更新 `TODO.md` 的任务标题为 `[DONE]` 并补全 completion record。
7. 检查 `git status`、`git diff`、最近提交，提交本次任务相关所有变更。

## 当前进度
- 已创建初始执行计划。
- 已读取 `TODO.md`，首个未完成任务为 `M3.3 只读工具`：实现 `read_file`、`list_files`、`search_text`，路径必须限制在 workspace 内。
- 已确认最新提交 `[M3.2] Aggregate streamed tool calls` 未声明与 M3.3 直接相关的未完成事项。
- 已在 app crate 实现只读工具注册、workspace 约束、`read_file`/`list_files`/`search_text` 执行逻辑，并更新 `/tools` 输出。
- 已将只读工具实现拆分到 `crates/atto-agent-app/src/tool/readonly.rs`，保留 `tool.rs` 作为抽象/注册表模块。
- 已补充只读工具单元测试和 PTY `/tools` 断言。
- 拆分后已重新通过 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 已将 `TODO.md` 中 M3.3 标记为 `[DONE]` 并写入完成记录。

## M3.3 执行步骤
1. 检查最近提交是否明确提到与 M3.3 直接相关的未完成事项。
2. 阅读现有 tool 抽象、配置 workspace 逻辑、slash `/tools` 输出和相关测试。
3. 在 app crate 内实现只读工具注册与执行，确保所有路径解析后仍位于配置 workspace 内。
4. 为正常路径、越界路径、非法参数、搜索行为和注册表行为补充测试。
5. 运行格式化、lint 和测试验证；若出现未排期失败，先修复或更新 `TODO.md` 添加前置任务。
6. 将 `M3.3` 标记为 `[DONE]` 并记录验证结果。
7. 检查 diff 与状态，提交本次任务全部变更后停止。

## M3.3 验证结果
- `cargo fmt --all`：通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo test --workspace --all-targets`：通过；首次运行发现 PTY `/tools` 旧断言并已更新，重新运行通过；拆分 `tool/readonly.rs` 后再次运行通过。
- `cargo fmt --all -- --check`：通过。
