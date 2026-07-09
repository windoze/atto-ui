本文件记录本次调用的执行计划与进度。为避免泄露内部推理，仅记录可审计的执行计划、决策依据与状态更新。

## 执行计划

1. 读取 `TODO.md`，按文档顺序定位第一个标题未带 `[DONE]` 的任务，并记录任务 ID、要求、依赖与验证标准。
2. 检查最近提交信息是否明确提到与该任务直接相关的未完成事项；仅在其阻塞当前任务时纳入当前任务或作为前置任务写入 `TODO.md`。
3. 读取与当前任务相关的计划、源码、测试和文档，避免做开放式历史缺陷清扫。
4. 以最小正确改动实现当前任务；如遇到阻塞性规格不匹配或缺失能力，优先修复阻塞问题，或在 `TODO.md` 中插入最小前置任务后停止。
5. 按要求更新测试，先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，通过后运行完整测试套件；如无编译输出相关改动，则复用最近一次绿色结果并在完成记录中说明。
6. 更新 `TODO.md`：将当前任务标题加上 `[DONE]`，填写完成记录、验证结果与关键变更。仅当阶段级计划实际变化时更新 `PLAN.md`。
7. 检查 `git status`、`git diff` 和最近提交，提交本次任务相关全部改动，提交信息包含任务 ID 与简要说明。
8. 完成一个任务后停止，不继续处理后续任务。

## 当前状态

- 状态：已读取 `TODO.md`，确认首个未完成任务为 `P3.R Review：P3 阶段复核`。
- 当前任务范围：逐条复核 P3.1–P3.4 的 store 截断/fork、user 消息编辑重发、assistant retry/regenerate、快照与 PTY 覆盖；重点确认版本/block 清理、滚动/自动跟随、流式竞态、回调契约和全套 CI。
- 最近提交检查：最新提交为 `[P3.4] Add chat session fork PTY snapshots`，未在提交标题中明确标出未完成事项。
- 复核发现候选问题：需要确认 pending edit 目标被删除后的提交语义、旧流式生产者在截断后继续追加消息的防护、非 tail retry/fork 后自动跟随，以及 retry/regenerate 回调契约是否足够清晰。
- 已确认并修复：
  - `ChatMessageStore` 新增 `ChatBranchToken`、`branch_token`、`is_branch_current` 与 `push_if_branch_current`，`replace_all`、`truncate_from`、`fork_at` 会在实际分支变化时让旧 token 失效；条件 push 在消息更新临界区内检查 token，并在并发截断已移除该消息时清理注册版本。
  - pending user edit 的目标若已被外部截断，提交会被编辑拦截器消费并清除编辑态，不再退化成普通提交。
  - `ChatMessageList` 现在用消息 ID 序列识别截断/fork/尾部替换，`auto_scroll` 开启时会恢复尾部跟随；普通追加和历史前置不会误触发。
  - `on_message_action` / Node 文档补充说明 Retry/Regenerate 回调触发前已截断目标 assistant 回合及后续旧分支。
- 已补测试：store 分支 token、缺失 truncate no-op、流式截断后旧 delta/status/push no-op，list pending edit 竞态和分支重写自动跟随。
- 验证结果：`cargo fmt --all`、`cargo test -p atto-ui-chat --lib`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 文档状态：`TODO.md` 已将 `P3.R Review：P3 阶段复核` 标记为 `[DONE]`，并记录修复、覆盖和验证结果；`PLAN.md` 未更新，因为阶段级计划没有变化。
- 下一步：检查 `git diff --check`、`git status`、`git diff` 和最近提交，确认改动范围后提交。
