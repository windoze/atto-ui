# Atto UI 代码审查报告

> 审查范围：整个工作区（根 crate `atto-ui` + `crates/` 下 11 个子 crate），约 54,600 行 Rust 代码。
> 审查方法：按子系统分模块深度阅读源码（composable 核心层、窗口/应用层、反应式/运行时/宏、控件/文本/主题、上层 crate 与工作区架构），并运行 `cargo clippy --workspace --all-targets`。
> 审查日期：2026-06-05

---

## 一、执行摘要

整体质量：**良好但存在若干真实正确性 bug 与历史遗留的架构债**。

亮点：
- `#![forbid(unsafe_code)]` 全程坚持，无 unsafe。
- Clippy 极其干净（全工作区仅 3 条轻微告警），格式统一。
- PTY 集成测试框架设计出色，窗口管理/resize/placement 的边界处理严谨且有充分测试覆盖。
- TextBuffer 的 grapheme cluster 核心逻辑稳健（光标移动/删除经过 ZWJ emoji 测试）。
- Python 绑定采用轮询模型规避了跨语言 GIL 重入风险，安全性良好。
- 依赖图是清晰的 DAG，无循环依赖。

主要问题集中在三类：
1. **Unicode 列宽与字节索引混用**（状态栏、文本框选区）——TUI 最易出 bug 处。
2. **真实正确性 bug**：动态运行时 `move_node` 会丢失节点；滚动容器对超视口子项整块裁剪导致其消失。
3. **历史遗留架构债**：`cache` 模块与 `Observable` 是从未接线的死代码；`Component` 是 37 方法的 god trait；文档（CLAUDE.md 等）对增量渲染/声明式 API 的承诺与实现脱节。

---

## 二、严重问题（建议优先修复）

### S1. 状态栏用字节长度当列宽，CJK/emoji 会错位并可能 panic
`src/app/status.rs:28-41`。`draw` 全程用 `line.len()`（UTF-8 字节数）与终端列宽比较，并用 `line.truncate(width)` 截断。
- 中文/emoji 每字符多字节 → 右侧文本对齐错误。
- `String::truncate` 在非 char 边界会 **panic**；`width` 落在多字节字符中间即崩溃。
- 项目其它地方（`menu.rs`/`chrome.rs`）已正确使用 `UnicodeWidthStr`，此处是遗漏。

**建议**：改用 `unicode_width::UnicodeWidthStr::width` 计算列宽，用 grapheme 边界截断。

### S2. 动态运行时 `move_node` 在重插入失败时丢失节点
`src/runtime/mod.rs:644-652`。`move_node` 先 `take_node` 把节点摘出树，再 `insert_existing_node`。若目标父节点不存在或为 TabView（返回 false），被取出的 `node` 在函数返回后直接被 drop —— **视图树永久丢失该节点**。spec 端 `apply_tree_ops` 的 `Move` 同样是「先摘后插」，目标父不存在时 `self.root` 已被破坏，后续操作基于损坏的树。

**建议**：插入前先校验目标父节点存在；失败时把取出的节点放回原位，而非提前摘除。

### S3. 滚动容器对「未完全可见」的子项整块丢弃（渲染+命中都跳过）
`src/composable/stack/scrollbars.rs:60`、`src/composable/stack/events.rs:218` 用 `bounds_fully_visible` 决定是否绘制/命中子项。高度大于视口、或部分滚出视口的子项会被整体跳过 —— 既不渲染也无法点击。对单个高子项（长文本块）垂直滚动直接失效。

**建议**：改为相交测试 + 裁剪渲染（部分可见即裁剪绘制），而非「未完全可见则丢弃」。

### S4. TextBox 选区锚点未做 grapheme 对齐，可能在字符内部切割
`src/widgets/textbox.rs:200-206,475`、`src/text/buffer.rs:48-63`。鼠标点击宽字符右半格时 `set_cursor_display_col` 会对齐到字符起始字节，但 Shift+点击的 `selection_anchor` 直接存任意字节，未经 grapheme 对齐。后续 `selection_range` 按字节比较，`delete_selection` 的 `replace_range` 可能切在 grapheme 内部，导致 panic 或乱码。

