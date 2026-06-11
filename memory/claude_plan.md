# 执行计划

> 说明：这里记录可审计的执行计划、决策依据和进度，不记录私有推理链。

## 通用流程

1. 读取 `TODO.md`，只定位第一个标题未带 `[DONE]` 的任务，不做开放式历史问题扫描。
2. 查看最近提交信息，只有在其明确提到与当前任务直接相关的未完成问题时才纳入当前任务或作为前置任务写入 `TODO.md`。
3. 阅读当前任务涉及的源码、测试和文档，确认任务要求、依赖和验证方式。
4. 若发现当前任务被缺失功能、规格不匹配或未安排的失败测试阻塞，按要求在 `TODO.md` 中新增最小前置任务并提交后停止。
5. 以最小正确改动实现当前任务，不绕开规格要求。
6. 为新增或变更行为补充必要测试。
7. 分阶段运行格式化、lint 和测试：先 `cargo fmt`，再 `cargo clippy --workspace --all-targets -- -D warnings`，最后运行完整测试套件。
8. 将当前任务标题前缀更新为 `[DONE]`，并填写完成记录和验证结果。
9. 仅在阶段计划本身变化时更新 `PLAN.md`。
10. 检查 `git status`、`git diff`、最近提交记录，确认只提交本次相关变更。
11. 提交 Git commit，提交信息包含任务编号和简要说明。
12. 停止，不继续下一个任务。

## 历史记录：P1.1

- 已创建本计划文件，下一步读取 `TODO.md` 定位首个未完成任务。
- 已读取 `TODO.md`：首个未完成任务为 `P1.1 新消息模型`。
- 已查看最近提交：`3623f2b Add agent chat UI design doc and plan; archive UI-gaps docs`，未声明与 `P1.1` 直接相关的未完成事项。
- 下一步读取 `CHAT_UI.md` §3 以及 `src/message.rs`/相关引用，实施新消息模型。
- 已确认影响范围：`message.rs`、`lib.rs`、`list.rs`、`store.rs`、`dynamic.rs`、chat 示例/快照，以及 editor 快照中对旧 `ChatSender` 等类型的引用。
- 实施策略：完成 `P1.1` 的新消息 envelope、`ChatRole`、`ChatBlockId`、`ChatTurnStatus`、错误/meta/usage/stop reason；为现有单内容渲染和旧 store 方法添加过渡适配，完整内容块类型矩阵留给后续 `P1.2`。
- 已完成 Rust 侧首轮改造并通过 `cargo check -p atto-ui-chat --all-targets`。下一步运行格式化、lint 和测试。
- 已通过验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 下一步更新 `TODO.md` 的 `P1.1` 标题和完成记录，然后检查 diff 并提交。
- 已更新 `TODO.md`：`P1.1` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 下一步检查工作区状态/diff/最近提交，确认提交范围后创建任务提交。
- 已检查提交前状态：本次提交将包含 `P1.1` 相关 Rust/TODO/计划文件，并按用户要求包含已变更的 `PROMPT.md`；保留未参与本任务的 `crates/atto-ui-node/index.js` 与未跟踪脚本不提交。

## 历史记录：P1.2 内容块类型

