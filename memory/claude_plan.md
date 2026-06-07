# Claude Execution Plan

## Current Invocation

公开计划说明：本文件记录本次执行的可审计计划、关键决策和进度更新。不会包含私有推理链；会包含足够的步骤、依据和验证记录，便于检查执行过程。

## Step-by-Step Plan

1. 读取 `TODO.md`，按文档顺序找到第一个标题未以 `[DONE]` 开头的任务。
2. 检查该任务的要求、依赖、完成记录和验证要求；仅在与当前任务直接相关时查看最近提交信息。
3. 如果当前任务被未记录的具体前置问题阻塞，则将最小必要前置任务插入 `TODO.md`，提交后停止。
4. 如果任务可直接执行，则检查相关代码和测试，做最小正确实现。
5. 运行格式化、lint 和任务要求的测试；如发现未调度的失败测试/fixture，修复或在 `TODO.md` 中排入正确位置。
6. 完成后在 `TODO.md` 中把任务标题加上 `[DONE]`，更新完成记录；仅当阶段级计划变化时更新 `PLAN.md`。
7. 检查 `git status`、`git diff`、最近提交，提交本次任务相关变更，然后停止，不进入下一任务。

## Progress Log

- 已写入初始计划；下一步读取 `TODO.md` 识别首个未完成任务。
- 已读取 `TODO.md`，首个未完成任务为 `R20 — 审阅 T20`。
- 本次执行范围限定为审阅 T20：确认 chat 接口未改动且富 viewer 仍实现 T14 的 `ArtifactViewer` 接口，确认 diff hunk 模型来自 editor-core headless 层，运行相关 PTY/单测和 workspace 验证；完成后只标记 R20 为 `[DONE]` 并提交。
- 2026-06-07 16:16 +08:00：开始新的调用。先重新读取 `TODO.md`，按当前文档状态选择首个未以 `[DONE]` 开头的任务；随后只执行该任务，完成或记录阻塞后提交并停止。
- 已确认首个未完成任务仍为 `R20 — 审阅 T20`。本次执行计划：审阅 `RichArtifactViewer` 与 T14 `ArtifactViewer` 接口一致性、确认 chat 仍通过既有 `on_open_artifact` 注入 viewer、确认 diff hunk 折叠模型来自 `editor-core` headless diff 层、运行 R20 要求的 PTY/单测和 workspace 验证；若通过，则只更新 R20 完成记录并提交。
- 审阅发现：`RichArtifactViewer` 在 `atto-ui-editor` 中直接 `impl atto_ui_chat::ArtifactViewer`，方法签名保持 `open(&mut self, Artifact) -> WindowId`；chat crate 未依赖 editor crate，富 viewer 通过现有 `ChatMessageList::on_open_artifact` 回调注入；`DiffSession` 使用 `editor_core_diff_view::{DiffModel, DiffProjection}` 和 `editor_core_diff::diff_line_hunks` 生成 headless diff/hunk 数据，UI 只维护 collapsed row projection。
- 2026-06-07 16:20 +08:00：开始新的调用。计划先重新读取 `TODO.md`，选择当前第一个未以 `[DONE]` 开头的任务；随后检查直接相关代码/测试，按任务要求实现或审阅，运行所需验证，更新 `TODO.md` 完成记录，提交本次任务相关变更后停止。
- 已确认当前首个未完成任务为 `R20 — 审阅 T20`。执行范围：审阅富 `ArtifactViewer` 接口兼容性、确认 chat 注入链路无需改动、确认 diff hunk 模型来自 editor-core headless 层、运行 R20 所需 PTY/相关验证；通过后只标记 `R20` 完成并提交本任务相关文件。
- 审阅发现：`atto-ui-chat::ArtifactViewer` 仍为 `open(&mut self, Artifact) -> WindowId`；`atto-ui-editor::RichArtifactViewer` 直接实现同一 trait，chat crate 未依赖 editor crate；`snapshot_rich_artifact_app` 通过现有 `ChatMessageList::on_open_artifact` 回调注入富 viewer。`DiffSession` 使用 `DiffModel`/`DiffProjection` 和 `diff_line_hunks`，UI 层仅维护折叠状态与可见投影。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-editor --test pty_diff -- --nocapture`；`cargo test -p atto-ui-editor --test pty_rich_artifact -- --nocapture`；`cargo test --workspace --all-targets`。下一步更新 `TODO.md` 的 R20 标题和完成记录。
- 已更新 `TODO.md`：`R20 — 审阅 T20` 标题加 `[DONE]`，并记录审阅结论与验证命令。最终步骤为检查 diff/status 并提交本任务相关文件。
