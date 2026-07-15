# TODO：脚本化 / Introspection 控制平面

执行计划见 [`PLAN.md`](PLAN.md)，分层设计见 [`SCRIPTING_LAYERS.md`](SCRIPTING_LAYERS.md)。

上一阶段「全功能多窗口终端 App」计划（M1-M7）已归档至 [`docs/archive/2026-07-12-terminal-app/`](docs/archive/2026-07-12-terminal-app/)。

## 使用约定

- 任务按实现顺序编号：`M<阶段>-<序号>`，例如 `M1-1` 是阶段 1 第 1 个任务。
- 每个任务标题保留 `[TODO]` 标记；完成后由 coding agent 改为 `[DONE]` 并在任务下补「完成记录」与「验证」两行（沿用归档 TODO 的格式）。
- 每阶段结尾有独立的 `M<n>-R Review` 任务，负责复核本阶段正确性与完整性。

## 通用验收

每个任务完成后至少运行：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

第 1/2 层逻辑测试优先走**进程内**读值断言（构造 `Desktop` → `desktop.inspect()`），不经 PTY；第 4 层终端交互走 PTY 快照。全套 `cargo test --workspace` 在本机约需较长时间，聚焦验证可先跑相关 crate/测试文件，最终务必全套通过。

## 代码位置速查

| 主题 | 位置 |
|---|---|
| 第 1 层门面 `DesktopInspector` | `src/inspect.rs`（`component_find`/`component_find_mut` 在 `1147-1170`） |
| `Component` trait（寻址/属性/动作方法） | `src/composable/component.rs`：`tag()` `354`、`property_names()` `476`、`get_property` `480`、`set_property` `484`、`apply_command` `488`、`children()` `358`、`children_mut()` `362` |
| `ComponentTag` / `.tag()` 扩展 | `src/composable/component_tag.rs`（`ComponentTag` `18`、`ComponentTagExt` `43`） |
| 动作 / 目标 / 值类型 | `src/component_api.rs`：`ComponentCommand` `42`、`ComponentTarget` `52`、`ComponentError` `58`、`ComponentValueCodec` `9` |
| `#[derive(ComponentProperties)]` 宏 | `crates/atto-ui-macros/src/component_properties.rs` |
| 叶子控件 | `src/widgets/{checkbox,button,textbox,slider}.rs`（均已 `#[derive(ComponentProperties)]`，`impl Component` 分别在 `74/97/135/186` 行附近） |
| 已实现 `apply_command` 的参考 | `src/widgets/{disclosure,list,table,radio,tab_view,typeahead}.rs`、`src/composable/{visibility,border,scroll_container/mod}.rs` |
| reactive 变更信号 | `src/reactive/dirty.rs`（`DirtyFlag` `6`、`DirtyObserver` `16`、`check_and_clear` `43`、`observer` `50`） |
| runtime 私有寻址 | `src/runtime/tree.rs`（`ViewPathIndex` `592`） |
| PTY 测试宿主 | `crates/atto-ui-test-host/src/`（`PtyTestHost` API 见 `lib`；`wait_for_text`/`wait_for_screen` 已有） |
| chat OCR 痛点 helper（迁移对象） | `crates/atto-ui-chat/tests/pty_chat.rs`（`find_text_position` `26`、`wait_for_disclosure_text` `54`） |
| 终端 spawn / handle | `crates/atto-ui-terminal/src/terminal.rs`：`spawn_command` `2775`、`send_input_bytes` `3443`、`resize` `3448`、`is_running` `3728`、`exit_status` `3733`、`snapshot` `3738` |
| 终端系统剪贴板后端 | `crates/atto-ui-terminal/src/terminal.rs`（`TerminalSystemClipboard`，M4.6 引入，供 L1 复用） |
| 终端分屏引擎 | `crates/atto-ui-terminal/src/pane.rs`（`TerminalPaneGroup` `203`、`TerminalPaneGroupHandle` `76`） |

---

## 阶段 M1 - 第 1 层 introspection（地基）

目标：把分散的寻址收敛成公共 `find_by_tag`，把 `DesktopInspector` 明确为第 1 层门面，兑现「逻辑测试改用读值断言」的独立价值。第 1 层不得依赖第 2/3/4 层。

- [x] **[DONE] M1-1 公共 `find_by_tag` 寻址 API**
  - 上下文：目前寻址实现分散——`src/inspect.rs:1147-1170` 有私有递归 `component_find`/`component_find_mut`，`src/runtime/tree.rs:592` 有私有 `ViewPathIndex`。二者都按 `Component::tag()`（`component.rs:354`，返回 `Option<&str>`）匹配。需要一个公共、纯只读、进程内的寻址入口。
  - 实现：在 composable 层（建议 `src/composable/` 下新增或挂到现有 trait/自由函数）提供 `pub fn find_by_tag<'a>(view: &'a dyn Component, tag: &str) -> Option<&'a dyn Component>` 与 `find_by_tag_mut`，语义同现有 `component_find`（先比自身 `tag()`，再 DFS `children()`/`children_mut()`）。`children_mut()` 返回 `Option<&mut Vec<ComponentNode>>`，遍历 `ComponentNode.view`。
  - 收敛：`src/inspect.rs` 的 `component_find`/`component_find_mut` 改为委托新公共函数，删除重复递归；`component_get_property`/`component_set_property`/`component_action`/`component_exists` 行为不变。
  - 从 `src/lib.rs` 导出该 API（`inspect` 与 composable 的 `pub use` 区域，`lib.rs:8-32`）。
  - 验证：新增单测覆盖「命中根节点」「命中深层嵌套子节点」「未命中返回 None」「同名 tag 返回首个」；`cargo test -p atto-ui find_by_tag -- --nocapture`；确认 `inspect.rs` 既有测试（`inspect_tree_finds_tags` 等）仍通过；全套 fmt/clippy/test 通过。
  - 完成记录：新增 `src/composable/find.rs`，提供公共 `find_by_tag` / `find_by_tag_mut`，按 root-first DFS 先匹配当前 `Component::tag()`，再遍历 `children()` / `children_mut()` 中的 `ComponentNode.view`；从 `src/composable/mod.rs` 与 `src/lib.rs` 导出；`src/inspect.rs` 的私有 `component_find` / `component_find_mut` 已收敛为委托公共 API，属性读取、属性写入、动作派发和存在性检查行为保持不变。新增测试覆盖根节点命中、深层子节点命中、未命中、同名 tag 返回首个，以及 mutable 同名 tag 首个匹配。按测试失败策略，验证过程中同时修复了 selectable `Text` 拖拽 capture / 释放 / 选区渲染行为，并将多处 PTY 中不可稳定读取的样式 / 颜色断言迁移到进程内 buffer 或状态断言，保留对应 PTY 端到端文本与交互覆盖。
  - 验证：`cargo test -p atto-ui find_by_tag -- --nocapture`；`cargo test -p atto-ui inspect_tree_finds_tags -- --nocapture`；`cargo fmt --all -- --check`；`CARGO_TARGET_DIR=target/codex cargo clippy --workspace --all-targets -- -D warnings`；`CARGO_TARGET_DIR=target/codex cargo test --workspace --all-targets`。后两条使用独立 `target/codex`，避免与本机 VS Code/rust-analyzer 占用的默认 `target` 锁冲突。

