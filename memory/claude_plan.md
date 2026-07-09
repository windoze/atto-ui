# 执行计划

## 约束
- `TODO.md` 是任务排序、验收和完成记录的唯一权威来源。
- 本次只完成第一个标题未带 `[DONE]` 的任务，然后停止。
- 若遇到阻塞当前任务的缺陷或未排期失败测试，先修复；若无法直接修复，则在 `TODO.md` 中加入最小必要前置任务并提交后停止。
- 先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后在需要时运行完整测试。
- 完成后更新 `TODO.md` 的任务标题和完成记录，必要时更新 `PLAN.md`，并提交所有相关变更。

## 初始步骤
1. 读取 `TODO.md`，确定第一个未完成任务及其验收要求。
2. 查看最近提交，判断是否存在与该任务直接相关的未完成事项。
3. 读取任务涉及的代码、测试和文档，确认最小正确实现范围。
4. 实现当前任务，不进行无关历史问题扫查。
5. 补充或调整必要测试，避免通过缩窄范围或特例绕过规格。
6. 运行格式化、lint 和相关测试；若代码有实质变更，再运行完整测试套件。
7. 更新 `TODO.md` 完成状态和完成记录，按需更新本计划文件。
8. 检查 git diff/status，提交本次任务的全部相关改动。

## 当前任务：P3.4 快照与测试
- 任务要求：`snapshot_chat_app` 增加编辑/重发场景；PTY 覆盖编辑 user 后截断、retry 后回合截断、fork 后旧消息不再显示。
- 执行步骤：
  1. 检查最近提交，确认是否有直接关联 P3.4 的未完成事项需要纳入。
  2. 定位 `snapshot_chat_app`、现有 chat PTY 测试和 P3.2/P3.3 的 API 测试，复用既有行为而不改动非必要业务逻辑。
  3. 增加确定性 snapshot fixture，使测试能触发 user 编辑提交、assistant retry/regenerate 截断，并在屏幕输出中清晰暴露旧消息是否仍存在。
  4. 增加 PTY 测试覆盖三类验收：编辑 user 后截断旧分支、retry 后截断 assistant 回合、fork 后旧消息不再显示。
  5. 运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
  6. 将 `TODO.md` 中 P3.4 标记为 `[DONE]` 并填写完成记录。
  7. 检查状态与 diff，提交本次任务所有相关文件。

## 进度记录
- 已确认当前任务为 P3.4；最近提交 P3.3 未包含额外未完成事项。
- 已新增 P3.4 专用 snapshot fixture 参数：`--edit-resubmit`、`--retry-resubmit`、`--fork-at`。
- 已新增 PTY 测试覆盖编辑重发截断、assistant retry 截断、`fork_at` 后旧分支隐藏。
- 首次运行新增 PTY 过滤测试时，retry 用例等待了已滚出屏幕的 user prompt；已改为等待当前验收需要的可见旧 assistant/旧 tail 与 Retry 操作。
- 已通过验证：`cargo fmt --all`、新增 `chat_p3` PTY 过滤测试、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 已将 `TODO.md` 中 P3.4 标记为 `[DONE]` 并填写完成记录。
