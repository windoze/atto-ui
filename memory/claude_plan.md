# 当前执行计划

## 约束

- 以 `TODO.md` 为唯一任务顺序与完成状态来源。
- 只处理第一个标题未带 `[DONE]` 的任务，完成后提交并停止。
- 不做开放式历史问题清扫；只处理当前任务的直接依赖、阻塞项或测试失败政策要求的问题。
- 若遇到无法按规格完成的阻塞项，只添加最小必要前置任务并提交，不继续绕过。
- 计划文件只记录可审查的执行步骤和进度，不记录私有推理细节。

## 当前任务

- 首个未完成任务：收尾阶段 `CI 检查`。
- 任务要求：确认默认 CI 不依赖 `DEEPSEEK_API_KEY` 或外部网络。
- 最近提交：`02b975a [Docs] Update agent app documentation`，未显式声明与本任务直接相关的未完成问题。

## 执行步骤

1. 检查仓库 CI 配置文件，确认默认 workflow 的命令、环境变量和测试选择。
2. 检查 DeepSeek 真实 smoke 测试和网络相关测试，确认默认 `cargo test` 不会访问外网，真实测试为 ignored 或需要显式 opt-in。
3. 如 CI 配置缺失或默认命令可能触发网络/API key 依赖，做最小配置或文档/测试调整以满足任务要求。
4. 先运行 `cargo fmt --all`，再运行 `cargo clippy --workspace --all-targets -- -D warnings`。
5. clippy 通过后运行完整测试 `cargo test --workspace --all-targets`，超时时间不少于 30 分钟。
6. 复查 `git diff`，在 `TODO.md` 中给 `CI 检查` 标题加 `[DONE]` 并补充完成记录与验证命令。
7. 更新本计划文件进度，检查 `git status`、`git diff`、`git log --oneline -10`，提交本任务相关变更后停止。

## 进度

- 已读取 `TODO.md` 并确认当前任务为 `CI 检查`。
- 已检查最近提交，未发现直接相关的未完成事项。
- 已建立本次任务执行计划。
- 已检查 `.github/workflows/ci.yml` 和 `.github/workflows/release.yml`：Rust 默认测试命令未使用 `--ignored`，workflow 未设置或要求 `DEEPSEEK_API_KEY`。
- 已检查 `crates/atto-agent-app/tests/deepseek_real_smoke.rs`：真实 DeepSeek smoke 测试标记为 `#[ignore]`，需手动设置 `DEEPSEEK_API_KEY` 并显式运行 ignored 测试；默认测试仅编译不执行外网请求。
- 已检查 DeepSeek client 单测：默认 HTTP streaming client 覆盖使用 `127.0.0.1` 本地 mock SSE server。
- 已运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`，均通过。
- 已运行 `cargo test -p atto-agent-app --test deepseek_real_smoke`，结果为 0 passed / 1 ignored，确认默认不执行真实 DeepSeek 网络请求。
