# TODO：Chat 控件对齐 Claude Code 能力缺口

执行计划见 [`PLAN.md`](PLAN.md)，缺口分析见 [`AGENT_GAP.md`](AGENT_GAP.md)。
编号 `Pn.m` 对应 PLAN 的阶段。所有改动均针对 `crates/atto-ui-chat`（或 `crates/atto-ui-markdown`），除非另注。

通用验收（每条任务完成都要满足）：`cargo build` / `cargo test`（含 PTY）/
`cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --all -- --check` 全过（CI 同款）。

> 每个阶段最后有一个独立的 **Review** 任务，用来确保本阶段改动的正确性与完整性——它不是走过场，
> 而是要逐条复核该阶段所有任务的实现、边界与测试覆盖，并跑通全套 CI 命令后才算完成。

> **不含图片/多模态内联渲染**（`AGENT_GAP.md` B2），见 `PLAN.md` 范围说明。

## 阶段 P1 — 渲染保真度（B1 + B3）

参考 `AGENT_GAP.md` B1、B3。改动集中在 `crates/atto-ui-markdown` 与 chat diff 渲染。

- [x] **[DONE] P1.0 语法高亮方案选型** — 评估 syntect / tree-sitter / 轻量自研 tokenizer 三种路线的体积、编译时间、`#![forbid(unsafe_code)]` 兼容性与语言覆盖；确定选型并记录到 `AGENT_GAP.md`（附理由）。产出：依赖决定 + 高亮接口草案。
  - 完成记录（2026-07-09）：已在 `AGENT_GAP.md` 增补 P1.0 选型记录，决定 P1.1/P1.2 采用 `syntect`，但显式关闭默认特性并使用 `default-syntaxes` + `regex-fancy`，避免 `default-onig`/oniguruma 路线；同时记录 tree-sitter 与轻量自研 tokenizer 的取舍理由。
  - 接口草案：记录 `atto-ui-markdown::syntax` 封装、`LanguageHint`、`HighlightedLine`/`HighlightedSpan`/`SyntaxClass` 中立输出、代码块宽度/滚动保留方式，以及 chat diff payload 复用同一高亮器且不覆盖 +/- 语义样式的合成约束。
  - 验证：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
  - 测试失败处理：完整测试首次运行时 `tests/pty_rich_text.rs::pty_rich_text_handles_link_click` 曾出现空屏超时；已单独复现调查，保留 raw PTY 输出诊断增强，随后该 exact 用例重复通过，完整 `cargo test --all --all-targets` 复跑通过。
- [x] **[DONE] P1.1 代码块语法高亮** — `crates/atto-ui-markdown`：为 fenced code block 按 fence info string（语言标识）做语法高亮；无语言标识或不识别时回退纯文本。高亮结果落到现有 `Line`/`Span` 渲染路径，保持既有的代码块水平滚动与换行行为。
  - 完成记录（2026-07-09）：新增 `atto_ui_markdown::syntax` 公共封装，使用 `syntect`（关闭默认特性，启用 `default-syntaxes` + `regex-fancy`）将 fence info string 的首词/文件扩展名解析为语言提示，并输出 `HighlightedLine` / `HighlightedSpan` / `SyntaxClass` 中立结构，不向调用方暴露 syntect 类型。
  - 渲染实现：`CodeBlockState` 保留原有纯文本行用于宽度计算和嵌入式水平/垂直滚动，同时保存可选高亮行；已知语言按 syntax span 绘制，缺失或未知语言继续走纯文本 fallback；高亮样式通过 `MarkdownStyles` 映射并 patch 到既有 `markdown-code-block` 样式上。
  - 测试覆盖：新增 syntax hint/fallback/Rust 高亮单测，以及 markdown code block state 已知语言高亮和未知语言 fallback 单测。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。完整测试首次运行发现 Rust `storage.*` scope 分类未映射为 `SyntaxClass::Keyword`，已修复并用 `cargo test -p atto-ui-markdown --lib` 及完整套件复验通过。
