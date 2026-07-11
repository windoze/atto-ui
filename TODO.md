# TODO：全功能多窗口终端 App

执行计划见 [`PLAN.md`](PLAN.md)，缺口分析见 [`TERMINAL_GAP.md`](TERMINAL_GAP.md)。

上一阶段 TUI Agent / DeepSeek 计划已归档至 [`docs/archive/2026-07-11-tui-agent-deepseek/`](docs/archive/2026-07-11-tui-agent-deepseek/)。

通用验收：每个阶段完成后至少运行 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。终端交互优先走 PTY 快照（`snapshot_terminal_app` / `snapshot_terminal_window_app` + `pty_terminal_*`），不依赖真实交互终端。

代码位置速查：组件层 `crates/atto-ui-terminal/src/terminal.rs`（~1300 行），外壳层 demo `crates/atto-ui-terminal/examples/terminal_viewer.rs`，PTY fixture `src/bin/snapshot_terminal_app.rs` / `snapshot_terminal_window_app.rs`。

## 阶段 M1 - 进程生命周期 + Callbacks 基础（P0.1 + P0.3 组件层）

- [x] **[DONE] M1.1 进程退出信号** - reader 线程 EOF 或 `child` 退出时 `try_wait()` 记录 `ExitStatus` 到 `TerminalShared`，触发 `on_exit(status)` 回调（区别于析构期 `on_close`，`terminal.rs:461-467` / `terminal.rs:777-783`）。
  - 完成记录（2026-07-11）：在 `TerminalShared` 中记录进程 `ExitStatus`，新增独立于 `on_close` 的 `on_exit(status)` 回调；reader EOF、draw-time `try_wait()` 轮询和生命周期 watcher 均可幂等发布退出状态。新增 `process_exit` 集成测试覆盖退出码回调只触发一次，以及无子进程 drop 时只触发 `on_close`。
  - 验证：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M1.2 运行状态查询接口** - `TerminalHandle` 暴露 `is_running()` / `exit_status()` 供外壳轮询。
  - 完成记录（2026-07-11）：在 `TerminalShared` 中维护 subprocess running 状态，`TerminalHandle::is_running()` 可轮询当前子进程是否仍存活，`TerminalHandle::exit_status()` 可读取最近一次记录的 `ExitStatus`。新进程启动会清空旧退出状态并标记运行中，进程自然退出时复用 M1.1 的退出记录路径翻转为非运行，显式 `stop_process()` 也会清除运行态。
  - 验证：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M1.3 new_with_callbacks 改造** - `TerminalEmulator::new` 改用 `Parser::new_with_callbacks`（替换 `terminal.rs:351` 的裸 `Parser::new`），桥接 `set_window_title` / `set_window_icon_name` / `audible_bell` / `copy_to_clipboard` 到 `TerminalShared`，经 handle/回调暴露。
  - 完成记录（2026-07-11）：`TerminalEmulator` 的 parser 初始化与 scrollback 重建路径均改为 `vt100::Parser::new_with_callbacks`，新增 callback bridge 捕获 OSC 0/1/2 标题与图标名、BEL audible bell、OSC 52 clipboard copy 请求，写入 `TerminalShared` 并通过 `TerminalHandle` 查询接口与注册回调暴露。新增 `TerminalClipboardCopy` 公共事件类型与 `callbacks` 测试覆盖 handle 状态和回调可观察性。
  - 验证：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M1.4 测试** - 单测/PTY 覆盖 shell `exit` 后报告退出码、`is_running()` 翻转、`on_exit` 触发；title/bell/clipboard 回调可被观察到。
  - 完成记录（2026-07-11）：补强 `callbacks` 集成测试，新增真实 `/bin/sh` 子进程输出 OSC 2 title、BEL、OSC 52 clipboard 并以指定退出码结束的回归用例；与既有 `process_exit` 测试共同覆盖退出码报告、`is_running()` 翻转、`on_exit` 触发一次、title/bell/clipboard 回调与 handle 状态可观察。
  - 验证：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo test -p atto-ui-terminal --test callbacks --test process_exit`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M1.R Review** - 复核退出信号与 callbacks 桥接无 unsafe、不破坏既有 capture/paste/scrollback 路径，全套验证通过。
  - 完成记录（2026-07-11）：复核 `atto-ui-terminal` 生命周期与 callback 桥接实现，确认 crate 继续 `#![forbid(unsafe_code)]` 且无 unsafe 使用；退出状态记录幂等、`on_exit` 与 `on_close` 解耦，callback 事件在锁外派发；`new_with_callbacks` 覆盖默认初始化与 scrollback parser 重建路径，未破坏 capture、bracketed paste、mouse forwarding 或本地 scrollback 行为。
  - 验证：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。