- [x] **[DONE] M1-2 `DesktopInspector` 收敛为第 1 层门面**
  - 上下文：`DesktopInspector`（`src/inspect.rs:108`）已提供 `tree`/`export_snapshot`/`get_property`/`set_property`/`action`，是第 1 层门面雏形。本任务只做「收敛 + 补只读能力」，不加动作能力（动作属第 2 层 M2）。
  - 实现：补 `property_names(id) -> Result<Vec<String>, ComponentError>`（复用 `Component::property_names`，跨 menu/window/component 三类查找，风格对齐现有 `get_property` 的三段式 `menu_/window_/component_`）；`get_property`/`set_property`/`export_snapshot` 的组件寻址改用 M1-1 的公共 `find_by_tag`。保持 `#![forbid(unsafe_code)]`。
  - 明确边界：不改动 `action`/`action_by_id`（那是第 2 层要增强的入口），本任务范围仅只读门面。
  - 验证：新增单测覆盖 `property_names("some-tag")` 返回该组件属性名集合、未知 tag 返回 `ComponentError::NotFound`；既有 `export_snapshot_*` / `inspect_can_*` 测试不回归；全套通过。
  - 完成记录：新增 `DesktopInspector::property_names(id)` 只读门面，按 menu、window、component 三段式查找；menu/window 路径返回与现有 `get_property` 可读属性一致的属性名集合，component 路径复用 `Component::property_names()` 并通过 M1-1 的公共 `find_by_tag` 委托链寻址。`action` / `action_by_id` 未改动，仍保留第 2 层边界；`export_snapshot` 继续做全树导出，不引入按组件 id 的新寻址逻辑。新增单测覆盖 menu spec、menu item、window、component 四类属性名读取，以及未知 id 返回 `ComponentError::NotFound`。
  - 验证：`cargo test -p atto-ui inspect_property_names -- --nocapture`；`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M1-3 tag 覆盖诊断辅助**
  - 上下文：`tag`/`id` 是 `Option`（`component.rs:354` 返回 `Option<&str>`），未标 tag 的节点不可寻址。约定「可脚本 / 可测组件必须显式标 tag」，需要一个诊断工具来发现漏标。
  - 实现：在 `DesktopInspector` 上加 `untagged_interactive_nodes(screen) -> Vec<InspectNode>`（或返回轻量描述），遍历 `build_desktop_tree`（`inspect.rs:343`）产物，筛出「可交互但 `id` 为 `None`」的节点。判定「可交互」：`property_names()` 含 `checked`/`text`/`selected`/`value`/`selection` 等交互属性之一，或 `is_focusable()` 为真（参考 `inspect.rs:724` 的 `is_snapshot_component_property` 白名单）。
  - 定位：这是诊断辅助（测试期使用），不是运行时强制；不改变任何交互行为。
  - 验证：单测构造含「标了 tag 的 Checkbox」+「未标 tag 的 Checkbox」的 Desktop，断言诊断只列出后者；全套通过。
  - 完成记录：新增 `DesktopInspector::untagged_interactive_nodes(screen) -> Vec<InspectNode>` 诊断入口，刷新桌面布局后遍历 `build_desktop_tree` 产物，返回 `id == None` 且可交互的节点副本；`InspectNode` 新增 `focusable` 诊断字段，组件节点由 `Component::is_focusable()` 填充，窗口节点由 `WindowKind::is_focusable()` 填充。交互判定覆盖 `is_focusable()`，以及 `checked` / `text` / `selected` / `selected_index` / `selection` / `value` / `index` / `progress` / `active` 等可读属性名；实现仅用于测试期诊断，不改变事件、寻址、属性读写或渲染行为。新增单测构造已标 tag 的 `Checkbox` 和未标 tag 的 `Checkbox`，并给窗口与容器打 tag，断言诊断结果只包含未标 tag 的 `Checkbox`。
  - 验证：`cargo fmt --all`；`cargo test -p atto-ui untagged_interactive_nodes -- --nocapture`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M1-4 变更信号聚合（为 M2 `wait_for` 预留）**
  - 上下文：reactive 是拉模型——`DirtyFlag`/`DirtyObserver`（`src/reactive/dirty.rs`），`check_and_clear()`（`:43`）返回自上次以来是否 dirty，`observer()`（`:50`）克隆观察者。第 2 层 `wait_for` 需要一个统一的「UI 是否发生过变更」进程内信号，避免轮询屏幕。
  - 实现：提供一个进程内变更检测封装（建议挂在 `DesktopInspector` 或独立小结构），聚合 desktop 关注的 `DirtyFlag`，暴露 `changed_since_last_poll() -> bool` 之类接口。**只做拉模型聚合**，不引入 push 订阅（`SCRIPTING_LAYERS.md` 第 1 层缺口 4 明确「不强求 push」）。
  - 明确边界：本任务只交付「信号读取」原语；真正的 `wait_for(predicate, timeout)` 循环在 M2-5 实现，此处不写等待循环。
  - 验证：单测：改一个 `Binding` 后聚合信号报告 changed；`mark_clean`/poll 后回落 false；全套通过。
  - 完成记录：新增 `DirtySignal` / `DirtySignalSet` 拉模型封装，基于 `DirtyObserver` 为每个 consumer 独立检测变更，不清除全局 dirty 状态；`Property` / `Binding` 新增 `dirty_signal()`。`Component` trait 新增 `dirty_signals()` 默认空实现，`#[derive(ComponentProperties)]` / `#[component_properties]` 自动为组件 `Binding` 属性生成 dirty 信号，`ComponentTag`、`Visibility`、`Border`、`WindowMinSizeView` 等透明 wrapper 显式合并 inner 信号，避免窗口根 wrapper 截断组件树。新增 `DesktopChangeTracker`，由 `DesktopInspector::change_tracker()` 创建，聚合 menu、status segment、window binding 与组件树 binding，并暴露 `changed_since_last_poll()`；`DesktopInspector::refresh_change_tracker()` 可在窗口 / 组件结构变化后刷新信号集合。实现只提供进程内拉模型信号读取，不实现等待循环，不引入 push 订阅，不改变交互和渲染行为。
  - 验证：`cargo test -p atto-ui desktop_change_tracker -- --nocapture`；`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M1-5 进程内读值断言范式 + 示范迁移一例 chat 逻辑测试**
  - 上下文：兑现第 1 层独立价值。`crates/atto-ui-chat/tests/pty_chat.rs` 用 `find_text_position`（`:26`，抓屏 + `UnicodeWidthStr` 反算列坐标）和 `wait_for_disclosure_text`（`:54`，`sleep` + 字形 `▶` 推断展开状态）来测逻辑，脆弱且是「OCR 状态」。
  - 实现：
    1. 落地进程内测试范式样板：构造 `Desktop`（含带 `tag` 的 chat 组件）→ `desktop.inspect()` → `get_property`/`property_names` 读 `Binding` 活值断言。放在合适的测试模块（chat crate 的单测或集成测试）。
    2. **示范迁移一例**：挑 `pty_chat.rs` 中一个「断言的是逻辑 / 状态而非渲染」的用例（如 disclosure 展开态、某值是否更新），改写为进程内读值断言版本；保留（不删除）原 PTY 用例中真正测渲染 / 端到端的部分。
    3. 若 chat 组件相关节点缺 tag，按 M1-3 约定补标 tag。
  - 明确边界：只迁移**一例**作示范，不要求全量迁移；不得为此改动 chat 组件的交互语义。
  - 验证：迁移后的逻辑测试不含 `find_text_position`/字形推断，改为读值断言；新旧测试均通过；`cargo test -p atto-ui-chat`；全套通过。
  - 完成记录：新增 `crates/atto-ui-chat/tests/inspect_chat.rs` 作为 chat 进程内读值断言样板：构造带 tagged `ChatInputPanel` 的 `ChatPanel` 与 `Desktop`，通过 `desktop.inspect()` 的 `tree` / `property_names` / `get_property` 断言 chat input 可按 tag 发现，并验证 `mode` 活值从 `text` 更新为 `choice`。`ChatInputPanel` 新增 `with_tag` 与 `DynamicTree::tag()`，只影响 introspection 寻址；`ChatPanel` 透明转发内部 `VStack` 的 `children()` / `children_mut()`，避免外层 chat 根组件截断第 1 层组件寻址，不改变绘制、输入、回调或交互语义。原 `pty_chat.rs` 中输入模式和端到端渲染覆盖保留不删。按测试失败策略，完整测试中发现 `pty_virtual_scrolling` 在 workspace 负载下同文件 PTY fixture 并发启动会出现空屏超时；已为 `tests/pty_virtual_scrolling.rs` 添加文件级互斥锁，修复该整类测试隔离问题。
  - 验证：`cargo test -p atto-ui-chat chat_input_mode_state_is_readable_through_desktop_inspector -- --nocapture`；`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-chat`；`cargo test -p atto-ui --test pty_virtual_scrolling -- --nocapture`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M1-R Review — 第 1 层完整性与正确性复核**
  - 复核点：
    1. 公共 `find_by_tag` 语义与旧 `component_find` 一致（含同名 tag、深层嵌套、mut 路径），`inspect.rs` 无残留重复递归。
    2. `DesktopInspector` 只读门面自洽，未混入第 2 层动作能力；第 1 层代码**不依赖** `apply_command` 的语义派发、不依赖第 2/3/4 层模块。
    3. tag 覆盖诊断与变更信号聚合均为进程内、纯读、不改变交互行为。
    4. 示范迁移的测试确实脱离了 OCR / 字形推断，且未误删渲染 / 端到端覆盖。
    5. 保持 `#![forbid(unsafe_code)]`。
  - 验证：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 全套通过；在完成记录中列出复核结论。
  - 完成记录：第 1 层复核通过。公共 `find_by_tag` / `find_by_tag_mut` 保持 root-first DFS 语义，覆盖根节点、深层子节点、未命中、同名 tag 首个匹配和 mutable 路径；`inspect.rs` 中 `component_find` / `component_find_mut` 只保留对公共 API 的委托。`DesktopInspector::property_names` 与既有 `get_property` / `set_property` 组件路径均复用公共寻址；M1 新增的 `property_names`、`untagged_interactive_nodes`、`change_tracker` / `refresh_change_tracker` 不依赖 `apply_command` 语义派发，也不引入第 2/3/4 层依赖。tag 覆盖诊断基于绘制后的 inspect tree 只读筛选未标 tag 的交互节点；dirty change tracker 基于 `DirtyObserver` 做 per-consumer 拉模型检测，不清除全局 dirty 状态、不改变交互行为。chat 示例迁移通过 `DesktopInspector` 的 `tree` / `property_names` / `get_property` 读取 `ChatInputPanel` 的 `mode` 活值，不依赖 `find_text_position`、字形 `▶` 推断或屏幕 OCR；原 `pty_chat.rs` 的渲染 / 端到端覆盖保留。`src/lib.rs` 仍保留 `#![forbid(unsafe_code)]`。
  - 验证：`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

---

## 阶段 M2 - 第 2 层 scriptable（语义动作 + 查询 + 等待）

目标：在第 1 层「读」之上加「触发」和「等待」。核心是补齐叶子组件 `apply_command`，并提供按可序列化设计的进程内 `invoke`/`query`/`wait_for`。依赖 M1。

- [x] **[DONE] M2-1 Checkbox `apply_command`**
  - 上下文：`src/widgets/checkbox.rs`，`#[derive(ComponentProperties)]`（`:17`），`impl Component`（`:74`），已有 `checked: Binding<bool>` 属性可被 `get_property("checked")` 读到。当前 `apply_command` 走 trait 默认实现（`component.rs:488`，返回 `ignored()`），外部触发只能退回合成点击。
  - 实现：实现 `Checkbox::apply_command`，`ComponentCommand::Toggle` 翻转 `checked`、`ComponentCommand::Click` 等价于用户点击（与键盘 Space/Enter 及鼠标点击相同的状态转移与回调触发路径，复用组件内既有的 toggle 逻辑，勿另写一套）。命中返回 `EventResult::consumed()`/合适结果，未命中的命令返回 `ignored()`。
  - 验证：进程内单测：`invoke`/`apply_command(Toggle)` 后 `checked` 的 `Binding` 翻转、`on_toggle` 类回调按既有语义触发；`cargo test -p atto-ui checkbox -- --nocapture`；全套通过。
  - 完成记录：`Checkbox::apply_command` 现支持 `ComponentCommand::Toggle` 与 `ComponentCommand::Click`。两者在组件启用时复用既有私有 `toggle()` 路径，因此与键盘 Space/Enter 和鼠标释放命中共享同一 `checked` binding 翻转与 `on_change_callback` payload 触发逻辑；命中后返回 `EventResult::changed()`，禁用态及其他命令返回 `EventResult::ignored()`。新增进程内单测覆盖 `Toggle` 连续翻转 binding、`Click` 触发 change callback 且 payload 为新 `checked` 值、禁用态 `Toggle` / `Click` 均 ignored 且不触发回调。
  - 验证：`cargo test -p atto-ui checkbox -- --nocapture`；`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M2-2 Button `apply_command`**
  - 上下文：`src/widgets/button.rs`，`impl Component`（`:97`）。按钮激活当前靠 Enter/Space/鼠标点击触发 `on_activate` 类回调。
  - 实现：`ComponentCommand::Click`/`Submit` 触发与用户激活相同的回调路径；不改变禁用态语义（禁用时应 `ignored()`）。
  - 验证：进程内单测：`apply_command(Click)` 触发激活回调、禁用按钮 `ignored()` 且不触发回调；全套通过。
  - 完成记录：`Button::apply_command` 现支持 `ComponentCommand::Click` 与 `ComponentCommand::Submit`。启用态下两者都复用既有私有 `trigger()` 路径，因此与键盘 Enter/Space 和鼠标释放命中共享同一 `on_click` 闭包与 `on_click_callback` 触发逻辑，并返回 `EventResult::submitted()`；禁用态及其他命令返回 `EventResult::ignored()`。新增进程内单测覆盖 `Click` 触发 `on_click`、`Submit` 触发 callback handle、禁用态 `Click` / `Submit` 均 ignored 且不触发任何回调。
  - 验证：`cargo test -p atto-ui button -- --nocapture`；`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M2-3 TextBox `apply_command`**
  - 上下文：`src/widgets/textbox.rs`，`impl Component`（`:135`），基于 `TextBuffer`（Unicode 感知），有 `text` 属性。现有 `inspect.rs` 的 `InputText` 兜底靠合成点击 + `Event::Paste`（`inspect.rs:246-271`）。
  - 实现：`ComponentCommand::InputText(s)` 直接把文本写入缓冲（语义级：设置 / 插入文本，遵循组件既有的粘贴 / 输入路径以保持光标、滚动、Unicode 行为一致），使 `get_property("text")` 随即反映新值。明确定义是「替换全部」还是「在光标处插入」——建议对齐现有 `Event::Paste` 语义（插入）并在完成记录中写明。
  - 验证：进程内单测：`apply_command(InputText("你好👋"))` 后 `text` 属性等于预期、宽字符 / emoji 不裂；全套通过。
  - 完成记录：`TextBox::apply_command` 现支持 `ComponentCommand::InputText`。启用态下该命令复用新增私有 `insert_text_at_cursor` 路径，与 `Event::Paste` 和 Ctrl+V 共享同一状态更新逻辑：若存在选区则先替换选区，然后在当前光标处插入文本，随后通过 `sync_binding_from_buffer()` 更新 `text` binding 并触发 `on_change_callback` payload；命中后返回 `EventResult::changed()`。禁用态及其他命令保持 `EventResult::ignored()`。新增进程内单测覆盖 Unicode / emoji 输入后 `get_property("text")` 可立即读回、在 emoji 后的光标插入不会拆裂 grapheme、禁用态不改 binding 且不触发回调。
  - 验证：`cargo test -p atto-ui textbox -- --nocapture`；`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M2-4 Slider `apply_command`**
  - 上下文：`src/widgets/slider.rs`，`impl Component`（`:186`），有 min/max/value 类属性（见 `inspect.rs:724` 白名单含 `min`/`max`/`progress`）。
  - 实现：为 Slider 选一个语义动作落地——建议复用 `ComponentCommand::SelectIndex(usize)` 或 `Custom` 表示「设为某刻度 / 值」，与键盘左右箭头调值走相同的 clamp / 步进逻辑。在完成记录中写明选用的命令形态（供 M2-5 的 `invoke` 与将来 M4 序列化对齐）。
  - 验证：进程内单测：设值后 `value`/`progress` 属性更新且被 min/max clamp；越界值不 panic；全套通过。
  - 完成记录：`Slider::apply_command` 现支持 `ComponentCommand::SelectIndex(usize)`。命令语义定义为「从规范化后的 `min` 起按 `abs(step)` 计算第 N 个刻度」，即 `min + index * abs(step)`，再复用既有 `snap_value` / `clamp_value` / `set_value_and_emit` 路径落到合法值并触发 `change` callback payload；禁用态及其他命令返回 `EventResult::ignored()`。`Slider` 同时新增只读虚拟属性 `progress`，按 clamp 后的 `(value - min) / (max - min)` 导出归一化进度，零宽范围返回 `0.0`。新增进程内单测覆盖 `SelectIndex` 正常设值、`value` / `progress` 读值更新、越界 index clamp 到 `max` 且不 panic、反向 min/max 规范化、禁用态 ignored 以及 callback payload。
  - 验证：`cargo test -p atto-ui slider -- --nocapture`；`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M2-5 进程内语义 API：`invoke` / `query` / `wait_for`**
  - 上下文：`inspect.rs` 已有 `action`/`action_target`（`:167`）与 `get_property`（`:136`），`ComponentTarget`（`component_api.rs:52`）支持 `Id`/`Focused`。本任务把它们收敛成第 2 层稳定语义 API，并新增 `wait_for`。
  - 实现：
    1. `invoke(target, action)`：语义级派发。目标组件实现了 `apply_command` 就走语义派发（M2-1..M2-4）；未实现才退回坐标注入兜底（`inspect.rs:222-278` 的现有逻辑保留）。返回值要能区分「语义派发成功」vs「退回坐标」以便可观测。
    2. `query(target, prop)`：= 第 1 层 `get_property`，统一命名对齐 API 形状。
    3. `wait_for(predicate, timeout)`：进程内循环——推进 UI（draw / tick）→ 用 M1-4 变更信号或直接重查 `predicate`（对 `query` 结果判定）→ 未成立则继续到超时。**替代 chat helper 的 `sleep` 轮询屏幕**。predicate 建议接受 `&mut DesktopInspector` 或以 `(target, prop, 期望值)` 表达以便将来序列化。
    4. **可序列化约束**：`invoke`/`query`/`wait_for` 的入参（target、action、prop、期望值）都用第 3 层能直接序列化的值（`ComponentCommand` 已 `Clone+PartialEq`，`ComponentValue` 已 serde）。在完成记录中确认无进程内独有的闭包 / 引用泄漏到 API 边界（`wait_for` 的 predicate 闭包属例外，M4 时再定序列化表达）。
  - 验证：进程内单测：`invoke("checkbox", Toggle)` 直接翻转 `Binding` 且可观测到「走了语义派发」而非坐标；`wait_for` 能等到由后台 / 定时驱动的状态成立、且超时返回错误不挂死；全套通过。
  - 完成记录：新增第 2 层进程内语义入口 `DesktopInspector::invoke` / `query` / `wait_for` / `wait_for_with_interval` / `wait_for_predicate`。`invoke` 返回 `InvokeResult { dispatch, result }`，可观测 `InvokeDispatch::Semantic`、`CoordinateFallback` 与 `Unsupported`；Id 目标按 menu/window/component 三段式语义优先派发，未实现 Click/Toggle/Submit/InputText 时才退回既有坐标/粘贴注入兜底，Focused 目标走进程内语义派发。为区分默认 ignored 与禁用态有意 ignored，`Component` trait 新增 `supports_command(&ComponentCommand)`，四个 M2 叶子组件以及既有 `apply_command` 组件、透明 wrapper、runtime wrapper 均声明或转发支持关系，避免 wrapped/tagged 组件误走坐标兜底。`query(target, prop)` 对齐第 1 层 `get_property`，并支持 Focused 组件查询。`wait_for` 使用 `WaitCondition::PropertyEquals { target, property, expected }` 这一可序列化形状循环 draw / 刷新 dirty signal / 重查属性，成功返回读到的值和 poll 次数，超时返回新增 `ComponentError::Timeout`；`wait_for_predicate` 作为进程内闭包便利 API 保留在 M2 层，不进入可序列化边界。`ComponentCommand` / `ComponentTarget` 以及 invoke / wait 结果类型已 serde 化，为 M4 协议层复用预留。
  - 验证：`cargo test -p atto-ui invoke_ -- --nocapture`；`cargo test -p atto-ui wait_for_ -- --nocapture`；`cargo test -p atto-ui query_matches -- --nocapture`；`cargo fmt --all`；`cargo fmt --all -- --check`；`git diff --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M2-6 用 `wait_for` / `invoke` 迁移一批 chat 逻辑测试**
  - 上下文：延续 M1-5，把 `pty_chat.rs`（`crates/atto-ui-chat/tests/pty_chat.rs`）中依赖 `sleep` 轮询（`:22`/`:73` 等多处 `thread::sleep`）+ 字形推断的一批逻辑用例，迁到 `wait_for` + 读值断言。
  - 实现：迁移一批（非全量）「测逻辑 / 状态」的用例；`sleep` 轮询屏幕改 `wait_for`；坐标点击改 `invoke(Id, action)`。保留纯渲染 / 端到端 PTY 用例。补齐所需 tag。
  - 明确边界：不改 chat 组件交互语义；不删渲染覆盖。
  - 验证：迁移用例不含 `find_text_position`/字形推断/裸 `sleep` 轮询；`cargo test -p atto-ui-chat`；全套通过。
  - 完成记录：`ChatInputPanel` 现按当前 input mode 声明并支持第 2 层语义 `ComponentCommand::InputText` / `SelectIndex` / `Submit`，让可执行命令通过 `DesktopInspector::invoke` 走 `InvokeDispatch::Semantic`。`InputText` 复用既有 `handle_text_paste` / `TextArea::replace_byte_range` 路径，保持多行粘贴归一化、光标和 draft binding 行为一致；`SelectIndex` 复用 choice / confirm 的既有 selection clamp 规则；`Submit` 复用 `emit_response`，因此 text / choice / confirm 提交、streaming queue、清空 draft/custom 与回调仍走原状态转移路径。新增 `crates/atto-ui-chat/tests/inspect_chat.rs` 进程内迁移覆盖：通过 tagged `ChatInputPanel` 构造 `Desktop`，使用 `invoke` / `wait_for(PropertyEquals)` / `wait_for_predicate` 验证 text submit、choice/confirm selection+submit、streaming queue 释放，不依赖 PTY 坐标、`find_text_position`、字形推断或裸 `sleep` 轮询。`crates/atto-ui-chat/tests/pty_chat.rs` 删除已迁移的提交回调与 input queue 纯逻辑用例，保留 input mode 渲染烟测、补全、滚动、消息列表、approval、tool disclosure 等 PTY 端到端覆盖。
  - 验证：`cargo test -p atto-ui-chat --test inspect_chat -- --nocapture`；`cargo test -p atto-ui-chat --test pty_chat -- --nocapture`；`cargo test -p atto-ui-chat`；`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M2-R Review — 第 2 层完整性与正确性复核**
  - 复核点：
    1. 四个叶子组件（Checkbox/Button/TextBox/Slider）的 `apply_command` 与各自既有鼠标 / 键盘交互**走同一状态转移与回调路径**，无重复 / 分叉逻辑；禁用态正确 `ignored()`。
    2. `invoke` 语义优先、坐标兜底，且路径可观测；`query` 与第 1 层 `get_property` 语义一致。
    3. `wait_for` 超时可控、不挂死、不轮询屏幕字符。
    4. API 入参可序列化（为 M4 铺路），无引用 / 闭包泄漏到边界（`wait_for` predicate 例外并记录）。
    5. 第 2 层不依赖第 3/4 层。
  - 验证：全套 fmt/clippy/test 通过；完成记录列出复核结论。
  - 完成记录：第 2 层复核通过。`Checkbox`、`Button`、`TextBox`、`Slider` 的 `apply_command` 均与既有键盘 / 鼠标路径复用同一状态转移与回调函数：checkbox 复用 `toggle()`，button 复用 `trigger()`，textbox 复用插入 / paste 路径，slider 复用 `set_value_and_emit` 的 clamp / step / callback 逻辑；禁用态均返回 `EventResult::ignored()` 且不触发回调。`DesktopInspector::invoke` 按 menu/window/component 三段式语义优先派发，语义不可用时保留坐标 / paste 兜底，并通过 `InvokeDispatch::{Semantic, CoordinateFallback, Unsupported}` 暴露路径；`query` 与第 1 层 `get_property` 对齐，`wait_for` 使用进程内 `WaitCondition::PropertyEquals` 循环 draw / dirty signal / query，不轮询屏幕字符，超时返回 `ComponentError::Timeout`。M2 API 入参保持可序列化值形状（`ComponentTarget`、`ComponentCommand`、属性名字符串、`ComponentValue`、`WaitCondition`），`wait_for_predicate` 作为进程内闭包便利入口留在 M2 边界并已记录为 M4 例外；M2 实现未引入第 3/4 层依赖。chat 迁移后的 `inspect_chat.rs` 使用 `invoke` / `wait_for` / 读值断言，原 PTY 文件中保留的坐标 helper 仅用于端到端渲染 / 交互覆盖。
  - 验证：`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

---

## 阶段 M3 - 第 4 层 L0+L1（tmux 甜点区，可与 M1/M2 并行）

目标：近乎免费、立刻见效的 tmux 伪装地基。**不依赖第 3 层**。让「已在为 tmux 适配」的程序（claude code / opencode / vim 插件）在 terminal view 里直接享受环境探测与原生剪贴板 passthrough。

- [x] **[DONE] M3-1 L0 环境探测注入**
  - 上下文：`crates/atto-ui-terminal/src/terminal.rs` 的 `spawn_command`（`:2775`）已统一设置 `TERM=xterm-256color`/`COLORTERM=truecolor`（M6.3 引入）。程序靠 `$TMUX`（socket,pid,session）、`$TMUX_PANE`、`$TERM=screen*/tmux*` 探测「在 tmux 里」。
  - 实现：在 `spawn_command` 的环境准备处，可选注入 `$TMUX`（格式 `socket_path,pid,session_id`）、`$TMUX_PANE`（如 `%<id>`）。是否注入 / socket 路径由配置或 builder 开关控制（默认关闭，避免误导未预期程序）。`$TMUX` 的 socket 路径此阶段可为占位 / 尚未监听的路径（真正 socket 在 M4 起来）——注入的目的先满足「探测存在性」。`$TERM` 是否改为 `tmux-256color` 作为开关项，默认保持现值以免破坏渲染。
  - 明确边界：只注入环境变量，不实现任何 tmux 子命令；关闭开关时行为与现状完全一致。
  - 验证：PTY 覆盖——开启开关后子进程 `echo $TMUX` / `echo $TMUX_PANE` 能读到注入值（复用 `snapshot_terminal_*` fixture + 子进程 probe）；关闭时子进程读到空；`cargo test -p atto-ui-terminal`；全套通过。
  - 完成记录：新增 `TerminalTmuxEnvironmentConfig` 作为持久化配置与 builder / handle 运行时开关，默认 `inject = false`，因此默认不向子进程写入 `$TMUX` / `$TMUX_PANE`，`TERM` 仍保持 `xterm-256color`，`COLORTERM` 仍保持 `truecolor`。开启后 `prepare_spawn_command` 注入 `$TMUX=socket_path,pid,session_id` 与 `$TMUX_PANE=%<pane_id>`；`server_pid` 可显式配置，未配置时使用当前进程 id；`override_term` 可选把 `TERM` 改为 `tmux-256color`。`TerminalEmulator::tmux_environment` 与 `TerminalHandle::{set_tmux_environment,tmux_environment}` 提供 builder / handle 入口；`TerminalConfig`、settings draft 与 YAML/JSON roundtrip 会保留 tmux 配置。实现只注入环境变量，不实现 tmux 子命令，也不在默认关闭时额外清理宿主继承环境；PTY 测试通过 `/usr/bin/env -u TMUX -u TMUX_PANE` 控制外层环境，验证默认关闭为空和开启后可读指定值。
  - 验证：`cargo test -p atto-ui-terminal tmux -- --nocapture`；`cargo test -p atto-ui-terminal terminal_config -- --nocapture`；`cargo test -p atto-ui-terminal terminal_settings_draft_round_trips_config -- --nocapture`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M3-2 L1 DCS `tmux;` passthrough 解包 → 原生 OSC**
  - 上下文：程序在 tmux 里发剪贴板 / 进度会用 DCS passthrough 包裹：`\033Ptmux;<escaped-inner>\033\\`（内层每个 `\033` 被转义成 `\033\033`）。解开后内层通常是 OSC 52 剪贴板（`\033]52;...\a`）或 OSC 9;4 进度。终端已有系统剪贴板后端 `TerminalSystemClipboard`（M4.6，terminal.rs）与 OSC 52 处理路径。
  - 实现：在终端输出解析链中识别 `\033Ptmux;` … `\033\\` 包裹，还原内层转义（`\033\033` → `\033`），把还原出的 OSC 52 走现有剪贴板后端（OSC 52 优先、arboard 兜底）、OSC 9;4 走进度处理（若已有则复用，否则先安全忽略）。
  - 降级：非 tmux DCS、包裹不完整 / 解析失败时不崩、不误写系统剪贴板、原样降级。
  - 明确边界：只做「解包 → 转交已有原生处理」，不新增剪贴板 / 进度后端。
  - 验证：单测 / PTY：`\033Ptmux;\033\033]52;c;<base64>\a\033\\` 被解包并写入剪贴板后端（复用 M4.6 可注入的假后端断言）；畸形包裹不崩;无包裹路径回归不变；`cargo test -p atto-ui-terminal`；全套通过。
  - 完成记录：新增流式 `TmuxDcsPassthroughDecoder` 并挂入 `TerminalShared`，`TerminalHandle::process_output` 现在会在 vt100 parser / DSR 检测前识别完整 `ESC P tmux; ... ESC \` 包裹，把内层 tmux 转义的 `ESC ESC` 严格还原为 `ESC`，再交给既有原生 OSC 处理链。因此 tmux passthrough 内的 OSC 52 继续复用现有 `TerminalCallbacks::copy_to_clipboard`、`TerminalClipboardCopy`、`on_clipboard_copy` 和可注入 `TerminalSystemClipboard` 后端；OSC 9;4 当前没有专用进度后端，解包后仍按既有未处理 OSC 路径安全忽略。解包器保留跨 `process_output` 分片状态，支持包裹在 PTY 读包边界被拆开；非 `tmux;` DCS、畸形 tmux 包裹和超长未完成控制串不会执行内部 OSC，避免当前 vt100 parser 把 DCS 内嵌 OSC 52 误当作原生剪贴板请求。实现只做解包和转交，不新增剪贴板或进度后端。
  - 验证：`cargo fmt --all`；`cargo test -p atto-ui-terminal tmux_dcs -- --nocapture`；`cargo test -p atto-ui-terminal -- --nocapture`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M3-R Review — 第 4 层 L0+L1 复核**
  - 复核点：
    1. 环境注入受开关控制，默认关闭时 spawn 行为与现状逐字节一致；开启时 `$TMUX`/`$TMUX_PANE` 格式符合程序探测预期。
    2. DCS passthrough 解包正确还原内层转义，转交现有 OSC 52 / 进度路径，不新造后端；畸形输入健壮降级、不误写剪贴板。
    3. 本阶段**未引入对第 3 层的任何依赖**。
    4. 保持 `#![forbid(unsafe_code)]`。
  - 验证：全套 fmt/clippy/test 通过；完成记录列出复核结论与手动验证提示（`cargo run -p atto-ui-terminal --example terminal_viewer`）。
  - 完成记录：第 4 层 L0+L1 复核通过。M3-1 的 tmux 环境注入仍由 `TerminalTmuxEnvironmentConfig::inject` 控制，默认关闭时不写入 `$TMUX` / `$TMUX_PANE`，只保留既有 `TERM=xterm-256color` 与 `COLORTERM=truecolor` spawn 准备行为；开启时 `$TMUX` 按 `socket_path,pid,session_id`，`$TMUX_PANE` 按 `%pane_id`，且只有 `override_term` 开启时才改用 `tmux-256color`。M3-2 的 `TmuxDcsPassthroughDecoder` 在 vt100 parser 前流式解包 `ESC P tmux; ... ESC \`，严格把内层 `ESC ESC` 还原为 `ESC` 后转交既有 OSC 52 clipboard callback / system clipboard 路径；当前无专用 OSC 9;4 进度后端，因此解包后仍按既有未处理 OSC 路径安全忽略。畸形 tmux DCS、非 tmux DCS 和超长未完成控制串不会执行内部 OSC，不会误写剪贴板。复核未发现 M3 引入 Unix socket、IPC server 或第 3 层协议依赖；`rg unsafe . -g '*.rs'` 未发现实际 `unsafe` 块。按测试失败策略，复核中修复了 `terminal_viewer` repro 测试为传配置路径而使用 `unsafe std::env::set_var` 的问题：`terminal_viewer` 现在支持 `--config <path>`，并作为 bin target 暴露给 PTY 测试使用 `CARGO_BIN_EXE_terminal_viewer`，同时保留 `cargo run -p atto-ui-terminal --example terminal_viewer` 手动入口。手动验证提示：可运行 `cargo run -p atto-ui-terminal --example terminal_viewer` 打开真实终端 viewer，检查默认非 tmux 环境与启用配置后的探测行为。
  - 验证：`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions repro_viewer -- --nocapture`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

---

## 阶段 M4 - 第 3 层 ipc（暴露到进程外）

目标：Unix domain socket + 自定义 JSON-RPC 类协议，把第 2 层语义 API 暴露给外部进程。依赖 M2 的可序列化 API 设计（决策 C/D）。

- [x] **[DONE] M4-1 协议定义（可序列化请求 / 响应）**
  - 上下文：M2 的 `invoke`/`query`/`wait_for`/`tree` 入参已按可序列化值设计（`ComponentCommand` `Clone+PartialEq`、`ComponentValue` serde、`DesktopSnapshot` serde 见 `inspect.rs:60`）。
  - 实现：定义 serde 序列化的请求 / 响应枚举（JSON-RPC 类：`id` + `method` + `params` / `result` / `error`），method 覆盖 `query`/`invoke`/`wait_for`/`tree`(export_snapshot)/`property_names`。`error` 映射 `ComponentError`（`component_api.rs:58`）。放在合适模块（建议新 crate 或 `src/` 下新模块，避免污染第 1/2 层）。
  - 验证：单测：每种请求 / 响应 JSON roundtrip；`ComponentError` 各变体可序列化；全套通过。
  - 完成记录：新增 `src/protocol.rs` 作为第 3 层 IPC 协议定义模块，只定义可序列化数据形状，不引入 socket、传输或 UI 主循环分发逻辑。`ProtocolRequest` 采用 JSON-RPC 类顶层 `{ id, method, params }` 外形，`ProtocolMethod` 覆盖 `query`、`invoke`、`wait_for`、`tree`（对应 `DesktopInspector::export_snapshot`）和 `property_names`；`query` / `property_names` 参数直接对齐第 1/2 层 API，`invoke` / `wait_for` / `tree` 参数使用可序列化 `runtime::Rect` 表达屏幕区域，`wait_for` 使用毫秒整数表达 timeout 与可选 poll interval，避免把进程内 `Duration` 暴露到协议边界。`ProtocolResponse` 采用顶层 `{ id, result }` 或 `{ id, error }`，成功结果由 `ProtocolResult::{Query,Invoke,WaitFor,Tree,PropertyNames}` 区分，对应承载 `ComponentValue`、`InvokeResult`、`WaitResult`、`DesktopSnapshot` 和属性名列表。`ComponentError` 现派生 `Serialize` / `Deserialize`，并将 `InvalidValue.expected` 改为拥有的 `String`，使 `NotFound`、`UnsupportedProperty`、`InvalidValue`、`ActionNotSupported`、`RenderFailed`、`Timeout` 均可直接在协议错误响应中 roundtrip；`invalid_value` 构造函数仍接收字符串字面量等 `Into<String>` 输入。新增单测覆盖所有请求类型 JSON roundtrip、所有成功响应类型 JSON roundtrip，以及所有 `ComponentError` 变体的直接序列化和错误响应 roundtrip。
  - 验证：`cargo test -p atto-ui protocol -- --nocapture`；`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M4-2 Unix socket server + 主循环请求分发**
  - 上下文：`DesktopInspector` 持 `&mut Desktop`，只能在持有 Desktop 的 UI 线程执行。外部请求需线程安全地转交该线程。
  - 实现：Unix domain socket 监听（路径由环境变量指定，为 M5 的 `$TMUX` socket 铺路）；接收线程解析 M4-1 协议 → 通过 channel 把请求交给 UI 线程，在其 tick / 事件循环中用 `desktop.inspect()` 执行 → 回传响应。定义清晰的集成点（UI 主循环每帧 drain 请求队列）。`wait_for` 在服务端循环，不阻塞其他请求处理的设计需说明。
  - 验证：集成测试：起 server → 客户端连 socket 发 `query`/`invoke` → 读到 / 改变状态 → 响应正确；modal 边界与进程内一致；全套通过。
  - 完成记录：新增 `src/ipc.rs` 作为第 3 层 Unix socket transport，实现 `IpcServerConfig`、`IpcServer`、`IPC_SOCKET_ENV=ATTO_UI_SOCKET` 和 `send_protocol_request` 测试 / 客户端 helper。server 绑定 Unix domain socket 后由 accept 线程接收连接，每个连接按 JSON line 解析 M4-1 `ProtocolRequest`，通过 `std::sync::mpsc` 把请求和一次性 response channel 转交持有 `Desktop` 的 UI 线程；UI 线程 drain 时使用 `DesktopInspector` 执行 `query`、`invoke`、`tree`、`property_names` 并回写 `ProtocolResponse`。畸形 JSON、无效 screen、执行错误和关闭的请求 / 响应 channel 均映射到协议 `error`，不 panic；socket bind 会拒绝覆盖非 socket 文件，并只在确认是不可连接的 stale socket 时清理旧 socket 文件。`AppHost` 新增 `enable_ipc` / `enable_ipc_from_env` / `disable_ipc` / `ipc_socket_path`，`AppHost::step`、`run_crossterm_desktop` 与 `run_crossterm_desktop_with_actions_and_tasks` 均在 draw 前每帧 drain IPC 请求；crossterm runner 会在 `ATTO_UI_SOCKET` 存在时自动启动 server。`wait_for` 没有调用会 sleep 的 `DesktopInspector::wait_for`，而是保存为 pending wait，每帧按 `poll_interval_ms` 做一次 `poll_wait_condition`，满足或超时后才响应，因此一个长等待不会阻塞其他连接上的请求处理。新增集成式单测覆盖 Unix socket query / invoke 改变活 `Binding` 状态、pending `wait_for` 期间其他 query 仍可响应，以及 `ComponentTarget::Focused` 在 active modal 存在时命中 modal 内焦点组件，保持与进程内 inspector 边界一致。
  - 验证：`cargo test -p atto-ui ipc -- --nocapture`；`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M4-3 外部 `atto` CLI 客户端**
  - 上下文：第一个进程外消费者（类 iTerm `it2`），也是端到端测试载体。
  - 实现：最小 CLI（新 bin / crate）连 socket，子命令 `query <tag> <prop>` / `invoke <tag> <action>` / `tree`，走 M4-1 协议。输出人类可读 + 可选 JSON。
  - 验证：端到端测试：启动带 server 的 fixture app → CLI 子命令驱动 UI 并读回状态；全套通过。
  - 完成记录：新增根 crate bin `atto`（`src/bin/atto.rs`，并在 `Cargo.toml` 注册），作为第一个进程外 IPC 客户端。CLI 通过 `--socket PATH` 或 `ATTO_UI_SOCKET` 选择 Unix socket，复用 M4-1/M4-2 的 `send_protocol_request`、`ProtocolRequest` 与 `ProtocolResponse`，未重新实现协议或绕过 server；支持 `query <tag> <prop>`、`invoke <tag> <action>`、`tree`，其中 `invoke` 覆盖 `click` / `toggle` / `submit` / `input-text` / `select-index` / `custom` 到 `ComponentCommand` 的映射，并提供 `--screen WIDTHxHEIGHT|X,Y,W,H` 给需要屏幕区域的请求。默认输出人类可读结果，`--json` 输出完整协议响应，便于脚本消费与测试断言。新增 `tests/atto_cli.rs` 端到端测试，启动启用 IPC 的 headless `AppHost` fixture，通过真实 `CARGO_BIN_EXE_atto` 进程执行 `query`、`invoke --json`、`query --json` 与 `tree --json`，断言 CLI 能读回 checkbox 状态、通过 socket 翻转 UI binding，并导出包含目标组件和窗口 tag 的 snapshot。
  - 验证：`cargo test -p atto-ui --test atto_cli -- --nocapture`；`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M4-R Review — 第 3 层完整性与正确性复核**
  - 复核点：
    1. 协议是「加传输 + 序列化」，**未重新设计语义**——method 与第 2 层 API 一一对应。
    2. 跨线程分发对持有 `Desktop` 的线程安全，无数据竞争；`wait_for` 不阻塞其他请求。
    3. 错误路径（未知 tag / 不支持动作 / 畸形请求）映射到协议 `error` 而非 panic。
    4. socket 路径策略为 M5 `$TMUX` 指向预留。
  - 验证：全套 fmt/clippy/test 通过；完成记录列出复核结论。
  - 完成记录：第 3 层复核通过。`src/protocol.rs` 只定义 JSON-RPC 类可序列化 envelope 与参数 / 结果枚举，`ProtocolMethod::{Query,Invoke,WaitFor,Tree,PropertyNames}` 分别映射第 1/2 层 `DesktopInspector::query`、`invoke`、`wait_for` / `poll_wait_condition`、`export_snapshot` 和 `property_names`，没有在协议层重新定义组件语义。`src/ipc.rs` 的 socket accept / client 线程只负责解析 JSON line 并通过 channel 排队；实际 `DesktopInspector` 调用均在 `IpcServer::drain_pending` 所在 UI 线程持有 `&mut Desktop` 时执行。`wait_for` 请求被保存为 pending wait，并在每帧按 poll interval 调用 `poll_wait_condition`，不会让一个长等待阻塞其他连接的 query / invoke。错误路径复核通过：执行错误统一写入 `ProtocolResponse.error`，新增 IPC 单测覆盖 `property_names` 成功路径、未知 tag 的 `NotFound`、不支持自定义动作的 `ActionNotSupported`，以及无效协议 method 的 `InvalidValue { name: "request", .. }`，均不 panic。socket 路径仍由 `ATTO_UI_SOCKET` / `IpcServerConfig` / 显式 `--socket` 指定，server 绑定策略保留 stale socket 清理与非 socket 文件拒绝逻辑，可供 M5 `$TMUX` 指向同一路径或派生路径。`AppHost` 与 crossterm runner 的 IPC drain 集成点仍在 draw 前每帧执行；`atto` CLI 复用 `send_protocol_request` 与协议类型，没有绕过 server。`rg -n "unsafe" . -g '*.rs'` 未发现实际 `unsafe` 块，M4 相关 crate 仍受 `#![forbid(unsafe_code)]` 约束。
  - 验证：`cargo test -p atto-ui ipc_server_maps_boundary_failures_to_protocol_errors -- --nocapture`；`cargo test -p atto-ui ipc -- --nocapture`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