- [x] **[DONE] P1.2 diff 语法高亮** — `src/list.rs`（diff 渲染）：在现有 +/- 行着色基础上，对 diff 内容按语言做语法层着色（复用 P1.1 的高亮引擎）；确保 +/- 增删语义的背景/前景不被语法色覆盖。
  - 完成记录（2026-07-09）：`crates/atto-ui-chat/src/list.rs` 的 `DiffView` 与 `DiffDecisionView` 现在保存可选 path，并通过显式 `DiffBlock.path` 或 unified diff header（`diff --git` / `---` / `+++`）推断语言，复用 `atto_ui_markdown::syntax::highlight_code_block` 高亮 diff payload。
  - 样式合成：diff 行先分类为文件头、hunk、增删、上下文或其他行；仅对增删/上下文 payload 做语法分段；增删行最后重新应用 diff 语义前景/背景，确保 `+` / `-` 行的绿色/红色语义不被语法色覆盖。
  - 测试覆盖：新增 list 单测覆盖既有 unified diff 语义色、显式 path 的 Rust payload 高亮、增行语义色压过 syntax fg、无显式 path 时从 header 推断语言，以及 hunk 内 `---` payload 仍按删除行处理。
  - 验证：`cargo fmt --all`、`cargo clippy --all-targets -- -D warnings`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat diff_display_lines`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P1.3 快照与测试** — `snapshot_markdown_app` / `snapshot_chat_app` 增加多语言代码块与带语法高亮的 diff 场景；`tests/` 补 PTY 覆盖高亮着色（可通过屏幕内容/样式断言验证）。
  - 完成记录（2026-07-09）：`snapshot_markdown_app` 新增 `--syntax-highlighting` fixture，覆盖 Rust、Python fenced code block 与未知语言 fallback；`snapshot_chat_app` 新增 `--syntax-diff` fixture，覆盖带 Rust path/header 的 unified diff 场景。
  - 测试覆盖：新增 markdown PTY 样式断言，验证 Rust/Python 关键字单元格前景色不同于未知语言 fallback；新增 chat PTY 样式断言，验证 diff context payload 获得语法色，且新增/删除行的前景/背景语义色不被语法色覆盖。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-markdown --test pty_markdown_viewer_blocks pty_markdown_viewer_renders_syntax_highlighted_code_blocks`、`cargo test -p atto-ui-chat --test pty_chat chat_syntax_diff_highlights_context_and_preserves_semantic_lines`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P1.R Review：P1 阶段复核** — 逐条复核 P1.0–P1.3：确认高亮选型无 unsafe、无语言标识回退正确、diff +/- 语义未被破坏、宽字符/中文代码不错位、超长/未闭合代码块不 panic；确认新增场景有 PTY 覆盖；跑通 `cargo build`/`cargo test`/`cargo clippy`/`cargo fmt --check`。发现问题回填对应任务而非放行。
  - 完成记录（2026-07-09）：已逐条复核 P1.0–P1.3。`atto-ui-markdown` 采用 `syntect` 且关闭默认特性，仅启用 `default-syntaxes` + `regex-fancy`；高亮封装隐藏 syntect 类型，未知或缺失语言标识继续回退纯文本；代码块保留 plain lines 用于宽度计算与嵌入式水平/垂直滚动，宽字符/中文渲染继续走 grapheme + Unicode width 切片路径；未闭合 fence 已由 tolerant parser 覆盖，长代码块滚动 PTY 覆盖通过。
  - diff 复核：chat diff 复用 markdown 高亮器，显式 path 与 unified diff header 均可推断语言；只高亮 diff payload；新增/删除行最后保留语义前景/背景，`+`/`-` 语义未被语法色覆盖。
  - 覆盖复核：`snapshot_markdown_app --syntax-highlighting` 与 markdown PTY 覆盖 Rust/Python/未知语言 fallback；`snapshot_chat_app --syntax-diff` 与 chat PTY 覆盖 context payload 语法色及增删行语义色保留。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。

## 阶段 P2 — 输入补全：斜杠命令 + @文件提及（A1 + A2）

参考 `AGENT_GAP.md` A1、A2。核心是在 `input.rs` 上叠加 overlay 补全菜单。

