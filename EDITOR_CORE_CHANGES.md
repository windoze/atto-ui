# editor-core / editor-core-lsp 需要的改动

本文件描述 **atto-ui-editor 需要、但应由 `editor-core-lsp` 上游提供** 的改动。
目标读者是 editor-core 侧的维护者；atto 侧会在上游落地后按「消费方式」一节接入。

当前版本基线：`editor-core = 0.4.2`、`editor-core-lsp = 0.4.2`。

> **状态：改动 A 已实施（2026-07-25，`main` 分支，尚未发版）。**
> 落地方式与最终行号见下方各节的「✅ 已实施」标注。行号基于实施后的 `main` 源码，
> 与 0.4.2 crates.io 源码略有偏移。改动 B 未做（非必需，见对应小节）。

---

## 背景：为什么需要上游改

atto 的单文档编辑视图（`EditorView`，基于 `editor_core::EditorStateManager` + `editor_core_lsp::LspSession`）
在每次编辑后要给 LSP 发 `textDocument/didChange`。目前 atto 侧是**手工推断增量范围**的，例如退格：

```rust
// atto-ui-editor/src/view/editing.rs（backspace 路径，简化）
let offset = self.cursor_offset();
let change = lsp.content_change_for_offsets(line_index, offset - 1, offset, ""); // 假设删了 1 个字符
```

这里硬编码了「退格删除 1 个字符」。但 `editor-core` 在开启自动配对
（`AutoPairsConfig { enabled: true, delete_pair: true, .. }`，而 `delete_pair` **默认就是 `true`**）时，
若光标位于一对配对符之间（如 `(|)`），`EditCommand::Backspace` / `DeleteForward` 会**一次删除两个字符**。
此时 atto 告诉服务器只删了 1 个字符，服务端镜像与真实缓冲区就此错位，且版本号仍在同步递增，
导致后续所有诊断 / 语义高亮 / 代码动作的位置整体漂移，且没有任何断言能发现。

> 这不是理论问题：`atto-editor-app` 对所有非 `plaintext` 语言启用了自动配对
> （`crates/atto-editor-app/src/language.rs:91`），只要配置了 LSP 就会触发。

**根因**：atto 侧在"猜"编辑产生了什么变更，而不是使用编辑**实际**产生的变更。
`editor-core` 其实已经记录了真实变更（`EditorStateManager::take_last_text_delta()` → `TextDelta`），
`editor-core-lsp` 也已经有了把 `TextDelta` 转成 LSP `didChange` 的完整逻辑——
**但那套逻辑只在多文档的 `LspWorkspaceSync` 里，单文档的 `LspSession` 上没有对称暴露。**

---

## 现状梳理（上游已有 / 缺失）

已有、可复用：

- `editor_core::EditorStateManager::take_last_text_delta() -> Option<Arc<TextDelta>>`
  （`editor-core-0.4.2/src/state.rs:1302`）——返回上次编辑真实产生的结构化变更。
- `editor_core::TextDelta` / `TextDeltaEdit`（`editor-core-0.4.2/src/delta.rs`）——含精确的
  `deleted_text` / `inserted_text` 与 char 偏移；配对删除自然表现为一条 `deleted_len() == 2` 的 edit。
- `editor_core_lsp::DeltaCalculator`（`lsp_sync.rs:150`，公开）+ `TextChange`（`lsp_sync.rs:77`，
  已从 `lib.rs:78` 导出）——维护一份镜像文本，把 char-offset 变更正确转成 LSP `range` 变更。
- 多文档路径 `LspWorkspaceSync::did_change_from_text_delta(workspace, buffer_id)`
  （`workspace_sync.rs:273`）——已经把「取 delta → 转 change → 发 didChange」串起来了。

缺失（**本请求要补的**，以下为 0.4.2 时的现状；均已在改动 A 中补齐）：

