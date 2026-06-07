# 当前执行计划

## 目标

- 以 `TODO.md` 为唯一任务来源，完成其中第一个标题未带 `[DONE]` 的任务。
- 完成且验证后，更新 `TODO.md` 的完成状态与记录，提交一次 Git commit，然后停止。

## 执行步骤

1. 读取 `TODO.md`，按现有顺序定位第一个未完成任务，不做开放式历史问题排查。
2. 检查最近提交与该任务是否直接相关；仅在其明确影响当前任务时纳入处理或作为前置任务记录。
3. 阅读当前任务涉及的计划、约束、源码与测试，确认实施范围。
4. 如任务存在阻塞性前置缺口，更新 `TODO.md` 记录最小必要前置任务并停止；否则直接实现当前任务。
5. 按要求运行格式化、lint、相关测试及必要的完整测试；发现未安排的失败则修复或在 `TODO.md` 中排入前置任务。
6. 更新 `TODO.md`：给完成任务标题加 `[DONE]`，补充完成记录与验证结果；仅在阶段计划真实变化时更新 `PLAN.md`。
7. 检查 Git 状态与差异，提交本次任务相关全部改动。
8. 停止，不继续处理后续任务。

## 进度记录

- 已创建本计划文件，下一步读取 `TODO.md` 定位首个未完成任务。
- 已确认第一个未完成任务为 `TODO-2.md` 的 `T6 — 阶段三首批编辑动作接线`。
- 最近提交 `c803ec6 [R5] Review Explorer docking migration` 未明确提到与 T6 直接相关的未完成事项。
- 当前工作树除本计划文件外已有未跟踪 `notification.sh`、`run_agent.sh`，本任务不修改这些文件。
- 下一步读取 T6 涉及的 editor/app 源码与 `editor-core` API，确认动作、快捷键、注释配置和同步路径后实施。
- 已读取 T6 涉及文件和 `editor-core` API；确认 `CommentConfig` 可通过 `editor_core` re-export 使用，行操作与多光标命令已存在于 `editor-core`。
- 实施方案：扩展 `EditorAction`/默认 keymap；在 `EditorConfig` 增加 `comment` binding；新增统一的 full-document LSP/text/syntax 同步 helper；将新 action 映射到 `EditCommand`/`CursorCommand`；在 app 语言层提供 comment config 并注入 `DocumentTabView`；补 view 单测和 Ctrl+/ PTY 覆盖。
- 已完成初版实现并通过 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`。
- 首次完整 `cargo test --workspace --all-targets` 仅新增 Ctrl+/ PTY 失败；原因是 Crossterm legacy C0 parser 将 raw `0x1f` 报告为 `Ctrl+7`。已补默认 keymap 归一绑定 `Ctrl+7 -> ToggleComment`，下一步重新格式化、lint 并跑测试。
- 修正后重新验证通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
- 已将 `TODO.md` 索引和 `TODO-2.md` 中 T6 标题标记为 `[DONE]`，并补充完成记录；`PLAN.md` 未变化。
- 下一步检查 Git diff/status，确认只提交本任务相关改动，然后创建提交并停止。