- [x] **[DONE] P2.1 补全 overlay 组件** — 新增可复用的 completion popup：候选列表 + 匹配高亮 + 键盘上下选择 + Enter 确认 + Esc 关闭，锚定输入框上/下方，处理边界（空候选、超长列表滚动、宽字符）。先独立组件 + 单测/快照。
  - 完成记录（2026-07-09）：新增 `crates/atto-ui-chat/src/completion.rs`，提供独立可复用的 `CompletionPopup`、`CompletionItem`、`CompletionAnchor`、`CompletionPlacement`，并从 `atto-ui-chat` 导出。组件通过 `Binding` 管理 `query` / `items` / `open` / `selection` / `accepted`，复用 `atto_ui::fuzzy` 过滤候选并对匹配字素做高亮。
  - 渲染与交互：popup 根据输入框 `Rect` anchor 在给定边界内自动选择上方或下方绘制；支持空候选提示、超长列表随选择滚动、宽字符/组合字符安全裁剪；打开状态下处理 Up/Down 选择、Enter 确认并写入 accepted binding、Esc 关闭，Release 与 Ctrl+Up/Down 不捕获。
  - 测试覆盖：新增 completion 单测覆盖键盘选择/确认/关闭、空候选、长列表滚动、自动上方锚定、匹配高亮、宽字符裁剪，以及不应捕获的事件路径。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-chat completion`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P2.2 斜杠命令** — `input.rs`：输入行首 `/` 触发命令菜单；命令集合由宿主注入（`register`/provider 回调），内置示例若干；输入变化实时过滤；确认后写回输入或触发命令回调。
  - 完成记录（2026-07-09）：新增 `ChatSlashCommand` / `ChatSlashCommandAction` Rust API，`ChatInputHandle` 默认内置 `/help`、`/clear`、`/model`、`/review` 示例命令，并支持 `set_slash_commands`、`slash_commands_binding`、`register_slash_command` 由宿主注入或替换命令。
  - 输入交互：`ChatInputPanel` 在文本输入模式下检测行首 `/` draft，复用 P2.1 `CompletionPopup` 渲染命令菜单，输入变化实时更新 fuzzy query；Up/Down/Enter/Esc 由 popup 优先处理，其它输入继续传给 `TextArea`；Esc 对当前 draft 关闭直到内容变化。
  - 确认语义：插入型命令写回 replacement 到 draft；提交型命令在注册 `on_slash_command` 时触发回调并按 `clear_on_submit` 清空 draft，无回调时回退为写回 replacement。
  - 测试覆盖：新增 input 单测覆盖行首触发规则、实时过滤渲染、插入确认、回调确认、Esc 关闭直到 draft 变化，以及 register 替换同 id 命令。
  - 验证：`cargo fmt --all`、`cargo clippy --all-targets -- -D warnings`、`cargo test -p atto-ui-chat input`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P2.3 @ 文件提及** — `input.rs`：输入 `@` 触发文件/资源补全，候选由宿主 provider 回调提供；确认后在输入中渲染为 mention 芯片或路径文本；处理光标位置与多次提及。
  - 完成记录（2026-07-09）：新增 `ChatMentionCandidate` / `ChatMentionContext` Rust API，并从 `atto-ui-chat` re-export；`ChatInputHandle` 支持静态 mention 候选绑定、设置和按 id 注册替换；`ChatInputPanel::mention_provider` 支持宿主按当前 draft/query/cursor/range 同步返回文件或资源候选。
  - 输入交互：文本模式下光标位于以 `@` 开头的 token 内时触发 mention popup，复用 P2.1 `CompletionPopup` 做 fuzzy 过滤、选择、Enter 确认和 Esc 关闭；mention popup 优先于 slash popup；无 provider/候选时静默降级；email/非 token 起始 `@` 不误触发。
  - 确认语义：新增 `TextArea` 公开光标与 byte range 替换 API；接受候选时只替换当前光标所在 `@query` token，默认插入 `@path` 路径文本，支持自定义 replacement，且不会影响同一 draft 中的其它 mention。
  - 测试覆盖：新增 input 单测覆盖 query/range 识别、provider context、popup 过滤、接受插入、光标处替换、多次提及、无 provider 降级、email 不误触发、register 替换同 id 候选；新增 TextArea 单测覆盖区间替换后的绑定与光标位置。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat input`、`cargo test -p atto-ui --lib textarea`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P2.4 运行时/JS 侧同步** — `src/dynamic.rs`：暴露命令/提及协议与回调 + schema；同步 `crates/atto-ui-node`、`packages/core`、`packages/react` 的类型/构造器，更新 `docs/NODE_API.md`；跑 `npm run smoke --prefix examples/react-tsx` 与 core runtime 兼容测试。
  - 完成记录（2026-07-09）：`ChatInputPanel` 动态 schema 新增 `slash_commands`、`mention_candidates` 属性和 `slash_command`、`mention_query` 事件；运行时构建与 `set_property` 支持 slash command / mention candidate 列表，submit-action slash command 会向 JS 发出命令 payload，mention provider 通过 `mention_query` 暴露 `{ draft, query, cursor, replacement_start, replacement_end }` 并读取最新候选属性。
  - JS 同步：`crates/atto-ui-node/index.d.ts`、`packages/core` builder/types、`packages/react` wrapper/raw JSX/types 已新增 `ChatSlashCommand`、`ChatMentionCandidate`、`slashCommands`、`mentionCandidates`、`onSlashCommand`、`onMentionQuery`；`docs/NODE_API.md` 已记录协议与事件 payload。为避免 Bun 本地测试加载全局缓存旧 native，`packages/core/native.js` 现在在 optional platform 包前优先尝试 workspace `crates/atto-ui-node` fallback，并同步 README/API 文档加载顺序。
  - 测试覆盖：新增 Rust dynamic schema/属性测试、core builder/type/runtime schema 断言、React 类型覆盖。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`npm run build --prefix packages/react`、`npm run typecheck --prefix packages/core`、`npm run typecheck --prefix packages/react`、`node packages/core/__test__/builders.cjs`、`npm run smoke --prefix examples/react-tsx`、`npm run test:runtime --prefix packages/core`、`npm test --prefix packages/react`、`npm test --prefix packages/core`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P2.5 快照与测试** — `snapshot_chat_app` 增加 slash/mention 场景；PTY 覆盖 `/` 触发→过滤→选择→确认、`@` 触发→文件补全→确认插入、Esc 关闭。
  - 完成记录（2026-07-09）：`snapshot_chat_app` 新增 `--input-completion` fixture，提供确定性的 slash 命令、mention provider 文件候选和 slash submit 回调输出；该 fixture 下普通输入字符不再被 snapshot 演示快捷键拦截，确保 PTY 能真实输入 `/` 与 `@` 查询。
  - 测试覆盖：新增 chat PTY 用例覆盖 `/` 打开命令菜单、按 query 过滤、键盘 Down 选择、Enter 确认插入、submit-action slash 命令回调，以及 Esc 关闭；新增 mention PTY 用例覆盖 `@` 打开文件补全、过滤到文件候选、Enter 插入 mention 文本、提交后回显，以及 Esc 关闭文件 popup。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-chat --test pty_chat completion -- --nocapture`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P2.R Review：P2 阶段复核** — 逐条复核 P2.1–P2.5：确认 overlay 焦点/键盘语义与既有输入不冲突、Esc/Enter 行为一致、空候选与无 provider 时优雅降级、mention 芯片光标编辑正确、JS 侧类型与 Rust 协议一致；确认 PTY + smoke 全过；跑通全套 CI。
  - 完成记录（2026-07-09）：已逐条复核 P2.1–P2.5。`CompletionPopup` 在 open 状态下独占 Up/Down/Enter/Esc，Release 与 Ctrl+Up/Down 不捕获；空候选阻止提交但保持弹层；超长列表滚动、宽字符裁剪和上下锚定均有单测覆盖。`ChatInputPanel` 中 mention popup 优先于 slash popup，Esc 只关闭当前 draft/context，Enter 对 insert/submit slash command 与 mention token 替换的语义一致。
  - 边界复核：slash 仅行首单行 `/` 触发，命令可由 handle 注入/替换；mention 仅在光标所在 `@token` 内触发，无 provider/候选时静默降级，email 不误触发，多次提及时只替换当前光标 token；`TextArea::replace_byte_range` 同步 binding 并把光标置于插入文本之后。Rust dynamic schema、Node/core/react 类型、React wrapper/raw JSX 事件名与 `docs/NODE_API.md` 的 `slash_command` / `mention_query` 协议一致。
  - 复核修复：验证中发现裸 `napi build` 会重写 `crates/atto-ui-node/index.js` / `index.d.ts` 并丢失仓库定制 loader/type 补充，导致 CI 末尾工作区变脏；同时 stale ignored `.node` 产物会阻塞 `napi artifacts`。已新增 `crates/atto-ui-node/scripts/build.cjs`、`patch-generated.cjs`、`artifacts.cjs`，并同步 CI/release workflow 与 README/release/example 文档，确保 native build 后生成文件稳定、artifacts 收集前清理无对应平台包目录的 stale root artifact。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`、`npm run build --prefix crates/atto-ui-node`、`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`、`npm test --prefix crates/atto-ui-node`、`npm test --prefix packages/core`、`npm run test:runtime:bun --prefix packages/core`、`npm run test:runtime:deno --prefix packages/core`、`npm run typecheck --prefix packages/react`、`npm test --prefix packages/react`、`npm run smoke --prefix examples/react-tsx`、`npm run npm:artifacts --prefix crates/atto-ui-node`、`npm pack --dry-run --json ./crates/atto-ui-node`、`npm pack --dry-run --json ./crates/atto-ui-node/npm/darwin-arm64`、`npm pack --dry-run --json ./packages/core`、`npm pack --dry-run --json ./packages/react`、`git diff --check` 均通过。本机曾尝试打包 Linux 平台子包但缺少 Linux `.node` artifact；该平台包 dry-run 由 Ubuntu CI/release artifact 流程覆盖。

