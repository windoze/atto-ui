# Claude 执行计划

## 范围

- 任务来源：`TODO.md`。
- 本轮目标：只完成第一个未完成任务，标记 `[DONE]`，完成验证并提交，然后停止。
- 当前任务：`P2.5 快照与测试`。
- 说明：本文件记录可审查的计划、决策、阻塞和进度，不记录隐藏推理链。

## 执行计划

1. 检查最近提交信息是否明确提到与 `P2.5` 直接相关的未完成事项。
2. 阅读 `snapshot_chat_app`、现有 chat PTY 测试、P2.2/P2.3 输入补全实现与测试入口。
3. 为 `snapshot_chat_app` 增加 slash/mention 专用场景，提供可稳定触发的命令、文件候选和事件输出。
4. 新增或扩展 PTY 测试，覆盖 `/` 触发、过滤、选择、确认，`@` 触发、文件补全、确认插入，以及 Esc 关闭。
5. 先运行针对性测试，修复发现的问题；再按要求运行格式化、clippy、格式检查、构建和完整测试套件。
6. 更新 `TODO.md`：将 `P2.5` 标题加 `[DONE]`，补全完成记录和验证命令。
7. 检查 git 状态与 diff，提交本轮改动，提交后停止。

## 进度

- 已读取 `TODO.md`，确认第一个未完成任务为 `P2.5 快照与测试`。
- 已写入本轮执行计划，下一步检查最近提交和相关代码。
- 最近提交为 `402b991 [P2.4] Sync chat completion runtime APIs`，未明确提到与 `P2.5` 直接相关的未完成事项。
- 当前工作区未提交改动仅有本轮 `memory/claude_plan.md`。
- 已定位主要改动点：`crates/atto-ui-chat/src/bin/snapshot_chat_app.rs` 和 `crates/atto-ui-chat/tests/pty_chat.rs`。
- 实现决策：新增 `--input-completion` snapshot 场景，注入确定性 slash 命令、mention provider 与 slash 回调输出；该场景下禁用 snapshot 的普通字符快捷键拦截，让 `/`、`@` 查询可以真实进入输入框。
- 已完成初版代码改动：新增 snapshot 场景、slash/mention 候选与两个 PTY 用例；下一步运行格式化和验证。
- 已运行 `cargo fmt --all`。
- 新增 PTY 用例验证通过：`cargo test -p atto-ui-chat --test pty_chat completion -- --nocapture`。
- 完整验证通过：`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 已更新 `TODO.md`，将 `P2.5` 标记为 `[DONE]` 并补全完成记录。
- 下一步提交本轮改动并停止。
