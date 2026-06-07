# Claude Plan

## 当前约束

- 以 `TODO.md` 为唯一任务顺序与完成状态来源。
- 本次只处理第一个标题未带 `[DONE]` 的任务，完成后提交并停止。
- 先执行格式化，再执行 `cargo clippy --all-targets -- -D warnings`，最后执行完整测试；若只改文档且已有可复用的绿色结果，则记录跳过原因。
- 不使用规避实现；遇到阻塞当前任务的缺口时，在 `TODO.md` 中加入最小前置任务并提交后停止。
- `PLAN.md` 只在阶段级计划或依赖结构变化时更新。

## 执行计划

1. 读取 `TODO.md`，找出第一个标题未以 `[DONE]` 标记的任务。
2. 检查最近提交是否明确提到与该任务直接相关的未完成事项；如有，将其纳入当前任务或作为前置任务记录到 `TODO.md`。
3. 阅读当前任务涉及的代码、测试与文档，确认验收要求和边界。
4. 以最小正确改动实现任务；编辑前后持续更新本文件记录关键进展。
5. 增加或调整必要测试，避免 fixture-only hack 或缩小规格。
6. 按顺序运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、完整测试套件；发现未排期失败则修复或加入前置/后续任务。
7. 在 `TODO.md` 中给完成任务标题加 `[DONE]`，并更新完成记录；仅在阶段计划变化时更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、最近提交，确认只提交本次任务相关改动。
9. 提交所有本次任务相关变更并停止，不继续下一个任务。

## 进展记录

- 已创建初始执行计划，下一步读取 `TODO.md` 确定当前任务。
- 已读取 `TODO.md`；第一个未完成任务为 `NT4`：`#[napi] AppHost` 全方法暴露（B.1）。最近提交为 `NR3` 审阅提交，未发现直接指向 `NT4` 的未完成事项。下一步读取 `TODO-1.md` 中的任务详情。
- 已读取 `TODO-1.md` 的 `NT4` 详情；任务要求在 `crates/atto-ui-node/src/lib.rs` 暴露完整 `AppHost` 方法、注册所有组件、支持 headless 选项、提供 `alloc_callback()`，并用 JS headless 冒烟验证窗口/spec/op/callback 闭环。
- 已对照 `PLAN-1` B.1、现有 `convert.rs`/`ids.rs`/`error.rs` 与 Python `PyAppHost`。实施将复用 NT2/NT3 的 JSON 转换和 string handle；Node 方法返回 JSON `Value`，避免 JS 侧接触 raw `u64`/BigInt；同时补齐事件解析、窗口/快照 JSON 编码和 JS headless 冒烟。
- 已编辑 `crates/atto-ui-node`：新增 `AppHost` napi 类、`AppHostConfig`/`Rect`、事件解析模块、窗口/快照 JSON 编码、组件错误映射、crossterm/ratatui 依赖与 `__test__/app_host.cjs`。下一步运行 `cargo fmt`，再修正编译/clippy 问题。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-node`、`cargo test --all --all-targets`、`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`、`npm test`。下一步更新 `TODO.md`/`TODO-1.md` 的 NT4 完成状态与完成记录。
