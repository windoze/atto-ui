# 执行计划

## 约束

- 输出、进度记录和最终说明使用中文。
- `TODO.md` 是任务顺序、任务要求、依赖和完成记录的唯一权威来源。
- 本次只完成 `TODO.md` 中第一个标题未带 `[DONE]` 的任务，完成后提交并停止。
- 不做开放式历史问题扫描；只处理当前任务直接相关或测试暴露且未被明确排期的问题。
- 在修改代码前先阅读相关上下文；手工编辑使用小而集中的补丁。
- 验证顺序为 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、完整测试套件；如果仅文档变更且已有可复用的绿色结果，再按任务要求记录跳过原因。

## 初始步骤

1. 读取 `TODO.md`，按标题是否带 `[DONE]` 判断第一个未完成任务。
2. 查看最新提交信息，只在其明确提到与当前任务直接相关的未完成事项时纳入当前任务或作为前置任务记录到 `TODO.md`。
3. 阅读当前任务涉及的计划、代码和测试上下文，确认实现范围和验收要求。
4. 如发现当前任务被未排期的具体前置问题阻塞，最小化新增前置任务到 `TODO.md`，提交后停止。

## 执行步骤

1. 针对第一个未完成任务做最小但完整的实现。
2. 为新增或修复行为补充聚焦测试，优先遵循仓库现有 PTY/单元测试模式。
3. 运行格式化、lint 和必要测试；若出现未排期失败，修复或在 `TODO.md` 中排入正确位置。
4. 将已完成任务标题加上 `[DONE]`，更新其完成记录，必要时只在阶段级计划变化时更新 `PLAN.md`。
5. 检查工作区变更，确保不回滚用户已有改动。
6. 用清晰的任务消息提交本次所有相关变更。
7. 停止，不继续下一个任务。

## 进度记录

- 已创建初始执行计划，下一步读取 `TODO.md` 并确定第一个未完成任务。
- 已读取 `TODO.md`，第一个未完成任务是 `M2-4 Slider apply_command`。
- 最新提交为 `[M2-3] Implement textbox apply command`，未明确提到与 `M2-4` 直接相关的未完成事项。
- 本任务范围：为 `src/widgets/slider.rs` 实现语义级 `apply_command`，选定并记录命令形态，补充进程内测试，验证 `value` / `progress` 更新、min/max clamp 和越界不 panic。
- 下一步：阅读 `ComponentCommand`、`Slider` 的现有键盘/鼠标调值路径、属性暴露和相邻组件 `apply_command` 实现，确认应复用的状态转移函数。
- 已确认 `Slider` 现有调值路径集中在 `snap_value` / `clamp_value` / `set_value_and_emit`；`apply_command` 应复用这些 helper。
- 命令形态选择：使用 `ComponentCommand::SelectIndex(usize)` 表示「从规范化 min 起按 step 计算的第 N 个刻度」，即 `min + index * abs(step)`，再经现有 snap / clamp 逻辑落到合法值。
- 验收补充：为 `Slider` 增加只读虚拟属性 `progress`，按 clamp 后的 `(value - min) / (max - min)` 返回 `0.0..=1.0`；这样 `TODO.md` 要求的 `value` / `progress` 读值断言都可覆盖。
- 已修改 `src/widgets/slider.rs`：新增 `SelectIndex` 语义派发、`progress` 只读属性，以及覆盖正常设值、越界 clamp、反向 min/max、禁用态 ignored 和 callback payload 的进程内测试。
- 下一步：先运行聚焦的 slider 测试；若通过，再按要求执行 fmt、clippy 和完整 workspace 测试。
- 聚焦验证已通过：`cargo test -p atto-ui slider -- --nocapture`。
- 下一步：运行 `cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`，最后运行完整 `cargo test --workspace --all-targets`。
- 全部验证已通过：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。
- 下一步：更新 `TODO.md` 的 `M2-4` 标题为 `[DONE]`，写入完成记录和验证记录，然后检查 diff 并提交。
