# 终端组件缺口分析 (Terminal Gap Analysis)

面向目标：将 `crates/atto-ui-terminal` 的 `terminal_viewer` demo 扩展为**全功能多窗口终端 app**。

本文档从两层梳理最初现状与缺口：
- **组件层**：`TerminalEmulator` (`src/terminal.rs`, ~1300 行) —— vt100 模拟器芯。
- **外壳层**：`terminal_viewer.rs` (~280 行 demo) —— 多窗口/菜单外壳。

> 状态更新（2026-07-12）：本文列出的 P0-P3 缺口已按 `TODO.md` 的 M1-M7 任务闭合。本文继续保留为设计背景与验收索引；当前实现状态以 `TODO.md` 的 `[DONE]` 完成记录为准。

## 当前闭合状态

| 缺口组 | 状态 | 闭合里程碑 |
|---|---|---|
| P0 进程生命周期、死窗口、callbacks/OSC | 已闭合 | M1-M2 |
| P1 标题联动、选择复制、剪贴板、OSC 133/7、滚动分流、前缀键 | 已闭合 | M2-M5 |
| P2 分屏、会话管理、spawn 环境 | 已闭合 | M6 |
| P3 光标/keypad、配置模型与设置界面 | 已闭合 | M7 |

## 历史现状小结

组件层的“终端芯”在计划启动前已具备：PTY spawn、reader 线程、按键/鼠标 ANSI 编码、scrollback、DSR 光标查询响应、bracketed paste、capture/release 快捷键、宽字符渲染、鼠标协议转发。外壳层已具备菜单、new/close/minimize/maximize、窗口列表切换。

当时缺口集中在**进程生命周期闭环、OSC/标题回调、体验层（选择复制、分屏、会话管理）与配置面**；这些缺口现已闭合。

---

## P0 — 硬缺陷，外壳绕不过去（必须先做）

### P0.1 进程生命周期没有闭环 【组件层】
> 状态：已闭合（M1.1/M1.2/M1.R）。组件记录进程级 `ExitStatus`，发布 `on_exit`，并通过 `TerminalHandle::is_running()` / `exit_status()` 暴露运行状态。

- reader 线程读到 EOF 仅 `break` (`terminal.rs:461-467`)，`child` 从不 `try_wait()`。shell `exit` 后窗口变成一个**死画面**：无退出码、不触发回调、不自动关窗。
- `on_close` 目前只在 `Drop` 时触发 (`terminal.rs:777-783`)。进程已死但组件仍存活时，外壳无从感知。
- 缺 `is_running()` / `exit_status()` 查询接口供外壳轮询。

**改动点**：
- reader 线程 EOF 或 `child` 退出时，记录 `ExitStatus` 到 `TerminalShared`，并触发一个 `on_exit(status)` 回调（区别于析构期的 `on_close`）。
- `TerminalHandle` 暴露 `is_running()` / `exit_status()`。

### P0.2 死窗口不回收 【外壳层】
> 状态：已闭合（M2.1/M6.2）。外壳 tick 路径检测退出状态，显示退出提示并支持按 `R` 按原 session spec/cwd 重启。

- 承接 P0.1：shell 退出后 demo 没有任何回收逻辑（未接退出回调，tick 里也不轮询进程状态）。
- 全功能 app 必须做到：exit 即关窗，或原地显示 `[Process exited: code N — press R to restart]`。

**改动点**：
- 在 tick 回调或退出回调中检测进程退出，按策略关窗 / 显示退出提示。

### P0.3 窗口标题 / OSC 全丢弃 【组件层 + 外壳层】
> 状态：已闭合（M1.3/M2.2/M4.6）。parser callback 桥接 title/icon/bell/OSC 52，窗口标题与 Windows 菜单同步，剪贴板路径可观察并可接系统 clipboard。

- vt100 0.16 支持 `Parser::new_with_callbacks`，可接 `set_window_title` / `set_window_icon_name` / `audible_bell` / `copy_to_clipboard`。但 `TerminalEmulator::new()` 用的是裸 `Parser::new` (`terminal.rs:351`)，全部回调丢弃。
- 后果：shell/vim 设置的标题 (`OSC 0/2`) 拿不到 → 窗口标题永远是静态 "Terminal N"；响铃、OSC 52 剪贴板写入全部静默丢弃。