1. ~~单文档 `LspSession`（`editor.rs:248`）**不持有** `DeltaCalculator`，也没有 delta 版 `did_change`。
   它只有手工范围版的 `did_change(LspContentChange)` / `did_change_many(Vec<LspContentChange>)`
   （`editor.rs:729` / `734`）。~~ → 已补：见改动 A.1 / A.2。
2. ~~delta → `Vec<TextChange>` 的转换函数 `text_changes_for_text_delta` /
   `position_for_char_offset`（`workspace_sync.rs:473` / `456`）是**私有**的，host 复用不了。~~
   → 已提升为 `lsp_sync.rs` 的 `pub(crate)`，见改动 A.3。

---

## 请求的改动

### 改动 A（主，必需）：给 `LspSession` 增加 delta 版 `did_change` — ✅ 已实施

让单文档路径与 workspace 路径对称：`LspSession` 内部维护一个 `DeltaCalculator`，
并暴露一个直接吃 `TextDelta` 的方法。atto 侧就只需「取 delta → 交给 session」，不再猜范围。

> **实施摘要**（`crates/editor-core-lsp/`）：
> - A.1：`LspSession` 新增私有字段 `change_calculator: DeltaCalculator`（`editor.rs:257`），
>   在 `start()` 中用 `initial_text` 初始化。手工路径 `did_change_many`（`editor.rs:747`）在
>   发送成功后推进镜像。
> - A.2：新增公开方法 `LspSession::did_change_from_text_delta(&TextDelta)`（`editor.rs:816`）。
>   另附赠只读访问器 `LspSession::mirror_char_count()`（`editor.rs:850`）供 host 做一致性自检。
> - A.3：`position_for_char_offset` / `text_changes_for_text_delta` 已提升到 `lsp_sync.rs`
>   （`:333` / `:359`）并改为 `pub(crate)`，两条路径共用。
> - 配套：`DeltaCalculator::char_count()`（`lsp_sync.rs:185`）用于镜像一致性断言。

**A.1 — `LspSession` 持有并维护 `DeltaCalculator`** — ✅ 已实施

- 在 `LspSession`（`editor.rs:249`）中新增字段 `change_calculator: DeltaCalculator`（`editor.rs:257`）。
- 在 session 启动时用 `LspSessionStartOptions.initial_text` 初始化：
  `DeltaCalculator::from_text(&initial_text)`。
- 手工范围版 `did_change_many`（`editor.rs:747`）在成功发送后，把这些变更
  `apply_change` 进 calculator，保证两条写入路径下镜像始终与服务端一致（采用了双写方案）。
- 发送逻辑抽出私有 `send_active_did_change`（`editor.rs:767`，不碰镜像），
  `did_change_many` 与 `did_change_from_text_delta` 都走它，确保「镜像每次调用只前进一次」。

**A.2 — 新增公开方法** — ✅ 已实施（`editor.rs:816`）

最终实现与下方草案一致，签名相同。差异仅在：`send_active_did_change` 承担纯发送，
`did_change_from_text_delta` 只通过 `text_changes_for_text_delta` 推进镜像一次，不再二次 apply。

```rust
impl LspSession {
    /// 用一次编辑真实产生的 `TextDelta` 发送 `textDocument/didChange`。
    ///
    /// 相比 `did_change_many` 需要调用方自己推断增量范围，本方法直接消费
    /// `EditorStateManager::take_last_text_delta()` 的结果，因此对多字符删除
    /// （如自动配对的成对删除）、多光标、缩进等所有编辑都天然正确。
    ///
    /// `delta` 的 `before_char_count` 必须与 session 内部镜像的当前字符数一致
    /// （即：调用方每次编辑后都取 delta 并送入，不跳变更）。
    pub fn did_change_from_text_delta(&mut self, delta: &TextDelta) -> Result<(), String> {
        if delta.is_empty() {
            return Ok(());
        }
        // 复用 A.3 提取出的共享转换逻辑：
        let changes = text_changes_for_text_delta(&mut self.change_calculator, delta);
        let content_changes = changes
            .into_iter()
            .map(|c| LspContentChange { range: c.range, text: c.text })
            .collect::<Vec<_>>();
        self.did_change_many(content_changes)
        // 注意：若 A.1 让 did_change_many 也 apply 进 calculator，
        // 这里要避免二次 apply（text_changes_for_text_delta 内部已 apply 过）。
        // 实现时二者取其一即可，关键是 calculator 最终恰好前进一次。
    }
}
```

