当前执行计划

1. 读取 `TODO.md`，严格按文件顺序识别第一个标题未带 `[DONE]` 的任务；只把该任务作为本次执行范围。
2. 检查近期提交和相关文件，只确认是否存在与当前任务直接相关的未完成事项或阻塞项，不做开放式历史问题扫查。
3. 阅读当前任务涉及的代码、测试和文档，确认任务要求、依赖、验证方式和完成记录格式。
4. 如任务可直接完成，按最小正确改动实现；如发现当前任务无法正确执行且需要新的具体前置任务，则更新 `TODO.md` 并停止在该前置任务安排处。
5. 运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行完整测试套件；若出现未安排的失败，修复或把最小必要任务加入 `TODO.md`。
6. 任务完成后，在 `TODO.md` 中给任务标题加 `[DONE]` 并更新完成记录；仅在阶段计划确实变化时更新 `PLAN.md`。
7. 检查 `git status`、`git diff` 和近期提交，提交本次全部相关改动；提交后停止，不继续下一个任务。

进度记录

- 已创建本执行计划。下一步读取 `TODO.md` 以确定本次唯一任务。
- 已读取 `TODO.md`，本次唯一任务为 `M6.4 Compact`：超预算时生成 `CompactBlock`，后续请求使用摘要替代旧 turn。
- 下一步只检查与 context/compact/transcript 构建直接相关的设计、代码和最近提交，避免无关历史扫查。
- 最近提交为 `[M6.3] Record completion status`，未显式声明与 M6.4 直接相关的未完成事项。
- 初步实现方向：复用 `ContextBuilder` 已有 `CompactBlock` -> system context 映射，在 agent app 的 transcript 进入 DeepSeek request 前加入预算检查与压缩插入逻辑；压缩后 transcript 中保留 compact 摘要和近期消息，后续请求由 `ContextBuilder` 使用该摘要替代旧 turn。
- 已新增 `crates/atto-agent-app/src/compact.rs`，实现默认 64K 上下文窗口 70% 阈值、保留最近 20 条消息、本地确定性摘要兜底、跳过 streaming/pending 交互块，并通过 `ChatMessageStore::replace_all` 让旧 branch token 失效。
- 已在普通提交和 accepted plan 继续执行路径中接入 compact，确保新 assistant turn 启动前 transcript 已压缩；已新增单元测试覆盖算法和提交路径后的 DeepSeek request 映射。
- 验证中 `cargo test -p atto-agent-app compact` 暴露一个测试数据问题：短消息的本地摘要可能比原文更长，导致“before_tokens > after_tokens”断言不稳定。将该测试改为使用长旧消息以验证压缩预算下降。
- 已修正 compact 测试数据；`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-agent-app compact` 均通过。下一步运行完整 workspace 测试。
- 完整验证已通过：`cargo test --workspace --all-targets` 和 `cargo fmt --all -- --check` 成功。
- 已更新 `TODO.md`，将 `M6.4 Compact` 标记为 `[DONE]` 并写入完成记录；`PLAN.md` 阶段级计划未变化，不更新。
- 下一步检查 git status/diff/log，确认只提交本任务相关文件。
