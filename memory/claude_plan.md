# 执行计划

本文件记录本次调用的可公开执行计划与进度。不会记录隐藏推理链，只记录可审计的计划、决策和结果。

## 初始计划

1. 读取 `TODO.md`，按标题是否带有 `[DONE]` 识别第一个未完成任务。
2. 检查最近提交与该任务是否直接相关；只处理会阻塞当前任务或直接属于当前任务的问题。
3. 阅读当前任务涉及的代码、测试和文档，确认约束、验收条件与依赖。
4. 如任务可直接完成，做最小正确实现，并补充或调整相关测试。
5. 先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，通过后运行完整测试套件。
6. 若出现未安排的失败测试，优先修复；如果确属当前任务的必要前置阻塞，则在 `TODO.md` 插入最小前置任务并停止。
7. 完成后在 `TODO.md` 中给任务标题加 `[DONE]`，更新完成记录；仅当阶段计划变化时才更新 `PLAN.md`。
8. 提交本次任务相关全部变更，然后停止，不继续下一项任务。

## 进度

- 已创建初始计划，下一步读取 `TODO.md` 选择第一个未完成任务。
- 已读取 `TODO.md`，第一个未完成任务是 `M2.R Review`。
- 最近提交为 `[M2.5] Map DeepSeek stream errors`，未显式声明与 `M2.R Review` 直接相关的未完成事项；下一步检查工作区状态和 M2.6 是否已有未提交变更。
- 已检查工作区状态，存在未提交的 M2.6 相关变更：`Cargo.lock`、`crates/atto-agent-app/Cargo.toml`、`src/lib.rs`、新增 `src/deepseek_client.rs`、新增 `tests/deepseek_real_smoke.rs`、`TODO.md`。这些变更与当前 M2 复核直接相关，将作为复核输入并在本次提交中一并处理。

## 当前任务计划：M2.R Review

1. 检查 git 状态，确认是否存在前次调用遗留的未提交文件。
2. 复核 workspace 与 crate 依赖，确认网络依赖只在 `crates/atto-agent-app`，`atto-ui` 和 `atto-ui-chat` 不新增网络依赖。
3. 复核 DeepSeek mock/real smoke 测试，确认默认测试不依赖外网，真实 DeepSeek 测试被 `#[ignore]` 或等效机制排除。
4. 复核取消路径和错误路径，确认 branch token、turn 状态、UI error detail 与测试覆盖稳定。
5. 按要求运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 和格式检查。
6. 若验证通过，在 `TODO.md` 标记 `M2.R Review` 为 `[DONE]` 并补充完成记录。
7. 提交本任务相关变更，提交后停止。

## 复核结论

- 依赖边界：`reqwest`、`futures-util` 和 app 测试所需 `tokio` 仅新增在 `crates/atto-agent-app/Cargo.toml`；`atto-ui` 和 `atto-ui-chat` 未新增网络依赖。
- 默认无外网：`DeepSeekClient` 单测使用 `127.0.0.1` 本地 mock SSE server；真实 DeepSeek smoke 测试位于 `crates/atto-agent-app/tests/deepseek_real_smoke.rs`，已标记 `#[ignore = "requires DEEPSEEK_API_KEY and external DeepSeek network access"]`。
- 取消和错误路径：app action 先检查 `ChatMessageStore` branch token；`cancel_streaming_turn` 和 `fail_streaming_turn` 都会推进 branch token；单测覆盖 `/abort`、Esc、SSE error 失败和迟到 token 拒绝，PTY 覆盖 Esc 取消后不出现迟到 `Done.`。
- 对照 M2 计划：当前交互式 app 保持 mock provider，真实 DeepSeek 通过 ignored smoke 手动验证；这符合 `M2.6` 的“PTY 走 mock client；真实 DeepSeek smoke 标记 ignored”任务边界。

## 验证计划

1. 运行 `cargo fmt --all`。
2. 运行 `cargo clippy --workspace --all-targets -- -D warnings`。
3. 运行 `cargo test --workspace --all-targets`，超时设置不少于 30 分钟。
4. 运行 `cargo fmt --all -- --check`。

## 验证结果

- `cargo fmt --all`：通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo test --workspace --all-targets`：通过。
- `cargo fmt --all -- --check`：通过。

## 完成状态

- 已在 `TODO.md` 将 `M2.R Review` 标记为 `[DONE]`，并补充完成记录。
- 下一步提交本次 M2.6 遗留变更、M2.R Review 记录和计划文件，然后停止。
