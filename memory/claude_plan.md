## 执行计划

说明：此文件记录可审阅的执行计划与进度，不包含隐藏推理链路。

1. 已读取 `TODO.md`，按文件顺序识别第一个标题未带 `[DONE]` 的任务。
2. 当前任务为 `T10 — 控件层共享抽象（M10）`；本次只完成 T10，不进入 `R10` 或后续任务。
3. 检查最新提交信息，若显式提到与 T10 直接相关的未完成问题，将其纳入 T10 或作为前置任务记录到 `TODO.md`。
4. 定位 `src/widgets/textbox.rs`、`src/widgets/table.rs`、`src/widgets/list.rs`、`src/widgets/button.rs` 中重复的三态样式、鼠标坐标/contains、ListBox/TableView selection/scroll 逻辑。
5. 采用最小正确抽象实现 T10：新增 widgets 内部公共 util，替换重复样式与命中逻辑；若 selection/scroll 无法一次性抽出且不破坏行为，则先识别具体阻塞并按规则更新 `TODO.md` 后停止。
6. 运行 `cargo fmt`，再运行 `cargo clippy --workspace --all-targets -- -D warnings`，通过后运行相关 widget/PTY 测试和全量 `cargo test --all --all-targets`。
7. 若发现未排期测试失败，立即修复或在 `TODO.md` 中加入最小必要前置任务；不能绕过失败。
8. 完成后更新 `TODO.md`：将 T10 标题改为 `[DONE] T10 ...` 并填写完成记录；仅当阶段级计划变化时更新 `PLAN.md`。
9. 检查 `git status`、`git diff`、最近提交，提交本次 T10 涉及的全部变更，然后停止。

## 当前进度

- 已读取 `TODO.md`，确认第一个未完成任务为 `T10 — 控件层共享抽象（M10）`。
- 已检查最近提交：最新提交为 `[R9] Review shared scroll input handling`，未显式记录与 T10 直接相关的未完成事项。
- 已检查工作区状态：`memory/claude_plan.md` 为本次计划更新；`notification.sh`、`run_agent.sh` 为既有未跟踪文件，本次不触碰。
- 已新增 `src/widgets/util.rs`，集中放置 widgets 内部共享的三态样式、命中/鼠标坐标转换、row selection + scroll 状态。
- 已将 `Button`、`Checkbox`、`TextBox`、`ListBox`、`TableView` 的标准三态样式切换改为 `widget_style`。
- 已将 `TextBox`、`ListBox`、`TableView` 的重复 `mouse_coords_local_to_area` / `contains` 逻辑收敛到 widgets util。
- 已将 `ListBoxContent` 与 `TableBodyContent` 重复的 selection 归一化、滚动可见性和 Up/Down/鼠标选择逻辑收敛到 `SelectionScroll`。
- 已验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --test pty_mouse_support && cargo test --test pty_textbox_selection && cargo test --test pty_virtual_scrolling && cargo test --test pty_desktop`、`cargo test --all --all-targets` 全部通过。
- 已更新 `TODO.md`：T10 标题已标记 `[DONE]`，完成记录包含抽象内容与验证命令。
- 下一步检查 Git 状态和差异，确认只提交 T10 相关文件与本计划记录。
