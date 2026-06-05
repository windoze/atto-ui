# Atto UI 修复任务列表

> 来源：`PLAN.md`（基于 `CODE_REVIEW.md`，审查日期 2026-06-05）
> 说明：每个「修复任务」(T) 后紧跟一个「审阅任务」(R)，R 用于审阅前一个 T 的质量与正确性。
> 通用要求（每个 T 完成前必须满足）：`cargo fmt && cargo clippy --workspace --all-targets && cargo test` 全绿。
> 行号均基于审查时快照，执行前如有偏移以函数名/符号为准。

---

## 阶段一：P0（可触发 panic 的正确性 bug）

### [DONE] T1 — 修复状态栏字节宽度（S1）
**文件**：`src/app/status.rs`（`StatusBar::draw`，第 24-45 行）
**现状**：`draw` 用 `self.left.clone()` + `line.len()`（UTF-8 字节数）与 `width`（列数）比较；超长时 `line.truncate(width)`。`String::truncate` 落在非 char 边界 panic；CJK/emoji 列宽 ≠ 字节数导致右对齐错位。
**依赖**：`unicode-width` 0.2 已在依赖中；`menu.rs`/`chrome.rs` 已用 `UnicodeWidthStr`，可参考其用法。
**步骤**：
1. 顶部 `use unicode_width::UnicodeWidthStr;`。
2. 计算 `left_w = UnicodeWidthStr::width(self.left.as_str())`，`right_w` 同理。
3. 重写布局逻辑：
   - 若 `left_w >= width`：按 grapheme 累加列宽截断 left 到不超过 `width`（参考 `src/text/buffer.rs:48-63` `set_cursor_display_col` 的累加循环：`for (byte_idx, g) in s.grapheme_indices(true)` 累加 `UnicodeWidthStr::width(g)`，超出即在该 grapheme 边界切断）。
   - 否则：`remaining = width - left_w`；若 `right_w <= remaining`，填充 `remaining - right_w` 个空格再接 right；否则填充 `remaining` 个空格（不显示 right，或同样按 grapheme 截断 right）。
4. 所有减法用 `saturating_sub`。注意宽字符截断后实际列宽可能比 `width` 少 1，需补空格补齐到 `width` 以保证背景样式铺满。
5. 需要 `unicode-segmentation`（已是依赖）做 `grapheme_indices`。
**测试**：新增 `tests/pty_status_bar.rs`（或扩展 `tests/pty_desktop.rs`）。需要 snapshot_app 能设置状态栏文本——先检查 `src/bin/snapshot_app.rs` 是否暴露设置入口，若无则在该 app 中加一个可通过按键设置含中文/emoji 状态栏文本的分支。
- 用例 A：left=中文、right=emoji，断言屏幕中 right 贴右边界、无 panic。
- 用例 B：left 为超长 CJK，窗口宽度故意落在宽字符中间，断言不 panic 且不出现半个字。
**验收**：两用例通过；`cargo run --example demo` 状态栏中文显示正确对齐。

**完成记录（2026-06-06）**：
- `StatusBar::draw` 改为基于 `UnicodeWidthStr::width` 计算列宽，移除状态栏布局中的 `.len()` 字节宽度判断。
- 新增 grapheme 边界截断逻辑，超长左侧文本会按完整 grapheme 累加到不超过状态栏宽度，并补空格铺满整行。
- `snapshot_app` 新增 `--status-unicode` / `--status-long-cjk` fixture 和 F3/F4/F5 状态栏切换入口，便于 PTY 覆盖中文、emoji 与宽字符截断路径。
- 新增 `tests/pty_status_bar.rs` 覆盖中文 left + emoji right 的右边界对齐，以及宽度落在 CJK 字符中间时不 panic、不出现半个字符。
- 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --test pty_status_bar`；`cargo test --all --all-targets` 全部通过。
- 为满足当前 `-D warnings` 验收，顺带修复 3 个既有 clippy 阻塞项：`src/composable/splitter.rs`、`crates/atto-ui-file-tree/src/lib.rs`、`crates/atto-ui-editor/src/bin/mock_lsp_server.rs`。

### [DONE] R1 — 审阅 T1
审阅 T1 改动：
- 确认彻底移除了 `.len()` 作为列宽的用法，全部改为 `UnicodeWidthStr::width`。
- 确认截断在 grapheme 边界，构造极端输入（emoji ZWJ 序列、混合 CJK）人工/测试验证不 panic。
- 确认右对齐在纯 ASCII、纯 CJK、混合三种情况下都正确，背景样式铺满整行无空洞。
- 确认测试真实覆盖 panic 路径（width 落在多字节字符中间）。
- 运行 `cargo test` 与 demo 目视检查。

**完成记录（2026-06-06）**：
- 审阅 `src/app/status.rs`：状态栏布局路径已使用 `UnicodeWidthStr::width` 计算列宽，未继续用 `.len()` 作为显示宽度；超长左侧文本在 grapheme 边界截断并补空格铺满宽度。
- 审阅 `src/bin/snapshot_app.rs` 与 `tests/pty_status_bar.rs`：fixture 覆盖中文/emoji 右对齐和宽度落在 CJK 字符中间时的无 panic 截断路径。
- 补充 `status_line_right_aligns_ascii`、`status_line_right_aligns_cjk`、`status_line_right_aligns_mixed_width_text` 单元测试，固定验证纯 ASCII、纯 CJK、混合宽度文本右对齐。
- 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test status_line`；`cargo test --test pty_status_bar`；`cargo test --all --all-targets` 全部通过。
- demo 检查：以 100x30 伪终端运行 `cargo run --example demo`，确认示例绘制状态栏并可用 `q` 正常退出。