**改动点**：
- 组件层：改用 `new_with_callbacks`，把 title / bell / clipboard 事件桥接到 `TerminalShared`，通过 handle 或回调暴露。
- 外壳层：把标题同步到 `Window.title`（联动 P1.1）。

---

## P1 — 全功能终端的核心体验

### P1.1 窗口标题联动 【外壳层】
> 状态：已闭合（M2.2/M2.3）。OSC 0/2 标题会同步到 `Window.title`，Windows 菜单窗口列表使用最新标题。

- 即便组件暴露了标题（P0.3），demo 也需把它同步到 `Window.title`，并刷新 Windows 菜单里的窗口列表。

### P1.2 文本选择 / 复制 【组件层 + 外壳层】
> 状态：已闭合（M4.1-M4.4/M4.R）。统一 selection 状态机同时服务鼠标框选和 copy-mode，支持宽字符命中、选区高亮、文本提取、内部 copy buffer 与粘贴回子进程。

- 组件把鼠标 down 一律当作 recapture 或转发给子进程 (`terminal.rs:747-770`)，无法框选屏幕文本复制。
- CLAUDE.md 记录 chat 组件已有文本选择，可参考其实现。
- **前置设计**：见下方「escape 机制设计决策」——选择/复制必须先解决“捕获态下如何脱出去做本地选择”的通路问题。

**改动点**：
- 组件层：新增统一的 selection 状态机（选区高亮 + 命中测试 + 从 vt100 `screen` 提取选中文本），鼠标与键盘两条入口共享它。
- 组件层：鼠标 `Shift` 旁路框选 + 键盘 copy-mode（经 P1.6 的前缀 `前缀 + [` 进入，详见下方决策）。
- 外壳层：选择 → 剪贴板（首版组件内部 buffer，后续接系统剪贴板 / OSC 52）。

### escape 机制设计决策（P1.2 + P1.6 的共同前置）

**背景**：`TerminalEmulator` 是“捕获全部键鼠、编码成 ANSI 转发给子进程”的组件，与 screen/tmux 概念同构——都要在“转发一切”的前提下给复用器自己留命令/选择通道。当前只有二元 `capture` 开关 + `release_shortcut`（`Ctrl+Shift+Esc`，`terminal.rs:121`），这个 release 是“整体切断捕获”的粗开关，无法支撑选择/复制，也无法在 capture 态里精细取用外壳快捷键。

**四种「输入归属」机制的全景（本文档一路讨论收敛的结果，勿再混淆）**：

| 机制 | 性质 | 例子 | 时长 | 归属 |
|---|---|---|---|---|
| 外壳快捷键通路 | 前缀（one-shot） | `前缀 + F10` 菜单、`前缀 + w` 窗口模式 | 一下即还 | P1.6 |
| copy-mode / 选择 | 前缀进入的模态 | `前缀 + [` 进、`Esc`/`q` 出 | 持续到手动退出 | P1.2 |
| alt screen 滚动分流 | 自动（按信号） | 滚轮 | 每事件即时判定 | P1.5 |
| 鼠标本地框选 | 修饰键旁路 | `Shift+拖拽` | 单次拖拽 | P1.2 |

前三种“键盘类”统一收敛到**一个 tmux 式前缀键**（详见 P1.6）；鼠标框选走修饰键旁路，是独立的第四种。

**身份判断（决定为何采用前缀）**：我们不是原生 GUI terminal，而是“跑在宿主终端字节流里、又去托管别的 CLI app”的复用器，与 tmux/screen 同构。
- GUI terminal 不需要前缀，因为它是原生 OS app，拥有 CLI **看不到**的 collision-free 键空间（macOS `Cmd`、或作为宿主自己能干净区分的 `Ctrl+Shift`）。
- 我们**没有** collision-free 键空间：收到的只是宿主终端字节流，无 `Cmd`；`Ctrl+Shift+<字母>` 在传统宿主终端与 `Ctrl+<字母>` 无法区分（要靠 kitty 协议，不可靠）。→ 只能像 tmux 那样用一个前缀**造**出命名空间。

