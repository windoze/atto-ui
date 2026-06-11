执行计划（记录可检查的推理摘要与步骤，不包含私有链式思考）

1. 读取 `TODO.md`，按文件顺序找到第一个标题未带 `[DONE]` 的任务，并只处理该任务。
2. 检查该任务的依赖、验收标准、完成记录要求，以及最近提交是否明确提到与该任务直接相关的未完成事项。
3. 读取与当前任务直接相关的代码、测试和文档，避免进行无关的历史问题扫查。
4. 若任务可直接完成，则做最小且完整的实现；若发现阻塞当前任务的缺失能力或规范不匹配，则在 `TODO.md` 中添加最小前置任务并停止。
5. 按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，通过后再运行相关测试或完整测试套件。
6. 验证通过后，将当前任务标题加上 `[DONE]`，更新其完成记录；仅在阶段级计划实际变化时更新 `PLAN.md`。
7. 检查 git 状态和差异，提交本轮所有相关改动，然后停止，不处理下一个任务。

当前状态：已读取 `TODO.md`，第一个未完成任务是 `P8.2 子 agent / Task 嵌套块`；最近提交是 `[P8.1] Add plan mode block`，未发现直接阻塞 P8.2 的未完成事项。

P8.2 具体执行计划：

1. 在 Rust 模型中新增 `ChatBlock::Task(TaskBlock)`，包含 `title`、`status`、`summary`、`transcript`、`collapsed`；`transcript` 使用带 `role` 与 `blocks` 的嵌套 transcript item，避免把子 agent 内容伪装成工具输出文本。
2. 在 `ChatMessageStore` 增加 task 块的块级更新 API：更新状态、摘要与嵌套 transcript，保持未变化不发脏通知并 bump block version。
3. 在 `dynamic.rs` 增加 task block 递归序列化/解析，TS/core/react/node 类型与 builders 同步 `task` block 形。
4. 在 `list.rs` 增加 task row kind、占位块、行高估算、绑定同步和 `Disclosure` 渲染；折叠只显示任务标题/状态，展开显示摘要与嵌套 transcript/block 行。
5. 在 `snapshot_chat_app` 增加 `--nested-task` 场景，按键触发 task 状态/摘要/transcript 更新；新增 PTY 覆盖折叠/展开、嵌套内容、状态更新和虚拟化可见窗口行为。
6. 补充 Rust 单元测试、dynamic round-trip、core/react 类型和 builder 测试，更新 `docs/NODE_API.md`。
7. 按顺序运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、构建与测试；完成后更新 `TODO.md` 完成记录并提交。

进度更新：已完成 P8.2 的首轮实现，包含 Rust `TaskBlock` 模型、store 块级更新、dynamic 递归序列化/解析、列表折叠渲染、`snapshot_chat_app --nested-task` 场景、PTY/单元测试草案，以及 core/react/node/docs 同步。下一步运行格式化、lint 和测试，根据结果修复问题。

验证进度：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 已通过；core/react/node 相关 npm typecheck/build/test/smoke 也已通过。下一步更新 `TODO.md` 完成记录并提交本任务。

完成记录进度：`TODO.md` 已将 P8.2 标记为 `[DONE]` 并补充完成记录与验证命令。下一步检查 git 状态/差异并提交本任务改动。
