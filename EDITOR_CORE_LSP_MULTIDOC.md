# editor-core-lsp 需求：多文档共享 session 的 per-URI 交互请求

本文件描述 **atto-ui 需要、但应由 `editor-core-lsp` 上游提供** 的改动，用于把 `atto-editor-app`
的两层 LSP 合并为「每 `(workspace_root, language)` 一个共享 server」。目标读者是 `editor-core`
侧维护者。性质与 [`EDITOR_CORE_CHANGES.md`](./EDITOR_CORE_CHANGES.md) 的「改动 A」相同：上游落地后
atto 侧再接入。

版本基线：`editor-core-lsp = 0.4.3`。文件/行号引用基于 crates.io 0.4.3 源码。

---

## 背景：atto 当前是「两层 LSP」，每文件双开 server

`atto-editor-app` 打开一个文件会启动**两个** LSP server 进程：

- **Layer 1（per-view）**：每个 `EditorView`（crate `atto-ui-editor`，`src/view/`）自持一个
  `LspSession`，负责全部**交互特性 + 诊断**——hover、补全、签名帮助、inlay hints、语义高亮、
  折叠、goto(definition/declaration/typeDefinition/implementation/references)、code action、
  formatting、document symbols、prepare-rename/rename、诊断。
- **Layer 2（workspace bridge）**：`LspWorkspaceBridge`（`atto-editor-app/src/lsp_workspace.rs`）
  按 `(root, language)` 复用一个 `LspWorkspaceSync`，负责 **workspace symbols** 与跨文件
  **`workspace/applyEdit`**（rename fan-out）。它**故意丢弃诊断**（`notification_message` 对
  `PublishDiagnostics` 返回 `None`），以免与 Layer 1 冲突。

两层进程独立、互不共享 session。像 rust-analyzer 这类重 server，因此每个文件被双开，内存/CPU
成本翻倍。理想是合并为「每 `(root, language)` 一个共享 server，服务其下所有打开的文档」。

---

## 为什么现在合并不了：`LspSession` 的交互特性是单-active-document 的

合并的前提是「一个共享 `LspSession` 能同时服务 N 个打开文档的交互特性」。但 0.4.3 的 `LspSession`
做不到——它对交互特性写死了「唯一 active document」，证据（均在 `editor-core-lsp-0.4.3/src/`）：

### 1. 所有交互请求都写死 `self.document.uri`，无 per-URI 变体

`editor.rs:1146-1631`。位置类请求经私有 helper `text_document_position_params`（`:1146-1157`，
`"uri": self.document.uri.as_str()`）/ `text_document_range_params`（`:1159-1164`）构造：
hover `:1167`、definition `:1180`、declaration `:1193`、type_definition `:1206`、
implementation `:1219`、references `:1232`、document_highlight `:1250`、
prepare_call_hierarchy `:1263`、prepare_type_hierarchy `:1286`、completion `:1309`、
signature_help `:1327`、inlay_hints `:1340`、prepare_rename `:1392`、rename `:1377`。
文档类请求直接内联 `self.document.uri.as_str()`：document_symbols `:1362`、formatting `:1462`、
on_type_formatting `:1497`、semantic_tokens_delta `:1510`、semantic_tokens_range `:1524`、
document_links `:1572`、document_diagnostic `:1586`、document_color `:1610`、code_lens `:1446`。

**这些方法都没有 `uri` 参数**——调用方无法指定「对哪个文档」发请求。唯一的 per-URI 接口是
**非交互**的 `did_change_for_uri(_many)`（`:942`/`:951`）与 `did_save_for_uri`（`:1000`）。

### 2. 切换 active document 有破坏性副作用

`set_active_document`（`editor.rs:888-903`）会调用 `drop_pending_for_inactive_document`
（`:909-912`，`pending.retain(|_, req| req.uri() == active)`）并 `clear_semantic_tokens_cache`。
`handle_pending_response`（`:2063-2098`）拒绝 `uri != self.document.uri` 的语义高亮/折叠响应。
回归测试 `set_active_document_drops_pending_for_previous_document`（`:2477`）、
`close_document_drops_its_pending_requests`（`:2511`）表明这是**有意**行为（各文档版本号可能碰撞，
故丢弃跨文档迟到响应）。因此即便用「切 active → 发请求」的模式绕，也会丢掉其它文档在途的请求，
语义高亮/折叠结构性地单-active-doc。

### 3. 响应不带 URI