> 关于镜像一致性（实施结果）：`text_changes_for_text_delta` 在生成每条 `TextChange` 时会
> `apply_change` 推进 calculator。因此 `did_change_from_text_delta` 走这条路时**不**再重复 apply，
> 而是调用不碰镜像的 `send_active_did_change` 发送；手工路径 `did_change_many` 则在发送成功后
> 单独 apply。两条路径「calculator 每次调用只前进一次」。已加断言
> `debug_assert_eq!(delta.before_char_count, self.change_calculator.char_count())`。
>
> **一个已知的顺序不对称**（有意保留）：`did_change_from_text_delta` 是「先推进镜像、后发送」，
> 而 `did_change_many` 是「先发送成功、后推进」。前者若发送失败，镜像会领先服务端一步。但
> `send_active_did_change` 失败的唯一来源是 `client.notify` 的 io error（server 管道已死），
> 此时会话即进入禁用终态、不再有后续 `didChange`，故该漂移无实际后果。为避免每次编辑 clone
> 整个镜像做回滚，保留此顺序并在代码注释中说明。

**A.3 — 把 delta→change 的私有转换提升为 crate 内共享** — ✅ 已实施

`text_changes_for_text_delta` 与其依赖 `position_for_char_offset` 已从 `workspace_sync.rs`
移到 `lsp_sync.rs`（`:359` / `:333`），改为 `pub(crate)`，由 `LspWorkspaceSync` 与
`LspSession` 共用，未对外 `pub`。

---

### 改动 B（可选增强，非阻塞）— ⏭️ 未实施

以下不是必须，本次未做。既然改动 A 已落地，B.2 的退路不再需要；B.1 留待后续按需评估。

- **B.1** — 让 `EditorStateManager::execute()`（`state.rs:565`）在返回的 `CommandResult`
  里附带本次产生的 `TextDelta`（或提供 `execute_with_delta`）。当前必须「先 `execute` 再单独
  `take_last_text_delta()`」两步且靠约定，容易漏取或错序。可选，**未实施**。
- **B.2** — 若不做改动 A 的退路。**已被改动 A 取代，不再需要。**

---

## 验收测试（放在 editor-core-lsp）— ✅ 已实施

针对改动 A 的 4 个建议场景全部覆盖，另加集成测试。全部通过。

单元测试（`crates/editor-core-lsp/src/lsp_sync.rs`，用真实 `EditorStateManager` 产出的 delta
驱动共享转换，断言重建镜像 == 编辑器缓冲区）：

1. **配对删除** `test_delta_pair_deletion_removes_two_chars`：`"()"` 光标居中执行 `Backspace`
   （auto-pairs 开启）→ 镜像变为 `""`。
2. **多光标** `test_delta_multi_cursor_insert_keeps_mirror_in_sync`：一次编辑多条 `TextDeltaEdit`，
   镜像与缓冲区一致。
3. **CRLF / 多字节** `test_delta_crlf_and_multibyte_ranges`：含 `\r\n` + CJK + emoji，断言 LSP
   `range`（UTF-16 列）正确。
4. **连续编辑不漂移** `test_delta_consecutive_edits_never_drift`：连续多次「编辑→取 delta→送入」，
   镜像字符数与缓冲区始终相等。

外加 `test_delta_calculator_char_count` 校验 `char_count()` 语义。

