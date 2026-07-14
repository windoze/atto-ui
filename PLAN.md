# 执行计划：脚本化 / Introspection 控制平面

本计划对应 [`SCRIPTING_LAYERS.md`](SCRIPTING_LAYERS.md)。目标是把一个稳定的**控制平面分四层依次叠加**地建起来：组件可寻址、状态可读（introspection）→ 语义动作 + 等待（scriptable）→ 暴露到进程外（ipc）→ 伪装成 tmux 让第三方程序驱动原生 UI（tmux adapter）。每层是上一层的地基，依赖方向单向、不回头。

上一阶段的「全功能多窗口终端 App」计划（M1-M7）已归档至 [`docs/archive/2026-07-12-terminal-app/`](docs/archive/2026-07-12-terminal-app/)。

## 动机

直接触发点：PTY 测试框架（`crates/atto-ui-test-host`）在测**逻辑与状态变化**时非常繁琐——靠屏幕文字反查坐标（`find_text_position`）、靠轮询屏幕字形猜状态（`wait_for_disclosure_text` + `sleep`）、断言只能断字符断不了值。见 `crates/atto-ui-chat/tests/pty_chat.rs` 开头的 helper。

**结论**：PTY 测试不该被取代，应回归「渲染正确性 / 端到端」主场；把被误塞进它的「逻辑 / 状态测试」用一条**语义操作 + 语义断言**的新路径接出来。这条新路径的通用形态就是一个控制平面——测试只是它的第一个、也是风险最低的消费者。

## 现状基线（2026-07-15）

摸底 `src/inspect.rs` + `src/runtime/` + `src/reactive/` 后确认，**第 1 层读取链基本齐备**，是本计划最重要的起点：

| 能力 | 现状 | 位置 |
|---|---|---|
| 稳定寻址键 | 字符串 `tag`（spec 层叫 `id`），贯穿组件/窗口/菜单 | `src/composable/component_tag.rs`、`src/runtime/spec.rs` |
| 状态读取 | `Component::get_property/set_property/property_names` 由 `#[derive(ComponentProperties)]` 自动填充，直连活 `Binding<T>` | `src/composable/component.rs:476-488` |
| 门面雏形 | `DesktopInspector`：`tree()`/`export_snapshot()`/`get_property`/`set_property`/`action`，内部按 `tag` 递归寻址活树 | `src/inspect.rs` |
| 动作抽象 | `apply_command(ComponentCommand)` + `ComponentCommand` | `src/composable/component.rs:488`、`src/component_api.rs:42` |

**关键缺口**（本计划要补的）：
1. 寻址实现分散：`inspect.rs` 自己写了递归 `component_find`（`inspect.rs:1147-1170`），`runtime/tree.rs` 有私有 `ViewPathIndex`，缺一个**公共 `find_by_tag`**。
2. `apply_command` **只有约 10 个容器/控件实现**（disclosure/tab_view/list/table/radio/typeahead/scroll_container/visibility/border/min_size_view）。**Checkbox / Button / TextBox / Slider 等关键叶子组件未实现**，外部触发只能退化成合成坐标鼠标事件。
3. 只能拉不能推：reactive 是 `DirtyFlag`/`DirtyObserver` 版本号拉模型（`src/reactive/dirty.rs`），第 2 层 `wait_for` 需要一个统一的进程内变更信号。
4. `DesktopInspector` 是**进程内**门面（持 `&mut Desktop`）；跨进程消费（PtyTestHost、外部 CLI）要等第 3 层 ipc。

## 范围

| 范围 | 说明 |
|---|---|
| 第 1 层 introspection | 公共 `find_by_tag`；`DesktopInspector` 收敛为第 1 层门面；tag 覆盖诊断；进程内「读值断言」测试范式落地并示范迁移一例 chat 逻辑测试。 |
| 第 2 层 scriptable | 补齐叶子组件 `apply_command`；进程内语义 API `invoke`/`query`/`wait_for`（按可序列化设计）；接入进程内测试。 |
| 第 4 层 L0+L1（甜点区，可提前） | spawn 时注入 `$TMUX`/`$TMUX_PANE`/`$TERM`；DCS `\033Ptmux;` passthrough 解包 → 复用现有 OSC 52 / arboard。**不依赖第 3 层。** |
| 第 3 层 ipc | Unix domain socket + 自定义 JSON-RPC 类协议，把第 2 层语义 API 暴露给外部进程；外部 CLI 客户端。 |
| 第 4 层 L2/L3 | tmux `send-keys`/`capture-pane`/pane 管理子命令映射；本地 pane 层体验补全（方向导航、resize、zoom、close）。 |

## 非范围

