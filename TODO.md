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

- [ ] **[TODO] M1-3 tag 覆盖诊断辅助**
  - 上下文：`tag`/`id` 是 `Option`（`component.rs:354` 返回 `Option<&str>`），未标 tag 的节点不可寻址。约定「可脚本 / 可测组件必须显式标 tag」，需要一个诊断工具来发现漏标。
  - 实现：在 `DesktopInspector` 上加 `untagged_interactive_nodes(screen) -> Vec<InspectNode>`（或返回轻量描述），遍历 `build_desktop_tree`（`inspect.rs:343`）产物，筛出「可交互但 `id` 为 `None`」的节点。判定「可交互」：`property_names()` 含 `checked`/`text`/`selected`/`value`/`selection` 等交互属性之一，或 `is_focusable()` 为真（参考 `inspect.rs:724` 的 `is_snapshot_component_property` 白名单）。
  - 定位：这是诊断辅助（测试期使用），不是运行时强制；不改变任何交互行为。
  - 验证：单测构造含「标了 tag 的 Checkbox」+「未标 tag 的 Checkbox」的 Desktop，断言诊断只列出后者；全套通过。

- [ ] **[TODO] M1-4 变更信号聚合（为 M2 `wait_for` 预留）**
  - 上下文：reactive 是拉模型——`DirtyFlag`/`DirtyObserver`（`src/reactive/dirty.rs`），`check_and_clear()`（`:43`）返回自上次以来是否 dirty，`observer()`（`:50`）克隆观察者。第 2 层 `wait_for` 需要一个统一的「UI 是否发生过变更」进程内信号，避免轮询屏幕。
  - 实现：提供一个进程内变更检测封装（建议挂在 `DesktopInspector` 或独立小结构），聚合 desktop 关注的 `DirtyFlag`，暴露 `changed_since_last_poll() -> bool` 之类接口。**只做拉模型聚合**，不引入 push 订阅（`SCRIPTING_LAYERS.md` 第 1 层缺口 4 明确「不强求 push」）。
  - 明确边界：本任务只交付「信号读取」原语；真正的 `wait_for(predicate, timeout)` 循环在 M2-5 实现，此处不写等待循环。
  - 验证：单测：改一个 `Binding` 后聚合信号报告 changed；`mark_clean`/poll 后回落 false；全套通过。

- [ ] **[TODO] M1-5 进程内读值断言范式 + 示范迁移一例 chat 逻辑测试**
  - 上下文：兑现第 1 层独立价值。`crates/atto-ui-chat/tests/pty_chat.rs` 用 `find_text_position`（`:26`，抓屏 + `UnicodeWidthStr` 反算列坐标）和 `wait_for_disclosure_text`（`:54`，`sleep` + 字形 `▶` 推断展开状态）来测逻辑，脆弱且是「OCR 状态」。
  - 实现：
    1. 落地进程内测试范式样板：构造 `Desktop`（含带 `tag` 的 chat 组件）→ `desktop.inspect()` → `get_property`/`property_names` 读 `Binding` 活值断言。放在合适的测试模块（chat crate 的单测或集成测试）。
    2. **示范迁移一例**：挑 `pty_chat.rs` 中一个「断言的是逻辑 / 状态而非渲染」的用例（如 disclosure 展开态、某值是否更新），改写为进程内读值断言版本；保留（不删除）原 PTY 用例中真正测渲染 / 端到端的部分。
    3. 若 chat 组件相关节点缺 tag，按 M1-3 约定补标 tag。
  - 明确边界：只迁移**一例**作示范，不要求全量迁移；不得为此改动 chat 组件的交互语义。
  - 验证：迁移后的逻辑测试不含 `find_text_position`/字形推断，改为读值断言；新旧测试均通过；`cargo test -p atto-ui-chat`；全套通过。

