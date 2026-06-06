# 当前执行计划

## 约束与决策依据
- `TODO.md` 是任务顺序、验收要求和完成记录的权威来源。
- 本次只处理第一个标题未带 `[DONE]` 的任务，完成后停止。
- 若遇到阻塞当前任务的缺陷、失败测试或缺失能力，优先修复；若无法在当前任务内正确修复，则在 `TODO.md` 中插入最小必要前置任务并提交后停止。
- 不用 workaround、缩小范围或改变预期表示来绕过规格问题。
- 完成任务后需要更新 `TODO.md` 的任务标题和完成记录，必要时才更新 `PLAN.md`。
- 提交前按要求执行格式化、lint 和相关测试；若只改文档且已有可复用绿色结果，则在完成记录中说明跳过原因。

## 步骤计划
1. 读取 `TODO.md`，按标题是否带 `[DONE]` 找出第一个未完成任务。
2. 查看最近提交信息，判断是否存在与当前任务直接相关的未完成事项；只处理会阻塞当前任务的内容。
3. 阅读当前任务涉及的代码、测试和文档，明确实现边界与验收标准。
4. 如任务可直接执行，进行最小且完整的实现；如存在具体前置阻塞，更新 `TODO.md` 记录前置任务并停止。
5. 运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行相关测试和必要的完整测试套件。
6. 若测试或 fixture 出现未排期失败，修复或在 `TODO.md` 中安排最小必要任务，不能把当前任务标记为完成。
7. 验证通过后，在 `TODO.md` 中把当前任务标题加 `[DONE]` 并更新完成记录。
8. 检查 `git status`、`git diff` 和最近提交，确认只提交本次应包含的文件。
9. 提交本次更改并停止，不继续处理下一个任务。

## 进度记录
- 已初始化计划文件；下一步读取 `TODO.md` 识别第一个未完成任务。
- 已读取 `TODO.md`，第一个未完成任务为 `R14 — 审阅 T14`。
- 本次执行范围：审阅 T14 的 artifact link、`on_open_artifact`、`ArtifactViewer` / `TextArtifactViewer`、窗口打开/关闭链路和相关 PTY 覆盖；完成后只标记 R14，不进入 T15。
- 已检查最近提交：最新提交为 `[T14] Add chat artifact viewer`，未发现提交信息中显式标注的未完成事项。
- 已观察到工作区存在与当前 R14 审阅无关的未提交改动；后续不会回退这些改动，提交时只纳入本次 R14 相关文件。
- 已审阅 artifact 相关模型、list 渲染、dynamic schema、viewer trait 和 snapshot fixture；确认 chat crate 未依赖 editor，消息列表通过 `on_open_artifact(ArtifactId)` 与 viewer 解耦。
- 审阅发现 R14 的关闭链路要求缺少直接 PTY 断言；计划在现有 artifact PTY 中补一个关闭 viewer 窗口的断言，而不是新增新的独立任务。
- 已补充 `chat_artifact_code_link_opens_text_viewer_window`：打开 code viewer 后定位标题栏关闭按钮，点击关闭并断言 viewer 内容消失。
- 首次 `cargo clippy --workspace --all-targets -- -D warnings` 发现新增 helper 使用 `.last()` 触发 `double_ended_iterator_last`；已改为 `.next_back()`，准备重新格式化和验证。
- 第二次 clippy 要求进一步使用 `rfind(...)` 替代 `filter(...).next_back()`；已按建议修正。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-chat --test pty_chat chat_artifact -- --nocapture`；`cargo test -p atto-ui-chat`；`cargo test --workspace --all-targets`。
- 已更新 `TODO.md`：`R14` 标记为 `[DONE]`，并写入审阅结论、关闭链路测试补充和验证记录。