| 非范围 | 说明 |
|---|---|
| 用 introspection 取代 PTY 渲染测试 | 渲染 / 端到端仍走 PTY；只把「逻辑 / 状态」测试接到控制平面。 |
| tmux window 层 / `n/p` 切换 / `c` 新窗口 | 已被原生 `WindowManager` 覆盖，明确不做（见 `SCRIPTING_LAYERS.md`「概念映射」）。 |
| tmux control mode（`-CC`） | 那是「消费外部真 tmux」的另一条路，与「伪装成 tmux」目标相反，不做（决策 F）。 |
| 反射非 `Binding` 的任意业务态 | 第 1 层以 `Binding` 反射为主干，复杂状态按需补结构化描述，不追求全反射。 |

## 原则

| 原则 | 要求 |
|---|---|
| 单向依赖不回头 | 上层是下层的 consumer，不是重新实现；第 1 层不得依赖第 2/3/4 层。 |
| 第 1 层自身即有价值 | 做完即可独立交付：逻辑测试从「OCR 屏幕」变成「读值」，不必等上面三层。 |
| 语义而非坐标 | `invoke(target, action)` 是语义级动作，不是合成坐标鼠标事件；只有目标不支持该动作时才允许退回坐标注入。 |
| 可序列化优先 | 第 2 层命令/查询/事件都设计成可序列化的值，让第 3 层只是「加传输 + 序列化」，不重新设计语义。 |
| 甜点区先行 | 第 4 层 L0+L1 近乎免费、立刻见效，不依赖第 3 层，可与前两层并行。 |
| 小步可编译 | 每阶段结束必须 `fmt`/`clippy`/`test` 全绿，关键路径有测试覆盖。 |

## 阶段划分

落地顺序遵循 `SCRIPTING_LAYERS.md`「落地顺序建议」：第 1 层 → 第 2 层 →（第 4 层 L0+L1 可提前）→ 第 3 层 → 第 4 层 L2/L3。

### M1 - 第 1 层 introspection（地基）

把分散的寻址收敛成公共能力，兑现「逻辑测试改用读值断言」的独立价值。

| 产出 | 说明 |
|---|---|
| 公共 `find_by_tag` | 新增 `pub fn find_by_tag` / `find_by_tag_mut`（进程内、纯只读寻址），`inspect.rs` 的 `component_find`/`component_find_mut` 改为委托它。 |
| 门面收敛 | `DesktopInspector` 明确为第 1 层门面：`tree`/`export_snapshot`/`get_property`/`property_names` 复用公共寻址。 |
| tag 覆盖诊断 | 提供一个「列出可交互但未标 tag 的节点」的诊断辅助，支撑「可脚本组件必须显式标 tag」约定。 |
| 变更信号聚合 | 为第 2 层 `wait_for` 预留：基于 `DirtyObserver` 的进程内变更检测封装（拉模型即可，不强求 push）。 |
| 读值断言范式 | 落地进程内测试范式（构造 `Desktop` → `inspect()` → 读值断言），并**示范迁移一例** chat 里靠 OCR/字形推断状态的逻辑测试。 |

验收：`find_by_tag` 单测覆盖命中/未命中/嵌套；示范迁移的逻辑测试不再依赖屏幕字符反查，改为读 `Binding` 活值断言；全套验证通过。

### M2 - 第 2 层 scriptable（语义动作 + 查询 + 等待）

在第 1 层「读」之上加「触发」和「等待」。

| 产出 | 说明 |
|---|---|
| 叶子组件 `apply_command` | Checkbox（`Toggle`/`Click`）、Button（`Click`/`Submit`）、TextBox（`InputText`）、Slider（调值）落地 `apply_command`，与既有鼠标/键盘交互语义一致。 |
| 语义 API | 进程内 `invoke(target, action)` / `query(target, prop)` / `wait_for(predicate, timeout)`，`target` 支持 `Id`/`Focused`，全部按可序列化值设计。 |
| 退回策略 | 组件实现了 `apply_command` 就语义派发；未实现才允许退回坐标注入（保留 `inspect.rs` 现有兜底），并可观测走了哪条路径。 |
| 接入测试 | `wait_for` 替代 chat helper 的 `sleep` 轮询屏幕；再迁移一批逻辑测试作为回归。 |

验收：`invoke("checkbox-id", Toggle)` 直接翻转 `Binding<bool>` 而非合成点击；`wait_for` 能等到异步驱动的状态成立且超时可控；单测覆盖每个新叶子组件动作；全套验证通过。

### M3 - 第 4 层 L0+L1（tmux 甜点区，可提前 / 可与 M1-M2 并行）

近乎免费、立刻见效，**不依赖第 3 层**。

| 产出 | 说明 |
|---|---|
| L0 环境探测注入 | `spawn_command`（`crates/atto-ui-terminal/src/terminal.rs:2775`）注入 `$TMUX`（socket,pid,session）、`$TMUX_PANE`、`$TERM`，让 opencode/claude code/vim 插件探测到「在 tmux 里」。 |
| L1 DCS passthrough | 识别 `\033Ptmux;...\033\\` 包裹 → 拆开内层转义 → 走原生 OSC 52 剪贴板（复用 M4.6 的 `TerminalSystemClipboard`/arboard）与 OSC 9;4 进度。 |
| 降级 | 未探测到 / 未包裹时行为不变；passthrough 解析失败不崩、不误写系统剪贴板。 |

