# 当前执行计划

说明：我会记录可公开的执行计划与进度摘要；不会记录不可公开的内部推理细节。

1. 读取 `TODO.md`，按标题是否带 `[DONE]` 确认第一个未完成任务。
2. 检查最近提交是否明确提到与该任务直接相关的未完成问题；只处理会阻塞当前任务的内容。
3. 阅读当前任务涉及的代码、测试与文档，确定最小正确实现范围。
4. 实现第一个未完成任务；如发现必须先修复的具体前置问题，则更新 `TODO.md` 记录该前置任务并停止。
5. 运行规定验证流程：先 `cargo fmt`，再 `cargo clippy --all-targets -- -D warnings`，最后运行相关或完整测试。
6. 更新 `TODO.md`，将完成任务标题加 `[DONE]` 并补充完成记录；仅在阶段计划实际变化时更新 `PLAN.md`。
7. 检查 git 状态与差异，提交本次任务相关全部变更，然后停止，不继续下一个任务。

## 进度

- 已创建本执行计划文件，下一步读取 `TODO.md` 选择第一个未完成任务。
- 已确认第一个未完成任务为 `T7 — 新建 atto-ui-async crate（tokio，feature-gated）`。
- 最近提交 `[R6] Review task cancellation registry` 与 T6 取消注册表相关；T7 将复用该注册表，不进行无关历史问题扫描。
- 下一步读取 workspace 配置、`AppHost`/运行入口、`TaskRegistry` 和组件 crate feature 配置，确定最小实现与测试位置。
- 已确定实现策略：新增 `crates/atto-ui-async`，默认 feature 为空；`tokio-runtime` 只提供 runtime builder 与 `spawn_async`/`spawn_blocking`；`event-stream` 再启用 `crossterm/EventStream`、ratatui 终端 session、std action channel 到 tokio select 的桥接和 async 运行入口。
- 测试策略：新增 `snapshot_tokio_app` feature-gated PTY fixture 与 `tests/pty_tokio_runtime.rs`，验证 async task 回灌 UI 与 Esc 取消；同时运行 cargo tree 检查默认/feature-off 路径不引入 tokio。
- 已完成初版代码变更：workspace 加入 `atto-ui-async`，`atto-ui-components` 增加默认关闭的 `async` feature；新 crate 提供 runtime/spawn helper、EventStream 运行入口、PTY fixture 与集成测试。
- 下一步执行 `cargo fmt`，随后运行 clippy 与 feature/default 测试。
- 默认 workspace clippy 已通过。
- feature clippy 发现 `tokio::select!` 缺少 tokio `macros` feature；已将 `macros` 加入 `tokio-runtime` 配置，准备复跑 feature clippy。
- feature clippy 已通过；`cargo test -p atto-ui-async --features event-stream` 通过，包含 2 个 runtime 单测和 2 个 PTY 集成测试。
- `cargo clippy -p atto-ui-components --no-default-features --features async --all-targets -- -D warnings` 已通过，确认 components 的 `async` feature 透传可编译。
- `cargo tree -p atto-ui` 与 `cargo tree -p atto-ui-async --no-default-features` 输出中未出现 tokio，确认默认/core 路径不引入 tokio。
- `cargo test --workspace --all-targets` 已通过。
- 已将 `TODO.md` 中 T7 标记为 `[DONE]` 并写入完成记录；`PLAN.md` 未更新，因为阶段级计划未变化。
- 已检查 git 状态、目标文件 diff、暂存 diff 与空白问题；工作区存在无关未提交变更，将只提交已暂存的 T7 相关文件。
