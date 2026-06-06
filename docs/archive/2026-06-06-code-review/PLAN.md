# Atto UI 代码审查问题落地计划

> 来源：`CODE_REVIEW.md`（审查日期 2026-06-05）
> 本计划针对审查报告中的所有问题，按优先级（P0 → P3）排期，给出每项的根因、改动点、实现步骤、测试策略与验收标准。
> 总体执行原则：**每个 P0/P1 项独立成 PR，先写复现测试再修复（红→绿）**；死代码删除与文档对齐可合并提交；P2/P3 重构在功能稳定后分批进行。

---

## 阶段总览

| 阶段 | 包含条目 | 目标 | 预估 |
|---|---|---|---|
| 阶段一（P0） | S1, S2, S4 | 消除可触发 panic 的正确性 bug | 优先，尽快合入 |
| 阶段二（P1） | S3, M4, M5, 文档对齐(M7/L8/CLAUDE.md) | 修复滚动失效 + 清除死代码 + 文档与实现对齐 | 紧随其后 |
| 阶段三（P2） | M1, M2, M3, M10, M11 | trait 拆分、事件模型澄清、去重与大数据性能 | 中期重构 |
| 阶段四（P3） | L1–L10, M6, M8, M9, 命名建议 | 技术债清理与一致性 | 随手 / 长期 |

---

## 阶段一：P0（可触发 panic 的正确性 bug）

### S1. 状态栏用字节长度当列宽 → CJK/emoji 错位且可能 panic
- **位置**：`src/app/status.rs:28-41`
- **根因**：`draw` 用 `line.len()`（UTF-8 字节数）与终端列宽比较，并用 `String::truncate(width)` 截断。`truncate` 落在非 char 边界会 panic；CJK/emoji 列宽 ≠ 字节数导致右对齐错位。
- **改动点**：
  1. 引入 `unicode_width::UnicodeWidthStr`（项目已依赖 `unicode-width` 0.2）。
  2. 用 `UnicodeWidthStr::width(s)` 计算 `left`/`right` 的列宽，替换所有 `.len()`。
  3. 截断改为按 grapheme 累加列宽直到不超过 `width`（参考 `text/buffer.rs:set_cursor_display_col` 的累加模式），保证落在 grapheme 边界。
  4. 填充空格数 = `width - left_width - right_width`，用 `saturating_sub` 防下溢。
  5. 注意宽字符跨边界时截断列宽可能差 1，需补空格对齐。
- **测试**（`tests/` 新增 `pty_status_bar.rs` 或在现有 desktop 测试扩展）：
  - 设置含中文/emoji 的 left/right，断言屏幕缓冲区中右文本贴右边界、无 panic。
  - 设置超长含 CJK 文本，窗口宽度故意落在宽字符中间，断言不 panic 且不截半个字。
- **验收**：`cargo test` 全绿；手动 `cargo run --example demo` 状态栏中文显示正确。

### S2. 动态运行时 `move_node` 重插入失败时丢失节点
- **位置**：`src/runtime/mod.rs:644-653`（`move_node`）、`677-709`（`insert_existing_node`）、`take_node`；以及 `apply_tree_ops` 中的 `TreeOp::Move` 处理。
- **根因**：`move_node` 先 `take_node` 摘出节点，再 `insert_existing_node`。若目标父不存在或为 TabView（返回 false），被摘出的 `node` 在函数返回后被 drop，节点永久丢失；`apply_tree_ops` 的 Move 也是「先摘后插」，失败时树已损坏。
- **改动点**：
  1. **先校验后摘除**：新增 `fn parent_exists_and_insertable(view, parent_id) -> bool`，递归判断目标父节点存在且非 TabView。
  2. `move_node` 改为：先校验目标可插入；不可插入则直接返回 `false`，**不调用 `take_node`**。
  3. 兜底保护：即使走 `insert_existing_node` 后 `node` 仍为 `Some`（理论不该发生），把它放回原父或原位置，绝不 drop。可在 `move_node` 末尾 `if let Some(orphan) = node { /* 放回 / 记录错误 */ }`。
  4. 同步修正 `apply_tree_ops` 的 `Move`：先校验目标父存在再执行，失败返回 `Err` 或保持树不变。
- **测试**（`tests/` 新增或扩展 runtime 单测，可用 Rust 单测而非 PTY）：
  - Move 到不存在的父 → 断言节点仍在原位、树完整。
  - Move 到 TabView 父 → 断言失败且节点未丢失。
  - 正常 Move → 断言节点出现在新父的指定 index。
