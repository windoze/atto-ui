# 执行计划

说明：本文件记录可审查的执行计划与进度，不包含隐藏推理过程。

1. 读取 `TODO.md`，按标题是否带 `[DONE]` 判断第一个未完成任务。
2. 检查该任务的要求、依赖、验证方式和完成记录格式。
3. 如当前任务被具体前置问题阻塞，按要求更新 `TODO.md` 并提交后停止。
4. 否则实现该任务的最小正确改动。
5. 按项目要求运行格式化、lint 和相关测试；若发现未排期的失败，修复或把最小前置任务写入 `TODO.md`。
6. 更新 `TODO.md`，在完成任务标题前加 `[DONE]` 并填写完成记录。
7. 更新本文件记录关键进展。
8. 检查 git 状态和 diff，提交本次任务涉及的全部变更。
9. 完成一个任务后停止，不继续下一个任务。

## 当前任务

- 首个未完成任务：`T11 — C4 分段式 StatusBar 与 editor diagnostics 接入`。
- 直接相关最近提交：`[R10] Review menu mnemonics and accelerators`，未发现与 T11 直接相关的未完成问题。

## T11 具体执行步骤

1. 阅读 `src/app/status.rs`、`src/app/desktop.rs`、`src/theme/mod.rs` 以及 `atto-editor-app` 的状态更新路径。
2. 为 `StatusBar` 增加 segment 数据结构、兼容旧 `left/right` API 的绘制分支、分段绘制、优先级隐藏、grapheme 截断和 click hit-test。
3. 在 `Desktop` 鼠标分发中把 status bar 区域 click 路由给 `StatusBar::handle_mouse`。
4. 注册状态栏相关 named styles 和 separator glyph fallback。
5. 在 `atto-editor-app` tick/update 路径中根据当前/最近 editor 状态更新分段状态栏，至少显示 diagnostics summary、language/path 等可用信息。
6. 添加/更新单元测试和 PTY 测试覆盖指定验收点。
7. 依次运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`。
8. 将 `TODO-2.md` 和索引 `TODO.md` 标记为 `[DONE]` 并填写完成记录。
9. 检查 diff 后以 `[T11] ...` 提交并停止。

## 进展记录

- 已实现 `StatusSegmentAlign` / `StatusSegment`、分段绘制、优先级隐藏、grapheme 截断、click hit-test 与 Desktop status bar click 路由。
- 已注册 `status-segment`、`status-segment-warning`、`status-segment-error` named styles 和 `status-separator` glyph。
- 已为 `atto-editor-app` 增加活动 editor status binding，并在 status bar 分段显示 app/path/dirty、diagnostics 和 language。
- 已添加 StatusBar 单元测试、Desktop click 路由测试和 editor app PTY smoke 测试。
- 首次完整测试发现新增 PTY 用例中 `language` 段会被超长完整路径按优先级挤掉；已将状态栏路径显示压缩为文件名，保留 diagnostics/language 可见性。
- 第二次完整测试发现宽字符片段写入后 trailing cell 样式会被重置；已改为手写 grapheme/宽字符单元格渲染，确保背景样式铺满。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`。
- 已在 `TODO-2.md` 将 T11 标记为 `[DONE]` 并填写完成记录；已同步 `TODO.md` 索引状态。