验收：PTY 覆盖子进程读到注入的 `$TMUX`/`$TMUX_PANE`；`\033Ptmux;\033]52;...\a\033\\` 包裹的 OSC 52 被解包并写入剪贴板后端；无包裹路径回归不变。

### M4 - 第 3 层 ipc（暴露到进程外）

传输 + 序列化，把第 2 层语义 API 暴露给外部进程。**依赖 M2 的可序列化 API 设计。**

| 产出 | 说明 |
|---|---|
| 传输 | Unix domain socket server（决策 C），socket 路径可由环境变量指定，为第 4 层的 `$TMUX` 指向铺路。 |
| 协议 | 自定义干净协议（JSON-RPC 类，决策 D）：`invoke`/`query`/`wait_for`/`tree` 请求-响应，命令/查询/结果复用第 2 层可序列化值。 |
| 集成 | UI 主循环侧的请求分发（线程安全地把外部请求交给持有 `Desktop` 的线程执行）。 |
| 外部客户端 | 一个最小 `atto` CLI（类 iTerm `it2`）连 socket 驱动 UI，作为第一个进程外消费者与端到端测试载体。 |

验收：外部进程经 socket 发 `query`/`invoke` 能读到/改变 UI 状态；协议往返序列化正确；CLI 端到端跑通；modal 等边界行为与进程内一致。

### M5 - 第 4 层 L2/L3（tmux 子命令 + 本地 pane 补全）

把 tmux 接口面翻译成第 3 层调用。**它是第 3 层之上的 client，不是新协议实现。**

| 产出 | 说明 |
|---|---|
| send-keys / capture-pane | 映射到 `TerminalHandle::send_input_bytes`（`terminal.rs:3443`）/ `snapshot`（`terminal.rs:3738`）。 |
| pane 管理命令 | `split-window`/`select-pane -LRUD`/`display-popup`/`list-panes`/`break-pane` 映射到 `TerminalPaneGroup`（`crates/atto-ui-terminal/src/pane.rs:203`）与原生 `WindowManager`。 |
| 伪装载体 | shim `tmux` 可执行文件（决策 E 倾向乙：可控、薄翻译层）拦截命令转第 3 层调用；`$PATH` 前置注入。 |
| 本地 pane 补全 | 方向性 pane 导航（`prefix+方向键`）、pane resize、pane zoom（`z`）、pane 关闭（`x`）——属 tmux-like 体验补全，与第 4 层伪装无关。 |

验收：shim 的 `tmux send-keys`/`capture-pane` 经第 3 层驱动原生 pane；`split-window`/`select-pane` 落到原生 pane 布局；本地 pane 方向导航/resize/zoom/close 有 PTY 覆盖。

## 依赖关系

| 阶段 | 依赖 |
|---|---|
| M1 | 无。是所有东西的地基。 |
| M2 | 依赖 M1 的公共寻址与门面；`wait_for` 依赖 M1 变更信号聚合。 |
| M3 | 与 M1/M2 并行，仅依赖终端组件已有的 spawn/OSC 能力，**不依赖第 3 层**。 |
| M4 | 依赖 M2 的可序列化语义 API。 |
| M5 | 依赖 M4 的 socket + 协议；pane 命令依赖既有 `TerminalPaneGroup`；本地 pane 补全独立。 |

建议顺序：M1 → M2 →（M3 可提前/并行）→ M4 → M5。

## 待拍板的关键决策（汇总，默认取 `SCRIPTING_LAYERS.md` 倾向）

| # | 决策 | 默认倾向 | 落地阶段 |
|---|---|---|---|
| A | 第 1 层寻址方案 | 复用字符串 `tag` + 提炼公共 `find_by_tag`，约定可脚本组件必须标 tag | M1 |
| B | 第 1 层状态读取 | 以 `get_property`（Binding 反射）为主干，复杂状态按需补 | M1 |
| C | 第 3 层传输 | Unix domain socket | M4 |
| D | 第 3 层协议 | 自定义干净协议（JSON-RPC 类），tmux 语法作为第 4 层翻译目标 | M4 |
| E | 第 4 层 tmux 子命令伪装 | 乙（shim `tmux` 可执行文件，薄翻译层） | M5 |
| F | 第 4 层是否做 control mode | 不做 | M5 |

## 验证

每阶段至少运行：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

第 1/2 层逻辑测试优先走**进程内**读值断言（直接构造 `Desktop` → `inspect()`），不经 PTY；渲染 / 端到端仍走 PTY 快照。第 4 层终端交互走 PTY 快照（`snapshot_terminal_app` / `snapshot_terminal_window_app` + `pty_terminal_*`）。