**技术约束（终端字节流限制）**：
- 裸 modifier 组合（只按住 `Ctrl+Alt` 不配实体键）**收不到**——终端输入是字节流，没有 modifier keydown/keyup 事件。→ 前缀/chord 必须是 **modifier + 实体键**。
- 传统宿主终端里**只有 plain `Ctrl+<字母>` 是可靠可区分的控制字节**；`Ctrl+Shift+X` / `Ctrl+Alt+X` 依赖 kitty keyboard protocol，不能当命脉。当前 app 只推了 `DISAMBIGUATE_ESCAPE_CODES`（`src/app/run.rs:137`）。→ 前缀键落在 plain `Ctrl+<字母>`（tmux `Ctrl+B` / screen `Ctrl+A` 同理）。

**鼠标本地框选（第四种，独立于前缀）**：
- 子进程开着鼠标报告时，`Shift+拖拽` = 本地框选、不按 = 转发子进程（xterm/iTerm2/kitty/alacritty 的事实标准）；子进程没开鼠标报告时，直接拖拽即本地框选（顺带修掉当前把点击浪费在 `capture_on_click` recapture 上的问题，`terminal.rs:749`）。

**selection 状态机**：鼠标框选与 `前缀 + [` 的 copy-mode 汇聚到**同一个 selection 状态机**，只是入口不同。copy-mode 键位：方向键与 hjkl 都支持、起选 `v`/`Space`、复制 `y`/`Enter`、`Esc`/`q` 取消（兼容 vi 派与方向键派）。

> 注：早期草案曾定 `Ctrl+Shift+C` 直达进 copy-mode，现已**废弃**——零散 chord 有自身碰撞风险，且 `Ctrl+Shift` 在传统终端不可靠；统一收进 P1.6 的前缀（`前缀 + [`）。

### P1.3 剪贴板打通 【组件层 + 外壳层】
> 状态：已闭合（M4.4/M4.6）。选择与 copy-mode 复制写入组件内部 buffer，可通过前缀粘贴回子进程；系统剪贴板后端与 OSC 52 路径已接入且可禁用/替换。

- 粘贴已支持 bracketed paste (`terminal.rs:719-737`)，但缺“复制出去”的路径（依赖 P1.2 的选择 + P0.3 的 clipboard 回调）。
- **分期落地**：
  - *首版*：选择 → 组件内部 copy buffer + 粘贴回子进程，先让选择/高亮/命中测试这套核心逻辑落地并过 PTY 测试，避免一次把 `arboard` 跨平台依赖 + OSC 52 桥接塞进同一个 PR。
  - *后续*：接系统剪贴板（`arboard`）与 OSC 52（依赖 P0.3 的 `copy_to_clipboard` 回调），可做成 OSC 52 优先、`arboard` 兜底。

### P1.4 语义提示符标记（shell integration / OSC 133 · OSC 7）
> 状态：已闭合（M5.1-M5.6/M5.R）。组件层感知 OSC 133/7 并暴露命令块，第 2 层呈现/交互和第 3 层可选 shell integration 注入均已实现且相互解耦。

**目标**：让终端能把 scrollback 精确切成「提示符 / 用户命令 / 命令输出」三段，从而支持命令级导航（跳到上/下一条命令）、整条命令输出的一键选择/复制、命令失败高亮、命令级退出码、以及会话继承 cwd。

事实标准是 **OSC 133** 语义提示符标记（FinalTerm 起源，iTerm2 / kitty / WezTerm / VS Code 终端 / ghostty 均支持），配套 **OSC 7** 报告工作目录：

| 序列 | 含义 |
|---|---|
| `OSC 133 ; A ST` | prompt start（提示符开始）|
| `OSC 133 ; B ST` | prompt end / command start（用户输入起点）|
| `OSC 133 ; C ST` | output start（命令开始执行、输出起点）|
| `OSC 133 ; D ; <exit> ST` | command finished（命令结束，带退出码）|
| `OSC 7 ; file://host/path ST` | 当前工作目录 |

