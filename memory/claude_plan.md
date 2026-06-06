# 当前执行计划

## 约束

- 以 `TODO.md` 为任务排序和完成状态的唯一依据。
- 本轮只完成第一个未标记 `[DONE]` 的任务，然后停止。
- 若发现阻塞当前任务的未排期前置问题，先更新 `TODO.md` 并提交后停止。
- 修改代码后按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后运行完整测试。
- 不回退或覆盖用户已有改动。

## 步骤

1. 读取 `TODO.md`，定位第一个标题未带 `[DONE]` 的任务。
2. 读取相关计划、代码和测试，确认当前任务的实现范围、依赖和验证要求。
3. 实现当前任务；若遇到必须先解决的前置问题，则更新 `TODO.md` 并停止。
4. 运行格式化、lint 和相关/完整测试，修复发现的问题。
5. 在 `TODO.md` 中给当前任务标题加 `[DONE]`，并填写完成记录。
6. 检查工作区差异，提交本轮全部相关改动。
7. 停止，不处理下一个任务。

## 进度

- 已读取 `TODO.md`，首个未完成任务为 `T15 — id 索引替代 O(n) 查找（M9，低优先）`。
- 最新提交为 `[R14] Review giant file splits`，未发现与 T15 直接相关的未完成事项。
- 已阅读 `src/wm/manager/events.rs|focus.rs|z_order.rs`、`src/wm/manager/core.rs|types.rs`、`src/runtime/spec.rs` 与 `src/runtime/tree.rs`。
- 发现窗口管理器当前通过 `Vec<Window>` 的 `iter().find/position/any` 做 id 查找；runtime spec tree-op 与 live view tree 增量操作通过递归 DFS 查找 id。
- 实施计划更新为：
  1. 在 `WindowManager` 中增加 `HashMap<WindowId, usize>`，新增索引重建/读取 helper，并在 add/close/bring_to_front 后同步索引。
  2. 将 `window`/`window_mut`/`window_kind`/`dispatch_to_window_view`/focus 判断/bring_to_front 改为通过索引定位。
  3. 在 runtime spec tree-op 中增加 `ComponentSpec` 的 `id -> path` 索引，tree-op 通过 path 定位，结构变更后重建索引。
  4. 在 live view tree 增量路径中增加 `tag -> path` 索引，插入/删除/替换/移动与属性/事件局部更新通过 path 定位，结构变更后重建索引。
  5. 补充索引同步回归测试后运行验证。
- 已完成主要实现：窗口管理器新增 `window_index`，runtime spec/live view tree-op 新增 id path 索引；结构变更后重建索引。
- 已补充索引同步测试：window manager 重排/聚焦/关闭，spec tree-op 批内路径偏移，ComponentTree 增量批内路径偏移。
- 已运行 `cargo fmt`。
- 第一次 `cargo clippy --workspace --all-targets -- -D warnings` 发现两个仅测试使用的 runtime helper 在普通 lib 目标中为 dead code；已用 `#[cfg(test)]` 限定 wrapper 并重新格式化。
- 第二次 `cargo clippy --workspace --all-targets -- -D warnings` 已通过；下一步运行 `cargo test --all --all-targets`。
- `cargo test --all --all-targets` 已通过；下一步更新 `TODO.md` 将 T15 标记为 `[DONE]` 并填写完成记录，然后检查 diff 并提交。
- `TODO.md` 已将 T15 标记为 `[DONE]` 并填写完成记录；下一步检查工作区 diff/status，确认仅包含本轮改动，然后提交。