---

## 阶段 M5 - 第 4 层 L2/L3（tmux 子命令 + 本地 pane 补全）

目标：把 tmux 接口面翻译成第 3 层调用（shim 为 client，非新协议），并补全本地 pane 体验。依赖 M4。

- [x] **[DONE] M5-1 send-keys / capture-pane 映射**
  - 上下文：`TerminalHandle::send_input_bytes`（`terminal.rs:3443`）= send-keys 载体，`snapshot`（`:3738`）= capture-pane 载体，`TerminalPaneGroupHandle`（`pane.rs:76`）暴露 `panes()`/`active_pane`/`pane_at_screen_position`。
  - 实现：在第 3 层协议 / server 侧提供 `send-keys`/`capture-pane`/`list-panes` 语义方法，映射到上述 handle。pane 寻址用 pane id（`TerminalPaneId`，`pane.rs:25`）。
  - 验证：集成测试：经第 3 层 send-keys 把字节送入目标 pane 的子进程、capture-pane 取回该 pane 屏幕快照；全套通过。
  - 完成记录：核心 `src/protocol.rs` 新增 `send_keys`、`capture_pane`、`list_panes` 三个可序列化方法及 `SendKeysResult`、`CapturePaneResult`、`PaneInfo` 成功载荷，协议 roundtrip 测试覆盖新增请求 / 响应。核心 `IpcServer` 新增 UI 线程扩展分发器，保留 `DesktopInspector` 原有方法分发；pane 方法在未注册处理器时返回显式 `ActionNotSupported`，不静默成功、不 panic。`atto-ui-terminal` 新增 `TerminalPaneIpc` / `terminal_pane_ipc_handler`，把 `send_keys` 映射到目标 pane 的 `TerminalHandle::send_input_bytes`，把 `capture_pane` 映射到 `TerminalHandle::snapshot()`，把 `list_panes` 映射到 `TerminalPaneGroupHandle::panes()`；pane 寻址使用 `TerminalPaneId::raw()` 的协议值，若注册多个 pane group 且出现重复 pane id，则返回 `InvalidValue`，避免把请求误投到任意 pane。`atto` CLI 对新增响应类型补了人类可读输出，保持 bin 编译完整。新增 `crates/atto-ui-terminal/tests/ipc_pane.rs`，覆盖第 3 层 IPC list/capture，并用真实 `/bin/sh` 子进程验证 `send_keys` 字节经 socket 进入目标 pane 后能由 `capture_pane` 读回回显。
  - 验证：`cargo test -p atto-ui protocol -- --nocapture`；`cargo test -p atto-ui ipc_server_reports_extension_methods_unsupported_without_handler -- --nocapture`；`cargo test -p atto-ui-terminal --test ipc_pane -- --nocapture`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M5-2 pane 管理命令映射**
  - 上下文：`TerminalPaneGroup`（`pane.rs:203`）已支持 `Ctrl+B %`/`"` 分屏、`o`/Tab 切焦点。tmux `split-window`/`select-pane -LRUD`/`list-panes`/`break-pane`/`display-popup` 需映射到原生 pane 与 `WindowManager`。
  - 实现：协议 / server 侧提供 pane 管理方法：`split-window`（→ pane 分屏）、`select-pane -LRUD`（→ 几何方向选 pane）、`break-pane`（pane→独立 Window）、`display-popup`（→ 浮动窗口）、`list-panes`。`break-pane` 参考 `SCRIPTING_LAYERS.md`「特色项」——常用、值得顺手触发。
  - 验证：集成测试：`split-window` 后 pane 数增加、`select-pane -L/-R` 按几何切换、`break-pane` 把 pane 变独立窗口；全套通过。
  - 完成记录：`src/protocol.rs` 新增 `split_window`、`select_pane`、`break_pane`、`display_popup` 四个可序列化 pane 管理方法，补充 `PaneSplitDirection`、`PaneSelectDirection`、对应 params/result payload，并保持既有 `list_panes` 作为 pane 枚举入口；协议 roundtrip 测试覆盖新增请求和响应。核心 `IpcMethodHandler` 现在接收当前 screen，使第 3 层 extension handler 可以在 UI 线程创建原生窗口；未注册 terminal handler 时新增方法仍返回明确 `ActionNotSupported`。`TerminalPaneGroup` 的 pane tree、pane 列表、active pane、last layout 与 pane factory 已收敛到共享权威状态，`TerminalPaneGroupHandle` 可同步执行 split、LRUD 几何 select 和 break；split / break 会在已有 last area 时立即重算布局，避免客户端在下一帧 draw 前连续发送 split→select 时读到过期 rect。`atto-ui-terminal` 的 IPC handler 将 `split_window` 映射到原生 pane 分屏，将 `select_pane` 映射到基于 pane 几何的 LRUD 焦点选择，将 `break_pane` 映射为从 pane group 移出目标 `TerminalEmulator` 并创建独立 normal `Window`，将 `display_popup` 映射为 floating terminal window（可选按 argv spawn 命令）。`atto` CLI 补齐新增结果类型的人类可读输出。新增 `ipc_pane` 集成测试通过真实 Unix socket、真实 `Desktop` 中的 `TerminalPaneGroup` 验证 split 后 pane 数增加、LRUD 几何选择、break 后独立窗口存在、display-popup 创建 floating window，并保留 M5-1 的 send/capture 覆盖。
  - 验证：`cargo test -p atto-ui protocol -- --nocapture`；`cargo test -p atto-ui ipc_server_reports_extension_methods_unsupported_without_handler -- --nocapture`；`cargo test -p atto-ui-terminal --test ipc_pane -- --nocapture`；`cargo test -p atto-ui-terminal pane_group -- --nocapture`；`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M5-3 `tmux` shim 可执行文件（决策 E 乙）**
  - 上下文：决策 E 倾向乙——shim 假 `tmux` 放子进程 `$PATH` 前列，拦截命令转第 3 层调用（薄翻译层），比逆向 tmux server 协议可控。配合 M3-1 注入的 `$TMUX`。
  - 实现：一个小 `tmux` shim bin：解析常用子命令（`send-keys`/`capture-pane`/`split-window`/`select-pane`/`list-panes`/`display-popup`/`break-pane`）→ 连 M4 socket（`$TMUX` 指向）→ 调 M5-1/M5-2 方法。不支持的子命令明确报错 / 降级，不静默假成功。`spawn_command` 把 shim 目录前置到子进程 `$PATH`。
  - 明确边界：不做 control mode（`-CC`，决策 F）；程序用绝对路径 / 查版本可能露馅，属已知限制，记录即可。
  - 验证：集成测试：子进程 `PATH` 前置 shim 后，`tmux send-keys` / `tmux capture-pane` / `tmux split-window` 经 socket 驱动原生 pane；不支持子命令返回非零并提示；全套通过。
  - 完成记录：`atto-ui-terminal` 新增 bin `tmux`（`crates/atto-ui-terminal/src/bin/tmux.rs`），作为纯客户端 shim：从 `-S PATH`、`$TMUX` 第一段或 `ATTO_UI_SOCKET` 解析 M4 Unix socket，解析 `send-keys`、`capture-pane`、`list-panes`、`split-window`、`select-pane`、`break-pane`、`display-popup` 后复用既有 M5-1/M5-2 `ProtocolRequest` 方法发送 IPC 请求，不实现 tmux server 协议，也不做 control mode；`-CC` 与未支持子命令均明确非零失败，不静默假成功。`send-keys` 支持 `-t`、`-l`、`-N` 与常用按键名（Enter/Space/Tab/Escape/Backspace/C-<key>），`capture-pane -p` 输出 capture 文本，`list-panes -F` 支持常用 pane format token，`split-window -h/-v`、`select-pane -LRUD`、`break-pane` 和 `display-popup` 分别映射到现有 pane 管理协议。`TerminalTmuxEnvironmentConfig` 新增可选 `shim_path`；`spawn_command` 在 `tmux.inject=true` 时注入 `$TMUX`/`$TMUX_PANE` 并把 `shim_path`（未设置时为当前可执行文件目录）前置到子进程 `PATH`，默认 `inject=false` 时行为不变。新增测试覆盖 unsupported subcommand 非零退出，以及真实子进程通过前置 `PATH` 调用 `tmux capture-pane -p` / `tmux send-keys` / `tmux split-window -h` 经 socket 驱动原生 pane；测试中确认使用非 login shell，避免 shell 启动文件重置 PATH 后误命中系统 tmux。已知限制按任务边界保留：不支持 control mode，程序使用绝对路径调用系统 tmux 或依赖完整版本探测时仍可能绕过 / 识别 shim。
  - 验证：`cargo test -p atto-ui-terminal tmux_shim -- --nocapture`；`cargo test -p atto-ui-terminal --test ipc_pane -- --nocapture`；`cargo fmt --all`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M5-4 本地 pane 层体验补全**
  - 上下文：`SCRIPTING_LAYERS.md`「本地 pane 层剩余缺口」——与第 4 层伪装无关，属 tmux-like 体验：方向性 pane 导航（`prefix+方向键`，现仅 `o`/Tab 线性）、pane resize（现固定五五分，可复用 `Splitter` 拖动）、pane zoom（`z` 临时全屏，现仅窗口级 ToggleMaximize）、pane 关闭（`x`）+ 重布局。
  - 实现：在 `TerminalPaneGroup`（`pane.rs`）的前缀命令处理中加 `prefix+方向键` 几何导航、`prefix+z` pane zoom、`prefix+x` pane 关闭 + 重布局、pane resize（键盘调分隔比例）。默认键位对齐 tmux 习惯。
  - 验证：PTY 覆盖：`Ctrl+B` + 方向键几何切 pane、`Ctrl+B z` pane 全屏 / 还原、`Ctrl+B x` 关 pane 后布局重排、resize 改变分隔比例；`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions`；全套通过。
  - 完成记录：`TerminalPaneGroup` 的 split tree 现为 split 节点保存可调 `first_len`，默认仍按五五分计算，`prefix+Ctrl+方向键` 会调整当前 active pane 最近相邻分隔线并在布局面积内夹紧，避免任一侧被压成无效尺寸。前缀命令新增 `prefix+方向键` 几何选择（复用现有 `select_pane` / `neighbor_pane` 几何逻辑）、`prefix+z` pane 级 zoom / restore（zoom 时仅 active pane 以整个 pane group 区域绘制，隐藏分隔线与其他 pane 可见 rect）、`prefix+x` close active pane 并通过现有 tree 移除 / 重布局路径回流；最后一个 pane 不会被关闭。`break_pane` / IPC pane 管理继续使用同一 tree 权威状态，方向选择在 zoom 状态下仍基于完整底层布局计算。`terminal_viewer` 的可见提示已同步为 pane zoom / resize / close 键位，旧的 `prefix+z` 窗口最大化 PTY 断言改为由 pane zoom 覆盖。
  - 验证：`cargo fmt --all`；`cargo test -p atto-ui-terminal pane_ -- --nocapture`；`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions pty_terminal_prefix_splits_panes_inside_one_window -- --nocapture`；`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions -- --nocapture`；`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。

