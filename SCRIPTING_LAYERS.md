# SCRIPTING_LAYERS.md — 脚本化 / Introspection 分层设计

本文记录 atto-ui 引入"组件 introspection + 脚本化 + 外部控制"能力的分层设计与讨论结论。目标是把一个稳定的控制平面**分四层依次叠加**地建起来,每层是上一层的地基,依赖方向单向、不回头。

## 最终落地状态

截至当前实现,四层控制平面已按本文路线落地:

| 层级 | 状态 | 最终决策 / 已实现入口 |
|---|---|---|
| 第 1 层 introspection | 已落地 | 公共 `find_by_tag` / `find_by_tag_mut`;`DesktopInspector` 提供 `tree`、`export_snapshot`、`property_names`、`get_property`、`set_property`、tag 覆盖诊断与变更信号聚合。 |
| 第 2 层 scriptable | 已落地 | `DesktopInspector::query` / `invoke` / `wait_for`;关键叶子控件补齐 `apply_command`,逻辑测试可直接读活 `Binding` 值。 |
| 第 3 层 ipc | 已落地 | Unix domain socket + JSON-RPC 类协议;`ATTO_UI_SOCKET`;外部 `atto` CLI 支持 `query` / `invoke` / `tree`。 |
| 第 4 层 tmux adapter | 已落地 | L0 环境注入、L1 tmux DCS passthrough、L2/L3 pane 方法与 client-side `tmux` shim;未实现 tmux server 协议和 control mode。 |

最终决策:寻址复用稳定字符串 `tag`;状态读取以 `Binding` 反射出的 `ComponentValue` 为主;IPC 传输使用 Unix domain socket;协议使用 atto-ui 自有 JSON-RPC 类 envelope;tmux 子命令采用 **shim 可执行文件** 翻译到第 3 层,而不是逆向实现 tmux server 协议;control mode 明确不做。

## 动机:从"测试繁琐"倒推出来的通用能力

直接触发点是 PTY 测试框架(`crates/atto-ui-test-host`)在测**逻辑与状态变化**时非常繁琐。

**PTY 测试的能力边界(它的主场 vs 被误用):**

| 测什么 | PTY 测试 | 适配度 |
|---|---|---|
| 渲染正确性:字符、颜色、宽字符、边框、布局、滚动可见区 | 为之而生 | ✅ 不可替代 |
| 端到端真实性:真 PTY → 真 crossterm 编码 → 真 vt100 解析 | 唯一能覆盖整条输入→渲染链的方式 | ✅ 不可替代 |
| 逻辑 / 状态 / 变化:值变了没、状态机转对没、回调触发没 | 只能靠 OCR 屏幕反推 | ❌ 被误用 |

**繁琐的三个根源**(以 `crates/atto-ui-chat/tests/pty_chat.rs` 开头的 helper 为证):
1. **靠屏幕文字反查坐标**:`find_text_position` 抓屏 → 逐行 `find` 文字 → `UnicodeWidthStr` 把字节位置换算成列坐标才能 `click`。控件挪位/改字即碎。
2. **靠轮询屏幕猜状态**:`wait_for_disclosure_text` 用 `sleep(10ms)` 循环抓屏、用字形(`▶`)推断"是否展开"。把"读状态"降级成"OCR"。
3. **断言只能断字符,断不了值**:验证 textbox 内容 / checkbox 勾选 / list 选中项,都只能在渲染字符里找证据。渲染正确 ≠ 状态正确。

**结论**:PTY 测试**不该被取代**,而应回归渲染主场;把被误塞进它的"逻辑测试"用一条**语义操作 + 语义断言**的新路径接出来。这条新路径的通用形态,恰好就是一个控制平面——测试只是它的第一个、也是风险最低的消费者。

**分工,而非替代:**
```
                     测的是什么?
        ┌──────────────────────┴──────────────────────┐
        ▼                                              ▼
  渲染正确性 / 端到端                          逻辑 / 状态 / 变化
   PTY 测试(保留不动)                       脚本化控制平面(新增)
   screen_contents / cell_*                query 值 / invoke 事件 / 等状态
   "看起来对不对"                            "算得对不对"
```

## 四层架构

四层依次叠加,每层把下层能力"再暴露一次"给更远的消费者:

