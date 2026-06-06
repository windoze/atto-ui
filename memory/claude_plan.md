# 执行计划

## 当前状态

- 本次调用目标：只完成 `TODO.md` 中第一个未标记 `[DONE]` 的任务，然后停止。
- 计划文件已在开始执行其他检查前初始化。
- 已定位当前任务：`T11 — 仅可见行 parse + 借用替代 clone（M11）`。
- 最近提交 `[R10] Review shared widget abstractions` 未明确提出与 T11 直接相关的未完成阻塞项。

## 步骤

1. 读取 `TODO.md`，按文件顺序定位第一个标题未以 `[DONE]` 标记的任务。
2. 检查最近提交信息，只有当它明确提到与当前任务直接相关的未完成问题时，才把该问题纳入当前任务或作为前置任务写入 `TODO.md`。
3. 阅读当前任务要求、依赖、验证条件和完成记录，必要时查看相关代码与测试。
4. 实现当前任务；如果发现阻塞当前任务的真实前置缺口，按最小必要范围更新 `TODO.md` 并停止。
5. 运行格式化、lint 和相关测试；若有未安排的失败，修复或把最小前置任务写入 `TODO.md`。
6. 将当前任务标题更新为 `[DONE]` 并填写完成记录；仅在阶段计划真实改变时更新 `PLAN.md`。
7. 检查工作区差异，提交本次任务相关全部变更。
8. 停止，不继续处理下一个任务。

## 进度记录

- 已完成：读取 `TODO.md` 并确定第一个未完成任务为 T11。
- 已完成：检查 `src/widgets/list.rs`、`src/widgets/table.rs` 中 `bindings()` clone 与 `draw` parse 范围。
- 已完成：新增共享可见行区间 helper，并将 ListBox/TableView 内容渲染限制为当前可见行。
- 已完成：移除 ListBox/TableView 内容对象的 `bindings()` 整体 clone helper，改为各方法中的局部 read guard。
- 已完成：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test visible_row_range --lib`、`cargo test --test pty_virtual_scrolling`、`cargo test --all --all-targets` 均通过。
- 已完成：`TODO.md` 中 T11 已标记 `[DONE]` 并填写完成记录。
- 已完成：检查差异；本次待提交文件为 `TODO.md`、`memory/claude_plan.md`、`src/widgets/list.rs`、`src/widgets/table.rs`、`src/widgets/util.rs`。工作区另有未跟踪 `notification.sh`、`run_agent.sh`，本次不触碰。
- 下一步：提交 T11 相关变更。
