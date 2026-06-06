# 当前执行计划

## 状态
- 本轮目标：完成 `R3 — 审阅 T3`，然后停止。
- 已确认最新提交 `[T3] Add AppHost event injection APIs` 与当前审阅任务直接相关，但提交信息未声明未完成事项。

## 执行步骤
1. 审阅 T3 相关实现文件，重点确认 `AppHost::send_event` 坐标系和目标窗口路由。
2. 审阅窗口管理 API 对现有 `Desktop`/`WindowManager` 不变量的复用情况，重点关注模态焦点陷阱、Z 序、最小化态。
3. 审阅 `set_property` 与 `get_property` 是否通过同一动态 tree-op/属性路径完成往返。
4. 审阅 T3 新增测试是否真实覆盖 R3 要求，必要时补充缺失测试或修复发现的问题。
5. 按顺序运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、相关 PTY/单测；如代码有变更或发现失败，修复后再验证。
6. 将 `R3` 标记为 `[DONE]` 并写入完成记录；只有阶段级计划变化时才更新 `PLAN.md`。
7. 提交本轮相关变更，提交信息使用 `[R3] ...`，然后停止。

## 更新记录
- 初始化执行计划，下一步读取 `TODO.md` 确认当前任务。
- 已确认当前任务为 `R3 — 审阅 T3`，并将计划聚焦到该审阅范围。
- 已完成 R3 代码审阅：`send_event` 坐标转换、目标窗口路由、窗口管理 API、`set_property`/`get_property` 往返路径均与任务要求一致；暂未发现需要代码修复的问题。
- 下一步执行验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui app::desktop`、`cargo test --test pty_apphost_api`。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui app::desktop`、`cargo test --test pty_apphost_api`、`cargo test --workspace --all-targets`。
- 已将 `TODO.md` 中 `R3` 标记为 `[DONE]` 并补充完成记录；下一步提交本轮变更。
