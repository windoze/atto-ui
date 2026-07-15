# 执行计划

## 当前状态

- 尚未读取 `TODO.md`，因此当前任务编号和任务正文待确认。
- 本文件用于记录可审计的执行计划、关键决策和进度更新；不会记录隐藏推理逐字稿。

## 执行原则

- 以 `TODO.md` 为唯一任务顺序和完成状态来源。
- 只完成第一个标题未带 `[DONE]` 的任务，完成后停止。
- 不做开放式历史问题排查；只处理阻塞当前任务、影响当前任务指定行为、或测试策略要求必须处理的问题。
- 若遇到不能直接完成的具体阻塞，向 `TODO.md` 插入最小必要前置任务，提交后停止。
- 完成任务前必须按要求运行格式化、lint 和相关测试；若观察到未被明确排期的失败测试，必须修复或排期。

## 步骤计划

1. 读取 `TODO.md`，确认第一个未完成任务及其验收要求。
2. 查看最新提交信息，判断是否明确提到与当前任务直接相关的未完成问题。
3. 按当前任务需要读取相关源码、测试和计划文档；避免无关排查。
4. 实现任务要求，编辑前在对话中说明将修改的区域，并在本文件记录关键进展。
5. 运行 `cargo fmt`。
6. 运行 `cargo clippy --all-targets -- -D warnings`。
7. 运行必要测试；若涉及代码行为变更，运行完整测试套件并设置不超过 30 分钟的超时。
8. 更新 `TODO.md`：给已完成任务标题加 `[DONE]`，补全 completion record。
9. 仅当阶段级计划发生变化时更新 `PLAN.md`。
10. 检查 git 状态，提交本次任务涉及的所有未提交更改。
11. 停止，不进入下一个任务。

## 进度记录

- 已创建初始执行计划，下一步读取 `TODO.md` 并确认当前任务。
- 已读取 `TODO.md`，确认第一个未完成任务为 `M5-2 pane 管理命令映射`。
- 最新提交为 `[M5-1] Add terminal pane IPC methods`，与当前任务直接相关：M5-2 应在 M5-1 的 pane IPC 协议、server 扩展分发和 terminal pane handler 基础上继续实现。

## M5-2 任务计划

1. 精读 `M5-2` 任务正文、`SCRIPTING_LAYERS.md` 中 tmux / pane 管理相关设计，以及 M5-1 已提交的协议和 terminal IPC 代码。
2. 梳理现有 `TerminalPaneGroup` / `TerminalPaneGroupHandle` 是否已经暴露 split、方向选中、break pane、popup 所需能力；若缺少公共 handle 方法，优先补齐通用能力，而不是在 IPC 层绕过内部状态。
3. 扩展第 3 层协议，增加可序列化 pane 管理方法和结果类型：
   - `split_window`
   - `select_pane`
   - `break_pane`
   - `display_popup`
   - 复用或保持现有 `list_panes`
4. 扩展 `IpcServer` 的 extension dispatch 和 `atto-ui-terminal` 的 pane IPC handler，把协议方法映射到原生 pane / window 行为。
5. 根据实现边界补充测试，至少覆盖：
   - split 后 pane 数增加；
   - `select-pane -L/-R` 等几何方向选择能切换 active pane；
   - `break-pane` 能把 pane 脱离为独立窗口，或在发现缺少必要架构入口时添加最小前置任务并停止。
6. 更新 `atto` CLI 的人类可读输出，避免新增协议结果导致客户端不可用。
7. 执行 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`，并按观察到的失败测试策略处理。
8. 将 `TODO.md` 中 `M5-2` 标记 `[DONE]` 并写入完成记录 / 验证记录。
9. 提交本次任务所有变更后停止。

## M5-2 设计确认

- `IpcMethodHandler` 当前只接收 `&mut Desktop` 和 `ProtocolMethod`，不接收屏幕区域；`break-pane` / `display-popup` 创建窗口时需要屏幕区域进行窗口 placement，因此需要把核心 handler 签名扩展为接收当前 screen。
- `TerminalPaneGroupHandle` 当前只保存 pane 快照，不能同步执行 mutating pane 管理命令。为了避免“响应成功但实际命令稍后才执行”的伪完成，本任务会把 pane group 的权威状态移入共享 mutex，让 handle 可以在 UI 线程通过同一份状态立即执行 split、几何 select、break。
- `break-pane` 将从 pane group 中移出目标 `TerminalEmulator`，重建 pane tree，并把该 terminal 作为新的 normal `Window` view 加到 `Desktop`。
- `display-popup` 将创建一个 floating terminal window。若协议携带命令，则在新 terminal 中 spawn 该命令；否则创建空 terminal view。该方法不实现 tmux shim 参数解析，解析工作留给 M5-3。

## M5-2 当前进度

- 已扩展 `src/protocol.rs`：新增 `split_window`、`select_pane`、`break_pane`、`display_popup` 请求 / 响应类型，并补充 JSON roundtrip 测试。
- 已扩展核心 IPC extension handler 签名：handler 现在接收当前 screen，新增 pane 管理方法在未注册 terminal handler 时会继续映射为 `ActionNotSupported`。
- 已重构 `TerminalPaneGroup`：pane tree、pane 列表、active pane、last layout 与 pane factory 进入共享权威状态，`TerminalPaneGroupHandle` 可同步执行 split/select/break。
- 已实现 terminal IPC 映射：
  - `split_window` → 原生 pane split；
  - `select_pane` → 基于 pane rect center 与重叠区的 LRUD 几何选择；
  - `break_pane` → 从 group 移出 pane 并加入独立 normal window；
  - `display_popup` → 创建 floating terminal window，可选 spawn argv command。
- 已补充 `crates/atto-ui-terminal/tests/ipc_pane.rs` 集成测试，覆盖 split、LRUD select、break 到独立窗口、display popup。
- 已通过验证：
  - `cargo test -p atto-ui protocol -- --nocapture`
  - `cargo test -p atto-ui-terminal --test ipc_pane -- --nocapture`
  - `cargo test -p atto-ui-terminal pane_group -- --nocapture`
  - `cargo test -p atto-ui ipc_server_reports_extension_methods_unsupported_without_handler -- --nocapture`

## M5-2 完成状态

- 已运行 `cargo fmt --all` 并通过最终 `cargo fmt --all -- --check`。
- 已通过最终 `cargo clippy --workspace --all-targets -- -D warnings`。
- 已通过最终完整测试：`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。
- 已将 `TODO.md` 中 `M5-2 pane 管理命令映射` 标记为 `[DONE]`，并写入完成记录与验证记录。
- 已完成提交前检查：变更范围仅包含 M5-2 相关协议、IPC、terminal pane 实现、测试、`TODO.md` 和本执行计划。
- 下一步：提交本次任务所有变更，然后停止。
