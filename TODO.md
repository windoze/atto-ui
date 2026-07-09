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
- [ ] **P3.2 编辑 user 消息** — `src/list.rs`：支持进入某条 user 消息编辑态，编辑后从该点截断并触发重发回调（`on_edit_and_resubmit`）；与输入区衔接（把原文回填输入）。
- [ ] **P3.3 retry / regenerate** — `src/list.rs`：assistant 回合支持重生成（截断该回合后触发回调），与现有 `on_message_action` 的 Retry/Regenerate 打通。
- [ ] **P3.4 快照与测试** — `snapshot_chat_app` 增加编辑/重发场景；PTY 覆盖编辑 user 后截断、retry 后回合截断、fork 后旧消息不再显示。
- [ ] **P3.R Review：P3 阶段复核** — 逐条复核 P3.1–P3.4：确认截断不泄漏悬挂 block_id/版本、fork 后滚动与自动跟随正常、流式进行中编辑/重发的竞态被正确处理、回调契约清晰；确认 PTY 覆盖边界；跑通全套 CI。

## 阶段 P4 — 输入交互增强：排队 & Esc 中断 + 多行编辑（A3 + A4）

参考 `AGENT_GAP.md` A3、A4。建立在 P2 输入层之上，衔接 P3 的中断语义。

- [ ] **P4.1 输入排队** — `input.rs`：流式进行中允许继续输入并排队新消息；流式结束后自动出队或提示用户发送；排队态有可见指示。
- [ ] **P4.2 Esc 中断语义** — `src/list.rs` + `input.rs`：完善 Esc 状态机——一次 Esc 中断当前流式（置 `ChatTurnStatus::Canceled`），分级/连按语义明确，与现有取消按钮统一入口。
- [ ] **P4.3 多行编辑增强** — `input.rs`：多行粘贴规整；（可选）拖入/粘贴文件路径转 `Attachment` block。
- [ ] **P4.4 快照与测试** — PTY 覆盖流式中排队新消息、Esc 中断置 `Canceled`、多行粘贴规整。
- [ ] **P4.R Review：P4 阶段复核** — 逐条复核 P4.1–P4.4：确认排队态与流式状态机无死锁/丢消息、Esc 分级语义在各状态下一致、多行粘贴不破坏 undo/历史、取消入口唯一且幂等；确认 PTY 覆盖；跑通全套 CI。

## 阶段 P5 — 会话导航：历史搜索 + Turn 级折叠/引用（C2 + C3）

参考 `AGENT_GAP.md` C2、C3。

- [ ] **P5.1 会话内搜索** — `src/list.rs`：类 Ctrl+R 搜索/跳转——输入关键词高亮匹配、在命中间上一处/下一处跳转、退出搜索恢复；与虚拟滚动协同（跳转到屏外命中）。
- [ ] **P5.2 Turn 级折叠** — `src/list.rs`：在块级折叠之上支持折叠整个回合（回合 header 折叠控件），折叠态占位与展开还原滚动位置。
- [ ] **P5.3 引用回复（可选）** — `src/list.rs` + `input.rs`：选中某回合/块作为引用附加到下一条输入；引用在输入区可见、可移除。
- [ ] **P5.4 快照与测试** — PTY 覆盖搜索命中跳转（含屏外）、turn 折叠/展开、引用附加与移除。
- [ ] **P5.R Review：P5 阶段复核** — 逐条复核 P5.1–P5.4：确认搜索跳转与自动跟随/虚拟化不冲突、turn 折叠不破坏块级折叠状态、引用附加的生命周期清晰、宽字符高亮不错位；确认 PTY 覆盖；跑通全套 CI。

## 阶段 P6 — 细节层：工具权限层级 + 上下文压缩块（D1 + D2）

参考 `AGENT_GAP.md` D1、D2。涉及模型变更，需同步运行时/JS 侧。

- [ ] **P6.1 工具权限层级模型** — `src/message.rs` + `src/store.rs`：`ApprovalRequest`/`ApprovalOption` 扩展支持 allow-once / always / 项目级等层级语义；决策回调携带层级；`resolve_approval` 相应扩展；补单测。
- [ ] **P6.2 权限层级渲染** — `src/list.rs`：审批区渲染分层选项（一次允许/始终允许/项目级/拒绝等），选择后状态锁定并显示已选层级。
- [ ] **P6.3 上下文压缩块** — `src/message.rs` + `src/list.rs`：新增专门的 compact 块类型（或扩展 `Notice`），展示压缩进度/前后 token/摘要，视觉区别于普通通知。
- [ ] **P6.4 运行时/JS 侧同步** — `src/dynamic.rs`：模型变更同步序列化 + schema（保留旧形兼容）；同步 `crates/atto-ui-node`、`packages/core`、`packages/react` 类型/构造器，更新 `docs/NODE_API.md`；跑 smoke 与 runtime 兼容测试。
- [ ] **P6.5 快照与测试** — `snapshot_chat_app` 增加多层级审批与压缩块场景；PTY 覆盖层级审批选择/锁定、压缩块渲染。
- [ ] **P6.R Review：P6 阶段复核** — 逐条复核 P6.1–P6.5：确认权限层级语义与回调契约一致、旧形序列化仍可解析、压缩块与 Notice 区分清晰、JS 侧类型同步无遗漏；确认 PTY + smoke 全过；跑通全套 CI。做一次整体收尾比对，确认 `AGENT_GAP.md` 中除 B2（图片）外的缺口均已落地。
