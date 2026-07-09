# 执行计划

本文件记录本轮可共享的执行计划和进度，不包含私有推理细节。

## 初始计划

1. 读取 `TODO.md`，定位第一个标题未带 `[DONE]` 的任务。
2. 只检查最新提交是否提到与当前任务直接相关的未完成事项。
3. 阅读当前任务相关的 app、fixture、依赖和测试结构，避免无关历史问题排查。
4. 完整执行当前任务；若出现具体阻塞，则在 `TODO.md` 中加入最小前置任务并停止。
5. 先运行 `cargo fmt`，再运行 `cargo clippy --workspace --all-targets -- -D warnings`，最后运行所需测试。
6. 更新 `TODO.md` 的完成状态和完成记录；只有阶段级计划变化时才更新 `PLAN.md`。
7. 提交前检查 `git status`、`git diff` 和最近提交，然后用描述性提交信息提交本轮变更并停止。

## 进度

- 已初始化计划，并读取 `TODO.md`。
- 已确认首个未完成任务为 `M1.R Review`。
- 本轮范围限定为复核 M1 app skeleton、mock/network 依赖边界和完整验证，不进入 M2。
- 已检查最新提交 `[M1.6] Add agent PTY snapshot fixture`，未发现直接相关的未完成事项。
- 已复核 M1 app crate：它是 workspace member，依赖为本地 UI crates 加 `anyhow` / `ratatui`，mock fixture 可确定性运行。
- 已确认 `atto-agent-app` 和 `atto-ui-chat` 中未发现直接网络/API 调用，M1 mock 未依赖 DeepSeek/API key/外网。
- 验证已通过：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
- 已更新 `TODO.md`，将 `M1.R Review` 标记为 `[DONE]` 并写入完成记录；`PLAN.md` 无阶段级变更，不需要更新。
- 下一步：复查 git 状态、diff 和最近提交，然后提交本轮 review 完成记录。
