# 执行计划

## 约束说明

- 我不会记录完整隐性推理过程；本文件记录可审计的执行计划、关键决策、执行进度和验证结果。
- `TODO.md` 是任务顺序和完成状态的唯一权威来源。
- 本次只完成 `TODO.md` 中第一个标题未带 `[DONE]` 的任务，完成后提交并停止。

## 初始步骤

1. 读取 `TODO.md`，定位第一个未完成任务。
2. 检查最近一次提交信息是否明确提到与该任务直接相关的未完成问题。
3. 读取该任务相关代码和测试，避免进行无关历史问题扫描。
4. 按任务要求实施修改；如果发现阻塞当前任务的真实前置问题，则最小化新增前置任务到 `TODO.md`，提交后停止。
5. 按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后运行完整测试套件；如仅文档变更且已有可复用绿色结果，则在完成记录中说明跳过原因。
6. 将完成任务的标题加上 `[DONE]`，更新完成记录；仅在阶段计划发生真实变化时更新 `PLAN.md`。
7. 提交所有与本任务相关的变更，使用清晰提交信息。

## 当前进度

- 已创建本计划文件。
- 已读取 `TODO.md`，第一个未完成任务为 `M3-R Review — 第 4 层 L0+L1 复核`。
- 已检查最近提交：`105da4c [M3-2] Unwrap tmux DCS passthrough OSC`。该提交属于 M3-R 直接复核范围，没有发现提交信息中明确声明的未完成问题。

## 当前任务执行计划

1. 复核 M3-1 环境注入实现：确认默认关闭、开启时 `$TMUX` / `$TMUX_PANE` 格式、`TERM` 覆盖开关和 spawn 行为边界。
2. 复核 M3-2 DCS `tmux;` passthrough 解包实现：确认内层 `ESC ESC` 还原、OSC 52 转交现有剪贴板路径、畸形输入健壮降级。
3. 检查 M3 阶段没有引入第 3 层 IPC / socket 协议依赖，且 crate 仍保持 `#![forbid(unsafe_code)]`。
4. 运行 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
5. 复核通过后，将 `TODO.md` 中 `M3-R` 标记为 `[DONE]`，补完成记录和验证记录。
6. 提交本次 review 相关变更后停止。

## 复核记录

- M3-1：已确认 `TerminalTmuxEnvironmentConfig` 默认 `inject = false`；`prepare_spawn_command` 默认只保持既有 `TERM=xterm-256color` / `COLORTERM=truecolor` 设置，不注入 `TMUX` / `TMUX_PANE`。开启后 `$TMUX` 使用 `socket_path,pid,session_id`，`$TMUX_PANE` 使用 `%pane_id`，`override_term` 才切换到 `tmux-256color`。
- M3-2：已确认 `TmuxDcsPassthroughDecoder` 在 `vt100` parser 前流式解包 `ESC P tmux; ... ESC \`，严格将内层 `ESC ESC` 还原为 `ESC`，正常 OSC 52 继续走既有 clipboard callback / system clipboard 后端；畸形 tmux DCS 与非 tmux DCS 不转发内部 OSC，避免误写剪贴板。
- 边界：未发现 `atto-ui-terminal` 引入 Unix socket、IPC server 或第 3 层协议依赖。
- 发现并修复：终端 PTY 测试中有三处 `unsafe { std::env::set_var(...) }`。为满足无 unsafe 约束，已给 `terminal_viewer` example 增加 `--config <path>` 参数，并将这些测试改为通过命令行传配置路径；`rg unsafe` 现在只剩 `#![forbid(unsafe_code)]` 声明。
- 验证发现：完整 `cargo test --workspace --all-targets` 中三处 `terminal_viewer` repro 测试失败，因为测试直接执行 `target/debug/examples/terminal_viewer`，该裸 example 二进制未由 full test 保证重建，运行到旧二进制后把 `--config` 当成子进程命令。
- 修复：将 `terminal_viewer` 同时声明为 cargo bin target，并把测试改为 `env!("CARGO_BIN_EXE_terminal_viewer")`，确保 full test 使用当前构建产物。为避免 Cargo 对同一路径同时作为 example/bin 发 warning，新增 `src/bin/terminal_viewer.rs` wrapper 并保留 `examples/terminal_viewer.rs` 作为手动入口。

## 验证记录

- `cargo fmt --all`：通过。
- `cargo fmt --all -- --check`：通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，无 warning。
- `cargo test -p atto-ui-terminal --test pty_terminal_window_interactions repro_viewer -- --nocapture`：通过，3 passed。
- `python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`：通过。
- 已更新 `TODO.md` 中 M3-R 的 `[DONE]` 状态、完成记录、验证记录与手动验证提示。此后仅修改任务记录文档，未改代码，不需要重新运行测试。
- 下一步：检查 git diff / status 并提交本任务变更。