### [DONE] T2 — 修复 `move_node` 重插入失败丢节点（S2）
**文件**：`src/runtime/mod.rs`
**相关符号**：`move_node`(644-653)、`take_node`(655-675)、`insert_existing_node`(677-709)、`is_tab_view`(519)、`apply_tree_ops` 中 `TreeOp::Move`(307-322)。
**现状**：`move_node` 先 `take_node` 摘出节点，再 `insert_existing_node`；若目标父不存在或为 TabView，`node` 在返回后被 drop → 节点永久丢失。`TreeOp::Move`(317) 调用失败时走 `rebuild()`，但此时树已被 `take_node` 破坏。
**步骤**：
1. 新增 `fn can_insert_into(view: &dyn Component, parent_id: &str) -> bool`：递归查找 tag==parent_id 的节点，存在且 `!is_tab_view(node)` 返回 true。
2. 改写 `move_node`：先 `if !can_insert_into(view, new_parent_id) { return false; }`，再 `take_node`，再 `insert_existing_node`。
3. 兜底：`insert_existing_node` 返回后若 `node` 仍为 `Some`（理论不该发生），不能 drop——记录错误并把节点放回（可简单 append 回原 take 处或返回 false 触发上层处理）。因为已先校验，此分支应不可达，加 `debug_assert!(node.is_none())`。
4. `TreeOp::Move`(317)：因 `move_node` 现在失败前不破坏树，`rebuild()` 兜底仍安全；确认逻辑无需额外改动，但补注释说明「move_node 失败时树保持不变」。
**测试**：在 `src/runtime/` 增加 Rust 单元测试（无需 PTY）。参考现有 runtime 测试构造一棵小树（用 registry 构建 VStack 含若干带 tag 的子节点）：
- 用例 A：Move 到不存在的 parent_id → 断言返回 false 且原节点仍在原位、子节点总数不变。
- 用例 B：Move 到 TabView 类型父 → 断言失败且节点未丢失。
- 用例 C：正常 Move → 断言节点出现在新父指定 index。
**验收**：三用例通过；现有动态注册/tree-ops 测试不回归。

**完成记录（2026-06-06）**：
- `move_node` 新增 `can_insert_into` 预校验，目标父不存在或为 `TabView` 时会在摘除节点前直接返回失败。
- `take_node` 现在记录原父路径与原索引；若后续插入未消费节点，会把节点恢复回原位置并返回失败，避免失败路径 drop 节点。
- `TreeOp::Move` 调用处补充注释，明确 `move_node` 失败时视图树保持完整，触发 rebuild 仍安全。
- 新增三条 runtime 单元测试覆盖：移动到不存在父节点、移动到 `TabView` 父节点、正常移动到指定 index。
- 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test move_node --lib`；`cargo test --all --all-targets` 全部通过。

### [DONE] R2 — 审阅 T2
审阅 T2 改动：
- 确认「先校验后摘除」逻辑正确，所有失败路径都不会 drop 已摘出节点。
- 检查 `can_insert_into` 与 `take_node`/`insert_existing_node` 的 TabView 判定一致（都走 `is_tab_view`）。
- 确认 `apply_tree_ops` 的 Move 失败兜底不会基于损坏的树继续。
- 检查单测是否真实覆盖「目标父不存在」「TabView 父」两条原本丢节点的路径。
- 运行 `cargo test`。

**完成记录（2026-06-06）**：
- 审阅 `src/runtime/mod.rs`：`move_node` 已在摘出节点前通过 `can_insert_into` 校验目标父存在且不是 `TabView`；`take_node`、`insert_existing_node`、恢复路径均保留被摘出的节点，不会在失败路径 drop。
- 审阅并修复 spec 层 Move 原子性缺口：`atto-ui-runtime::apply_tree_ops` 现在会在摘出前校验源节点、目标父存在，并拒绝移入自身/后代，避免 `ComponentTree::apply_ops_incremental` 在失败时留下损坏的 `root` spec。
- 补充测试覆盖：spec Move 缺失父节点保持树不变、移入后代保持树不变、`ComponentTree::apply_ops_incremental` 缺失父节点保持 root/view 不变、`move_node` 目标为叶子父节点时恢复已摘出节点。
- 复核 T2 原测试：目标父不存在、目标父为 `TabView`、正常移动到指定 index 均覆盖并通过。
- 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test move_node --lib`；`cargo test -p atto-ui-runtime tree_ops_move`；`cargo test component_tree_incremental_move --lib`；`cargo test --all --all-targets` 全部通过。

