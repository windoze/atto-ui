# 当前执行计划

说明：本文件记录可审计的执行计划、关键决策和进度更新，不记录私有推理细节。

## 步骤

1. 阅读 `TODO.md`，按文件顺序识别第一个标题未带 `[DONE]` 的任务。
2. 如最新提交明确提到与该任务直接相关的未完成事项，检查是否属于当前任务范围或需要作为前置任务写入 `TODO.md`。
3. 阅读当前任务涉及的源码、测试和文档，确认验收要求、依赖和禁止事项。
4. 完整实现该任务；如果发现阻塞当前任务的规格缺口或测试失败，优先修复，或在 `TODO.md` 中插入最小必要前置任务后停止。
5. 按要求运行验证：先 `cargo fmt`，再 `cargo clippy --all-targets -- -D warnings`，最后运行完整测试套件（如需）。
6. 更新 `TODO.md`：任务完成时在标题前加 `[DONE]` 并填写完成记录；仅在阶段计划确实变化时更新 `PLAN.md`。
7. 检查 git 状态和差异，提交本次任务相关全部变更。
8. 完成一个任务后停止，不继续处理下一个任务。

## 进度

- 已写入初始计划。
- 已读取 `TODO.md`，第一个未完成任务为 `P5.3 引用回复（可选）`。
- 最新提交为 `[P5.2] Add chat turn folding`，与当前 P5 阶段连续，未发现需要先插入的显式未完成事项。
- 下一步阅读 `crates/atto-ui-chat/src/list.rs`、`input.rs` 及相关模型/测试，确认现有 action row、输入状态和测试模式。
- 已确认实现边界：复用现有 action row/button 和 `ChatInputHandle` binding 模式，为 turn/block 添加引用附加入口；输入区显示引用摘要和移除控件；提交或手动移除后清理引用。
- 已完成初版代码改动：`ChatInputReference`、输入引用栏、提交时 Markdown 引用前缀合成、列表 `Quote` / `Quote block` 按钮和相关单测。
- 定向测试已通过：引用栏渲染/移除、文本提交携带引用并清理、turn 引用按钮、block 引用按钮。
- 完整验证已通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 已更新 `TODO.md`，将 `P5.3 引用回复（可选）` 标记为 `[DONE]` 并写入完成记录。
- 已提交本次 P5.3 实现：`b061710 [P5.3] Add chat quote replies`。
- 本次调用到此停止，不继续处理 P5.4。
