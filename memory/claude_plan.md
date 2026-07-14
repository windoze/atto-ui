# Claude Plan

## 执行约束
- 输出使用中文。
- 以 `TODO.md` 为权威任务来源，只完成第一个标题未带 `[DONE]` 的任务，然后停止。
- 不做开放式历史问题扫描；只处理当前任务直接相关、阻塞或测试失败策略要求处理的问题。
- 若遇到无法按规格完成的具体阻塞，更新 `TODO.md` 加入最小前置任务并提交后停止。
- 完成实现后按要求执行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行完整测试；若仅文档变更且可复用最近绿色结果，则记录跳过原因。
- 完成任务时更新 `TODO.md` 标题为 `[DONE]` 并填写 completion record，必要时才更新 `PLAN.md`。
- 最后提交本次任务涉及的全部未提交变更，不继续下一个任务。

## 当前步骤计划
1. 读取 `TODO.md`，识别第一个标题未带 `[DONE]` 的任务，并记录任务编号与要求。
2. 查看最新提交信息，确认是否有直接影响该任务的未完成事项。
3. 读取当前任务相关源码、测试和文档，限定范围内理解实现边界。
4. 如需修改代码，先更新本文件说明编辑方向，再做小步补丁。
5. 增加或调整聚焦测试，避免窄化规格或使用临时绕过。
6. 运行格式化、lint 和测试；发现未计划失败时按测试失败策略修复或排入 `TODO.md`。
7. 更新 `TODO.md` completion record 并给任务标题加 `[DONE]`。
8. 检查 git diff，提交清晰 commit，然后停止。

## 状态
- 2026-07-15：已读取 `TODO.md`，第一个未完成任务是 `M4-R Review — 第 3 层完整性与正确性复核`。
- 最新提交为 `[M4-3] Add atto IPC CLI client`，未发现直接点名的未完成事项；本轮范围限定为 M4 review。
- 已补充 IPC 边界测试，覆盖 `property_names` 成功路径、未知 tag、不支持自定义动作和无效协议 method 的 error 响应；`cargo fmt --all` 已执行。
- 验证已通过：`cargo test -p atto-ui ipc -- --nocapture`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。
- `TODO.md` 已将 `M4-R` 标记为 `[DONE]` 并补完成记录；`git diff --check` 已通过，下一步提交。

## M4-R 执行计划
1. 读取 M4 相关实现：`src/protocol.rs`、`src/ipc.rs`、`src/bin/atto.rs`、AppHost / runner 集成点与相关测试。
2. 复核协议 method 是否与第 2 层 `DesktopInspector` API 一一对应，确认没有重新设计语义或绕过 M2。
3. 复核 Unix socket 跨线程分发：请求只在 UI 线程持有 `Desktop` 时执行，pending `wait_for` 不阻塞其他请求。
4. 复核错误路径：未知 tag、不支持动作、畸形 JSON / 请求、channel 关闭等应映射为协议 error 或客户端错误，不 panic。
5. 复核 socket 路径策略是否可由环境变量指定，并为 M5 `$TMUX` socket 指向预留。
6. 已确认核心实现未见语义重写或线程分发缺陷；为支撑 review 的错误路径要求，补充 M4 IPC 边界测试，覆盖 `property_names`、未知 tag、不支持动作与畸形 JSON。
7. 按要求运行 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
8. 将 `M4-R` 标记为 `[DONE]`，记录复核结论和验证命令，检查 diff 后提交并停止。