`LspEvent::Response { id, method, result, error }`（`editor.rs:1761-1788`）只带 `id` + `method`。
request id 是 `LspClient` 上的全局单调计数器（`lsp_client.rs:108-110`），全局唯一、可多路复用，
但响应**不含来源 uri**，消费方须自行维护 id→document 映射。

### 4. 多文档目前只支持非交互路径

`LspWorkspaceSync`（`workspace_sync.rs`）为多文档提供的只有三样：per-URI `didChange`
（`did_change_from_text_delta` → `did_change_for_uri_many`）、`publishDiagnostics` 按 uri 路由
（`poll_workspace` `:219-239`，靠 `workspace.buffer_id_for_uri` + `diagnostics_version_matches`）、
`workspace/applyEdit` 跨文件应用。模块文档（`workspace_sync.rs:1-9`）也只列这三项，**交互特性未提及**。

---

## 请求上游加的能力（二选一）

### 方案 1（首选）：per-URI 交互请求 API + 响应带 uri

为每个交互请求增加 `*_for_uri` 变体，不依赖单一 active document：

```
request_hover_for_uri(uri, line_index, line, col) -> Result<u64, String>
request_completion_for_uri(uri, ...) -> ...
request_signature_help_for_uri(uri, ...) -> ...
request_definition_for_uri / _declaration_ / _type_definition_ / _implementation_ / _references_
request_document_highlight_for_uri
request_inlay_hints_for_uri
request_code_action_for_uri
request_formatting_for_uri / on_type_formatting_for_uri
request_document_symbols_for_uri
request_semantic_tokens_full_for_uri / _range_ / _delta_
request_folding_ranges_for_uri
request_prepare_rename_for_uri / request_rename_for_uri
request_document_diagnostic_for_uri / code_lens / document_links / document_color ...
```

配套：
- `LspEvent::Response` 携带来源 `uri`（或提供 `id -> uri` 查询接口），使消费方能按 `(uri, id)`
  路由响应到正确的文档/视图。
- 上述 `_for_uri` 请求应基于 `extra_documents`/`document_for_uri`（`editor.rs:552/557`）里已跟踪的
  文档状态，**不触发** active-doc 切换的副作用（不丢别的文档的 pending、不清缓存）。
- 保留现有无 uri 的方法（针对 active document）作为向后兼容/单文档便捷入口。

### 方案 2（次选）：无副作用的 active-doc 切换

若不做全套 per-URI，退而让 `set_active_document` 可选地**不**丢弃其它文档 pending、不清缓存，
使「切 active → 发请求 → 收响应」模式在多文档下可靠。仍是串行 active-doc，吞吐受限，且响应仍需
消费方自己对应 uri，故不如方案 1 干净。

---

## 验收测试建议（放在 editor-core-lsp）

- 一个 session 打开 2 个文档，交替对**各自 uri** 发 hover/completion/semantic-tokens 请求，
  断言响应能按 uri 正确归属、互不丢弃。
- 语义高亮/折叠：对文档 A 发起后立即对文档 B 发起，断言 A 的响应不被 B 的请求丢弃。
- 诊断：多文档下 `publishDiagnostics` 按 uri 路由（已支持，作为回归保护）。

---

## atto 侧后续（上游支持后再做，本仓不含实现）

- `EditorView`（`atto-ui-editor`）不再独占 `LspSession`；改为持有对共享 session（按
  `(root, language)`）的句柄，按自身 `uri` 发 `*_for_uri` 请求，响应按 `(uri, id)` 路由到本 view 的
  `pending_*`（`view/mod.rs:172-217`、`view/lsp.rs:89-335`）。
- 诊断沿用 bridge 的 per-URI 路由（`publishDiagnostics` 自带 uri）。
- **必须保留** 3 个 standalone 消费者的可用性——它们直接构造带 LSP 的 `EditorView` 且无 bridge：
  - `crates/atto-ui-editor/examples/editor.rs`
  - `crates/atto-ui-editor/src/bin/snapshot_editor_app.rs`
  - `crates/atto-ui-editor/tests/lsp_editor.rs`（~25 用例）

  即合并方案须保留「无 bridge 时 `EditorView` 仍能自起 session」的退化路径。

---

## 已在 atto 侧完成的相关改动（无需上游）

- **共享 server 会话期回收**：`LspWorkspaceBridge::close_document` 在某 `(root, language)` 的最后
  一个文档关闭时移除并 `session_mut().exit()` 关停共享 server
  （`atto-editor-app/src/lsp_workspace.rs` `retire_key_if_unused`）。这只回收 Layer 2 的一个 server，
  两层合并落地后 Layer 1 的重复 server 才会消除。
