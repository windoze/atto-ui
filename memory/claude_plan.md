# 执行计划

## 当前状态

- 状态：已确认当前任务。
- 当前任务：`P4.4 快照与测试`。
- 目标：补齐 PTY 覆盖流式中排队新消息、Esc 中断置 `Canceled`、多行粘贴规整，然后提交并停止。
- 约束：`TODO.md` 是任务顺序与完成状态的唯一权威来源；`PLAN.md` 只在阶段级计划变化时更新。

## 步骤

1. 读取 `TODO.md`，确认第一个标题未带 `[DONE]` 的任务及其验证要求。
2. 查看最近提交信息，判断是否存在与当前任务直接相关的未完成事项。
3. 读取当前任务涉及的源文件、测试与文档，确认实现边界。
4. 如任务可直接完成，实施最小且完整的代码或文档变更；如发现必须先解决的具体阻塞项，将其作为最小 prerequisite 写入 `TODO.md` 后停止。
5. 按要求运行验证：先 `cargo fmt`，再 `cargo clippy --all-targets -- -D warnings`，最后在需要时运行完整测试套件。
6. 若验证失败，修复失败；若失败是未计划且无法在当前任务内修复的阻塞项，将其按依赖顺序写入 `TODO.md`。
7. 完成后在 `TODO.md` 中给任务标题加 `[DONE]` 并更新 completion record。
8. 检查 `git status`、`git diff`、最近提交，确认仅提交本次任务相关改动。
9. 用清晰提交信息提交改动。
10. 停止，不进入下一个任务。

## 进度日志

- 已创建本计划文件，下一步读取 `TODO.md` 选择第一个未完成任务。
- 已读取 `TODO.md`，首个未完成任务为 `P4.4 快照与测试`；下一步检查最近提交与现有测试/fixture。
- 已确认最近提交为 P4.3，未标明与 P4.4 直接相关的未完成事项。
- 已定位现有 `--input-queue` 与 `--cancel-action` PTY 覆盖；本次将补齐多行粘贴规整的 snapshot fixture 与 PTY，并保留已有排队/Esc 测试。
- 已修改 `snapshot_chat_app`：新增 `--multiline-paste` fixture、启用 bracketed paste、专用 submit 回显；已新增 PTY 用例 `chat_multiline_paste_normalizes_and_submits`。
- 已完成验证：`cargo fmt --all`、新增 PTY 定向测试、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 已将 `TODO.md` 中 `P4.4 快照与测试` 标记为 `[DONE]` 并写入完成记录。
- 下一步检查 git diff/status/log 并提交本任务改动。