- **验收**：新增单测全绿；现有动态注册测试不回归。

### S4. TextBox 选区锚点未做 grapheme 对齐 → 可能在字符内部切割
- **位置**：`src/widgets/textbox.rs:200-206`（Shift+点击锚点）、`209-214`（Drag）、`475` 附近（selection 使用）；`src/text/buffer.rs:48-63`。
- **根因**：`set_cursor_display_col` 会对齐到 grapheme 起始字节，但 `selection_anchor` 在 `cursor_before`（Shift+点击）与 Drag 初始化时直接存任意 byte index，未对齐。`selection_range`/`delete_selection` 按字节 `replace_range`，可能切在 grapheme 内部 → panic 或乱码。
- **改动点**：
  1. 在 `text/buffer.rs` 新增 `pub fn align_to_grapheme_boundary(&self, byte: usize) -> usize`，向下取整到最近的 grapheme 起始字节（用 `grapheme_indices(true)`）。
  2. TextBox 所有写入 `selection_anchor` 的地方统一过 `self.buffer.align_to_grapheme_boundary(...)`：`:202`、`:204`、`:211` 三处。
  3. `selection_range`/`delete_selection` 入口再加一道对齐保险（防御性，防止其他路径绕过）。
- **测试**（PTY，扩展 `tests/pty_mouse_support.rs` 或新增 `pty_textbox_selection.rs`）：
  - 输入含宽字符文本（如 `a你b好c`），Shift+点击宽字符右半格，按 Delete，断言不 panic 且删除的是完整字符。
  - Drag 选区跨宽字符，断言 `delete_selection` 后内容正确。
- **验收**：PTY 测试全绿；模糊点击 emoji/CJK 不再 panic。

---

## 阶段二：P1（滚动失效 + 死代码清除 + 文档对齐）

### S3. 滚动容器对「未完全可见」子项整块丢弃
- **位置**：`src/composable/stack/scrollbars.rs:60`（绘制）、`src/composable/stack/events.rs:218`（命中）。`bounds_fully_visible` 决定是否绘制/命中。
- **根因**：高度大于视口或部分滚出的子项被整体跳过 → 既不渲染也不可点击，单个高子项垂直滚动直接失效。
- **改动点**：
  1. 新增 `fn bounds_intersects_viewport(r, scroll, viewport) -> bool`（相交测试），替换绘制处的 `bounds_fully_visible` 判断（`scrollbars.rs:60`）。
  2. 绘制时对部分可见子项做**裁剪渲染**：计算子项与视口的交集 Rect 作为绘制区域；若 ratatui 的 `Frame` 无法直接限制子组件越界绘制，使用裁剪后的 `abs` Rect 传给 `child.view.draw`，并确保 `abs` 不超出 `inner`（用交集裁剪 x/y/width/height）。
  3. 命中测试（`events.rs:218`）改用相交测试 + 在交集区域内做 `contains`。
  4. 检查是否需保留 `bounds_fully_visible`（若别处仍用则保留，否则移除）。
- **风险**：裁剪渲染若子组件自身不感知裁剪区域，可能绘制越界覆盖滚动条。需验证 `inner` 边界与滚动条预留列不被覆盖。
- **测试**（扩展 `tests/pty_scrolling.rs`）：
  - 放置一个高度 > 视口的长文本子项，向下滚动，断言下半部分内容随滚动出现（而非整块消失）。
  - 部分可见子项上的点击能命中。
- **验收**：长内容滚动可见且可交互；现有滚动测试不回归。

### M4. 删除未接线的 `cache` 模块（死代码）
- **位置**：`src/cache/`（buffer.rs/diff.rs/scheduler.rs），仅 `lib.rs:6` 暴露，无调用点。
- **决策**：**删除**（实际渲染走 ratatui 双缓冲 diff，cache 模块从未接线）。
- **改动点**：
  1. 先 `grep -rn "VirtualBuffer\|cache::\|crate::cache" src crates` 确认零引用。
  2. 删除 `src/cache/` 目录与 `lib.rs` 中 `pub mod cache;`。
  3. 删除相关测试（若有）。
- **验收**：`cargo build`/`cargo test` 通过；CLAUDE.md 中「增量差异计算/脏标记精确追踪」描述同步修正（见文档对齐）。