- [ ] **[TODO] M1-R Review — 第 1 层完整性与正确性复核**
  - 复核点：
    1. 公共 `find_by_tag` 语义与旧 `component_find` 一致（含同名 tag、深层嵌套、mut 路径），`inspect.rs` 无残留重复递归。
    2. `DesktopInspector` 只读门面自洽，未混入第 2 层动作能力；第 1 层代码**不依赖** `apply_command` 的语义派发、不依赖第 2/3/4 层模块。
    3. tag 覆盖诊断与变更信号聚合均为进程内、纯读、不改变交互行为。
    4. 示范迁移的测试确实脱离了 OCR / 字形推断，且未误删渲染 / 端到端覆盖。
    5. 保持 `#![forbid(unsafe_code)]`。
  - 验证：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 全套通过；在完成记录中列出复核结论。

---

## 阶段 M2 - 第 2 层 scriptable（语义动作 + 查询 + 等待）

目标：在第 1 层「读」之上加「触发」和「等待」。核心是补齐叶子组件 `apply_command`，并提供按可序列化设计的进程内 `invoke`/`query`/`wait_for`。依赖 M1。

- [ ] **[TODO] M2-1 Checkbox `apply_command`**
  - 上下文：`src/widgets/checkbox.rs`，`#[derive(ComponentProperties)]`（`:17`），`impl Component`（`:74`），已有 `checked: Binding<bool>` 属性可被 `get_property("checked")` 读到。当前 `apply_command` 走 trait 默认实现（`component.rs:488`，返回 `ignored()`），外部触发只能退回合成点击。
  - 实现：实现 `Checkbox::apply_command`，`ComponentCommand::Toggle` 翻转 `checked`、`ComponentCommand::Click` 等价于用户点击（与键盘 Space/Enter 及鼠标点击相同的状态转移与回调触发路径，复用组件内既有的 toggle 逻辑，勿另写一套）。命中返回 `EventResult::consumed()`/合适结果，未命中的命令返回 `ignored()`。
  - 验证：进程内单测：`invoke`/`apply_command(Toggle)` 后 `checked` 的 `Binding` 翻转、`on_toggle` 类回调按既有语义触发；`cargo test -p atto-ui checkbox -- --nocapture`；全套通过。

- [ ] **[TODO] M2-2 Button `apply_command`**
  - 上下文：`src/widgets/button.rs`，`impl Component`（`:97`）。按钮激活当前靠 Enter/Space/鼠标点击触发 `on_activate` 类回调。
  - 实现：`ComponentCommand::Click`/`Submit` 触发与用户激活相同的回调路径；不改变禁用态语义（禁用时应 `ignored()`）。
  - 验证：进程内单测：`apply_command(Click)` 触发激活回调、禁用按钮 `ignored()` 且不触发回调；全套通过。

- [ ] **[TODO] M2-3 TextBox `apply_command`**
  - 上下文：`src/widgets/textbox.rs`，`impl Component`（`:135`），基于 `TextBuffer`（Unicode 感知），有 `text` 属性。现有 `inspect.rs` 的 `InputText` 兜底靠合成点击 + `Event::Paste`（`inspect.rs:246-271`）。
  - 实现：`ComponentCommand::InputText(s)` 直接把文本写入缓冲（语义级：设置 / 插入文本，遵循组件既有的粘贴 / 输入路径以保持光标、滚动、Unicode 行为一致），使 `get_property("text")` 随即反映新值。明确定义是「替换全部」还是「在光标处插入」——建议对齐现有 `Event::Paste` 语义（插入）并在完成记录中写明。
  - 验证：进程内单测：`apply_command(InputText("你好👋"))` 后 `text` 属性等于预期、宽字符 / emoji 不裂；全套通过。

- [ ] **[TODO] M2-4 Slider `apply_command`**
  - 上下文：`src/widgets/slider.rs`，`impl Component`（`:186`），有 min/max/value 类属性（见 `inspect.rs:724` 白名单含 `min`/`max`/`progress`）。
  - 实现：为 Slider 选一个语义动作落地——建议复用 `ComponentCommand::SelectIndex(usize)` 或 `Custom` 表示「设为某刻度 / 值」，与键盘左右箭头调值走相同的 clamp / 步进逻辑。在完成记录中写明选用的命令形态（供 M2-5 的 `invoke` 与将来 M4 序列化对齐）。
  - 验证：进程内单测：设值后 `value`/`progress` 属性更新且被 min/max clamp；越界值不 panic；全套通过。

