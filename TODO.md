# TODO：Turbo Vision UI 对齐

执行计划见 [`PLAN.md`](PLAN.md)，差异背景见 [`UI_GAPS.md`](UI_GAPS.md)。
编号对应 UI_GAPS 的 GAP 序号。

## 阶段 1 — 窗口装饰与按钮（高优先级）

- [x] **[DONE] #1 窗口标题居中** — `src/wm/manager/chrome.rs`：标题在顶边居中，前后各留 1 空格。补快照测试。
  - 完成记录（2026-06-09）：`draw_titlebar_text` 改为在标题可绘制区域内居中绘制，并在标题前后各写入 1 个空格；保留 grapheme/Unicode 宽度裁剪。新增 `pty_window_title_is_centered_with_padding` PTY 回归测试。验证：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test -p atto-ui --test pty_desktop --test pty_apphost_api`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] #2 关闭/缩放钮归位** — 关闭钮 `[■]` 移左上角，缩放钮 `[↑]`/`[↕]` 右上角，统一 `[ ]` 包裹。glyph 改 `src/theme/mod.rs`。
  - 完成记录（2026-06-09）：`src/wm/manager/chrome.rs` 将关闭钮绘制到左侧并以 `[■]` 包裹，将缩放/还原钮绘制到右侧并以 `[↑]`/`[↕]` 包裹；标题区域同步避让左右按钮。`src/theme/mod.rs` 默认 glyph 改为 `close-button = "■"`、`maximize-button = "↑"`，并新增 `restore-button = "↕"`。为保持新按钮可点击，命中测试改为复用标题栏按钮布局；同步更新受影响 PTY 坐标/夹具与回归测试。验证：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] #2b 同步命中测试** — 更新 `src/wm/manager/` 鼠标处理：点击关闭/缩放区域坐标随按钮位置调整，回归拖动/调整大小。
  - 完成记录（2026-06-09）：确认 `src/wm/manager/chrome.rs` 的标题栏按钮命中测试复用与绘制一致的 `titlebar_layout`，关闭钮命中区域随左上角 `[■]` 迁移，缩放/还原钮命中区域随右上角 `[↑]`/`[↕]` 迁移。新增 `relocated_titlebar_buttons_handle_mouse_at_drawn_positions` 与 `titlebar_drag_still_starts_outside_relocated_buttons` 回归测试，覆盖左侧关闭命中不触发拖动、右侧缩放/还原命中以及避开按钮后的标题栏拖动；既有 `mouse_drag_resize_handles_work_on_all_corners` 继续覆盖调整大小回归。验证：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] #3 按钮重绘** — `src/widgets/button.rs`：单行色块 + 阴影 + 默认按钮强调，去边框。
  - 完成记录（2026-06-09）：`Button` 改为直接绘制无边框单行色块，支持右侧与下方阴影，并以 focused/default 状态使用强调样式；新增 `default_button` 构建器与动态运行时 `default` 属性。`Theme` 注册 `button`/`button-focused`/`button-default`/`button-disabled`/`button-shadow` 命名样式，便于主题覆盖。新增按钮渲染单元测试与 `pty_core_widgets_t19` PTY 回归断言，验证按钮不再出现边框且 focused 按钮渲染为彩色单行块。按钮布局高度暂保持 3，尺寸收敛留给后续 `#3b`。验证：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test -p atto-ui --lib widgets::button`、`cargo test -p atto-ui --test pty_core_widgets_t19`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] #3b 按钮尺寸回归** — 按钮高度 3→1，检查依赖按钮尺寸的布局与现有测试。
  - 完成记录（2026-06-09）：`Button` 的 `min_height`/`desired_height` 从 3 行收敛为 1 行，并新增单行布局单元测试；按钮绘制与鼠标命中测试改为使用 1 行区域。`snapshot_app` 的 T19 核心控件夹具同步改为 1 行按钮区域并压缩后续控件坐标，`pty_core_widgets_t19` 点击坐标随之更新。示例与 demo 中按钮工具栏不再硬编码 3 行高度，改用内容高度。验证：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test -p atto-ui widgets::button --lib`、`cargo test -p atto-ui --test pty_core_widgets_t19`、`cargo test --all --all-targets` 均通过。
- [x] **[DONE] #4 桌面背景纹理** — `src/app/desktop.rs:580`：`Fill` 改用 `░` 纹理。
  - 完成记录（2026-06-09）：`Desktop::draw` 的全屏背景填充从空格改为 `░`，并新增 `pty_desktop_background_uses_texture` PTY 回归测试覆盖未被窗口/菜单/状态栏覆盖的桌面工作区单元格。纹理背景暴露出部分 PTY 测试辅助函数将 UTF-8 byte offset 当作终端列的问题；同步修正受影响的点击/取色坐标 helper 改用显示宽度计算，避免多字节桌面纹理、边框或宽字符导致坐标偏移。验证：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`、相关 PTY 聚焦测试、`cargo test --all --all-targets` 均通过。

## 阶段 2 — 菜单条（中优先级）

- [x] **[DONE] #5 菜单条整体化** — `src/app/menu/draw.rs` `MenuBar::draw`：先用 `menu_bar` 填满整行，再绘各项。
  - 完成记录（2026-06-09）：`MenuBar::draw` 现在会先用 `theme.menu_bar` 填满菜单栏整行，再绘制各顶层菜单项，避免未被菜单标题覆盖的右侧区域透出桌面背景或旧缓冲区内容。新增 `draw_fills_entire_menu_bar_row_before_titles` 单元回归测试，验证菜单栏尾部空白单元格使用菜单栏前景/背景样式。验证：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test -p atto-ui draw_fills_entire_menu_bar_row_before_titles`、`cargo test --all --all-targets` 均通过。
- [ ] **#6 点击只高亮、无下沉阴影** — 顶层菜单项激活仅切 `menu_bar_active`，不调用 `draw_shadow`。
- [ ] **#7 热键字母配色** — `mnemonic_style` / 主题键 `menu-mnemonic` 用 accent（经典红）。

## 阶段 3 — 状态栏与滚动条（中优先级）

- [ ] **#9 状态栏 item 可点击** — `src/app/desktop.rs:602` 默认状态栏改用 `StatusSegment` 渲染并接命令回调。
- [ ] **#8 滚动条箭头/轨道** — composable/scroll：`▲`/`▼` 端帽与 `░` 轨道在矮内容区也渲染。

## 阶段 4 — TV 配色主题（中优先级）

- [ ] **#10 新增 `Theme::turbo()`** — `src/theme/mod.rs`：蓝桌面/灰青菜单状态栏/灰底对话框/绿色选中高亮；支持主题文件加载；默认主题不变。

## 阶段 5 — 细节字形（低优先级）

- [ ] **#11 复选框字形** — `[x]` → `[X]`（`src/theme/mod.rs`）。
- [ ] **#12 单选按钮字形** — `(*)` → `(•)`（`src/widgets/radio.rs`）。
- [ ] **#13 系统菜单图标**（可选）— 菜单栏左侧加 `≡`。
- [ ] **#14 顶层菜单项间距** — 间距微调，视配色而定。

## 收尾

- [ ] 全量 `cargo test` + `cargo clippy` 通过。
- [ ] 用 `snapshot_app` 抓屏与参考截图人工比对。
