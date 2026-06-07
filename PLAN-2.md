# atto-editor-app 全功能编辑器详细设计

本文把 `EDITOR_APP.md` 的方向拆成可实现设计。目标是让实现者按文件和 API 落地，不需要反复全库搜索。

## 0. 设计边界与当前代码地图

### 0.1 总原则

- **交互模型保持 CUA 非模态**：不引入 Normal/Insert/Select 模态状态机；高级功能通过菜单、命令面板、多键序列暴露。
- **通用能力下沉到 `atto-ui`**：拖拽、docking、多键序列、menu/statusbar 外观都属于框架能力，不写成 editor 专用逻辑。
- **editor 侧先接 LSP / 智能能力**：`editor-core-lsp` 已有完整请求和解析工具，`atto-ui-editor` 目前只接 hover / completion / goto 子集。
- **L1/L2 可先保持单文档 LSP**：诊断、code action 可以先在当前 `EditorView` 内部 `LspSession` 上完成。Rename / workspace symbol / workspace edit 跨文件前，先引入 app 层共享 workspace / LSP 管理。

### 0.2 atto-ui 关键文件

| 区域 | 文件 | 当前关键类型 / 函数 | 设计落点 |
|---|---|---|---|
| Component 事件 | `src/composable/component.rs` | `ComponentContext`, `EventResult`, `EventOutcome`, `ComponentAction`, `Component` | C1 扩展通用 drag/drop 上下文与动作 |
| WM 状态 | `src/wm/window.rs` | `Window`, `WindowId`, `WindowKind`, `WindowState`, `movable`, `resizable`, `rect` | C2 新增 dock 状态 |
| WM 内部状态 | `src/wm/manager/types.rs` | `WindowManager`, `DragState`, `DragKind`, `HitRegion` | C1/C2 增加全局拖拽和 dock resize |
| WM 事件 | `src/wm/manager/events.rs` | `handle_event`, `handle_mouse`, `dispatch_to_window_view`, `window_at` | C1 drag 会话分发；C2 dock hit-test / resize |
| WM 绘制 | `src/wm/manager/draw.rs` | `WindowManager::draw` | C1 drag ghost / drop indicator；C2 dock handle / auto-hide |
| WM 布局 | `src/wm/manager/placement.rs` | `normalize_rect`, resize/move helpers | C2 work area reserve 与 dock rect 计算 |
| Desktop | `src/app/desktop.rs` | `Desktop::layout`, `Desktop::add_window`, `send_event_to_window` | C2 work area 应感知 dock reserve |
| Menu | `src/app/menu.rs`, `src/app/menu/{model,input,draw,layout}.rs` | `MenuBar`, `MenuItem`, `MenuSpec`, `handle_event`, `draw` | C4 Turbo Vision 风格与热键 |
| StatusBar | `src/app/status.rs` | `StatusBar { left, right }`, `set_left`, `set_right`, `draw` | C4 分段式 statusbar |
| Theme | `src/theme/mod.rs` | `Theme` typed fields + named styles | C4 新增 menu/statusbar style token |
| Fuzzy | `src/fuzzy.rs` | `fuzzy_match`, `fuzzy_filter` | P2 pickers / command palette 优先复用 |

### 0.3 atto-editor-app 关键文件

| 文件 | 当前职责 | 设计落点 |
|---|---|---|
| `crates/atto-editor-app/src/app.rs` | `run`, `AppState`, menu 构建、动作分发、Explorer 手算 dock rect、打开/保存文件 | 改为消费 C2 dock；接命令面板和 pickers；后续引入 workspace state |
| `crates/atto-editor-app/src/actions.rs` | `AppAction`, `OpenTarget` | 扩展命令面板、picker、LSP workspace 动作 |
| `crates/atto-editor-app/src/window.rs` | `EditorWindowCommand`, `EditorWindowView` | 扩展 editor 命令，传递 app/workspace action |
| `crates/atto-editor-app/src/window/tabs.rs` | 文件 tab 状态、open/save/close、dirty title | 后续迁移到 `editor_core::Workspace` 或保留 Binding bridge |
| `crates/atto-editor-app/src/window/document_tab.rs` | split-capable tab，内部持有 primary/secondary `EditorView` | LSP 跨文件前要避免每个 split 启动重复 LSP |
| `crates/atto-editor-app/src/explorer_window.rs` | `ExplorerWindowView`, `ExplorerWindowCommand`, FileTree 封装 | F-FT 消费 C1 drag/drop、context menu、inline rename |
| `crates/atto-editor-app/src/workspace.rs` | `WorkspaceTree`, `build_workspace_tree` | F-FT 文件树数据源；后续补 FS 监听 / git status |
| `crates/atto-editor-app/src/language.rs` | `guess_language_id`, `syntax_config_for_file`, `lsp_mode_for_file` | LSP config / workspace folders / formatting options |

### 0.4 atto-ui-editor 关键文件

| 文件 | 当前职责 | 设计落点 |
|---|---|---|
| `crates/atto-ui-editor/src/config.rs` | `EditorConfig`, `EditorLspConfig`, `EditorLspMode` | 加 diagnostics/code action/rename/signature/inlay/format config |
| `crates/atto-ui-editor/src/keymap.rs` | `KeyChord`, `EditorAction`, `EditorKeymap` 单 chord 查表 | C3 迁移到通用 keymap；阶段三先扩 `EditorAction` |
| `crates/atto-ui-editor/src/view/mod.rs` | `EditorView`, `EditorViewHandle`, `EditorEvent`, `EditorLspController` | LSP pending state、事件输出、popup bindings |
| `crates/atto-ui-editor/src/view/input.rs` | `handle_key_event` | 接新 keymap / 多键序列结果；触发 LSP/编辑动作 |
| `crates/atto-ui-editor/src/view/actions.rs` | `handle_action(EditorAction)` | 新动作 dispatch 到 `editor-core` / LSP |
| `crates/atto-ui-editor/src/view/lsp.rs` | `start_lsp_session`, `maybe_poll_lsp`, request/response handling | L1-L6 主落点 |
| `crates/atto-ui-editor/src/view/render.rs` | gutter/text/popup 渲染 | 诊断 gutter、signature/code-action popup、inlay/code lens virtual text |
| `crates/atto-ui-editor/src/view/state.rs` | 状态同步辅助 | 新增 diagnostics 状态查询 helper |
| `crates/atto-ui-editor/src/popup.rs` | hover/completion popup models/views | 新增 code action / signature / rename popup model |
| `crates/atto-ui-editor/src/theme.rs` | `EditorTheme`, style id 映射 | 新增诊断、inlay hint、code lens 等样式 |
| `crates/atto-ui-editor/src/lib.rs` | public re-export | 新类型导出 |

### 0.5 editor-core / editor-core-lsp 可直接复用 API

源码位置：`../editor-core`。