集成测试（`crates/editor-core-lsp/tests/did_change_from_text_delta.rs`，用 shell mock server
端到端跑公开方法）：

- `did_change_from_text_delta_handles_auto_pair_deletion_end_to_end`：配对删除后文档版本递增一次、
  `mirror_char_count()` 与缓冲区一致。
- `did_change_from_text_delta_is_noop_for_empty_delta`：空 delta 不递增版本、不动镜像。

---

## atto 侧的消费方式 — ✅ 已接入（editor-core 0.4.3）

> **状态：atto 侧已接入并全绿，依赖已升级到 crates.io 正式版 `editor-core-* = 0.4.3`**
> （改动 A 已随 0.4.3 发版）。本地 `[patch.crates-io]` 验证段已删除。
>
> 落地内容：
> - 新增统一 helper `EditorView::execute_edit_and_sync_delta`（`crates/atto-ui-editor/src/view/state.rs`）：
>   `execute` → `take_last_text_delta()` → `LspSession::did_change_from_text_delta`，用 delta 是否
>   非空判定 changed，保证镜像每次编辑恰好前进一次。
> - 新增 `lsp_did_change_from_delta`（`view/lsp.rs`），与旧 `lsp_did_change` 共用错误处理。
> - **单次 execute 的编辑路径全部迁移**：backspace / delete_forward / delete_selection /
>   insert_text / indent_or_tab / execute_full_document_edit_and_sync（`view/editing.rs`）、
>   ReplaceCurrent / ReplaceAll（`view/search.rs`）、Undo / Redo（`view/actions.rs`）。所有手写的
>   `content_change_for_offsets(offset-1, offset, "")` 分支已删除。
> - **保留全文档替换的路径**（有意为之）：格式化 `ApplyTextEdits`、code-action `apply_workspace_edit`、
>   补全 `textEdit`（均在 `view/lsp.rs`）。这些在 editor-core-lsp 内部按每条 edit **多次 execute**，
>   `last_text_delta` 只保留最后一条子编辑，无法代表整批；全文档替换是精确的，且经 `did_change_many`
>   同样会推进上游镜像，与 delta 路径混用保持一致。已在代码中注释说明。
> - 回归测试 `lsp_auto_pair_deletion_keeps_didchange_in_sync`（`tests/lsp_editor.rs`）：auto-pairs +
>   LSP 下 `(` → `()` → Backspace 删两字符 → 再编辑 / undo / redo，依赖上游 debug 断言校验镜像不漂移。
>
> 接入形态示例：

```rust
// atto-ui-editor/src/view/editing.rs 等所有编辑路径，统一收敛为：
let executed = self.execute(Command::Edit(EditCommand::Backspace));
if executed {
    self.config.text.set(self.state_manager.editor().get_text());
    self.maybe_apply_syntax_highlighting();
    self.hide_popups();
    if let Some(lsp) = self.lsp.session.as_mut() {
        if let Some(delta) = self.state_manager.take_last_text_delta() {
            let _ = lsp.did_change_from_text_delta(&delta);
        }
    }
}
```

这样可以删除 `editing.rs` 中所有手写的 `content_change_for_offsets(offset-1, offset, "")` /
`full_document_change` 分支（约 8 处），并从根上消除「猜删了几个字符」这一整类 bug。

---

## 与本次不相关的说明（上游无需改动）

以下几项在 review 中被提到，但**不需要上游改**，仅记录以免混淆：

- LSP 会话优雅关闭：0.4.2 已提供 `LspSession::shutdown()/exit()/cancel_request()`
  （`editor.rs:1000-1015`），是 atto 侧「何时调」的生命周期问题。
- 搜索正则重复编译：0.4.2 已提供 `editor_core::search::CompiledSearch`，atto 侧换用即可。
- workspace-edit 版本冲突：atto 已在用 0.4.2 的 `workspace_edit_expected_versions`。
