执行计划（不包含私密推理链）：

1. 读取 TODO.md，按文件顺序确认第一个标题未带 [DONE] 的任务；只围绕该任务建立上下文。
2. 查看最新提交信息，若其明确提到与当前任务直接相关的未完成问题，将其纳入当前任务或作为前置任务记录到 TODO.md。
3. 阅读当前任务涉及的 PLAN.md、源码、测试与既有完成记录，确认需求、依赖、验证命令和边界条件。
4. 若发现当前任务被未跟踪的具体前置问题阻塞，最小化更新 TODO.md 记录前置任务并停止；否则直接实现当前任务。
5. 按最小正确改动编辑代码与测试；如计划发生关键变化或完成关键步骤，及时更新本文件。
6. 先运行 cargo fmt，再运行 cargo clippy --all-targets -- -D warnings，最后运行相关测试和必要的完整测试套件。
7. 若发现未调度的测试或夹具失败，修复它或在 TODO.md 中安排为当前任务完成前的前置项。
8. 完成后将当前任务标题加上 [DONE]，更新完成记录；仅在阶段计划变化时更新 PLAN.md。
9. 检查 git status、diff 和最近提交，提交本次任务涉及的所有变更，然后停止，不继续下一个任务。

当前任务：`R7 — 审阅 T7`。

R7 执行步骤：

1. 检查最新提交摘要，确认是否有与 T7/R7 直接相关的未完成事项。
2. 审阅 `atto-ui-async` crate、workspace feature 配置、`atto-ui-components` async 透传、async 运行入口、spawn/cancel 联动和 PTY 测试。
3. 验证 core `atto-ui` 依赖图无 tokio，默认 workspace 构建不被新 crate 破坏，feature 开启下 async 路径和 PTY 测试确定性。
4. 若发现缺陷，先修复同类根因并补测试；若发现阻塞且无法直接完成，按规则更新 TODO 后停止。
5. 验证通过后，将 R7 标记为 `[DONE]` 并填写完成记录。
6. 提交本次审阅、验证与记录变更。

进度更新：已确认第一个未完成任务为 R7；最新提交 `[T7] Add async runtime crate` 未声明直接相关未完成项；已审阅 T7 相关 async crate、feature 配置、运行入口、取消联动与 PTY fixture，暂未发现需要先修复的缺陷。

进度更新：R7 验证已完成并通过，包括 `cargo fmt`、workspace clippy、async/components feature clippy、core/default 依赖图检查、`atto-ui-async` 默认与 `event-stream` feature 测试，以及完整 `cargo test --workspace --all-targets`。已将 R7 在 TODO.md 标记为 `[DONE]` 并补充完成记录。
