# QUALITY_PLAN

## 发现

- 多个模块重复实现几何/鼠标辅助函数：`contains`、`mouse_coords_local_to_area`、`clamp_u16`、`position_anchored`、`align_within`、`tab_direction_for_event`、`focusable_children_in_tab_order`，分散在 `src/composable/stack.rs`、`src/composable/grid.rs`、`src/composable/border.rs`、`src/composable/scroll_container.rs`、`src/composable/splitter.rs`。
- 关键文件体量偏大且职责混杂：`src/widgets/markdown.rs` (~3421 行)、`src/composable/stack.rs` (~3120 行)、`src/editor/view.rs` (~3008 行)、`src/wm/manager.rs` (~2228 行)、`src/composable/grid.rs` (~1598 行)、`src/composable/scroll_container.rs` (~1088 行)。
- `VStack` 与 `HStack` 在 `src/composable/stack.rs` 中字段与逻辑高度相似，存在可抽象空间。

## 目标

- 减少重复代码与隐性分叉，降低维护成本。
- 拆分多职责大文件，使核心逻辑更易测试与定位。
- 保持现有 API 行为尽量稳定，避免破坏性变更。
- 将 MarkdownViewer 与 Editor 拆分为独立 crate，缩小主 crate 依赖与编译负担。

## 重构计划（分阶段）

### 1) 提取 composable 通用几何/输入工具（已完成）

**动机**：重复实现增加维护成本，容易出现边界行为不一致。

**动作**：
- 新增 `src/composable/geom.rs`（或 `src/composable/utils.rs`），收拢以下函数：
  - `contains`、`mouse_coords_local_to_area`、`clamp_u16`
  - `position_anchored`、`align_within`
  - `tab_direction_for_event`、`focusable_children_in_tab_order`
- 在 `stack.rs`、`grid.rs`、`border.rs`、`scroll_container.rs`、`splitter.rs` 中统一改为 `use` 引用。
- 在 `src/composable/tests.rs` 增加边界用例（如 0 宽高、偏移溢出、嵌套坐标等）。

### 2) 统一 Stack 轴向实现，保留对外类型（已完成）

**动机**：`VStack`/`HStack` 逻辑重复，修改易漏。

**动作**：
- 引入 `StackAxis { Vertical, Horizontal }` 与内部 `StackCore`，集中布局、滚动、focus、事件处理逻辑。
- `VStack`/`HStack` 变为轻量封装（保持现有 API），调用 `StackCore` 的共用实现。
- 将滚动条绘制与拖拽处理放入共享函数，减少复制粘贴。

### 3) 拆分 `stack.rs` / `grid.rs` / `scroll_container.rs` 的内部模块（已完成）

**动机**：单文件过大，职责混合导致改动成本高。

**动作**：
- 以目录化方式拆分：
  - `src/composable/stack/` → `mod.rs` + `layout.rs` + `events.rs` + `scrollbars.rs`
  - `src/composable/grid/` → `mod.rs` + `layout.rs` + `events.rs`
  - `src/composable/scroll_container/` → `mod.rs` + `events.rs` + `scrollbars.rs`
- 通过 `pub use` 维持外部路径稳定。

### 4) 抽离 MarkdownViewer 为独立 crate（先于内部拆分）（已完成）

**动机**：Markdown 相关依赖较重且逻辑复杂，独立 crate 便于隔离依赖、缩小主 crate 体积，并为后续拆分做铺垫。

**动作**：
- 新增 crate：`crates/atto-ui-markdown/`，导出 `MarkdownViewer` 与必要的配置类型。
- 主 crate 中保留轻量 re-export（`pub use atto_ui_markdown::MarkdownViewer`），避免破坏对外 API。
- 重新整理依赖：`pulldown_cmark` 等仅在新 crate 中引入；主 crate 依赖尽量保持精简。
- 将现有实现从 `src/widgets/markdown.rs` 迁移到新 crate，主 crate 仅保留最小 glue 代码。
- 在新 crate 内完成模块化拆分（见下一步）。

### 5) 模块化 Markdown Viewer（在新 crate 内部）（已完成）

**动机**：原 `markdown.rs` 混合了解析、布局、渲染、事件处理、嵌入滚动条等，修改风险高。

**动作**：
- 将代码拆成目录：`crates/atto-ui-markdown/src/markdown/` 下的模块（`lib.rs` 保持薄入口 + re-export）。
- 建议拆分模块：
  - `parser.rs`（pulldown_cmark → 内部块结构）
  - `layout.rs`（行宽计算与布局）
  - `render.rs`（Ratatui 绘制）
  - `events.rs`（鼠标/键盘/链接回调）
  - `embedded_scrollbar.rs`（表格/代码块滚动条）
  - `styles.rs`（样式/主题）
- 引入 `MarkdownCache`（包装 `parsed/layout` 及 dirty 逻辑）以降低状态分散。
- 补充解析/布局相关单元测试：`crates/atto-ui-markdown/src/markdown/tests.rs`。