## 阶段 M2 - 死窗口回收 + 标题联动（P0.2 + P1.1 外壳层）

- [x] **[DONE] M2.1 死窗口回收** - 在 tick 或 `on_exit` 检测进程退出，按策略关窗，或原地显示 `[Process exited: code N — press R to restart]`。
  - 完成记录（2026-07-11）：终端 viewer 与 PTY window fixture 增加外壳层 session tracking；tick 轮询发现子进程退出后会释放 terminal capture、原地写入 `[Process exited: code N — press R to restart]` 提示，并在 focused dead terminal 上按 plain `R` 时替换为新的 `TerminalEmulator` 并按原命令重启。fixture 支持通过参数启动真实子进程并暴露 `PROC`/`RESTARTS` 状态，新增 PTY 回归覆盖退出提示与 R 重启入口。
  - 验证：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions -- pty_terminal_dead_process_prompts_and_restarts --nocapture`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M2.2 标题联动** - 把组件暴露的标题（M1.3）同步到 `Window.title`，刷新 Windows 菜单窗口列表。
  - 完成记录（2026-07-11）：终端 viewer 与 PTY window fixture 在 UI tick/action 路径轮询 `TerminalHandle::window_title()`，将 OSC 0/2 暴露的标题同步到对应 `Window.title`，并在刷新 Windows 菜单窗口列表前使用当前窗口标题作为菜单项来源。终端重启时恢复默认窗口标题，避免旧进程标题滞留。新增 PTY 回归覆盖子进程输出 OSC 2 后标题栏更新，并在 Windows → Switch to 菜单中显示更新后的窗口标题。
  - 验证：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions -- pty_terminal_osc_title_updates_window_title_and_windows_menu --nocapture`、`cargo test --workspace --all-targets` 均通过。
- [ ] **M2.3 测试** - PTY 覆盖 shell 退出后窗口回收/退出提示与 R 重启入口；`OSC 0/2` 标题联动到窗口标题与菜单。
- [ ] **M2.R Review** - 复核回收策略不误杀存活窗口、标题联动线程安全，验证通过。

## 阶段 M3 - tmux 式前缀键框架（P1.6 组件层 + 外壳层）

- [ ] **M3.1 前缀态状态机** - capture 分支（`terminal.rs:701-712`）收到前缀键进入前缀态（不转发），下一个键查前缀命令表；未命中按策略连同前缀发子进程或吞掉。
- [ ] **M3.2 可配置前缀键** - 默认 `Ctrl+B`，约束为 plain `Ctrl+<字母>`；前缀键可配置（暂用组件字段，M7 接入配置模型）。
- [ ] **M3.3 前缀命令表** - `前缀 + F10` 激活菜单、`前缀 + w` 窗口模式、`前缀 + z` 最大化/还原、`前缀 + [` 进 copy-mode（占位，M4 实现选择）、`前缀 + 前缀` 转义一个字面前缀给子进程。
- [ ] **M3.4 事件冒泡** - 命中外壳命令 `return EventResult::ignored()` 冒泡给 Desktop（`desktop.rs:664-680` / `749`），使 capture 态下外壳快捷键可达。
- [ ] **M3.5 测试** - PTY 覆盖 capture 态 `前缀 + F10` 激活菜单、`前缀 + w` 窗口模式、`前缀 + 前缀` 字面前缀到子进程；非终端窗口快捷键仍直达、capture 释放后 `F10` 直达。
- [ ] **M3.R Review** - 复核前缀不吞掉子进程需要的键（可靠转义）、命令表可配置、双向 escape 收敛为单一前缀，验证通过。

