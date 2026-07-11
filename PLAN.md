# 执行计划：全功能多窗口终端 App

本计划对应 [`TERMINAL_GAP.md`](TERMINAL_GAP.md)。目标是把 `crates/atto-ui-terminal` 的 `terminal_viewer` demo 从「能跑一个 shell 的多窗口外壳」扩展为**全功能多窗口终端 app**：进程生命周期闭环、tmux 式前缀键、文本选择/复制、alt screen 滚动分流、语义提示符标记、分屏/会话管理，以及一个完整的**配置界面**。

上一阶段的 TUI Agent / DeepSeek 接入计划已归档至 [`docs/archive/2026-07-11-tui-agent-deepseek/`](docs/archive/2026-07-11-tui-agent-deepseek/)。

## 现状（2026-07-11）

组件层的「终端芯」`TerminalEmulator`（`crates/atto-ui-terminal/src/terminal.rs`，~1300 行）已相当扎实：PTY spawn、reader 线程、按键/鼠标 ANSI 编码、scrollback、DSR 光标查询响应、bracketed paste、capture/release 快捷键、宽字符渲染、鼠标协议转发均已就绪。外壳层 `examples/terminal_viewer.rs`（~280 行 demo）已具备菜单、new/close/minimize/maximize、窗口列表切换。

缺口集中在**进程生命周期闭环、OSC/标题回调、体验层（选择复制、分屏、会话管理）与配置面**。详见 `TERMINAL_GAP.md` 的 P0-P3 分级。

## 范围

| 范围 | 说明 |
|---|---|
| 组件层增强 | `TerminalEmulator` 增补进程退出信号、`new_with_callbacks`（title/bell/clipboard/OSC）、selection 状态机、alt screen 滚动分流、tmux 式前缀键状态机、语义提示符标记感知、光标形状/keypad。 |
| 外壳层增强 | `terminal_viewer` 死窗口回收、标题联动、选择→剪贴板、分屏/标签页、会话管理、语义标记呈现/交互。 |
| 配置界面 | 新增可视化设置界面（scrollback、色板、前缀键、release 快捷键、滚动分流键位、shell/命令、cwd/profile 等），配套配置模型与持久化。 |
| shell integration | OSC 133/7 的可选注入脚本（第 3 层），零侵入降级为默认。 |

## 非范围

| 非范围 | 说明 |
|---|---|
| fork vt100 | OSC 133/7 走 `Callbacks::unhandled_osc` 透传，无需 fork。 |
| 系统级沙箱 | 终端本就托管任意子进程，不承诺隔离。 |
| GUI 原生键空间 | 不依赖 `Cmd` / 可靠 `Ctrl+Shift`；键盘类命令统一走 tmux 式前缀。 |
| 远程/SSH 会话托管 | 本计划只做本地 PTY 会话，远程 profile 后续单独排期。 |

## 原则

| 原则 | 要求 |
|---|---|
| 组件/外壳分层 | 「认出来并暴露」属组件层，「怎么用/怎么显示」属外壳层；两层解耦，语义标记的第 1 层不得硬依赖第 2/3 层。 |
| 前缀造命名空间 | 我们是跑在宿主字节流里的复用器，无 collision-free 键空间；键盘类外壳/模式命令统一收敛到**一个可配置的 plain `Ctrl+<字母>` 前缀**（默认 `Ctrl+B`）。 |
| 信号分流而非猜测 | alt screen 滚动分流用 `mouse_protocol_mode()` + `alternate_screen()` 两个稳定信号，不用 `application_cursor()` 或清屏启发式。 |
| 降级不崩 | 有语义标记则增强，无标记则退回普通 scrollback；shell integration 注入失败不影响前两层。 |
| 小步可编译 | 每阶段结束必须能 build/test/clippy/fmt，PTY 覆盖关键交互。 |
| 配置可回归 | 配置项默认值不变行为；配置界面改动通过 PTY 快照验证。 |

## 阶段划分

落地顺序遵循 `TERMINAL_GAP.md`「落地顺序建议」。

### M1 - 进程生命周期 + Callbacks 基础（P0.1 + P0.3 组件层）

一次性引入 `new_with_callbacks`，同时补进程退出回调与 title/bell/clipboard 桥接。这是组件层的硬缺陷，后续一切依赖它。