### [DONE] T3 — TextBox 选区锚点 grapheme 对齐（S4）
**文件**：`src/widgets/textbox.rs`、`src/text/buffer.rs`
**相关符号**：textbox `selection_anchor` 写入点 202/204/206/211、`selection_range`(475)、`delete_selection`(506-519)、`cursor_byte_index`；buffer `set_cursor_display_col`(48-63)。
**现状**：`set_cursor_display_col` 会对齐到 grapheme 起始字节，但 Shift+点击的 `cursor_before`(199) 与 Drag 初始化(211) 直接存任意 byte index。`selection_range` 按字节取 min/max，`delete_selection` 用 `replace_range(start..end)` 可能切在 grapheme 内部 → panic/乱码。
**步骤**：
1. `src/text/buffer.rs` 新增 `pub fn align_to_grapheme_boundary(&self, byte: usize) -> usize`：用 `self.text.grapheme_indices(true)` 找到 `<= byte` 的最大 grapheme 起始字节；若 `byte >= text.len()` 返回 `text.len()`。
2. textbox 中所有写入 `selection_anchor` 的位置统一对齐：
   - 199：`let cursor_before = self.buffer.align_to_grapheme_boundary(self.buffer.cursor_byte_index());`（或确认 `cursor_byte_index` 已对齐则只需处理裸 byte 来源）。重点是 202/204 存入 anchor 前对齐。
   - 211：`self.selection_anchor = Some(self.buffer.align_to_grapheme_boundary(self.buffer.cursor_byte_index()));`
   - 其他 `ensure_selection_anchor`(484) 用的是 `cursor_byte_index`，若该值始终对齐则无需改，但仍建议统一过对齐函数防御。
3. `delete_selection`(506) 入口对 `start`/`end` 再各 `align_to_grapheme_boundary` 一次作为最终保险。
**测试**：扩展 `tests/pty_mouse_support.rs` 或新增 `tests/pty_textbox_selection.rs`。需 snapshot_app 有含宽字符的 TextBox（检查现有 app，如无则在 textbox 演示窗口预填 `a你b好c`）。
- 用例 A：Shift+点击宽字符右半格 → Delete → 断言不 panic 且删除完整字符。
- 用例 B：Drag 选区跨宽字符 → Delete → 断言内容正确。
**验收**：用例通过；模糊点击 CJK/emoji 不再 panic。

**完成记录（2026-06-06）**：
- `TextBuffer` 新增 `align_to_grapheme_boundary`，按 `grapheme_indices(true)` 将任意 byte offset 对齐到不大于该位置的 grapheme 起始边界，并覆盖空字符串、0、len、CJK/emoji 内部 byte 与超大 offset。
- `TextBox` 的 `selection_anchor` 写入统一收敛到边界对齐 helper；`selection_range` 与 `delete_selection` 也做防御性对齐，避免 `replace_range` 切入 grapheme 内部。
- `snapshot_app` 新增 `--textbox-unicode` fixture，默认行为保持 `hello` 不变，测试 fixture 使用 `a你b好c`。
- `atto-ui-test-host` 新增 `shift_click`，新增 `tests/pty_textbox_selection.rs` 覆盖 Shift+点击 CJK 右半格后 Delete、拖拽跨 CJK 后 Delete，断言删除完整 grapheme 范围且不 panic。
- 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test align_to_grapheme_boundary --lib`；`cargo test --test pty_textbox_selection`；`cargo test --all --all-targets` 全部通过。

### [DONE] R3 — 审阅 T3
审阅 T3 改动：
- 确认 `align_to_grapheme_boundary` 边界正确：byte=0、byte=len、byte 落在宽字符中间、空字符串。
- 确认 textbox 所有 anchor 写入点都已对齐，无遗漏路径（grep `selection_anchor = Some`）。
- 确认 `delete_selection` 的 `replace_range` 不可能切在 grapheme 内部。
- 检查测试是否真实触发了原 panic 场景。
- 运行 `cargo test`。

**完成记录（2026-06-06）**：
- 审阅 `src/text/buffer.rs`：`align_to_grapheme_boundary` 覆盖空字符串、0、len、CJK 内部 byte、emoji grapheme 内部 byte 与超大 offset，返回不大于输入位置的 grapheme 起始边界或 `text.len()`。
- 审阅 `src/widgets/textbox.rs`：`selection_anchor` 写入收敛到 `set_selection_anchor`，`selection_range` 与 `delete_selection` 均重新对齐边界，`replace_range` 不会切入 grapheme 内部。
- 审阅 `tests/pty_textbox_selection.rs` 与 `src/bin/snapshot_app.rs`：`--textbox-unicode` fixture 使用 `a你b好c`，PTY 测试真实覆盖 Shift+点击宽字符内部删除与拖拽跨宽字符删除路径。
- 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test align_to_grapheme_boundary --lib`；`cargo test --test pty_textbox_selection`；`cargo test --all --all-targets` 全部通过。