- [ ] **[TODO] M2-5 进程内语义 API：`invoke` / `query` / `wait_for`**
  - 上下文：`inspect.rs` 已有 `action`/`action_target`（`:167`）与 `get_property`（`:136`），`ComponentTarget`（`component_api.rs:52`）支持 `Id`/`Focused`。本任务把它们收敛成第 2 层稳定语义 API，并新增 `wait_for`。
  - 实现：
    1. `invoke(target, action)`：语义级派发。目标组件实现了 `apply_command` 就走语义派发（M2-1..M2-4）；未实现才退回坐标注入兜底（`inspect.rs:222-278` 的现有逻辑保留）。返回值要能区分「语义派发成功」vs「退回坐标」以便可观测。
    2. `query(target, prop)`：= 第 1 层 `get_property`，统一命名对齐 API 形状。
    3. `wait_for(predicate, timeout)`：进程内循环——推进 UI（draw / tick）→ 用 M1-4 变更信号或直接重查 `predicate`（对 `query` 结果判定）→ 未成立则继续到超时。**替代 chat helper 的 `sleep` 轮询屏幕**。predicate 建议接受 `&mut DesktopInspector` 或以 `(target, prop, 期望值)` 表达以便将来序列化。
    4. **可序列化约束**：`invoke`/`query`/`wait_for` 的入参（target、action、prop、期望值）都用第 3 层能直接序列化的值（`ComponentCommand` 已 `Clone+PartialEq`，`ComponentValue` 已 serde）。在完成记录中确认无进程内独有的闭包 / 引用泄漏到 API 边界（`wait_for` 的 predicate 闭包属例外，M4 时再定序列化表达）。
  - 验证：进程内单测：`invoke("checkbox", Toggle)` 直接翻转 `Binding` 且可观测到「走了语义派发」而非坐标；`wait_for` 能等到由后台 / 定时驱动的状态成立、且超时返回错误不挂死；全套通过。

- [ ] **[TODO] M2-6 用 `wait_for` / `invoke` 迁移一批 chat 逻辑测试**
  - 上下文：延续 M1-5，把 `pty_chat.rs`（`crates/atto-ui-chat/tests/pty_chat.rs`）中依赖 `sleep` 轮询（`:22`/`:73` 等多处 `thread::sleep`）+ 字形推断的一批逻辑用例，迁到 `wait_for` + 读值断言。
  - 实现：迁移一批（非全量）「测逻辑 / 状态」的用例；`sleep` 轮询屏幕改 `wait_for`；坐标点击改 `invoke(Id, action)`。保留纯渲染 / 端到端 PTY 用例。补齐所需 tag。
  - 明确边界：不改 chat 组件交互语义；不删渲染覆盖。
  - 验证：迁移用例不含 `find_text_position`/字形推断/裸 `sleep` 轮询；`cargo test -p atto-ui-chat`；全套通过。

- [ ] **[TODO] M2-R Review — 第 2 层完整性与正确性复核**
  - 复核点：
    1. 四个叶子组件（Checkbox/Button/TextBox/Slider）的 `apply_command` 与各自既有鼠标 / 键盘交互**走同一状态转移与回调路径**，无重复 / 分叉逻辑；禁用态正确 `ignored()`。
    2. `invoke` 语义优先、坐标兜底，且路径可观测；`query` 与第 1 层 `get_property` 语义一致。
    3. `wait_for` 超时可控、不挂死、不轮询屏幕字符。
    4. API 入参可序列化（为 M4 铺路），无引用 / 闭包泄漏到边界（`wait_for` predicate 例外并记录）。
    5. 第 2 层不依赖第 3/4 层。
  - 验证：全套 fmt/clippy/test 通过；完成记录列出复核结论。

---

## 阶段 M3 - 第 4 层 L0+L1（tmux 甜点区，可与 M1/M2 并行）

目标：近乎免费、立刻见效的 tmux 伪装地基。**不依赖第 3 层**。让「已在为 tmux 适配」的程序（claude code / opencode / vim 插件）在 terminal view 里直接享受环境探测与原生剪贴板 passthrough。