| 产出 | 说明 |
|---|---|
| 进程退出信号 | reader 线程 EOF 或 `child` 退出时 `try_wait()` 记录 `ExitStatus` 到 `TerminalShared`，触发 `on_exit(status)` 回调（区别于析构期 `on_close`）。 |
| 查询接口 | `TerminalHandle` 暴露 `is_running()` / `exit_status()`。 |
| callbacks 改造 | `TerminalEmulator::new` 改用 `Parser::new_with_callbacks`，桥接 `set_window_title` / `set_window_icon_name` / `audible_bell` / `copy_to_clipboard` 到 `TerminalShared`，经 handle/回调暴露。 |

验收：单测/PTY 覆盖 shell `exit` 后组件报告退出码、`is_running()` 翻转；title/bell/clipboard 回调可被外壳观察到。

### M2 - 死窗口回收 + 标题联动（P0.2 + P1.1 外壳层）

让多窗口外壳「活」起来。

| 产出 | 说明 |
|---|---|
| 死窗口回收 | tick 或 `on_exit` 检测进程退出，按策略关窗，或原地显示 `[Process exited: code N — press R to restart]`。 |
| 标题联动 | 把组件暴露的标题同步到 `Window.title`，刷新 Windows 菜单窗口列表。 |

验收：PTY 覆盖 shell 退出后窗口回收/退出提示；shell/vim 设置 `OSC 0/2` 标题后窗口标题与菜单同步更新。

### M3 - tmux 式前缀键框架（P1.6 组件层 + 外壳层）

先落地前缀键解析，因为 copy-mode（P1.2 入口）与外壳快捷键通路都挂在它上面。

| 产出 | 说明 |
|---|---|
| 前缀态状态机 | capture 分支收到前缀键进入前缀态（不转发），下一个键查前缀命令表。 |
| 可配置前缀 | 默认 `Ctrl+B`，必须是 plain `Ctrl+<字母>`；前缀键与命令表可配置。 |
| 前缀命令表 | `前缀 + F10` 激活菜单、`前缀 + w` 窗口模式、`前缀 + z` 最大化/还原、`前缀 + [` 进 copy-mode、`前缀 + 前缀` 转义一个字面前缀给子进程。 |
| 事件派发 | 命中外壳命令通过 typed `ComponentAction` 交给 Desktop 处理（比 raw-key 冒泡更适合 `前缀+w/z` 这类非全局原始键），命中 `[` 进 copy-mode。 |

验收：PTY 覆盖 capture 态下 `前缀 + F10` 能激活菜单、`前缀 + w` 进窗口模式、`前缀 + 前缀` 把字面前缀发给子进程；非终端窗口快捷键仍直达。

### M4 - 选择复制 + 剪贴板 + alt screen 滚动分流（P1.2 + P1.3 + P1.5）

三者共用同一片鼠标/键盘处理代码，一起重写最省。

| 产出 | 说明 |
|---|---|
| selection 状态机 | 选区高亮 + 命中测试 + 从 vt100 `screen` 提取选中文本；鼠标与键盘两条入口共享。 |
| 鼠标本地框选 | 子进程开鼠标报告时 `Shift+拖拽`=本地框选、不按=转发；未开鼠标报告时直接拖拽即框选（修掉 `capture_on_click` recapture 浪费点击）。 |
| copy-mode | 经 `前缀 + [` 进入；方向键与 hjkl、起选 `v`/`Space`、复制 `y`/`Enter`、`Esc`/`q` 取消；滚轮/方向键永远本地 scrollback 导航。 |
| 剪贴板（首版） | 选择 → 组件内部 copy buffer + 粘贴回子进程。 |
| 剪贴板（后续） | 接系统剪贴板（`arboard`）与 OSC 52（依赖 M1 clipboard 回调），OSC 52 优先、`arboard` 兜底。 |
| alt screen 滚动分流 | 滚轮前置三级决策树：`mouse_protocol_mode() != None` → 转发 SGR 滚轮；`alternate_screen()` → alternate scroll 翻方向键（默认 ×3）发子进程；else → 本地 `set_scrollback`。 |

验收：PTY 覆盖鼠标框选/复制、`前缀 + [` copy-mode 选择复制、vim(开/关鼠标)/less/htop/fzf 滚轮各自落到正确分支、主屏 scrollback 仍正常。

### M5 - 语义提示符标记（P1.4，OSC 133/7）

三层互相独立、职责与依赖方向不同，不混在一起实现。