**这个能力必须拆成三个互相独立、职责与依赖方向都不同的层。三层不要混在一起实现。**

#### 第 1 层：感知与信号 【组件层】
- 职责：`TerminalEmulator` 解析这些序列，转成结构化的语义事件 / 回调，供上层消费。**这一层只负责“认出来并暴露出去”，不决定怎么显示。**
- 可行性：**无需 fork vt100**。vt100 0.16 内建只处理 OSC 0/1/2/52，其余 OSC（含 133/7）原样透传给 `Callbacks::unhandled_osc(params)`（`callbacks.rs:66` / `perform.rs:237`），params 是分号切好的原始字节片，形如 `[b"133", b"A"]` / `[b"133", b"D", b"0"]` / `[b"7", b"file://..."]`。
- 改动点：与 P0.3 共用 `new_with_callbacks` 改造。在同一个 `Callbacks` 实现里多接 `unhandled_osc`，识别 `133`/`7` 前缀，推进一个小状态机，把命令块记入 `TerminalShared`：
  - `command_marks: Vec<CommandBlock>`，每块记 `{ prompt_start_row, command_start_row, output_start_row, end_row, exit_code, cwd }`（行号用 vt100 绝对行 / scrollback 坐标）。
  - `TerminalHandle` 暴露 `command_blocks()` / `last_exit_code()`；可选 `on_command_finished(status)` 回调。
- 协同：`D;<exit>` 提供**命令级**退出码（区别于 P0.1 的进程级退出码）；`OSC 7` 的 cwd 供 P2.2「新会话继承 cwd」。

#### 第 2 层：呈现与交互 【外壳层 + 组件层】
- 职责：拿到第 1 层的语义事件后**怎么用**。这一层纯属产品/体验决策，可独立演进、可完全不做而不影响第 1 层：
  - 视觉：命令块之间插分隔线、给输出区不同底色、失败命令（exit≠0）标红标记。
  - 交互：命令级导航（`Ctrl+↑/↓` 跳上/下一条命令）、把 P1.2 的选择粒度从「框选区域」升级到「选中整条命令输出」、右键“重跑这条命令 / 复制命令 / 复制输出”。
- 依赖：只依赖第 1 层暴露的 `command_blocks()`，不关心序列从哪来。

#### 第 3 层：谁产生这些序列 【外壳层 / 配置面】
- 职责：OSC 133/7 **不是终端自己产生的**，是 shell 在 prompt 里 echo 出来的（bash `PROMPT_COMMAND`、zsh `precmd`/`preexec`、fish 内建、starship 自带）。这一层决定标记从哪来：
  - *方案 A（零侵入）*：仅当用户 shell 已配置 integration 就用，没有则第 1/2 层自动降级（无标记 = 退回普通 scrollback）。
  - *方案 B（开箱即用）*：spawn 时（配合 P2.3 的 spawn 环境）主动注入一段 shell integration 脚本，代价是要按 shell 类型维护脚本。
- **关键性质：第 1、2 层对第 3 层是解耦的**——无论标记由谁注入，甚至完全没有，前两层都应正常工作（有标记则增强，无标记则降级），不得硬依赖注入成功。

**优先级**：整体作为 P1 的可选增强，依赖 P0.3 的 callbacks 改造。第 1 层可与 P0.3 顺带落地（改动小、零依赖）；第 2、3 层按体验需求排期，互不阻塞。

### P1.5 alt screen / 全屏应用的滚动分流 【组件层】
> 状态：已闭合（M4.5/M4.7/M4.R）。滚轮按 mouse reporting、alternate screen、主屏本地 scrollback 的三级树分流，copy-mode 内滚轮/方向键始终本地消费。

**问题**：当前滚动全建立在 `screen.set_scrollback(offset)` 上（`handle_scrollback_wheel`/`handle_scrollback_key`，`terminal.rs:499/521`）。但 vt100 的 alt grid scrollback 恒为 0（`grid::new(size, 0)`，`screen.rs:76`），进入 vim/less/htop/tmux 后往上滚 `set_scrollback` 永远返回 0、毫无反应。根因：alt screen 是临时全屏画布，本就无历史；用户滚轮在两种模式下期望的是**根本不同**的两件事：