- [ ] **[TODO] M3-1 L0 环境探测注入**
  - 上下文：`crates/atto-ui-terminal/src/terminal.rs` 的 `spawn_command`（`:2775`）已统一设置 `TERM=xterm-256color`/`COLORTERM=truecolor`（M6.3 引入）。程序靠 `$TMUX`（socket,pid,session）、`$TMUX_PANE`、`$TERM=screen*/tmux*` 探测「在 tmux 里」。
  - 实现：在 `spawn_command` 的环境准备处，可选注入 `$TMUX`（格式 `socket_path,pid,session_id`）、`$TMUX_PANE`（如 `%<id>`）。是否注入 / socket 路径由配置或 builder 开关控制（默认关闭，避免误导未预期程序）。`$TMUX` 的 socket 路径此阶段可为占位 / 尚未监听的路径（真正 socket 在 M4 起来）——注入的目的先满足「探测存在性」。`$TERM` 是否改为 `tmux-256color` 作为开关项，默认保持现值以免破坏渲染。
  - 明确边界：只注入环境变量，不实现任何 tmux 子命令；关闭开关时行为与现状完全一致。
  - 验证：PTY 覆盖——开启开关后子进程 `echo $TMUX` / `echo $TMUX_PANE` 能读到注入值（复用 `snapshot_terminal_*` fixture + 子进程 probe）；关闭时子进程读到空；`cargo test -p atto-ui-terminal`；全套通过。

- [ ] **[TODO] M3-2 L1 DCS `tmux;` passthrough 解包 → 原生 OSC**
  - 上下文：程序在 tmux 里发剪贴板 / 进度会用 DCS passthrough 包裹：`\033Ptmux;<escaped-inner>\033\\`（内层每个 `\033` 被转义成 `\033\033`）。解开后内层通常是 OSC 52 剪贴板（`\033]52;...\a`）或 OSC 9;4 进度。终端已有系统剪贴板后端 `TerminalSystemClipboard`（M4.6，terminal.rs）与 OSC 52 处理路径。
  - 实现：在终端输出解析链中识别 `\033Ptmux;` … `\033\\` 包裹，还原内层转义（`\033\033` → `\033`），把还原出的 OSC 52 走现有剪贴板后端（OSC 52 优先、arboard 兜底）、OSC 9;4 走进度处理（若已有则复用，否则先安全忽略）。
  - 降级：非 tmux DCS、包裹不完整 / 解析失败时不崩、不误写系统剪贴板、原样降级。
  - 明确边界：只做「解包 → 转交已有原生处理」，不新增剪贴板 / 进度后端。
  - 验证：单测 / PTY：`\033Ptmux;\033\033]52;c;<base64>\a\033\\` 被解包并写入剪贴板后端（复用 M4.6 可注入的假后端断言）；畸形包裹不崩;无包裹路径回归不变；`cargo test -p atto-ui-terminal`；全套通过。

- [ ] **[TODO] M3-R Review — 第 4 层 L0+L1 复核**
  - 复核点：
    1. 环境注入受开关控制，默认关闭时 spawn 行为与现状逐字节一致；开启时 `$TMUX`/`$TMUX_PANE` 格式符合程序探测预期。
    2. DCS passthrough 解包正确还原内层转义，转交现有 OSC 52 / 进度路径，不新造后端；畸形输入健壮降级、不误写剪贴板。
    3. 本阶段**未引入对第 3 层的任何依赖**。
    4. 保持 `#![forbid(unsafe_code)]`。
  - 验证：全套 fmt/clippy/test 通过；完成记录列出复核结论与手动验证提示（`cargo run -p atto-ui-terminal --example terminal_viewer`）。

---

## 阶段 M4 - 第 3 层 ipc（暴露到进程外）

目标：Unix domain socket + 自定义 JSON-RPC 类协议，把第 2 层语义 API 暴露给外部进程。依赖 M2 的可序列化 API 设计（决策 C/D）。

