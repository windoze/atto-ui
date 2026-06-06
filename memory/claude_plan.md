# 执行计划

说明：此文件记录可审阅的执行计划与进度，不包含隐藏推理链路。

1. 读取 `TODO.md`，按文件顺序识别第一个标题未带 `[DONE]` 的任务。
2. 当前任务为 `R10 — 审阅 T10`；本次只完成 R10，不进入 T11 或后续任务。
3. 检查最新提交信息，仅处理与 R10/T10 直接相关的未完成事项。
4. 审阅 T10 涉及的共享 widgets 抽象：`widget_style`、`contains`、`mouse_coords_local_to_area`、`SelectionScroll`。
5. 对照 `Button`、`Checkbox`、`TextBox`、`ListBox`、`TableView` 原有行为，确认共享函数未改变样式选择、鼠标坐标转换、选择/滚动和回调语义。
6. 如发现缺陷，直接修复并补充验证；如发现阻塞项，按规则写入 `TODO.md` 后停止。
7. 按顺序运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`。
8. 验证通过后，将 `TODO.md` 中 R10 标题标记为 `[DONE]` 并填写完成记录；仅阶段级计划变化时更新 `PLAN.md`。
9. 检查 `git status`、`git diff`、最近提交，提交 R10 的任务记录与本计划记录，然后停止。

## 当前进度

- 已读取 `TODO.md`，确认第一个未完成任务为 `R10 — 审阅 T10`。
- 已检查最近提交：最新提交为 `[T10] Extract shared widget abstractions`，与 R10 直接相关，未记录额外未完成事项。
- 已检查工作区状态：`notification.sh`、`run_agent.sh` 为既有未跟踪文件，本次不触碰。
- 已审阅 `src/widgets/util.rs`：`widget_style` 保持 disabled/focused/normal 三态优先级；`contains` 与 `mouse_coords_local_to_area` 保留非空矩形、绝对坐标和嵌套本地坐标兼容语义。
- 已审阅 `src/widgets/button.rs`、`checkbox.rs`、`textbox.rs`、`list.rs`、`table.rs`：共享 helper 替换未改变原有样式、命中转换、边框滚动条或 TextBox 单行点击限制。
- 已审阅 `SelectionScroll` 在 `ListBoxContent` 与 `TableBodyContent` 的使用：selection clamp、滚动到可见区域、鼠标点击选择、Up/Down wrap 选择和 `on_change` 触发与原实现一致。
- 已搜索共享抽象调用点，确认 T10 未误改 slider/tab/styled_label 等未纳入本任务的独立实现。
- 已验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets` 全部通过。
- 已更新 `TODO.md`：R10 标题已标记 `[DONE]`，完成记录包含审阅结论与验证命令；`PLAN.md` 无阶段级变化，未更新。
