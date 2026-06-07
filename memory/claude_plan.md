执行计划（公开摘要）

1. 读取 `TODO.md`，只定位第一个标题未带 `[DONE]` 的任务，并记录任务编号、要求、依赖与验证标准。
2. 检查最近提交信息是否明确提到与该任务直接相关的未完成问题；仅在其阻塞当前任务时纳入当前范围或作为前置任务记录到 `TODO.md`。
3. 根据当前任务范围检查相关代码与测试，避免开放式历史问题排查。
4. 实现第一个未完成任务；如遇到规范级阻塞，更新 `TODO.md` 插入最小前置任务并停止。
5. 按要求运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行相关/完整测试；若发现未排期失败，修复或排入 `TODO.md`。
6. 完成后在 `TODO.md` 将该任务标题前缀改为 `[DONE]`，更新完成记录；只有阶段计划变更时才更新 `PLAN.md`。
7. 提交本次任务相关全部变更，提交后停止，不进入下一个任务。

进度记录

- 已写入初始执行计划，下一步读取 `TODO.md` 定位第一个未完成任务。
- 已定位第一个未完成任务：`NR8 — 审阅 NT8`。最近提交为 NT8 完成记录/实现提交，未发现提交标题中直接标记的未完成阻塞项。
- 下一步聚焦 `packages/react` 的 HostConfig、host instance/id 生命周期、静态渲染测试与 headless 测试，不做无关历史问题排查。
- 已补充 `packages/react/__test__/reconciler.cjs` 覆盖 host instance 的 parent/windowId 生命周期、同一实例重渲染 id 稳定性，以及默认 container id 前缀唯一性。
- `npm test --prefix packages/react` 已通过；下一步按要求运行格式化、lint 与完整测试。
- 验证已完成：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`、`npm run typecheck --prefix packages/react`、`npm test --prefix packages/react` 均通过；未找到 `tools/run_fixtures.py`。
- 已在 `TODO.md` 与 `TODO-1.md` 将 `NR8` 标记为 `[DONE]` 并写入完成记录；下一步检查 diff/status 后提交本任务变更。
- 已提交主要变更：`84d14cb [NR8] Review React reconciler static rendering`。下一步提交本进度文件最终状态后停止。
