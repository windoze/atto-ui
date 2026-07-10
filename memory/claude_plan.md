# 执行计划

> 说明：本文件记录可审计的执行计划、关键决策和进度更新；不包含私有推理链。

## 范围

- 以 `TODO.md` 为任务顺序和验收要求的唯一来源。
- 本轮只完成第一个未标记 `[DONE]` 的任务。
- 完成或阻塞当前任务后停止，不处理后续任务。
- 不在选择当前任务前做开放式历史问题排查。

## 当前任务

- 第一个未完成任务：`M5.R Review`。
- 任务要求：复核 plan mode 不依赖模型特殊能力，副作用门控不可绕过，验证通过。
- 最近提交：`[M5.7] Add plan mode PTY coverage`，未声明与 `M5.R Review` 直接相关的未完成事项。

## 执行步骤

1. 读取 `TODO.md`，确认第一个标题未标记 `[DONE]` 的任务。
2. 检查最近提交是否有与当前任务直接相关的未完成事项。
3. 只审查 M5 plan mode 相关实现、测试和请求构造路径。
4. 确认 plan 判定为本地 deterministic 逻辑，plan 草稿请求只暴露虚拟 `submit_plan`，并保留 markdown fallback。
5. 确认 `apply_patch`、`run_command` 等 mutating tool 在计划接受前由 app 层 gate 优先拦截，且项目级授权不能绕过。
6. 运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
7. 验证通过后，将 `M5.R Review` 在 `TODO.md` 标记为 `[DONE]` 并填写完成记录。
8. 检查 diff/status/log，提交本轮相关变更，然后停止。

## 进度

- 已读取 `TODO.md` 并确认当前任务为 `M5.R Review`。
- 已检查最近提交，无直接相关未完成事项。
- 已复核本地 plan decision、虚拟 `submit_plan`、markdown fallback、PlanBlock Accept/Reject 流程和 mutating-tool gate 顺序。
- 未发现阻塞 M5.R 的问题：plan mode 不依赖模型特有能力，副作用工具 gate 先于权限策略执行，项目级授权不能绕过 gate。
- `cargo fmt --all` 已通过。
- `cargo clippy --workspace --all-targets -- -D warnings` 已通过。
- `cargo test --workspace --all-targets` 已通过。
- `cargo fmt --all -- --check` 已通过。
- `TODO.md` 已将 `M5.R Review` 标记为 `[DONE]` 并记录完成情况和验证命令。
- 下一步：提交 `TODO.md` 和 `memory/claude_plan.md`。