## 阶段 M4 - 选择复制 + 剪贴板 + alt screen 滚动分流（P1.2 + P1.3 + P1.5）

- [ ] **M4.1 selection 状态机** - 新增统一 selection 状态机：选区高亮 + 命中测试 + 从 vt100 `screen` 提取选中文本；鼠标与键盘两条入口共享。可参考 chat 组件已有文本选择实现。
- [ ] **M4.2 鼠标本地框选** - 子进程开鼠标报告时 `Shift+拖拽`=本地框选、不按=转发；未开鼠标报告时直接拖拽即框选（修掉 `capture_on_click` recapture 浪费点击，`terminal.rs:747-770`）。
- [ ] **M4.3 copy-mode** - 经 `前缀 + [`（M3.3）进入；方向键与 hjkl、起选 `v`/`Space`、复制 `y`/`Enter`、`Esc`/`q` 取消；copy-mode 内滚轮/方向键永远本地 scrollback 导航。
- [ ] **M4.4 剪贴板（首版）** - 选择 → 组件内部 copy buffer + 粘贴回子进程（bracketed paste 已支持，`terminal.rs:719-737`），先让选择/高亮/命中测试核心逻辑落地过 PTY。
- [ ] **M4.5 alt screen 滚动分流** - 滚轮分支前置三级决策树：`mouse_protocol_mode() != None` → 转发 SGR 滚轮（已有 `encode_mouse_event` 64/65，`terminal.rs:1266`）；`alternate_screen()` → alternate scroll 翻方向键（默认 ×3）发子进程；else → 本地 `set_scrollback`（现有逻辑，`terminal.rs:499/521`）。不用 `application_cursor()`/清屏启发式。
- [ ] **M4.6 剪贴板（后续，可选）** - 接系统剪贴板（`arboard`）与 OSC 52（依赖 M1.3 `copy_to_clipboard` 回调），OSC 52 优先、`arboard` 兜底。可拆独立 PR。
- [ ] **M4.7 测试** - PTY 覆盖鼠标框选/复制、copy-mode 选择复制、vim(开/关鼠标)/less/htop/fzf `--height` 滚轮各落正确分支、主屏 scrollback 仍正常。
- [ ] **M4.R Review** - 复核 selection 命中测试对宽字符正确、滚动分流三级树覆盖残余类靠 copy-mode 兜底、剪贴板首版不引入跨平台依赖，验证通过。

## 阶段 M5 - 语义提示符标记（P1.4，OSC 133/7）

三层互相独立，不混在一起实现。

- [ ] **M5.1 第 1 层 感知与信号【组件层】** - 与 M1.3 共用 callbacks，接 `unhandled_osc`（vt100 透传 `[b"133", b"A"]` / `[b"133", b"D", b"0"]` / `[b"7", b"file://..."]`），推进小状态机记 `command_marks: Vec<CommandBlock>`（prompt_start/command_start/output_start/end 行号、exit_code、cwd，行号用 vt100 绝对行/scrollback 坐标）。**无需 fork vt100。**
- [ ] **M5.2 第 1 层 查询接口** - `TerminalHandle` 暴露 `command_blocks()` / `last_exit_code()`，可选 `on_command_finished(status)` 回调；无标记时退回普通 scrollback 不崩。
- [ ] **M5.3 第 2 层 呈现【外壳层】** - 命令块分隔线/输出区底色、失败命令（exit≠0）标红标记。仅依赖第 1 层 `command_blocks()`。
- [ ] **M5.4 第 2 层 交互【外壳层】** - 命令级导航（`Ctrl+↑/↓` 跳上/下一条命令）、选择粒度升级到整条命令输出、右键「重跑/复制命令/复制输出」。
- [ ] **M5.5 第 3 层 shell integration【配置面】** - 方案 A 零侵入（用户已配则用，未配降级）；方案 B spawn 时（配合 M6.3 spawn 环境）按 shell 类型注入 integration 脚本，注入开关进配置。第 1/2 层不得硬依赖注入成功。
- [ ] **M5.6 测试** - 单测覆盖 OSC 133/7 解析与 `command_blocks()` 状态机、无标记降级；PTY 覆盖第 2 层导航与命令级复制（如实现）。
- [ ] **M5.R Review** - 复核三层解耦（第 1 层不依赖第 2/3 层）、命令级退出码区别于进程级退出码，验证通过。