**建议**：所有进入 buffer 的 byte index（含 selection_anchor）统一经 grapheme 对齐。

---

## 三、中等问题

### M1. `Component` 是 god trait（37 个方法），且 `Box<dyn Component>` 需手写全套透传
`src/composable/component.rs:167-508`。单 trait 混合了属性反射、命令、焦点、布局协商、三套事件（capture/bubble/handle）、8 个滚动方法、标题栏、动态树操作。`Box<dyn Component>` 的 impl 要手写约 40 个透传方法，新增方法须两处同步，极易漏。
**建议**：拆为 `Layout` / `Scrollable` / `FocusNav` / `DynamicTree` 等子 trait 用 supertrait 组合。

### M2. 事件分发模型 capture/bubble/handle 三套语义不清
`src/composable/stack/events.rs:301` 等。trait 同时对外暴露三个事件方法，但「框架何时调 capture、何时调 handle」无契约文档，包装层把三者都透传，易重复分发或漏分发。
**建议**：明确定义 capture→target→bubble 的调用时序，或收敛为单一 `handle_event` 内部编排。

### M3. 滚动键盘/滚轮逻辑在三处重复实现
`stack/events.rs:141`、`grid/events.rs:141`、`scroll_container/events.rs:9` 的 `scroll_by` / 方向键/PageUp/wheel 处理几乎逐字重复，而 `scroll.rs` 已有 `scroll_by_delta` 却未被复用。
**建议**：抽成共享的 `ScrollState` 方法。

### M4. `cache` 模块（VirtualBuffer/diff/scheduler）是完全未接线的死代码
`src/cache/` 全模块仅在 `lib.rs:6` 暴露，全工作区无任何调用点。实际渲染走 ratatui 自身的双缓冲 diff。CLAUDE.md 宣称的「增量差异计算减少重绘/脏标记精确追踪」与事实不符。
**建议**：删除 `cache` 模块，或在文档中明确标注为未接线/实验性。

### M5. `Observable` 是死代码且本身有通知正确性问题
`src/reactive/observable.rs`。全代码库无使用点。其 `set` 无判等、并发下回调收到的值可能与 `get()` 最终值背离。
**建议**：直接删除（项目实际用的是基于轮询的 `Property`/`DirtyFlag`，模型干净无泄漏）。

### M6. 增量更新动辄退化为全量 rebuild
`src/runtime/mod.rs:226-251`。root 节点 `SetProp` 时 `NotFound`/`NeedsRebuild`/`UnsupportedProperty` 都触发整树 `rebuild()`。许多常见属性的动态 set 因此退化为全量重建，「增量」名不副实。
**建议**：区分「属性不存在」与「组件不支持动态 set」。

### M7. 上层组件全部绕过声明式 API 手写 `impl Component`
全工作区 18 处 `impl Component for`，而 VStack/HStack/`.build()` 在 `atto-ui-editor`/`atto-ui-file-tree` 源码中出现 0 次。CLAUDE.md 反复强调「使用声明式 API 构建所有 UI」，与实现脱节。叶子级高频重绘组件手写有合理性，但文档需澄清分层约定。
**建议**：文档中明确「叶子级高性能组件可手写 Component，容器组合用声明式」。

### M8. 巨型文件职责过载
`crates/atto-ui-editor/src/view/mod.rs`（1971 行，单 struct 含 5+ 内嵌控制器）、`crates/atto-editor/src/window.rs`（1839 行，两个大组件挤一文件）、`src/runtime/mod.rs`（1851 行，CallbackHandle+注册表+树转发+20 个工厂+tree-ops+解析器）、`src/wm/manager/mod.rs`（972 行）、`src/app/menu.rs`（923 行）。
**建议**：按职责拆分子模块。

### M9. 窗口/节点查找全为 O(n) 线性扫描，单次事件多次重复扫描
`src/wm/manager/events.rs`、`focus.rs`、`z_order.rs` 遍布 `windows.iter().find(|w| w.id == id)`；运行时 tree-ops 的 `find_by_id_mut`/`take_by_id` 等全是 O(n) 全树递归 + 字符串比较。窗口/节点少时无碍，属技术债。
**建议**：引入 id→index/路径索引。

