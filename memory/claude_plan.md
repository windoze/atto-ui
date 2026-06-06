# 执行计划

## 范围与原则
- 本轮只处理 `TODO.md` 中第一个标题未以 `[DONE]` 开头的任务，完成后停止。
- `TODO.md` 是任务顺序、约束、验证要求和完成记录的权威来源。
- 不做开放式历史问题扫查；只有阻塞当前任务或使当前任务行为无效的问题才纳入本轮。
- 不采用规避实现、削弱测试或改变规格的方式推进。
- 本文件记录可审计计划、关键进展和结果；不记录隐藏推理过程。

## 初始步骤
1. 读取 `TODO.md`，定位第一个未完成任务。
2. 检查最近提交信息是否明确提到与该任务直接相关的未完成问题。
3. 阅读当前任务涉及的代码、测试和文档，确认需求、依赖和验证命令。
4. 如发现当前任务必须先解决的新前置问题，更新 `TODO.md` 并提交后停止。

## 实施步骤
1. 按任务要求做最小正确实现，避免无关重构。
2. 为变更补充或调整相关测试。
3. 按要求先运行格式化，再运行 lint，再运行相关和完整验证；若仅文档变更且可复用最近绿色结果，则在完成记录中说明跳过原因。
4. 修复验证中发现且未被明确排期的失败；无法本轮修复时，将最小前置任务插入 `TODO.md` 正确位置。
5. 将当前任务标题加上 `[DONE]`，更新完成记录。
6. 检查 `git status`、`git diff` 和最近提交，提交本轮所有相关变更。
7. 停止，不处理下一个任务。

## 当前状态
- 已创建本轮执行计划文件。
- 已读取 `TODO.md`，第一个未完成任务是 `T16 — 通用 typeahead / 命令面板 / 模糊匹配（core）（C.3）`。
- 最近提交为 `[R15] Record completion plan`，仅修改计划记录，未提到与 T16 直接相关的未完成问题。
- 发现工作树已有与本轮无关的未提交变更（若干文档删除/移动、`PLAN.md`、脚本等）；本轮不回退、不修改这些无关变更。
- 已阅读现有 `TextBox`、`TextArea`、`ListBox`、组件 trait、Stack 事件/布局、运行时 builtins、主题与 PTY fixture 模式。
- 实施方案：新增 `src/fuzzy.rs` 提供子序列模糊匹配；新增 `src/widgets/typeahead.rs` 提供可绑定 query/items 的 `TypeAhead` 以及基于它的 `CommandPalette`；导出并注册到 runtime；新增 `snapshot_typeahead_app` 和 PTY 测试覆盖弹层、过滤、选择确认和 Esc 关闭。
- 已完成核心实现、运行时注册、snapshot fixture 与 `tests/pty_typeahead.rs`。
- 验证进度：`cargo fmt` 通过；`cargo clippy --workspace --all-targets -- -D warnings` 通过。
- 验证进度：`cargo test --test pty_typeahead` 通过；完整 `cargo test` 通过。
- 已将 `TODO.md` 中 T16 标记为 `[DONE]` 并补充完成记录。
- 提交前审查发现并修正了 `TypeAhead` 鼠标事件命中范围：现在只有 inner 内容区内的点击会定位输入或确认建议，避免边框点击误触。
- 修正后验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --test pty_typeahead`；`cargo test`。
- 下一步：更新暂存区、检查 staged diff/status/log，提交 T16 相关文件并停止。