---

## 阶段二：P1（滚动失效 + 死代码清除 + 文档对齐）

### [DONE] T4 — 滚动容器相交裁剪渲染与命中（S3）
**文件**：`src/composable/stack/scrollbars.rs`、`src/composable/stack/events.rs`
**相关符号**：绘制处 `scrollbars.rs:60`（`bounds_fully_visible` 判断 + `abs` Rect 计算 63-68）；命中处 `events.rs:218`；`bounds_fully_visible` 定义 `events.rs:169`（grid 同名 `grid/events.rs:169`）。
**现状**：`if scrollable && !Self::bounds_fully_visible(r, scroll, viewport_size) { continue; }` 导致高度 > 视口或部分滚出的子项被整块跳过 → 既不渲染也不可点击。
**步骤**：
1. 在 `events.rs:169` 附近新增 `pub(super) fn bounds_intersects_viewport(r, scroll, viewport) -> bool`（content 坐标系下子项 rect 与 `[scroll, scroll+viewport)` 区间相交）。
2. 绘制处 `scrollbars.rs:60` 改为 `if scrollable && !Self::bounds_intersects_viewport(r, scroll, viewport_size) { continue; }`。
3. 裁剪：当前 `abs`(63-68) 用 `saturating_sub(scroll)` 平移，部分滚出顶部时 `r.y < scroll.y` 会因 saturating 归零导致错位——需正确裁剪：
   - 计算子项在视口内可见的起始偏移，调整 `abs.y`/`abs.height`（及 x/width）使其落在 `inner` 内且不覆盖滚动条预留列。
   - 用 `inner` 与平移后矩形求交集得到最终绘制 Rect；若子组件自身按传入 Rect 绘制，裁剪交集即可避免越界。
4. 命中处 `events.rs:218` 改用 `bounds_intersects_viewport` + 在交集区域内 `contains(child.bounds(), content_x, content_y)`。
5. 检查 `bounds_fully_visible` 是否仍有其他调用点；若仅这两处使用则可移除，否则保留。
**风险**：裁剪渲染需验证不覆盖滚动条预留列与窗口边框。
**测试**：扩展 `tests/pty_scrolling.rs`。需 snapshot app 有「单个高度 > 视口的长子项」场景（检查 `snapshot_scroll_app.rs`，如无则添加一个高文本块）。
- 用例 A：向下滚动，断言长子项下半部分随滚动出现（非整块消失）。
- 用例 B：部分可见子项上的点击能命中（返回正确 child id / 触发交互）。
**验收**：长内容滚动可见可交互；现有 `pty_scrolling.rs`/`pty_horizontal_scrolling.rs` 不回归。

**完成记录（2026-06-06）**：
- 新增 `src/composable/clipped.rs`，提供滚动视口相交计算、离屏渲染与可见区域拷贝，支持子组件顶部/左侧滚出视口时只绘制交集并保留可见区域背景与光标映射。
- `StackCore` 与 `Grid` 的滚动绘制路径从“必须完全可见”改为“与视口相交即可绘制”，部分可见子项会按 `inner` 内容区裁剪，不覆盖滚动条预留区域或窗口边框。
- `StackCore` 与 `Grid` 的鼠标命中路径改用相交判断，点击部分可见子项时按内容坐标映射到正确 child-local 坐标。
- `snapshot_scroll_app` 新增 `--long-child` fixture，包含单个高于视口的可点击长子项，用于覆盖原本整块跳过的渲染和命中路径。
- `tests/pty_scrolling.rs` 新增长子项滚动可见与部分可见点击命中测试；现有滚动条、键盘、鼠标滚轮测试保持通过。
- 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --test pty_scrolling`；`cargo test --test pty_horizontal_scrolling`；`cargo test --all --all-targets` 全部通过。

### [DONE] R4 — 审阅 T4
审阅 T4 改动：
- 确认相交测试与裁剪逻辑正确，尤其「子项顶部滚出视口」时 `abs.y`/`height` 计算无 off-by-one、无 saturating 归零错位。
- 目视 demo 滚动演示，确认部分可见内容正确裁剪显示、不覆盖滚动条与边框。
- 确认命中测试与渲染裁剪坐标系一致。
- 检查是否遗漏 grid 容器同类问题（`grid/events.rs:169`），如有应一并修或单列任务说明。
- 运行 `cargo test` + demo 目视。

**完成记录（2026-06-06）**：
- 审阅 `src/composable/clipped.rs`：`scrolled_region` 使用半开区间求交，子项顶部/左侧滚出视口时 `source` 与 `dest` 坐标保持一致，不再依赖 `saturating_sub` 将负向偏移归零。
- 审阅 `src/composable/stack/scrollbars.rs`、`src/composable/stack/events.rs`：绘制和命中均使用相同的相交/裁剪坐标系，部分可见子项命中后会按 `scroll` 映射为 child-local 鼠标坐标。
- 审阅 `src/composable/grid/scrollbars.rs`、`src/composable/grid/events.rs`：grid 路径已同步使用共享裁剪 helper 与相交命中逻辑，未遗漏同类问题。
- 补充 `scrolled_region` 单元测试，覆盖子项顶部滚出、左侧滚出、边界相切不相交、零尺寸区域拒绝，固定 off-by-one 与坐标映射风险。
- demo 检查：以 100x30 伪终端运行 `cargo run --example demo`，打开滚动 demo，确认滚动内容可见并可正常退出。
- 验证：`cargo fmt`；`cargo test scrolled_region --lib`；`cargo test --test pty_scrolling`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets` 全部通过。