## 阶段 P3 — 会话管理：消息编辑 / 回退 / 重发（C1）

参考 `AGENT_GAP.md` C1。核心是 store 的截断-fork 能力。

- [x] **[DONE] P3.1 store 截断-fork API** — `src/store.rs`：新增"截断到某条消息并从该点重生成"的能力（如 `truncate_from(message_id)` / `fork_at`）；保持版本跟踪与"值未变不发脏"约定；补单测覆盖截断边界（首条/末条/中间/流式中）。
  - 完成记录（2026-07-09）：`ChatMessageStore` 新增 `truncate_from(message_id)` 与 `fork_at(message_id)`。`truncate_from` 移除目标消息及其后的旧分支，`fork_at` 保留目标消息作为 fork 点并移除其后的旧分支，二者均返回被移除的消息 suffix；目标不存在时不变更，`fork_at` 位于末条消息时返回空 suffix 且不标脏。
  - 版本/脏追踪：截断后清理被移除 message/block 的版本记录，保留未变前缀及 fork anchor 的既有版本；不回退 `next_message_id` / `next_block_id`，避免重生成时复用旧 ID；无实际值变更不触发 dirty observer。
  - 测试覆盖：新增 store 单测覆盖从首条/中间/末条 `truncate_from`、中间/末条 `fork_at`、缺失目标 no-op、流式 turn 截断时移除 streaming text/thinking block 并清理版本。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-chat store --lib`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
  - 测试失败处理：完整测试首轮曾出现 `chat_artifact_code_link_opens_text_viewer_window` 空屏超时；随后 exact 用例与 `pty_chat` 全文件复跑均通过，完整 `cargo test --all --all-targets` 复跑通过。
- [x] **[DONE] P3.2 编辑 user 消息** — `src/list.rs`：支持进入某条 user 消息编辑态，编辑后从该点截断并触发重发回调（`on_edit_and_resubmit`）；与输入区衔接（把原文回填输入）。
  - 完成记录（2026-07-09）：`ChatMessageList` 新增 `on_edit_and_resubmit(&ChatInputHandle, callback)` 专用 API，注册编辑控制器与输入提交拦截器；配置后 user 消息的 `Edit` 按钮进入 pending 编辑态并把原 text block 内容回填输入 draft，未配置时既有 `MessageActionKind::EditUser` 回调保持可用。
  - 截断/回调语义：用户提交编辑后的文本时，输入层先交给编辑控制器；控制器调用 `ChatMessageStore::truncate_from(message_id)` 移除目标 user 消息及其后的旧分支，然后触发 `EditAndResubmitEvent`，payload 包含 `message_id`、`original_text`、`edited_text` 与 `removed_messages`，供宿主重发。
  - 测试覆盖：新增 list/input 单测覆盖 user 文本提取（含多 text block 与非文本 block）、输入回填、提交后截断旧分支、事件 payload，以及配置专用编辑控制器后的 `Edit` 按钮行为。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-chat --lib`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P3.3 retry / regenerate** — `src/list.rs`：assistant 回合支持重生成（截断该回合后触发回调），与现有 `on_message_action` 的 Retry/Regenerate 打通。
  - 完成记录（2026-07-09）：Retry/Regenerate 回合按钮现在复用既有 `on_message_action` 回调入口，但在触发回调前会校验目标消息仍为 assistant 回合并调用 `ChatMessageStore::truncate_from(message_id)`，移除该 assistant 回合及其后的旧分支；Copy/Edit/CopyBlock 仍保持原有回调语义，目标缺失或非 assistant 时 Retry/Regenerate 不触发回调。
  - 测试覆盖：新增 list 单测覆盖 Retry 与 Regenerate 都会在回调前完成截断并只保留前缀消息，以及非 assistant / 缺失目标 no-op；更新 `snapshot_chat_app --message-actions` 与 PTY 用例，避免 retry 截断后同一目标上的 regenerate 按钮消失导致测试误用，并修正 Copy 与 CopyBlock 前缀匹配风险。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P3.4 快照与测试** — `snapshot_chat_app` 增加编辑/重发场景；PTY 覆盖编辑 user 后截断、retry 后回合截断、fork 后旧消息不再显示。
  - 完成记录（2026-07-09）：`snapshot_chat_app` 新增 `--edit-resubmit`、`--retry-resubmit`、`--fork-at` 三个确定性 fixture，分别覆盖 `on_edit_and_resubmit` 编辑提交后截断旧分支、assistant Retry 触发截断并追加新回复、以及直接 `fork_at` 后保留 anchor 并移除旧分支。
  - 测试覆盖：新增 chat PTY 用例 `chat_p3_edit_resubmit_truncates_old_branch`、`chat_p3_retry_resubmit_truncates_assistant_turn`、`chat_p3_fork_at_hides_old_branch`，断言新分支显示且旧 user/assistant/tail sentinel 不再出现在屏幕中。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-chat --test pty_chat chat_p3 -- --nocapture`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
  - 测试失败处理：新增 PTY retry 用例首轮曾等待已滚出屏幕的 user prompt 导致超时；已改为等待当前验收所需的可见旧 assistant/旧 tail 与 Retry 操作，随后新增 PTY 过滤测试与完整测试套件均通过。
- [x] **[DONE] P3.R Review：P3 阶段复核** — 逐条复核 P3.1–P3.4：确认截断不泄漏悬挂 block_id/版本、fork 后滚动与自动跟随正常、流式进行中编辑/重发的竞态被正确处理、回调契约清晰；确认 PTY 覆盖边界；跑通全套 CI。
  - 完成记录（2026-07-09）：已逐条复核 P3.1–P3.4。`truncate_from` / `fork_at` 会清理被移除 message/block 的版本记录，保留前缀和 fork anchor 的版本，旧 block_id 的流式 delta/status 在截断后 no-op 且不触发 dirty；新增 `ChatBranchToken`、`branch_token`、`is_branch_current` 与 `push_if_branch_current`，并在 `replace_all`、实际 `truncate_from`、实际 `fork_at` 后让旧 token 失效，供宿主阻止旧流式任务迟到追加新消息。
  - 复核修复：pending user edit 的目标若在提交前被其它截断/fork 删除，输入提交现在会被编辑拦截器消费并清空编辑态，不再退化为普通消息发送；`ChatMessageList` 通过消息 ID 序列识别截断、fork 和尾部重写，在 `auto_scroll` 开启时恢复尾部跟随，同时不影响普通追加和历史前置；`MessageAction` / `on_message_action` 与 Node/API 文档已说明 Retry/Regenerate 回调触发前会先截断目标 assistant 回合及其后的旧分支。
  - 测试覆盖：新增 store 单测覆盖缺失 truncate no-op、分支 token 对 truncate/fork/replace 的失效、当前 token 条件 push、流式截断后旧 delta/status/push 不复活旧分支；新增 list 单测覆盖 pending edit 目标消失时不触发普通 submit，以及非 tail 截断/fork 尾部重写后恢复自动跟随。既有 P3 PTY 覆盖编辑、retry、fork 后旧分支不再显示。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-chat --lib`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。