### 6) 抽离 Editor 为独立 crate（先于内部拆分）（已完成）

**动机**：Editor 依赖 LSP/高亮/外部命令等，独立 crate 便于隔离依赖并提升主 crate 编译速度。

**动作**：
- 新增 crate：`crates/atto-ui-editor/`，导出 `EditorView` 与配置/事件类型。
- 主 crate 中保留轻量 re-export（`pub use atto_ui_editor::EditorView`），避免破坏对外 API。
- 将 `src/editor/*` 迁移到新 crate；主 crate 仅保留 re-export 与最小 glue 代码。
- 在新 crate 内进行模块化拆分（见下一步）。

### 7) 模块化 Editor View（在新 crate 内部）（已完成）

**动机**：`view.rs` 同时处理输入、渲染、LSP、语法高亮、弹窗等职责。

**动作**：
- 拆分为 `crates/atto-ui-editor/src/view/`：`mod.rs` + `input.rs` + `render.rs` + `lsp.rs` + `syntax.rs` + `selection.rs`。
- 引入 `EditorLspController` 或类似结构集中 LSP 连接与消息转换，减少主视图状态膨胀。
- 新增针对输入映射与选择框行为的测试（可复用现有 PTY 测试骨架）。

### 8) 低风险清理（可选）（已完成）

- 将 `src/wm/manager.rs` 目录化为 `src/wm/manager/`，并拆出子模块以降低认知负担：
  - `draw.rs`：渲染（`WindowManager::draw`）与阴影/填充等绘制辅助函数
  - `events.rs`：事件路由、命中测试、鼠标拖拽（含窗口边框滚动条交互）
  - `focus.rs`：focus/modal 管理（`focus_next`/`active_modal_id` 等）
  - `z_order.rs`：z-order（如 `bring_to_front`）
  - `placement.rs`：移动/缩放/最大化与几何夹紧（`normalize_rect` 等）
  - `chrome.rs`：titlebar/按钮与边框滚动条绘制、命中测试等 window chrome 逻辑
- 为复杂函数添加简短说明性注释（已覆盖滚动条几何与命中测试的关键路径）。

## 风险与验证

**风险**：
- focus/滚动/鼠标事件边界行为改变。
- Markdown 布局与渲染细节变化。
- Editor LSP 生命周期或弹窗触发时机变化。

**验证建议（基础）**：
- 运行 `cargo test`（含 PTY 测试）。
- 手动跑 `cargo run --example demo` 与 `cargo run --bin snapshot_app` 进行 UI 回归确认。

## 全面测试计划（覆盖所有功能）

### A. 构建与依赖边界
- `cargo build` / `cargo test` 在拆分前后对比编译时间与依赖树，确保主 crate 依赖缩减。
- 验证主 crate 对外 API 未破坏（re-export 保持）。

### B. Composable 与布局系统
- `VStack` / `HStack`：padding/spacing/scrollable/scrollbars/tab order/焦点移动。
- `Grid`：对齐、权重、固定/内容/填充尺寸、鼠标命中与滚动条。
- `ScrollContainer`：嵌套滚动、鼠标拖拽滚动条、滚轮与键盘滚动。
- `Border` / `Splitter`：鼠标坐标转换、命中测试边界、拖拽分割条。

### C. MarkdownViewer（新 crate）
- 解析：标题、列表、表格、代码块、引用、链接、内联样式。
- 布局：自动换行、宽度变化、表格/代码块高度上限、嵌入滚动条。
- 交互：滚动条拖拽、滚轮、链接点击回调、鼠标 hover 样式。
- 主题：前景/背景覆盖、样式继承、marker 显示开关。
- 断言方式：单元测试覆盖解析/布局；PTY 测试覆盖渲染与交互。

### D. Editor（新 crate）
- 基础编辑：插入/删除/撤销/重做、选择/矩形选择、多光标。
- 输入映射：快捷键、Tab 行为、组合键冲突。
- 滚动与渲染：长文档、横向滚动、光标可见性、行高/列宽。
- 语法高亮：Regex/Sublime 路径、主题切换、性能回退。
- LSP：hover/completion/goto/diagnostics 生命周期，断连重连。
- 弹窗：Hover/Completion 弹出与关闭逻辑、定位与遮挡。
- 断言方式：单元测试覆盖状态机与输入映射；PTY 测试覆盖 UI 行为。

### E. Window Manager / Desktop
- 多窗口 focus/z-order/移动/尺寸约束。
- 状态栏/菜单交互回归。
- 鼠标与键盘事件路由到正确窗口与组件。

### F. 回归与性能
- 关键路径 smoke：`examples/demo.rs`、`snapshot_app`、`snapshot_*` 系列。
- 性能基线：markdown 大文档 / editor 大文件 / 多窗口场景。
- 可选：在 `tests/pty_*` 增加大文件场景与交互脚本。
