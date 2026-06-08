# atto-editor-app 全功能编辑器任务列表

> 来源：`PLAN-2.md`（基于 `EDITOR_APP.md`，设计日期 2026-06-07）
> 说明：每个「实现任务」(T) 后紧跟一个「审阅任务」(R)，R 用于审阅前一个 T 的质量、正确性与测试覆盖。
> 通用要求（每个 T 完成前必须满足）：`cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test` 全绿；若任务只改文档可跳过构建。
> 代码定位：行号会随实现漂移，执行时以本文列出的文件路径、类型名、函数名和 `PLAN-2.md` 对应章节为准。
> editor-core 参考源码：`../editor-core`。当前本仓库各 editor crate 依赖 `editor-core = 0.4.1` / `editor-core-lsp = 0.4.1`，优先把 `../editor-core` 当 API 依据；除非任务明确要求，不要修改 `../editor-core`。

---

## 阶段一：框架底座 + editor 快速增益

### [DONE] T1 — C1 通用拖拽数据模型与 Component hooks

**依赖**：无。

**文件**：
- 新增 `src/composable/drag.rs`
- 修改 `src/composable/mod.rs`
- 修改 `src/composable/component.rs`
- 检查所有手写 `impl Component` 的类型，必要时补 `impl DragAndDrop for ... {}` 或更新 `impl_component_default_traits!` 调用

**相关现状**：
- `src/composable/component.rs` 当前核心类型：`ComponentContext`, `EventResult`, `ComponentAction`, `Component`。
- `Component` 当前约束为 `Layout + Scrollable + FocusNav + DynamicTree + EventHandling + Send`。
- 现有局部拖拽只在滚动条 / splitter 内部实现，不是跨组件通用 drag/drop。

**步骤**：
1. 新增 `src/composable/drag.rs`，定义：
   - `DragPayloadType(pub &'static str)`
   - `DragPayload::{Text, FilePath, ComponentId, WindowId, Custom { ty, data }}`
   - `DragOperation::{Copy, Move, Link}`
   - `DragSource { payload, operation, threshold, ghost }`
   - `DragOffer<'a> { payload, operation, screen_x, screen_y }`
   - `DropEffect::{None, Copy, Move, Link}`
   - `DropFeedback { effect, rect, label }`
   - `DragContext<'a> { payload, operation, source_window }`
2. 在 `src/composable/mod.rs` re-export 上述类型。
3. 在 `component.rs` 新增 trait：
   - `DragAndDrop::drag_source_at(...) -> Option<DragSource>`
   - `DragAndDrop::drag_over(...) -> DropFeedback`
   - `DragAndDrop::drop(...) -> EventResult`
   - `DragAndDrop::drag_cancelled(...)`
   - 所有方法必须有 no-op 默认实现。
4. 扩展 `ComponentContext<'a>`，增加 `pub drag: Option<DragContext<'a>>`。所有构造 `ComponentContext` 的位置必须显式填 `drag: None`，后续 T2 再填 active drag。
5. 扩展 `Component` trait 约束，把 `DragAndDrop` 纳入 supertraits。
6. 更新 `impl_component_default_traits!` 宏说明和使用方式；让简单组件可以通过宏补默认 trait。
7. 使用 `rg "impl .*Component for|ComponentContext \\{" src crates tests examples` 检查所有构造点与实现点，不要遗漏 workspace crates（`atto-ui-editor`, `atto-ui-file-tree`, `atto-editor-app`, `atto-ui-chat`, `atto-ui-markdown`, `atto-ui-terminal`）。

**测试**：
- 新增 `src/composable/drag.rs` 内简单单元测试，确认默认 `DropFeedback` 为 reject / none。
- 编译即覆盖 trait 迁移完整性。

**验收**：
- 全 workspace 编译通过。
- 现有滚动条、splitter 局部拖拽行为无变化。

**完成记录（2026-06-08）**：
- 新增 `src/composable/drag.rs`，定义通用拖拽 payload、operation、source、offer、drop feedback 与 drag context 类型，并从 `src/composable/mod.rs` re-export。
- 在 `ComponentContext` 增加 `drag: Option<DragContext<'_>>`，所有现有构造点显式设置 `drag: None`，为 T2 active drag 注入保留入口。
- 新增 `DragAndDrop` supertrait，提供 `drag_source_at`、`drag_over`、`drop`、`drag_cancelled` 的 no-op 默认实现，并将其纳入 `Component` 约束。
- 更新 `impl_component_default_traits!` 宏，使简单组件自动获得默认 no-op drag/drop hooks；补齐 workspace、demos、examples、tests 中手写 `impl Component` 类型的默认 `DragAndDrop` 实现。
- 新增 `src/composable/drag.rs` 单元测试，覆盖默认 `DropFeedback` reject/none 以及默认 `drag_over` 行为。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test`。

### [DONE] R1 — 审阅 T1

审阅 T1 改动：
- 确认 `DragAndDrop` 是默认 no-op，未强迫所有组件写业务逻辑。
- 确认所有 `ComponentContext` 构造点都设置了 `drag` 字段，且未用不安全占位或 panic。
- 确认 public re-export 合理，不泄露 wm 内部私有类型。
- 运行 `cargo check --workspace --all-targets`、`cargo clippy --workspace --all-targets -- -D warnings`。

**完成记录（2026-06-08）**：
- 已审阅 T1 拖拽基础类型、`DragAndDrop` supertrait、`ComponentContext.drag` 字段、`composable` re-export、workspace 中的 `ComponentContext` 构造点和手写 `impl Component` 类型补齐情况。
- 确认 `DragAndDrop` 默认实现均为 no-op / reject，未强迫普通组件写业务拖拽逻辑；`ComponentContext` 构造点已显式设置 `drag: None`，未发现 panic、不安全占位或 unsafe；公开导出只暴露 composable 拖拽类型与既有 public `WindowId`。
- 未发现需要修改代码的问题。
- 验证通过：`cargo fmt`；`cargo check --workspace --all-targets`；`cargo clippy --workspace --all-targets -- -D warnings`。

### [DONE] T2 — C1 WindowManager 全局拖拽会话与反馈绘制

**依赖**：T1。

**文件**：
- `src/wm/manager/types.rs`
- `src/wm/manager/events.rs`
- `src/wm/manager/draw.rs`
- `src/wm/manager/mod.rs`（如需引入新子模块）
- `src/theme/mod.rs`

**相关现状**：
- `WindowManager` 当前字段：`windows`, `window_index`, `focused`, `drag: Option<DragState>`, `mouse_capture`。
- `DragState` / `DragKind` 目前只覆盖 window move、resize、scrollbar。
- `dispatch_to_window_view` 当前构造 `ComponentContext`，并阻止 chrome/border 事件进入 view。

**步骤**：
1. 在 `types.rs` 新增 `GlobalDragState`：
   - `source_window`
   - `source_component: Option<ComponentId>`
   - `start_x/start_y`
   - `last_x/last_y`
   - `source: DragSource`
   - `active: bool`
   - `feedback: Option<DropFeedback>`
   - `target_window: Option<WindowId>`
2. 给 `WindowManager` 增加 `global_drag: Option<GlobalDragState>`。
3. 修改 `handle_mouse`：
   - `Down(Left)` 命中 `HitRegion::Body` 时，先按现有逻辑 focus，再向 view 查询 `drag_source_at`；写入 `global_drag` 但 `active=false`。
   - chrome hit（titlebar、resize、scrollbar、buttons）继续走现有 `drag`，不要启动 component drag。
   - `Drag(Left)` / `Moved`：如果 `global_drag` 存在，超过 `DragSource.threshold` 后置 `active=true`；用 `window_at(m.column, m.row)` 找 target；对 target view 调 `drag_over` 并保存 feedback；返回 consumed。
   - `Up(Left)`：如果 active 且 target feedback effect 不是 `None`，调用 target view `drop`；否则调用 source view `drag_cancelled`；清理 `global_drag`。
   - `Esc` 键取消 active/pending drag。
4. active drag 期间向 target/source 构造 `ComponentContext { drag: Some(...) }`；非 drag 路径保持 `None`。
5. 在 `draw.rs` 所有窗口绘制后叠加：
   - ghost 文本：`DragSource.ghost` 或 payload fallback label。
   - `DropFeedback.rect` 高亮：用 `drop-target-active` 或 `drop-target-reject` named style。
6. 在 `theme/mod.rs` 的 `populate_named_styles` 注册：
   - `drag-ghost`
   - `drop-target-active`
   - `drop-target-reject`
   - `drop-insertion-marker`
   - 如需要读取 typed field，可优先用 `named_style` 避免扩 `Theme` 字段。
7. 保持现有 `drag: Option<DragState>` 语义不变；不要把 window move/resize 混入 component drag。

**测试**：
- 在 `src/wm/manager/tests.rs` 增加测试组件：
  - 未超过 threshold 不触发 active drag。
  - 超过 threshold 后 target 收到 `drag_over`。
  - drop 到 reject target 时 source 收到 cancel。
- 新增 `tests/pty_drag_drop.rs` 或扩展 snapshot fixture：
  - 两个窗口，左侧 source 拖到右侧 target，屏幕出现 `Dropped: ...`。
  - Esc cancel 后不出现 drop 文本。

**验收**：
- drag 期间普通 hover/click 不误触发。
- window titlebar 移动、resize handle、window scrollbar 拖动不回归。

**完成记录（2026-06-08）**：
- 在 `WindowManager` 增加 `global_drag: Option<GlobalDragState>`，保留原有 `drag: Option<DragState>` 专用于 window move/resize/window scrollbar，component drag 不混入 chrome drag 状态。
- `Down(Left)` 命中窗口 body 时聚焦窗口并查询 `drag_source_at`，记录 pending drag；`Drag(Left)`/`Moved` 达到 threshold 后激活，按 `window_at` 定位 target 并调用 `drag_over` 保存 feedback；`Up(Left)` 对接受的 target 调 `drop`，拒绝/无 target 时通知 source `drag_cancelled`；`Esc` 可取消 pending/active drag。
- active drag 期间 source/target 的 `ComponentContext.drag` 会传入 `DragContext`；普通 dispatch/draw 路径仍保持 `drag: None`。
- 修复 `WindowMinSizeView` 对 drag/drop hooks 的转发，并在 overflow 内部转发时保留 `ctx.drag`，避免 `Window::new` wrapper 吞掉组件拖拽能力。
- 在窗口绘制完成后叠加 drop feedback rect 与 ghost 文本；新增 `drag-ghost`、`drop-target-active`、`drop-target-reject`、`drop-insertion-marker` named styles。
- 新增 `src/wm/manager/tests.rs` 单元覆盖：threshold 未达到不激活、不触发 target；threshold 达到后 target 收到 `drag_over`；reject target drop 时 source 收到 cancel。
- 新增 `snapshot_app --drag-drop` fixture 与 `tests/pty_drag_drop.rs`，覆盖双窗口拖拽显示 `Dropped: drag-item`，以及 `Esc` cancel 后不 drop。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test`。

### [DONE] R2 — 审阅 T2

审阅 T2 改动：
- 确认 `global_drag` 与现有 WM chrome `drag` 优先级清晰，不互相覆盖。
- 确认 `Up`、`Esc`、source/target window 被关闭时都能清理 drag 状态。
- 确认 ghost/drop feedback 绘制在所有窗口之上且不会 panic 于 0 宽/高 rect。
- 确认测试真实经过 `WindowManager::handle_mouse`，不是直接调用组件方法。

**完成记录（2026-06-08）**：
- 已审阅 `global_drag` 与既有 WM chrome `drag` 的优先级：component drag 只从窗口 body 启动，titlebar、resize handle 与 window scrollbar 仍走原 `drag` 路径，active `global_drag` 在 mouse drag/move/up 期间优先处理。
- 发现并修复 Desktop chrome 抢先消费全局拖拽 mouse `Up` 的问题：当 `global_drag` 存在时，Desktop 先把鼠标事件和 `Esc` 交给 `WindowManager`，避免拖到 menu/status bar 释放后 drag 状态残留。
- 确认 `Up`、`Esc`、source/target window close 都会清理 `global_drag`；新增 Desktop 回归测试覆盖 status bar release，新增 WM 单测覆盖关闭 source/target window。
- 确认 ghost/drop feedback overlay 在所有窗口绘制后叠加，feedback rect 经过 clipping，0 宽/高 rect 不会绘制或 panic。
- 确认 T2 单测经由 `WindowManager::handle_event`/mouse event 路径，PTY 测试经真实 mouse 序列驱动 snapshot app，而不是直接调用组件方法。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] T3 — C2 Docking 类型、work area reserve 与基础绘制

**依赖**：无；可与 T1/T2 并行，但与 T5 Explorer 迁移有依赖。

**文件**：
- `src/wm/window.rs`
- `src/wm/manager/types.rs`
- 新增 `src/wm/manager/docking.rs`
- `src/wm/manager/mod.rs`
- `src/wm/manager/core.rs`
- `src/wm/manager/draw.rs`
- `src/wm/manager/placement.rs`
- `src/app/desktop.rs`
- `src/lib.rs`

**相关现状**：
- `Window` 只有 `movable`, `resizable`, `rect`, `state`，没有 dock 状态。
- `Desktop::layout(screen).work_area` 只扣 menu/statusbar。
- `WindowManager::draw(bounds, theme)` 对 maximized window 使用传入 bounds。

**步骤**：
1. 在 `window.rs` 定义并 re-export：
   - `DockSide::{Left, Right, Bottom, Top}`
   - `DockAutoHide::{Disabled, Enabled { visible: bool }}`
   - `WindowDock { side, size, min_size, max_size, auto_hide, handle_label }`
2. `Window` 增加 `pub dock: Binding<Option<WindowDock>>`，`Window::new` 默认 `None`。
3. 增加 builder：
   - `Window::with_dock(...)`
   - `WindowDock::docked(side, size)` 或等价 constructor。
4. 新增 `src/wm/manager/docking.rs`：
   - `dock_rect(bounds, dock, reserved_work_area) -> Rect`
   - `reserve_for_docked_windows(windows, bounds) -> Rect`
   - `WindowManager::effective_work_area(bounds) -> Rect`
   - clamp 规则：`size` 在 `[min_size, max_size.unwrap_or(available)]`；Left/Right 扣 width，Bottom/Top 扣 height；auto-hide invisible 只 reserve 1 cell handle。
5. 修改 `WindowManager::add_window`：dock window rect 由 dock 计算；非 dock window normalize 到 `effective_work_area(bounds)`。
6. 修改 `WindowManager::draw`：
   - draw 前计算 dock window rect 并 `window.rect.set(rect)`。
   - 非 dock maximized window 使用 `effective_work_area(bounds)`。
   - 非 dock normal window normalize 到 `effective_work_area(bounds)`。
7. 修改 window move/resize/maximize 路径（`events.rs`、`placement.rs` 调用点）使用 `effective_work_area(bounds)`，避免普通窗口覆盖 dock reserve。
8. 在 `src/lib.rs` re-export `DockSide`, `DockAutoHide`, `WindowDock`。

**测试**：
- `src/wm/manager/tests.rs`：
  - left dock reserve 后 normal maximized window 的 rect.x >= dock right edge。
  - right dock/bottom dock reserve 正确。
  - dock window rect 不受原始 `rect` builder 值影响。
- 运行现有 window move/resize/maximize 相关测试，确认不回归。

**验收**：
- Docked window 可以绘制在 work area 边缘。
- 其他窗口 maximize 不覆盖 docked window。

