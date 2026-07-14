# 执行计划

## 约束说明

- 本文件记录本次执行的可审计计划、决策依据、关键进展和验证结果。
- 不记录不可公开的内部推理细节；后续如计划变化或关键步骤完成，会及时更新本文件。
- 本次只完成 `TODO.md` 中第一个未完成任务，完成后提交 Git commit 并停止。

## 初始计划

1. 读取 `TODO.md`，按文档顺序识别第一个标题未带 `[DONE]` 前缀的任务。
2. 检查最新提交信息；仅当它明确提到与当前任务直接相关的未完成问题时，将其纳入当前任务或作为前置项记录到 `TODO.md`。
3. 阅读当前任务相关的代码、测试、计划文档和上下文，确认任务要求、依赖、完成标准和验证要求。
4. 如任务可以直接完成，按现有代码风格实现；如遇到阻塞当前任务的真实前置缺陷，则在 `TODO.md` 中插入最小必要前置任务并停止。
5. 在修改代码前更新本文件，说明将要修改的范围。
6. 完成实现后运行 `cargo fmt`。
7. 运行 `cargo clippy --all-targets -- -D warnings`，修复所有告警。
8. 运行相关测试；若需要完整验证，则运行完整测试套件并设置不超过 30 分钟的超时。
9. 根据验证结果更新 `TODO.md`：完成当前任务时在任务标题前加 `[DONE]`，并填写 completion record；如只修改了文档且可复用上一轮绿色全量测试，则记录跳过原因。
10. 检查 `git status`，将本次相关变更作为一个清晰的 Git commit 提交。
11. 停止，不处理下一个任务。

## 当前状态

- 状态：已读取 `TODO.md`，首个未完成任务为 `M5-1 send-keys / capture-pane 映射`。
- 最新提交：`47c3948 [M4-R] Review IPC control plane`，未明确提到与 M5-1 直接相关的未完成问题。
- 当前任务要求：在第 3 层协议 / server 侧提供 `send-keys`、`capture-pane`、`list-panes` 语义方法；映射到 `TerminalHandle::send_input_bytes`、`TerminalHandle::snapshot` 与 `TerminalPaneGroupHandle::{panes,active_pane,pane_at_screen_position}`；pane 寻址使用 `TerminalPaneId`；新增集成测试验证经第 3 层发送字节和抓取 pane 快照。
- 已完成关键步骤：
  - 核心 `src/protocol.rs` 新增 `send_keys` / `capture_pane` / `list_panes` 请求和对应成功响应类型，并补 JSON roundtrip 样例。
  - 核心 `src/ipc.rs` 新增可选 UI-thread 扩展分发器，未注册时对 pane 方法返回显式 `ActionNotSupported`。
  - `atto-ui-terminal` 新增 `TerminalPaneIpc` 映射模块，负责把 pane 协议方法映射到 `TerminalPaneGroupHandle` / `TerminalHandle`。
  - 新增 `crates/atto-ui-terminal/tests/ipc_pane.rs`，覆盖 list/capture 和真实子进程 send/capture 回显路径。
  - `cargo fmt --all -- --check` 通过。
  - `cargo clippy --workspace --all-targets -- -D warnings` 通过。
  - `python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets` 通过。
  - `TODO.md` 已将 M5-1 标记为 `[DONE]` 并补完成记录 / 验证记录。
  - 已创建 Git commit：`[M5-1] Add terminal pane IPC methods`。
- 下一步：停止，不处理 M5-2。

## M5-1 执行步骤

1. 阅读 `src/protocol.rs`、`src/ipc.rs`、`crates/atto-ui-terminal/src/terminal.rs`、`crates/atto-ui-terminal/src/pane.rs` 和现有 IPC / CLI 测试，确认第 3 层协议扩展点与终端 pane API 形状。
2. 找到 `TerminalHandle` / `TerminalPaneGroupHandle` 是否已能从 UI 线程或组件树中稳定寻址；如果缺少必要的公开桥接 API，优先补齐通用 API，不做任务私有 workaround。
3. 扩展协议数据结构，加入 `send-keys`、`capture-pane`、`list-panes` 请求 / 响应类型，并补充 JSON roundtrip 单测。
4. 扩展 server 分发逻辑，使请求在线程安全边界内落到 UI 线程，并映射到目标 pane 的终端 handle。
5. 增加集成测试，验证经协议发送输入到目标 pane 子进程、抓取目标 pane 快照、列出 pane 信息。
6. 运行 `cargo fmt --all`，再运行 clippy 与相关测试；最后按要求运行完整 workspace 测试。
7. 更新 `TODO.md` 的 M5-1 标题为 `[DONE]` 并填写完成记录与验证命令。
8. 检查变更并提交一个描述清晰的 Git commit，然后停止。
