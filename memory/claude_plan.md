# 执行计划

说明：这里记录可审阅的执行计划、关键决策和进度更新；不记录隐藏推理细节。

## 当前目标

1. 读取 `TODO.md`，严格按顺序识别第一个标题未以 `[DONE]` 开头的任务。
2. 读取该任务相关上下文，必要时查看 `PLAN.md`、最新提交和相关源码，但不做无关历史问题扫查。
3. 完整实现当前任务；如果发现阻塞当前任务的真实前置问题，则按要求更新 `TODO.md` 并停止。
4. 先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，通过后运行完整测试套件；若仅文档变更则按规则复用最近一次绿色结果并说明。
5. 更新 `TODO.md`：给完成任务标题加 `[DONE]`，补充完成记录；仅在阶段计划变化时更新 `PLAN.md`。
6. 检查 `git status`、`git diff`、`git log --oneline -10`，提交本次任务相关全部变更。
7. 停止，不继续下一个任务。

## 进度

- 已创建初始执行计划。
- 已识别首个未完成任务：`P3.3 retry / regenerate`。下一步只核对与该任务直接相关的最新提交、源码和测试，不做无关历史扫查。
- 已实现 Retry/Regenerate 按钮在触发 `on_message_action` 前截断目标 assistant 回合，并补充 list 单测；已调整 message action PTY fixture 以适配截断后按钮消失的行为。
- 验证已通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