```
4. tmux protocol adapter   ── 把 tmux 命令/转义翻译成第 3 层调用(众多适配器之一)
        ▲ 依赖
3. ipc                     ── 把第 2 层能力暴露到进程外(socket / CLI / 配置 / language binding)
        ▲ 依赖
2. scriptable              ── 命令-查询-事件的语义 API:invoke 动作 / 读状态 / 等待状态
        ▲ 依赖
1. introspection           ── 组件可寻址、状态可读(纯读、进程内、零协议)
```

**核心原则**:
- 第 1 层是所有东西的地基,且**自身即有独立价值**(解放 PTY 逻辑测试),不必等上面三层。
- 每往上一层 = 把下层能力暴露给更远的消费者:introspection→进程内、scriptable→测试代码、ipc→外部进程、tmux→不知情的第三方程序。
- 上层是下层的 **consumer**,不是下层的重新实现。tmux adapter 只是"翻译",不是"再造 tmux 协议"。

---

## 第 1 层:introspection(地基)

**职责**:组件能被稳定寻址;其当前状态能被读取。进程内、无协议。

> 注:这一层描述的是*能力目标*(状态可读)。落地的 `DesktopInspector` 门面并非类型级只读——它持 `&mut Desktop`,读方法(`tree`/`snapshot`/`get_property`)也取 `&mut self` 并跑一次 layout/draw 以刷新 bounds 与 dirty(即"读"带渲染副作用),且同一门面在第 2 层叠加了写方法。详见 `DesktopInspector` 的 doc。

**做完即可独立交付**:逻辑测试从"OCR 屏幕"变成"读值"。

### 现状:地基大部分已存在(重要发现)

摸底 `src/runtime/` + `src/reactive/` + `src/inspect.rs` 后确认,introspection 这条**读取链基本齐备**:

- **寻址键已有且稳定**:唯一稳定、用户可指定的寻址键是字符串 `tag`(在 spec 层叫 `id`)。
  - 组件层 `ComponentTag { id: String }`(`src/composable/component_tag.rs:18-21`),`.tag("my-id")` 扩展方法可挂任意组件,`DynamicTree::tag()` 返回它。
  - spec 层 `ComponentSpec.id: Option<String>`(`src/runtime/spec.rs:373`),构建时 `wrap_with_id` 变成 `ComponentTag`。
  - 窗口/菜单层也有 `Window.tag` / `MenuSpec.tag` / `MenuItem.tag`。
  - ⚠️ `ComponentId`(`src/composable/node.rs:8`)是全局自增 `AtomicU64`,**不跨运行/不跨重建稳定,不能做外部键**,只用于内部焦点/滚动定位。
- **状态读取已有且直连活值**:`Component::get_property/set_property/property_names`(`src/composable/component.rs:476-486`)由 `#[derive(ComponentProperties)]` 宏自动填充(`crates/atto-ui-macros/src/component_properties.rs`),把每个 `Binding<T>` 字段暴露成可读可写属性。`get_property` 直接 `Binding::get()` 取**活状态**。
  - 组件交互态绝大多数是 `Binding<T>`(reactive):Checkbox 的 `checked`、TextBox 的 `text` 等,天然可读。
- **现成门面 `DesktopInspector`(`src/inspect.rs`)**:已提供 `tree()`、`export_snapshot()`(可 serde 的 `DesktopSnapshot`)、`get_property(id,name)`、`set_property(id,name,val)`、`action(id, ComponentCommand)`,内部 `component_find` 按 `tag()` 递归寻址整棵活树。**这几乎就是第 1 层的门面雏形。**

### 第 1 层缺口

1. **寻址实现分散**:`runtime/tree.rs` 有 PathIndex 但私有;`inspect.rs` 又自己写了一遍递归 `component_find`。缺一个公共的 `Component::find_by_tag`。
2. **无强制 tag**:`tag`/`id` 是 `Option`,没标的节点不可寻址。测试要用需约定"可测组件必须标 tag"。
3. **反射受限**:只反射 `Binding` 字段,且类型受宏白名单限制;非 Binding 的业务态(自定义枚举等)默认不可见。
4. **只能拉、不能推**:reactive 变更是版本号拉模型(`DirtyFlag`/`DirtyObserver`),没有 push 式订阅——第 2 层"等待状态"要么轮询版本号,要么给关键状态补事件。