| 产出 | 说明 |
|---|---|
| 第 1 层 感知与信号【组件层】 | 与 M1 共用 callbacks，接 `unhandled_osc` 识别 `133`/`7`，推进小状态机记 `command_marks: Vec<CommandBlock>`（prompt/command/output/end 行号、exit_code、cwd）；`TerminalHandle` 暴露 `command_blocks()` / `last_exit_code()`，可选 `on_command_finished`。 |
| 第 2 层 呈现与交互【外壳层】 | 命令块分隔线/底色、失败命令标红、命令级导航（`Ctrl+↑/↓`）、选择粒度升级到整条命令输出、右键重跑/复制命令/复制输出。可独立演进、可不做。 |
| 第 3 层 shell integration【配置面】 | 方案 A 零侵入（用户已配则用，未配降级）；方案 B spawn 时按 shell 类型注入 integration 脚本。第 1/2 层不得硬依赖注入成功。 |

验收：单测覆盖 OSC 133/7 解析与 `command_blocks()` 状态机；无标记时退回普通 scrollback 不崩；第 2 层导航/命令级复制、第 3 层注入按体验需求排期，互不阻塞。

### M6 - 分屏/标签页 + 会话管理 + spawn 环境（P2）

| 产出 | 说明 |
|---|---|
| 分屏 | 基于现有组件和功能在单窗口内做 tmux 式 split panes。 |
| 会话管理 | 新建时选 shell/命令入口；重启已死会话（配合 M1 `exit_status`）；每窗口独立 cwd/profile（cwd 可继承 M5 OSC 7）。 |
| spawn 环境 | `spawn_command` 设 `TERM` / `COLORTERM`、初始 `cwd`；提供显式 resize 接口（不再仅在 `draw` 被动触发）。 |

验收：PTY 覆盖单窗口内分屏布局、死会话重启、新建时选择 shell/命令并落到指定 cwd。

### M7 - 渲染保真度 + 配置界面（P3.1 + P3.2）

**本阶段包含用户明确要求的「配置界面」**，作为整个终端 app 的设置面板。

| 产出 | 说明 |
|---|---|
| 光标形状 | 光标渲染读取 vt100 光标形状（block/bar/underline），不再一律 REVERSED 涂格。 |
| keypad 模式 | 接 `application_keypad()`（DECCKM `application_cursor` 已接）。 |
| 配置模型 | 集中式 `TerminalConfig`：scrollback 长度、色板、前缀键、release 快捷键、alt screen 滚动键位与开关、shell/命令、cwd/profile、shell integration 注入开关、光标形状默认值等；配套默认值与持久化（沿用项目 JSON/YAML 主题配置风格）。 |
| 配置界面 | 新增可视化**设置窗口**（复用声明式 `VStack`/`HStack`/`Grid` + 现有 widgets：`TextBox`/`Checkbox`/`RadioGroup`/`ListBox`），分组编辑上述配置项，支持即时预览/应用与保存；从菜单入口打开。 |
| 配置生效 | 组件层各写死项（scrollback、色板、release 快捷键、前缀键、滚动键位）改读 `TerminalConfig`，配置界面改动运行时生效。 |

验收：PTY 覆盖打开配置界面、修改 scrollback/前缀键/色板等并应用后行为随之改变、保存后重启保留；光标形状随 vt100 序列切换正确渲染。

## 依赖关系

| 阶段 | 依赖 |
|---|---|
| M1 | 无，组件层硬缺陷先做。 |
| M2 | 依赖 M1 的 `on_exit` / `is_running` / 标题回调。 |
| M3 | 依赖 M1（capture 路径稳定），是 M4 copy-mode 的前置。 |
| M4 | 依赖 M3 前缀键（copy-mode 入口）与 M1 clipboard 回调（后续剪贴板）。 |
| M5 | 第 1 层依赖 M1 的 `new_with_callbacks`；第 2 层依赖第 1 层的 `command_blocks()`；第 3 层依赖 M6 的 spawn 环境（方案 B）。 |
| M6 | 分屏独立；死会话重启依赖 M1 `exit_status`；cwd 继承依赖 M5 OSC 7。 |
| M7 | 配置模型贯穿全程；配置界面依赖各阶段已暴露的可配置项与 M6 的 shell/profile。 |

建议顺序：M1 → M2 → M3 → M4 → M5（第 1 层可与 M1 顺带） → M6 → M7。M5 第 2/3 层、M6、M7 可按体验需求灵活穿插，互不阻塞。

## 验证

每阶段至少运行：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

终端交互优先走 PTY 快照（`snapshot_terminal_app` / `snapshot_terminal_window_app` + `pty_terminal_*` 测试），不依赖真实交互终端。手动验证用 `cargo run -p atto-ui-terminal --example terminal_viewer`。
