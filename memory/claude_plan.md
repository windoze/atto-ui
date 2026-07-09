# 执行计划

## 当前约束
- 以 `TODO.md` 为唯一任务顺序与完成状态来源。
- 本轮只处理第一个标题未带 `[DONE]` 的任务，完成后停止。
- 任务完成必须实现、验证、更新 `TODO.md` 完成记录并提交 Git。
- 如发现阻塞当前任务的具体前置问题，先在 `TODO.md` 中插入最小必要前置任务并提交后停止。
- 不进行开放式历史问题扫查；只处理与当前任务直接相关的问题或测试失败策略要求处理的问题。

## 步骤计划
1. 读取 `TODO.md`，定位第一个未完成任务，确认任务正文、验证要求、依赖和完成记录格式。
2. 查看最近提交信息，仅判断是否明确提到与该任务直接相关的未完成事项。
3. 按当前任务范围检查相关代码和测试，避免无关重构。
4. 实现任务所需的最小正确改动；如遇必须先修的具体阻塞，更新 `TODO.md` 并停止。
5. 按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，通过后运行完整测试套件。
6. 根据验证结果修复问题，直到相关验证通过或按测试失败策略补充前置任务。
7. 更新 `TODO.md`：在任务标题前加 `[DONE]`，补全完成记录和验证记录；只有阶段计划真实变化时才更新 `PLAN.md`。
8. 检查 Git 状态、差异和最近提交，提交本轮所有应提交改动。
9. 停止，不处理下一个任务。

## 进度记录
- 已创建本执行计划文件，下一步读取 `TODO.md`。
- 已读取 `TODO.md`，本轮首个未完成任务为 `P2.3 @ 文件提及`。下一步检查最近提交是否有直接相关未完成事项。
- 最近提交未显示与 P2.3 直接相关的未完成阻塞。
- 已检查 `input.rs` 与 `completion.rs`。实现方向：复用现有 `CompletionPopup`，新增 mention 候选 API、触发范围识别、provider 回调刷新、确认时只替换当前 `@query` 片段，并补单测覆盖光标/多次提及相关行为。
- 已完成主要代码改动：`TextArea` 增加公开光标/区间替换 API；`ChatInputHandle`/`ChatInputPanel` 增加 mention 候选、provider、popup 同步、Esc dismiss 与确认替换；已补输入层和 TextArea 单测。下一步执行格式化、lint 与测试。
- 验证已通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat input`、`cargo test -p atto-ui --lib textarea`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 已更新 `TODO.md`，将 `P2.3 @ 文件提及` 标记为 `[DONE]` 并补充完成记录。下一步检查 Git 差异并提交。
