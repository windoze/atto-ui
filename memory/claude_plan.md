本文件记录本轮执行的可审计计划与进度摘要。为避免泄露不可见的内部推理，这里只记录可执行步骤、依据和状态。

## 当前目标

- 严格以 `TODO.md` 为任务来源，识别并完成第一个标题未带 `[DONE]` 的任务。
- 完成后更新 `TODO.md` 的任务标题与完成记录，按要求验证，并提交一个清晰的 Git commit。
- 本轮只完成一个任务，完成后停止。

## 初始执行计划

1. 读取 `TODO.md`，找出第一个未完成任务；必要时查看最新提交信息，确认是否存在与该任务直接相关的未完成事项。
2. 阅读该任务涉及的说明、依赖、验收条件，以及必要的 `PLAN.md` 上下文；不进行无关的历史问题扫查。
3. 检查当前工作区状态，识别已有未提交变更，避免覆盖或回退非本轮修改。
4. 根据任务要求定位相关源码和测试，做最小但完整的实现修改；若发现阻塞当前任务的规格不匹配或必需前置项，则更新 `TODO.md` 并提交后停止。
5. 运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后运行完整测试套件；若存在未调度的失败测试，修复或在 `TODO.md` 中加入正确顺序的前置任务。
6. 更新 `TODO.md`：在已完成任务标题前加 `[DONE]`，填写完成记录；仅在阶段级计划发生变化时更新 `PLAN.md`。
7. 提交本轮所有相关变更，提交信息包含任务编号和简短说明。

## 进度

- 2026-07-09：已写入初始计划，下一步读取 `TODO.md` 识别第一个未完成任务。
- 2026-07-09：已确认第一个未完成任务为 `P5.R Review：P5 阶段复核`。本轮将只复核 P5.1-P5.4，不推进 P6。
- 2026-07-09：复核 P5 实现时确认搜索、turn 折叠、引用回复主路径与 PTY fixture 已存在；为覆盖 P5.R 明确要求的宽字符搜索高亮，已新增 `chat_search_highlights_wide_character_match_cells` 单测。
- 2026-07-09：已运行 `cargo fmt --all`；新增宽字符搜索定向测试 `cargo test -p atto-ui-chat chat_search_highlights_wide_character_match_cells --lib` 已通过。
- 2026-07-09：`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均已通过。下一步更新 `TODO.md` 并提交。
- 2026-07-09：已将 `TODO.md` 中 P5.R 标记为 `[DONE]` 并补充完成记录；准备检查 diff 并提交。

## P5.R 具体执行步骤

1. 查看当前工作区状态和 P5 相关文档上下文，避免覆盖既有未提交变更。
2. 复核 `crates/atto-ui-chat/src/list.rs` 与 `input.rs` 中 P5 搜索、turn 折叠、引用回复的实现边界。
3. 复核 P5 单元测试、snapshot fixture 与 PTY 测试，确认覆盖 `TODO.md` 要求的屏外搜索、折叠/展开、引用附加/移除、宽字符高亮等关键路径。
4. 如发现阻塞 P5.R 的缺陷，直接修复并补测试；如出现必须前置的规格缺口，则按要求更新 `TODO.md` 并停止。
5. 验证顺序：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
6. 验证通过后，将 `TODO.md` 中 P5.R 标记 `[DONE]` 并填写完成记录，提交本轮相关变更。
