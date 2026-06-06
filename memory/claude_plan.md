# 执行计划

## 约束
- 以 `TODO.md` 为唯一任务顺序与完成状态来源。
- 本轮只处理第一个标题未带 `[DONE]` 的任务，完成后停止。
- 不做开放式历史问题扫描；只处理当前任务相关或验证暴露且未排期的问题。
- 如果发现阻塞当前任务的未排期问题，先在 `TODO.md` 中加入最小必要前置任务并提交，然后停止。

## 步骤
1. 读取 `TODO.md`，识别第一个未完成任务及其验证要求。
2. 检查最近提交信息，仅判断是否存在与该任务直接相关的未完成事项。
3. 阅读当前任务涉及的源码、测试和文档，确定最小实现范围。
4. 实现任务，保持改动聚焦，避免无关重构。
5. 按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，通过后运行相关或完整测试。
6. 若验证失败且未在后续任务明确排期，则修复或在 `TODO.md` 中加入必要前置任务。
7. 将任务标题标记为 `[DONE]`，更新完成记录；仅在阶段计划实际变化时更新 `PLAN.md`。
8. 检查 `git status`、`git diff` 和最近提交，提交本轮相关更改。
9. 停止，不处理下一个任务。

## 当前状态
- 已识别本轮任务：`R11 — 审阅 T11`。
- 最近提交为 `[T11] Add core disclosure component`，未发现提交信息中声明的相关未完成事项。
- 已审阅 Disclosure 组件实现、主题接入、runtime schema、fixture 与 PTY 测试，未发现阻塞当前任务的实现问题。
- 已完成验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --test pty_disclosure`、`cargo test --workspace --all-targets`。
- 已将 `R11` 在 `TODO.md` 中标记为 `[DONE]` 并写入完成记录。
- 已检查本轮目标差异；只提交 `TODO.md` 与 `memory/claude_plan.md`，保留工作树中其他既有改动不动。
- 下一步提交本轮更改后停止。
