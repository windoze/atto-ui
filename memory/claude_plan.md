# 当前执行计划

## 可审计思路摘要

- 先以 `TODO.md` 为唯一任务排序来源，找出第一个标题未带 `[DONE]` 的任务。
- 不做开放式历史问题扫查，只处理当前任务直接需要的上下文。
- 若发现阻塞当前任务的真实缺陷、缺失能力或未安排的测试/夹具失败，将先修复；若无法在本次完成，则在 `TODO.md` 中插入最小必要前置任务并停止。
- 完成任务后按要求更新 `TODO.md` 的 `[DONE]` 标记和完成记录；仅当阶段级计划变化时更新 `PLAN.md`。
- 先运行格式化，再运行严格 lint，再运行相关或完整测试；若仅文档变更且已有可复用绿色结果，则在完成记录中说明跳过原因。
- 最后检查 Git 状态与差异，提交本次任务涉及的所有未提交文件，然后停止，不继续下一个任务。

## 步骤计划

1. 已读取 `TODO.md`，第一个未完成任务为 `T9 — 抽取共享滚动逻辑（M3）`。
2. 检查 Git 状态和最近提交，仅判断是否存在与 T9 直接相关的未完成事项。
3. 阅读 `src/composable/stack/events.rs`、`src/composable/grid/events.rs`、`src/composable/scroll_container/events.rs`、`src/composable/scroll.rs`，确认现有按键/滚轮滚动行为。
4. 在共享滚动模块中提供统一方法，将方向键、PageUp/PageDown、Home/End、鼠标滚轮转换为 delta 并调用 `scroll_by_delta`。
5. 将 stack/grid/scroll_container 三处重复实现替换为共享方法，保持既有行为不变。
6. 运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --test pty_scrolling`、`cargo test --test pty_horizontal_scrolling`，再运行 `cargo test --all --all-targets`。
7. 更新 `TODO.md`：给 T9 标记 `[DONE]` 并填写完成记录；只有阶段级计划变化时才更新 `PLAN.md`。
8. 检查工作区、差异和最近提交，提交本次 T9 变更后停止。

## 进度记录

- 已确认 `T9` 是当前第一个未完成任务。
- 已检查最近 10 个提交，最新提交为 R8/T8，未发现明确指向 T9 的未完成事项。
- 当前工作区存在未跟踪 `notification.sh`、`run_agent.sh`，与 T9 无关，将保持不动且不纳入提交。
- 已阅读三处重复滚动事件逻辑与 `scroll.rs::scroll_by_delta`，下一步在 `scroll.rs` 添加共享输入事件到滚动偏移的转换函数，并替换三处调用。
- 已完成共享函数初版：`scroll_offset_for_input_event` 统一处理方向键、PageUp/PageDown、Home/End 与鼠标滚轮，并复用 `scroll_by_delta`。
- 已替换 `stack/events.rs`、`grid/events.rs`、`scroll_container/events.rs` 三处重复逻辑；同时发现 `src/wm/min_size_view.rs` 存在同类滚动映射重复，已一并收敛到共享函数。
- 已运行 `cargo fmt`，格式化通过。
- 已运行 `cargo clippy --workspace --all-targets -- -D warnings`，严格 lint 通过。
- 已运行滚动专项验证：`cargo test scroll_input --lib`、`cargo test --test pty_scrolling`、`cargo test --test pty_horizontal_scrolling` 均通过。
- 已运行 `cargo test --all --all-targets`，全量测试通过。
- 下一步更新 `TODO.md`：将 T9 标记为 `[DONE]` 并记录实现与验证结果。
- 已更新 `TODO.md`：T9 已标记 `[DONE]`，完成记录包含实现范围和验证命令。
- 已检查工作区差异；待提交文件为 T9 相关 Rust 源码、`TODO.md` 与本计划文件。未跟踪 `notification.sh`、`run_agent.sh` 将保持不提交。