| 能力 | 文件 | API |
|---|---|---|
| 单文档状态 | `crates/editor-core/src/state.rs`, `commands.rs` | `EditorStateManager`, `execute(Command)`, `apply_processing_edits`, `apply_processor` |
| Workspace | `crates/editor-core/src/workspace.rs` | `Workspace::new`, `open_buffer`, `create_view`, `set_active_view`, `buffer_id_for_uri`, `buffer_text`, `buffer_text_for_saving`, `apply_text_edits`, `apply_processing_edits`, `take_last_text_delta_for_buffer` |
| 编辑原语 | `crates/editor-core/src/model.rs` | `EditCommand::{Indent, Outdent, DuplicateLines, DeleteLines, MoveLinesUp, MoveLinesDown, JoinLines, SplitLine, ToggleComment, TypeChar, InsertNewline}`, `CursorCommand::{MoveWordLeft, MoveWordRight, MoveToMatchingBracket, SelectWord, SelectLine, ExpandSelection, AddCursorAbove, AddCursorBelow, AddNextOccurrence, AddAllOccurrences}` |
| LSP session | `crates/editor-core-lsp/src/editor.rs` | `LspSession::{request_hover, request_completion, request_definition, request_declaration, request_type_definition, request_implementation, request_references, request_signature_help, request_inlay_hints, request_document_symbols, request_workspace_symbol, request_prepare_rename, request_rename, request_code_action, request_execute_command, request_code_lens, request_formatting, request_document_diagnostic, apply_workspace_edit}` |
| LSP parsing / application | `crates/editor-core-lsp/src/lib.rs` | `locations_from_value`, `signature_help_from_value`, `code_action_items_from_value`, `apply_plan_for_code_action_item`, `apply_text_edits`, `text_edits_from_value`, `lsp_diagnostics_to_processing_edits`, `lsp_inlay_hints_to_decorations`, `lsp_code_lens_to_decorations`, `lsp_document_symbols_to_outline`, `lsp_workspace_symbols_to_results` |
| Workspace LSP | `crates/editor-core-lsp/src/workspace_sync.rs` | `LspWorkspaceSync::{start, open_workspace_document, close_workspace_document, set_active_workspace_document, poll_workspace, did_change_from_text_delta, apply_workspace_edit}`, `apply_workspace_edit_to_workspace` |
| App helpers | `crates/editor-core-app/src/*` | `CommandPalette`, `FuzzyMatcher`, `WorkspaceFileIndex`, `find_in_files`, `WorkspaceIo`, `status_bar_info`, `FileExplorer` |

## 1. Part 1 — atto-ui 框架级能力

## C1 — 通用拖拽 drag-and-drop

### 1.1 目标

为任意 `Component` 提供跨组件、跨窗口的 drag session，支持 typed payload、drop target、drag-over 反馈、drop/cancel 分发。初期不强制替换滚动条和 splitter 的局部 drag；先并存，后续逐步收敛。

### 1.2 新增模块建议

