# 当前执行计划

## 说明

- 本文件记录可公开的执行计划、关键决策和进度，不包含私密推理链路。
- `TODO.md` / `TODO-2.md` 是任务顺序、要求和完成状态的依据；本次只完成第一个未完成任务后停止。

## 执行步骤

1. 先读取 `TODO.md`，识别标题未带 `[DONE]` 的第一个任务。
2. 只检查最新提交中是否有与该任务直接相关的未完成事项。
3. 读取所选任务的文件范围、步骤、测试、验收和完成记录要求。
4. 只检查与该任务相关的代码和测试，不做开放式历史问题扫描。
5. 按任务原文实现；若出现阻塞该任务的规格缺口，优先修复或在 `TODO.md` 中加入最小前置任务后停止。
6. 按顺序运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test`。
7. 将完成任务在 `TODO.md` / `TODO-2.md` 中显式标记为 `[DONE]`，并填写完成记录。
8. 检查 Git 状态和 diff，只提交本任务相关文件，不提交无关未跟踪文件。
9. 提交后停止，不处理下一个任务。

## 进度记录

- 已在执行仓库命令前记录本次公开执行计划。
- 已选择首个未完成任务：`T2 — C1 WindowManager 全局拖拽会话与反馈绘制`。
- 最新提交 `ccf02a7 [R1] Review T1 drag and drop hooks` 未提到会改变 T2 顺序的未完成事项。
- 检查 T2 时发现直接阻塞点：`Window::new` 会用 `WindowMinSizeView` 包装视图，必须转发 drag/drop hooks，否则全局拖拽无法触达实际组件；该修复已纳入 T2 范围。
- 已实现 T2：`WindowManager.global_drag`、source/target hook 调用、threshold 激活、drop/cancel、Esc 取消、overlay 绘制、主题 named styles、`WindowMinSizeView` hook 转发、单元测试和 PTY fixture/test。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test`。
- 已在 `TODO.md` 与 `TODO-2.md` 中将 T2 标记完成并写入完成记录。