### 第 1 层最终决策

- **A. 寻址方案**:复用字符串 `tag`(已稳定、贯穿 spec/活树),并把 `inspect.rs` 的递归寻址收敛为公共 `find_by_tag` / `find_by_tag_mut`。约定"可测/可脚本组件必须显式标 `tag`"。
- **B. 状态读取**:以 `get_property` / `property_names`(复用 `Binding` 反射)为主干,复杂/非 Binding 状态按需补充结构化描述。`DesktopInspector::untagged_interactive_nodes` 用于测试期发现漏标 tag 的交互节点。

---

## 第 2 层:scriptable(语义动作 + 事件 + 等待)

**职责**:在 introspection 的"读"之上加"触发"和"等待"。核心三类 API:
- `invoke(target, action)` —— 语义级动作(**不是**合成坐标鼠标事件)
- `query(target, prop)` —— 读值(第 1 层能力)
- `wait_for(predicate)` —— 等待某语义状态成立(替代 `sleep` 轮询屏幕)

### 现状与缺口

- **动作抽象已有**:`apply_command(ComponentCommand)`(`component.rs:488`)+ `ComponentCommand`(`src/component_api.rs`)是"触发动作"的入口,`inspect.rs::action` 已封装。
- **关键短板 —— 从外部触发的方向反了 / 大面积缺实现**:
  - `CallbackHandle`(`src/runtime/callback_handle.rs`)只能由组件**自己 emit**(组件→外部方向),外部无法"按 handle 反向触发某组件回调"。
  - `apply_command` **只有约 8 个组件实现**(disclosure/tab_view/list/table/radio/typeahead/scroll_container/visibility/border)。**Checkbox / Button / TextBox / Slider 等关键叶子组件未实现**,schema 声明了 action 却没落地,外部触发只能退化成合成鼠标事件。
  - → **第 2 层的主要工作量在补齐叶子组件的 `apply_command`**(或提供统一的外部触发入口),让 `invoke("close-btn", Click)` 能语义级派发,而非回退到坐标注入。

### 第 2 层设计约束

- **API 形状按"将来可被 IPC 序列化"设计**(命令/查询/事件都是可序列化的值),但第 2 层先只提供**进程内 Rust 实现**,供 `PtyTestHost` 或纯单元测试直接调用。这样第 3 层只是"加传输 + 序列化",不重新设计语义。

---

## 第 3 层:ipc(暴露到进程外)

**职责**:传输 + 序列化,把第 2 层语义 API 暴露给外部进程。

- 第 3 层实现为 "socket server + 序列化",不重新设计第 1/2 层语义。
- **最终决策**:
  - 传输:**Unix domain socket**。默认环境变量为 `ATTO_UI_SOCKET`;`AppHost::enable_ipc` / `enable_ipc_from_env` 和 crossterm runner 都会在 UI 线程每帧 drain 请求。
  - 协议形态:**自定义 JSON-RPC 类协议**。method 覆盖 `query` / `invoke` / `wait_for` / `tree` / `property_names`,并扩展 terminal pane 的 `send_keys` / `capture_pane` / `list_panes` / `split_window` / `select_pane` / `break_pane` / `display_popup`。tmux 语法只作为第 4 层翻译目标。
- **消费者不止 tmux**:脚本化(外部 `atto` CLI 连 socket 驱动 UI,类 iTerm 的 `it2`)、配置驱动(启动读声明式布局文件 = 控制平面命令的批处理)都在这一层受益。

---

## 第 4 层:tmux protocol adapter(一个消费者)

**职责**:把 tmux 的接口面翻译成第 3 层命令。**它是第 3 层之上的一个 client,不是新协议实现。**

### 定位:伪装成 tmux,让运行在 terminal view 里的程序驱动我们的原生 UI

不做"真 tmux 套在我们里面"的套娃,而是让跑在 terminal view 里的程序(agent、vim、fzf)**以为自己在 tmux 里**,从而通过 tmux 的接口来利用我们的 fancy 原生窗口/split。

**程序不"用 tmux 功能",而是探测环境 + 走三个接口面**(基于实证调研):

