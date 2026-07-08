# 执行计划

我不能记录私有推理链，但此文件记录本次调用的可执行计划、关键决策和进度更新。

## 计划

1. 先读取 `TODO.md`，识别第一个标题未以 `[DONE]` 开头的任务。
2. 只检查最新提交中是否有与所选任务直接相关的未完成工作。
3. 只查看该任务所需的代码、测试和文档。
4. 完整实现所选任务，避免 workaround 或缩小范围。
5. 按要求顺序验证：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`，代码变更后再运行完整测试套件。
6. 如发现阻塞性前置任务或未排期失败测试，向 `TODO.md` 添加最小必要任务条目，提交该记录后停止。
7. 任务完成后，在 `TODO.md` 标题添加 `[DONE]`，更新完成记录，提交相关变更，并停止，不开始下一项任务。

## 进度

- 已在读取项目任务文件或运行命令前写入初始执行计划。
- 已读取 `TODO.md`；选定第一个未完成任务：`P1.1 代码块语法高亮`。
- 当前范围：基于 P1.0 选型记录，在 `crates/atto-ui-markdown` 实现 fenced code block 语法高亮；缺失或未知语言提示时回退纯文本。
- 已检查最新提交：`[P1.0] Select syntax highlighting approach`；未发现除 P1.1 外的额外相关未完成事项。
- 已确认当前代码块渲染只在 `CodeBlockState` 保存规范化纯文本行，并在宽度切片后统一应用 `code_block` 样式；本次实现新增可选逐行语法 spans，同时保留原有纯文本宽度与嵌入式滚动行为。
- 已新增公共 `atto_ui_markdown::syntax` 模块，提供中立高亮输出类型和 syntect 驱动的 fenced-code 高亮。
- 已扩展 markdown code block 状态与渲染：已知语言绘制 syntax spans，未知或缺失语言继续走纯文本路径，并保留既有宽度/滚动行为。
- 已新增单元测试覆盖 hint 提取、fallback 行为和 markdown code block state 高亮。
- 已运行 `cargo fmt --all`；首次 clippy 发现两个默认值风格警告，已用 `unwrap_or_default()` 修复并重新格式化。
- `cargo clippy --workspace --all-targets -- -D warnings` 已通过。
- 首次完整测试发现 Rust `storage.*` scope 应映射为 `SyntaxClass::Keyword`；已调整分类规则，并确认 `cargo test -p atto-ui-markdown --lib` 通过。
- 分类修复后已重新验证：`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 已在 `TODO.md` 将 `P1.1 代码块语法高亮` 标记为 `[DONE]`，并写入完成记录与验证命令。
