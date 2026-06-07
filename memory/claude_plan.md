## 执行计划

我会先以 `TODO.md` 作为唯一任务来源，找出第一个标题未带 `[DONE]` 的任务。随后只围绕该任务建立上下文，避免做开放式历史问题排查。

步骤：

1. 读取 `TODO.md`，确认第一个未完成任务的编号、要求、依赖和验证要求。
2. 如有必要，查看最新提交信息，判断是否存在与当前任务直接相关的未完成事项。
3. 检查与当前任务相关的源码、测试和文档，确定最小正确实现范围。
4. 按任务要求完成实现；如果发现阻塞当前任务的真实缺口，优先修复，或在 `TODO.md` 中插入最小 prerequisite 任务并停止。
5. 运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后按任务要求运行相关测试；如需要全量验证，使用足够长的超时。
6. 更新 `TODO.md`：将已完成任务标题加上 `[DONE]`，并补全 completion record；仅当阶段计划发生变化时才更新 `PLAN.md`。
7. 提交本次任务涉及的全部变更，提交信息包含任务编号和明确动作。
8. 完成一个任务后停止，不继续处理下一个任务。

当前状态：已确认最新提交 `a0fe556 [NR6] Record completion progress` 不包含与 NT7 直接相关的未完成事项；工作区已有非本次创建的 `notification.sh`、`run_agent.sh` 未跟踪文件，后续不触碰。

NT7 实施细化：

1. 在 `styled_text.rs` 中补结构化 `StyledTextSegment` 构造入口，加入颜色字段并保持 `spans_from_segments` / `slice_segments` / `hit_test_link` 管线复用。
2. 新增 `src/widgets/rich_text.rs`，实现 `TextSpan`（结构化 props）与 `RichText`（遍历 `TextSpan` 子节点生成 segments、链接点击发 `link` 事件）。
3. 在 `widgets/mod.rs`、`composable/mod.rs` 和 `runtime/builtins.rs` 注册并导出 `RichText` / `TextSpan`，schema 中 `TextSpan` 不允许 children，`RichText` 允许 children 并暴露 `link` 事件。
4. 补单元测试覆盖结构化样式、合并/空 span 清理、schema、callback payload 与动态 spec 构建；补 PTY fixture 覆盖链接点击。
5. 按要求运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`，再更新任务完成记录并提交。

进度更新：已完成初版实现，包括结构化 `StyledTextSegment`、`RichText`/`TextSpan` 组件、runtime builtin 注册、schema/unit 测试和 PTY fixture。下一步先运行格式化，再处理编译/测试反馈。

验证更新：`cargo fmt` 已运行，`cargo clippy --workspace --all-targets -- -D warnings` 已通过。下一步运行完整 Rust 测试套件 `cargo test --all --all-targets`。

完成记录更新：`cargo test --all --all-targets` 已通过。已将 `TODO-1.md` 中 NT7 标记为 `[DONE]` 并写入完成记录，也已同步更新 `TODO.md` 索引状态。下一步检查 diff/status 后提交本任务变更。