**完成记录（2026-06-08）**：
- 新增 `DockSide`、`DockAutoHide`、`WindowDock` public API，并从 `src/wm/mod.rs` 与 `src/lib.rs` re-export；`Window` 增加 `dock: Binding<Option<WindowDock>>`，`Window::with_dock(...)` 与 `WindowDock::docked(side, size)` 可声明 docked window。
- 新增 `src/wm/manager/docking.rs`，实现 `dock_rect`、`reserve_for_docked_windows`、`WindowManager::effective_work_area(bounds)` 与 draw/event 前 dock layout 同步；Left/Right/Top/Bottom 按当前窗口顺序扣 reserve，`size` clamp 到 dock min/max/available，auto-hide invisible 只 reserve 1 cell handle。
- `WindowManager::add_window`、`draw`、view dispatch、component drag/drop 路径、window move/resize/maximize 路径均改用 dock-aware effective work area；dock window rect 由 dock layout 覆盖，默认 `movable=false`、`resizable=true`、`state=Normal`。
- 现有 titlebar move、普通角落 resize、minimize/maximize chrome 不再把 docked window 当普通 floating window 处理，为后续 T4 内侧边 dock resize 保留语义。
- 在 `src/wm/manager/tests.rs` 增加覆盖：left dock reserve 后 maximized normal window 不覆盖 dock；right + bottom dock reserve 顺序正确；dock rect 不受 builder 原始 rect 影响且可绘制到边缘；普通 move/resize clamp 到 dock reserve；auto-hide invisible dock 只 reserve 1 cell。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] R3 — 审阅 T3

审阅 T3 改动：
- 确认 dock reserve 只在 desktop work_area 内计算，不覆盖 menu/statusbar。
- 确认多个 dock window 的 reserve 顺序 deterministic。
- 确认 maximized/normal/floating/modal window 行为未被错误统一；modal 是否覆盖 dock 需有明确设计和测试。
- 确认 `WindowDock` public API 不暴露 manager 内部细节。

**完成记录（2026-06-08）**：
- 已审阅 T3 docking public API：`DockSide`、`DockAutoHide`、`WindowDock` 只暴露窗口 docking 配置，并从 `wm` / crate root re-export，未泄露 manager 内部 layout 或 hit-test 细节。
- 确认 dock reserve 由 `Desktop` 传入的 `work_area` 驱动，不覆盖 menu/statusbar；新增 `app::desktop::tests::dock_layout_is_confined_to_desktop_work_area` 固定该行为。
- 确认多个 dock window 按 `WindowManager` 当前窗口顺序扣 reserve，已有 `right_and_bottom_docks_reserve_work_area_in_order` 覆盖 deterministic reserve 顺序。
- 确认 docked window 与 normal/maximized window 路径未被错误统一：dock window 跳过普通 move/resize/maximize chrome，normal/maximized window 使用 dock-aware effective work area。
- 明确 modal 设计：modal 不覆盖 dock reserve，而是停留在 dock-reserved work area 内；active modal 期间 dock 保持可见但不参与 hit-test。新增 `wm::manager::tests::maximized_modal_uses_dock_reserved_work_area` 覆盖该设计。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] T4 — C2 Dock resize / auto-hide / hit-test

**依赖**：T3。

**文件**：
- `src/wm/manager/types.rs`
- `src/wm/manager/events.rs`
- `src/wm/manager/docking.rs`
- `src/wm/manager/draw.rs`
- `src/wm/manager/chrome.rs`（如 hit-test 需要复用 chrome helper）

**步骤**：
1. 扩展 `HitRegion`：
   - `DockResizeEdge(DockSide)`
   - `DockAutoHideHandle`
2. 扩展 `DragKind`：
   - `DockResize { start_size: u16, side: DockSide }`
3. 在 `docking.rs` 实现：
   - `dock_resize_edge_rect(window_rect, side) -> Rect`
   - `dock_handle_rect(bounds, dock) -> Rect`
4. 修改 hit-test：
   - Left dock 内侧边：`rect.x + rect.width - 1`
   - Right dock 内侧边：`rect.x`
   - Bottom dock 内侧边：`rect.y`
   - Top dock 内侧边：`rect.y + rect.height - 1`
5. 修改 `handle_mouse`：
   - `Down(Left)` 命中 `DockResizeEdge` 时设置 `DragKind::DockResize`。
   - `Drag(Left)` 时只更新 `WindowDock.size`，不要直接持久化 `rect.width/height`。
   - 点击 auto-hide handle 时切换 `DockAutoHide::Enabled { visible: true }`。
   - 点击 dock 以外或焦点离开时，把 visible 置回 false。
6. `draw.rs` 绘制 auto-hide handle；MVP 不做动画。

**测试**：
- `src/wm/manager/tests.rs`：
  - drag left dock 内侧边增减 `dock.size`。
  - auto-hide invisible 只 reserve 1 cell。
  - 点击 handle 后 visible true，点击外部后 false。

**验收**：
- Dock resize 不影响非 dock window 的 persisted rect。
- Auto-hide 不需要动画，但状态切换和 reserve 必须正确。

**完成记录（2026-06-08）**：
- 扩展 WM hit-test 与 drag 状态：新增 `DockResizeEdge(DockSide)`、`DockAutoHideHandle` 和 `DragKind::DockResize { start_size, side }`，dock resize 只更新 `WindowDock.size`，不直接持久化 window rect。
- 在 `docking.rs` 增加 dock 内侧 resize edge、auto-hide handle rect、resize size clamp、dock 专属可用区域计算和 auto-hide hide/show helper；Left/Right/Top/Bottom 均按内侧边规则计算。
- 调整 auto-hide reserve：`visible=false` 只 reserve 1 cell handle；`visible=true` 作为 overlay 绘制/命中，但 work area 仍只扣 handle，避免普通窗口 persisted rect 被 dock resize/auto-hide 状态污染。
- `handle_mouse` 支持点击 handle 显示 auto-hide dock、点击 dock 外或焦点离开隐藏；visible auto-hide dock 在 hit-test 中优先于普通窗口，避免事件穿透到被 overlay 遮住的窗口。
- `draw.rs` 绘制 auto-hide handle，hidden 状态不绘制 dock view；visible auto-hide overlay 在无 modal 时绘制到普通窗口之上。
- 新增 `dock-auto-hide-handle` named style，便于主题覆盖 handle 样式。
- 新增 WM 单元测试覆盖：left dock resize 更新 `dock.size` 且不改普通窗口 rect；hidden auto-hide handle click 后 visible=true 且 reserve 仍为 1 cell；点击外部后 hidden；visible auto-hide overlay 优先命中 dock。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] R4 — 审阅 T4

审阅 T4 改动：
- 确认每个 `DockSide` 的内侧 resize 边无 off-by-one。
- 确认 resize clamp 尊重 min/max/available。
- 确认 auto-hide visible=false 时不会把 view 画到不可见区域外。
- 确认鼠标事件不穿透到被 dock overlay 遮住的普通窗口。

**完成记录（2026-06-08）**：
- 已审阅 T4 的 dock resize / auto-hide / hit-test 实现：`DockResizeEdge` 对 Left/Right/Bottom/Top 均使用内侧边，resize 只写 `WindowDock.size` 并经 `clamp_dock_size` 限制到 min/max/available。
- 确认 auto-hide hidden 状态只 reserve/draw 1 cell handle，`WindowManager::draw` 在 `visible=false` 时跳过 dock view 绘制；visible overlay 在 hit-test 中优先于普通窗口，鼠标不会穿透到被遮住的 normal window。
- 新增 WM 回归测试覆盖：四个 `DockSide` 的 resize edge off-by-one、min/max/available clamp、hidden auto-hide 不绘制 view 且仍绘制 handle、visible auto-hide overlay mouse 不穿透 normal window。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] T5 — C2 atto-editor-app Explorer 改用 WM Docking

**依赖**：T3；若实现 auto-hide UI，则依赖 T4。

**文件**：
- `crates/atto-editor-app/src/app.rs`
- `crates/atto-editor-app/src/actions.rs`
- 相关测试：`crates/atto-editor-app/tests/*`

**相关现状**：
- `app.rs` 内部有 `ExplorerDock::{Left,Right}`、`default_explorer_rect`、`docked_explorer_rect`、`work_without_explorer`。
- 初始 Explorer 用 `Window::new(WindowKind::Normal, "Explorer", explorer_rect, ...)`。
- Editor 初始 rect 用 `work_without_explorer(work, explorer_rect, dock)` 手算。

**步骤**：
1. 删除或废弃 `ExplorerDock` 和手算 rect/work area helper：
   - `default_explorer_rect`
   - `docked_explorer_rect`
   - `work_without_explorer`
2. `AppState` 中 `explorer_dock` 改为 `DockSide` 或直接不保存，取窗口 `dock.side`。
3. 创建 Explorer window 时：
   - `rect` 可传 `Rect::default()` 或合理 fallback。
   - `.with_dock(Some(WindowDock { side: DockSide::Left, size: 34, min_size: 20, max_size: None, auto_hide: DockAutoHide::Disabled, handle_label: Some("Explorer".into()) }))`
   - `.with_tag("atto-editor-app-explorer")` 保持。
4. `AppAction::ExplorerLeft/ExplorerRight` 改为更新 `w.dock` 中 `side`，不要直接写 `w.rect`。
5. 初始 editor window 继续用 `default_editor_rect(Desktop::layout(screen).work_area, offset)`，让 WM clamp 到 effective work area。
6. 清理 `explorer_rect` 的语义：如果保留，只记录上次 dock size；不要保存 dock rect。

**测试**：
- 新增 `crates/atto-editor-app/tests/explorer_docking.rs`：
  - 启动后 Explorer 在左侧，editor window 不覆盖 Explorer。
  - 触发 Dock Explorer Right 后 Explorer 到右侧。
  - resize terminal 后 dock reserve 仍正确。

**验收**：
- app 层不再手算 Explorer reserve。
- View 菜单中的 Explorer left/right 行为保持可用。

**完成记录（2026-06-08）**：
- `atto-editor-app` 的 Explorer 创建、toggle 与 Dock Explorer Left/Right 操作改用 WM `WindowDock`；Explorer window 以 `DockSide` + dock size 表达位置和宽度，不再写入手算 dock rect。
- 移除 app 层 `ExplorerDock`、`default_explorer_rect`、`docked_explorer_rect`、`work_without_explorer`；初始 editor window 使用完整 desktop work area，并交由 WM effective work area clamp，避免 app 手算 Explorer reserve。
- `AppState` 改为记录 `DockSide` 与上次 dock size；tick 中同步已打开 Explorer 的 dock side/size，保留 dock resize 后 close/reopen 的尺寸语义。
- 新增 `crates/atto-editor-app/tests/explorer_docking.rs`，覆盖 Explorer 左侧 dock reserve、Dock Explorer Right 后右侧 reserve、terminal resize 后 dock reserve 仍正确；补充 app 单测覆盖 dock helper/action 状态同步。
- 调整既有 Explorer PTY smoke 测试点击坐标，以匹配 WM dock 贴齐 work area 边缘后的文件树内容行。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] R5 — 审阅 T5

审阅 T5 改动：
- 确认 `atto-editor-app` 没有残留 `work_without_explorer` 逻辑。
- 确认 Explorer close/reopen 后 dock side/size 保持合理。
- 确认 `active_editor_commands` 在 Explorer focused 时仍能 fallback 到 last focused editor。
- 运行 `cargo test -p atto-editor-app` 和相关 PTY。