- [x] **[DONE] M5-R Review — 第 4 层 L2/L3 完整性与正确性复核**
  - 复核点：
    1. shim / 子命令映射是第 3 层之上的**纯 client 翻译**，未在 socket 上重实现 tmux server 协议；未做 control mode（决策 F）。
    2. send-keys / capture-pane / pane 管理正确落到目标 pane / 原生窗口，pane 寻址稳定。
    3. 本地 pane 补全（方向导航 / resize / zoom / close）不破坏既有 `%`/`"`/`o`/Tab 与外层 WM 浮动窗口行为。
    4. 不支持的 tmux 子命令显式失败、不静默假成功。
  - 验证：全套 fmt/clippy/test 通过；完成记录列出复核结论与手动验证提示。
  - 完成记录：第 4 层 L2/L3 复核通过。M5 新增的 `send_keys` / `capture_pane` / `list_panes` / `split_window` / `select_pane` / `break_pane` / `display_popup` 只是 M4 协议上的可序列化方法与 terminal 扩展分发，核心 `IpcServer` 未重新实现 tmux server 协议；未注册 terminal handler 时显式返回 `ActionNotSupported`。`tmux` shim 只是纯客户端翻译器，从 `-S`、`$TMUX` 第一段或 `ATTO_UI_SOCKET` 解析 socket 后发送既有协议请求；`-CC` control mode、未知子命令和未知选项均非零失败，不静默假成功。terminal IPC handler 复用 `TerminalPaneGroupHandle` / `TerminalHandle` / `Desktop` 原生窗口操作，`send-keys` 与 `capture-pane` 按 pane id 命中目标 pane，多 pane group 下 pane id 冲突或缺省 target 会显式报错；`split-window`、方向性 `select-pane`、`break-pane` 和 `display-popup` 分别落到共享 pane tree 与原生 normal / floating 窗口。M5-4 的本地 pane 方向导航、resize、zoom、close 均复用同一共享 pane tree / active id / last layout 状态，保留 `%` / `"` / `o` / Tab 行为，PTY 覆盖确认 pane 分屏不改变外层 terminal window rect，也不扰动 sibling floating window。手动验证提示：可运行 `cargo run -p atto-ui-terminal --example terminal_viewer` 打开真实 viewer，再在启用 tmux 环境注入 / shim PATH 的终端子进程中试用 `tmux list-panes`、`tmux capture-pane -p`、`tmux send-keys`、`tmux split-window -h` 与 pane prefix 键位。
  - 验证：`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。完成记录写入后仅 `TODO.md` / `memory/claude_plan.md` 文档记录变化，未再重跑测试，复用上述绿色结果。

---

## 收尾

- [x] **[DONE] FINAL 文档与示例更新**
  - 根据实际实现更新 `SCRIPTING_LAYERS.md`（标注已落地层级 / 收敛后的最终决策）、`README.md`（若新增 `atto` CLI / tmux shim 用法）、`AGENTS.md`（如涉及代理使用）。若引入新 crate，更新 `CLAUDE.md` 的工作区 crates 清单与项目规模。
  - 验证：仅改文档时可沿用最近一次全套通过结果并注明；涉及代码则重跑全套。
  - 完成记录：`SCRIPTING_LAYERS.md` 已新增最终落地状态总览，并把第 1-4 层的“待定 / 建议 / 缺口”表述收敛为当前实现和最终决策：`tag` 寻址、`Binding` 属性读取、Unix socket + JSON-RPC 类协议、`atto` CLI、terminal pane IPC、client-side `tmux` shim，以及不实现 tmux server 协议 / control mode 的边界。根 `README.md` 已补充 `atto` CLI 的 `ATTO_UI_SOCKET` 使用示例、terminal pane IPC handler 边界、tmux shim 构建与能力说明，并同步 pane zoom / resize / close 快捷键摘要。`crates/atto-ui-terminal/README.md` 已补充 tmux 配置字段、shim 构建说明、pane IPC handler 注册说明和最新 pane 快捷键。`AGENTS.md` 已从旧 `IMPLEMENTATION_PLAN.md` 记账提示更新为当前 `TODO.md` / `PLAN.md` 约定。`CLAUDE.md` 已更新当前 workspace crate 清单、粗略项目规模、控制平面实现状态和文档资源入口。未更新 `PLAN.md`，因为本轮没有改变阶段级计划、依赖或完成标准。
  - 验证：`git diff --check` 通过。未运行 `cargo fmt` / `cargo clippy` / `cargo test`，因为本轮仅修改 Markdown 文档与 `memory/claude_plan.md` 进度记录，没有代码或编译输出相关变更；复用 M5-R 最近一次绿色结果：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。