## 阶段 P4 — 输入交互增强：排队 & Esc 中断 + 多行编辑（A3 + A4）

参考 `AGENT_GAP.md` A3、A4。建立在 P2 输入层之上，衔接 P3 的中断语义。

- [x] **[DONE] P4.1 输入排队** — `input.rs`：流式进行中允许继续输入并排队新消息；流式结束后自动出队或提示用户发送；排队态有可见指示。
  - 完成记录（2026-07-09）：`ChatInputHandle` 新增 host-controlled `streaming_binding` 与 FIFO `queued_responses_binding`；文本模式下流式进行中提交不会触发 `on_submit`，而是排入队列并按 `clear_on_submit` 清空草稿；流式结束后空 Enter 会发送队首，若用户已输入新草稿则先追加到队尾再发送旧队首，保持消息顺序。
  - 可见状态：`ChatInputPanel` 在输入区底部显示 streaming/queued 状态行，覆盖“流式中可排队”“已排队 N 条”“流式结束后按 Enter 发送下一条”三种状态。
  - 复核修复：定向测试发现面板内部程序化改写 `draft` 后 `TextArea` 缓冲可能滞后，导致排队清空或 slash 替换后继续输入拼接旧文本；已统一通过 `set_draft_from_panel` 同步绑定、缓冲和光标，并增加 slash 替换回归测试。
  - 测试覆盖：新增 input 单测覆盖流式中排队与状态行、流式结束后出队、已有队列时新草稿 FIFO、编辑提交拦截器优先级，以及 slash 命令替换后的缓冲同步；`snapshot_chat_app --input-queue` 与 PTY 用例覆盖真实流式中输入排队、未提前 submit、流式完成后提示发送并按 Enter 发送。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-chat input --lib`、`cargo test -p atto-ui-chat --test pty_chat chat_input_queues_text_while_streaming_and_sends_after_prompt -- --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P4.2 Esc 中断语义** — `src/list.rs` + `input.rs`：完善 Esc 状态机——一次 Esc 中断当前流式（置 `ChatTurnStatus::Canceled`），分级/连按语义明确，与现有取消按钮统一入口。
  - 完成记录（2026-07-09）：新增共享 streaming cancel controller，取消按钮、消息列表焦点下未消费 Esc、以及 `ChatPanel` 默认输入焦点下未消费 Esc 都走同一取消入口；入口会先确认目标仍为 streaming，再将回合置为 `ChatTurnStatus::Canceled` 并触发现有 `on_cancel` 回调，连按 Esc 因目标已非 streaming 而 no-op。
  - 分级语义：completion/mention popup 的 Esc 仍优先关闭弹层；输入控件或自定义子组件已消费 Esc 时不会触发中断；未消费 Esc 才作为当前流式中断 fallback。当前流式按消息顺序选择最后一个 streaming 回合。
  - 测试覆盖：新增 input 单测覆盖 Esc fallback、popup Esc 优先级和回调拒绝时 ignored；新增 list 单测覆盖 controller 先置 canceled 再回调、取消按钮复用同一入口、列表 Esc 取消并连按幂等；新增 PTY 用例覆盖 `snapshot_chat_app --cancel-action` 下按 Esc 触发取消并显示 canceled。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo test -p atto-ui-chat escape --lib`、`cargo test -p atto-ui-chat streaming_cancel --lib`、`cargo test -p atto-ui-chat list_escape_cancels_latest_streaming_turn_once --lib`、`cargo test -p atto-ui-chat --test pty_chat chat_streaming_escape_emits_and_marks_turn_canceled -- --nocapture`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P4.3 多行编辑增强** — `input.rs`：多行粘贴规整；（可选）拖入/粘贴文件路径转 `Attachment` block。
  - 完成记录（2026-07-09）：`ChatInputPanel` 现在在文本模式下拦截 `Event::Paste` 并执行 chat 专用规整：容错剥离 bracketed paste 包裹、统一 CRLF/CR 为 LF、去除粘贴尾部空白行，同时保留正文缩进、内部空行和单行尾随空格；规整后通过 `TextArea::replace_byte_range` 插入，确保 draft binding、内部缓冲和光标位置同步。
  - 测试覆盖：新增 input 单测覆盖换行规整、尾部空白行裁剪、bracketed paste 容错、规整后继续输入不拼回旧缓冲，以及提交多行粘贴时回调和历史记录均使用规整后的文本。可选的拖入/粘贴文件路径转 `Attachment` block 未启用，本任务未改变 `ChatInputResponse` 模型。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat input --lib`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P4.4 快照与测试** — PTY 覆盖流式中排队新消息、Esc 中断置 `Canceled`、多行粘贴规整。
  - 完成记录（2026-07-09）：`snapshot_chat_app` 新增 `--multiline-paste` fixture，并在 snapshot app raw 模式下启用 bracketed paste；该 fixture 使用 `PASTE_SUBMIT: {text:?}` 回显提交文本，便于 PTY 精确断言 CRLF 归一化与尾部空白行裁剪后的值。
  - 测试覆盖：保留并复验 P4.1/P4.2 的 `chat_input_queues_text_while_streaming_and_sends_after_prompt` 与 `chat_streaming_escape_emits_and_marks_turn_canceled`；新增 `chat_multiline_paste_normalizes_and_submits`，覆盖 bracketed paste 输入 `ML-A\r\n  ML-B  \n\n\t \n` 后输入区显示多行，提交回显为 `"ML-A\n  ML-B  "`。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-chat --test pty_chat chat_multiline_paste_normalizes_and_submits -- --nocapture`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P4.R Review：P4 阶段复核** — 逐条复核 P4.1–P4.4：确认排队态与流式状态机无死锁/丢消息、Esc 分级语义在各状态下一致、多行粘贴不破坏 undo/历史、取消入口唯一且幂等；确认 PTY 覆盖；跑通全套 CI。
  - 完成记录（2026-07-09）：已逐条复核 P4.1–P4.4。输入排队由 host-controlled `streaming_binding` 与 FIFO `queued_responses` 驱动，流式中提交只入队不触发 `on_submit`，流式结束后通过空 Enter/有草稿 Enter 维持先旧队列、后新草稿的顺序；编辑重发拦截器优先于排队，状态行覆盖 streaming、queued、ready-to-send 三类可见提示。
  - Esc/取消复核：`StreamingCancelController` 是取消按钮、消息列表未消费 Esc、`ChatPanel` 输入 fallback 的共享入口；入口先确认目标仍为最新 streaming 回合，再置 `ChatTurnStatus::Canceled` 并触发 `on_cancel`，连按 Esc 或按钮重复触发均幂等 no-op。completion/slash/mention popup 已在输入层优先消费 Esc，不会透传为流式中断；本次补充 `mention_popup_escape_takes_priority_over_streaming_interrupt` 单测固定 mention popup 的同类优先级。
  - 多行粘贴与覆盖复核：paste 规整会剥离 bracketed paste 包裹、统一 CRLF/CR 为 LF、裁剪尾部空白行，同时保留正文缩进、内部空行和单行尾随空格；插入走 `TextArea::replace_byte_range`，同步 draft binding、内部 buffer、光标与提交历史。当前 `TextArea` 无独立 undo 栈，本阶段未新增会破坏 undo 的状态。PTY 已覆盖流式中排队且不提前提交、Esc 中断并显示 canceled、bracketed multiline paste 规整后提交。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo test -p atto-ui-chat mention_popup_escape_takes_priority_over_streaming_interrupt --lib`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`、`git diff --check` 均通过。

## 阶段 P5 — 会话导航：历史搜索 + Turn 级折叠/引用（C2 + C3）

参考 `AGENT_GAP.md` C2、C3。

- [x] **[DONE] P5.1 会话内搜索** — `src/list.rs`：类 Ctrl+R 搜索/跳转——输入关键词高亮匹配、在命中间上一处/下一处跳转、退出搜索恢复；与虚拟滚动协同（跳转到屏外命中）。
  - 完成记录（2026-07-09）：`ChatMessageList` 新增会话内搜索状态机，`Ctrl+R` 进入搜索并聚焦查询输入；输入字符实时重算命中并高亮当前可见内容；Enter/Down/PageDown/Tab/Ctrl+R 跳到下一处，Up/PageUp/BackTab 跳到上一处，Esc 退出搜索并恢复进入搜索前的滚动位置。
  - 滚动/虚拟化：搜索命中按现有 `ChatRowKey` 顺序从 header、文本、thinking/tool/diff/plan/task/todo/notice/artifact 等行的可搜索文本收集；新增虚拟行滚动调整 `ToRow`，命中在屏外时下一次布局会把目标行滚入视口并暂停 tail follow，不破坏现有自动跟随和加载更多路径。
  - 测试覆盖：新增 list 单测覆盖搜索打开与可见命中高亮、上一处/下一处在屏外命中间跳转、Esc 关闭后恢复原滚动并清除 overlay/高亮。
  - 验证：`cargo test -p atto-ui-chat chat_search --lib`、`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P5.2 Turn 级折叠** — `src/list.rs`：在块级折叠之上支持折叠整个回合（回合 header 折叠控件），折叠态占位与展开还原滚动位置。
  - 完成记录（2026-07-09）：`ChatMessageList` 新增本地 turn 折叠状态，回合 header action row 现在显示 `Collapse` / `Expand` 控件；折叠时虚拟行键只保留该回合 header，隐藏该回合的 block 行和 pending/paired tool result 行，并在 header 中显示 `Collapsed · N blocks hidden` 占位，不修改 store 中既有块级折叠状态。
  - 滚动/虚拟化：折叠状态纳入 header 行键和高度版本，避免复用展开态缓存；折叠/展开后虚拟列表重算 row keys，折叠时将 header 保持可见，展开时恢复折叠前的 scroll offset，且不破坏搜索、自动跟随、加载更多和既有块级 disclosure 行为。
  - 测试覆盖：新增 list 单测覆盖折叠按钮状态切换、折叠 row keys 只保留 header、折叠后隐藏正文并显示占位、展开恢复折叠前滚动位置；同步更新受新增 header 控件高度影响的任务详情单测与 `snapshot_chat_app` / `pty_chat` fixture 视口。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。完整测试首轮曾发现 `chat_tool_call_disclosure_streams_status_and_toggles` 与 `chat_turn_header_renders_meta_and_structured_error` 依赖旧垂直高度；已调整 fixture 高度和 disclosure-aware 等待后 exact 用例及完整套件复跑通过。