## 阶段 M6 - 分屏/标签页 + 会话管理 + spawn 环境（P2）

- [ ] **M6.1 分屏/标签页** - 基于现有 `VStack`/`HStack`/`Grid` 在单窗口内做 tmux 式 split panes 或 tab，与既有 WM 浮动窗口形态并存。
- [ ] **M6.2 会话管理** - 新建时选 shell/命令入口；重启已死会话（配合 M1.2 `exit_status`）；每窗口独立 cwd/profile（cwd 可继承 M5 OSC 7）。
- [ ] **M6.3 spawn 环境** - `spawn_command`（`terminal.rs:435-489`）设 `TERM` / `COLORTERM`、初始 `cwd`；提供显式 resize 接口（不再仅在 `draw` 被动触发，`terminal.rs:566-568`）。
- [ ] **M6.4 测试** - PTY 覆盖单窗口内分屏布局、tab 切换、死会话重启、新建时选择 shell/命令并落到指定 cwd。
- [ ] **M6.R Review** - 复核分屏焦点/尺寸传播正确、会话 profile 与 spawn 环境不泄漏宿主变量污染，验证通过。

## 阶段 M7 - 渲染保真度 + 配置界面（P3.1 + P3.2）

**含用户明确要求的「配置界面」——终端 app 的可视化设置面板。**

- [ ] **M7.1 光标形状** - 光标渲染（`terminal.rs:603-612`）读取 vt100 光标形状（block/bar/underline），不再一律 REVERSED 涂格。
- [ ] **M7.2 keypad 模式** - 接 `application_keypad()`（DECCKM `application_cursor` 已接）。
- [ ] **M7.3 配置模型** - 集中式 `TerminalConfig`：scrollback 长度、色板、前缀键、release 快捷键、alt screen 滚动键位与开关、shell/命令、cwd/profile、shell integration 注入开关、光标形状默认值等；配套默认值与持久化（沿用项目 JSON/YAML 主题配置风格，参考 `src/theme/config.rs`）。
- [ ] **M7.4 配置界面** - 新增可视化**设置窗口**：复用声明式 `VStack`/`HStack`/`Grid` + 现有 widgets（`TextBox`/`Checkbox`/`RadioGroup`/`ListBox`）分组编辑各配置项，支持即时预览/应用与保存；从菜单入口打开。
- [ ] **M7.5 配置生效接线** - 组件层各写死项（scrollback、色板、release 快捷键 `terminal.rs:121`、前缀键、滚动键位）改读 `TerminalConfig`，配置界面改动运行时生效。
- [ ] **M7.6 测试** - PTY 覆盖打开配置界面、修改 scrollback/前缀键/色板等并应用后行为随之改变、保存后重启保留；光标形状随 vt100 序列切换正确渲染。
- [ ] **M7.R Review** - 复核配置默认值不改变既有行为、配置持久化格式向后兼容、界面对无效输入有校验，验证通过。

## 收尾

- [ ] **Docs 更新** - 根据实际实现更新 `TERMINAL_GAP.md`（标注已闭合缺口）、README 或新增终端 app README；更新 `IMPLEMENTATION_PLAN.md` 里程碑状态（见 `AGENTS.md`）。
- [ ] **示例升级** - 把 `terminal_viewer` demo 升级为体现全功能（前缀键、copy-mode、分屏、会话管理、配置界面）的示例。