### M5. 删除 `Observable` 死代码
- **位置**：`src/reactive/observable.rs`，无使用点，且 `set` 无判等、并发下回调值可能与 `get()` 背离。
- **决策**：**删除**（项目实际用基于轮询的 `Property`/`DirtyFlag`）。
- **改动点**：`grep` 确认零引用 → 删除文件 → 从 `reactive/mod.rs` 移除 `pub mod observable;` 及 re-export。
- **验收**：`cargo build`/`cargo test` 通过。

### 文档对齐（M7 / L8 / CLAUDE.md）
- **问题**：
  - M7：上层组件全部手写 `impl Component`，VStack/HStack/`.build()` 在 editor/file-tree crate 中出现 0 次，与「使用声明式 API 构建所有 UI」承诺脱节。
  - L8：`ASYNC.md` 规划的 async API 在 src 中 `tokio/async/.await` 命中 0 次。
  - M4/M5：CLAUDE.md 宣称增量渲染/脏标记，实际不符。
- **改动点**（仅文档）：
  1. CLAUDE.md「技术亮点 / 高性能渲染」：删除或修正「增量差异计算/脏标记精确追踪」表述，改为「依赖 ratatui 双缓冲 diff」。
  2. CLAUDE.md「代码约定」补充分层约定：**叶子级高频重绘组件可手写 `impl Component`，容器组合优先用声明式 API**。
  3. `ASYNC.md` 顶部标注「计划中（未落地）」。
  4. 同步移除 cache/Observable 在文档中的描述。
- **验收**：文档与代码事实一致，无误导承诺。

---

## 阶段三：P2（架构/性能重构）

### M1. 拆分 `Component` god trait（37 方法）
- **位置**：`src/composable/component.rs:167-508`。
- **方案**：用 supertrait 组合拆分：
  - `Layout`（布局协商/尺寸）、`Scrollable`（8 个滚动方法）、`FocusNav`（焦点）、`DynamicTree`（动态树操作 children_mut/tag 等）、`EventHandling`（capture/bubble/handle，与 M2 联动）。
  - `Component: Layout + Scrollable + FocusNav + ...` 保持对外统一。
- **关键收益**：`Box<dyn Component>` 透传减少；新增方法只需改对应子 trait。
- **步骤**：先按职责给方法分组（不改签名）→ 抽 trait → 调整 `Box<dyn>` impl → 编译驱动修复调用点。
- **风险**：改动面大，需在 S 系列 bug 稳定后做；建议单独大 PR + 全量 PTY 回归。

### M2. 澄清事件分发 capture/bubble/handle 语义
- **位置**：`src/composable/stack/events.rs:301` 等。
- **方案**：定义并文档化 `capture → target → bubble` 调用时序契约；或收敛为单一 `handle_event` 内部编排 capture/bubble。需先确定框架现有调用顺序（在 wm/desktop 分发处）。
- **与 M1 联动**：在 `EventHandling` 子 trait 上写明契约文档注释。
- **测试**：新增事件时序 PTY 测试（点击嵌套容器，断言 capture 与 bubble 各调用一次、顺序正确）。

### M3. 滚动键盘/滚轮逻辑三处重复 → 抽共享
- **位置**：`stack/events.rs:141`、`grid/events.rs:141`、`scroll_container/events.rs:9`；`scroll.rs` 已有 `scroll_by_delta` 未复用。
- **方案**：在 `ScrollState`（或 `scroll.rs`）上提供统一方法处理方向键/PageUp/PageDown/Home/End/滚轮 → delta 计算 → `scroll_by_delta`。三处改为调用共享方法。
- **测试**：现有 `pty_scrolling.rs`/`pty_horizontal_scrolling.rs` 覆盖回归。

### M10. 控件层重复样板 → 共享抽象
- **位置**：`mouse_coords_local_to_area`/`contains` 在 textbox/table/list 重复；三态 `base_style` 在 5 个 widget 重复；ListBox/TableView selection/scroll 雷同。
- **方案**：
  1. 抽 `pub(crate) fn widget_style(theme, enabled, focused) -> Style`。
  2. 抽共享 `mouse_coords_local_to_area`/`contains` 到 widgets 公共 util 模块。
  3. ListBox/TableView 的 selection/scroll 抽 mixin（trait 或 struct 组合）。
- **测试**：各 widget 现有 PTY 测试回归。

### M11. 每帧 clone bindings + 全量 parse_inline
- **位置**：`src/widgets/list.rs:374-376,543-557`、`table.rs:617`。
- **方案**：
  1. `bindings()` 用 read guard 局部借用替代整体 clone。
  2. `draw` 仅对**可见行区间**调 `parse_inline`（结合滚动 offset 与视口高度计算可见范围）。