- 已读取 `TODO.md`：首个未完成任务为 `P1.2 内容块类型`。
- 已查看最近提交：`e28ca50 [P1.1] Implement new chat message model`，未声明与 `P1.2` 直接相关的未完成事项。
- 实施范围：补齐 `crates/atto-ui-chat/src/message.rs` 的剩余 block variants/types，从 `src/lib.rs` 公开导出，并调整现有过渡期 `list.rs`/`dynamic.rs` match 分支以保持编译通过。
- 已实现 `Thinking`、`Diff`、`Todo`、`Notice` block 类型和 `ToolOutput::Diff`，新增 `ThinkingBlock`、`DiffData`/`EditDecision`、`TodoBlock`/`TodoItem`/`TodoState`、`NoticeBlock`/`NoticeLevel`。
- 已为完整 block id 覆盖、thinking streaming 状态同步、diff tool output 文本更新补充单元测试。
- 首次 `clippy` 失败原因：过渡渲染不再使用 `ChatMessage::first_text` 的非测试路径导致 dead code。已改为复用该 helper，同时保留 thinking block 支持。
- 已通过验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --workspace --all-targets`。
- 已更新 `TODO.md`：`P1.2` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 已检查提交前状态：本次提交包含 P1.2 相关 Rust/TODO/计划文件；保留未参与本任务的 `crates/atto-ui-node/index.js` 与未跟踪脚本不提交。
- 下一步创建任务提交并停止。

## 历史记录：P1.3 store 改造

- 已读取 `TODO.md`：首个未完成任务为 `P1.3 store 改造`。
- 已查看最近提交：`1cf224b [P1.2] Add chat content block types`，未声明与 `P1.3` 直接相关的额外未完成事项。
- 执行范围限定为 `crates/atto-ui-chat/src/store.rs` 的新 block/message store API、脏通知语义与单测；不推进 P1.4/P1.5。
- 已检查 `store.rs`、`message.rs` 与 `CHAT_UI.md` 相关章节；当前 store 仍使用按 `ChatMessageId` 的文本/工具流式更新，需改为按 `ChatBlockId`。
- 实施策略：保留基础集合操作，新增块级 API；移除/替换旧 delta 调用点；给 reactive `Property` 增加只读访问能力以实现不克隆的 `with_block`。
- 已完成首轮实现：`ChatMessageStore` 增加 `next_block_id`、`append_block`、`with_block`、按 `ChatBlockId` 的文本/工具/审批/diff/todo 更新 API，并同步 `chat_demo` 与 `snapshot_chat_app` 的旧调用点。
- 首次 `clippy` 失败原因：P1.3 删除旧 store 调用后，`ChatMessage` 的旧可变 helper 只剩测试间接使用而触发 dead code。已改 list 单测直接操作 block 并删除这些旧 helper。
- 验证已完成：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 已更新 `TODO.md`：`P1.3` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 已检查提交前状态：本次提交将包含 P1.3 相关 Rust/TODO/计划文件；保留未参与本任务的 `crates/atto-ui-node/index.js` 与未跟踪脚本不提交。
- 下一步创建任务提交并停止。

## 历史记录：P1.4 序列化新形 + 旧形兼容

- 已读取 `TODO.md`：首个未完成任务为 `P1.4 序列化新形 + 旧形兼容`。
- 已查看最近提交：`080c0dd [P1.3] Rewrite chat message store`，未声明与 `P1.4` 直接相关的额外未完成事项。
- 执行范围限定为 `crates/atto-ui-chat/src/dynamic.rs` 的消息序列化/解析和单测；不推进 P1.5 渲染过渡或 P1.6 JS 侧同步。
- 已检查 `CHAT_UI.md` §8、`dynamic.rs` 和 `message.rs`：需要输出新形 `{id,role,status,meta?,blocks:[...]}`，并保留旧形顶层 `content`/`markdown`/`tool_call`/`file`/`artifact` 兼容解析。
- 实施策略：新增 block 级序列化/解析 helper，覆盖 meta、失败状态、工具输入输出、审批、diff/todo/notice/artifact 等字段；旧形解析保留 `sender`、`status:"in_progress"` 与派生 block id。
- 已完成实现：`message_to_value` 现在只输出 `role/status/meta?/blocks` 新形；`parse_message_value` 优先解析新形 `blocks`，否则走旧形兼容路径。
- 已补充单测：新形输出断言、完整多 block round-trip、旧形顶层 `content`/`markdown`/`file`/`artifact`/`tool_call` 解析兼容。
- 验证已完成：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 已更新 `TODO.md`：`P1.4` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 已检查提交前状态：本次提交将包含 P1.4 相关 `TODO.md`、`memory/claude_plan.md`、`crates/atto-ui-chat/src/dynamic.rs`；保留未参与本任务的 `crates/atto-ui-node/index.js` 与未跟踪脚本不提交。
- 下一步创建任务提交并停止。

## 历史记录：P1.5 渲染过渡(每 block 一行)

- 已读取 `TODO.md`：首个未完成任务为 `P1.5 渲染过渡(每 block 一行)`。
- 已查看最近提交：`d14da67 [P1.4] Implement chat message serialization`，未声明与 `P1.5` 直接相关的额外未完成事项。
- 执行范围限定为 `crates/atto-ui-chat/src/list.rs` 和 `crates/atto-ui-chat/src/bin/snapshot_chat_app.rs` 的渲染过渡；不推进 P1.6 JS 侧同步或 P2 回合头拆分。
- 已检查 `list.rs`：当前实现每条消息只生成一个行 key，并只渲染第一个非 `ToolResult` block，工具输出通过 `ToolUse` 行间接显示。
- 实施策略：将 row key 过渡为 block 级，空消息保留消息级占位；按消息内 `blocks` 顺序生成行；`ToolResult` 单独渲染为 disclosure 行；保持文本/工具输出 delta 不进入 key，避免流式更新频繁重建。
- 已完成实现：`ChatMessageList` 现在为每个 block 生成独立行，`Text`/`Thinking` 继续使用 Markdown 渲染，`ToolUse` 和 `ToolResult` 分别渲染为 disclosure，工具输出绑定到 `ToolResult` block 刷新。
- 已更新 `snapshot_chat_app`：默认种子数据包含一个由 `Text`、`Thinking`、`Notice` 组成的多 block assistant 回合。
- 首次 `clippy` 失败原因：P1.5 后旧 `ChatMessage::first_*` helper 不再使用，且一个嵌套 `if` 可折叠。已删除 stale helper 并修正 lint。
- 验证已完成：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 已更新 `TODO.md`：`P1.5` 标题已加 `[DONE]`，完成记录和验证结果已写入。

## 历史记录：P1.6 运行时/JS 侧同步

- 已读取 `TODO.md`：首个未完成任务为 `P1.6 运行时/JS 侧同步`。
- 已查看最近提交：`8b690fc [P1.5] Render chat blocks as rows`，未声明与 `P1.6` 直接相关的额外未完成事项。
- 执行范围限定为 `crates/atto-ui-node`、`packages/core`、`packages/react` 和 `docs/NODE_API.md` 的 JS/TS 侧同步；不推进 P2 渲染结构任务。
- 已确认 Rust `crates/atto-ui-chat/src/dynamic.rs` 已使用新消息形：`{id, role, status, meta?, blocks}`，block 用 `type` 和 `block_id` 区分；旧形仅作为解析兼容。
- 已发现工作区既有未提交变更：`crates/atto-ui-node/index.js` 已修改，且有未跟踪脚本 `notification.sh`、`run_agent.sh`。这些不是本次计划产生的变更；除非验证显示它们阻塞当前任务，否则不修改或回退。
- 已更新 `packages/core/src/builders.ts` 与对应 CommonJS 文件，使 chat builder 产出新 block 形，并补充 Text/Thinking/ToolUse/ToolResult/Diff/Todo/Attachment/Notice/Artifact block 构造器。
- 已同步 core builder 与类型测试中的 chat 期望。
- 已同步 React：新增 `ChatMessageList` wrapper、raw JSX `chatMessageList`/`chatmessagelist` 类型、host type 映射，并在 React 类型测试中覆盖新消息 builder。
- 已同步 `crates/atto-ui-node/index.d.ts` 的 raw chat value 类型声明，并更新 `docs/NODE_API.md` 的 chat 段，说明新 block-based shape 和 builders。
- 验证已通过：`npm run typecheck --prefix packages/core`、`node packages/core/__test__/builders.cjs`、`npm run typecheck --prefix packages/react`、`npm run build --prefix packages/react`、`npm run test:runtime --prefix packages/core`、`npm run smoke --prefix examples/react-tsx`、`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`、`npm test --prefix packages/core`、`npm test --prefix packages/react`。
- 后续调整 `ChatToolCallMessage` 的默认 turn status：`pending`/`running` 生成 `streaming`，其他工具状态生成 `complete`；已重跑 `npm run typecheck --prefix packages/core`、`node packages/core/__test__/builders.cjs`、`npm test --prefix packages/core`、`npm run typecheck --prefix packages/react`、`npm run smoke --prefix examples/react-tsx`。
- 已更新 `TODO.md`：`P1.6` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 下一步检查提交前状态/diff/最近提交，确认提交范围后创建任务提交并停止。

## 历史记录：P2.1 行模型改为「回合头 + 各 block」

- 已读取 `TODO.md`：首个未完成任务为 `P2.1 行模型改为「回合头 + 各 block」`。
- 已查看最近提交：`7a13613 [P1.6] Sync JavaScript chat block shape`，未声明与 `P2.1` 直接相关的额外未完成事项。
- 执行范围限定为 `crates/atto-ui-chat/src/list.rs` 的行 key 与渲染结构；不推进 P2.2 的去全量 clone、P2.3 的完整 block 控件映射或 P2.4 滚动修复。
- 已确认现状：P1.5 已让每个 block 独立成行，但 header/timestamp 仍嵌在每个 block 行中，多 block 回合会重复 header。
- 实施策略：将行 key 拆为 `ChatRowKey::Header` 与 `ChatRowKey::Block`；每个消息先生成 header 行，再按 block 顺序生成 block 行；block `kind_tag` 继续排除 markdown/output/status 等流式易变字段。
- 已完成实现：header 行负责回合角色/状态/时间戳，block 行只渲染内容气泡；header 文本通过 binding 随回合状态刷新，block 行不再重复角色 header。
- 已补充/调整单测：覆盖文本 delta/status 不改行 key、工具 output/status 不改 block key、多 block 消息只产生一个 header 且 block 顺序稳定。
- 验证已完成：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 已更新 `TODO.md`：`P2.1` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 下一步检查提交前状态/diff/最近提交，确认提交范围后创建任务提交并停止。

## 历史记录：P2.2 去除每行全量 clone(性能头号)

- 已读取 `TODO.md`：首个未完成任务为 `P2.2 去除每行全量 clone(性能头号)`。
- 已查看最近提交：`1250696 [P2.1] Split chat rows into turn headers`，未声明与 `P2.2` 直接相关的额外未完成事项。
- 执行范围限定为 chat list 的行同步性能路径和 store 版本跟踪；不推进 P2.3 的完整 block 控件映射或 P2.4 的滚动修复。
- 已定位 `sync_body_bindings` 中的 `self.messages.get()` 全量 clone 路径，以及 `ChatMessageRow` 持有整条 messages binding 的设计。
- 实施方案：让 `ChatMessageList` 持有 `ChatMessageStore`，在 store 中维护 message/block 版本号；header 行按 message 版本同步，block 行按自身 block 版本同步，并通过 `with_message`/`with_block` 只读访问数据。
- 已完成实现：`ChatMessageList` 构造和动态注册改为传入 store；行构造和同步使用 store 只读访问；block 行只保存自身 `ChatBlockId`，header 行保存对应 message id；row key 刷新改为 `Binding::with`，不再克隆整条消息列表。
- 已完成 store 版本跟踪：文本 delta、工具输出、工具状态、turn status、审批/diff/todo、append/upsert/replace 等实际变更路径会 bump 对应 message/block 版本；`set_turn_status` 同步 bump Text/Thinking block 版本以刷新 streaming suffix。
- 已补充单测：覆盖 message/block 版本独立更新、目标 block delta 不影响兄弟 block、turn status 会更新文本 block 版本。
- 验证已完成：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 已更新 `TODO.md`：`P2.2` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 已检查提交前状态：本次提交将包含 P2.2 相关 Rust/TODO/计划文件；保留未参与本任务的 `crates/atto-ui-node/index.js` 与未跟踪脚本不提交。
- 下一步创建任务提交并停止。

## 历史记录：P2.3 块→控件映射

- 已读取 `TODO.md`：首个未完成任务为 `P2.3 块→控件映射`。
- 已查看最近提交：`fbf42ed [P2.2] Remove chat row message clones`，未声明与 `P2.3` 直接相关的额外未完成事项。
- 执行范围限定为 `crates/atto-ui-chat/src/list.rs` 的 block 渲染映射，以及用于验证的 `snapshot_chat_app`/PTY 覆盖；不推进 P2.4 滚动修复或 P3 工具配对语义。
- 已实现映射：Text 使用 `MarkdownViewer`；Thinking 使用 `Disclosure + MarkdownViewer` 并弱化颜色；ToolUse 使用以工具名为标题的 `Disclosure`，渲染 Text/Json 输入和静态审批信息；ToolResult 按 Ansi/Markdown/Diff 分派，Ansi 解析 SGR 样式，Diff 复用 `viewer::diff_line_style`；Diff block 使用 inline diff；Todo 自绘；Attachment/Notice 使用 Text；Artifact 沿用 `ArtifactLink`。
- 已新增验证：list 单测覆盖 Json 入参格式化、ANSI SGR 样式解析、diff 行样式复用；`snapshot_chat_app --block-mapping` 生成覆盖所有 block 的 deterministic 场景；PTY 测试验证终端可见渲染。
- 验证已完成：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat --test pty_chat chat_block_mapping_renders_each_block_with_target_widget -- --exact`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 已更新 `TODO.md`：`P2.3` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 代码验证已在更新 `TODO.md` 前完成；之后仅修改文档记录，无需重跑测试。
- 下一步检查提交范围并提交。

