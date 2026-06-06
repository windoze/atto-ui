# 当前执行计划

## 约束

- 以 `TODO.md` 为唯一任务排序和完成状态来源。
- 只处理第一个标题未带 `[DONE]` 的任务，完成后停止。
- 不做开放式历史问题扫描；只处理会阻塞当前任务或验证的缺陷。
- 若发现当前任务依赖未跟踪的前置问题，先把最小前置任务插入 `TODO.md`，提交后停止。
- 完成任务后必须更新 `TODO.md` 的标题与完成记录，并按要求提交 Git commit。
- 验证顺序为 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、完整测试；仅文档变更且可复用已有绿色结果时才跳过相关套件。

## 步骤

1. 读取 `TODO.md`，定位第一个标题未带 `[DONE]` 的任务。
2. 检查最新提交信息是否明确提到与该任务直接相关的未完成事项；若是，将其纳入当前任务或作为前置任务记录。
3. 根据当前任务读取必要代码、测试、计划文件和相关上下文，避免无关范围扫描。
4. 实现当前任务，优先采用最小正确改动，避免 workaround 或规格偏离。
5. 添加或更新直接相关测试，必要时修复阻塞当前任务的失败测试或把未跟踪失败登记为前置任务。
6. 按要求运行格式化、lint 和测试验证。
7. 更新 `TODO.md`：给完成任务标题加 `[DONE]`，填写完成记录；只有阶段级计划变化才更新 `PLAN.md`。
8. 检查 Git 状态和 diff，提交本次任务相关全部未提交变更。
9. 停止，不继续处理下一个任务。

## 进度

- 已创建本计划文件，下一步读取 `TODO.md` 定位第一个未完成任务。
- 已定位第一个未完成任务：`T18 — 通知队列 + 超大块 windowing + 多模态（C.4）`。
- 下一步检查最新提交是否含有与 T18 直接相关的未完成事项，然后读取相关 app/composable/drawing 测试与实现上下文。
- 最新提交未明确提到与 T18 直接相关的未完成事项。
- 当前实现方案：在 `src/app` 增加 toast 队列并由 `Desktop::draw` 作为独立 overlay 绘制；在 `src/composable` 增加 `WindowedText`，默认软截断并支持展开后按可见区域 windowed 渲染；在 `src/drawing.rs` 增加 OSC8 链接序列、终端图片协议序列构造与无能力降级。
- 工作树已有未跟踪 `notification.sh`、`run_agent.sh`，视为非本任务文件，后续不修改也不提交。
- 已完成首轮实现与 PTY fixture 编写，下一步执行 `cargo fmt` 并根据编译/lint 结果修正。
- 已通过验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、新增 PTY 测试、`cargo test --workspace --all-targets`。
- 已将 `TODO.md` 中 T18 标记为 `[DONE]` 并补充完成记录；未更新 `PLAN.md`，因为阶段级计划未变化。
- 下一步检查 diff/status，提交本次 T18 相关变更后停止。
