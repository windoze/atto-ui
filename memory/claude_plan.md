# 执行计划

## 当前状态

- 本轮目标：只完成 `TODO.md` 中第一个未标记 `[DONE]` 的任务，然后停止。
- 约束：`TODO.md` 是任务顺序和完成状态的唯一权威来源；`PLAN.md` 仅在阶段级计划变化时更新。
- 说明：本文件记录可审查的执行计划、关键决策和进度；不会记录不可公开的内部推理细节。

## 步骤计划

1. 读取 `TODO.md`，按标题是否带有 `[DONE]` 判断第一个未完成任务。
2. 查看最近提交信息，只判断其是否明确提到与该任务直接相关的未完成问题。
3. 根据当前任务范围读取相关源码、测试和文档，避免开放式历史问题排查。
4. 完整实现当前任务；若发现必须先修复的具体前置问题，则把最小前置任务插入 `TODO.md` 并停止。
5. 按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后运行完整测试套件。
6. 若测试失败且未被明确排期，立即修复；无法在当前任务内修复时，将最小修复任务加入 `TODO.md` 并停止。
7. 完成后在 `TODO.md` 中给当前任务标题加 `[DONE]` 并填写完成记录；仅当阶段计划变化时更新 `PLAN.md`。
8. 提交所有本任务相关改动，提交信息包含任务编号和清晰说明。
9. 停止，不处理下一个任务。

## 进度记录

- 已创建初始计划，下一步读取 `TODO.md` 识别第一个未完成任务。
- 已识别第一个未完成任务：`M3-2 L1 DCS tmux; passthrough 解包 → 原生 OSC`。
- 下一步检查最近提交是否明确提到与 M3-2 直接相关的未完成问题，然后读取 terminal 输出解析、OSC 52、剪贴板后端和相关测试。
- 最近提交为 `[M3-1] Add tmux environment probe injection`，未明确留下与 M3-2 直接相关的未完成问题。
- 下一步重点读取 `crates/atto-ui-terminal/src/terminal.rs` 中 `vt100::Callbacks`、输出解析入口、OSC 52 剪贴板派发，以及 `callbacks.rs` 等现有测试。
- 方案确定：在 `TerminalShared` 增加 tmux DCS passthrough 解包状态；`TerminalHandle::process_output` 先将输入流通过解包器，完整 `ESC P tmux; ... ESC \` 包裹会把内部 `ESC ESC` 还原成 `ESC`，再交给现有 vt100 parser，因此 OSC 52 继续复用现有 `copy_to_clipboard` / 系统剪贴板后端。
- 降级策略：非 tmux DCS、缺少 `tmux;` 前缀、或已经判定不是目标包裹的内容不解包、不执行内部 OSC；跨 `process_output` 分片的完整 tmux DCS 会被状态机拼回后处理。
- 测试计划：新增 callbacks 测试覆盖 tmux DCS 包裹 OSC 52、跨分片包裹、畸形 / 非 tmux DCS 不写剪贴板；保留无包裹 OSC 52 现有回归。
- 实施调整：当前 vt100 parser 会在原始 DCS 内误执行嵌套 OSC，因此非 tmux DCS 与畸形 tmux DCS 改为作为不可执行控制串安全忽略到 ST，满足“不崩、不误写系统剪贴板”的降级要求；完整 tmux DCS 仍解包后交给原生 OSC 路径。
- 已完成：`TerminalHandle::process_output` 接入流式 `TmuxDcsPassthroughDecoder`；新增 callbacks 测试覆盖完整解包、跨分片解包、畸形包裹和非 tmux DCS 不触发剪贴板。
- 已通过聚焦验证：`cargo fmt --all`；`cargo test -p atto-ui-terminal tmux_dcs -- --nocapture`；`cargo test -p atto-ui-terminal -- --nocapture`。
- 下一步运行通用验收：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、完整 workspace 测试。
- 已通过通用验收：`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。
- 已完成：`TODO.md` 中 M3-2 已标记 `[DONE]` 并补充完成记录与验证命令。
- 已通过最终检查：`git diff --check`；待提交文件仅包括 `TODO.md`、`terminal.rs`、`callbacks.rs` 和本计划文件。
- 下一步提交本任务变更并停止。
