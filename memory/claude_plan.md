# 执行计划

本文件记录本次调用的可审查执行计划、关键决策和进度；不包含隐藏推理细节。

## 当前计划

1. 读取 `TODO.md`，定位第一个标题未带 `[DONE]` 的任务。
2. 只针对当前任务核对最近 Git 上下文，确认最新提交是否明确提到与该任务直接相关的未完成问题。
3. 阅读当前任务的要求、依赖、验证命令和完成记录要求。
4. 检查当前任务相关源码、测试和文档。
5. 完整实现当前任务；如发现阻塞正确实现的具体前置问题，则更新 `TODO.md` 并停止。
6. 按要求先运行 `cargo fmt`，再运行 `cargo clippy --workspace --all-targets -- -D warnings`，最后运行 `cargo test --all --all-targets`。
7. 如验证发现未被任务列表明确安排的失败，先修复或把最小前置任务加入 `TODO.md`，不得将当前任务标记完成。
8. 验证通过后，仅将当前任务标记为 `[DONE]` 并更新完成记录。
9. 仅当阶段级计划或依赖结构变化时更新 `PLAN.md`。
10. 检查工作区差异，只提交本任务相关文件。
11. 提交完成后停止，不继续处理下一个任务。

## 进度记录

- 已在读取项目任务文件和运行命令前初始化本次执行计划。
- 已读取 `TODO.md`；首个未完成任务是 `T14E — 拆分 app menu 巨型文件（M8）`。
- 当前任务范围：将 `src/app/menu.rs` 机械拆分为聚焦子模块，保持零行为变更，并运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`。
- 已检查 Git 状态和最近提交。最新提交为 `[T14D] Split window manager module`，未提到与 T14E 直接相关的未完成事项。
- 已观察到无关未跟踪脚本 `notification.sh` 与 `run_agent.sh`；本任务不会修改或暂存它们。
- 拆分方案：保留 `src/app/menu.rs` 作为公开 facade，将菜单模型、输入处理、绘制、布局 helper、最小化窗口菜单同步移入 `src/app/menu/` 下的职责子模块。
- 已完成机械拆分：新增 `model.rs`、`input.rs`、`draw.rs`、`layout.rs`、`minimized.rs`；`src/app/menu.rs` 仅保留模块声明和现有公开菜单 API re-export。
- 首次 clippy 发现 facade 中 `MenuCallback` re-export 未被使用；已移除该 re-export，以保持现有 `app` re-export 面并满足 `-D warnings`。
- `cargo fmt` 已通过。
- `cargo clippy --workspace --all-targets -- -D warnings` 已通过。
- `cargo test --all --all-targets` 已通过；未观察到未安排的测试失败。
- 已更新 `TODO.md`：将 `T14E` 标记为 `[DONE]` 并写入完成记录。`PLAN.md` 未更新，因为阶段级计划没有变化。