| 接口面 | 内容 | 谁在用 | 落地层级 |
|---|---|---|---|
| **环境探测** | `$TMUX`(socket,pid,session)、`$TMUX_PANE`、`$TERM=screen*/tmux*` | opencode/claude code/vim 插件全靠 `$TMUX` | L0(几行,地基) |
| **转义序列行为** | DCS passthrough `\033Ptmux;...\033\\`、OSC 52 剪贴板、OSC 9;4 进度 | claude code `/copy`、opencode `writeOsc52`、status shimmer | L1(低成本,复用现有 `on_clipboard_copy`+arboard) |
| **`tmux` 子命令** | `send-keys` / `capture-pane` / `display-popup` / `split-window` / `select-pane -LRUD` / `list-panes` / `break-pane` | fzf `--tmux`、vim-tmux-navigator、agent 编排/dashboard | L2/L3(已通过 shim 翻译到第 3 层,不实现 tmux server 协议) |

**性价比分层**:

| 级别 | 做什么 | 成本 | 杠杆 |
|---|---|---|---|
| **L0 环境探测** | spawn 子进程时注入 `$TMUX`/`$TMUX_PANE`/`$TERM` | 极低 | 地基,不做全灭 |
| **L1 DCS/OSC passthrough** | 认 `\033Ptmux;` 包裹 → 拆开 → 走原生 OSC 52/9(已有 `on_clipboard_copy`/arboard) | **低** | **最高**:claude code/opencode 立即受益 |
| **L2 send-keys/capture-pane** | `tmux` shim 解析子命令后调用第 3 层 pane IPC 方法 | 中 | agent 编排/dashboard/fzf 关键 |
| **L3 pane 管理命令** | split/select/display-popup/break 映射到原生 pane / 原生窗口 | 中高 | vim-tmux-navigator、fzf popup、并行 agent |

**甜点区 = L0 + L1**:近乎免费、立刻见效——已有 OSC 52 回调 + arboard + 环境变量注入 + 一个 `\033Ptmux;` 解包,就能让"已经在为 tmux 适配"的程序在 terminal view 里直接享受原生剪贴板/passthrough。强烈建议先做。

### `tmux` 子命令伪装的最终做法

最终采用 **shim `tmux` 可执行文件**。`atto-ui-terminal` 提供一个名为 `tmux` 的小二进制,把假 `tmux` 放进子进程 `$PATH` 前列后,常用子命令会被解析成第 3 层协议请求。shim 从 `-S PATH`、`$TMUX` 第一段或 `ATTO_UI_SOCKET` 找 Unix socket。

已支持的子命令:
- `send-keys [-t %PANE] [-l] [-N COUNT] <key>...`
- `capture-pane [-p] [-t %PANE]`
- `list-panes [-F FORMAT]`
- `split-window [-h|-v] [-t %PANE]`
- `select-pane [-L|-R|-U|-D] [-t %PANE]`
- `break-pane [-t %PANE]`
- `display-popup [-T TITLE] [-x X -y Y -w W -h H] [--] [command ...]`

明确不做:
- 不实现 tmux server socket 协议。
- 不支持 control mode(`-CC`)。
- 不支持完整 tmux CLI 兼容性;使用绝对路径调用系统 tmux、做完整版本探测或依赖未实现子命令的程序可能绕过 / 识别 shim。不支持的命令必须显式失败,不能静默假成功。

---

## 关键设计:tmux 的概念如何映射(不沿用 tmux 的 window 层)

tmux 的 session/window/pane 三层,其 window 层本质是"被困在单个宿主终端一块矩形里"不得已造的伪多窗口(同一时刻只见一个,靠 `n/p` 切换)。**我们这个库天生多窗口——宿主给的是整个桌面,不是一块矩形。** 所以用原生 `WindowManager` 承载 tmux window 是升维,不是妥协:

| tmux 概念 | 存在原因 | 我们的原生对应 | 谁更强 |
|---|---|---|---|
| session | window 容器 + detach 载体 | Desktop / 一组窗口 | —— |
| **window**(全屏切换的标签) | 单矩形只能显示一个 | **`WindowManager` 真 Window** | **我们**(真并排/浮动/z-order) |
| **pane**(window 内平铺) | 想同时看多个 | **`TerminalPaneGroup`**(单窗口内) | 平手 |