| | 主屏 (normal) | 全屏 / alt screen 应用 |
|---|---|---|
| 用户滚轮想要 | 回看历史输出 (scrollback) | 让**应用内部**滚动（vim/less 翻页）|
| 正确行为 | 本地 `set_scrollback`（不碰子进程）| 把滚轮意图**转发给子进程** |

核心洞察：**全屏下的"滚动"不是终端的事，是应用的事**——vim 自己维护缓冲和视口，终端只需把"用户想上滚"的意图告诉它。

#### 关键设计：不检测"全屏"，用两个稳定信号分流

**"全屏"在字节流层面没有可靠检测信号**——主屏画布型 app（把 main screen 当画布、不用 alt screen）和普通 shell 输出在字节流上无法区分，没有任何序列宣告"我是全屏应用"。因此判据不是"检测全屏"，而是两个稳定信号的组合，**二者互补、缺一不可**：

```
滚轮事件进来:
  1. if mouse_protocol_mode() != None:     # app 明确请求鼠标 —— 最强信号
        → 转发 SGR 滚轮序列 (已有 encode_mouse_event, 64/65, terminal.rs:1266)
        # 覆盖: vim(开鼠标)、htop、fzf --height(主屏画布型!) 等
        # 不看用不用 alt screen —— 主屏画布型也能被这条抓到

  2. elif alternate_screen():              # 在 alt screen 但没开鼠标 —— 不可省!
        → alternate scroll: 翻译成 ↑/↓ (默认 ×3) 发给子进程
        # 覆盖: vim `set mouse=`(关鼠标)、less、man —— 高频场景

  3. else:                                 # 主屏 + 没开鼠标
        → 本地 set_scrollback (现有逻辑)
```

- **第 1 条（鼠标报告 `mouse_protocol_mode()`）**：抓"开了鼠标的 app"，且顺带覆盖 alt screen 检测抓不到的**主屏画布型**（fzf `--height` 等）。
- **第 2 条（`alternate_screen()`）不可省**：专门兜"没开鼠标但进了 alt screen"的一大类——**关鼠标的 vim、less、man 全靠它**。这是 xterm 的 **DECSET 1007 alternate scroll mode** 思路：alt screen + 应用没开鼠标时，把滚轮翻译成方向键发给子进程。

#### 真正无解的残余类 —— 靠 copy-mode 兜底
同时满足「主屏 + 没开鼠标 + 不用 alt screen」的 app（如 `less -X`、部分老式 pager、`dialog`）落进第 3 条，会滚到 app 画的陈旧帧——**字节流层面无解，所有终端都躺平**。兜底不靠猜，靠明确的人工入口：
- `前缀 + [` 进入的 copy-mode（见 P1.2 / P1.6）里，滚轮/方向键**永远是本地 scrollback 导航**，与子进程无关——这本身就是"我要看历史"的明确入口。
- 可选：一个"强制转发滚轮给 app"的配置开关（呼应 P3.2），用于极少数想吃滚轮却没开鼠标的 app。

#### 不要用的信号（会帮倒忙）
- `application_cursor()`（DECCKM）：像"全屏"信号，但很多 shell/readline 配置也会开它，拿它当判据会**误伤普通 shell 的 scrollback**。
- "屏幕被清空/写满"这类启发式：噪声太大，vim 与 `cat 大文件` 无法区分。

**改动点 / 归属**：纯组件层，与 P1.2 的鼠标处理重写是同一片代码，顺手一起做。当前滚轮转发被绑在 `capture` 态里、scrollback 又不分主屏/alt——需在滚轮分支前置这棵三级决策树。alternate scroll 的键位（方向键 ×3 / `Ctrl+U`·`Ctrl+D`）与默认开关可做成配置项（呼应 P3.2），倾向默认方向键 ×3（vim/less/emacs 都吃）。