### M10. 控件层大量重复样板，缺共享抽象
`mouse_coords_local_to_area`/`contains` 在 textbox/table/list 三处逐字重复；三态 `base_style`（disabled/focused/normal）在 5 个 widget 反复手写；ListBox 与 TableView 的 selection/scroll 逻辑近乎雷同。
**建议**：抽出 `widget_style(theme, enabled, focused)` 与共享 selection/scroll mixin。

### M11. 每帧重复 clone bindings + 对全部数据 parse_inline
`src/widgets/list.rs:374-376,543-557`、`table.rs:617`。`bindings()` 每帧 clone 整个结构；`draw` 对每一项（含视口外）调 `parse_inline`。大数据集下 O(总行数)，违背「虚拟滚动高效」宣称。
**建议**：仅对可见行 parse；用 read guard 局部借用代替 clone。

---

## 四、轻微问题

- **L1. 坐标系启发式不可靠**：`src/composable/geom.rs:70-84` 用 `column < width && row < height` 区分绝对/相对坐标，落在窗口左上角而不属本容器的点会被误判命中。建议调用方显式传坐标系。
- **L2. Button 无命中判断**：`src/widgets/button.rs:88-91` 任意位置 `Down(Left)` 都 `trigger()`，未保存 `last_area` 做 contains 检查（其它 widget 都有）。当前靠父容器命中路由兜底，但属一致性缺失。
- **L3. `Size::Weight(u16)` 与文档不符**：`src/composable/layout.rs:95` 实现是 `u16`，CLAUDE.md 称权重为 `f32`，无法表达 0.5/1.5 比例。
- **L4. 主题命名样式每帧 HashMap 查找**：`table.rs:266,612`、`list.rs:542` 热路径上 `named_style("markdown-link")` 字符串哈希查找，可缓存为字段。
- **L5. `is_tab_view` 靠 `type_name().rsplit("::")` 字符串匹配识别**（runtime），重命名/移动模块即失效，脆弱。建议改 trait 方法或枚举。
- **L6. `view_builder!` 宏卫生性**：`crates/atto-ui-macros/src/view_builder.rs:85` 硬编码 `::atto_ui::...`，依赖被重命名即断；未知类型无 `compile_error!` 友好提示。
- **L7. `parse_color` 不支持 3 位 hex**：`src/theme/config.rs:121` 仅 `#RRGGBB`，`#fff` 报错。
- **L8. `ASYNC.md` 计划未落地**：规划的后台任务/async API 在 src 中 `tokio`/`async`/`.await` 命中 0 次，文档应标注「计划中」。
- **L9. `draw_shadow` 重复**：`src/wm/manager/draw.rs:114` 与 `src/app/menu.rs:868` 完全重复，应提取共享函数。
- **L10. clippy 三条告警**：`while let` 可简化、`if` 可并入 `match`、手动 checked division。可一键 `--fix`。

---

## 五、命名建议

`atto-editor`（应用层：窗口/Tab/分屏）与 `atto-ui-editor`（组件库：EditorView）**职责不同、非重复造轮子**，但命名过近极易误导。建议将 `atto-editor` 更名为 `atto-editor-app` 消歧义。`atto-ui-runtime` 当前仅被 `atto-ui-python` 引用，定位悬空，建议明确其是否应作为核心被组件 crate 共享，否则合并进 `atto-ui`。

---

## 六、修复优先级建议

| 优先级 | 条目 | 理由 |
|---|---|---|
| P0 | S1 状态栏字节宽度 | 可触发 panic，CJK 场景必现 |
| P0 | S2 move_node 丢节点 | 破坏视图树，难调试 |
| P0 | S4 选区 grapheme 对齐 | 可触发 panic/乱码 |
| P1 | S3 滚动整块裁剪 | 长内容滚动失效 |
| P1 | M4/M5 删除 cache/Observable 死代码 | 消除「两套割裂机制」假象，降低维护面 |
| P1 | 文档对齐（M7/L8/CLAUDE.md） | 增量渲染/声明式/async 的承诺与实现脱节 |
| P2 | M1/M2 trait 拆分与事件模型澄清 | 长期可维护性 |
| P2 | M3/M10/M11 去重与性能 | 减少漂移、提升大数据性能 |
| P3 | 其余轻微项 | 随手清理 |