**因此下列 tmux 能力被原生能力覆盖、明确不做**:tmux window 层、`n/p` 切窗口(= `WindowManager::focus_next/previous`)、`c` 新窗口(= 新建终端窗口)、tmux 底部 status line 的窗口列表(= Windows 菜单 + 现有 StatusBar)。

**特色项(我们强于 tmux)**:pane ↔ window 互转。`break-pane`(pane→window)常用、值得给顺手触发(前缀键 / 拖 pane 标题拽出,借鉴浏览器拖标签);`join-pane`(window→pane)罕见且几乎全靠手动,放进命令模式/菜单即可,不占主快捷键。参照:tmux `break-pane`/`join-pane`、zellij float/embed、Emacs window/frame(`C-x 4`/`C-x 5`)、浏览器拖标签成窗口、VS Code "Move Editor into New Window"。

### 已落地的 terminal / pane 引擎

- 本地 tmux-like 分屏位于 `crates/atto-ui-terminal/src/pane.rs` 的 `TerminalPaneGroup`:二叉树布局、`%` 竖分、`"` 横分、`o` / Tab 线性切焦点、方向键几何选 pane、`Ctrl+方向键` 调整相邻分隔比例、`z` pane 级 zoom、`x` 关闭 pane 并重布局、`Ctrl+B` 前缀、鼠标点击聚焦。
- `TerminalHandle` 提供 `send_input_bytes`(=send-keys)、`snapshot`(=capture-pane)、`resize`、`is_running` / `exit_status`。
- `TerminalPaneGroupHandle` 提供 `panes()`(=list-panes)、`active_pane`、`pane_at_screen_position`、`split_pane`、方向性 `select_pane`、`break_pane`。
- `atto-ui-terminal::terminal_pane_ipc_handler` 把第 3 层 pane 协议方法映射到上述 handle 和原生 `WindowManager`。
- control mode(`-CC`)不在本路线内。若未来要"消费外部真 tmux",才需要在 reader→parser 之间增加控制帧拦截层;当前"伪装成 tmux"路线不需要它。

### 本地 pane 层状态

| 能力 | 当前状态 |
|---|---|
| 方向性 pane 导航(`prefix+方向键`) | 已实现,基于 pane 几何邻居选择。 |
| pane resize(`prefix+Ctrl+方向键`) | 已实现,调整当前 pane 附近的 split 比例并夹紧有效尺寸。 |
| pane zoom(`prefix+z`) | 已实现,pane 级临时全屏 / 还原。 |
| pane 关闭(`prefix+x`) + 重布局 | 已实现,最后一个 pane 不会被关闭。 |
| copy-mode 搜索(`/`) | 不属于本阶段已落地范围。 |

---

## 实际落地顺序

1. **第 1 层 introspection** —— 公共 `find_by_tag`;可测组件标 `tag`;`DesktopInspector` 门面(可变句柄,非类型级只读)、tag 诊断、dirty change tracker;chat 示例逻辑测试改用读值断言。
2. **第 2 层 scriptable** —— 叶子组件 `apply_command`;进程内 `invoke` / `query` / `wait_for`;`PtyTestHost` scriptable helper。
3. **第 4 层 L0+L1** —— 环境变量注入 + tmux DCS/OSC passthrough,不依赖第 3 层。
4. **第 3 层 ipc** —— Unix socket + JSON-RPC 类协议 + `atto` CLI。
5. **第 4 层 L2/L3** —— pane IPC 方法 + client-side `tmux` shim + 本地 pane 方向导航 / resize / zoom / close。

## 最终关键决策(汇总)

| # | 决策 | 倾向 |
|---|---|---|
| A | 第 1 层寻址方案 | 复用字符串 `tag` + 公共 `find_by_tag`,约定可脚本组件必须标 tag |
| B | 第 1 层状态读取 | 以 `get_property` / `property_names`(Binding 反射)为主干,复杂状态按需补 |
| C | 第 3 层传输 | Unix domain socket,路径由 `ATTO_UI_SOCKET` 或显式配置指定 |
| D | 第 3 层协议 | 自定义 JSON-RPC 类协议,tmux 语法只是第 4 层翻译目标 |
| E | 第 4 层 tmux 子命令伪装 | 乙:client-side `tmux` shim;背后统一连第 3 层 |
| F | 第 4 层是否做 control mode | 不做(那是"消费外部 tmux"的另一条路,与"伪装成 tmux"目标相反) |