- [x] **[DONE] P5.3 引用回复（可选）** — `src/list.rs` + `input.rs`：选中某回合/块作为引用附加到下一条输入；引用在输入区可见、可移除。
  - 完成记录（2026-07-09）：新增 `ChatInputReference` 与 `ChatInputHandle` 引用绑定/API；`ChatInputPanel` 在文本输入框上方显示引用栏，支持点击 `[Remove]` 移除引用；文本提交或排队时将引用合成为标准 Markdown blockquote 前缀并在本次提交消费后清理引用，避免泄漏到下一条输入。
  - 列表交互：`ChatMessageList::with_quote_replies(&ChatInputHandle)` 启用引用回复；turn action row 新增 `Quote`，block action row 新增 `Quote block`，重复引用同一 turn/block 时替换既有引用而非堆叠重复项；引用摘要会压缩空白并截断过长内容。
  - 测试覆盖：新增 input 单测覆盖引用栏渲染、点击移除、提交携带引用并清理；新增 list 单测覆盖 turn/block 引用按钮写入输入引用。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-chat reference_bar_renders_and_remove_click_clears_reference`、`cargo test -p atto-ui-chat quote_message_button_attaches_turn_reference`、`cargo test -p atto-ui-chat quote_block_button_attaches_block_reference`、`cargo test -p atto-ui-chat text_submit_includes_references_and_clears_them`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] P5.4 快照与测试** — PTY 覆盖搜索命中跳转（含屏外）、turn 折叠/展开、引用附加与移除。
  - 完成记录（2026-07-09）：`snapshot_chat_app` 新增 `--p5-search` 与 `--p5-fold-quote` 两个确定性 fixture；前者提供底部初始视图和两个屏外/屏内搜索命中，后者启用 `with_quote_replies` 并提供可折叠、可引用的多 block 回合。
  - 测试覆盖：新增 `chat_p5_search_jumps_between_offscreen_matches`、`chat_p5_turn_fold_collapses_and_expands`、`chat_p5_quote_reply_attaches_and_removes_references` 三个 PTY 用例，分别覆盖 Ctrl+R 搜索跳到屏外首个命中并切换到下个命中、turn Collapse/Expand 隐藏和恢复块内容、turn/block 引用附加后通过 `[Remove]` 移除。
  - 复核修复：PTY 覆盖发现引用栏保存绝对绘制区域，而父布局会把鼠标事件转为本地坐标，导致真实 UI 点击 `[Remove]` 不能移除引用；已在 `ChatInputPanel` 中记录最后绘制区域并将本地鼠标坐标转换回绝对坐标再命中，同时新增本地坐标单测。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-chat reference_remove_click_handles_local_mouse_coordinates --lib`、`cargo test -p atto-ui-chat --test pty_chat chat_p5 -- --nocapture`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets` 均通过。
- [ ] **P5.R Review：P5 阶段复核** — 逐条复核 P5.1–P5.4：确认搜索跳转与自动跟随/虚拟化不冲突、turn 折叠不破坏块级折叠状态、引用附加的生命周期清晰、宽字符高亮不错位；确认 PTY 覆盖；跑通全套 CI。

## 阶段 P6 — 细节层：工具权限层级 + 上下文压缩块（D1 + D2）

参考 `AGENT_GAP.md` D1、D2。涉及模型变更，需同步运行时/JS 侧。

- [ ] **P6.1 工具权限层级模型** — `src/message.rs` + `src/store.rs`：`ApprovalRequest`/`ApprovalOption` 扩展支持 allow-once / always / 项目级等层级语义；决策回调携带层级；`resolve_approval` 相应扩展；补单测。
- [ ] **P6.2 权限层级渲染** — `src/list.rs`：审批区渲染分层选项（一次允许/始终允许/项目级/拒绝等），选择后状态锁定并显示已选层级。
- [ ] **P6.3 上下文压缩块** — `src/message.rs` + `src/list.rs`：新增专门的 compact 块类型（或扩展 `Notice`），展示压缩进度/前后 token/摘要，视觉区别于普通通知。
- [ ] **P6.4 运行时/JS 侧同步** — `src/dynamic.rs`：模型变更同步序列化 + schema（保留旧形兼容）；同步 `crates/atto-ui-node`、`packages/core`、`packages/react` 类型/构造器，更新 `docs/NODE_API.md`；跑 smoke 与 runtime 兼容测试。
- [ ] **P6.5 快照与测试** — `snapshot_chat_app` 增加多层级审批与压缩块场景；PTY 覆盖层级审批选择/锁定、压缩块渲染。
- [ ] **P6.R Review：P6 阶段复核** — 逐条复核 P6.1–P6.5：确认权限层级语义与回调契约一致、旧形序列化仍可解析、压缩块与 Notice 区分清晰、JS 侧类型同步无遗漏；确认 PTY + smoke 全过；跑通全套 CI。做一次整体收尾比对，确认 `AGENT_GAP.md` 中除 B2（图片）外的缺口均已落地。
