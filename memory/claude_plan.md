# 执行计划

本文件记录本次调用的可审查执行计划、关键决策和进度；不包含隐藏推理细节。

## 当前目标

完成 `TODO.md` 中首个未完成任务：`R14 — 审阅 T14A–T14E`。完成后更新任务记录、提交变更并停止，不继续处理 T15。

## 执行计划

1. 读取 `TODO.md`，定位第一个标题未带 `[DONE]` 的任务。
2. 只针对当前任务核对最近 Git 上下文，确认最新提交是否明确提到与该任务直接相关的未完成问题。
3. 审阅 T14A–T14E 涉及的拆分文件，重点检查 facade 导出、模块边界、trait impl/测试迁移和行为保持。
4. 若发现拆分引入的回归，直接修复；不得绕过或缩小审阅范围。
5. 按顺序运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`。
6. 验证通过后，将 R14 标记为 `[DONE]` 并写入完成记录。
7. 检查最终 diff，仅暂存本任务相关文件，提交后停止。

## 进度记录

- 已在任务执行前初始化本次计划文件。
- 已读取 `TODO.md`；首个未完成任务是 `R14 — 审阅 T14A–T14E`。
- 已检查最近提交；最新五个提交为 T14A 到 T14E，提交标题未显示与 R14 直接相关的未完成事项。
- 已观察到无关未跟踪脚本 `notification.sh` 与 `run_agent.sh`；本任务不修改、不暂存它们。
- 已审阅 T14A editor view 拆分、T14B editor app window 拆分、T14C runtime 拆分、T14D window manager 拆分、T14E app menu 拆分。
- 审阅发现一个 T14E facade 回归：`src/app/menu.rs` 不再提供拆分前位于 `menu` facade 的 `MenuCallback` 别名。
- 已修复该回归：将 `MenuCallback` 恢复到 `src/app/menu.rs`，并让 `model`/`input` 通过 `super::MenuCallback` 使用它；未新增拆分前不存在的 `app` 顶层导出。
- 初次修复使用 `pub use` 触发 `unused_imports`，第二次仅在 facade 定义但内部未使用触发 `dead_code`；最终采用“facade 定义 + 内部引用 facade 别名”的形态，匹配拆分前所有权且无 lint 抑制。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。
- 已将 `TODO.md` 中 R14 标记为 `[DONE]`，并写入完成记录。
