# 执行计划

## 当前约束

- `TODO.md` 是任务顺序、依赖、完成标准和完成记录的唯一权威来源。
- 本轮只完成第一个标题未带 `[DONE]` 的任务，然后停止。
- 如果发现阻塞当前任务的具体前置问题，优先修复；无法直接修复时，在 `TODO.md` 中插入最少必要的前置任务并提交后停止。
- 不做开放式历史问题清扫，不跳过 review 任务，不用缩小范围或临时 workaround 规避规格问题。
- 代码变更后按要求运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行完整测试；若仅文档变更且已有可复用绿色结果，则记录跳过原因。

## 步骤计划

1. 读取 `TODO.md`，定位第一个标题未带 `[DONE]` 的任务，并读取其要求、依赖、验证标准和完成记录。
2. 检查最新提交信息，仅在它明确提到与当前任务直接相关的未完成问题时，将其纳入当前任务或作为前置任务记录。
3. 按当前任务需要读取相关代码、测试和文档，限定范围在完成该任务所需上下文内。
4. 实现当前任务；若需要编辑，使用小而聚焦的补丁，并在关键进展后更新本计划文件。
5. 运行格式化、lint 和相关测试；若完整测试需要运行，使用足够超时时间。
6. 对发现的失败测试按策略处理：修复，或在 `TODO.md` 中安排为当前任务完成前的明确任务。
7. 更新 `TODO.md`：给当前完成任务标题加 `[DONE]`，补充完成记录和验证结果；仅在阶段级计划改变时更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、最近提交，确认只提交本轮应包含的变更；若是恢复前次未提交任务，则按要求纳入所有当前未提交文件。
9. 创建一个清晰的 Git 提交，然后停止，不继续下一个任务。

## 进度记录

- 已创建本执行计划，下一步读取 `TODO.md` 确认首个未完成任务。
- 已确认首个未完成任务为 `M4.5 Prompt 注入`。最新提交为 `[M4.4] Add automatic skill loading`，未声明与 M4.5 直接相关的未完成事项。
- 下一步定位 DeepSeek request/context 构建和 skill 运行时状态，设计最小 prompt 注入入口与预算限制测试。
- 已在 `skill.rs` 中加入 `<skills>` prompt block 构建、默认 6 KiB 单 skill body / 20 KiB 总 prompt 预算和 UTF-8 安全截断。
- 已在 `lib.rs` 中新增带 skill 注入的 transcript request/messages 构建入口，并补充单元测试验证注入位置和工具 schema 保持不变。
- 下一步运行 `cargo fmt` 和相关单测，随后按要求运行 clippy 与完整测试。
- 已完成验证：`cargo fmt --all`、新增相关单测、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- 已更新 `TODO.md`，将 `M4.5 Prompt 注入` 标记为 `[DONE]` 并记录完成内容和验证命令；`PLAN.md` 阶段级计划未变化。
- 下一步检查 git 状态、diff 和最近提交，确认提交范围后创建本轮提交并停止。
