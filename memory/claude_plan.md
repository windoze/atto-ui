# 执行计划

## 约束理解

- 输出、进度记录和执行说明使用中文。
- `TODO.md` 是任务顺序、任务状态、依赖、验证要求和完成记录的权威来源。
- 本轮只完成 `TODO.md` 中第一个标题未带 `[DONE]` 的任务，然后停止。
- 任务只有在标题显式加上 `[DONE]` 后才算完成。
- 不做开放式历史问题扫查；只处理会阻塞当前任务或当前验证中暴露且未被排期的失败。
- 如遇无法按原规格完成的具体阻塞，需要在 `TODO.md` 中插入最小必要前置任务并提交后停止。
- 代码变更前后需要保持工作树中非本轮改动不被回退。
- 提交前按要求优先运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行完整测试；若只有文档变更且已有可复用绿色结果，可按规则跳过完整测试并记录原因。

## 当前执行计划

1. 读取 `TODO.md`，确定第一个标题未带 `[DONE]` 的任务，并检查该任务的正文、依赖、验收与完成记录要求。
2. 检查最新提交摘要，只有在其明确提到与当前任务直接相关的未完成问题时，才纳入当前任务或作为前置任务记录到 `TODO.md`。
3. 针对当前任务阅读必要代码和测试上下文，避免无关历史扫查。
4. 如当前任务可直接实现，则按仓库既有结构和风格进行最小完整实现；如发现具体阻塞，则更新 `TODO.md` 记录前置任务并停止。
5. 为实现添加或调整聚焦测试，确保覆盖任务要求和相关边界。
6. 运行 `cargo fmt`。
7. 运行 `cargo clippy --all-targets -- -D warnings`。
8. 运行完整测试套件，优先使用 `cargo test --all --all-targets`，并控制在 30 分钟内。
9. 更新 `TODO.md`：给完成任务标题加 `[DONE]`，补充完成记录，包括实现摘要、验证命令和结果。
10. 仅当阶段计划、依赖结构或完成标准发生真实变化时更新 `PLAN.md`。
11. 检查 `git status`，确认本轮相关变更和任何必须包含的未提交状态。
12. 使用清晰的任务编号提交信息提交变更。
13. 停止，不推进下一个任务。

## 进度记录

- 已创建本执行计划文件，下一步读取 `TODO.md` 确认当前任务。
- 已读取 `TODO.md` 和最新提交摘要。第一个未完成任务是 `M5-3 tmux shim 可执行文件（决策 E 乙）`；最新提交为 `[M5-2] Map terminal pane management IPC commands`，没有在摘要中明确标出与 M5-3 直接相关的未完成阻塞项。

## M5-3 任务执行计划

1. 阅读现有 `atto` CLI、M4 协议 / IPC helper、M5 pane 方法、terminal spawn 环境注入和相关测试，确认可复用入口。
2. 设计并实现一个名为 `tmux` 的 shim bin，作为薄客户端翻译层：解析受支持子命令后构造既有协议请求，通过 socket 发送，不新增协议语义。
3. 明确 socket 解析策略：优先按 tmux 环境变量中的 socket 路径连接，必要时兼容 `ATTO_UI_SOCKET`，且不支持命令必须非零退出并给出明确错误。
4. 在 terminal spawn 路径中增加可配置的 shim 目录 PATH 前置，并确保默认关闭或未配置时现有行为不变。
5. 添加集成测试，覆盖 PATH 前置后子进程调用 `tmux send-keys`、`tmux capture-pane`、`tmux split-window` 能经 socket 驱动原生 pane，以及不支持子命令返回非零。
6. 按顺序运行 `cargo fmt`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、完整测试。
7. 更新 `TODO.md` 中 M5-3 标题为 `[DONE]` 并补完成记录与验证记录。
8. 提交本轮所有相关变更后停止。

## M5-3 进度

- 已新增 terminal crate 的 `tmux` shim bin 注册。
- 已扩展 `TerminalTmuxEnvironmentConfig::shim_path`，并在 `prepare_spawn_command` 中让 `inject=true` 时前置 shim 目录到子进程 `PATH`。
- 已新增 shim 可执行文件，解析 `send-keys`、`capture-pane`、`list-panes`、`split-window`、`select-pane`、`break-pane`、`display-popup`，并通过既有 IPC 协议发送请求。
- 已新增聚焦测试，覆盖 unsupported subcommand 非零退出，以及子进程通过前置 `PATH` 调用 shim 后驱动 pane capture/send/split。
- 已运行 `cargo fmt --all`。
- 聚焦验证已通过：`cargo test -p atto-ui-terminal tmux_shim -- --nocapture` 和 `cargo test -p atto-ui-terminal --test ipc_pane -- --nocapture`。
- 正式验证已通过：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。
- 已更新 `TODO.md`，将 M5-3 标题标记为 `[DONE]`，并补充完成记录与验证记录。
- 下一步：检查最终 diff / status，提交本轮变更，然后停止。

## M5-4 任务执行计划