### [DONE] T5 — 删除 cache + Observable 死代码（M4 + M5）
**文件**：`src/cache/`（整目录）、`src/lib.rs:6`、`src/reactive/observable.rs`、`src/reactive/mod.rs:7,13`
**步骤**：
1. 先确认零引用：`grep -rn "VirtualBuffer\|crate::cache\|cache::\|Observable\|observable::" src crates examples tests`。若有任何生产引用则停止并上报（计划假定为零）。
2. 删除 `src/cache/` 整目录；移除 `src/lib.rs:6` 的 `pub mod cache;`。
3. 删除 `src/reactive/observable.rs`；移除 `src/reactive/mod.rs:7` 的 `mod observable;` 与 `:13` 的 `pub use observable::Observable;`。
4. 删除两模块自带的测试（若有）。
**验收**：`cargo build`/`cargo test`/`cargo clippy` 全绿。

**完成记录（2026-06-06）**：
- 删除前按要求确认引用：`VirtualBuffer|crate::cache|cache::|Observable|observable::` 在 `src crates examples tests` 中仅命中待删除模块自身及其自带测试，`crates/examples/tests` 无外部引用；删除后复查无命中。
- 删除 `src/cache/` 整目录，并移除 `src/lib.rs` 中的 `pub mod cache;`。
- 删除 `src/reactive/observable.rs`，并移除 `src/reactive/mod.rs` 中的 `mod observable;` 与 `pub use observable::Observable;`。
- 同步删除两处模块自带单元测试，无悬空模块声明或导出。
- 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo build`；`cargo test --all --all-targets` 全部通过。

### [DONE] R5 — 审阅 T5
审阅 T5 改动：
- 复核删除前的 grep 确实零引用（含 examples/tests/其他 crate）。
- 确认无悬空 `mod`/`pub use`、无 dead_code 警告残留。
- 确认未误删仍被使用的 `reactive` 其他成员（`Property`/`DirtyFlag`）。
- 运行全量 `cargo test`。

**完成记录（2026-06-06）**：
- 复核 T5 最新提交删除范围：`src/cache/**` 与 `src/reactive/observable.rs` 已删除，`src/lib.rs` 无 `pub mod cache`，`src/reactive/mod.rs` 无 `mod observable`/`pub use observable::Observable`。
- 按要求搜索 `src`、`crates`、`examples`、`tests`，确认 `VirtualBuffer|crate::cache|cache::|Observable|observable::` 无代码引用残留。
- 复核保留 reactive 成员：`Property`/`Binding`/`DirtyFlag`/`DirtyObserver` 仍由 `src/reactive/mod.rs` 导出，且在主库与 workspace crate 中继续被使用，未被误删。
- 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets` 全部通过。

### [DONE] T6 — 文档对齐（M7 / L8 / cache/Observable）
**文件**：`CLAUDE.md`、`ASYNC.md`（如存在）
**步骤**：
1. CLAUDE.md「技术亮点 / 高性能渲染」：删除/修正「增量差异计算减少重绘」「脏标记系统精确追踪」等表述，改为「依赖 ratatui 双缓冲 diff 渲染」。移除对 cache 模块的描述（已删除）。
2. CLAUDE.md「支持模块」中 `cache/`、`reactive/observable.rs` 条目删除。
3. CLAUDE.md「代码约定」新增分层约定：**叶子级高频重绘组件可手写 `impl Component`，容器组合优先用声明式 API（VStack/HStack/Grid）**——解释 editor/file-tree 手写 Component 的合理性。
4. `ASYNC.md`（若存在）顶部标注「计划中（未落地），src 中暂无 async/tokio 实现」。
**验收**：文档与代码事实一致，无 cache/Observable 残留描述、无增量渲染误导。

**完成记录（2026-06-06）**：
- `CLAUDE.md` 的架构图和支持模块清单已移除已删除的 `cache/` 模块与 `reactive/observable.rs` / `Observable` 描述。
- `CLAUDE.md` 的高性能渲染说明已从“增量差异计算/脏标记精确追踪/渲染调度器”改为依赖 Ratatui 双缓冲 diff 与可见区渲染，避免继续承诺未接线机制。
- `CLAUDE.md` 新增分层约定：容器组合优先使用 `VStack`/`HStack`/`Grid` 声明式 API，叶子级高频重绘组件可手写 `impl Component`，以匹配 editor/file-tree 等实际实现。
- `ASYNC.md` 顶部已标注“计划中（未落地）”，并明确当前 `src` 中暂无 async/tokio 实现。
- 验证：复查 `CLAUDE.md`/`ASYNC.md` 中无 `cache`/`Observable` 残留描述；本任务仅修改 Markdown 文档，未重跑 `cargo fmt`/`cargo clippy`/`cargo test`，复用 R5 完成记录中的全量绿色结果。

### [DONE] R6 — 审阅 T6
审阅 T6 改动：
- 逐条核对 CLAUDE.md 不再有与实现脱节的承诺（增量渲染、脏标记、cache、Observable）。
- 确认分层约定表述清晰、与实际 18 处 `impl Component` 现状一致。
- 确认 ASYNC.md 标注准确。

**完成记录（2026-06-06）**：
- 审阅 `CLAUDE.md`：高性能渲染说明已限定为 Ratatui 双缓冲 diff 与可见区渲染，未继续承诺未接线的增量渲染/脏标记精确追踪；支持模块清单中无 `cache/`、`reactive/observable.rs`、`Observable` 残留描述。
- 审阅分层约定：`CLAUDE.md` 明确容器组合优先使用 `VStack`/`HStack`/`Grid`，叶子级高频组件可手写 `impl Component`，与当前 widgets、editor/file-tree 等手写实现现状一致，且未写死易漂移的数量。
- 审阅并修正 `ASYNC.md`：发现当前代码已提供标准库通道式入口 `EventQueue::channel()` 与 `run_crossterm_desktop_with_actions()`，并有 `tests/pty_async_actions.rs` 覆盖；文档已从“计划中（未落地）”改为“部分已落地”，仅保留 tokio/native async-await 为后续方向。
- 验证：`rg -n "计划中（未落地）|run_crossterm_desktop\(\) does not integrate|No code changes yet|manual event loop|no first-class API|暂无 async/tokio|增量差异|脏标记系统|cache/|cache 模块|Observable|observable::|reactive/observable|VirtualBuffer" CLAUDE.md ASYNC.md` 无命中；`rg -n "run_crossterm_desktop_with_actions|EventQueue::channel|pty_async_actions" ASYNC.md src/app/run.rs src/reactive/queue.rs tests/pty_async_actions.rs examples/async_progress.rs` 确认文档与代码入口一致。
- 本次仅修改 Markdown 文档与任务记录，未改 Rust 编译产物；未重跑 `cargo fmt`/`cargo clippy`/`cargo test`，复用 T6/R5 记录中的既有绿色结果。

---

## 阶段三：P2（架构/性能重构）

### [DONE] T7 — 拆分 `Component` god trait（M1）
**文件**：`src/composable/component.rs:167-508`、`Box<dyn Component>` 的 impl、全工作区调用点。
**步骤**：
1. 先按职责给 37 个方法分组（不改签名）：`Layout`（布局/尺寸协商）、`Scrollable`（8 个滚动方法）、`FocusNav`（焦点）、`DynamicTree`（children_mut/tag/动态树操作）、`EventHandling`（capture/bubble/handle）、属性反射/命令/标题栏归核心。
2. 抽为子 trait，`Component: Layout + Scrollable + FocusNav + DynamicTree + EventHandling`。
3. 调整 `Box<dyn Component>` 的透传 impl 按子 trait 拆分。
4. 编译驱动修复所有调用点。
**说明**：改动面大，必须在 T1–T6 合入稳定后单独大 PR。
**测试**：全量 PTY 回归，确保零行为变更。

**完成记录（2026-06-06）**：
- `src/composable/component.rs` 已按职责拆出 `Layout`、`Scrollable`、`FocusNav`、`DynamicTree`、`EventHandling`，并将 `Component` 收敛为这些子 trait 的 supertrait 组合加核心属性/命令/标题栏/绘制入口。
- `Box<dyn Component>` 的布局、滚动、焦点、动态树、事件与核心方法透传已拆分到对应 trait impl。
- 全工作区组件实现已迁移到对应子 trait，包含主库、workspace crates、examples、demos、snapshot binaries 与测试假组件；需要调用子 trait 方法的位置已补充相应 trait 作用域或使用完全限定调用。
- 为降低外部/示例组件默认实现样板，新增 `impl_component_default_traits!` 宏，用于只绘制或只覆盖少数组职责的组件显式实现默认子 trait。
- 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets` 全部通过。

### [DONE] R7 — 审阅 T7
审阅 trait 拆分：分组是否合理、supertrait 组合无遗漏方法、`Box<dyn>` 透传完整、全量 PTY 测试通过、无行为变更。

**完成记录（2026-06-06）**：
- 审阅 `src/composable/component.rs`：原 `Component` 方法已按职责拆入 `Layout`、`Scrollable`、`FocusNav`、`DynamicTree`、`EventHandling`，核心 `Component` 保留类型名、属性、命令、标题栏与 `draw` 入口，supertrait 组合无遗漏。
- 审阅 `Box<dyn Component>`：布局、滚动、焦点、动态树、事件与核心方法均有对应透传实现，未发现丢失的默认行为。
- 抽查主要包装/容器委派：`ComponentTag`、`Border`、`WindowMinSizeView`、`ComponentTree`、`StackCore`/`VStack`/`HStack`、`Grid` 均保留 T7 前的布局、滚动、焦点、动态树与事件分发语义。
- 复核 `impl_component_default_traits!` 用途：仅用于显式补齐等价默认子 trait，未发现应自定义行为却落到默认实现的组件。
- 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets` 全部通过，包含 PTY 集成测试。

### [DONE] T8 — 澄清事件分发 capture/bubble/handle 语义（M2）
**文件**：`src/composable/stack/events.rs`（`handle_event_capture_impl:228` 等）、wm/desktop 事件分发处。
**步骤**：
1. 先定位框架实际调用顺序（在 desktop/wm 分发入口 grep `handle_event_capture`/`handle_event`/bubble）。
2. 文档化 `capture → target → bubble` 调用契约（写在 trait 方法注释），或收敛为单一 `handle_event` 内部编排。
3. 确保包装层不重复分发。
**测试**：新增事件时序 PTY 测试——点击嵌套容器，断言 capture/bubble 各调用一次且顺序正确。

**完成记录（2026-06-06）**：
- 定位事件入口：`Desktop::handle_event` 通过 `WindowManager::dispatch_to_focused_view` / `dispatch_to_window_view` 进入根组件 `handle_event`，窗口管理器不会额外包一层 capture/bubble。
- `EventHandling` trait 注释已明确三阶段契约：容器自己的 `handle_event` 先运行本地 capture，未消费时分发到鼠标目标或焦点子组件的 `handle_event`，目标未消费时再运行本地 bubble。
- 文档说明嵌套容器的有效顺序为 `outer capture -> inner capture -> target handle -> inner bubble -> outer bubble`，并明确透明包装层应直接委派 `handle_event`，避免重复调用内部 capture/bubble 导致重复分发。
- `snapshot_app` 新增 `--event-order` PTY 夹具，渲染嵌套事件时序视图并在点击目标后显示实际调用序列。
- 新增 `tests/pty_event_order.rs`，点击嵌套目标并断言 trace 为 `root-capture>child-capture>target-handle>child-bubble>root-bubble`，覆盖 capture/bubble 各调用一次且顺序正确。
- 验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --test pty_event_order`；`cargo test --all --all-targets` 全部通过。

### [TODO] R8 — 审阅 T8
审阅事件模型：契约文档清晰、无重复/漏分发、时序测试覆盖嵌套场景。

### [TODO] T9 — 抽取共享滚动逻辑（M3）
**文件**：`stack/events.rs:141`、`grid/events.rs:141`、`scroll_container/events.rs:9`；复用 `scroll.rs:141` 的 `scroll_by_delta`。
**步骤**：在 `ScrollState`/`scroll.rs` 提供统一方法处理方向键/PageUp/PageDown/Home/End/滚轮 → delta → `scroll_by_delta`。三处改为调用共享方法，删除重复实现。
**测试**：`pty_scrolling.rs`/`pty_horizontal_scrolling.rs` 回归。

### [TODO] R9 — 审阅 T9
审阅去重：三处行为与原实现完全一致、无遗漏按键、滚动测试全绿。

### [TODO] T10 — 控件层共享抽象（M10）
**文件**：`src/widgets/textbox.rs`、`table.rs`、`list.rs`、`button.rs`、新增 widgets 公共 util。
**步骤**：
1. 抽 `pub(crate) fn widget_style(theme, enabled, focused) -> Style`（三态样式），替换 5 个 widget 重复实现。
2. 抽共享 `mouse_coords_local_to_area`/`contains` 到公共 util（textbox/table/list 三处去重）。
3. ListBox/TableView 的 selection/scroll 抽 mixin。
**测试**：各 widget 现有 PTY 测试回归。

### [TODO] R10 — 审阅 T10
审阅抽象：共享函数语义覆盖各 widget 原有差异、无回归、命名清晰。

### [TODO] T11 — 仅可见行 parse + 借用替代 clone（M11）
**文件**：`src/widgets/list.rs:374-376,543-557`、`table.rs:617`。
**步骤**：
1. `bindings()` 用 read guard 局部借用替代整体 clone。
2. `draw` 仅对可见行区间调 `parse_inline`（结合滚动 offset + 视口高度计算可见范围）。
**测试**：`tests/pty_virtual_scrolling.rs` 大数据集（1000+ 行）渲染正确性回归。

### [TODO] R11 — 审阅 T11
审阅性能改动：可见区间计算正确（边界行不漏）、借用无生命周期问题、大数据集渲染正确。

---

## 阶段四：P3（技术债 / 一致性 / 命名）

> 以下为随手清理项，可批量提交。每项较小，统一在 R12 中集中审阅。

### [TODO] T12 — P3 批量清理
- **M6**：`src/runtime/mod.rs:227-251,473-516`。区分 `PropertyApply::NotFound`（属性不存在）与组件不支持动态 set。当前 `apply_property_to_view`(492-493) 把 `UnsupportedProperty` 和 `NotFound` 都映射为 `NeedsRebuild`。新增 `PropertyApply::UnsupportedProperty` 变体，仅真正 `NotFound` 在 root 触发 rebuild，其余尽量局部替换。
- **L1**：`src/composable/geom.rs:70-84`。移除 `column<width && row<height` 坐标系猜测，改为调用方显式传坐标系枚举参数。
- **L2**：`src/widgets/button.rs:88-91`。保存 `last_area`，`Down(Left)` 前做 `contains` 检查（参考其他 widget）。
- **L3**：`src/composable/layout.rs:95` `Size::Weight(u16)`。低成本方案：修正 CLAUDE.md 称权重为 `u16`（非 `f32`）。如需 `f32` 化另开任务（牵动布局计算）。
- **L4**：`table.rs:266,612`、`list.rs:542`。缓存 `named_style("markdown-link")` 为字段，主题切换时失效重取。
- **L5**：`src/runtime/mod.rs:519` `is_tab_view` 用 `type_name().rsplit("::")` 字符串匹配。改为 `Component` trait 方法 `fn is_tab_container(&self) -> bool { false }`，TabView 重写返回 true。
- **L6**：`crates/atto-ui-macros/src/view_builder.rs:85` 硬编码 `::atto_ui::`。改用可配置 crate 路径方案；未知类型加 `compile_error!`。
- **L7**：`src/theme/config.rs:118` `parse_color`。支持 3 位 hex `#fff` → 扩展为 `#ffffff`。
- **L9**：`src/app/menu.rs:868` 与 `src/wm/manager/draw.rs:114` `draw_shadow` 完全重复。提取到共享位置，二者调用同一函数。
- **L10**：`cargo clippy --workspace --all-targets --fix` 修复 3 条告警，人工复核。（3 条既有告警已在 T1 验收中提前修复；T12 执行时复核 clippy 仍全清。）

### [TODO] R12 — 审阅 T12
逐项审阅 P3 清理：每项改动正确且不引入回归；L5 trait 方法替换无遗漏 TabView 识别点；M6 增量路径不再误触发全量 rebuild；clippy 全清；`cargo test` 全绿。

### [TODO] T13 — 命名消歧义（命名建议，需单独评估）
**说明**：影响 Cargo workspace，改动面大，**执行前需与维护者确认**。
- `atto-editor` → `atto-editor-app`：改 `Cargo.toml` 包名 + 所有 `[dependencies]` 引用 + import 路径。
- 评估 `atto-ui-runtime` 定位（仅被 `atto-ui-python` 引用）：作为核心共享 or 合并进 `atto-ui`。
**测试**：全工作区 `cargo build`/`cargo test`。

### [TODO] R13 — 审阅 T13
确认改名后全工作区编译通过、无遗漏引用、CI/文档同步更新。

### [TODO] T14 — 巨型文件拆分（M8，长期）
**文件**：`crates/atto-ui-editor/src/view/mod.rs`(1971)、`crates/atto-editor/src/window.rs`(1839)、`src/runtime/mod.rs`(1851)、`src/wm/manager/mod.rs`(972)、`src/app/menu.rs`(923)。
**说明**：纯机械重构，按职责拆子模块，**逐文件单独 PR**，零行为变更。
**测试**：每个文件拆分后全量 PTY 回归。

### [TODO] R14 — 审阅 T14
确认拆分纯机械、无行为变更、模块边界合理、全量测试通过。

### [TODO] T15 — id 索引替代 O(n) 查找（M9，低优先）
**文件**：`src/wm/manager/events.rs|focus.rs|z_order.rs`、runtime tree-ops。
**说明**：引入 `id→index/path` 索引替代线性扫描。窗口/节点少时无碍，最低优先级。
**测试**：现有 wm/runtime 测试回归。

### [TODO] R15 — 审阅 T15
确认索引与实际数据一致（增删窗口/节点时同步更新）、无悬挂索引、测试全绿。

---

## 执行顺序建议
1. T1→R1→T2→R2→T3→R3（P0，逐项独立 PR，先红后绿）
2. T4→R4→T5→R5→T6→R6（P1）
3. T7..T11（P2 重构，功能稳定后分批）
4. T12→R12（P3 批量），T13/T14/T15 视资源安排
