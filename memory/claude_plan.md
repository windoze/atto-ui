# 当前执行计划

## 目标

按照 `TODO.md` 的顺序完成第一个未标记 `[DONE]` 的任务，并在完成后停止，不继续处理后续任务。

## 执行步骤

1. 读取 `TODO.md`，确认第一个未完成任务的编号、标题、依赖、验证要求和完成记录要求。
2. 检查最近提交和当前工作区状态，只识别与当前任务直接相关或会阻塞当前任务的问题。
3. 阅读当前任务涉及的代码、测试和文档，确定最小正确实现范围。
4. 实现当前任务；如果发现必须先修复的具体前置问题，则按要求更新 `TODO.md` 并停止。
5. 运行格式化、lint 和相关测试；若代码发生变更，按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后运行完整测试。
6. 将当前任务标题在 `TODO.md` 中标记为 `[DONE]`，并更新完成记录，包含实现摘要和验证结果。
7. 根据阶段计划是否发生变化判断是否需要更新 `PLAN.md`；常规任务日志不写入 `PLAN.md`。
8. 检查 `git status`、`git diff` 和最近提交，确认只提交本次任务相关变更。
9. 使用清晰的任务编号提交信息提交所有本次任务变更，然后停止。

## 进度记录

- 已创建本计划文件。下一步读取 `TODO.md` 以确定当前任务。
- 已读取 `TODO.md`，第一个未完成任务为 `T14 — 消息内 Artifact link + 最简文本 viewer（核心方案）`。
- T14 范围：扩展 chat 消息模型为 artifact link；暴露 artifact open 回调；定义 `ArtifactViewer` 接口；实现最简 `TextArtifactViewer`，点击 link 后在独立普通窗口显示 code/diff/file 文本；补 PTY 覆盖 code 与 diff link。
- 下一步检查最近提交与工作区状态，随后阅读 chat message/list/store/dynamic、window/app host、可点击组件和现有 PTY fixture 的实现。
- 已检查最近提交与工作区状态：最新提交为 R13，未发现直接阻塞 T14 的未完成项；工作区已有无关归档/脚本/PLAN 变更，本次不回退、不纳入无关修改。
- 已阅读 chat message/list/store/dynamic、snapshot fixture、PTY 测试、Button/Text 事件处理和 Desktop/Window API。
- 实现方案：在 `message.rs` 中加入 `ArtifactId`、`ArtifactKind`、`Artifact` 与 `ChatMessageContent::Artifact`；在 `ChatMessageList` 中增加 `on_open_artifact(ArtifactId)` 回调并渲染可点击 artifact link；新增 `viewer.rs` 定义 `ArtifactViewer` trait 和 `TextArtifactViewer`，通过 `Desktop::add_window(WindowKind::Normal, ...)` 打开只读文本窗口，diff 行按前缀简单着色；更新 dynamic 序列化和 schema；在 `snapshot_chat_app --artifact-link` 中用队列连接 link 点击和 viewer.open；补 PTY 与单测。
- 已完成 T14 主要代码草案：消息模型、列表回调/link、dynamic roundtrip、viewer trait/文本窗口、snapshot artifact fixture 和 code/diff PTY 用例均已落地。下一步运行格式化和验证，根据编译/测试结果修正。
- 第一次 `cargo clippy --workspace --all-targets -- -D warnings` 发现 `snapshot_chat_app` 中 artifact/tool 分支存在相同 `None` 返回；已合并条件，准备重新运行格式化和验证。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat`、`cargo test --workspace --all-targets`。
- 已将 `TODO.md` 中 T14 标题标记为 `[DONE]`，并补充完成记录。下一步检查 diff/status，仅提交 T14 相关文件和本计划记录，不纳入既有无关工作区变更。
