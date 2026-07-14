# 执行计划

## 任务边界

- 目标：按照 `TODO.md` 的顺序只完成第一个未完成任务，然后停止。
- 权威来源：`TODO.md` 决定任务顺序、完成状态、约束、验证要求和完成记录。
- `PLAN.md` 只在阶段级计划、依赖或完成标准变化时更新。
- 任务只有标题显式带 `[DONE]` 才算完成。

## 执行原则

- 不做开放式历史问题扫描；先定位第一个未完成任务。
- 不因任务较大就拆分；只有遇到具体阻塞且必须新增前置任务时，才以最小粒度更新 `TODO.md`。
- 当前任务相关的已知缺陷、回归、规格不匹配或测试失败必须修复，或明确加入 `TODO.md` 的正确依赖位置。
- 不接受规避实现、缩窄范围或临时 shim 作为完成标准。
- 编辑前后持续更新本文件，记录当前计划、关键发现、验证结果和完成状态。

## 步骤计划

1. 读取 `TODO.md`，按标题是否带 `[DONE]` 识别第一个未完成任务。
2. 查看最新提交信息，判断是否明确提到与该任务直接相关的未完成事项。
3. 针对该任务读取最小必要上下文，包括相关源文件、测试、文档和任务依赖。
4. 如任务可直接执行，按代码库既有模式实现；如存在具体阻塞，在 `TODO.md` 中加入最小必要前置任务并停止。
5. 修改前在本文件记录将要编辑的文件和意图。
6. 运行 `cargo fmt`。
7. 运行 `cargo clippy --all-targets -- -D warnings`。
8. 在 clippy 通过后运行完整测试，优先使用 `cargo test --all --all-targets`，并控制在 30 分钟以内。
9. 若发现未被排期的测试失败，修复或在 `TODO.md` 中加入正确顺序的任务；不能在失败未处理时标记当前任务完成。
10. 更新 `TODO.md`：给当前任务标题加 `[DONE]`，并补充完成记录、验证命令和结果。
11. 如阶段级计划未变化，不更新 `PLAN.md`。
12. 检查 git diff，确认只包含当前任务相关变更。
13. 用清晰的任务提交信息提交所有相关未提交文件。
14. 停止，不进入下一个任务。

## 当前状态

- 状态：已读取 `TODO.md`，第一个未完成任务为 `M3-1 L0 环境探测注入`。
- 最新提交：`a59f62b [M2-R] Review semantic scripting layer`，未明确提到与 M3-1 直接相关的未完成事项。
- 当前任务目标：在 `crates/atto-ui-terminal/src/terminal.rs` 的 `spawn_command` 环境准备处增加可选 tmux 环境探测变量注入，默认关闭；关闭时行为保持现状；开启时子进程可读取 `$TMUX`、`$TMUX_PANE`，并可选把 `$TERM` 改为 `tmux-256color`。

## M3-1 具体计划

1. 读取 `crates/atto-ui-terminal/src/terminal.rs` 中 `spawn_command` 及相关 builder / config / test helper。
2. 查找 terminal crate 现有 PTY / snapshot 测试模式，优先复用已有 fixture。
3. 设计一个最小公开配置入口，默认关闭 tmux 探测注入；配置字段应能提供 socket path、session id、pane id，以及是否覆盖 TERM。
4. 在 `spawn_command` 环境准备处注入 `$TMUX`、`$TMUX_PANE`，仅在开关启用时生效；关闭时不改变当前环境设置。
5. 添加覆盖：关闭时 probe 读到空值；开启时 probe 读到指定 tmux 值；可选 TERM 覆盖关闭 / 开启行为明确。
6. 按顺序运行 `cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
7. 更新 `TODO.md` 的 M3-1 标题为 `[DONE]` 并补完成记录、验证命令。
8. 检查 diff 并提交，提交信息使用 `[M3-1] Add tmux environment probe injection`。

## M3-1 实施记录

- 设计决策：新增 `TerminalTmuxEnvironmentConfig`，作为持久化配置和 builder/handle 运行时开关共用的数据结构。默认 `inject = false`，因此默认不设置 `$TMUX` / `$TMUX_PANE`，`TERM` 仍为 `xterm-256color`。
- 设计决策：默认关闭时不清理继承自宿主进程的 tmux 变量，以保持既有 spawn 行为；测试中通过 `/usr/bin/env -u TMUX -u TMUX_PANE` 控制宿主环境，验证 atto-ui 本身是否注入变量。
- 待编辑文件：`crates/atto-ui-terminal/src/config.rs`、`src/terminal.rs`、`src/settings.rs`、`src/lib.rs`、`tests/pty_terminal_window_interactions.rs`。
- 已完成：配置模型、运行时共享状态、spawn 注入逻辑、builder/handle API、配置/settings 单测样例和 PTY probe 测试均已修改；`cargo fmt --all` 已运行通过。
- 聚焦测试发现：新增 PTY probe 的启用场景实际输出正确，但长字符串被终端窗口换行，导致单个 `wait_for_text` 无法匹配。已将断言拆成两段稳定文本。
- 已通过聚焦验证：`cargo test -p atto-ui-terminal tmux -- --nocapture`、`cargo test -p atto-ui-terminal terminal_config -- --nocapture`、`cargo test -p atto-ui-terminal terminal_settings_draft_round_trips_config -- --nocapture`。
- 已通过完整验收：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。
- 已完成：`TODO.md` 中 M3-1 已标为 `[DONE]`，并补充完成记录与验证命令。
- 已完成：核心 diff 已人工复核，`git diff --check` 通过。
- 下一步：提交本任务变更并停止。