- **测试**：扩展 `tests/pty_virtual_scrolling.rs`，大数据集（1000+ 行）渲染断言正确 + 性能不退化（行为正确性为主）。

---

## 阶段四：P3（技术债 / 一致性 / 命名）

> 这些为随手清理项，可批量提交，不阻塞功能。

- **M6. 增量更新退化为全量 rebuild**：`runtime/mod.rs:226-251`。区分 `PropertyApply::NotFound`（属性不存在）与 `UnsupportedProperty`（组件不支持动态 set），仅前者必要时 rebuild。需扩展 `PropertyApply` 枚举并修正分支。
- **M8. 巨型文件拆分**：`atto-ui-editor/src/view/mod.rs`(1971)、`atto-editor-app/src/window.rs`(1839)、`runtime/mod.rs`(1851)、`wm/manager/mod.rs`(972)、`app/menu.rs`(923)。按职责拆子模块，纯机械重构，逐文件单独 PR。
- **M9. O(n) 查找 → id 索引**：`wm/manager/events.rs|focus.rs|z_order.rs` 与 runtime tree-ops。引入 `HashMap<id, index/path>` 索引。低优先（窗口/节点少时无碍）。
- **L1. 坐标系启发式不可靠**：`composable/geom.rs:70-84`。改为调用方显式传坐标系（绝对/相对枚举参数），移除 `column<width && row<height` 猜测。
- **L2. Button 无命中判断**：`widgets/button.rs:88-91`。保存 `last_area`，`Down(Left)` 前做 `contains` 检查，与其他 widget 一致。
- **L3. `Size::Weight(u16)` 与文档不符**：`composable/layout.rs:95`。决策二选一：①改为 `f32` 支持 0.5/1.5（需改布局计算）；②保持 `u16` 并修正 CLAUDE.md。建议先修文档（低成本），`f32` 化作为后续增强。
- **L4. 主题命名样式每帧 HashMap 查找**：`table.rs:266,612`、`list.rs:542`。缓存 `named_style("markdown-link")` 为字段，主题切换时失效重取。
- **L5. `is_tab_view` 靠字符串匹配**：runtime 用 `type_name().rsplit("::")` 识别 TabView，脆弱。改为 `Component` 上的 trait 方法（如 `fn is_tab_container(&self) -> bool { false }`）。
- **L6. `view_builder!` 宏卫生性**：`atto-ui-macros/src/view_builder.rs:85` 硬编码 `::atto_ui::`。改用 `$crate` 等价方案或可配置 crate 路径；未知类型加 `compile_error!` 友好提示。
- **L7. `parse_color` 不支持 3 位 hex**：`theme/config.rs:121`。支持 `#fff` → 扩展为 `#ffffff`。
- **L9. `draw_shadow` 重复**：`wm/manager/draw.rs:114` 与 `app/menu.rs:868` 完全重复，提取共享函数。
- **L10. clippy 三条告警**：`cargo clippy --workspace --all-targets --fix` 一键修复（while let 简化、if 并入 match、checked division）。
- **命名消歧义（已确认并实施，2026-06-06）**：维护者经 T13A 确认后，T13 将应用 crate 改为 `atto-editor-app`，并将原独立 runtime crate 合并为 `atto-ui::runtime` 内部模块；不再保留独立 runtime crate。涉及 Cargo workspace 改名，影响面大，逐步实施并全量回归。

---

## 测试与回归约定

- 每个 P0/P1 修复**先写复现测试**（红），再修复（绿）。
- 优先复用现有 PTY 框架：`PtyTestHost::spawn(snapshot_app, ...)`，必要时在 `src/bin/` 新增专用 snapshot app。
- 纯逻辑 bug（如 S2 move_node）用 Rust 单元测试，无需 PTY。
- 每次提交前：`cargo fmt && cargo clippy --workspace --all-targets && cargo test`。
- 重构类 PR（M1/M8）必须全量 PTY 回归，确保零行为变更。

## 提交拆分建议

1. PR #1：S1 状态栏列宽（+ 测试）
2. PR #2：S2 move_node 防丢失（+ 单测）
3. PR #3：S4 选区 grapheme 对齐（+ 测试）
4. PR #4：S3 滚动相交裁剪（+ 测试）
5. PR #5：删除 cache + Observable 死代码
6. PR #6：文档对齐（CLAUDE.md / ASYNC.md）
7. 之后：P2 重构分批，P3 随手清理批量提交
