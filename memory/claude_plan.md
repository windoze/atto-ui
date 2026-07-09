# 执行计划

## 当前状态

- 已读取 `TODO.md`，第一个未完成任务是 `P4.2 Esc 中断语义`。
- 任务范围：完善 `crates/atto-ui-chat` 中 `src/list.rs` 与 `input.rs` 的 Esc 流式中断状态机，使一次 Esc 可中断当前流式并置 `ChatTurnStatus::Canceled`，并与现有取消按钮统一入口。
- 已完成实现草案：新增共享 streaming cancel controller，取消按钮和未消费 Esc 都复用同一入口；`ChatPanel::new` 自动把列表取消入口接入输入区；新增单测和 PTY Esc 覆盖。
- 已完成验证并更新 `TODO.md`：P4.2 已标记 `[DONE]`，完成记录包含实现、分级语义、测试覆盖和验证命令。

## 步骤

1. 检查最近提交信息是否明确提到与 P4.2 直接相关的未完成问题。
2. 读取 `input.rs`、`list.rs`、消息状态模型和现有取消/流式测试，确认当前取消入口与 Esc 键处理路径。
3. 设计并实现最小统一取消入口：输入区 Esc 在合适状态下触发当前流式取消；按钮取消与 Esc 共享同一 store/status 更新和回调路径；非流式 Esc 继续遵守现有 popup/编辑等优先级。
4. 补充单测覆盖一次 Esc 取消当前流式、非流式 Esc 不误取消、取消按钮与 Esc 入口一致，以及必要的分级/连按语义。
5. 如已有 PTY fixture 能覆盖，新增或更新最小 PTY 场景；否则先用单测覆盖 P4.2 并保留 P4.4 的 PTY 总体验收。
6. 运行 `cargo fmt --all`。
7. 运行 `cargo clippy --workspace --all-targets -- -D warnings`。
8. 运行 `cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`，完整测试使用不少于 30 分钟超时。
9. 若发现未被明确排期的测试失败，修复它或在 `TODO.md` 中按依赖顺序新增最少任务。
10. 将 P4.2 标题加上 `[DONE]`，并更新完成记录。
11. 检查 `git status`、`git diff`、最近提交，确认提交范围。
12. 用描述性提交信息提交变更。
13. 停止，不继续下一个任务。

## 验证策略

- 先格式化，再 lint，最后跑测试。
- 若本次仅修改文档且自上次绿色全量测试后没有代码变化，可复用上次绿色结果并在完成记录中说明跳过原因。

## 已执行验证

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all -- --check`
- `cargo test -p atto-ui-chat escape --lib`
- `cargo test -p atto-ui-chat streaming_cancel --lib`
- `cargo test -p atto-ui-chat list_escape_cancels_latest_streaming_turn_once --lib`
- `cargo test -p atto-ui-chat --test pty_chat chat_streaming_escape_emits_and_marks_turn_canceled -- --nocapture`
- `cargo build --workspace --all-targets`
- `cargo test --all --all-targets`
