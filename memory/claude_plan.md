# 执行计划

## 当前约束

- 以 `TODO.md` 为任务顺序和完成状态的唯一依据。
- 只处理第一个标题未带 `[DONE]` 的任务，完成后停止。
- 完成后必须更新 `TODO.md` 的任务标题和完成记录，并提交 Git commit。
- 若遇到阻塞当前任务的缺陷或缺失能力，先修复；若无法在本次完成，则在 `TODO.md` 插入最小必要前置任务并提交后停止。
- 验证顺序遵循要求：先 `cargo fmt`，再 `cargo clippy --all-targets -- -D warnings`，最后运行相关或完整测试。

## 步骤

1. 读取 `TODO.md`，识别第一个未完成任务及其验证要求。
2. 查看最新提交信息，判断是否有与该任务直接相关的未完成事项。
3. 检查当前工作区状态，避免覆盖用户或其他代理的改动。
4. 阅读与该任务相关的代码、测试和文档，确定最小正确实现范围。
5. 实现该任务；若发现直接阻塞问题，按规则修复或写入新的前置任务。
6. 运行格式化、lint 和任务要求的测试；发现未排期失败时修复或排期。
7. 更新 `TODO.md`，将本任务标题加 `[DONE]` 并填写完成记录。
8. 复查 diff、状态和最近提交，提交本次任务相关全部改动。
9. 停止，不继续处理下一个任务。

## 进度

- 已建立初始执行计划。
- 已读取 `TODO.md`，首个未完成任务为 `NT8`：`react-reconciler HostConfig 骨架 + 节点 id + 静态渲染（U.1）`。
- 已查看最近提交，最新提交为 `NR7` 审阅任务；未发现提交信息中直接提示 `NT8` 的未完成事项。
- 当前工作区除本计划文件外，还有未跟踪的 `notification.sh`、`run_agent.sh`；它们与当前任务无关，保持不修改。
- 已读取 `TODO-1.md` 与 `PLAN-1.md` 中 `NT8/U.1` 要求：新增 `packages/react`，实现 LegacyRoot mutation HostConfig 骨架、节点 id、静态 React 树首次挂载到窗口，并覆盖 reconciler 纯单测与 headless 渲染测试。
- 已检查 `packages/core` 类型与 Node 测试结构。
- 已新增 `packages/react` 包骨架、Host 实例模型、React reconciler 接线、静态 `set_tree` flush、纯 JS reconciler 测试与 headless 测试。
- 已安装 `packages/react` 本地依赖并生成 lockfile。
- 已修正 TypeScript 与测试中暴露的问题；`npm run typecheck --prefix packages/react` 与 `npm test --prefix packages/react` 已通过。
- 已完成验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`、Node binding napi build 与 JS 测试、`packages/core` 类型检查与 JS 测试、`packages/react` 类型检查与 JS/headless 测试均通过。
- 未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。
- 已更新 `TODO.md` / `TODO-1.md` 的 `NT8` 完成状态与完成记录。
- 已复查新增文件、diff 与 Git 状态，并提交本次任务主要改动：`98c0bb7 [NT8] Add React reconciler static rendering`。
- 下一步提交本进度文件的最终状态后停止，不继续处理 `NR8`。
