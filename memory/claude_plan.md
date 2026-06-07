# 执行计划

说明：本文件记录可审计的执行计划和进度，不包含隐藏推理内容。

1. 读取 `TODO.md`，按标题是否带 `[DONE]` 识别第一个未完成任务。
2. 检查该任务的要求、依赖、验证方式和完成记录；如最新提交直接提到与该任务相关的未完成事项，也纳入当前任务判断。
3. 只围绕第一个未完成任务收集必要代码上下文，避免开放式历史问题排查。
4. 实现该任务要求；如果遇到阻塞任务的真实前置缺口，按要求更新 `TODO.md` 并停止。
5. 按项目要求运行格式化、lint 和相关测试；若观察到未排期失败，修复或在 `TODO.md` 中排期。
6. 将完成情况写回 `TODO.md`，任务标题加 `[DONE]` 并更新完成记录；仅在阶段计划变化时更新 `PLAN.md`。
7. 检查 git 状态和差异，提交本次任务相关全部变更，然后停止，不处理后续任务。

当前状态：已读取 `TODO.md`，第一个未完成任务为 `T20 — editor 完整化 → 富 ArtifactViewer`。下一步检查最新提交是否有直接相关未完成事项，然后只阅读 T20 相关的 `atto-ui-editor` / `atto-ui-chat` 代码与测试上下文。

T20 执行要点：

1. 在现有 `DiffView` 上实现 hunk 折叠/展开，并确保 diff hunk 数据来自 `editor-core` headless 投影/模型路径。
2. 为 code 视图和 diff 内容接入语法高亮；chat 侧接口保持 T14 的 `ArtifactViewer::open(&mut self, Artifact) -> WindowId` 不变。
3. 在 `atto-ui-editor` 提供富 viewer 实现，`atto-ui-chat` 只替换注入实现或通过 feature/依赖使用该实现，不改变消息列表打开 artifact 的对外语义。
4. 补充 PTY 测试覆盖 code 视图语法高亮、diff hunk 折叠/展开与 chat link 到富 viewer 链路。
5. 按顺序运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、相关测试和完整 workspace 测试；若发现未排期失败，修复或按规则更新 `TODO.md`。

进度更新：最新提交为 `[T20A] Add DiffView control on editor-core 0.4.1 diff crates`，未发现除 T20 本身余项外的直接相关未完成事项。开始阅读 `DiffView`、editor 渲染/语法高亮、chat artifact viewer 与现有 PTY fixture。

进度更新：已完成 T20 相关上下文阅读。实施路径为：把 editor 的语法处理抽到 crate 级共享模块并补一个轻量 `SimpleRust` 高亮；`DiffSession` 继续消费 `editor-core-diff`/`editor-core-diff-view` 投影，同时根据 `editor-core-diff::diff_line_hunks` 建立 hunk 折叠状态，并在投影 cells 上叠加语法样式；在 `atto-ui-editor` 新增富 `ArtifactViewer` 实现，chat 仍只通过既有 `on_open_artifact`/`ArtifactViewer` 接口注入 viewer。

进度更新：已实现共享语法处理、`SimpleRust`、read-only `EditorView` 配置、`DiffView` hunk 折叠/语法叠加、`RichArtifactViewer` 与 `snapshot_rich_artifact_app`。定向 PTY 初跑通过：`cargo test -p atto-ui-editor --test pty_diff -- --nocapture`、`cargo test -p atto-ui-editor --test pty_rich_artifact -- --nocapture`。

进度更新：验证已完成。通过 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。下一步更新 `TODO.md` 中 T20 标题与完成记录，然后检查 git 差异并提交。