### P1.6 捕获态下的外壳快捷键通路（tmux 式前缀键）【组件层 + 外壳层】
> 状态：已闭合（M3.1-M3.5/M3.R）。默认 `Ctrl+B` 前缀、可配置前缀键与命令表、typed `ComponentAction` 桥接、copy-mode 入口和字面前缀转义均已实现。

**问题**：capture 态下，终端组件把除 `release_shortcut` 外的**所有键编码转发给子进程并返回 `consumed`**（`terminal.rs:701-712`）。而 Desktop 的事件路由是「focused view 先吃 → WM → Desktop 全局快捷键」（`desktop.rs:664-680` 再到 `749`），全局快捷键排在**最后**。两者叠加的后果：**capture 态下 `F10` / `Ctrl+W` 等外壳快捷键被终端组件在第一步吞掉转发给 shell，永远到不了 Desktop——外壳操作在终端捕获时失效。** 且这是**双向**难题：既要让外壳能从终端里拿出快捷键，又要让子进程仍能收到它需要的 `F10` / `Ctrl+W`。

**方案取舍（GUI terminal 风格 vs tmux/screen 风格）**：
- *GUI terminal 风格*：用 CLI 用不到的键空间（`Cmd` / 宿主能区分的 `Ctrl+Shift`）直接抢，无需前缀。**不适用我们**——我们是 CLI app，拿不到 `Cmd`，`Ctrl+Shift` 在传统宿主终端不可靠（见「escape 机制设计决策」的技术约束）。
- *tmux/screen 风格*：自身就是跑在字节流里的 CLI 复用器，用一个**前缀键造命名空间**。**我们采用**——身份同构。

**最终方案——单一 tmux 式前缀键，统一承载所有键盘类外壳/模式命令**：
- **前缀键可配置，默认靠拢 tmux（`Ctrl+B`）**。理由：嵌套复用器撞前缀是 tmux 默认键唯一的真实代价，而“在本程序里再套一层 tmux”的概率很低，这个代价基本消解，故直接借用最广的肌肉记忆。（`Ctrl+A` 撞 readline 行首、`Ctrl+C` 是 SIGINT 太危险，均排除。）
- **前缀键必须是 plain `Ctrl+<字母>`**——传统宿主终端里唯一可靠可区分的控制字节。
- **前缀命令表**（借鉴 tmux）：
  - `前缀 + F10`（或某个键）→ 激活菜单
  - `前缀 + w` → 窗口管理模式 / 窗口列表
  - `前缀 + z` → 最大化/还原当前终端窗口
  - `前缀 + [` → 进入 copy-mode（P1.2 的选择/复制入口）
  - `前缀 + 前缀` → **前缀转义**：把一个字面前缀键发给子进程（tmux `Ctrl+B Ctrl+B`），解决“子进程也需要这个键”的反向需求
- **这一个前缀顺带消解了双向 escape**：默认全部键透传子进程、我们只偷前缀这一个键 → 子进程要 `F10`/`Ctrl+W` 都拿得到（甚至前缀本身也能靠 `前缀+前缀` 转义），无需额外的“反向 escape”机制。

**改动点**：
- 组件层：capture 分支引入「前缀态」状态机——收到前缀键则进入前缀态（不转发），下一个键查前缀命令表；命中外壳命令则通过 typed `ComponentAction` 交给 Desktop 处理（比 raw-key 冒泡更适合 `前缀+w/z` 这类非全局原始键），命中 `[` 则进 copy-mode，`前缀+前缀` 则转发一个字面前缀，未命中则（可选）连同前缀一起发给子进程或吞掉。前缀键、命令表可配置。
- 外壳层：Desktop 侧的 `F10`/`Ctrl+W` 等入口保持不变；对非终端窗口（editor/file-tree 不转发子进程、无碰撞）这些键仍直达，**只有终端 capture 态需要走前缀**——这点轻微不一致是 tmux 用户早已接受的，且 capture 释放后 `F10` 照常直达。

**四种输入归属机制的关系**见「escape 机制设计决策」节的全景表：P1.6 前缀承载「外壳快捷键 / copy-mode」两类键盘命令，P1.5 的滚轮分流与 P1.2 的鼠标 `Shift` 框选是独立的鼠标类机制。

