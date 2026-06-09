# TODO：Turbo Vision UI 对齐

执行计划见 [`PLAN.md`](PLAN.md)，差异背景见 [`UI_GAPS.md`](UI_GAPS.md)。
编号对应 UI_GAPS 的 GAP 序号。

## 阶段 1 — 窗口装饰与按钮（高优先级）

- [x] **[DONE] #1 窗口标题居中** — `src/wm/manager/chrome.rs`：标题在顶边居中，前后各留 1 空格。补快照测试。
  - 完成记录（2026-06-09）：`draw_titlebar_text` 改为在标题可绘制区域内居中绘制，并在标题前后各写入 1 个空格；保留 grapheme/Unicode 宽度裁剪。新增 `pty_window_title_is_centered_with_padding` PTY 回归测试。验证：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test -p atto-ui --test pty_desktop --test pty_apphost_api`、`cargo test --all --all-targets` 均通过。
- [ ] **#2 关闭/缩放钮归位** — 关闭钮 `[■]` 移左上角，缩放钮 `[↑]`/`[↕]` 右上角，统一 `[ ]` 包裹。glyph 改 `src/theme/mod.rs`。
- [ ] **#2b 同步命中测试** — 更新 `src/wm/manager/` 鼠标处理：点击关闭/缩放区域坐标随按钮位置调整，回归拖动/调整大小。
- [ ] **#3 按钮重绘** — `src/widgets/button.rs`：单行色块 + 阴影 + 默认按钮强调，去边框。
- [ ] **#3b 按钮尺寸回归** — 按钮高度 3→1，检查依赖按钮尺寸的布局与现有测试。
- [ ] **#4 桌面背景纹理** — `src/app/desktop.rs:580`：`Fill` 改用 `░` 纹理。

## 阶段 2 — 菜单条（中优先级）

- [ ] **#5 菜单条整体化** — `src/app/menu/draw.rs` `MenuBar::draw`：先用 `menu_bar` 填满整行，再绘各项。
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
