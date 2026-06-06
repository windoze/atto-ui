# 当前执行计划

## 任务边界

- 本次只处理 `TODO.md` 中第一个标题未以 `[DONE]` 开头的任务。
- `TODO.md` 是任务顺序、依赖、验证要求和完成记录的唯一来源。
- 若发现当前任务被具体前置缺陷阻塞，则只添加或完成最小必要前置任务，并停止。
- 不进行开放式历史问题清扫，不跳过 review 类任务。

## 执行步骤

1. 读取 `TODO.md`，定位第一个未完成任务，并记录任务编号、要求、依赖和验证条件。
2. 查看最近提交信息，仅判断是否存在与该任务直接相关的未完成事项。
3. 针对当前任务读取相关代码、测试和文档，确认最小实现范围。
4. 实现当前任务；若遇到阻塞当前任务的规范缺口或测试/fixture 失败，按要求修复或在 `TODO.md` 插入最小前置任务后停止。
5. 运行格式化和静态检查：先 `cargo fmt`，再 `cargo clippy --all-targets -- -D warnings`。
6. 运行当前任务要求的测试；若需要完整验证，则运行完整测试套件并使用足够长超时。
7. 更新 `TODO.md`：将完成任务标题加 `[DONE]`，补充完成记录、变更摘要和验证结果。
8. 仅在阶段级计划、依赖或完成标准变化时更新 `PLAN.md`。
9. 提交所有本次任务相关改动，提交信息包含任务编号和简明说明。
10. 停止，不处理下一个任务。

## 进度记录

- 已创建计划文件，下一步读取 `TODO.md` 识别第一个未完成任务。
- 已识别第一个未完成任务：`R18 — 审阅 T18`。
- R18 审阅范围：toast 队列是否不阻塞主循环且不与状态栏冲突；`WindowedText` 超大块 windowing 是否不丢内容且展开正确；OSC8/图片多模态能力检测与降级是否安全；运行对应 PTY、clippy 和 workspace 测试。
- 审阅发现并修复 `WindowedText` 可见行路径的 materialization 问题：展开状态滚动到大文本尾部时，现在先定位可见窗口再构造绘制行，常规可见行保持 borrowed slice，避免为跳过的行创建 `String`。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui windowed_text -- --nocapture`；`cargo test --test pty_notifications_windowing_multimodal -- --nocapture`；`cargo test --workspace --all-targets`。
- 已将 `TODO.md` 中 `R18` 标记为 `[DONE]` 并补充完成记录。下一步提交本次任务相关改动后停止。