**完成记录（2026-06-08）**：
- 已审阅 T5 的 Explorer docking 迁移，确认 `atto-editor-app` 不再保留 `work_without_explorer`、`default_explorer_rect`、`docked_explorer_rect` 或 `ExplorerDock` 等 app 层手算 Explorer reserve 逻辑。
- 确认 Explorer 由 WM `WindowDock` 表达 side/size；`sync_explorer_dock_state`、`toggle_explorer_window` 与 `dock_explorer_window` 会同步并保留 close/reopen 后的 dock side/size。
- 确认 `active_editor_commands` 在 Explorer focused 时优先 fallback 到 `last_focused_editor`，并在该 editor 仍存在时投递编辑命令。
- 新增 app 单元回归测试覆盖 Explorer close/reopen 保留 dock side/size，以及 Explorer focused 时编辑命令 fallback 到 last focused editor。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-editor-app`；`cargo test --workspace --all-targets`。

### [DONE] T6 — 阶段三首批编辑动作接线

**依赖**：无。建议在 LSP 大任务前做，低风险提升编辑能力。

**文件**：
- `crates/atto-ui-editor/src/keymap.rs`
- `crates/atto-ui-editor/src/view/actions.rs`
- `crates/atto-ui-editor/src/view/input.rs`
- `crates/atto-ui-editor/src/config.rs`
- `crates/atto-editor-app/src/language.rs`
- `crates/atto-editor-app/src/window/document_tab.rs`

**editor-core API 参考**：
- `../editor-core/crates/editor-core/src/model.rs`
- `EditCommand::{Indent, Outdent, DuplicateLines, DeleteLines, MoveLinesUp, MoveLinesDown, JoinLines, SplitLine, ToggleComment}`
- `CursorCommand::{MoveWordLeft, MoveWordRight, MoveToMatchingBracket, AddCursorAbove, AddCursorBelow, AddNextOccurrence, AddAllOccurrences, ExpandSelection}`

**步骤**：
1. 扩展 `EditorAction`：
   - `MoveWordLeft`, `MoveWordRight`, `MoveToMatchingBracket`
   - `ToggleComment`, `JoinLines`, `MoveLinesUp`, `MoveLinesDown`, `DuplicateLines`, `DeleteLines`, `Indent`, `Outdent`, `SplitLine`
   - `AddCursorAbove`, `AddCursorBelow`, `AddNextOccurrence`, `AddAllOccurrences`, `ExpandSelection`
2. 在 `EditorKeymap::default_bindings` 添加默认键：
   - `Ctrl+Left/Right` 或 `Alt+Left/Right`：word move
   - `Ctrl+/`：toggle comment
   - `Alt+Up/Down`：move lines
   - `Shift+Alt+Down`：duplicate lines
   - `Ctrl+Alt+Up/Down`：add cursor above/below
   - `Ctrl+D`：add next occurrence
   - `Ctrl+Shift+L`：add all occurrences
   - matching bracket / join lines / split line 可先只通过 command palette 后续暴露，如直接绑定需避免与现有快捷键冲突。
3. `EditorConfig` 增加 `comment: Binding<Option<editor_core_lang::CommentConfig>>`。
4. `atto-editor-app/src/language.rs` 增加 `comment_config_for_language(language_id)`：
   - Rust/JS/TS/JSON/TOML/YAML/Python/Markdown 至少给 line comment（JSON 无 comment 可返回 None）。
   - 使用 `editor_core_lang::CommentConfig` 的实际构造 API，执行前查 `../editor-core/crates/editor-core-lang/src/lib.rs`。
5. `DocumentTabView::build_editor_view` 设置 `cfg.comment`。
6. `view/actions.rs` 中把新 action 映射到 `Command::Edit` / `Command::Cursor`。
7. 对会修改文本的 action 用 `execute_and_sync_text`，并确保 LSP didChange 逻辑与 Undo/Redo 类似：文本变更后更新 `config.text`、syntax、LSP full change。
8. `action_mutates_document(action)` 增加对应修改类 action，read-only 时禁止。

**测试**：
- `crates/atto-ui-editor/src/view/tests.rs`：
  - indent/outdent、duplicate/delete/move line、join/split line、multi-cursor occurrence。
- PTY：
  - 在 `snapshot_editor_app` 或 app 测试中按 `Ctrl+/`，Rust 文件行注释切换。

**验收**：
- 所有新增 action 在 read-only 下不会修改文本。
- 所有文本修改同步到 `config.text`，保存时能写入新内容。

**完成记录（2026-06-08）**：
- 扩展 `EditorAction` 并接线到 `editor-core` 的 word movement、matching bracket、line edit、toggle comment、indent/outdent/split line、多光标 occurrence/selection expansion 等命令。
- `EditorKeymap::default_bindings` 新增 Ctrl+Left/Right、Ctrl+/（含 raw `0x1f`/Ctrl+7 兼容）、Alt+Up/Down、Shift+Alt+Down、Ctrl+Alt+Up/Down、Ctrl+D、Ctrl+Shift+L 等默认绑定，未覆盖既有 Copy/Paste/Find/Fold/LSP goto 绑定。
- `EditorConfig` 增加 `comment: Binding<Option<CommentConfig>>`；`atto-editor-app` 按语言注入 Rust/JS/TS/TOML/YAML/Python/Markdown 注释配置，JSON/plaintext 安全 no-op。
- 新增 full-document edit 同步 helper；所有新增文本修改 action 均经过 read-only gate，并在实际文本变化后同步 `config.text`、刷新 syntax、发送 LSP full didChange。
- `snapshot_editor_app` 增加 Rust comment fixture；新增 view 单测覆盖 indent/outdent、duplicate/delete/move line、join/split line、toggle comment、多光标 occurrence、read-only gate 与默认 keymap；新增 PTY 覆盖 Ctrl+/ 行注释切换。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] R6 — 审阅 T6

审阅 T6 改动：
- 确认所有 mutating action 都走 read-only gate。
- 确认所有文本变更都同步 binding、syntax、LSP didChange。
- 确认默认键不覆盖已有 Copy/Paste/Find/Fold/LSP goto 等绑定。
- 确认 comment config 对不支持注释的语言安全 no-op，而不是 panic。

**完成记录（2026-06-08）**：
- 已审阅 T6 的 `EditorAction` 扩展、默认 keymap、`handle_action` 分发、full-document edit sync helper、`EditorConfig.comment`、app language comment config 与 `DocumentTabView` 注入路径。
- 确认 mutating actions 统一经过 `action_mutates_document` + `read_only` gate；新增 Indent/Outdent/SplitLine/ToggleComment/JoinLines/MoveLinesUp/MoveLinesDown/DuplicateLines/DeleteLines 均在 read-only 下拒绝修改。
- 确认新增文本修改 action 均经 `execute_full_document_edit_and_sync`，实际文本变化后同步 `config.text`、刷新 syntax，并向 LSP 发送 full didChange；Undo/Redo 与既有插入/删除路径保持同步语义。
- 确认默认键保留既有 Copy/Paste/Find/Fold/LSP goto 绑定，新增 Ctrl+Left/Right、Ctrl+/、Alt+Up/Down、Shift+Alt+Down、Ctrl+Alt+Up/Down、Ctrl+D、Ctrl+Shift+L 未覆盖上述既有绑定。
- 确认 `comment_config_for_language` 对 JSON/plaintext/未知语言返回 `None`，`ToggleComment` 在无 comment config 或空 config 时安全 no-op，不 panic。
- 未发现需要修改代码的问题。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] T7 — L1 LSP diagnostics 数据接收与状态模型

**依赖**：无；如果要显示在 app statusbar，依赖 T10/T11 更完整。

**文件**：
- `crates/atto-ui-editor/src/view/mod.rs`
- `crates/atto-ui-editor/src/view/lsp.rs`
- `crates/atto-ui-editor/src/view/state.rs`
- `crates/atto-ui-editor/src/config.rs`
- `crates/atto-ui-editor/src/lib.rs`

**editor-core-lsp API 参考**：
- `../editor-core/crates/editor-core-lsp/src/lsp_events.rs`
- `LspEvent::{Notification, Response, DeferredRequest}`
- `LspNotification::PublishDiagnostics`
- `LspPublishDiagnosticsParams`, `LspDiagnostic`, `LspDiagnosticSeverity`
- `lsp_diagnostics_to_processing_edits`

**步骤**：
1. 新增 `DiagnosticsSummary { errors, warnings, infos, hints }`，并从 `lib.rs` re-export。
2. `EditorViewHandle` 增加 `diagnostics_summary: Binding<DiagnosticsSummary>`。
3. `EditorLspController` 增加：
   - `diagnostics: Vec<LspDiagnostic>`
   - `diagnostic_result_id: Option<String>`
   - `pending_document_diagnostic: Option<u64>`
   - `diagnostic_cursor: Option<usize>`
   - `diagnostics_revision: u64`
4. `EditorView::new` 创建 diagnostics binding 并放入 handle。
5. 重构 `maybe_poll_lsp`：
   - 不再只 `let LspEvent::Response(resp) = ev else { continue; }`。
   - match `Notification(PublishDiagnostics(params))`，调用 `apply_publish_diagnostics(params)`。
   - Response 交给现有 hover/completion/goto 分支。
   - DeferredRequest 暂时排队或安全忽略，但不要 panic；workspace/applyEdit 在 L2/L3 再处理。
6. `apply_publish_diagnostics`：
   - 用 `lsp.diagnostics_version_matches(&params)`（如 API 可见）过滤过期诊断；若不可见，至少按 document uri 匹配当前 `lsp.document().uri`。
   - 调 `lsp_diagnostics_to_processing_edits(self.state_manager.editor().line_index(), &params)`。
   - `state_manager.apply_processing_edits(edits)`。
   - 保存 `params.diagnostics` 到 controller，更新 summary binding。
7. 可选：实现 pull diagnostics request：
   - `request_document_diagnostic(previous_result_id)`。
   - response `textDocument/diagnostic` 解析 `result.items` 为 publish-like params。

**测试**：
- 扩展 `crates/atto-ui-editor/tests/lsp_editor.rs` 或新增 diagnostics 测试：
  - mock LSP 发 `publishDiagnostics`。
  - 断言 `diagnostics_summary.errors == 1`。
  - 断言 `state_manager.editor().diagnostics()` 有内容（可通过 test-only helper 暴露）。

**验收**：
- publish diagnostics 能进入 editor-core processing edits。
- hover/completion/goto response 行为不回归。

**完成记录（2026-06-08）**：
- 新增 `DiagnosticsSummary { errors, warnings, infos, hints }`，从 `atto-ui-editor` crate root re-export，并通过 `EditorViewHandle::diagnostics_summary` 暴露给宿主 UI。
- `EditorLspController` 增加 diagnostics 数据、result id、pending pull request、cursor 与 revision 状态；LSP session 出错、禁用或重启时会清理 diagnostics 状态与 summary。
- `maybe_poll_lsp` 改为显式 match `Notification` / `Response` / `DeferredRequest`，`publishDiagnostics` 会按当前 document URI/version 过滤后转换为 `editor-core` processing edits 并更新 summary；hover/completion/goto response 分支保持原行为。
- 扩展 mock LSP server，在 `file:///diagnostics.rs` 打开时发送 deterministic `publishDiagnostics`。
- 新增 editor view 单测覆盖 diagnostics summary 与 `state_manager.editor().diagnostics()`，新增 LSP integration 测试覆盖 mock `publishDiagnostics` 驱动 summary 更新。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] R7 — 审阅 T7

审阅 T7 改动：
- 确认 `maybe_poll_lsp` 不再丢弃 Notification/DeferredRequest。
- 确认 diagnostics uri/version 过滤不会把其他文档诊断写入当前 buffer。
- 确认 summary binding 只在值变化时更新，避免无意义 dirty。
- 确认 LSP session 出错时仍清理 diagnostics/style layer。

**完成记录（2026-06-08）**：
- 已审阅 T7 的 `DiagnosticsSummary` / `EditorViewHandle::diagnostics_summary`、`EditorLspController` diagnostics 状态、`maybe_poll_lsp` 事件分发、`publishDiagnostics` 应用路径、URI/version 过滤与 LSP 出错清理路径。
- 确认 `maybe_poll_lsp` 不再只处理 `Response`：`Notification(PublishDiagnostics)` 会更新 editor-core diagnostics 与 summary，其他 notification 和 `DeferredRequest` 当前安全忽略且不 panic，hover/completion/goto response 分支保持原行为。
- 确认 diagnostics 只接受当前 document URI，带 version 的 payload 必须匹配当前 document version；其他文档或 stale version 不会写入当前 buffer。
- 确认 `set_diagnostics_summary` 仅在值变化时写 binding，避免无意义 dirty。
- 发现并修复 `clear_lsp_diagnostics` 的短路清理问题：原实现用 `||` 串联带副作用的 `.take()`，已有 diagnostics 时可能跳过 `diagnostic_result_id`、`pending_document_diagnostic`、`diagnostic_cursor` 清理；已改为先分别清理再合并状态，并新增回归测试 `clear_lsp_diagnostics_clears_all_controller_state`。
- 确认 LSP `didChange`/poll/config restart/disable 错误路径调用 `clear_lsp_state` 清理 diagnostics style layer 与 core diagnostics，并重置 editor 侧 summary/controller diagnostics 状态。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] T8 — L1 diagnostics gutter/statusbar 渲染与 F8 跳转

**依赖**：T7。Statusbar 分段可先用旧 `set_left/set_right`，完整接入依赖 T11。

**文件**：
- `crates/atto-ui-editor/src/keymap.rs`
- `crates/atto-ui-editor/src/view/actions.rs`
- `crates/atto-ui-editor/src/view/render.rs`
- `crates/atto-ui-editor/src/theme.rs`
- `crates/atto-editor-app/src/window/document_tab.rs`
- `crates/atto-editor-app/src/window/tabs.rs`
- `crates/atto-editor-app/src/app.rs`

**步骤**：
1. `EditorAction` 增加：
   - `LspNextDiagnostic`
   - `LspPrevDiagnostic`
2. 默认键位：
   - `F8` -> next diagnostic
   - `Shift+F8` -> prev diagnostic
3. 在 `view/actions.rs` 实现 `jump_to_diagnostic(direction)`：
   - 取 `state_manager.editor().diagnostics()`。
   - 当前 offset 用现有 `cursor_offset()`。
   - next 找 `range.start > current`，否则 wrap 到第一个。
   - prev 找 `< current` 的最后一个，否则 wrap 到最后。
   - 用 `LineIndex` offset->position 方法移动 cursor；执行后 `adjust_scroll()`。
4. `render.rs::layout_rects` 在 diagnostics enabled 时给 gutter 额外 2 列。
5. `render_gutter`：
   - 构建 line -> highest severity map。
   - marker 使用 ASCII：`E`, `W`, `I`, `H`。
   - marker style 用 `EditorTheme` 新字段或 `style_ids` 映射。
6. `theme.rs`：
   - `EditorTheme` 增加 `diagnostic_error/warning/info/hint` 或在 `style_ids` 中映射 `editor-core-lsp` diagnostics style id。
   - style id 编码见 `../editor-core/crates/editor-core-lsp/src/editor.rs` `diagnostic_style_id`，当前约 `0x0400_0100 | severity_bits`。
7. `DocumentTabView` 不要丢弃 `EditorViewHandle`：
   - 保存 primary handle 的 `diagnostics_summary`。
   - `TabState` 或 `DocumentTabView` 暴露 active diagnostics summary。
8. `app.rs` on_tick 根据 active editor summary 更新 `desktop.status`（旧 StatusBar 可先 `set_right("E:1 W:0")`）。

**测试**：
- PTY：mock LSP diagnostics 后，屏幕 gutter 出现 `E`。
- `F8` 后 cursor/viewport 到诊断行；可通过 screen 或 test helper 断言。
- app statusbar 出现 `E:1 W:0`。

**验收**：
- 诊断 underline/style、gutter marker、summary 三者一致。
- 无 diagnostics 时 gutter 不额外占用空间，或占用行为有明确配置。