新增 `src/composable/drag.rs`，并在 `src/composable/mod.rs` re-export：

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DragPayloadType(pub &'static str);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DragPayload {
    Text(String),
    FilePath(std::path::PathBuf),
    ComponentId(ComponentId),
    WindowId(crate::wm::WindowId),
    Custom { ty: DragPayloadType, data: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragOperation {
    Copy,
    Move,
    Link,
}

#[derive(Clone, Debug)]
pub struct DragSource {
    pub payload: DragPayload,
    pub operation: DragOperation,
    pub threshold: u16, // default 2 cells
    pub ghost: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DragOffer<'a> {
    pub payload: &'a DragPayload,
    pub operation: DragOperation,
    pub screen_x: u16,
    pub screen_y: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropEffect {
    None,
    Copy,
    Move,
    Link,
}

#[derive(Clone, Debug)]
pub struct DropFeedback {
    pub effect: DropEffect,
    pub rect: Option<ratatui::layout::Rect>,
    pub label: Option<String>,
}
```

### 1.3 Component trait 扩展

在 `src/composable/component.rs` 增加默认 no-op hooks，避免破坏现有组件：

```rust
pub trait DragAndDrop: Send {
    fn drag_source_at(&mut self, _x: u16, _y: u16, _ctx: ComponentContext<'_>) -> Option<DragSource> {
        None
    }

    fn drag_over(&mut self, _offer: DragOffer<'_>, _ctx: ComponentContext<'_>) -> DropFeedback {
        DropFeedback { effect: DropEffect::None, rect: None, label: None }
    }

    fn drop(&mut self, _offer: DragOffer<'_>, _ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    fn drag_cancelled(&mut self, _ctx: ComponentContext<'_>) {}
}
```

然后把 `Component` 约束从：

```rust
pub trait Component: Layout + Scrollable + FocusNav + DynamicTree + EventHandling + Send
```

扩为：

```rust
pub trait Component:
    Layout + Scrollable + FocusNav + DynamicTree + EventHandling + DragAndDrop + Send
```

并更新 `impl_component_default_traits!` 调用点；多数组件只需新增 `impl DragAndDrop for X {}` 或通过宏覆盖。

### 1.4 EventResult / ComponentContext 扩展

`ComponentAction` 目前只有 `CloseWindow/Changed/Submitted`。不要把完整 drag payload 塞进 `ComponentAction`，因为 drag 是 WM 全局会话。建议：

- `ComponentContext` 增加只读 drag 状态：

```rust
pub drag: Option<DragContext<'a>>,

pub struct DragContext<'a> {
    pub payload: &'a DragPayload,
    pub operation: DragOperation,
    pub source_window: WindowId,
}
```

- `EventResult` 保持轻量，drop 是否成功由 target hook 的 `EventResult` 表示。

### 1.5 WindowManager 全局状态

在 `src/wm/manager/types.rs` 新增：

```rust
#[derive(Clone, Debug)]
pub(crate) struct GlobalDragState {
    pub source_window: WindowId,
    pub source_component: Option<ComponentId>,
    pub start_x: u16,
    pub start_y: u16,
    pub last_x: u16,
    pub last_y: u16,
    pub source: DragSource,
    pub active: bool, // mouse down 后移动超过 threshold 才 true
    pub feedback: Option<DropFeedback>,
    pub target_window: Option<WindowId>,
}
```

`WindowManager` 增加：

```rust
pub(super) global_drag: Option<GlobalDragState>,
```

注意与现有 `drag: Option<DragState>` 并存：

- `drag` 保留 WM chrome move/resize/scrollbar。
- `global_drag` 用于 component-level drag/drop。
- 如果点击命中 `HitRegion != Body`，优先走现有 `drag`，不启动 component drag。

### 1.6 事件流

修改 `src/wm/manager/events.rs`：

1. `MouseEventKind::Down(Left)`：
   - 现有 chrome hit-test 如果 `HitRegion::Body`，不要 `mouse_capture`。
   - 将事件传给目标 view 前，调用 `w.view.drag_source_at(local/screen coords, ctx)`。
   - 若返回 `DragSource`，写入 `global_drag`，但 `active=false`。
   - 仍继续把 Down 发给组件，让普通 selection/click 生效。
2. `MouseEventKind::Drag(Left)` 或 `Moved`：
   - 如果 `global_drag` 存在且未 active，计算曼哈顿距离 `abs(dx)+abs(dy) >= threshold` 后 active。
   - active 后用 `window_at(x, y)` 找 target；对 target view 调 `drag_over`，保存 `feedback`。
   - active drag 期间应返回 `WindowManagerAction { consumed: true }`，避免普通 hover/click 干扰。
3. `MouseEventKind::Up(Left)`：
   - 如果 active drag 且 target feedback `effect != None`，对 target 调 `drop`。
   - 否则对 source 调 `drag_cancelled`。
   - 清空 `global_drag`。
4. `Esc`：
   - 如果 `global_drag.is_some()`，cancel 并 consumed。

坐标约定：

- 对 `drag_source_at` / `drag_over` / `drop` 传入 `ComponentContext.mouse_coordinate_space` 的现有规则：WM 目前传 absolute，容器自行转 local。
- `DragOffer` 保留 screen 坐标，target 需要 local 时用自身 `last_area` 转换。

### 1.7 绘制反馈

在 `src/wm/manager/draw.rs` 末尾，所有窗口绘制后叠加：

- ghost：`DragSource.ghost.unwrap_or(payload label)`，样式使用 `theme.named_style("drag-ghost")` 或 `theme.widget.accent`。
- target highlight：`DropFeedback.rect` 如果存在，画边框/反色填充；style token `drop-target-active`。
- 终端里不做半透明；用 `Clear` + `Paragraph` / 手绘边框。

Theme 新增 token：

- `drag-ghost`
- `drop-target-active`
- `drop-target-reject`
- `drop-insertion-marker`

在 `src/theme/mod.rs::{populate_named_styles, refresh_typed_fields_from_named_styles}` 注册；是否新增 typed fields 可选，优先通过 `named_style` 使用，减少 `Theme` 字段膨胀。

### 1.8 首批消费者

1. `crates/atto-ui-file-tree/src/lib.rs`
   - `FileTree::drag_source_at`：选中节点或鼠标所在 visible row 生成 `DragPayload::FilePath(path)` 需要 app 层提供 id->path；因此底层 `FileTree` 更适合先用 `Custom { ty: "file-tree-node", data: node_id }`。
   - `FileTree::drag_over`：目录行接受，文件行可按 parent dir 接受；反馈插入线/目录高亮。
2. `TabWindow`（`src/composable/tab_window*`，实现时先 glob `src/composable/*tab*`）：
   - tab title drag payload：`Custom { ty: "tab", data: tab index/id }`。
3. C2 docking：
   - 可以先不依赖 C1，用 WM chrome drag 实现；后续拖窗口靠近边缘时复用 C1 feedback。

### 1.9 测试

- 单元：`src/wm/manager/tests.rs`
  - mouse down + move 未超过 threshold 不 active。
  - 超过 threshold 后 target `drag_over` 被调用。
  - drop target reject 时 source cancel。
- PTY：新增 `tests/pty_drag_drop.rs`
  - 用一个 test component 显示 `Dropped: X`。
  - 从左窗口拖到右窗口，断言 drop 文本出现。
  - Esc cancel 后不改变文本。

## C2 — WM Docking window 框架

### 2.1 目标

把 Explorer 目前在 `atto-editor-app/src/app.rs` 里手算的 `ExplorerDock` / `work_without_explorer` 下沉到 `WindowManager`。停靠窗口固定在桌面左/右/下，可 resize 内侧边，可 auto-hide，其他 Normal window 的 work area 自动扣除 dock reserve。

### 2.2 新增类型

在 `src/wm/window.rs`：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockSide {
    Left,
    Right,
    Bottom,
    Top, // 先保留，初期可不开放
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockAutoHide {
    Disabled,
    Enabled { visible: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowDock {
    pub side: DockSide,
    pub size: u16,           // Left/Right 表示 width；Top/Bottom 表示 height
    pub min_size: u16,
    pub max_size: Option<u16>,
    pub auto_hide: DockAutoHide,
    pub handle_label: Option<String>,
}
```

`Window` 增加：

```rust
pub dock: Binding<Option<WindowDock>>,
```

builder：

```rust
pub fn with_dock(mut self, dock: impl Into<Binding<Option<WindowDock>>>) -> Self;
pub fn docked(side: DockSide, size: u16) -> WindowDock;
```

Docked window 的默认行为：

- `movable=false`
- `resizable=true` 但只允许内侧边 resize
- `state=Normal`，不参与 maximize
- `WindowKind::Normal` 即可，不新增 kind

### 2.3 Work area 计算

新增 `src/wm/manager/docking.rs`：

```rust
pub(crate) fn dock_rect(bounds: Rect, dock: &WindowDock, reserved_before: Rect) -> Rect;
pub(crate) fn reserve_for_docked_windows(windows: &[Window], bounds: Rect) -> Rect;
pub(crate) fn effective_work_area(&self, bounds: Rect) -> Rect;
```

推荐简单规则：

1. 对 visible dock windows 按 `windows` 当前 z-order / insertion order 扣除 reserve。
2. Left/Right 扣 width；Bottom 扣 height。
3. `size` clamp 到 `[min_size, max_size.unwrap_or(available)]`。
4. Auto-hide 且 `visible=false` 时只 reserve 1 cell handle。

`WindowManager::draw(bounds, theme)`：

- draw 前先计算每个 dock window rect，并 `window.rect.set(rect)`。
- 非 dock + maximized window 的 bounds 使用 `effective_work_area(bounds)`，不是原始 bounds。

`WindowManager::add_window(window, bounds)`：

- 如果 window docked，rect 由 dock 计算覆盖。
- 如果非 dock，normalize 使用 `effective_work_area(bounds)`。

`Desktop::layout(screen)` 仍只负责 menu/statusbar 原始 work_area；dock reserve 是 `wm` 内部在该 work_area 中处理。

### 2.4 Dock hit-test / resize

扩展 `src/wm/manager/types.rs`：

```rust
pub(crate) enum HitRegion {
    ...
    DockResizeEdge(DockSide),
    DockAutoHideHandle,
}

pub(crate) enum DragKind {
    ...
    DockResize {
        start_size: u16,
        side: DockSide,
    },
}
```

在 `chrome` 或新 `docking` 模块提供：

```rust
fn dock_resize_edge_rect(window_rect: Rect, side: DockSide) -> Rect;
fn dock_handle_rect(bounds: Rect, dock: &WindowDock) -> Rect;
```

规则：

- Left dock 的内侧边是 `rect.x + rect.width - 1`。
- Right dock 的内侧边是 `rect.x`。
- Bottom dock 的内侧边是 `rect.y`。
- Top dock 的内侧边是 `rect.y + rect.height - 1`。

Resize 时只改 `WindowDock.size`，不要直接保留手改 `rect.width/height`。

### 2.5 Auto-hide

MVP 行为：

- `DockAutoHide::Enabled { visible: false }`：只绘制 handle，窗口 view 不绘制或绘制到 handle 外不可见区域。
- 点击 handle：`visible=true`，窗口浮出覆盖 work area，不 reserve 全尺寸，类似 overlay。
- 鼠标点击 dock 以外或焦点离开：`visible=false`。

避免动画；终端下不做滑动过渡。

### 2.6 atto-editor-app 迁移

删除/废弃 `app.rs` 中：

- `ExplorerDock`
- `default_explorer_rect`
- `docked_explorer_rect`
- `work_without_explorer`

替换为：

```rust
Window::new(WindowKind::Normal, "Explorer", Rect::default(), Box::new(view))
    .with_tag("atto-editor-app-explorer")
    .with_dock(Some(WindowDock {
        side: DockSide::Left,
        size: 34,
        min_size: 20,
        max_size: None,
        auto_hide: DockAutoHide::Disabled,
        handle_label: Some("Explorer".into()),
    }))
```

`AppAction::ExplorerLeft/ExplorerRight` 改成更新窗口 `dock.side`，而不是 `rect`。

初始 editor window 可仍用 `default_editor_rect(Desktop::layout(screen).work_area, 0)`；WM 会把它 clamp 到 effective work area。

### 2.7 测试

- `src/wm/manager/tests.rs`
  - left dock reserve 后 normal maximized window 不覆盖 dock。
  - resize left dock 只改变 `dock.size`。
  - right/bottom dock reserve 正确。
- `crates/atto-editor-app/tests/window_scrollbars.rs` 或新增 `explorer_docking.rs`
  - 启动 app 后 Explorer 占左侧，editor 内容不在 Explorer 下方。
  - View -> Dock Explorer Right 后 Explorer 在右侧。

## C3 — 多键序列 keymap 引擎

### 3.1 目标

框架级 keymap 支持单 chord 与 VSCode 风格 multi-chord（如 `Ctrl+K Ctrl+F`），带 prefix pending 状态与 which-key 风格提示。Editor 继续非模态，只是 action 可绑定多键序列。

### 3.2 新模块

新增 `src/app/keymap.rs` 或 `src/input/keymap.rs`（建议 `src/app/keymap.rs`，因为菜单/命令面板也消费 action registry）：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub code: crossterm::event::KeyCode,
    pub modifiers: crossterm::event::KeyModifiers,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeySequence(pub Vec<KeyChord>);

#[derive(Clone, Debug)]
pub struct CommandDescriptor<A> {
    pub id: String,
    pub title: String,
    pub category: Option<String>,
    pub default_sequence: Option<KeySequence>,
    pub action: A,
}

#[derive(Clone, Debug)]
pub enum KeymapMatch<A> {
    None,
    Prefix { choices: Vec<WhichKeyChoice> },
    Exact(A),
    AmbiguousExact { action: A, choices: Vec<WhichKeyChoice> },
    Timeout,
}

pub struct KeySequenceEngine<A> {
    trie: KeyTrie<A>,
    pending: Vec<KeyChord>,
    pending_since: Option<std::time::Instant>,
    timeout: std::time::Duration,
}
```

### 3.3 与现有 editor keymap 的关系

`atto-ui-editor/src/keymap.rs` 当前定义 `KeyChord`。迁移路径：

1. 第一阶段复制/桥接：
   - 保留 `atto_ui_editor::KeyChord`，新增 `impl From<atto_ui::app::KeyChord> for atto_ui_editor::KeyChord` 或反向转换。
   - `EditorKeymap` 内部继续 `HashMap<KeyChord, EditorAction>`。
2. C3 完成后：
   - `EditorKeymap` 改为 type alias / wrapper：`KeySequenceEngine<EditorAction>`。
   - `EditorConfig.keymap: Binding<EditorKeymap>` 保持 API 名称。

### 3.4 Which-key 弹窗

实现为普通 `Component`，可放在 `src/app/keymap_popup.rs`：

- 输入：`Binding<Option<WhichKeyModel>>`
- 绘制：边框 + rows，显示 `key label` 和 `command title`
- 样式：`theme.named_style("which-key-popup")`, `"which-key-key"`, `"which-key-title"`

弹窗由 `Desktop` 管理最合适：

- `Desktop` 增加 `keymap_popup: Binding<Option<WhichKeyModel>>`。
- `Desktop::handle_event` 若 `KeymapMatch::Prefix`，显示 tooltip/floating window 或直接在 desktop draw 中 overlay。

Editor 内局部 which-key 也可先 inline popup（和 completion popup 一样）实现，但长期应框架级。

### 3.5 Action registry

命令面板也需要同一份命令描述。新增：

```rust
pub trait CommandAction: Clone + Send + Sync + 'static {}

pub struct CommandRegistry<A> {
    commands: Vec<CommandDescriptor<A>>,
    by_id: HashMap<String, usize>,
}
```

Editor App 可创建 `CommandRegistry<AppAction>`，菜单、keymap、命令面板都从 registry 派生，避免快捷键文本散落在菜单中。

### 3.6 测试

- 单元：
  - `Ctrl+K` 返回 Prefix。
  - `Ctrl+K Ctrl+F` 返回 Exact(format)。
  - prefix timeout 清空 pending。
  - 单键精确匹配不进入 pending，除非同时也是更长序列前缀。
- PTY：
  - 按 `Ctrl+K` 显示 which-key。
  - 再按绑定键触发动作并关闭 popup。

## C4 — Menu / StatusBar Turbo Vision 风格翻新

### 4.1 MenuBar

当前：

- `MenuItem.shortcut` 既被用作显示文本，也被 `handle_shortcut_char` 当作单字符菜单助记。
- 顶部 menu 的助记规则是标题首字符。

建议拆分字段，保持兼容：

```rust
pub struct MenuItem {
    pub label: Binding<String>,
    pub accelerator: Binding<Option<String>>, // 显示 Ctrl+S
    pub mnemonic: Binding<Option<char>>,      // 菜单激活后按键
    ...
}
```

迁移方法：

- 保留 `shortcut()` builder，但语义改名为 `accelerator()` 后逐步替换。
- 新增 `.mnemonic('S')`。
- `handle_shortcut_char` 优先匹配 mnemonic；没有 mnemonic 时 fallback 到 label 首字符。

绘制增强在 `src/app/menu/draw.rs`：

- 顶部菜单 label 支持 `&File` / `_File` 标记热键，绘制时去掉标记并给该字符 `theme.named_style("menu-mnemonic")`。
- 下拉菜单：
  - border + shadow 已有，保留。
  - shortcut/accelerator 右对齐已有，保留但间距固定：`label | spacer | accelerator | submenu arrow`。
  - disabled 使用 `theme.widget.disabled` 已有。
  - selected 使用 reverse 或 active bg。

Theme token：

- `menu-mnemonic`
- `menu-item-shortcut`
- `menu-border`
- `menu-shadow` 可复用 `window-shadow`

### 4.2 StatusBar

替换 `src/app/status.rs` 的 `StatusBar { left, right }` 为分段模型，保持旧 API：

```rust
#[derive(Clone, Debug)]
pub enum StatusSegmentAlign { Left, Right }

#[derive(Clone, Debug)]
pub struct StatusSegment {
    pub id: Option<String>,
    pub text: Binding<String>,
    pub style: Option<Style>,
    pub align: StatusSegmentAlign,
    pub min_width: u16,
    pub priority: u8, // 宽度不足时低优先级先隐藏
    pub on_click: Option<MenuCallback>,
}

pub struct StatusBar {
    left: String,  // compat
    right: String, // compat
    segments: Vec<StatusSegment>,
}
```

API：

```rust
impl StatusBar {
    pub fn set_segments(&mut self, segments: Vec<StatusSegment>);
    pub fn push_segment(&mut self, segment: StatusSegment);
    pub fn handle_mouse(&mut self, event: &MouseEvent, area: Rect) -> EventResult;
}
```

绘制规则：

- 左 segments 从左到右，右 segments 从右到左。
- segment 之间用一个空格或 glyph `status-separator`。
- 宽度不足时按 `priority` 隐藏，最后按 grapheme 截断。
- `set_left/set_right` 兼容：如果 `segments.is_empty()`，用旧逻辑。

Editor statusbar 建议 segments：

- 左：mode 固定 `CUA`, path / tab title, dirty `*`
- 右：diagnostics `E:n W:n`, language, `Ln x, Col y`, indentation, LSP status

`../editor-core/crates/editor-core-app/src/status_bar.rs` 已有 `status_bar_info(ws, view_id, workspace_root, languages)` 可在迁移到 `Workspace` 后复用。

### 4.3 测试

- `src/app/status.rs` 单元补充：
  - Unicode 宽度分段对齐。
  - priority 隐藏。
  - click hit-test 返回正确 segment。
- `tests/pty_desktop.rs`：
  - menu mnemonic 高亮（可断言文本不含 `&`）。
  - statusbar 左右分段显示。

## 2. Part 2 — editor / atto-ui-editor 专用工作

## 2.1 Editor action / command 统一入口

### 2.1.1 近期扩展 `EditorAction`

在 `crates/atto-ui-editor/src/keymap.rs` 增加：

```rust
pub enum EditorAction {
    ...
    // LSP additions
    LspNextDiagnostic,
    LspPrevDiagnostic,
    LspCodeAction,
    LspRename,
    LspSignatureHelp,
    LspFormatDocument,
    LspToggleInlayHints,

    // Editing additions
    MoveWordLeft,
    MoveWordRight,
    MoveToMatchingBracket,
    ToggleComment,
    JoinLines,
    MoveLinesUp,
    MoveLinesDown,
    DuplicateLines,
    DeleteLines,
    Indent,
    Outdent,
    SplitLine,
    AddCursorAbove,
    AddCursorBelow,
    AddNextOccurrence,
    AddAllOccurrences,
    ExpandSelection,
}
```

默认键位建议：

| Action | Key |
|---|---|
| next diagnostic | `F8` |
| prev diagnostic | `Shift+F8` |
| code action | `Ctrl+.` |
| rename | `F2` |
| signature help | `Ctrl+Shift+Space` |
| format document | 先通过命令面板；C3 后 `Ctrl+K Ctrl+F` |
| toggle comment | `Ctrl+/` |
| move word | `Ctrl+Left/Right` 或 `Alt+Left/Right` |
| matching bracket | `Ctrl+Shift+\\` 或命令面板 |
| move line | `Alt+Up/Down` |
| duplicate line | `Shift+Alt+Down` |
| multi cursor up/down | `Ctrl+Alt+Up/Down` |
| add next occurrence | `Ctrl+D` |
| add all occurrences | `Ctrl+Shift+L` |

### 2.1.2 编辑原语 dispatch

在 `crates/atto-ui-editor/src/view/actions.rs` 中新增 match 分支：

```rust
EditorAction::MoveWordLeft => self.execute(Command::Cursor(CursorCommand::MoveWordLeft))
EditorAction::MoveWordRight => self.execute(Command::Cursor(CursorCommand::MoveWordRight))
EditorAction::MoveToMatchingBracket => self.execute(Command::Cursor(CursorCommand::MoveToMatchingBracket))
EditorAction::ToggleComment => self.execute_and_sync_text(Command::Edit(EditCommand::ToggleComment { config }))
EditorAction::JoinLines => self.execute_and_sync_text(Command::Edit(EditCommand::JoinLines))
EditorAction::MoveLinesUp => self.execute_and_sync_text(Command::Edit(EditCommand::MoveLinesUp))
EditorAction::MoveLinesDown => self.execute_and_sync_text(Command::Edit(EditCommand::MoveLinesDown))
EditorAction::DuplicateLines => self.execute_and_sync_text(Command::Edit(EditCommand::DuplicateLines))
EditorAction::DeleteLines => self.execute_and_sync_text(Command::Edit(EditCommand::DeleteLines))
EditorAction::Indent => self.execute_and_sync_text(Command::Edit(EditCommand::Indent))
EditorAction::Outdent => self.execute_and_sync_text(Command::Edit(EditCommand::Outdent))
EditorAction::SplitLine => self.execute_and_sync_text(Command::Edit(EditCommand::SplitLine))
EditorAction::AddCursorAbove => self.execute(Command::Cursor(CursorCommand::AddCursorAbove))
EditorAction::AddCursorBelow => self.execute(Command::Cursor(CursorCommand::AddCursorBelow))
EditorAction::AddNextOccurrence => self.execute(Command::Cursor(CursorCommand::AddNextOccurrence { options: SearchOptions::default() }))
EditorAction::AddAllOccurrences => self.execute(Command::Cursor(CursorCommand::AddAllOccurrences { options: SearchOptions::default() }))
EditorAction::ExpandSelection => self.execute(Command::Cursor(CursorCommand::ExpandSelection))
```

`ToggleComment` 需要语言注释配置。`editor-core-lang` 已有 `CommentConfig`，但 `atto-editor-app/src/language.rs` 当前只返回 language id / syntax / lsp。新增：

```rust
pub fn comment_config_for_language(language_id: &str) -> Option<editor_core_lang::CommentConfig>
```

并在 `EditorConfig` 增加：

```rust
pub comment: Binding<Option<CommentConfig>>,
```

`DocumentTabView::build_editor_view` 根据 language id 设置。

## L1 — 诊断显示

### 2.2.1 数据模型

在 `EditorLspController` 中新增：

```rust
diagnostics: Vec<editor_core_lsp::LspDiagnostic>,
diagnostic_result_id: Option<String>,
pending_document_diagnostic: Option<u64>,
diagnostic_cursor: Option<usize>,
diagnostics_revision: u64,
```

在 `EditorViewHandle` 增加：

```rust
pub diagnostics_summary: Binding<DiagnosticsSummary>,
```

新增类型（`view/mod.rs` 或独立 `diagnostics.rs`）：

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticsSummary {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub hints: usize,
}
```

### 2.2.2 接收 diagnostics

当前 `maybe_poll_lsp` 只处理 `LspEvent::Response`。改为：

```rust
for ev in lsp.drain_events() {
    match ev {
        LspEvent::Notification(LspNotification::PublishDiagnostics(params)) => {
            self.apply_publish_diagnostics(params);
        }
        LspEvent::Response(resp) => self.handle_lsp_response(resp),
        LspEvent::DeferredRequest(req) => self.handle_deferred_request(req),
    }
}
```

实现：

```rust
fn apply_publish_diagnostics(&mut self, params: LspPublishDiagnosticsParams) {
    let Some(lsp) = self.lsp.session.as_ref() else { return; };
    if !lsp.diagnostics_version_matches(&params) { return; } // editor-core-lsp 有该 helper
    let edits = editor_core_lsp::lsp_diagnostics_to_processing_edits(
        self.state_manager.editor().line_index(),
        &params,
    );
    self.state_manager.apply_processing_edits(edits);
    self.lsp.diagnostics = params.diagnostics;
    self.update_diagnostics_summary();
}
```

Pull diagnostics：

- 在 `EditorAction::LspNextDiagnostic/Prev` 或定时 idle 时，如果 server 支持 pull，可调用 `lsp.request_document_diagnostic(self.lsp.diagnostic_result_id.clone())`。
- `textDocument/diagnostic` response 形状需要按 LSP 解析；如果没有 typed helper，先从 `result.items` 转成 `LspPublishDiagnosticsParams { uri, diagnostics, version: None }`。

### 2.2.3 渲染

Style layer：

- `lsp_diagnostics_to_processing_edits` 会写 `StyleLayerId::DIAGNOSTICS`，style id 在 `editor-core-lsp` 内部以 `0x0400_0100 | severity` 编码。
- 在 `crates/atto-ui-editor/src/theme.rs` `EditorTheme::dark_default()` 的 `style_ids` 加：
  - error：red + underline / bg dark red
  - warning：yellow + underline
  - info：cyan
  - hint：dark gray

Gutter 标记：

在 `render_gutter` 中构建 `diagnostics_by_line: HashMap<usize, DiagnosticSeverity>`：

- 使用 `editor.diagnostics()`，每个 diagnostic 的 `range.start` 转 line：`editor.line_index().char_offset_to_position(offset)`（确认方法名；若不同，在 `LineIndex` 中找 offset->position）。
- 同一行取最高 severity。
- 在 folding marker 前或后留一列诊断 marker。需要调整 `layout_rects`：如果 diagnostics enabled，`gutter_w += 2`。
- marker 建议：`E`, `W`, `I`, `H` ASCII，避免依赖 nerd font。

跳转：

新增 helper：

```rust
fn jump_to_diagnostic(&mut self, direction: DiagnosticJumpDirection) -> bool
```

- 读取 `state_manager.editor().diagnostics()`。
- 当前 cursor offset = `cursor_offset()`。
- next：找 `diag.range.start > current`，否则 wrap 到第一个。
- prev：找 `< current` 的最后一个，否则 wrap 到最后。
- 执行 `CursorCommand::MoveTo { line, column }` 并 `adjust_scroll()`。

### 2.2.4 app 状态栏

短期：

- `EditorViewHandle.diagnostics_summary` 由 `DocumentTabView` 暴露给 `EditorWindowView` 较麻烦；可先只在 editor title 或 popup 中显示。

推荐：

- `DocumentTabView::new` 保留 `EditorViewHandle`，不要丢弃 `_handle`。
- `TabState` 增加 `diagnostics_summary: Binding<DiagnosticsSummary>`。
- `EditorWindowView::active_tab_status()` 返回 summary。
- `atto-editor-app/src/app.rs` on_tick 根据 active editor 更新 `desktop.status` segments。

### 2.2.5 测试

- `crates/atto-ui-editor/tests/lsp_editor.rs`
  - mock_lsp_server 发 `publishDiagnostics`，断言 `state_manager.editor().diagnostics()` 非空。
  - PTY buffer 出现 gutter marker `E` / `W`。
  - `F8` 跳到诊断行。
- `crates/atto-editor-app/tests/*`
  - 打开带 mock LSP 的文件，statusbar 显示 `E:1 W:0`。

## L2 — Code Action

### 2.3.1 状态与 popup

在 `EditorLspController` 增加：

```rust
pending_code_action: Option<u64>,
code_action_items: Vec<editor_core_lsp::LspCodeActionItem>,
```

`popup.rs` 新增：

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct CodeActionPopupModel {
    pub rect: Rect,
    pub items: Vec<CodeActionItemView>,
    pub selected: usize,
    pub scroll: usize,
    pub accept: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeActionItemView {
    pub title: String,
    pub kind: Option<String>,
    pub is_preferred: bool,
}
```

`EditorView` 增加 binding：

```rust
code_action_popup: Binding<Option<CodeActionPopupModel>>,
```

渲染可复用 `render_inline_completion_popup` 逻辑，抽一个通用 list popup helper，避免复制。

### 2.3.2 请求

`EditorAction::LspCodeAction`：

```rust
let cursor_state = self.state_manager.get_cursor_state();
let (start, end) = selection_offsets(primary_selection).unwrap_or(cursor_offset..cursor_offset);
let diagnostics = diagnostics_overlapping_range(start, end);
let context = json!({
    "diagnostics": diagnostics_as_lsp_json,
    "only": null,
});
lsp.request_code_action(line_index, start, end, context)
```

如果把 `LspDiagnostic` 原 JSON 丢失，初期可以传 `diagnostics: []`，多数 server 仍返回 refactor/source actions；quickfix 可能减少。更完整做法是在 `LspDiagnostic` 中保留 `data` 等字段并转回 JSON。

### 2.3.3 Response

在 `handle_lsp_response`：

```rust
if pending_code_action == Some(resp.id) && resp.method == "textDocument/codeAction" {
    let items = resp.result.as_ref().map(code_action_items_from_value).unwrap_or_default();
    self.lsp.code_action_items = items.clone();
    self.code_action_popup.set(Some(model_from_items(items)));
}
```

### 2.3.4 Apply

Popup Enter / click set `accept=Some(idx)`，`process_code_action_accept()`：

```rust
let item = self.lsp.code_action_items.get(idx).cloned();
let plan = apply_plan_for_code_action_item(&item);
if let Some(edit) = plan.edit {
    if let Some(lsp) = self.lsp.session.as_mut() {
        let changed = lsp.apply_workspace_edit(&mut self.state_manager, &edit)?;
        let after = self.state_manager.editor().get_text();
        self.config.text.set(after.clone());
        self.maybe_apply_syntax_highlighting();
        self.lsp_did_change(full_change_with(after));
    }
}
if let Some(cmd) = plan.command {
    lsp.request_execute_command(cmd.command, cmd.arguments)?;
}
```

单文档 `apply_workspace_edit` 只应用当前 URI；如果 `summarize_workspace_edit(&edit).documents.len() > 1`，短期显示 “requires workspace edit support” toast / event，不静默丢弃。

### 2.3.5 测试

- Mock LSP 返回 `CodeAction[]` 含 edit。
- `Ctrl+.` 显示 action title。
- Enter 应用 edit，文本改变且 LSP didChange 发出。

## L3 — Rename 与 workspace LSP 归属

### 2.4.1 为什么先做 workspace refactor

当前 `DocumentTabView::build_editor_view` 创建 `EditorView`，每个 primary view 持有自己的 `LspSession`，secondary split 禁用 LSP。Rename 需要：

- prepare rename 当前文档位置；
- rename response 返回 `WorkspaceEdit`，可能跨多个文件；
- 应用到所有已打开 buffer，必要时打开/保存未打开文件；
- 同步所有 tab / split 的文本 binding 和 dirty 状态。

因此 L3 前需要 app 层共享 `editor_core::Workspace` 或至少 workspace edit coordinator。

### 2.4.2 推荐目标架构

在 `crates/atto-editor-app/src/app.rs` 的 `AppState` 增加：

```rust
workspace: editor_core::workspace::Workspace,
workspace_io: editor_core_app::WorkspaceIo, // 需在 Cargo.toml 加 editor-core-app path 依赖，或复制极小 helper
lsp_by_root_language: HashMap<LspKey, editor_core_lsp::LspWorkspaceSync>,
```

新增 app 模块：

- `src/workspace_state.rs`：封装 `Workspace`, path<->buffer_id, tab id。
- `src/lsp_workspace.rs`：启动/复用 `LspWorkspaceSync`，处理 poll/apply。

迁移策略：

1. **Bridge 阶段**：保留 `EditorView` 基于 `Binding<String>`；workspace edit 后更新对应 tab binding。
2. **最终阶段**：新增 `WorkspaceEditorView`，直接持有 `Arc<Mutex<Workspace>> + ViewId`，替代 per-tab `EditorView` 的 `EditorStateManager`。

Bridge 阶段足够支持 rename：

- 打开文件时仍读到 binding，同时在 `Workspace` 中 `open_buffer(Some(path_to_file_uri(path)), &text, viewport_width)`。
- `TabState` 增加 `buffer_id: Option<BufferId>`。
- 文本编辑发生时，通过 binding dirty observer 同步回 `Workspace`（初期可在 save/rename 前同步整文档 replace）。
- Rename workspace edit 应用到 `Workspace` 后，把 `Workspace::buffer_text(buffer_id)` 写回各 tab binding。

### 2.4.3 Rename UI

`popup.rs` 新增 `RenamePopupModel`，或直接复用一个 modal input component：

- `F2` -> `request_prepare_rename(line_index, line, column)`。
- response OK 后打开 inline input popup，默认值为当前 word / prepare range text。
- 输入 Enter -> `request_rename(line_index, line, column, new_name)`。
- response -> `WorkspaceEdit`。

### 2.4.4 Apply workspace edit

如果已经有 `LspWorkspaceSync`：

```rust
lsp_sync.apply_workspace_edit(&mut workspace, &edit)
```

否则临时：

```rust
editor_core_lsp::apply_workspace_edit_to_workspace(&mut workspace, &edit)
```

对 `skipped_uris`：

- MVP：弹出/状态栏提示“Skipped unopened files”。
- 完整版：对 file URI 读取文件，应用 edits，写回磁盘或打开为 dirty buffer。这个需要用户确认，默认不要静默改未打开文件。

### 2.4.5 测试

- 单文档 rename：mock LSP 返回当前 file edit，binding 更新。
- 跨已打开两个 tab rename：两个 tab 文本都更新。
- 未打开 URI：显示 skipped，不写磁盘。

## L4 — Signature Help

### 2.5.1 触发

在 `view/input.rs`：

- 默认文本插入 `(`、`,` 后，如果 LSP enabled，调用 `request_signature_help_now()`。
- `EditorAction::LspSignatureHelp` 手动触发。

### 2.5.2 State / popup

`EditorLspController`：

```rust
pending_signature_help: Option<u64>,
```

`popup.rs`：

```rust
pub struct SignatureHelpPopupModel {
    pub rect: Rect,
    pub signatures: Vec<SignatureView>,
    pub active_signature: usize,
    pub active_parameter: Option<usize>,
}
```

Response：

```rust
let help = signature_help_from_value(result)?;
```

渲染：

- anchor 使用 `completion_popup_rect_for_cursor` 类似逻辑。
- 第一行显示 signature label。
- active parameter 用 `theme.popup_selected` 或 underline。
- 文档说明可先不渲染，避免 popup 过高。

### 2.5.3 测试

- Mock LSP 对 `textDocument/signatureHelp` 返回 signature。
- 输入 `(` 后 popup 出现。
- Esc 关闭 popup。

## L5 — Formatting

### 2.6.1 手动格式化

`EditorAction::LspFormatDocument`：

```rust
let options = editor_core_lsp::lsp_formatting_options_for_indentation_config(
    &indentation_config
);
lsp.request_formatting(options)
```

Response `textDocument/formatting`：

```rust
let edits = text_edits_from_value(result);
let old_char_count = self.state_manager.editor().char_count();
let full_lsp_change = lsp.full_document_change(...);
apply_text_edits(&mut self.state_manager, &edits);
self.config.text.set(after.clone());
self.lsp_did_change(full_change_with(after));
```

### 2.6.2 保存时格式化

在 `EditorConfig` 增加：

```rust
pub format_on_save: Binding<bool>,
```

`EditorWindowCommand::SaveActive` 前如果 active tab 的 view 支持 LSP formatting：

- Bridge 简单方案：新增 `EditorWindowCommand::FormatThenSave`，EditorView 格式化成功后通过 `EditorEvent::Formatted` 通知 app 再 save。
- 更简单 MVP：只做手动 formatting，format-on-save 后置。

### 2.6.3 测试

- Mock LSP 返回 TextEdit，触发 format 后文本改变。
- 无 LSP 时 action ignored，不改变文本。

## L6 — Inlay Hints

### 2.7.1 请求时机

`EditorConfig`：

```rust
pub inlay_hints: EditorInlayHintsConfig { enabled: Binding<bool>, refresh_delay: Binding<Duration> }
```

在 `draw` 或 idle 时，如果：

- focused；
- enabled；
- viewport/range/text revision 改变；
- 没有 pending；

请求当前可见 range：

```rust
let start = line_index.position_to_char_offset(first_visible_line, 0);
let end = line_index.position_to_char_offset(last_visible_line, line_len);
lsp.request_inlay_hints(line_index, start, end)
```

### 2.7.2 应用

Response：

```rust
let edit = lsp_inlay_hints_to_processing_edit(line_index, result);
self.state_manager.apply_processing_edits([edit]);
```

`editor-core` decorations 可通过 composed grid 渲染，但当前 `render_text` 使用：

```rust
state_manager.get_viewport_content_styled(...)
```

要显示 virtual text，需要改成 composed：

- 如果 inlay/code lens enabled，调用 `get_viewport_content_composed`（`editor-core` 和 Workspace 都有 composed snapshot API）。
- 或先把 inlay hints 转 style intervals 不显示文本（不推荐）。

### 2.7.3 渲染变更

`render_text` 抽象 grid 来源：

```rust
enum EditorRenderGrid {
    Styled(HeadlessGrid),
    Composed(ComposedGrid),
}
```

把 `ComposedCell` 的 sources 映射为 `Span`，virtual text style 用 `theme.inlay_hint` / `theme.code_lens`。

### 2.7.4 测试

- `crates/atto-ui-editor/tests/pty_editor.rs` 或新 `pty_inlay_hints.rs`
  - 注入 LSP inlay hint response。
  - 断言屏幕出现 `: Type` 之类 virtual text。

## 阶段二 — Pickers / 导航 / 命令面板

### 2.8.1 通用 picker component

新增 `crates/atto-editor-app/src/picker.rs`：

```rust
pub struct PickerItem<A> {
    pub title: String,
    pub subtitle: Option<String>,
    pub shortcut: Option<String>,
    pub action: A,
}

pub struct PickerView<A> {
    query: Binding<String>,
    items: Binding<Vec<PickerItem<A>>>,
    selected: usize,
    on_accept: EventQueue<A>,
}
```

实现为 modal/floating window：

- 顶部 `TextBox` 输入 query。
- 下方 list；使用 `atto_ui::fuzzy::fuzzy_filter`。
- Enter accept，Esc close。
- 鼠标点击选择/双击 accept。

### 2.8.2 Command palette

`AppAction` 增加：

```rust
OpenCommandPalette,
RunCommand(String),
```

新增 `src/commands.rs`：

```rust
pub fn app_command_registry(actions: EventQueue<AppAction>) -> CommandRegistry<AppAction>
```

所有菜单项也从 registry 构造，避免 action 文案和快捷键重复。

可参考 `../editor-core/crates/editor-core-app/src/command_palette.rs`：

- `CommandPaletteItem { id, title, shortcut, category }`
- `CommandPalette::filter(query, limit)`

但本仓库已有 `atto_ui::fuzzy`，优先用它，因为可返回 match positions 便于高亮。

### 2.8.3 File picker (`Ctrl+P`)

依赖：

- 当前可先用 `crates/atto-editor-app/src/workspace.rs::build_workspace_tree` flatten files。
- 更完整可引入 `editor-core-app::WorkspaceFileIndex`，但该 crate 依赖 `ignore/thiserror`，需要在 `atto-editor-app/Cargo.toml` 增加 path dependency 或复制轻量逻辑。

设计：

- `AppState` 增加 `file_index: Option<WorkspaceFileIndex>` 或 `Vec<FileIndexEntry>` cache。
- workspace roots 改变时 invalidation。
- accept -> `AppAction::OpenPath { path, target: OpenTarget::NewTab }`。

### 2.8.4 Buffer/tab picker

- 从 `AppState.editor_windows` + each `EditorWindowView` 暴露 tab list。
- 需要 `EditorWindowCommand::SelectTab(usize)`。
- `TabState` 增加 stable `tab_id: u64`，避免 index 改变导致 picker accept 错 tab。

### 2.8.5 Document / workspace symbols

Document symbol：

- 当前 tab `EditorView` 调 `lsp.request_document_symbols()`。
- response 用 `lsp_document_symbols_to_outline(line_index, result)`。
- picker item title = symbol name，subtitle = kind / line。
- accept -> cursor move to symbol range start。

Workspace symbol：

- 需要 app/workspace LSP。单文档 LSP 也能调用 `request_workspace_symbol(query)`，但结果 URI 可能跨文件；accept 需要 `AppAction::OpenPath` + jump.
- response 用 `lsp_workspace_symbols_to_results(result)`。

### 2.8.6 Global search

优先用 external `rg` 还是 Rust helper？

- `EDITOR_APP.md` 指 “ripgrep 后端”。实现可用 `std::process::Command::new("rg")`，但测试环境可能不稳定。
- 稳定 MVP：复用 `../editor-core/crates/editor-core-app/src/find_in_files.rs` 的逻辑或添加 `ignore` 依赖实现纯 Rust。

输出进入 docked Search Results panel（C2 下/右 dock）。

## 阶段三 — 编辑动作接线

### 2.9.1 语言配置

新增 `crates/atto-editor-app/src/language.rs`：

```rust
pub fn comment_config_for_language(language_id: &str) -> Option<CommentConfig>;
pub fn indentation_config_for_language(language_id: &str) -> Option<IndentationConfig>;
pub fn auto_pairs_config_for_language(language_id: &str) -> AutoPairsConfig;
```

应用到 `EditorConfig`：

- `cfg.comment.set(...)`
- `ViewCommand::SetIndentationConfig`
- `ViewCommand::SetAutoPairsConfig`

### 2.9.2 Action 映射表

| 功能 | editor-core command |
|---|---|
| Word left/right | `Command::Cursor(CursorCommand::MoveWordLeft/MoveWordRight)` |
| Matching bracket | `Command::Cursor(CursorCommand::MoveToMatchingBracket)` |
| Toggle comment | `Command::Edit(EditCommand::ToggleComment { config })` |
| Join lines | `Command::Edit(EditCommand::JoinLines)` |
| Move lines | `Command::Edit(EditCommand::MoveLinesUp/MoveLinesDown)` |
| Duplicate lines | `Command::Edit(EditCommand::DuplicateLines)` |
| Delete lines | `Command::Edit(EditCommand::DeleteLines)` |
| Indent/outdent | `Command::Edit(EditCommand::Indent/Outdent)` |
| Split line | `Command::Edit(EditCommand::SplitLine)` |
| Multi-cursor vertical | `Command::Cursor(CursorCommand::AddCursorAbove/AddCursorBelow)` |
| Add next/all occurrence | `Command::Cursor(CursorCommand::AddNextOccurrence/AddAllOccurrences { options })` |
| Textobject-ish expand | `Command::Cursor(CursorCommand::ExpandSelection)` first; tree-sitter structural expansion later |

### 2.9.3 Tests

- `crates/atto-ui-editor/src/view/tests.rs`
  - each action maps to expected text/cursor change.
- `tests/pty_editor.rs`
  - `Ctrl+/` toggles comment for Rust file.
  - `Alt+Down` moves line.
  - `Ctrl+D` creates second selection; rendering cursor count may need textual debug hook.

## 阶段四 — 编辑体验打磨

### 2.10.1 Auto-pairs / auto-indent

`editor-core` 已有：

- `EditCommand::TypeChar { ch }`
- `EditCommand::InsertNewline { auto_indent: bool }`
- `ViewCommand::SetAutoPairsConfig`
- `ViewCommand::SetIndentationConfig`

改 `EditorView::handle_key_event`：

- 普通 `Char(c)` 不再 `insert_text(&c.to_string())`，而是 `execute_and_sync_text(Command::Edit(EditCommand::TypeChar { ch: c }))`。
- Enter 不再插入 `"\n"`，改 `EditCommand::InsertNewline { auto_indent: config.auto_indent }`。
- Backspace/Delete 可用 `DeleteGraphemeBack/Forward` 或已有 `Backspace/DeleteForward`。

### 2.10.2 Trim trailing whitespace

`EditorConfig`：

```rust
pub trim_trailing_whitespace_on_save: Binding<bool>
```

实现为保存前生成 `TextEditSpec`，用 `EditCommand::ApplyTextEdits`。注意不能 trim 当前正在编辑的最后空白行除非用户开启。

### 2.10.3 Jumplist / registers

低优先级。`editor-core::Workspace` 已有 jump list 相关 API（`apply_jump_target` 等），等 workspace 迁移后接。

## F-FT — File tree 功能补齐

### 2.11.1 atto-ui-file-tree 数据模型扩展

当前 `FileTreeNode`：

```rust
pub struct FileTreeNode {
    pub id: FileTreeNodeId,
    pub name: String,
    pub kind: FileTreeNodeKind,
    pub children: Vec<FileTreeNode>,
    pub is_expanded: bool,
}
```

扩展：

```rust
pub enum FileTreeGitStatus { Modified, Added, Deleted, Renamed, Untracked, Ignored, Clean }

pub struct FileTreeNode {
    ...
    pub git_status: Option<FileTreeGitStatus>,
}
```

为了兼容，提供 builder：

```rust
pub fn with_git_status(mut self, status: FileTreeGitStatus) -> Self;
```

### 2.11.2 多选

`FileTreeBindings` 当前只有：

```rust
selection: Binding<Option<FileTreeNodeId>>
```

新增：

```rust
selections: Binding<BTreeSet<FileTreeNodeId>>
selection_anchor: Option<FileTreeNodeId>
```

行为：

- click：单选，anchor=clicked。
- Ctrl+click：toggle clicked。
- Shift+click：从 anchor 到 clicked 的 visible row range 全选。
- keyboard up/down + Shift：扩展 range。

保持 `selection` 表示 primary selection，兼容现有 API。

### 2.11.3 Context menu

框架没有通用 context menu；可先实现 file-tree 内部 popup，后续抽到 `atto-ui`。

`FileTree` 增加 callbacks：

```rust
on_new_file, on_new_folder, on_cut, on_copy, on_paste, on_copy_path, on_reveal
```

但更适合 app 层处理文件系统操作，所以底层 `FileTree` 只发事件：

```rust
pub enum FileTreeEvent {
    ContextAction { action: FileTreeContextAction, ids: Vec<FileTreeNodeId> },
}
```

由于当前 callback 系统是 runtime `CallbackHandle`，在 Rust app 内建议 `ExplorerWindowView` 拦截右键并显示 app-level menu。

### 2.11.4 Inline rename / new

`FileTree` 增加编辑状态：

```rust
inline_edit: Option<InlineEditState> {
    node_id: Option<FileTreeNodeId>, // None for new file/folder placeholder
    parent_id: Option<FileTreeNodeId>,
    text: TextBuffer,
    kind: InlineEditKind,
}
```

事件：

- Enter commit -> `ExplorerWindowCommand` / `AppAction` 执行 fs op。
- Esc cancel。
- draw row 时替换 label 为 input field。

### 2.11.5 拖拽移动

消费 C1：

- Source payload：`Custom { ty: "atto-ui-file-tree/node-ids", data: "id1,id2" }`
- ExplorerWindowView 在 drop 时解析 id -> path。
- 只允许 drop 到 directory 或 root。
- 文件移动用 `std::fs::rename`; 跨 filesystem 失败时 fallback copy+delete 需明确确认，MVP 可报错。
- 成功后 `ExplorerWindowCommand::Refresh`。

### 2.11.6 剪贴板

`ExplorerWindowView` 增加：

```rust
clipboard: Option<FileClipboard> {
    mode: Cut | Copy,
    paths: Vec<PathBuf>,
}
```

Paste 到 directory：

- copy file: `fs::copy`
- copy dir: recursive copy helper
- cut: `fs::rename`
- name collision: append ` copy`, ` copy 2` 或弹确认；MVP 选择不覆盖并显示 error。

### 2.11.7 Git status

MVP shell out：

```bash
git -C <root> status --porcelain=v1 --ignored=matching
```

解析 XY + path，映射到 `FileTreeGitStatus`。不要阻塞 draw；在 app tick 中后台刷新，结果写入 tree nodes。

后续可引入 git2，但当前 workspace 未依赖，先不用。

### 2.11.8 FS 监听

可选；若引入 `notify` 依赖，作为 feature gate。MVP 用手动 refresh + 文件操作后 refresh。

### 2.11.9 Tests

- `crates/atto-ui-file-tree/tests/pty_file_tree.rs`
  - multi-select keyboard/mouse。
  - inline rename commit/cancel。
  - context menu opens on right-click。
- `crates/atto-editor-app/tests/explorer_*`
  - create/rename/delete temp files。
  - drag move temp file into folder。
  - cut/copy/paste no overwrite。

## 3. 推荐实施顺序与依赖

1. **C1 drag-and-drop MVP**：只做 Component hooks + WM session + test component，不迁移所有局部 drag。
2. **C2 docking MVP**：实现 Left/Right/Bottom dock reserve；Explorer 改用 dock；auto-hide 可第二步。
3. **L1 diagnostics**：单文档 LSP 完成，状态可见，`F8` 跳转。
4. **阶段三编辑动作首批**：低风险接 `editor-core` 已有命令，快速提升可用性。
5. **L2 code action**：单文档 edit + execute command；跨文件 edit 显式提示不支持。
6. **C4 menu/statusbar**：视觉翻新，并把 diagnostics summary 接入 statusbar。
7. **C3 key sequence + command registry**：为 command palette 和高级快捷键铺路。
8. **Pickers / command palette**：先 file picker + command palette，再 symbols/search。
9. **Workspace/LSP refactor**：引入 `editor_core::Workspace` + `LspWorkspaceSync`，解锁 L3 rename / workspace symbols / cross-file edits。
10. **F-FT file tree 完整体验**：context menu、inline rename、多选、drag move、git status。

## 4. 验证命令

每个任务至少运行：

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```

针对局部任务可先运行：

```bash
cargo test -p atto-ui-editor
cargo test -p atto-editor-app
cargo test -p atto-ui-file-tree
cargo test --test pty_desktop
```

PTY 测试原则：

- 使用固定 terminal size。
- 使用 `wait_for_text(...)`，不要 ad-hoc sleep。
- 对 UI 状态断言屏幕 buffer 中稳定文本，例如 `E:1`, action title, picker title。

## 5. 关键风险与处理

| 风险 | 处理 |
|---|---|
| Component trait 扩展破坏大量实现 | DragAndDrop 提供默认 impl，并用 `impl_component_default_traits!` 批量覆盖 |
| Dock reserve 与现有 maximize/move/resize 冲突 | 所有 normalize/move/maximize bounds 改用 `effective_work_area(bounds)` |
| Rename 跨文件 edit 静默丢失 | L2/L3 前单文档只应用当前 URI；跨 URI 明确提示 skipped |
| 多 split 重复 LSP | 当前 secondary 已禁用 LSP；workspace refactor 后每 `(root, language)` 只保留一个 `LspWorkspaceSync` |
| Inlay hints 需要 composed grid | L6 明确切换 `render_text` 数据源；不要把 virtual text 伪装成普通 text edit |
| File tree 文件操作误覆盖 | MVP 禁止覆盖并提示；后续加确认 dialog |
| Shell git status 阻塞 UI | 后台/节流刷新；draw 只读缓存 |