---

## P2 — 多形态窗口与会话管理

### P2.1 分屏 / 标签页 【外壳层】
> 状态：已闭合（M6.1/M6.R）。`TerminalPaneGroup` 在单个 WM 窗口内维护 pane 树、active pane、split layout、pane-level focus/capture 和 resize 传播。

- “全功能多窗口终端”通常要 tmux 式 split panes 或 tab，目前只有 WM 浮动窗口一种形态。
- 可基于现有 `VStack`/`HStack`/`Grid` 布局在单窗口内做 split。

### P2.2 会话管理 【外壳层】
> 状态：已闭合（M6.2/M6.4/M6.R）。窗口持有独立 `TerminalSessionSpec`，支持 shell/command 新建入口、死会话重启、OSC 7 cwd 继承和每窗口 profile/cwd。

- 无“新建时选 shell/命令”入口。
- 无重启已死会话（配合 P0.1 的 `exit_status`）。
- 无每窗口独立 cwd / profile。

### P2.3 spawn 环境细节 【组件层】
> 状态：已闭合（M6.3/M6.R）。spawn 设置 `TERM=xterm-256color`、`COLORTERM=truecolor`、显式 cwd，并提供主动 resize API 与清理路径。

- `spawn_command` 未设 `TERM` / `COLORTERM`，无初始 `cwd` (`terminal.rs:435-489`)。
- resize 仅在 `draw` 中被动触发 (`terminal.rs:566-568`)，可考虑显式 resize 接口。

---

## P3 — 渲染保真度与配置面

### P3.1 光标形状 / keypad 模式 【组件层】
> 状态：已闭合（M7.1/M7.2）。DECSCUSR block/underline/bar 光标渲染与 `application_keypad()` 输入编码均已接入。

- 光标渲染是 REVERSED 涂格 (`terminal.rs:603-612`)，忽略 vt100 的光标形状（block/bar/underline）。
- DECCKM (`application_cursor`) 已接，但 `application_keypad()` 未接。

### P3.2 配置入口 【外壳层】
> 状态：已闭合（M7.3-M7.6/M7.R）。`TerminalConfig`、JSON/YAML 持久化、设置窗口、live apply/save/reload，以及 scrollback/prefix/palette/release/alt-scroll/session/shell-integration/cursor 配置均已接线。

- scrollback 长度、色板、release 快捷键均写死，无设置入口。

---

## 落地顺序建议

> 当前状态：以下建议已按 M1 → M7 全部落地，详细完成记录见 `TODO.md`。

1. **P0.1 + P0.3（组件层）** —— 一次性引入 `new_with_callbacks`，同时补进程退出回调与 title/bell/clipboard 桥接。这是组件层的硬缺陷，后续一切依赖它。
2. **P0.2 + P1.1（外壳层）** —— 死窗口回收 + 标题联动，让多窗口外壳“活”起来。
3. **P1.6 前缀键框架**（组件层）—— 先落地 tmux 式前缀键的解析（前缀态状态机 + 可配置前缀 + 前缀转义），因为 P1.2 的 copy-mode、P1.6 的外壳快捷键通路都挂在它上面。
4. **P1.2 + P1.3 + P1.5** —— 选择复制 + 剪贴板打通 + alt screen 滚动分流。三者共用同一片鼠标/键盘处理代码，一起重写最省。selection 状态机（鼠标 `Shift` 旁路 + `前缀 + [` 进 copy-mode），复制目标首版走组件内部 buffer；滚轮按 P1.5 的三级决策树分流（鼠标报告 → alt screen → 本地 scrollback）。
5. **P1.4 第 1 层** —— 语义提示符标记的「感知与信号」可与 P0.3 的 callbacks 改造顺带落地（改动小、零依赖）；第 2 层（呈现/交互）与第 3 层（shell integration 注入）解耦，按体验需求另行排期，互不阻塞。
6. **P2 / P3** —— 分屏、会话管理、渲染保真、配置面，按需推进。