## 历史记录：P2.4 滚动修复

- 已读取 `TODO.md`：首个未完成任务为 `P2.4 滚动修复`。
- 已查看最近提交：`ac84df2 [P2.3] Render chat block widgets`，未声明与 `P2.4` 直接相关的额外未完成事项。
- 已检查 `crates/atto-ui-chat/src/list.rs`、`snapshot_chat_app.rs`、PTY 测试，以及 `atto_ui` 的 `ForEach`/`VStack` 滚动实现。
- 关键发现：旧实现把 `apply_pending_scroll` 放在 `draw` 末尾，导致新内容尺寸已经计算但本帧已渲染，滚动只能下一帧生效；prepend 也需要在新布局后、子组件渲染前按新增高度补偿。
- 实施方案：在底层 `StackCore` 增加下一次布局后生效的滚动调整（滚到底、按内容高度差保锚点），通过 `VStack`/`ForEachIdentifiable` 暴露给 `ChatMessageList` 使用。
- 已完成实现：`ChatMessageList` 初始已有消息时预载滚底，消息变更在同帧排队滚底，`on_load_more` prepend 后按新增内容高度补偿 `scroll_y`，并公开 `scroll_to_bottom()`。
- 已补充/调整测试：新增 list 单测覆盖首帧预载滚底和 prepend 锚点补偿；更新 PTY 覆盖验证初始在底部、load-more 后锚点不被新历史顶走且继续上滚可见历史。
- 验证已完成：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat messages_`、`cargo test -p atto-ui-chat --test pty_chat chat_auto_follow_pauses_after_user_scrolls_up -- --exact`、`cargo test -p atto-ui-chat --test pty_chat chat_load_more_on_scroll_top -- --exact`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 已更新 `TODO.md`：`P2.4` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 已检查提交前状态：本次提交将包含 P2.4 相关 Rust/TODO/计划文件；保留未参与本任务的 `crates/atto-ui-node/index.js` 与未跟踪脚本不提交。
- 下一步创建任务提交并停止。

## 历史记录：P2.5 响应式换行 + 代码横向滚动

- 已读取 `TODO.md`：首个标题未带 `[DONE]` 的任务为 `P2.5 响应式换行 + 代码横向滚动`。
- 任务目标：在 `crates/atto-ui-chat/src/list.rs` 中让 Text/Markdown 的换行宽度使用布局得到的气泡内容宽度并随 resize 重算，移除固定 `wrap_width=72`；代码、diff、工具输出区域允许水平滚动，移除强制 `horizontal_scrollbar(Never)` 的限制。
- 执行边界：只完成 P2.5，不推进 P2.6 或 P3 工具语义；若发现实现被未跟踪的规格缺口或失败测试阻塞，将把最小前置任务写入 `TODO.md` 后提交并停止。
- 下一步：查看最近提交，确认是否提到与 P2.5 直接相关的未完成事项；随后阅读 `list.rs` 中 MarkdownViewer、代码/diff/工具输出滚动相关实现和现有测试。
- 已查看最近提交：`299b6be [P2.4] Fix chat scrolling`，未声明与 `P2.5` 直接相关的额外未完成事项。
- 已检查当前工作区：存在既有未提交变更 `crates/atto-ui-node/index.js`，以及未跟踪 `notification.sh`、`run_agent.sh`；这些不属于当前任务，除非阻塞 P2.5，否则不修改、不回退、不提交。
- 实施计划：定位固定 wrap 宽度和 horizontal scrollbar 设置；将正文 MarkdownViewer 的 wrap 宽度绑定到行/气泡布局宽度；确保 resize 后相关 binding 会重新同步或重建；为代码块、diff 与工具输出容器启用水平滚动或取消禁用设置；补充或调整 PTY/单元测试覆盖 resize 换行和横向滚动可见性。
- 验证计划：先运行针对性测试覆盖新增行为；再按要求执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 完成计划：验证通过后更新 `TODO.md` 标题为 `[DONE] P2.5 ...` 并填写完成记录；检查 `git status`/`git diff`/最近提交，提交本任务相关文件后停止。
- 已阅读 `list.rs` 与 `MarkdownViewer`：如果直接移除 `.wrap_width(...)`，Markdown 首次布局高度会在拿到真实宽度前退化为 1 行；因此需要为 chat 行提供响应式宽度绑定，并在 draw 时用真实气泡宽度刷新。
- 已阅读滚动实现：外层 `VStack` 的水平滚动不会自动让 block 内的 diff/ANSI 长行变成可滚动区域；P2.5 需要在 diff/ANSI 工具输出视图自身实现水平偏移，同时保留 Markdown 代码块已有的嵌入式水平滚动能力。
- 修订实施计划：给 `MarkdownViewer` 增加可绑定 wrap width 的小 API；chat Markdown body 使用响应式 wrapper，以列表布局估算宽度作为首次布局 fallback，并在绘制时同步真实气泡宽度；`DiffView`/`AnsiOutputView` 增加 `scroll_x`、viewport/content size、鼠标/键盘横向滚动处理；默认列表滚动配置不再强制 `horizontal_scrollbar(Never)`。
- 已完成首轮实现并通过 `cargo check -p atto-ui-chat --all-targets`：chat 默认不再使用固定 `72` 宽度；Text/Thinking/Markdown 工具输出通过响应式宽度绑定按气泡宽度重算；Diff/ANSI 输出增加水平偏移和滚动事件处理；列表默认滚动配置恢复为 `ScrollConfig::default()`。
- 下一步补充单元测试和 PTY 场景，覆盖窄/宽布局换行差异与 diff/ANSI 横向滚动。
- 已补充测试：list 单测覆盖响应式换行高度、DiffView 横向偏移、AnsiOutputView 横向偏移；`snapshot_chat_app --responsive-layout` 与 PTY 测试覆盖窄/宽终端下同一文本的换行差异。
- 已运行验证：`cargo fmt --all`、`cargo test -p atto-ui-chat --lib`、`cargo test -p atto-ui-chat --test pty_chat chat_markdown_wraps_to_responsive_bubble_width -- --exact` 通过。曾误用多个 test filter 运行 `cargo test -p atto-ui-chat ...`，该命令因参数格式错误未执行测试，已改用 `--lib` 覆盖新增单测。
- 下一步按全量验证顺序继续执行 `cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 全量验证已通过：`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 下一步更新 `TODO.md`：将 `P2.5` 标题加 `[DONE]` 并写入完成记录与验证命令；随后检查提交范围并提交。
- 已更新 `TODO.md`：`P2.5` 标题已加 `[DONE]`，完成记录和验证结果已写入。更新后仅变更任务记录文档，无需重跑验证。
- 下一步检查 `git status`、`git diff`、最近提交记录，确认本次提交包含 P2.5 相关源码、测试、TODO 和计划文件，并排除既有无关工作区变更。
- 已检查提交前状态/diff/最近提交：本次提交应包含 `TODO.md`、`memory/claude_plan.md`、`crates/atto-ui-chat/src/list.rs`、`crates/atto-ui-chat/src/bin/snapshot_chat_app.rs`、`crates/atto-ui-chat/tests/pty_chat.rs`、`crates/atto-ui-markdown/src/markdown/viewer.rs`。既有无关变更 `crates/atto-ui-node/index.js` 与未跟踪 `notification.sh`、`run_agent.sh` 不提交。
- 下一步只 stage 本任务相关文件，创建 `[P2.5] Add responsive chat wrapping` 提交并停止。

## 历史记录：P2.6 杂项渲染修复

- 已读取 `TODO.md`：首个标题未带 `[DONE]` 的任务为 `P2.6 杂项渲染修复`。
- 已查看最近提交：`ea0b02d [P2.5] Add responsive chat wrapping`，未声明与 `P2.6` 直接相关的额外未完成事项。
- 执行范围限定为 `crates/atto-ui-chat/src/list.rs` 的进行中指示、时间戳分隔线和 agent 场景默认显示策略；不推进 P3 工具语义。
- 实施计划：保留 Text 流式光标，避免 Thinking 这种已有 running disclosure 状态的 block 再追加光标；将时间戳分隔线改为显示宽度安全计算；默认隐藏逐条时间戳，并用弱化加粗的回合 header 强调回合边界。
- 已完成实现：`ChatMessageList` 默认 `show_timestamps=false`；Thinking block 渲染不再追加 `" ▍"`，仍通过 `DisclosureStatus::Running` 表示进行中；`ChatTimestampDivider` 使用 `UnicodeWidthStr::width` 和宽度安全截断；回合 header 使用弱化加粗样式。
- 已补充单测：覆盖默认隐藏时间戳、Text 保留流式光标但 Thinking 不重复光标、Unicode 时间戳分隔线显示宽度和截断。
- 首次全量测试发现 full-width 回合分隔线方案会影响 chat PTY 滚动夹具；已收敛为 header 样式强化，并单独复测 `chat_auto_follow_pauses_after_user_scrolls_up` 与 `chat_load_more_on_scroll_top` 通过。
- 验证已完成：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- 已更新 `TODO.md`：`P2.6` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 提交前检查发现初始计划写入覆盖了历史记录；已恢复既有历史并追加本次 P2.6 记录。
- 已检查提交前状态：本次提交将包含 P2.6 相关 `TODO.md`、`memory/claude_plan.md`、`crates/atto-ui-chat/src/list.rs`；保留未参与本任务的 `crates/atto-ui-node/index.js` 与未跟踪 `notification.sh`、`run_agent.sh` 不提交。
- 下一步只 stage 本任务相关文件，创建 `[P2.6] Fix miscellaneous chat rendering` 提交并停止。

## 历史记录：P3.1 ToolUse 入参渲染

- 已读取 `TODO.md`：首个标题未带 `[DONE]` 的任务为 `P3.1 ToolUse 入参渲染`。
- 已查看最近提交：`f16b3c1 [P2.6] Fix miscellaneous chat rendering`，未声明与 `P3.1` 直接相关的未完成事项。
- 执行范围限定为 ToolUse 入参展示和状态图标，不推进 P3.2 的 ToolResult 配对或 P3.3 的超长输出尾部窗口。
- 已确认现状：ToolUse 标题已使用工具名，Json 已能输出 key/value 文本；不足是 Text 入参没有明确单行/代码式呈现，ToolUse 内容不会随 block 版本同步刷新，且 `ToolStatus::Canceled` 复用 Error 图标。
- 实施计划：新增 ToolUse 内容子视图与绑定，渲染 Text/Json/审批文本并支持后续同步；扩展 `DisclosureStatus` 增加 Canceled 图标；更新 snapshot/PTY 和单元测试覆盖 Text/Json/status 映射。
- 已完成实现：`ToolUseDetailsView` 渲染 Text 入参为 `Input: ...` 单行或多行缩进代码块，Json 入参为 key/value 列表；ToolUse 行绑定随 block 版本同步入参和审批文本；`ToolStatus::Canceled` 映射到独立 `DisclosureStatus::Canceled`，默认图标为 `[-]`。
- 已补充验证：snapshot chat app 的 tool-call 场景填充 Text 入参并支持切到 Canceled；PTY 测试覆盖 Text 入参、Json 入参和 Pending/Running/Done/Error/Canceled 图标；单测覆盖 Text/Json 入参格式与 Canceled 映射。
- 验证已通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 已更新 `TODO.md`：`P3.1` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 提交前检查发现初始计划写入覆盖了历史记录；已恢复既有历史并追加本次 P3.1 记录。
- 已检查提交前状态：本次提交将包含 P3.1 相关 `TODO.md`、`memory/claude_plan.md`、`crates/atto-ui-chat/src/list.rs`、`crates/atto-ui-chat/src/bin/snapshot_chat_app.rs`、`crates/atto-ui-chat/tests/pty_chat.rs`、`src/widgets/disclosure.rs`、`src/theme/mod.rs`、`src/component_api.rs`；保留未参与本任务的 `crates/atto-ui-node/index.js` 与未跟踪 `notification.sh`、`run_agent.sh` 不提交。
- 下一步只 stage 本任务相关文件，创建 `[P3.1] Render tool use inputs` 提交并停止。

## 当前任务：P3.2 ToolResult 渲染 + use/result 配对

- 已在执行任何项目命令前写入本轮可审计计划；该初始写入曾覆盖历史记录，提交前已恢复既有历史并追加本次记录。
- 已读取 `TODO.md`：首个标题未带 `[DONE]` 的任务为 `P3.2 ToolResult 渲染 + use/result 配对`。
- 已查看最近提交：`b6ac361 [P3.1] Render tool use inputs`，未声明与 `P3.2` 直接相关的未完成事项。
- 执行范围限定为 ToolResult 输出渲染和 ToolUse/ToolResult 的 `call_id` 配对；不推进 P3.3 的超长输出尾部窗口。
- 已确认现状：P2.3 已具备 `ToolOutput::Ansi`/`Markdown`/`Diff` 的基础渲染；缺口是行扁平化仍按原始 block 顺序显示 result，缺失 result 时无等待行。
- 实施计划：保留现有输出渲染组件；在 `row_keys_from_messages` 中预索引 ToolResult，遍历 ToolUse 时配对同消息或后续消息中首个未配对 result 并紧邻插入，原位置跳过；未找到 result 时插入 pending result 行显示“等待中”。
- 已完成实现：新增 `PendingToolResult` 行 key/id/ref；ToolUse 按 `call_id` 与后续 ToolResult 相邻展示；缺失 result 渲染运行中 disclosure，标题和内容显示“等待中”。
- 已补充验证：单测覆盖同消息重排、后续消息配对且不重复 header、缺失 result 等待行；PTY block mapping 用例断言 `Tool result: call-json (等待中)`，并先滚到顶部以适配预载 auto-scroll 行为。
- 验证已通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat --test pty_chat chat_block_mapping_renders_each_block_with_target_widget -- --exact`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 已更新 `TODO.md`：`P3.2` 标题已加 `[DONE]`，完成记录和验证结果已写入。
- 已检查提交前状态：本次提交将包含 P3.2 相关 `TODO.md`、`memory/claude_plan.md`、`crates/atto-ui-chat/src/list.rs`、`crates/atto-ui-chat/tests/pty_chat.rs`；保留未参与本任务的 `crates/atto-ui-node/index.js` 与未跟踪 `notification.sh`、`run_agent.sh` 不提交。
- 下一步只 stage 本任务相关文件，创建 `[P3.2] Pair tool results with uses` 提交并停止。