- [ ] **[TODO] M4-1 协议定义（可序列化请求 / 响应）**
  - 上下文：M2 的 `invoke`/`query`/`wait_for`/`tree` 入参已按可序列化值设计（`ComponentCommand` `Clone+PartialEq`、`ComponentValue` serde、`DesktopSnapshot` serde 见 `inspect.rs:60`）。
  - 实现：定义 serde 序列化的请求 / 响应枚举（JSON-RPC 类：`id` + `method` + `params` / `result` / `error`），method 覆盖 `query`/`invoke`/`wait_for`/`tree`(export_snapshot)/`property_names`。`error` 映射 `ComponentError`（`component_api.rs:58`）。放在合适模块（建议新 crate 或 `src/` 下新模块，避免污染第 1/2 层）。
  - 验证：单测：每种请求 / 响应 JSON roundtrip；`ComponentError` 各变体可序列化；全套通过。

- [ ] **[TODO] M4-2 Unix socket server + 主循环请求分发**
  - 上下文：`DesktopInspector` 持 `&mut Desktop`，只能在持有 Desktop 的 UI 线程执行。外部请求需线程安全地转交该线程。
  - 实现：Unix domain socket 监听（路径由环境变量指定，为 M5 的 `$TMUX` socket 铺路）；接收线程解析 M4-1 协议 → 通过 channel 把请求交给 UI 线程，在其 tick / 事件循环中用 `desktop.inspect()` 执行 → 回传响应。定义清晰的集成点（UI 主循环每帧 drain 请求队列）。`wait_for` 在服务端循环，不阻塞其他请求处理的设计需说明。
  - 验证：集成测试：起 server → 客户端连 socket 发 `query`/`invoke` → 读到 / 改变状态 → 响应正确；modal 边界与进程内一致；全套通过。

- [ ] **[TODO] M4-3 外部 `atto` CLI 客户端**
  - 上下文：第一个进程外消费者（类 iTerm `it2`），也是端到端测试载体。
  - 实现：最小 CLI（新 bin / crate）连 socket，子命令 `query <tag> <prop>` / `invoke <tag> <action>` / `tree`，走 M4-1 协议。输出人类可读 + 可选 JSON。
  - 验证：端到端测试：启动带 server 的 fixture app → CLI 子命令驱动 UI 并读回状态；全套通过。

- [ ] **[TODO] M4-R Review — 第 3 层完整性与正确性复核**
  - 复核点：
    1. 协议是「加传输 + 序列化」，**未重新设计语义**——method 与第 2 层 API 一一对应。
    2. 跨线程分发对持有 `Desktop` 的线程安全，无数据竞争；`wait_for` 不阻塞其他请求。
    3. 错误路径（未知 tag / 不支持动作 / 畸形请求）映射到协议 `error` 而非 panic。
    4. socket 路径策略为 M5 `$TMUX` 指向预留。
  - 验证：全套 fmt/clippy/test 通过；完成记录列出复核结论。

---

## 阶段 M5 - 第 4 层 L2/L3（tmux 子命令 + 本地 pane 补全）

目标：把 tmux 接口面翻译成第 3 层调用（shim 为 client，非新协议），并补全本地 pane 体验。依赖 M4。

- [ ] **[TODO] M5-1 send-keys / capture-pane 映射**
  - 上下文：`TerminalHandle::send_input_bytes`（`terminal.rs:3443`）= send-keys 载体，`snapshot`（`:3738`）= capture-pane 载体，`TerminalPaneGroupHandle`（`pane.rs:76`）暴露 `panes()`/`active_pane`/`pane_at_screen_position`。
  - 实现：在第 3 层协议 / server 侧提供 `send-keys`/`capture-pane`/`list-panes` 语义方法，映射到上述 handle。pane 寻址用 pane id（`TerminalPaneId`，`pane.rs:25`）。
  - 验证：集成测试：经第 3 层 send-keys 把字节送入目标 pane 的子进程、capture-pane 取回该 pane 屏幕快照；全套通过。

