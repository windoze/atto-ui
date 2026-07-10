# 执行计划

> 说明：本文件记录可审计的执行计划、关键决策和进度更新；不包含私有推理链。

## 范围

- 以 `TODO.md` 为任务顺序和验收要求的唯一来源。
- 本轮只完成第一个未标记 `[DONE]` 的任务。
- 完成或阻塞当前任务后停止，不处理后续任务。

## 当前任务

- 第一个未完成任务：`M5.7 快照与测试`。
- 任务要求：PTY 覆盖计划生成、Accept 后执行、Reject 后停止、未接受计划时工具被拒绝。
- 最近提交：`[M5.6] Gate mutating tools before plan acceptance`，未声明与 `M5.7` 直接相关的未完成事项。

## 执行步骤

1. 检查最近提交和当前工作区，确认没有直接阻塞 `M5.7` 的未完成事项。
2. 阅读现有 snapshot fixture、plan mode runtime 和 `pty_agent` 测试。
3. 在 `crates/atto-agent-app/tests/pty_agent.rs` 中补充端到端 PTY 覆盖。
4. 先运行 `cargo fmt --all`，再运行针对性 PTY 测试。
5. 运行 `cargo clippy --workspace --all-targets -- -D warnings`。
6. 运行完整 `cargo test --workspace --all-targets`。
7. 运行 `cargo fmt --all -- --check`。
8. 验证通过后，将 `M5.7` 在 `TODO.md` 标记为 `[DONE]` 并填写完成记录。
9. 检查 diff/status/log，提交本轮全部相关变更，然后停止。

## 进度

- 已读取 `TODO.md` 并确认当前任务为 `M5.7 快照与测试`。
- 已检查最近提交，无直接相关未完成事项。
- 已确认现有单元测试覆盖 plan decision 和 mutating tool gate，本轮只补齐任务要求的 PTY 覆盖。
- 已新增 PTY 测试：plan 生成与 Accept 后继续执行、Reject 后停止、未接受计划时 `run_command` 被 gate 拒绝。
- 首次针对性 PTY 运行发现测试断言不当：`AGENT-ALLOW-OUTPUT` 会作为工具输入 argv 显示，即使工具未执行。已改为断言成功 tool result 不出现。
- `cargo test -p atto-agent-app --test pty_agent` 已通过，10 个 PTY 测试全部成功。
- `cargo fmt --all` 已通过。
- `cargo clippy --workspace --all-targets -- -D warnings` 已通过。
- `cargo test --workspace --all-targets` 已通过。
- `cargo fmt --all -- --check` 已通过。
- `TODO.md` 已将 `M5.7` 标记为 `[DONE]` 并记录完成情况和验证命令。
- 提交前检查显示当前变更只包含 `TODO.md`、`crates/atto-agent-app/tests/pty_agent.rs` 和 `memory/claude_plan.md`。
