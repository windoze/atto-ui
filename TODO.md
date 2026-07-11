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
- [x] **[DONE] M2.3 测试** - PTY 覆盖 shell 退出后窗口回收/退出提示与 R 重启入口；`OSC 0/2` 标题联动到窗口标题与菜单。
  - 完成记录（2026-07-11）：补齐 `pty_terminal_window_interactions` 中的 M2 PTY 覆盖；保留 shell 子进程退出后显示退出提示、释放 capture、按 `R` 重启并更新 `RESTARTS` 的回归测试；将标题联动测试扩展为同时覆盖 OSC 0 与 OSC 2，确认窗口标题和 Windows → Switch to 菜单项同步更新。
  - 验证：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions -- pty_terminal_dead_process_prompts_and_restarts pty_terminal_osc_zero_and_two_titles_update_window_title_and_windows_menu --nocapture`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M2.R Review** - 复核回收策略不误杀存活窗口、标题联动线程安全，验证通过。
  - 完成记录（2026-07-11）：复核终端 viewer、PTY window fixture 与 `TerminalHandle` 状态流，确认死窗口回收只在组件记录到 `exit_status()` 后释放 capture 并显示退出提示，不基于焦点/标题等启发式关闭或误杀仍存活窗口；重启路径会替换 view、刷新 handle 并恢复默认标题。标题联动通过 `TerminalHandle::window_title()` 克隆共享状态后，在 UI tick/action 线程调用 `Desktop::set_title` 与刷新 Windows 菜单，callback 事件在 terminal shared 锁外派发，未引入跨线程 UI 变更。
  - 验证：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。

## 阶段 M3 - tmux 式前缀键框架（P1.6 组件层 + 外壳层）

- [x] **[DONE] M3.1 前缀态状态机** - capture 分支（`terminal.rs:701-712`）收到前缀键进入前缀态（不转发），下一个键查前缀命令表；未命中按策略连同前缀发子进程或吞掉。
  - 完成记录（2026-07-11）：在 `TerminalShared` 中新增默认 `Ctrl+B` 前缀键与 pending 状态；capture 态收到前缀键时只进入 pending、不转发给子进程；下一次非 release 按键先走前缀命令 hook（M3.3 填充命令表），未命中时按 lossless fallback 将已暂存前缀与当前按键编码后一并发给子进程。Tab/BackTab 的 capture hook 也复用同一状态机，capture 释放或失焦会清除 pending 前缀，避免悬挂状态污染后续输入。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-terminal --test input_encoding terminal_prefix -- --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M3.2 可配置前缀键** - 默认 `Ctrl+B`，约束为 plain `Ctrl+<字母>`；前缀键可配置（暂用组件字段，M7 接入配置模型）。
  - 完成记录（2026-07-11）：新增终端前缀键配置入口，`TerminalEmulator::prefix_key` / `prefix_shortcut` 与 `TerminalHandle::set_prefix_shortcut` 均校验并规范化为 plain `Ctrl+<ASCII letter>`，默认保持 `Ctrl+B`；重配前缀键会清除 pending 前缀态，fallback 转发使用当前配置。动态组件 schema 暂时暴露 `prefix_key` 字段，供 M7 配置模型接线前使用。补充 input encoding 回归覆盖默认前缀、可配置 `Ctrl+A`、大小写规范化、非法修饰符/非字母拒绝与非前缀键继续转发。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-terminal --test input_encoding terminal_prefix -- --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M3.3 前缀命令表** - `前缀 + F10` 激活菜单、`前缀 + w` 窗口模式、`前缀 + z` 最大化/还原、`前缀 + [` 进 copy-mode（占位，M4 实现选择）、`前缀 + 前缀` 转义一个字面前缀给子进程。
  - 完成记录（2026-07-11）：终端 capture 态新增前缀命令表；默认/配置前缀后，`前缀+F10` 通过 typed `ComponentAction` 激活菜单，`前缀+w` 切换窗口管理模式，`前缀+z` 最大化/还原当前窗口，`前缀+[` 进入 copy-mode 占位状态供 M4 扩展，`前缀+前缀` 只向子进程发送一个字面前缀。未命中命令继续沿用 lossless fallback，把前缀和后续键一并转发给子进程。
  - 验证：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-terminal --test input_encoding terminal_prefix -- --nocapture`、`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions pty_terminal_prefix_commands_drive_desktop_chrome -- --nocapture`、`cargo test -p atto-ui --lib component_action_can_ -- --nocapture`、`cargo test --workspace --all-targets` 均通过；完成记录/计划文档更新后仅改动 Markdown，未重跑全套。
- [x] **[DONE] M3.4 事件派发桥接** - 复核并收敛 M3.3 引入的 typed `ComponentAction` 外壳命令桥接（替代 raw-key `EventResult::ignored()` 冒泡，避免 `前缀+w/z` 无法对应现有全局快捷键），确保 focused view、pointer capture、tooltip view、`send_event_to_window` 等路径语义一致，modal 仍阻断外壳命令。
  - 完成记录（2026-07-12）：收敛 typed `ComponentAction` 到统一 desktop bridge，`WindowManagerAction` 可携带组件结果并由 `Desktop` 统一处理 close/menu/window-mode/maximize 等外壳命令；focused view、titlebar、pointer capture、tooltip hit-test、drag/drop 和 `send_event_to_window` 均复用同一处理路径。modal 激活时 shell command 类组件动作会被消费并清理 which-key，不会激活菜单、窗口模式或最大化外壳状态。
  - 验证：`cargo test -p atto-ui --lib component_action -- --nocapture`、`cargo test -p atto-ui --lib modal -- --nocapture`、`cargo test -p atto-ui-terminal --test input_encoding terminal_prefix -- --nocapture`、`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M3.5 测试** - PTY 覆盖 capture 态 `前缀 + F10` 激活菜单、`前缀 + w` 窗口模式、`前缀 + 前缀` 字面前缀到子进程；非终端窗口快捷键仍直达、capture 释放后 `F10` 直达。
  - 完成记录（2026-07-12）：保留并复跑既有 `pty_terminal_prefix_commands_drive_desktop_chrome` 覆盖 capture 态 `前缀+F10` 激活菜单与 `前缀+w` 进入窗口模式；新增 PTY 子进程 raw-byte 回归，验证 `前缀+前缀` 只向子进程发送单个字面 `Ctrl+B`（`BYTE=02`）；新增非终端 Tools 窗口焦点下 `F10` 直达菜单，以及终端 capture 释放后 `F10` 直达菜单的回归覆盖。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions -- pty_terminal_prefix_commands_drive_desktop_chrome pty_terminal_prefix_escape_sends_literal_prefix_to_subprocess pty_terminal_global_shortcuts_reach_non_terminal_and_released_capture --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M3.R Review** - 复核前缀不吞掉子进程需要的键（可靠转义）、命令表可配置、双向 escape 收敛为单一前缀，验证通过。
  - 完成记录（2026-07-12）：复核 M3 前缀键状态机、typed `ComponentAction` 派发桥接与 PTY 覆盖，确认前缀后未命中的可编码按键会按 lossless fallback 连同前缀转发给子进程，`前缀+前缀` 始终绕过命令表并只发送一个字面前缀。补齐发现的命令表配置缺口：新增 `TerminalPrefixBinding`，`TerminalEmulator` / `TerminalHandle` 均可替换整张前缀命令表或增量替换单个绑定，默认 `F10`/`w`/`z`/`[` 行为保持不变，运行时替换会清除 pending 前缀态避免旧配置污染后续输入。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-terminal --test input_encoding terminal_prefix -- --nocapture`、`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions -- pty_terminal_prefix_commands_drive_desktop_chrome pty_terminal_prefix_escape_sends_literal_prefix_to_subprocess pty_terminal_global_shortcuts_reach_non_terminal_and_released_capture --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。

## 阶段 M4 - 选择复制 + 剪贴板 + alt screen 滚动分流（P1.2 + P1.3 + P1.5）

- [x] **[DONE] M4.1 selection 状态机** - 新增统一 selection 状态机：选区高亮 + 命中测试 + 从 vt100 `screen` 提取选中文本；鼠标与键盘两条入口共享。可参考 chat 组件已有文本选择实现。
  - 完成记录（2026-07-12）：新增 `selection` 模块，提供基于 scrollback+visible screen 绝对坐标的 `TerminalSelectionPosition` / `TerminalSelectionRange`、统一 anchor/focus selection 状态机、可复用的 visible-cell 命中测试、宽字符感知的高亮 cell range，以及从 vt100 `Screen` 提取选中文本的核心逻辑。`TerminalShared` 接入 selection 状态，`TerminalEmulator` draw 路径按主题 selection 样式渲染选区，`TerminalHandle` 暴露 `begin_selection` / `update_selection` / `clear_selection` / `selection_range` / `selection_position_for_view_cell` / `selected_text`，供后续鼠标框选与 copy-mode 键盘入口共享。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-terminal --test input_encoding terminal_selection -- --nocapture`、`cargo test -p atto-ui-terminal selection -- --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M4.2 鼠标本地框选** - 子进程开鼠标报告时 `Shift+拖拽`=本地框选、不按=转发；未开鼠标报告时直接拖拽即框选（修掉 `capture_on_click` recapture 浪费点击，`terminal.rs:747-770`）。
  - 完成记录（2026-07-12）：终端组件 mouse handling 接入 M4.1 selection 状态机；未开启子进程鼠标报告时 plain 左键拖拽会本地开始/更新/结束 selection，开启鼠标报告时 plain 左键 down/drag/up 继续按协议转发给子进程，`Shift+`左键拖拽改走本地 selection 且不转发。selection 状态新增 dragging/finish 语义，避免鼠标释放后后续 drag 继续污染已完成选区；鼠标焦点 recapture 不再吞掉原始点击，`capture_on_click` 重新捕获后同一个 down 事件会继续执行本地 selection 或子进程 mouse forwarding。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-terminal --test input_encoding -- --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M4.3 copy-mode** - 经 `前缀 + [`（M3.3）进入；方向键与 hjkl、起选 `v`/`Space`、复制 `y`/`Enter`、`Esc`/`q` 取消；copy-mode 内滚轮/方向键永远本地 scrollback 导航。
  - 完成记录（2026-07-12）：将 M3 的 copy-mode 占位标记替换为真实 modal 状态，进入时初始化本地 copy cursor 并清理旧 selection；copy-mode 内方向键、hjkl、PageUp/PageDown、Home/End 均只移动本地 cursor/scrollback，不再转发给子进程；`v`/`Space` 使用统一 selection 状态机起选，`y`/`Enter` 将选中文本写入组件内部 copy buffer 并退出，`Esc`/`q` 取消并清理 selection。copy-mode 鼠标滚轮在子进程启用 mouse reporting 时也被本地消费，避免泄漏到子进程。PTY fixture 状态行暴露 copy-mode/copy buffer 以覆盖端到端交互。
  - 验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-terminal --test input_encoding terminal_copy_mode -- --nocapture`、`cargo test -p atto-ui-terminal --test input_encoding terminal_prefix_command_table_enters_copy_mode -- --nocapture`、`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions -- pty_terminal_copy_mode_selects_and_copies_text --nocapture`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check` 均通过。
- [x] **[DONE] M4.4 剪贴板（首版）** - 选择 → 组件内部 copy buffer + 粘贴回子进程（bracketed paste 已支持，`terminal.rs:719-737`），先让选择/高亮/命中测试核心逻辑落地过 PTY。
  - 完成记录（2026-07-12）：终端组件首版剪贴板保持纯组件内部 copy buffer，不引入系统剪贴板依赖；copy-mode 的 `y`/`Enter` 继续写入 buffer，鼠标本地框选在释放左键时自动把选中文本写入 buffer。新增 `TerminalHandle::copy_selection()` 与 `paste_copied_text()`，并在默认前缀命令表中加入 `前缀+]` 粘贴内部 buffer；粘贴统一复用 bracketed paste 编码路径，子进程开启 bracketed paste 时自动包裹 `\x1b[200~` / `\x1b[201~`。新增单测覆盖鼠标选区入 buffer、handle/prefix 粘贴与 bracketed paste；新增 PTY 回归验证 copy-mode 复制后的内部 buffer 可通过 `前缀+]` 粘贴到真实子进程。
  - 验证：`cargo test -p atto-ui-terminal --test input_encoding -- --nocapture`、`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions pty_terminal_local_copy_buffer_pastes_to_subprocess -- --nocapture`、`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M4.5 alt screen 滚动分流** - 滚轮分支前置三级决策树：`mouse_protocol_mode() != None` → 转发 SGR 滚轮（已有 `encode_mouse_event` 64/65，`terminal.rs:1266`）；`alternate_screen()` → alternate scroll 翻方向键（默认 ×3）发子进程；else → 本地 `set_scrollback`（现有逻辑，`terminal.rs:499/521`）。不用 `application_cursor()`/清屏启发式。
  - 完成记录（2026-07-12）：终端 capture 态鼠标滚轮路径补齐三级分流；子进程启用 mouse reporting 时继续优先经 `encode_mouse_event` 转发滚轮事件，alternate screen 且未启用 mouse reporting 时将竖向滚轮转换为默认 3 次 Up/Down 方向键输入并发往子进程，主屏幕仍沿用本地 scrollback 调整，不使用 `application_cursor()` 或清屏状态作为分流条件。新增 `input_encoding` 回归覆盖 alternate screen 滚轮方向键、mouse reporting 优先级和主屏本地 scrollback。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-terminal --test input_encoding -- --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M4.6 剪贴板（后续，可选）** - 接系统剪贴板（`arboard`）与 OSC 52（依赖 M1.3 `copy_to_clipboard` 回调），OSC 52 优先、`arboard` 兜底。可拆独立 PR。
  - 完成记录（2026-07-12）：终端组件新增可配置 `TerminalSystemClipboard` 后端，默认复制路径先向宿主发送 OSC 52，再通过 `arboard` 尝试原生系统剪贴板兜底；测试可注入假后端或禁用真实剪贴板写入。selection、鼠标框选释放、copy-mode `y`/`Enter` 均继续写入组件内部 copy buffer，并同步到配置的系统剪贴板后端。OSC 52 `copy_to_clipboard` 回调现在会解析标准 clipboard selector 的 base64/UTF-8 payload，更新 `last_clipboard_copy`、内部 copy buffer 和系统剪贴板；非标准 selector 保持可观察回调但不误写系统 clipboard。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-terminal --test callbacks --test input_encoding terminal_ -- --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M4.7 测试** - PTY 覆盖鼠标框选/复制、copy-mode 选择复制、vim(开/关鼠标)/less/htop/fzf `--height` 滚轮各落正确分支、主屏 scrollback 仍正常。
  - 完成记录（2026-07-12）：补齐 `pty_terminal_window_interactions` 的 M4 PTY 覆盖；新增鼠标 plain drag 在无 mouse reporting 时本地框选并复制、`Shift+drag` 在子进程启用 mouse reporting 时仍本地框选并复制的回归测试。新增 app-like 滚轮分流 PTY probe，分别覆盖 vim mouse on、htop、fzf `--height` 这类 mouse reporting 优先转发 SGR wheel，vim mouse off、less 这类 alternate screen 无 mouse reporting 转换为 3 次方向键，以及主屏无 mouse reporting 时滚轮仍走本地 scrollback。既有 copy-mode 选择复制与内部 copy buffer 粘贴 PTY 覆盖一并复跑。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions -- --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 均通过。
- [x] **[DONE] M4.R Review** - 复核 selection 命中测试对宽字符正确、滚动分流三级树覆盖残余类靠 copy-mode 兜底、剪贴板首版不引入跨平台依赖，验证通过。
  - 完成记录（2026-07-12）：复核 M4 selection、copy-mode、滚轮分流与剪贴板实现；补齐并修复宽字符选区文本提取在只命中宽字符左/右半格时无法复制字符的问题，使高亮命中测试与实际 copied text 保持一致。确认滚轮分流仍按 mouse reporting 优先、alternate screen 转方向键、主屏本地 scrollback 的三级树执行，copy-mode 内滚轮/方向键继续本地消费。剪贴板首版核心仍保持组件内部 copy buffer；M4.6 系统剪贴板能力集中在可替换/可禁用的 `TerminalSystemClipboard` 后端。
  - 验证：`cargo test -p atto-ui-terminal selected_text_expands_partial_wide_character_cells -- --nocapture`、`cargo test -p atto-ui-terminal --test input_encoding terminal_mouse_selection_copies_wide_character_from_either_cell -- --nocapture`、`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`（30 分钟上限）均通过。

## 阶段 M5 - 语义提示符标记（P1.4，OSC 133/7）

三层互相独立，不混在一起实现。

- [x] **[DONE] M5.1 第 1 层 感知与信号【组件层】** - 与 M1.3 共用 callbacks，接 `unhandled_osc`（vt100 透传 `[b"133", b"A"]` / `[b"133", b"D", b"0"]` / `[b"7", b"file://..."]`），推进小状态机记 `command_marks: Vec<CommandBlock>`（prompt_start/command_start/output_start/end 行号、exit_code、cwd，行号用 vt100 绝对行/scrollback 坐标）。**无需 fork vt100。**
  - 完成记录（2026-07-12）：终端组件复用 M1.3 的 `new_with_callbacks` 通道，实现 `vt100::Callbacks::unhandled_osc` 捕获未处理 OSC 参数；新增内部 `CommandBlock` 状态机与 `command_marks`/`current_cwd` 状态，支持 OSC 133 `A`/`B`/`C`/`D` 记录 prompt、command、output、end 的 vt100 scrollback 绝对行坐标和命令级退出码，并支持 OSC 7 `file://...` cwd 解析与 percent decode。无 OSC 标记的普通输出保持降级为空状态，不依赖 shell integration 注入，也未 fork vt100。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-terminal osc133 -- --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`（30 分钟上限）均通过。
- [x] **[DONE] M5.2 第 1 层 查询接口** - `TerminalHandle` 暴露 `command_blocks()` / `last_exit_code()`，可选 `on_command_finished(status)` 回调；无标记时退回普通 scrollback 不崩。
  - 完成记录（2026-07-12）：新增公开 `TerminalCommandBlock` 快照类型并从 crate root 导出，`TerminalHandle::command_blocks()` 可读取 OSC 133/7 记录到的命令块列表，`TerminalHandle::last_exit_code()` 返回最近完成命令块的命令级退出码；新增 `TerminalEmulator::on_command_finished(...)` 回调，在 OSC 133 `D` 完成命令块时于锁外派发完成块快照。无 shell integration/OSC 标记的普通输出保持 `command_blocks()` 为空、`last_exit_code()` 为 `None`，不影响普通 scrollback。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-terminal --test callbacks terminal_command_block -- --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`（30 分钟上限）均通过。
- [x] **[DONE] M5.3 第 2 层 呈现【外壳层】** - 命令块分隔线/输出区底色、失败命令（exit≠0）标红标记。仅依赖第 1 层 `command_blocks()`。
  - 完成记录（2026-07-12）：新增可选 `TerminalCommandBlockPresentation` 命令块呈现模式，终端外壳层 `terminal_viewer` 与 `snapshot_terminal_window_app` 显式启用；渲染时仅依据 M5.2 暴露的 OSC 133 命令块行号，在 prompt 行空白区绘制命令块分隔线、为 output 行套用主题驱动底色，并在 exit code 非 0 的命令块结束行绘制红色失败标记。无命令块或未启用呈现时保持既有终端渲染行为。
  - 验证：`cargo fmt --all`、`cargo test -p atto-ui-terminal --test input_encoding terminal_command_block_presentation_marks_semantic_rows -- --nocapture`、`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions pty_terminal_command_block_presentation_marks_failed_commands -- --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`（30 分钟上限）均通过。
- [x] **[DONE] M5.4 第 2 层 交互【外壳层】** - 命令级导航（`Ctrl+↑/↓` 跳上/下一条命令）、选择粒度升级到整条命令输出、右键「重跑/复制命令/复制输出」。
  - 完成记录（2026-07-12）：终端命令块记录补充 OSC 133 marker 列坐标，保留既有行号查询的同时支持精确提取命令文本与输出文本。`TerminalHandle` 新增命令块命中、上一条/下一条命令导航、整条输出选区、复制命令、复制输出与重跑命令接口；`Ctrl+↑/↓` 在存在命令块时本地滚动到相邻命令且不转发给子进程，无标记时继续降级为普通输入路径。终端 window fixture 与 `terminal_viewer` 接入右键 Command 菜单，支持 Rerun / Copy command / Copy output，并在打开菜单时把选区提升为整条命令输出。
  - 验证：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-terminal --test input_encoding terminal_command_block -- --nocapture`、`cargo test -p atto-ui-terminal --test input_encoding terminal_ctrl_arrows_navigate_command_blocks_without_forwarding -- --nocapture`、`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions -- pty_terminal_command_context_menu_copies_output_and_reruns --nocapture`、`cargo test --workspace --all-targets`（30 分钟上限）均通过。
- [x] **[DONE] M5.5 第 3 层 shell integration【配置面】** - 方案 A 零侵入（用户已配则用，未配降级）；方案 B spawn 时（配合 M6.3 spawn 环境）按 shell 类型注入 integration 脚本，注入开关进配置。第 1/2 层不得硬依赖注入成功。
  - 完成记录（2026-07-12）：新增默认关闭的 `TerminalShellIntegration` 配置面，组件 builder、`TerminalHandle` 与动态组件 schema 均可读写开关；默认零侵入路径继续只消费用户 shell 已发出的 OSC 133/7 标记，无标记时第 1/2 层仍降级为普通 scrollback。开启后，`spawn_command` 会对支持的交互式 bash/zsh 启动注入临时 shell integration 脚本，生成 OSC 133/7 标记；非交互式 `-c` 等命令和不支持的 shell 保持原样。注入脚本临时文件随进程生命周期清理，注入失败会记录到 handle 可查询错误并继续按未注入命令启动，避免第 1/2 层硬依赖第 3 层成功。
  - 验证：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-terminal shell_integration -- --nocapture`、`cargo test --workspace --all-targets`（30 分钟上限）均通过。
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
