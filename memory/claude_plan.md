# 执行计划

## 当前状态

- 已读取 `TODO.md`，第一个未完成任务为 `P6.R Review：P6 阶段复核`。
- 最新提交为 `5625ac8 [P6.5] Add approval compact PTY coverage`，与当前 P6 review 直接相关，需要纳入复核范围。
- 本文件用于记录可审计的计划、关键步骤完成情况、验证结果和阻塞事项。

## 步骤计划

1. 读取 `TODO.md`，按文件顺序识别第一个标题未以 `[DONE]` 标记的任务。
2. 检查最新提交信息，若其明确提到与该任务直接相关的未完成事项，则纳入当前任务范围或作为前置任务记录到 `TODO.md`。
3. 读取当前任务相关的代码、测试和文档，确认实现范围与验证要求。
4. 实现当前任务；若发现必须先修复的具体阻塞问题，则最小化更新 `TODO.md` 记录前置任务并停止。
5. 运行格式化、lint 和相关测试；若有未排期失败测试，修复或按要求加入 `TODO.md`。
6. 更新 `TODO.md`：在完成任务标题前加 `[DONE]`，并填写完成记录。
7. 必要时更新本文件记录关键进度；只有阶段计划变化时才更新 `PLAN.md`。
8. 检查 git 状态、差异和最近提交，提交本次任务的全部相关变更。
9. 停止，不继续处理下一个任务。

## 进度记录

- 已创建初始执行计划。
- 已确定本轮任务：只完成 `P6.R Review`，不继续处理后续任务。
- P6 复核中发现并修复 React hook legacy approval deny 推断与 Rust 不一致的问题：`label: "No thanks"` 现在推断为 `deny/once`，并新增渲染测试覆盖。
- 已更新 `AGENT_GAP.md`，确认除 B2 图片/多模态内联渲染外，A1–A4、B1/B3、C1–C3、D1/D2 均已落地。
- 已完成验证：React targeted build/render test、Rust fmt/clippy/fmt-check/build/full test、native Node build、core/react typecheck 与测试、Bun/Deno runtime、React smoke、npm artifacts/pack dry-run、`git diff --check` 均通过。
- 已将 `TODO.md` 中 `P6.R Review` 标记为 `[DONE]` 并写入完成记录；后续只需检查差异并提交本轮变更。