- [ ] **[TODO] M5-2 pane 管理命令映射**
  - 上下文：`TerminalPaneGroup`（`pane.rs:203`）已支持 `Ctrl+B %`/`"` 分屏、`o`/Tab 切焦点。tmux `split-window`/`select-pane -LRUD`/`list-panes`/`break-pane`/`display-popup` 需映射到原生 pane 与 `WindowManager`。
  - 实现：协议 / server 侧提供 pane 管理方法：`split-window`（→ pane 分屏）、`select-pane -LRUD`（→ 几何方向选 pane）、`break-pane`（pane→独立 Window）、`display-popup`（→ 浮动窗口）、`list-panes`。`break-pane` 参考 `SCRIPTING_LAYERS.md`「特色项」——常用、值得顺手触发。
  - 验证：集成测试：`split-window` 后 pane 数增加、`select-pane -L/-R` 按几何切换、`break-pane` 把 pane 变独立窗口；全套通过。

- [ ] **[TODO] M5-3 `tmux` shim 可执行文件（决策 E 乙）**
  - 上下文：决策 E 倾向乙——shim 假 `tmux` 放子进程 `$PATH` 前列，拦截命令转第 3 层调用（薄翻译层），比逆向 tmux server 协议可控。配合 M3-1 注入的 `$TMUX`。
  - 实现：一个小 `tmux` shim bin：解析常用子命令（`send-keys`/`capture-pane`/`split-window`/`select-pane`/`list-panes`/`display-popup`/`break-pane`）→ 连 M4 socket（`$TMUX` 指向）→ 调 M5-1/M5-2 方法。不支持的子命令明确报错 / 降级，不静默假成功。`spawn_command` 把 shim 目录前置到子进程 `$PATH`。
  - 明确边界：不做 control mode（`-CC`，决策 F）；程序用绝对路径 / 查版本可能露馅，属已知限制，记录即可。
  - 验证：集成测试：子进程 `PATH` 前置 shim 后，`tmux send-keys` / `tmux capture-pane` / `tmux split-window` 经 socket 驱动原生 pane；不支持子命令返回非零并提示；全套通过。

- [ ] **[TODO] M5-4 本地 pane 层体验补全**
  - 上下文：`SCRIPTING_LAYERS.md`「本地 pane 层剩余缺口」——与第 4 层伪装无关，属 tmux-like 体验：方向性 pane 导航（`prefix+方向键`，现仅 `o`/Tab 线性）、pane resize（现固定五五分，可复用 `Splitter` 拖动）、pane zoom（`z` 临时全屏，现仅窗口级 ToggleMaximize）、pane 关闭（`x`）+ 重布局。
  - 实现：在 `TerminalPaneGroup`（`pane.rs`）的前缀命令处理中加 `prefix+方向键` 几何导航、`prefix+z` pane zoom、`prefix+x` pane 关闭 + 重布局、pane resize（键盘调分隔比例）。默认键位对齐 tmux 习惯。
  - 验证：PTY 覆盖：`Ctrl+B` + 方向键几何切 pane、`Ctrl+B z` pane 全屏 / 还原、`Ctrl+B x` 关 pane 后布局重排、resize 改变分隔比例；`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions`；全套通过。

- [ ] **[TODO] M5-R Review — 第 4 层 L2/L3 完整性与正确性复核**
  - 复核点：
    1. shim / 子命令映射是第 3 层之上的**纯 client 翻译**，未在 socket 上重实现 tmux server 协议；未做 control mode（决策 F）。
    2. send-keys / capture-pane / pane 管理正确落到目标 pane / 原生窗口，pane 寻址稳定。
    3. 本地 pane 补全（方向导航 / resize / zoom / close）不破坏既有 `%`/`"`/`o`/Tab 与外层 WM 浮动窗口行为。
    4. 不支持的 tmux 子命令显式失败、不静默假成功。
  - 验证：全套 fmt/clippy/test 通过；完成记录列出复核结论与手动验证提示。

---

## 收尾

- [ ] **[TODO] FINAL 文档与示例更新**
  - 根据实际实现更新 `SCRIPTING_LAYERS.md`（标注已落地层级 / 收敛后的最终决策）、`README.md`（若新增 `atto` CLI / tmux shim 用法）、`AGENTS.md`（如涉及代理使用）。若引入新 crate，更新 `CLAUDE.md` 的工作区 crates 清单与项目规模。
  - 验证：仅改文档时可沿用最近一次全套通过结果并注明；涉及代码则重跑全套。
