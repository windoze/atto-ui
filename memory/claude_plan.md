# 执行计划

## 当前状态

- 已读取 `TODO.md`，第一个未完成任务是 `R1 — 审阅 T1`。
- 最近提交为 `[T1] Complete test host input APIs`，与当前审阅任务直接相关。
- 已检查 T1 主要改动文件，新增输入 API 与 `snapshot_app --input-api` 测试夹具覆盖 R1 审阅范围。
- 已完成验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets` 全部通过。
- 本文件用于记录可审阅的执行计划、关键进度和必要调整；不记录私有推理链。

## 步骤

1. 阅读 T1 提交涉及的 test-host、snapshot_app 和 PTY 测试文件。
2. 对照 R1 要求检查：新增输入 API 的 crossterm 编码、`resize` 对 PTY 与 vt100 的尺寸同步、快照归一逻辑、`wait_for_screen` 轮询行为。
3. 如发现影响 R1 验收的缺陷，优先修复并补充/调整测试；如出现必须前置处理的问题，则更新 `TODO.md` 后停止。
4. 运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test`。
5. 若验证通过，在 `TODO.md` 将 `R1` 标记为 `[DONE]` 并补充完成记录。
6. 检查 git 状态、差异和近期日志，提交本次 R1 相关全部变更。
7. 停止，不继续处理 `T2`。

## 验证策略

- 先运行 `cargo fmt`。
- 再运行 `cargo clippy --all-targets -- -D warnings`。
- 最后根据任务范围运行相关测试；如需要完整验证，运行 `cargo test --all --all-targets` 并使用足够长的超时。
