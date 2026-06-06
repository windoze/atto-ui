# Claude Execution Plan

说明：本文件记录可审计的执行计划、关键决策和进度更新；不记录私有推理链。

## 当前目标

- 按 `TODO.md` 的顺序识别第一个标题未带 `[DONE]` 的任务。
- 完整处理该任务，或在遇到真实阻塞时按要求把最小必要前置任务写入 `TODO.md` 后停止。
- 完成后更新 `TODO.md` 的完成记录并提交 Git；本轮只处理一个任务。

## 初始执行步骤

1. 读取 `TODO.md`，定位第一个未完成任务及其验证要求。
2. 查看最近提交，判断是否有与该任务直接相关的未完成事项。
3. 只围绕当前任务收集必要上下文，避免开放式历史问题扫描。
4. 实施当前任务要求的最小正确变更。
5. 按要求运行格式化、lint 和相关测试；如果执行完整测试套件，设置足够长的超时。
6. 若发现未被安排的测试或夹具失败，修复或在 `TODO.md` 中排入必要任务，且不把当前任务标为完成。
7. 任务完成后，将任务标题加上 `[DONE]`，更新完成记录。
8. 检查 `git status`、`git diff`、最近提交记录，确认只提交本轮应包含的变更。
9. 使用清晰的任务相关提交信息提交，然后停止。

## 进度记录

- 已创建初始执行计划，下一步读取 `TODO.md` 确认当前任务。
- 已读取 `TODO.md`，当前第一项未完成任务是 `R16 — 审阅 T16`。
- 最新提交为 `13efd67 Update plan`，未明确提到与 R16 直接相关的未完成事项。
- 下一步审阅 T16 的 `src/fuzzy.rs`、`src/widgets/typeahead.rs`、命令面板/runtime/schema 导出、`snapshot_typeahead_app` 与 PTY 测试，重点确认复用性、焦点/命中行为和验证结果。
- 已审阅主要实现与测试，发现 T16 完成记录提到鼠标点击确认，但现有 PTY 只覆盖键盘选择和 Esc 关闭；已补充 `pty_typeahead_mouse_click_accepts_visible_suggestion`，通过真实屏幕坐标点击建议项验证命中路径。
- 验证进度：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --test pty_typeahead` 均通过。
- 完整 `cargo test --workspace --all-targets` 失败于 `atto-editor-app --test explorer_enter_open_smoke::enter_in_explorer_opens_file`，现需按测试失败策略定位并修复，或在 `TODO.md` 中安排最小必要任务后停止。
- 已复跑 `explorer_enter_open_smoke`：初次仍复现，随后在环境稳定后通过，判断为 PTY 首屏/内容等待预算在完整 suite 负载下过短。已将 `explorer_enter_open_smoke` 与同类 `explorer_open_smoke` 的文本等待预算从 3 秒提高到 5 秒，保持单用例远低于 1 分钟。
- 复验结果：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --test pty_typeahead`、`cargo test -p atto-editor-app --test explorer_enter_open_smoke`、`cargo test -p atto-editor-app --test explorer_open_smoke`、`cargo test --workspace --all-targets` 均通过。
- 已将 `TODO.md` 中 `R16` 标记为 `[DONE]`，并写入审阅完成记录。下一步检查 diff/status 后提交本轮变更。