**完成记录（2026-06-08）**：
- `EditorAction` 新增 `LspNextDiagnostic` / `LspPrevDiagnostic`，默认键位 `F8` / `Shift+F8`；跳转按当前 cursor offset 查找下一/上一条 editor-core diagnostics，并支持 wrap-around 与滚动调整。
- diagnostics 渲染接入 gutter：有 diagnostics 时额外占用 2 列，按行显示最高严重级别 ASCII marker `E`/`W`/`I`/`H`；wrapped visual row 不重复显示 marker，无 diagnostics 时不额外占用 gutter。
- `EditorTheme` 增加 diagnostics severity styles，并映射 `editor-core-lsp` 的 `0x0400_0100 | severity_bits` style id，使文本诊断 style、gutter marker 与 summary severity 保持一致。
- `DocumentTabView` 保留 primary `EditorViewHandle` 的 `diagnostics_summary`；`EditorWindowView` 汇总 active tab summary；`atto-editor-app` on_tick 将 active/last-focused editor diagnostics summary 写入旧 `StatusBar` custom right text（如 `E:1 W:0 I:0 H:0`），Explorer focused 时仍使用 last focused editor。
- `snapshot_editor_app --diagnostics` 使用 mock LSP 发布 deterministic diagnostics；新增/扩展测试覆盖 mock LSP gutter `E`、F8 跳回诊断行、诊断 gutter marker/style、F8 wrap-around、app statusbar last-focused fallback。
- 修复同文件 PTY 测试在并行负载下 2 秒初始文本等待偶发超时的问题：`pty_editor.rs` 文本等待统一使用 5 秒 `PTY_WAIT`，单个测试仍远低于 1 分钟。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-editor --test pty_editor`；`cargo test --workspace --all-targets`。

### [DONE] R8 — 审阅 T8

审阅 T8 改动：
- 确认 gutter 额外列与 line number/folding marker 的宽度计算一致，无覆盖文本首列。
- 确认 wrap line 不重复显示主行 marker，或行为明确。
- 确认 F8/Shift+F8 wrap-around 正确。
- 确认 statusbar 在非 editor focused（Explorer focused）时仍显示 last focused editor 的 diagnostics。

**完成记录（2026-06-08）**：
- 已审阅 T8 的 diagnostics gutter 宽度计算、`render_gutter` marker 绘制、`F8`/`Shift+F8` 诊断跳转、`EditorTheme` diagnostics style id 映射，以及 `atto-editor-app` active/last-focused editor diagnostics summary 到 statusbar 的传播路径。
- 确认 diagnostics gutter 额外列与 line number/folding marker 使用同一 `layout_rects` 宽度模型，未覆盖文本首列；无 diagnostics 时不额外占用 diagnostics gutter 列。
- 确认 wrapped visual row 不重复显示主逻辑行 diagnostics marker；新增 `editor_view_does_not_repeat_diagnostic_marker_on_wrapped_rows` 回归测试固定 continuation row 保留 gutter 空白、separator 与文本起始列。
- 确认 `F8`/`Shift+F8` 按 diagnostics start offset 排序并正确 wrap-around；已有单测覆盖 next/prev wrap，PTY 覆盖 F8 从远端 viewport 跳回诊断行。
- 确认 Explorer focused 时 statusbar 使用 `last_focused_editor` 的 diagnostics summary，且已有 app 单测覆盖 `E:1 W:2 I:3 H:4` fallback 渲染。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] T9 — L2 Code Action 请求、列表 popup 与单文档应用

**依赖**：T7；建议 T8 后做。

**文件**：
- `crates/atto-ui-editor/src/keymap.rs`
- `crates/atto-ui-editor/src/view/mod.rs`
- `crates/atto-ui-editor/src/view/lsp.rs`
- `crates/atto-ui-editor/src/view/actions.rs`
- `crates/atto-ui-editor/src/view/input.rs`
- `crates/atto-ui-editor/src/view/render.rs`
- `crates/atto-ui-editor/src/popup.rs`
- `crates/atto-ui-editor/src/lib.rs`

**editor-core-lsp API 参考**：
- `LspSession::request_code_action`
- `code_action_items_from_value`
- `apply_plan_for_code_action_item`
- `LspSession::apply_workspace_edit`
- `LspSession::request_execute_command`
- `summarize_workspace_edit`

**步骤**：
1. `EditorAction::LspCodeAction`，默认键 `Ctrl+.`。
2. `EditorLspController` 增加 `pending_code_action: Option<u64>` 和 `code_action_items: Vec<LspCodeActionItem>`。
3. `popup.rs` 新增：
   - `CodeActionPopupModel { rect, items, selected, scroll, accept }`
   - `CodeActionItemView { title, kind, is_preferred }`
4. `EditorView` 增加 `code_action_popup: Binding<Option<CodeActionPopupModel>>`。
5. 请求逻辑：
   - 取当前 selection offsets；无 selection 时用 cursor offset 的空 range。
   - context 初期可 `json!({ "diagnostics": [] })`；若 T7 保存了可转 JSON 的 diagnostics，则传 overlap diagnostics。
   - 调 `lsp.request_code_action(line_index, start, end, context)`。
6. Response：
   - method `textDocument/codeAction` 且 id match 时，`code_action_items_from_value(result)`。
   - 生成 popup；preferred action 排前或标记 `*`。
7. 输入：
   - popup 打开时 Up/Down/PageUp/PageDown/Enter/Esc 行为与 completion 一致。
   - 鼠标点击可后续补；MVP 至少 keyboard。
8. Apply：
   - `apply_plan_for_code_action_item`。
   - `plan.edit`：先用 `summarize_workspace_edit` 检查是否只涉及当前 uri；跨 URI 时通过 `EditorEvent` 或 popup/status 明确提示 skipped，不要静默丢弃。
   - 单文档 edit 用 `lsp.apply_workspace_edit(&mut state_manager, &edit)` 或当前 uri 的 `workspace_edit_text_edits_for_uri` + `apply_text_edits`。
   - 应用后更新 `config.text`、syntax、LSP didChange。
   - `plan.command`：调用 `request_execute_command(command, arguments)`。

**测试**：
- Mock LSP 返回 code action title，`Ctrl+.` 后 popup 显示。
- Enter 应用 edit，文本改变。
- 返回跨文件 edit 时不改文本，并显示 skipped/unsupported 提示。

**验收**：
- Popup 与 completion/hover 不互相遮挡；Esc 能关闭。
- Code action 应用后 undo 可恢复（单文档 edit 应走 editor-core edit path）。

**完成记录（2026-06-08）**：
- `EditorAction` 新增 `LspCodeAction`，默认键位为 `Ctrl+.`；请求使用当前 selection offsets，无 selection 时使用 cursor 空 range，并通过 `textDocument/codeAction` 发送空 diagnostics context。
- `EditorLspController` 增加 `pending_code_action` 与 `code_action_items`；LSP response 通过 `code_action_items_from_value` 解析，preferred action 排前并以 `*` 标记，生成 keyboard code action popup。
- 新增 `CodeActionPopupModel` / `CodeActionItemView`、`EditorViewHandle.code_action_popup` 与 inline/tooltip popup 绘制；Up/Down/PageUp/PageDown/Enter/Esc 行为与 completion popup 对齐，打开 code action 时会关闭 hover/completion 并清理 pending completion。
- Enter 应用 `apply_plan_for_code_action_item`：当前 URI 的 WorkspaceEdit 走 `LspSession::apply_workspace_edit` / editor-core edit path，随后同步 `config.text`、syntax、scroll 与 LSP didChange；command-only 或 edit 成功后的 command 通过 `request_execute_command` 执行。
- 跨 URI WorkspaceEdit 不做静默部分应用，保持文本不变并通过 `EditorEvent::CodeActionMessage` 报告 skipped/unsupported URI。
- mock LSP 增加 codeAction/executeCommand 支持；集成测试覆盖 popup title/kind/preferred 标记、Enter 应用单文档 edit、undo 恢复，以及跨文件 edit 跳过并发出事件。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] R9 — 审阅 T9

审阅 T9 改动：
- 确认跨文件 WorkspaceEdit 没有被静默部分应用。
- 确认 code action command 即使没有 edit 也能 execute。
- 确认 popup keyboard 与 completion popup 不冲突。
- 确认应用 edit 后 LSP didChange 和 syntax refresh 都发生。

**完成记录（2026-06-08）**：
- 已审阅 T9 的 code action request/response、popup keyboard 分发、WorkspaceEdit 应用、command 执行、LSP didChange 与 syntax refresh 路径。
- 确认跨文件 WorkspaceEdit 会整体跳过并发出 `CodeActionMessage`，不会静默部分应用当前文件 edits。
- 确认 code action popup 打开时优先处理 Up/Down/PageUp/PageDown/Enter/Esc，发起 code action 会关闭 completion，发起 completion 也会关闭 code action，二者 keyboard 路径不冲突。
- 确认单文档 edit 应用后会同步 `config.text`、刷新 syntax、调整滚动，并发送 full-document LSP didChange；已有测试覆盖 edit 后 undo 可恢复。
- 补充 mock LSP 与集成测试 `lsp_code_action_command_without_edit_executes`，覆盖无 edit 的 command-only code action 仍会发送 `workspace/executeCommand`。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

---

## 阶段二：界面统一、快捷键与 pickers

### [DONE] T10 — C4 MenuBar mnemonic/accelerator 与 Turbo Vision 绘制

**依赖**：无。

**文件**：
- `src/app/menu/model.rs`
- `src/app/menu/input.rs`
- `src/app/menu/draw.rs`
- `src/app/menu/layout.rs`
- `src/theme/mod.rs`
- `crates/atto-editor-app/src/app.rs`（菜单构建用新 API）

**相关现状**：
- `MenuItem.shortcut` 当前既显示 `Ctrl+S`，又在 `handle_shortcut_char` 中当单字符助记键使用，语义混杂。
- `draw.rs` 已有下拉菜单边框/阴影和 accelerator 右对齐基础。

**步骤**：
1. `MenuItem` 新增字段：
   - `accelerator: Binding<Option<String>>`
   - `mnemonic: Binding<Option<char>>`
2. 保持旧 `shortcut()` builder 兼容：
   - 推荐新增 `.accelerator("Ctrl+S")` 和 `.mnemonic('S')`。
   - 旧 `.shortcut()` 可暂时设置 `accelerator`；若传入单字符，可同时设置 mnemonic，需写清兼容规则。
3. `handle_shortcut_char` 优先匹配 `mnemonic`；没有 mnemonic 时 fallback 到 label 首字符，避免旧菜单失效。
4. `draw.rs` 支持 label 中 `&File` 或 `_File` 标记 mnemonic：
   - 绘制时不显示 `&`/`_`。
   - mnemonic 字符用 `theme.named_style("menu-mnemonic")` 或 `theme.status_bar_key`。
5. 下拉菜单绘制：
   - label、accelerator、submenu arrow 三段布局明确。
   - disabled 用 `theme.widget.disabled`。
   - selected 用 `theme.menu_item_selected`。
6. `theme/mod.rs` 注册 named styles：
   - `menu-mnemonic`
   - `menu-item-shortcut`
   - `menu-border`
7. 更新 `atto-editor-app/src/app.rs::build_menu`，把 `shortcut("Ctrl+S")` 迁到 `.accelerator("Ctrl+S")`，为 File/View/Split 菜单设置 mnemonic。

**测试**：
- `src/app/menu/*` 单元：`&File` 绘制不含 `&`，mnemonic 命中。
- PTY：打开菜单，断言文本为 `File` 而不是 `&File`；按 mnemonic 激活对应项。

**验收**：
- 旧代码调用 `.shortcut()` 仍编译。
- 菜单 accelerator 显示不影响 mnemonic 输入。

**完成记录（2026-06-08）**：
- `MenuItem` 新增 `accelerator` 与 `mnemonic` binding，并提供 `.accelerator(...)`、`.accelerator_binding(...)`、`.mnemonic(...)`、`.mnemonic_binding(...)` builder；旧 `.shortcut(...)` 保持可用，设置 accelerator，单字符 shortcut 同步为 mnemonic。
- 菜单输入改为优先匹配 explicit mnemonic，其次匹配 label 中 `&` / `_` marker，最后 fallback 到显示 label 首字符；顶部 Alt mnemonic 同样使用 marker-aware label。
- 菜单绘制新增 marker stripping 与 mnemonic highlight：顶部菜单和下拉项绘制时不显示 `&` / `_`，下拉行按 label、accelerator、submenu arrow 分段，accelerator 右对齐且不参与 mnemonic 输入。
- `Theme` 注册 `menu-mnemonic`、`menu-item-shortcut`、`menu-border` named styles，绘制路径通过 named style 读取，支持主题 overlay 覆盖。
- `atto-editor-app` 菜单构建迁移为 `.accelerator("Ctrl+...")`，并用 `&File` / `&View` / `&Split` 设置顶部菜单 mnemonic；Node binding 菜单 JSON 同步支持 `accelerator` / `mnemonic` 字段。
- 新增/扩展单元测试覆盖 marker stripping、Unicode marker byte offset、mnemonic 命中、旧 `.shortcut("q")` 兼容、accelerator 显示；新增 PTY 覆盖菜单 marker 隐藏和 mnemonic 激活 Quit。
- 处理验证中观察到的 `pty_diff` 并行负载下初始 3 秒等待偶发空屏超时：统一该 fixture 的 PTY wait 为 5 秒，单个用例仍远低于 1 分钟。
- 验证通过：`cargo fmt`；`cargo clippy --all-targets -- -D warnings`；`cargo test --all --all-targets`。

### [DONE] R10 — 审阅 T10

审阅 T10 改动：
- 确认 `shortcut` 兼容路径不会改变现有 demo/menu 行为。
- 确认 Unicode label 下 mnemonic 绘制不会破坏列宽。
- 确认 dropdown width 计算包含 stripped label + accelerator + arrow。
- 确认主题 named styles 可由 JSON/YAML overlay 覆盖。

**完成记录（2026-06-09）**：
- 已审阅 T10 的 `MenuItem` shortcut/accelerator/mnemonic API、菜单输入匹配、marker-aware layout/draw、`atto-editor-app` 菜单迁移、Node binding 菜单 JSON 转换和主题 named style overlay 路径。
- 发现并修复旧 `shortcut` 兼容缺口：静态 `.shortcut("q")` 已设置 mnemonic，但公开 `shortcut` binding / `.shortcut_binding(...)` 的单字符动态路径不会再被 `handle_shortcut_char` 匹配；现改为 explicit mnemonic / label marker 优先，其次单字符 `shortcut` fallback，最后 label 首字符 fallback。
- 确认 Unicode mnemonic 绘制按 display columns 推进，不会把 accelerator 覆盖到 CJK 宽字符列；新增回归测试固定 `_文件` / `_打开` 下 accelerator 的列位置。
- 确认 dropdown width 计算使用 stripped label，并包含 accelerator 与 submenu arrow reserve；新增精确宽度测试覆盖 accelerator-only 与 submenu arrow 场景。
- 确认 `menu-mnemonic`、`menu-item-shortcut`、`menu-border` 注册为 named styles，且可经 JSON/YAML overlay 覆盖；新增主题 overlay 回归测试。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。

### [DONE] T11 — C4 分段式 StatusBar 与 editor diagnostics 接入

**依赖**：T8 可提供 editor diagnostics summary；没有 T8 时先做 StatusBar API。

**文件**：
- `src/app/status.rs`
- `src/app/desktop.rs`
- `src/theme/mod.rs`
- `crates/atto-editor-app/src/app.rs`
- `crates/atto-editor-app/src/window.rs`
- `crates/atto-editor-app/src/window/tabs.rs`

**相关现状**：
- `StatusBar` 当前只有 `left: String`, `right: String`，`set_left`, `set_right`, `draw`。
- 旧实现已经修复 Unicode width，可复用 `UnicodeWidthStr` / grapheme 截断 helper。

**步骤**：
1. 新增：
   - `StatusSegmentAlign::{Left, Right}`
   - `StatusSegment { id, text: Binding<String>, style, align, min_width, priority, on_click }`
2. `StatusBar` 增加 `segments: Vec<StatusSegment>`，保留 `left/right` 兼容字段和 `set_left/set_right`。
3. 新增 API：
   - `set_segments(Vec<StatusSegment>)`
   - `push_segment(StatusSegment)`
   - `handle_mouse(&MouseEvent, area) -> EventResult`（点击 on_click）
4. `draw`：
   - 若 `segments.is_empty()`，走旧 left/right 逻辑。
   - left segments 从左到右，right segments 从右到左。
   - 宽度不足按 `priority` 隐藏；最后按 grapheme 截断。
   - segment separator 用 `theme.glyph("status-separator")` fallback `" "`.
5. `theme/mod.rs` 注册：
   - `status-bar`
   - `status-bar-key`
   - `status-segment`
   - `status-segment-warning`
   - `status-segment-error`
6. `Desktop` 事件分发中把 statusbar mouse click 路由到 `StatusBar::handle_mouse`。
7. `atto-editor-app` on_tick 更新 segments：
   - left：app name / active path / dirty marker。
   - right：diagnostics `E:n W:n`、language、`Ln x, Col y`、indentation、LSP status。
   - 短期如果 active editor status 暴露不足，先只接 diagnostics + language/path。

**测试**：
- `src/app/status.rs` 单元：
  - ASCII/CJK/emoji segment 对齐。
  - priority 隐藏。
  - click hit-test。
- PTY：
  - editor app 状态栏显示 diagnostics summary。

**验收**：
- `set_left/set_right` 旧调用仍能工作。
- statusbar 背景样式铺满整行。

**完成记录（2026-06-09）**：
- 新增 `StatusSegmentAlign` / `StatusSegment` API，`StatusBar` 支持 `set_segments`、`push_segment`、`clear_segments` 和 `handle_mouse`，同时保留 `set_left` / `set_right` / `set_custom` 的旧分支。
- 分段绘制支持 left/right 对齐、segment separator glyph、按 `priority` 隐藏低优先级段、按 grapheme 边界截断，并通过手写宽字符单元格渲染保持状态栏背景样式覆盖普通间隙和 segment 区域。
- `Desktop` 已将 status bar 区域鼠标 click 路由到 `StatusBar::handle_mouse`，仍保持 status bar click 不穿透到窗口 view。
- `Theme` 注册 `status-bar`、`status-bar-key`、`status-segment`、`status-segment-warning`、`status-segment-error` named styles，并新增 `status-separator` glyph fallback。
- `atto-editor-app` 增加 active editor status binding，在 tick 中以分段状态栏显示 app 名称、活动文件名、dirty marker、diagnostics `E:n W:n` 和 language；Explorer focused 时继续使用 last focused editor 状态。
- 新增/更新测试覆盖 ASCII/CJK/emoji 分段对齐、priority 隐藏、grapheme 截断、click hit-test、Desktop status click 路由、Explorer focused 状态栏 fallback，以及 editor app PTY diagnostics/language 显示。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。

### [DONE] R11 — 审阅 T11

审阅 T11 改动：
- 确认 segment truncation 在 grapheme 边界且列宽正确。
- 确认 click hit-test 与绘制坐标一致。
- 确认 `Desktop::layout` 不因 statusbar 内部分段而改变。
- 确认 editor app 在 Explorer focused 时仍显示 last focused editor 状态。

**完成记录（2026-06-09）**：
- 已审阅 T11 的 `StatusBar` segment API、priority layout、grapheme/列宽截断、手写宽字符绘制、Desktop status bar mouse 路由、主题 named styles/glyph 以及 `atto-editor-app` active/last-focused editor statusbar 接入。
- 确认 segment 截断经 grapheme 边界和 `UnicodeWidthStr` 计算，CJK/emoji 宽度测试覆盖仍通过，状态栏背景样式会填满 segment 和普通间隙。
- 发现并修复 click hit-test fallback 与绘制坐标不一致的问题：`set_segments` 清空 cached hit boxes 后，fallback 原固定使用 1 列 separator，遇到多列 `status-separator` glyph 会错算后续 segment 坐标；现缓存最近一次绘制的 separator 显示宽度并用于 fallback，新增回归测试覆盖多列 separator。
- 确认 `Desktop::layout` 仍只由 menu/status bar 固定高度决定，不依赖 statusbar 内部分段内容；status bar mouse click 被 Desktop chrome 消费，不穿透窗口 view。
- 确认 `atto-editor-app` 在 Explorer focused 时 `active_editor_status` / diagnostics summary 会回退到 `last_focused_editor`，既有单测覆盖 diagnostics/language fallback，PTY 覆盖 statusbar 显示。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。

### [DONE] T12 — C3 框架级多键序列 keymap engine

**依赖**：无；与 T13 command registry 配套。

**文件**：
- 新增 `src/app/keymap.rs` 或 `src/input/keymap.rs`（建议 `src/app/keymap.rs`）
- `src/app/mod.rs`
- `src/lib.rs`（如需要 re-export）
- `crates/atto-ui-editor/src/keymap.rs`（桥接）

**相关现状**：
- `atto-ui-editor/src/keymap.rs` 有自己的 `KeyChord` 和 `EditorKeymap(HashMap<KeyChord, EditorAction>)`，只支持单 chord。
- 框架层没有通用 key sequence。

**步骤**：
1. 新增框架 `KeyChord { code, modifiers }`，支持 `from_key_event`。
2. 新增 `KeySequence(Vec<KeyChord>)`。
3. 实现 trie / prefix state：
   - `KeySequenceEngine<A>`
   - pending sequence
   - timeout
   - `handle_key(chord, now) -> KeymapMatch<A>`
4. `KeymapMatch` 至少包含：
   - `None`
   - `Prefix { choices }`
   - `Exact(A)`
   - `AmbiguousExact { action, choices }`
   - `Timeout`
5. 新增 `WhichKeyChoice { key_label, command_id, title }`。
6. `atto-ui-editor::KeyChord` 与框架 `KeyChord` 做桥接，不立即删除 editor 内类型。
7. 添加格式化 helper：`Ctrl+K`, `Shift+F8`, `Ctrl+K Ctrl+F` label 生成。

**测试**：
- 单元：
  - 单键 exact。
  - `Ctrl+K` prefix。
  - `Ctrl+K Ctrl+F` exact。
  - ambiguous exact。
  - timeout 清 pending。

**验收**：
- 不影响现有 `EditorKeymap::get(chord)`。
- 新 engine 可独立用于 app command registry。

**完成记录（2026-06-09）**：
- 新增 `src/app/keymap.rs`，提供框架级 `KeyChord`、`KeySequence`、`KeySequenceEngine<A>`、`KeymapMatch<A>` 与 `WhichKeyChoice`，支持单 chord、multi-chord、prefix pending、ambiguous exact 和可注入 `Instant` 的 timeout。
- keymap engine 使用 trie 存储序列，并通过 `insert_with_metadata` 保存 command id/title，prefix choices 按 key label / command id / title deterministic 排序，便于后续 T13 command registry 与 which-key popup 复用。
- 新增 `key_chord_label` / `key_sequence_label` 与对应方法，覆盖 `Ctrl+K`、`Shift+F8`、`Ctrl+K Ctrl+F` 等 accelerator/which-key label 生成。
- 从 `src/app/mod.rs` 与 `src/lib.rs` re-export 框架 keymap API；`atto-ui-editor::KeyChord` 增加与 `atto_ui::app::KeyChord` 的双向转换和 label helper，现有 `EditorKeymap::get(chord)` 与单 chord bindings 保持不变。
- 新增单元测试覆盖单键 exact、`Ctrl+K` prefix、`Ctrl+K Ctrl+F` exact、ambiguous exact、timeout 清理 pending、invalid chord 清理 pending、label 生成和 editor/framework chord round-trip。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。

### [DONE] R12 — 审阅 T12

审阅 T12 改动：
- 确认 trie 匹配 deterministic。
- 确认 timeout 不依赖 wall clock hidden global，测试可注入 now。
- 确认 KeyModifiers 比较与 crossterm 语义一致。
- 确认没有把 editor 专用 action 类型引入 core keymap。

**完成记录（2026-06-09）**：
- 已审阅 T12 的框架级 `KeyChord` / `KeySequence` / `KeySequenceEngine` / `KeymapMatch` / `WhichKeyChoice`、`app` 与 crate root re-export，以及 `atto-ui-editor` 的 `KeyChord` 桥接。
- 确认 trie 查找按精确 chord 匹配，prefix choices 经 key label / command id / title 排序，输出对 which-key 使用保持 deterministic。
- 发现并修复多段序列 timeout 计时语义：成功推进 prefix 后现在会重新开始等待下一段 chord，避免三段及以上 key sequence 被首段累计超时误取消；新增 `timeout_resets_after_each_successful_prefix_chord` 回归测试。
- 确认 timeout API 由调用方传入 `Instant`，不依赖隐藏 wall clock；既有 timeout 测试和新增回归测试均使用可注入时间。
- 确认 `KeyModifiers` 以 crossterm bitset 精确比较；新增测试覆盖 `Ctrl+S` 不匹配 `Ctrl+Shift+S`。
- 确认 core keymap 模块保持泛型 action `A`，未引入 editor 专用 `EditorAction`；editor crate 仅提供双向 `KeyChord` 转换和 label helper。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。

### [DONE] T13 — Command registry 与 which-key popup

**依赖**：T12。

**文件**：
- `src/app/keymap.rs`
- 新增 `src/app/keymap_popup.rs` 或合并入 keymap 模块
- `src/app/desktop.rs`
- `src/theme/mod.rs`
- `crates/atto-editor-app/src/actions.rs`
- 新增 `crates/atto-editor-app/src/commands.rs`

**步骤**：
1. 新增框架泛型 registry：
   - `CommandDescriptor<A> { id, title, category, default_sequence, action }`
   - `CommandRegistry<A> { commands, by_id }`
2. 实现从 registry 构建 `KeySequenceEngine<A>`。
3. Which-key popup model：
   - `WhichKeyModel { prefix_label, choices }`
   - 绘制 key label + title。
4. 在 `Desktop` 增加可选 which-key overlay，或提供可复用 component 由 app 自己开 floating window。
5. Theme token：
   - `which-key-popup`
   - `which-key-key`
   - `which-key-title`
6. `atto-editor-app/src/commands.rs`：
   - 定义 app command registry，覆盖 File/View/Split/editor/LSP/picker 命令。
   - 菜单后续可从 registry 生成，当前可先与菜单共享 id/title/shortcut 数据。

**测试**：
- key prefix 后显示 which-key choices。
- 继续按完整序列触发 action 并关闭 popup。

**验收**：
- 命令面板和 keymap 能共享同一 command id/title。
- which-key 不抢占普通单键输入。

**完成记录（2026-06-09）**：
- 框架层 `src/app/keymap.rs` 新增 `CommandDescriptor<A>`、`CommandRegistry<A>` 与 duplicate command id 校验，并可从 registry 的 default key sequence 构建 `KeySequenceEngine<A>`；`app` 和 crate root 均 re-export 新 API。
- 新增 `WhichKeyModel` 与 Desktop which-key overlay；prefix choices 使用 `which-key-popup`、`which-key-key`、`which-key-title` named styles 绘制在窗口之上，modal/menu 状态会隐藏或清理 overlay。
- `atto-editor-app` 新增 `commands.rs`，集中登记 File/View/Split/editor/LSP/picker command id/title/category/action/default sequence；全局 prefix keymap 使用 `Ctrl+Alt+K`，只处理 Desktop 未消费的 key，避免抢占普通单键输入。
- app on_event 接入 command keymap：prefix 后显示 which-key choices，完整序列触发 action 并关闭 popup，Esc/无效输入清理 pending；Split/editor/LSP command 可通过 editor window/tab command queue 转发到 active editor。
- `atto-ui-editor` 暴露 `EditorView::handle_editor_action`，供 app command registry 不合成按键地执行 editor/LSP action。
- 新增/扩展单元测试覆盖 command id 唯一性、registry 构建 key sequence engine、which-key overlay 绘制与 modal 隐藏、app command registry 覆盖矩阵、prefix 显示 choices、完整序列触发 Save、已消费 key 不启动 which-key。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。

### [DONE] R13 — 审阅 T13

审阅 T13 改动：
- 确认 command id 唯一性有测试或 debug assertion。
- 确认 which-key overlay 绘制在窗口之上但不破坏 modal。
- 确认 prefix pending 时 Esc 可取消。
- 确认 app command registry 不持有短生命周期引用。

**完成记录（2026-06-09）**：
- 已审阅 T13 的 `CommandRegistry` / `CommandDescriptor`、which-key overlay、`atto-editor-app` command registry、prefix keymap 分发和 editor command 转发路径。
- 确认 command id 唯一性由 `CommandRegistry::new` 拒绝 duplicate id，并有框架单测与 app registry 构建测试覆盖；app registry 使用 owned command metadata/action，不持有短生命周期引用。
- 确认 which-key overlay 在 `wm.draw` 之后绘制到窗口之上，并在 modal active 时隐藏；既有测试覆盖 overlay 文本绘制与 modal 隐藏。
- 确认 command prefix pending 时 `Esc` 会在底层 consumed 结果检查前清理 pending 与 which-key overlay；新增 `command_prefix_escape_clears_pending_and_which_key` 回归测试固定该行为。
- 未发现需要修改 T13 功能代码的问题。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。

### [DONE] T14 — 通用 Picker component 与 Command Palette

**依赖**：T13 推荐；无 T13 时也可先做独立 picker。

**文件**：
- 新增 `crates/atto-editor-app/src/picker.rs`
- `crates/atto-editor-app/src/actions.rs`
- `crates/atto-editor-app/src/app.rs`
- `crates/atto-editor-app/src/commands.rs`
- 使用 `src/fuzzy.rs` 的 `atto_ui::fuzzy::{fuzzy_filter, fuzzy_match}`

**参考**：
- `../editor-core/crates/editor-core-app/src/command_palette.rs`
- `../editor-core/crates/editor-core-app/src/fuzzy.rs`

**步骤**：
1. 新增 `PickerItem<A> { title, subtitle, shortcut, action }`。
2. 新增 `PickerView<A>`：
   - query `TextBox`
   - filtered list
   - selected index / scroll
   - Enter accept、Esc close、Up/Down/PageUp/PageDown navigation
3. 使用 `atto_ui::fuzzy::fuzzy_filter`，不要复制 `editor-core-app` fuzzy。
4. `AppAction` 增加：
   - `OpenCommandPalette`
   - `RunCommand(String)` 或直接 accept `AppAction`
5. `app.rs` 增加打开 command palette 的 modal/floating window。
6. Command palette items 来自 `commands.rs` registry。

**测试**：
- Picker 单元：query 过滤、tie order、selected clamp。
- PTY：`Ctrl+Shift+P` 打开 command palette，输入 `save`，Enter 触发 Save（可用状态文本或 mock action 断言）。

**验收**：
- Picker 可复用到 file/buffer/symbol/search。
- Esc 必须关闭 picker 并恢复原窗口焦点。

**完成记录（2026-06-09）**：
- 新增 `crates/atto-editor-app/src/picker.rs`，提供泛型 `PickerItem<A>`、`PickerView<A>` 与 `PickerEvent<A>`；`PickerView` 内部使用 `TextBox` 作为 query 输入，query 变化时才用 `atto_ui::fuzzy::{fuzzy_filter, fuzzy_match}` 更新过滤结果，并支持 Enter accept、Esc close、Up/Down/PageUp/PageDown navigation。
- `atto-editor-app` 新增 `AppAction::OpenCommandPalette`，使用 modal window 打开 Command Palette；close hook 与 picker close 事件会清理 `AppState` 并恢复打开前的窗口焦点。
- Command Palette items 从 `commands.rs` 的 command registry 生成，保留 command id/title/category/action/default sequence 语义，选中项通过既有 `execute_command_action` 路径执行，不绕过命令分发规则。
- `picker.commandPalette` 增加默认快捷键 `Ctrl+Shift+P`；app command keymap 可打开 palette，palette 内输入 `save` 后 Enter 会触发 Save。
- 新增 picker 单元测试覆盖 query 过滤、tie order、`fuzzy_match` 搜索、selected clamp、Enter accept；新增 app 单元测试覆盖 `Ctrl+Shift+P` 分发、registry item 生成、关闭后恢复焦点；新增 PTY 测试覆盖 `Ctrl+Shift+P` 打开 Command Palette、输入 `save` 并执行 Save 写入文件。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] R14 — 审阅 T14

审阅 T14 改动：
- 确认 picker 没有每帧重建大列表导致明显卡顿；query 变化时过滤即可。
- 确认 fuzzy positions 如用于高亮时 byte offset 处理 Unicode 安全。
- 确认 modal/floating window close hook 清理 AppState。
- 确认 command palette 不绕过 disabled command 规则。

**完成记录（2026-06-09）**：
- 已审阅 T14 的 `PickerView` / `PickerItem` / `PickerEvent`、command palette 打开/关闭路径、`AppCommandAction` 分发、registry item 生成和 PTY 覆盖。
- 确认 `PickerView` 预计算 `search_texts`，`draw`/event 中的 `refresh_filter` 会用 `last_filter_query` 跳过未变化 query，避免每帧重建大列表；`max_results` 也限制了保留结果数。
- 确认当前 picker 未使用 fuzzy `positions` 做高亮；共享 `atto_ui::fuzzy` 返回的是 UTF-8 byte offset，后续若用于高亮需按 grapheme/char 边界转换，当前实现不存在 Unicode 高亮切片风险。
- 确认 Command Palette modal 的 close hook 与 `PickerEvent::Closed` 会清理 `command_palette_window` / `command_palette_restore_focus` 并恢复打开前焦点；accept 路径先恢复焦点再通过统一 `execute_command_action` 执行命令。
- 确认 command palette items 来自 `commands.rs` registry，选中后复用与 command keymap 相同的 `execute_command_action` / `handle_action` 路径，不绕过 active editor fallback、active modal gate 或无 active editor 时的 no-op 规则。
- 未发现需要修改 T14 功能代码的问题。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。

### [DONE] T15 — File picker 与 Buffer/tab picker

**依赖**：T14。

**文件**：
- `crates/atto-editor-app/src/app.rs`
- `crates/atto-editor-app/src/actions.rs`
- `crates/atto-editor-app/src/workspace.rs`
- `crates/atto-editor-app/src/window.rs`
- `crates/atto-editor-app/src/window/tabs.rs`
- `crates/atto-editor-app/src/picker.rs`

**参考**：
- `../editor-core/crates/editor-core-app/src/workspace_index.rs`

**步骤**：
1. `AppAction` 增加：
   - `OpenFilePicker`
   - `OpenBufferPicker`
   - `SelectEditorTab { window: WindowId, tab_id: u64 }`
2. File picker：
   - MVP：复用 `build_workspace_tree` 后 flatten file nodes。
   - workspace roots 改变时 invalid cache。
   - picker accept -> `AppAction::OpenPath { path, target: OpenTarget::NewTab }`。
   - 后续如引入 `editor-core-app::WorkspaceFileIndex`，需在 `Cargo.toml` 加 path dependency，并确认 `ignore` 依赖。
3. Buffer/tab picker：
   - `TabState` 增加 stable `tab_id: u64`。
   - `EditorWindowView` 暴露 `tab_summaries()` 或通过 command queue 响应。
   - `EditorWindowCommand::SelectTabById(u64)`。
4. 快捷键：
   - `Ctrl+P` 打开 file picker。
   - buffer picker 可用 command palette 或 `Ctrl+Shift+P` 命令。

**测试**：
- File picker 在 temp workspace 中能 fuzzy 找到 `src/main.rs` 并打开。
- Buffer picker 能从两个 tabs 切换到指定 tab。

**验收**：
- tab id 不因 close/reorder 改变而误选。
- 大 workspace 初期可同步构建，但要有 max entries 或缓存，避免每帧扫描。

**完成记录（2026-06-09）**：
- `AppAction` 新增 `OpenFilePicker`、`OpenBufferPicker`、`SelectEditorTab { window, tab_id }`，并接入 `handle_action`；`Ctrl+P` 打开 File Picker，Command Palette 中新增 File Picker 与 Buffer Picker 命令。
- File Picker 复用 workspace tree 构建文件索引，只 flatten file nodes；沿用默认 hidden / `.git` 过滤，增加 `MAX_FILE_PICKER_ENTRIES` 与 `WorkspaceFileIndex` cache，workspace roots 变化时自动重建/失效；accept 后走既有 `OpenPath { target: NewTab }` 路径。
- `TabState` 增加 stable `tab_id: u64`，`EditorWindowView` 同步 `EditorTabSummary` binding；Buffer Picker 从打开的 editor windows 收集 tab summaries，accept 后发送 `EditorWindowCommand::SelectTabById(u64)`，不依赖过期 tab index。
- 新增/更新测试：workspace file index files-only/hidden filtering/entry limit；app 单元覆盖 `Ctrl+P` dispatch、file picker cache invalidation、buffer picker 在关闭 tab 后仍按 stable id 选择；PTY 覆盖 `Ctrl+P` fuzzy 找到并打开 `src/main.rs`。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] R15 — 审阅 T15

审阅 T15 改动：
- 确认 file picker 不显示目录和隐藏 `.git` 内容。
- 确认 workspace root 变化后 index invalidation 正确。
- 确认 buffer picker accept 不依赖过期 tab index。
- 确认打开文件沿用现有 `open_path`，不会重复添加 workspace root。

**完成记录（2026-06-09）**：
- 已审阅 T15 的 `WorkspaceFileIndex` 构建、File Picker item 生成/cache、`OpenPath` accept 路径、`EditorTabSummary`/stable `tab_id` 同步，以及 Buffer Picker accept 到 `SelectTabById` 的分发路径。
- 确认 File Picker 从 workspace tree flatten 时只收集 `FileTreeNodeKind::File`，沿用默认 hidden/`.git` 过滤，不显示目录或 `.git` 内容。
- 确认 workspace roots 初始化、`add_workspace_root` 与 cache root 比对都会在 root 变化后失效或重建 file picker index。
- 确认 Buffer Picker item action 携带 stable `tab_id`，accept 后发送 `EditorWindowCommand::SelectTabById(u64)`，不依赖可能过期的 tab index。
- 确认 File Picker accept 复用 `AppAction::OpenPath { target: NewTab }` 和既有 `open_path` 流程；workspace 内文件不会重复添加 workspace root。
- 未发现需要修改 T15 功能代码的问题。
- 验证通过：`cargo fmt`；`cargo clippy --all-targets -- -D warnings`；`cargo clippy --workspace --all-targets -- -D warnings`；最终 `cargo test --all --all-targets`。

### [DONE] T16 — Document symbols / Workspace symbols / Global search pickers

**依赖**：T14；workspace symbols 最好依赖 T20/T21 workspace LSP refactor，MVP 可单文档 LSP。

**文件**：
- `crates/atto-ui-editor/src/view/lsp.rs`
- `crates/atto-ui-editor/src/view/mod.rs`
- `crates/atto-editor-app/src/actions.rs`
- `crates/atto-editor-app/src/app.rs`
- `crates/atto-editor-app/src/picker.rs`
- 可新增 `crates/atto-editor-app/src/search.rs`

**editor-core-lsp API 参考**：
- `LspSession::request_document_symbols`
- `lsp_document_symbols_to_outline`
- `LspSession::request_workspace_symbol`
- `lsp_workspace_symbols_to_results`

**editor-core-app 参考**：
- `../editor-core/crates/editor-core-app/src/find_in_files.rs`

**步骤**：
1. `EditorAction` 或 `AppAction` 增加：
   - `OpenDocumentSymbolPicker`
   - `OpenWorkspaceSymbolPicker`
   - `OpenGlobalSearch`
2. Document symbols：
   - 当前 active `EditorView` 调 `request_document_symbols()`。
   - response 转 outline，发送 `EditorEvent::DocumentSymbols`.
   - app 打开 picker，accept 后让 active editor cursor move 到 symbol range start。
3. Workspace symbols：
   - 若未做 workspace LSP，先通过 last focused editor 的 LSP session 调 `request_workspace_symbol(query)`。
   - accept 后 `OpenPath` 对应 URI，再 jump 到位置。
4. Global search：
   - MVP 用 Rust helper：复制/移植 `find_in_files` 逻辑或给 `atto-editor-app` 添加 `editor-core-app` path dependency。
   - 避免每次 keypress 全量搜索；输入确认后搜索，或 debounce。
   - 结果用 picker/list 显示 path:line: text。
5. 搜索结果 accept：
   - open file。
   - jump to line/column，需要给 `EditorWindowCommand::OpenFileAndJump { path, line, column }` 或 open 后排队 jump。

**测试**：
- Mock LSP document symbols response -> picker shows symbol -> accept moves cursor。
- Global search temp root 中找到 `TODO`，accept 打开对应文件。

**验收**：
- LSP response 异步到达时，如果 picker 已关闭，不应 panic。
- Workspace symbol URI 非 file:// 时明确提示 unsupported。

**完成记录（2026-06-09）**：
- 已为 `atto-ui-editor` 增加 Document Symbols / Workspace Symbols LSP request 与 response 事件，document symbol response 通过 `lsp_document_symbols_to_outline` 转换，workspace symbol response 通过 `lsp_workspace_symbols_to_results` 转换。
- 已在 `atto-editor-app` 中桥接 editor events，新增 Document Symbols、Workspace Symbols query/results、Global Search query/results picker 生命周期，并将 accept 动作接到 active editor jump 或统一 open-and-jump 路径。
- Workspace symbol accept 使用 `file://` URI 转本地路径并保留 UTF-16 坐标到 editor 侧转换；非 `file://` URI 会在 status bar 明确提示 unsupported。
- Global Search 使用本地 Rust helper，尊重 `.gitignore`/`.ignore`/git exclude，跳过 `.git`/`target`/`.build`，包含单文件大小限制与全局结果上限，且只在确认输入后执行搜索。
- 已新增 editor UTF-16 symbol/jump 单测、picker query-submit 单测、app picker action 单测、search helper 单测，以及 Global Search PTY 测试。
- 验证通过：`cargo fmt`；`cargo clippy --all-targets -- -D warnings`；`cargo test --all --all-targets`。

### [DONE] R16 — 审阅 T16

审阅 T16 改动：
- 确认 document symbol range 使用 UTF-16/LSP 坐标转 editor position 正确。
- 确认 workspace symbol accept 对 unopened file 走统一 `open_path`。
- 确认 global search 尊重 ignore/.gitignore 或明确 MVP 限制。
- 确认搜索大文件有 size limit，避免卡 UI。

**完成记录（2026-06-09）**：
- 已审阅 T16 document/workspace symbol 与 global search picker 变更：document symbols 经 `lsp_document_symbols_to_outline` 转为 editor char offset，accept 使用 selection range offset；workspace symbol accept 将 `file://` URI 转本地路径后走统一 `OpenPathAndJump` / `OpenFileAndJump` 路径，未打开文件也会先打开再跳转，非 `file://` URI 明确显示 unsupported status。
- 已确认 global search 使用 `ignore::WalkBuilder` 尊重 `.gitignore` / `.ignore` / git exclude，并跳过 `.git`、`target`、`.build`；搜索配置保留单文件 size limit 与结果上限，避免大文件拖慢 UI。
- 审阅中发现并修复：global search 原先遇到一个小型非 UTF-8 文件会让整次搜索失败；现在仅跳过 UTF-8 解码失败的文件，其他读取错误仍显式返回，并新增覆盖测试。
- 修复 T16 相关 clippy 问题：将 editor window reactive bindings 分组，避免过长构造函数参数列表；清理 one-item slice clone 警告。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

---

## 阶段三：Workspace LSP 与高级 LSP 功能

### [DONE] T17 — Workspace / LSP Bridge 状态层

**依赖**：建议 T7-T9 后做。

**文件**：
- 新增 `crates/atto-editor-app/src/workspace_state.rs`
- 新增 `crates/atto-editor-app/src/lsp_workspace.rs`
- `crates/atto-editor-app/src/app.rs`
- `crates/atto-editor-app/src/window/tabs.rs`
- `crates/atto-editor-app/src/window/document_tab.rs`
- `crates/atto-editor-app/Cargo.toml`

**editor-core API 参考**：
- `../editor-core/crates/editor-core/src/workspace.rs`
- `Workspace::{new, open_buffer, create_view, set_active_view, buffer_id_for_uri, buffer_text, buffer_text_for_saving, apply_text_edits, apply_processing_edits, take_last_text_delta_for_buffer}`
- `../editor-core/crates/editor-core-lsp/src/workspace_sync.rs`
- `LspWorkspaceSync::{start, open_workspace_document, close_workspace_document, set_active_workspace_document, poll_workspace, did_change_from_text_delta, apply_workspace_edit}`
- `../editor-core/crates/editor-core-app/src/workspace_io.rs` 可参考，不一定直接依赖。

**步骤**：
1. `Cargo.toml` 如需使用 `editor-core-app::WorkspaceIo`，添加 path dependency 到 `../editor-core/crates/editor-core-app`；否则在本 app 内实现最小 open/save helper。
2. `AppState` 增加：
   - `workspace: editor_core::workspace::Workspace`
   - `path_to_buffer: HashMap<PathBuf, BufferId>`
   - `buffer_to_tabs: HashMap<BufferId, Vec<TabRef>>`
   - `lsp_by_root_language: HashMap<LspKey, LspWorkspaceSync>`
3. 打开文件时：
   - 仍创建 `Binding<String>` 给现有 `EditorView`（bridge 阶段）。
   - 同时 `Workspace::open_buffer(Some(path_to_file_uri(path)), &text, viewport_width)`。
   - `TabState` 保存 `buffer_id`。
4. 文本同步：
   - 保存/rename/format 前，把 tab binding 最新文本同步到 workspace buffer。
   - workspace edit 后，把 `Workspace::buffer_text(buffer_id)` 写回对应 tab bindings。
5. LSP sync：
   - 按 `(workspace_root, language_id)` 复用 `LspWorkspaceSync`。
   - 打开 tab 时 `open_workspace_document`。
   - active tab 改变时 `set_active_workspace_document`。
   - on_tick `poll_workspace` 并 drain events。
6. 明确 bridge 限制：现有 `EditorView` 仍有自己的 `LspSession`，workspace LSP 先只服务 rename/workspace symbol；后续再新增 `WorkspaceEditorView` 替代 per-view session。

**测试**：
- 打开两个文件后 workspace 有两个 buffers。
- 对 workspace 应用 edit 后两个 tab binding 更新。
- active tab 切换后对应 LSP active document 改变（可用 mock / state 断言）。

**验收**：
- 不破坏当前打开/保存/dirty title。
- 同一个文件重复打开仍只对应一个 buffer。

**完成记录（2026-06-09）**：
- 新增 `workspace_state` 共享状态层，集中维护 `editor_core::Workspace`、path/buffer/tab 映射、tab binding 同步、重复打开 buffer 复用、workspace edit 应用后 binding 回写、active tab 到 active workspace buffer 的同步。
- 新增 `lsp_workspace` bridge，按 `(workspace_root, language_id)` 管理 `LspWorkspaceSync`，打开/关闭 workspace documents，切换 active document，转发 binding text delta，处理 workspace symbol request，以及 deferred `workspace/applyEdit`。
- `EditorWindowView` 现在接收共享 workspace state；打开、关闭、切换、保存、另存、dirty title 更新都会同步 workspace buffer，不改变现有 tab UI 和 per-view LSP bridge 限制。
- `atto-editor-app` AppState 接入共享 workspace roots/state，tick 中轮询 workspace LSP events，Workspace Symbols 改由 workspace LSP bridge 请求并显示结果。
- 已新增 workspace state 单测覆盖重复打开复用同一 buffer、workspace edit 回写多个 tab binding、active tab 切换更新 active workspace buffer。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] R17 — 审阅 T17

审阅 T17 改动：
- 确认 bridge 同步不会形成 binding dirty 循环。
- 确认 `path_to_file_uri` / `file_uri_to_path` 使用 `editor-core-lsp` helper，避免手写 URI。
- 确认 close tab 后 buffer/LSP document 生命周期合理；若暂不 close，需注释说明。
- 确认 workspace edit 后 dirty 状态正确更新。

**完成记录（2026-06-09）**：
- 已审阅 T17 workspace bridge：workspace edit 回写 tab binding 后，`update_tab_titles` 再同步到 workspace 时会先比较 workspace 当前文本，避免把同一 workspace edit 重新发送为 LSP didChange；新增 UI 层测试覆盖 workspace edit 后 tab dirty 状态变为 true 且无 bridge 错误。
- 确认文件 URI 转换统一使用 `editor-core-lsp::path_to_file_uri` / `editor_core_lsp::file_uri_to_path`，未发现手写 `file://` 拼接或自定义 URI 解析。
- 确认 tab/window 生命周期合理：关闭非最后一个 tab 只关闭 workspace view，关闭最后一个 tab 时关闭 LSP document、移除 path/buffer 映射并关闭 workspace buffer；关闭 editor window 会 unregister 该窗口所有 tabs。
- 审阅中发现并修复：多 `(workspace_root, language_id)` LSP sync 轮询时，inactive sync 原先会被传入全局 active buffer，可能因该 buffer 不属于该 sync 而反复 poll 失败。现在只有 owning active sync 使用 `poll_workspace`，inactive sync 通过 raw session poll drain workspace symbol / applyEdit / message events，避免丢事件且不把派生编辑应用到错误 buffer；新增 active poll key 单测。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] T18 — L3 Rename UI 与跨已打开文件 WorkspaceEdit 应用

**依赖**：T17。

**文件**：
- `crates/atto-ui-editor/src/keymap.rs`
- `crates/atto-ui-editor/src/view/lsp.rs`
- `crates/atto-ui-editor/src/view/actions.rs`
- `crates/atto-ui-editor/src/popup.rs`
- `crates/atto-editor-app/src/app.rs`
- `crates/atto-editor-app/src/lsp_workspace.rs`
- `crates/atto-editor-app/src/window/tabs.rs`

**editor-core-lsp API 参考**：
- `LspSession::request_prepare_rename`
- `LspSession::request_rename`
- `apply_workspace_edit_to_workspace`
- `LspWorkspaceSync::apply_workspace_edit`

**步骤**：
1. `EditorAction::LspRename`，默认键 `F2`。
2. prepare rename：
   - active editor position -> `request_prepare_rename(line_index, line, column)`。
   - response OK 后打开 rename input popup，默认文本为 prepare range 或当前 word。
3. Rename input：
   - Enter 调 `request_rename(line_index, line, column, new_name)`。
   - Esc 取消。
4. Rename response：
   - 通过 workspace LSP sync 或 `apply_workspace_edit_to_workspace(&mut workspace, &edit)` 应用到已打开 buffers。
   - 对 `skipped_uris` 显示明确提示，不写未打开文件。
   - 更新所有 tab binding、dirty title、diagnostics/syntax。
5. 如果没有 T17 workspace 可用，action 应显示 “Rename requires workspace support”，不要走 partial rename。

**测试**：
- Mock LSP prepare+rename 单文件 edit。
- 两个已打开文件 cross-file edit 都更新。
- 未打开 URI 被 skipped 且磁盘文件不被改。

**验收**：
- Rename 不会部分静默成功。
- Rename edit 是 undoable（至少 per buffer 可 undo；若 bridge 不支持，记录限制）。

**完成记录（2026-06-09）**：
- `EditorAction::LspRename` 已接入默认 `F2` 与命令面板命令；EditorView 现在支持 prepare-rename 请求、rename input popup、Enter 提交、Esc 取消，并从 prepare range / placeholder / 当前 word 推导默认文本。
- Rename response 不在单文档 editor 内做 partial apply，而是通过 `EditorEvent::LspRenameWorkspaceEdit` 交给 `atto-editor-app` 的共享 `WorkspaceState::apply_workspace_edit`，跨已打开文件同步 tab binding/dirty title；对 skipped unopened URI 显示明确状态提示且不写磁盘。
- Rename popup 与 completion/code action/hover popup 状态互斥；prepare/rename error、null/no-edit response 会通过 LSP message 事件提示，不静默失败。
- Undo 限制记录：workspace edit 回写 tab binding 后，现有 EditorView 外部文本同步使用 editor-core `Replace` edit path，因此同步后的每个打开 buffer 仍走 per-view undo 路径。
- 新增/更新测试覆盖：mock LSP prepare+rename 单文件 edit 事件、两个已打开文件 cross-file rename edit 同步、未打开 URI skipped 且磁盘文件不变、默认 keymap `F2`。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-editor --test lsp_editor lsp_rename_popup_submits_workspace_edit_event`；`cargo test -p atto-editor-app rename_workspace_edit`；`cargo test --workspace --all-targets`。

### [DONE] R18 — 审阅 T18

审阅 T18 改动：
- 确认 prepare rename error/null 时 UI 提示合理。
- 确认 skipped unopened URI 不写磁盘。
- 确认 multiple buffers 更新后 tab dirty markers 正确。
- 确认 rename popup 不与 completion/code action popup 状态冲突。

**完成记录（2026-06-09）**：
- 已审阅 T18 rename UI / workspace edit 路径：prepare rename error/null 通过 `EditorEvent::LspMessage` 给出明确状态提示且不打开 rename popup。
- 确认 skipped unopened URI 只进入 `ApplyWorkspaceEditResult::skipped_uris`，不会写未打开文件磁盘内容；跨两个已打开文件的 rename edit 会同步对应 tab binding。
- 确认 workspace edit 同步到打开 tab 后会触发 tab dirty marker；补充回归覆盖保持 dirty title 更新。
- 加固 rename popup 互斥：rename 打开/响应处理会清理 hover、completion、code action popup 和相关 pending 状态，避免与 completion/code action popup 冲突。
- 新增回归测试覆盖 prepare-rename null/error 提示、rename 请求清理 completion/code action popup；扩展 mock LSP 支持 prepare-rename error 响应。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-editor --test lsp_editor lsp_rename`；`cargo test -p atto-editor-app rename_workspace_edit`；`cargo test -p atto-editor-app workspace_edit_marks_open_tab_dirty`；`cargo test --workspace --all-targets`。

### [DONE] T19 — L4 Signature Help

**依赖**：T7 的 LSP response 分发。

**文件**：
- `crates/atto-ui-editor/src/keymap.rs`
- `crates/atto-ui-editor/src/view/input.rs`
- `crates/atto-ui-editor/src/view/lsp.rs`
- `crates/atto-ui-editor/src/view/render.rs`
- `crates/atto-ui-editor/src/popup.rs`

**editor-core-lsp API 参考**：
- `LspSession::request_signature_help`
- `signature_help_from_value`

**步骤**：
1. `EditorAction::LspSignatureHelp`，默认键 `Ctrl+Shift+Space`。
2. `EditorLspController` 增加 `pending_signature_help: Option<u64>`。
3. `popup.rs` 新增 `SignatureHelpPopupModel { rect, signatures, active_signature, active_parameter }`。
4. 输入触发：
   - 普通输入 `(` / `,` 后，如果 LSP enabled，调用 `request_signature_help_now()`。
   - 手动 action 也触发。
5. response：
   - `signature_help_from_value(result)`。
   - popup rect 类似 completion cursor rect。
6. 渲染：
   - 显示 active signature label。
   - active parameter 用 selected/underline style。
   - Esc 或普通输入关闭/刷新。

**测试**：
- Mock LSP：输入 `(` 后出现 signature popup。
- Esc 关闭。

**验收**：
- completion popup 打开时 signature popup 不抢焦点。
- 无 signature result 时 popup 清空。

**完成记录（2026-06-09）**：
- `EditorAction::LspSignatureHelp` 已接入默认 `Ctrl+Shift+Space`；普通输入 `(` / `,` 会在文本插入后的 cursor position 触发 `request_signature_help_now()`。
- `EditorLspController` 已增加 signature help pending/requested position 状态；response 通过 `signature_help_from_value` 解析，并在 cursor stale、completion/code action/rename popup 活跃、error/null/empty result 时清空 popup。
- 新增 `SignatureHelpPopupModel`、inline/window popup 渲染与 editor handle binding；active signature label 使用 popup 样式显示，active parameter 使用 selected + underline 样式，popup rect clamp 在 editor content bounds 内。
- Mock LSP 扩展 `signatureHelpProvider` 与 `textDocument/signatureHelp` 响应；新增测试覆盖 `(` 触发 popup、Esc 关闭、空 result 清空、completion popup 优先级，以及默认 keymap 绑定。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-editor --test lsp_editor lsp_signature_help -- --nocapture`；`cargo test -p atto-ui-editor --test lsp_editor`；`cargo test --workspace --all-targets`。

### [DONE] R19 — 审阅 T19

审阅 T19 改动：
- 确认触发字符插入后 cursor position 用 post-edit 位置请求。
- 确认 stale response 不显示到新 cursor 位置。
- 确认 popup rect clamp 在 editor content bounds 内。

**完成记录（2026-06-09）**：
- 已审阅 T19 signature help 路径，确认 `(` / `,` 触发在文本插入后的 cursor position 发起请求，signature help response 会按 requested cursor position 丢弃 stale 结果，popup rect 会 clamp 到 editor content bounds 内。
- 发现并修复 completion response 相关竞态：旧 completion 请求的 response 到达时不再先清空新的 signature help popup，而是先比较 completion requested cursor position，stale response 直接丢弃。
- 补充回归测试 `stale_completion_response_does_not_clear_signature_help_popup`，覆盖旧 completion response 不会清除当前 signature help popup。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-editor stale_completion_response_does_not_clear_signature_help_popup`；`cargo test -p atto-ui-editor --test lsp_editor lsp_signature_help`；`cargo test --workspace --all-targets`。

### [DONE] T20 — L5 Formatting 手动格式化与保存前格式化接口

**依赖**：T7；format-on-save 完整体验可依赖 T17。

**文件**：
- `crates/atto-ui-editor/src/config.rs`
- `crates/atto-ui-editor/src/keymap.rs`
- `crates/atto-ui-editor/src/view/actions.rs`
- `crates/atto-ui-editor/src/view/lsp.rs`
- `crates/atto-editor-app/src/window.rs`
- `crates/atto-editor-app/src/window/tabs.rs`

**editor-core-lsp API 参考**：
- `LspSession::request_formatting`
- `lsp_formatting_options`
- `lsp_formatting_options_for_indentation_config`
- `text_edits_from_value`
- `apply_text_edits`

**步骤**：
1. `EditorAction::LspFormatDocument`；C3 完成后绑定 `Ctrl+K Ctrl+F`，此前通过 command palette 暴露。
2. `EditorConfig` 增加 `format_on_save: Binding<bool>`，默认 false。
3. 手动 format：
   - 从 indent config 生成 LSP formatting options。
   - `lsp.request_formatting(options)`。
4. response：
   - `text_edits_from_value(result)`。
   - `apply_text_edits(&mut state_manager, &edits)`。
   - 更新 `config.text`、syntax、LSP didChange。
5. 保存前格式化：
   - MVP 只提供 config 和 `EditorWindowCommand::FormatActive`。
   - 完整版 `SaveActive` 如 `format_on_save=true`，先 format，成功后 save；失败要提示并可选择是否继续保存。

**测试**：
- Mock LSP formatting response 改变文本。
- 无 LSP 时 format action ignored 并不改文本。

**验收**：
- Formatting edit 可 undo。
- 保存时格式化不应造成重复 didChange 或 dirty 状态错乱。

**完成记录（2026-06-09）**：
- 新增 `EditorAction::LspFormatDocument`、`EditorConfig::format_on_save`（默认 false）与 `EditorWindowCommand::FormatActive`；命令面板注册 `Format Document`，并接入可工作的 `Ctrl+K Ctrl+F` 默认序列。
- 手动 formatting 使用当前 `tab_width` / `insert_spaces` 生成 `FormattingOptions` 并调用 `LspSession::request_formatting`；response 通过 `text_edits_from_value` 解析，并用 `EditCommand::ApplyTextEdits` 作为单个 undo step 应用，随后同步 `config.text`、syntax 与 LSP didChange。
- 保存前格式化已接入 `SaveActive`：当 active tab 的 `format_on_save=true` 时先请求格式化，成功后保存同一 tab；失败通过 `LspMessage`/status path 明确提示并跳过保存。
- Mock LSP 增加 `documentFormattingProvider` 与 formatting response；新增测试覆盖 edits 改变文本、`Ctrl+K Ctrl+F`、单步 undo、当前 indentation options、空 edits 不改文本、error message、无 LSP ignored。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-editor --test lsp_editor lsp_format_document -- --nocapture`；`cargo test -p atto-editor-app app_command_registry_binds_format_document_sequence`；`cargo test --workspace --all-targets`。

### [DONE] R20 — 审阅 T20

审阅 T20 改动：
- 确认 formatting 使用当前 tab_width/insert_spaces。
- 确认空 edits response 不改变 dirty 状态。
- 确认 format-on-save 失败路径不静默吞错误。

**完成记录（2026-06-09）**：
- 已审阅 T20 formatting 路径，确认手动 formatting 使用当前 `tab_width` / `insert_spaces` 构造 LSP options，空 edits response 只发出 `FormatFinished { success: true, changed: false }`，不会改文本或生成 undo step。
- 发现并修复保存前格式化失败路径缺口：LSP poll error 或 clean EOF/no-response 期间若存在 pending formatting，现在会发出可见 `LspMessage` 与 `FormatFinished { success: false, changed: false }`，从而让 format-on-save 清理 pending save 并跳过保存；新增 `formatting_timeout` 默认 10s 防止静默挂起。
- Mock LSP 增加 formatting request 后 clean exit fixture；新增回归测试 `lsp_format_document_transport_exit_times_out_and_finishes`。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-editor --test lsp_editor lsp_format_document_transport_exit_times_out_and_finishes`；`cargo test --workspace --all-targets`。

### [DONE] T21 — L6 Inlay Hints 与 composed grid 渲染

**依赖**：T7；建议 T20 后做。

**文件**：
- `crates/atto-ui-editor/src/config.rs`
- `crates/atto-ui-editor/src/view/mod.rs`
- `crates/atto-ui-editor/src/view/lsp.rs`
- `crates/atto-ui-editor/src/view/render.rs`
- `crates/atto-ui-editor/src/theme.rs`

**editor-core-lsp API 参考**：
- `LspSession::request_inlay_hints`
- `lsp_inlay_hints_to_processing_edit`
- `editor-core` composed viewport APIs：`get_headless_grid_composed` / `get_viewport_content_composed`

**步骤**：
1. `EditorInlayHintsConfig { enabled: Binding<bool>, refresh_delay: Binding<Duration> }`，加入 `EditorConfig`。
2. `EditorAction::LspToggleInlayHints`。
3. `EditorLspController` 增加:
   - `pending_inlay_hints: Option<u64>`
   - `last_inlay_range/revision`
4. 在 draw/idle 中，当 focused、enabled、viewport/text revision 变化且无 pending 时，请求当前可见 range：
   - start/end offset 用 `LineIndex` position conversion。
   - `lsp.request_inlay_hints(line_index, start, end)`。
5. response：
   - `lsp_inlay_hints_to_processing_edit(line_index, result)`。
   - `state_manager.apply_processing_edits([edit])`。
6. `render_text` 改造：
   - inlay/code lens enabled 时使用 composed grid。
   - 抽象 styled grid 与 composed grid 的 span 生成，virtual text style 使用 `theme.inlay_hint`。
7. `theme.rs` 增加 inlay hint/code lens style。

**测试**：
- Mock inlay hint response 后 PTY 断言出现 virtual text。
- Toggle off 后 virtual text 消失。

**验收**：
- Inlay hints 不修改 backing text。
- Virtual text 与 selection/cursor 渲染不互相错位。

**完成记录（2026-06-09）**：
- 新增 `EditorInlayHintsConfig { enabled, refresh_delay }` 并接入 `EditorConfig`、public re-export、动态组件 schema / property，以及 `EditorAction::LspToggleInlayHints`（默认 F7）和 atto-editor-app command registry。
- `EditorLspController` 增加 inlay hints pending/request tracking；draw 时在 focused、enabled、viewport/text fingerprint 变化且无 pending 时请求当前可见 range，response 通过 `lsp_inlay_hints_to_processing_edit` 应用到 `INLAY_HINTS` decoration layer，并丢弃 stale response。
- `render_text` 在 inlay hints enabled 时切换到 `get_viewport_content_composed`，将 document/virtual composed cells 转成 spans；selection 只作用于 document cells，cursor 在 composed 模式下按 virtual text 后的 cell x 坐标定位。
- `EditorTheme` 增加 `inlay_hint` / `code_lens` 样式并映射 `INLAY_HINT_STYLE_ID` / `CODE_LENS_STYLE_ID`；mock LSP 增加 `inlayHintProvider` 与 deterministic `textDocument/inlayHint` response；snapshot editor 增加 `--inlay-hints` fixture mode。
- 新增 direct renderer/LSP 测试覆盖 virtual text 显示、toggle off 清理与 backing text 不变；新增 PTY 测试覆盖端到端 inlay hints 显示和 F7 toggle off。
- 验证通过：`cargo fmt`；`cargo test -p atto-ui-editor --test lsp_editor lsp_inlay_hints_render_as_virtual_text_and_toggle_off -- --nocapture`；`cargo test -p atto-ui-editor --test pty_editor pty_editor_inlay_hints_render_and_toggle_off -- --nocapture`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] R21 — 审阅 T21

审阅 T21 改动：
- 确认 composed grid 渲染不破坏现有 syntax/semantic token style。
- 确认 virtual text 不参与 copy/save。
- 确认 viewport range 计算覆盖 soft wrap/folding 情况，至少不 panic。

**完成记录（2026-06-09）**：
- 已审阅 T21 的 inlay hints config/action wiring、LSP request/response tracking、`get_viewport_content_composed` 渲染路径、style id 映射、toggle-off 清理、mock LSP 与 PTY/direct 测试覆盖。
- 确认 composed grid 渲染仍通过统一 `style_for_style_ids` 解析 syntax / semantic token / LSP virtual text style；补充回归测试覆盖 inlay hints 与 semantic tokens、folding markers 同时启用时 string token 仍保留绿色 semantic style。
- 确认 virtual text 只作为 `INLAY_HINTS` decoration/composed virtual cells 渲染，不写入 backing text；copy 使用 editor text ranges，save 使用 tab/workspace backing text。补充回归测试确认 select-all copy 不包含 `: i32` virtual text。
- 确认 inlay request range 基于当前 viewport visual rows 映射到 logical line offsets，soft wrap 会覆盖可见 visual rows 对应的完整 logical lines，folded viewport 下通过 `visual_to_logical_line` 获取可见 logical line，且既有 folding + 新增组合测试覆盖无 panic 路径。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-editor --test lsp_editor lsp_inlay_hints_preserve_semantic_styles_and_copy_backing_text -- --nocapture`；`cargo test --workspace --all-targets`。

---

## 阶段四：File tree 与文件面板

### [TODO] T22 — F-FT FileTree 节点模型、git status 样式与多选

**依赖**：无。

**文件**：
- `crates/atto-ui-file-tree/src/lib.rs`
- `crates/atto-editor-app/src/workspace.rs`
- `crates/atto-editor-app/src/explorer_window.rs`
- `crates/atto-ui-file-tree/tests/pty_file_tree.rs`

**相关现状**：
- `FileTreeNode` 当前字段：`id`, `name`, `kind`, `children`, `is_expanded`。
- `FileTreeBindings` 当前只有 `selection: Binding<Option<FileTreeNodeId>>`，无多选。

**步骤**：
1. `atto-ui-file-tree` 增加：
   - `FileTreeGitStatus::{Modified, Added, Deleted, Renamed, Untracked, Ignored, Clean}`
   - `FileTreeNode.git_status: Option<FileTreeGitStatus>`
   - builder `with_git_status(status)`。
2. `FileTreeBindings` 增加：
   - `selections: Binding<BTreeSet<FileTreeNodeId>>`
   - `selection_anchor: Option<FileTreeNodeId>`
3. 保持 `selection` 作为 primary selection 兼容现有 API。
4. 事件：
   - click：单选。
   - Ctrl+click：toggle。
   - Shift+click：从 anchor 到 clicked visible row range。
   - Shift+Up/Down：扩展 range。
5. 绘制：
   - selected rows 用 selection style。
   - git status 根据 named style 或 theme widget accent 显示。
6. `ExplorerWindowView` 读取多选 ids，为后续 context menu/drag 做准备。

**测试**：
- 单元：visible rows range selection。
- PTY：Ctrl/Shift 多选，屏幕 selected style 或 debug text 可断言。

**验收**：
- 单选旧行为不回归。
- 多选后 Enter 打开文件只打开 primary selection。

### [TODO] R22 — 审阅 T22

审阅 T22 改动：
- 确认多选不破坏 runtime property schema 兼容。
- 确认 range selection 只在 visible rows 上操作，不选中 collapsed children。
- 确认 git status None/Clean 样式不会制造噪声。

### [TODO] T23 — F-FT Context menu 与 inline new/rename

**依赖**：T22。

**文件**：
- `crates/atto-ui-file-tree/src/lib.rs`
- `crates/atto-editor-app/src/explorer_window.rs`
- `crates/atto-editor-app/src/actions.rs`
- `crates/atto-editor-app/tests/explorer_*`

**步骤**：
1. 右键 context menu：
   - MVP 可在 `ExplorerWindowView` 内打开 app-level popup，不必先做框架通用 context menu。
   - actions：New File, New Folder, Rename, Delete, Cut, Copy, Paste, Copy Path, Reveal。
2. `FileTree` inline edit state：
   - `InlineEditState { node_id, parent_id, text: TextBuffer, kind }`
   - kind: Rename / NewFile / NewFolder。
3. 绘制 inline row 时用 input 样式替代 label。
4. Enter commit：
   - Rename -> `std::fs::rename(old, new)`。
   - New file -> `fs::File::create_new` 或检查 exists 后 create。
   - New folder -> `fs::create_dir`。
   - 成功后 `ExplorerWindowCommand::Refresh`。
5. Esc cancel。
6. Delete action：
   - MVP 可移到 trash 暂不做；若直接删除，必须弹确认。没有确认 dialog 时先只实现 Rename/New。

**测试**：
- Inline rename commit/cancel。
- New file/folder temp dir 成功创建。
- Existing target 不覆盖并显示错误。

**验收**：
- 所有 FS 操作错误都通过状态/提示显示，不 silent return。
- inline edit 不影响滚动条/selection。

### [TODO] R23 — 审阅 T23

审阅 T23 改动：
- 确认文件名为空、含路径分隔符、目标已存在时安全拒绝。
- 确认 rename/new 后 id/path maps 刷新。
- 确认右键菜单不会误触发左键 selection/open。

### [TODO] T24 — F-FT Drag move、剪贴板与 Git status 刷新

**依赖**：T2, T22；clipboard 可不依赖 drag。

**文件**：
- `crates/atto-ui-file-tree/src/lib.rs`
- `crates/atto-editor-app/src/explorer_window.rs`
- `crates/atto-editor-app/src/workspace.rs`
- `crates/atto-editor-app/src/app.rs`

**步骤**：
1. Drag move：
   - `FileTree::drag_source_at` 返回 `DragPayload::Custom { ty: "atto-ui-file-tree/node-ids", data: "id1,id2" }`。
   - `ExplorerWindowView::drop` 解析 ids -> paths。
   - drop target 仅 directory/root。
   - 用 `std::fs::rename` 移动；跨 filesystem 失败时 MVP 显示错误，不自动 copy+delete。
2. 剪贴板：
   - `ExplorerWindowView` 增加 `FileClipboard { mode: Cut|Copy, paths }`。
   - Paste 到 directory：
     - file copy: `fs::copy`
     - dir copy: recursive helper
     - cut: `fs::rename`
     - 冲突：MVP 不覆盖，显示错误。
3. Git status：
   - 后台/节流运行 `git -C <root> status --porcelain=v1 --ignored=matching`。
   - 解析 XY/path -> `FileTreeGitStatus`。
   - 写入 workspace tree nodes 后 refresh binding。
   - draw 不执行 git command。
4. Refresh：
   - 文件操作成功后 `ExplorerWindowCommand::Refresh`。
   - workspace roots 改变时清 git cache。

**测试**：
- Drag move temp file 到 folder。
- Cut/copy/paste 不覆盖已有文件。
- Git status parser 单元：modified/added/untracked/ignored。

**验收**：
- 文件移动失败不丢源文件。
- Git command 不在 draw/event hot path 同步运行。

### [TODO] R24 — 审阅 T24

审阅 T24 改动：
- 确认 drag payload 无法伪造越过 workspace root 限制。
- 确认 recursive copy 避免把目录复制到自身/后代。
- 确认 cut 成功后 clipboard 清理，失败后保持或明确策略。
- 确认 git status 对 renamed paths、spaces in filenames 解析正确或有明确限制。

---

## 阶段五：编辑体验收尾

### [TODO] T25 — Auto-pairs / auto-indent 改用 editor-core 原语

**依赖**：T6 可共享 action/text sync helper。

**文件**：
- `crates/atto-ui-editor/src/config.rs`
- `crates/atto-ui-editor/src/view/input.rs`
- `crates/atto-ui-editor/src/view/actions.rs`
- `crates/atto-editor-app/src/language.rs`
- `crates/atto-editor-app/src/window/document_tab.rs`

**editor-core API 参考**：
- `EditCommand::TypeChar { ch }`
- `EditCommand::InsertNewline { auto_indent }`
- `ViewCommand::SetAutoPairsConfig`
- `ViewCommand::SetIndentationConfig`
- `AutoPairsConfig`

**步骤**：
1. `EditorConfig` 增加：
   - `auto_pairs: Binding<AutoPairsConfig>` 或更小 config。
   - `auto_indent: Binding<bool>`。
2. `language.rs` 增加：
   - `indentation_config_for_language(language_id)`
   - `auto_pairs_config_for_language(language_id)`
3. `build_editor_view` 设置 indentation/auto-pairs view command。
4. `handle_key_event` 普通 char：
   - 不再直接 `insert_text(&c.to_string())`。
   - 改 `execute_and_sync_text(Command::Edit(EditCommand::TypeChar { ch: c }))`。
5. Enter：
   - 改 `EditCommand::InsertNewline { auto_indent: config.auto_indent.get() }`。
6. 保留 paste 走 `insert_text(text)` 或 `EditCommand::InsertText { text }`，不要对 paste 套 auto-pairs。

**测试**：
- 输入 `(` 自动补 `)`。
- 有 selection 输入 `"` wrap selection。
- Enter auto-indent 保持上一行缩进。

**验收**：
- Unicode 输入不被 auto-pairs 破坏。
- IME/paste 不被误判为普通 TypeChar。

### [TODO] R25 — 审阅 T25

审阅 T25 改动：
- 确认 TypeChar 后 cursor/selection 与 editor-core 预期一致。
- 确认 read-only gate 覆盖 TypeChar/InsertNewline。
- 确认 auto-pairs config 可按语言关闭。

### [TODO] T26 — Trim trailing whitespace 与 save 流程整理

**依赖**：T20 可共享 save/format 流程；无 T20 也可做。

**文件**：
- `crates/atto-ui-editor/src/config.rs`
- `crates/atto-editor-app/src/window/tabs.rs`
- `crates/atto-editor-app/src/window.rs`

**步骤**：
1. `EditorConfig` 或 app-level settings 增加 `trim_trailing_whitespace_on_save: Binding<bool>`，默认 false。
2. 保存前生成 edits：
   - 遍历每行，删除行尾空格/制表符。
   - 不改变最终 newline 语义。
   - 用 `EditCommand::ApplyTextEdits { edits }` 或 workspace `apply_text_edits`。
3. `SaveActive` 流程顺序：
   - format-on-save（如果启用且已实现）
   - trim trailing whitespace
   - write file
   - update `last_saved_text` / dirty marker。
4. 错误路径显示明确状态，不静默忽略。

**测试**：
- 保存启用 trim 后文件行尾空白被移除。
- 默认未启用时保存不改变空白。
- CRLF 文件保存仍保持 CRLF（如果使用 workspace line ending）。

**验收**：
- trim 操作可 undo（如果发生在 editor buffer 中）。
- 保存后 dirty marker 清除。

### [TODO] R26 — 审阅 T26

审阅 T26 改动：
- 确认 trim 不删除行内空格。
- 确认最后一行无 newline 的文件不会被强制添加 newline，除非已有策略。
- 确认保存失败时 dirty marker 不被清除。

### [TODO] T27 — Jumplist / registers 设计占位与 WorkspaceEditorView 决策

**依赖**：T17。

**文件**：
- `PLAN-2.md`（如需要补决策）
- 新增或修改 `crates/atto-editor-app/src/workspace_state.rs`
- `crates/atto-ui-editor/src/view/*`

**说明**：
此任务不是立即实现全部 jumplist/registers，而是在 workspace bridge 稳定后做架构决策，避免后续高级编辑功能继续堆在 per-view `EditorStateManager` 上。

**步骤**：
1. 对比两条路线：
   - 保持 `EditorView + Binding<String>`，app 层同步 workspace。
   - 新增 `WorkspaceEditorView { workspace: Arc<Mutex<Workspace>>, view_id }`。
2. 写入代码注释或 `PLAN-2.md` 附录，明确何时切换。
3. 如果决定新增 `WorkspaceEditorView`：
   - 列出需迁移的 `EditorView` 方法：render/input/scroll/lsp/search/selection。
   - 先做只读 prototype，不替换生产路径。
4. Jumplist/registers 只在 workspace view 路线确定后接。

**测试**：
- 无功能改动时不需要 PTY。
- 如做 prototype，单测 render/input smoke。

**验收**：
- 后续 Rename/workspace symbol/search result jump 有明确状态归属。
- 不再新增跨文件功能到无法同步的 per-view 孤立状态。

### [TODO] R27 — 审阅 T27

审阅 T27 改动：
- 确认决策记录具体到类型和文件，不是泛泛说明。
- 确认没有引入未使用的大量 dead code。
- 确认未来任务能据此判断应改 `EditorView` 还是 `WorkspaceEditorView`。

---

## 全局验证与维护任务

### [TODO] T28 — 更新测试 fixture 与 mock LSP 覆盖矩阵

**依赖**：贯穿 L1-L6，可在每个 LSP 任务后增量维护。

**文件**：
- `crates/atto-ui-editor/src/bin/mock_lsp_server.rs`
- `crates/atto-ui-editor/tests/lsp_editor.rs`
- `crates/atto-ui-editor/tests/pty_editor.rs`
- 可能新增：
  - `crates/atto-ui-editor/tests/pty_diagnostics.rs`
  - `crates/atto-ui-editor/tests/pty_code_action.rs`
  - `crates/atto-ui-editor/tests/pty_signature_help.rs`
  - `crates/atto-ui-editor/tests/pty_inlay_hints.rs`

**步骤**：
1. mock LSP 支持：
   - publishDiagnostics
   - textDocument/codeAction
   - textDocument/prepareRename
   - textDocument/rename
   - textDocument/signatureHelp
   - textDocument/formatting
   - textDocument/inlayHint
   - document/workspaceSymbol
2. 每个 method 都提供 deterministic response，不依赖真实 language server。
3. PTY tests 使用固定 terminal size 和 `wait_for_text`。
4. 对跨文件 edit 使用 temp dir，测试结束清理。

**验收**：
- LSP UI 功能均能在无外部 LSP server 情况下测试。
- mock server 输出不污染 test stdout，失败时有足够日志。

### [TODO] R28 — 审阅 T28

审阅 T28 改动：
- 确认 mock LSP JSON-RPC framing 正确。
- 确认 tests 不依赖时序 sleep。
- 确认 temp files/directories 清理。
- 确认每个 LSP 功能至少有一个成功路径和一个 empty/error 路径测试。

### [TODO] T29 — 文档与实施顺序维护

**依赖**：每个阶段完成后执行。

**文件**：
- `PLAN-2.md`
- `TODO-2.md`
- `README.md`（如果公开行为/快捷键变化）
- `crates/atto-editor-app/examples/basic.rs`（如示例需更新）

**步骤**：
1. 每完成一个 T/R 对，按归档 TODO 风格追加“完成记录”，记录：
   - 改了哪些文件/类型/函数。
   - 新增哪些测试。
   - 跑了哪些命令。
   - 是否有遗留限制。
2. 如果实现偏离 `PLAN-2.md`，同步更新设计中的对应章节。
3. 新快捷键、菜单项、环境变量（如 LSP command）变化同步 README 或 crate docs。
4. 不创建临时计划 markdown；只维护 `TODO-2.md` / `PLAN-2.md`。

**验收**：
- 新 agent 接手时可以从 `TODO-2.md` 当前状态继续，不需要读聊天上下文。
- 已完成任务都有完成记录和验证命令。

### [TODO] R29 — 审阅 T29

审阅 T29 改动：
- 确认完成记录不是泛泛描述，能追踪文件和测试。
- 确认文档没有过期路径/函数名。
- 确认 README 只记录用户可见行为，不泄露内部实现细节。