1. 读取 `TODO.md`，确认第一个未完成任务是 M5-4，并检查最新提交是否有直接相关未完成事项。
2. 阅读 `crates/atto-ui-terminal/src/pane.rs`、`SCRIPTING_LAYERS.md` 和 `pty_terminal_window_interactions` 中现有 pane 测试，确认实现边界。
3. 在 `TerminalPaneGroup` 的 split tree 中为 split 节点保存可调分隔尺寸，默认保持五五分布局。
4. 复用现有几何邻居选择逻辑，为 `prefix+方向键` 增加方向性 pane 导航。
5. 为 active pane 增加 `prefix+Ctrl+方向键` resize，调整最近相邻分隔线并夹紧到有效布局范围。
6. 增加 `prefix+z` pane 级 zoom / restore；zoom 时只绘制 active pane 到整个 pane group 区域。
7. 增加 `prefix+x` close active pane；关闭后复用 tree 移除和重布局路径，最后一个 pane 不关闭。
8. 更新或新增单元测试与 PTY 测试，覆盖方向导航、resize、zoom/restore、close/reflow。
9. 按顺序运行 `cargo fmt`、聚焦测试、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、完整 workspace 测试。
10. 更新 `TODO.md` M5-4 为 `[DONE]` 并补完成记录，提交本轮变更后停止。

## M5-4 进度

- 已读取 `TODO.md`，确认本轮第一个未完成任务是 `M5-4 本地 pane 层体验补全`。
- 已查看最新提交：`b4d6561 [M5-3] Add tmux shim executable`，提交标题未提示需要先处理的直接相关未完成事项。
- 已阅读 `crates/atto-ui-terminal/src/pane.rs`、`SCRIPTING_LAYERS.md` 和相关 PTY fixture。现有 pane tree 只保存 split 方向，布局固定五五分；已有 IPC 几何选择和 break-pane 可作为本地方向导航 / close 的基础。
- 决策：按 M5-4 规格让 `TerminalPaneGroup` 优先处理 `prefix+z` 为 pane zoom；这会替代单窗口测试里旧的“终端内部 prefix+z 触发窗口最大化”路径，相关测试已改成验证 pane zoom。
- 已完成实现：split node 记录 `first_len`，新增 active pane resize / zoom / close，前缀键支持方向选择、Ctrl+方向 resize、`z` zoom、`x` close；PT测试扩展覆盖新增交互，`terminal_viewer` 可见提示已同步。
- 已完成验证：`cargo fmt --all`、`cargo test -p atto-ui-terminal pane_ -- --nocapture`、`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions pty_terminal_prefix_splits_panes_inside_one_window -- --nocapture`、`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions -- --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets` 均通过。
- 已更新 `TODO.md`，将 M5-4 标题标记为 `[DONE]`，并补充完成记录与验证记录。
- 下一步：检查最终 diff / status，提交本轮变更，然后停止。

## M5-R 任务执行计划

1. 定向复核 M5-1 到 M5-4 的协议、IPC handler、`tmux` shim、pane 管理和本地 pane 交互实现。
2. 确认 shim / 子命令映射只是第 3 层 socket 之上的 client 翻译，没有实现 tmux server 协议或 control mode。
3. 确认 `send-keys` / `capture-pane` / pane 管理落到目标 pane 或原生窗口，pane id 寻址逻辑稳定且错误路径显式失败。
4. 确认本地 pane 方向导航、resize、zoom、close 不破坏既有 `%` / `"` / `o` / Tab 行为，也不影响外层窗口管理行为。
5. 按顺序运行 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
6. 若验证通过，更新 `TODO.md` 中 M5-R 标题为 `[DONE]` 并填写完成记录和验证命令。
7. 提交所有本轮相关变更后停止。

## M5-R 进度

- 已读取 `TODO.md`：第一个未完成任务是 `M5-R Review — 第 4 层 L2/L3 完整性与正确性复核`。
- 已检查最近提交：`dc3ff56 [M5-4] Complete local pane interactions`，未发现提交信息中明确提到与 M5-R 直接相关的未完成问题。
- 复核结论：协议层只新增 M5 pane 方法的数据形状和成功载荷，核心 IPC server 对这些方法走 `IpcMethodHandler` 扩展分发；未注册 terminal handler 时返回 `ActionNotSupported`。
- 复核结论：terminal IPC handler 将 `send_keys` / `capture_pane` / `list_panes` / `split_window` / `select_pane` / `break_pane` / `display_popup` 映射到 `TerminalPaneGroupHandle`、`TerminalHandle` 和原生 `Desktop` 窗口操作；多 pane group 下 pane id 冲突或省略 target 都会显式报错。
- 复核结论：`tmux` shim 是纯客户端翻译器，只解析常用子命令并通过 M4 socket 发送协议请求；`-CC` control mode、未知命令和未知选项均显式失败。
- 复核结论：本地 pane 交互复用 pane group 的共享 tree / active pane / layout 状态，新增方向选择、resize、zoom、close，同时保留 `%` / `"` / `o` / Tab 键位。
- 已完成验证：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets` 均通过。
- 已更新 `TODO.md`，将 M5-R 标题标为 `[DONE]`，并写入复核结论、手动验证提示和验证命令；完成记录写入后仅文档记录变化，复用刚才的绿色验证结果。
- 下一步：检查最终 diff / status，提交本轮变更，然后停止。
