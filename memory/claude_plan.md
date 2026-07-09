# 当前执行计划

## 约束

- 以 `TODO.md` 为任务顺序和完成状态的唯一依据。
- 只完成第一个标题未带 `[DONE]` 的任务，完成后停止。
- 如果遇到阻塞当前任务的规格缺口、失败测试或必要前置项，先在 `TODO.md` 中记录最小必要前置任务并停止。
- 不把内部推理链写入文件；本文件记录可审阅的计划、进度和关键决策。

## 步骤

1. 读取 `TODO.md`，识别首个未完成任务及其验证要求。
2. 检查与该任务直接相关的上下文文件；如最新提交明确指出相关未完成问题，也纳入当前任务或前置项。
3. 实现该任务所需的最小正确变更。
4. 先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后运行相关测试或完整测试套件。
5. 如验证发现未安排的失败测试，修复它或在 `TODO.md` 中加入正确排序的前置任务。
6. 完成后将任务标题加上 `[DONE]`，更新完成记录。
7. 检查 `git status`、`git diff` 和最近提交，提交本次任务涉及的全部变更。
8. 停止，不继续下一个任务。

## 当前状态

- 已读取 `TODO.md`，首个未完成任务为 `M1.4 Slash 命令`。
- 已检查最新提交：`add26bd [M1.3] Implement input submit loop`，未发现与 M1.4 直接相关的未完成说明。
- 已实现 app 层 slash 命令注册和直接输入分派：`/help`、`/clear`、`/plan`、`/skills`、`/tools`、`/abort`。
- `/abort` 通过替换 transcript 推进 branch token，避免旧 mock turn 的迟到 token 覆盖已取消回合；真正停止后台 worker 仍留给 `M1.5 取消语义`。
- 已补充单元测试覆盖命令注入、帮助输出、清空、plan mode 切换、skill/tool 空列表和 abort 分支隔离。
- 已通过验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo test --workspace --all-targets`。
- 已在 `TODO.md` 将 `M1.4 Slash 命令` 标记为 `[DONE]` 并补充完成记录。
- 下一步：检查 git diff/status，并提交本次任务变更后停止。
