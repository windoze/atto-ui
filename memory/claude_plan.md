# 当前执行计划

说明：本文件记录可公开的执行计划、关键进展和验证结果；不包含私有推理过程。

1. 读取 `TODO.md`，按文件顺序识别第一个标题未带 `[DONE]` 的任务。
2. 阅读该任务的要求、依赖、验证标准和完成记录；必要时查看 `PLAN.md`、最近提交和相关源码以确认上下文。
3. 若发现当前任务被具体阻塞，按要求在 `TODO.md` 中加入最小必要的前置任务并停止；否则完整实现当前任务。
4. 运行格式化、lint 和相关测试；若出现未被明确排期的失败，修复失败或在 `TODO.md` 中加入正确顺序的任务。
5. 将完成的任务标题加上 `[DONE]`，更新完成记录；仅在阶段计划实际改变时更新 `PLAN.md`。
6. 检查 git 状态和差异，提交本次任务相关的全部变更，然后停止，不处理下一项任务。

## 进展日志

- 已创建初始执行计划，下一步读取 `TODO.md` 识别第一个未完成任务。
- 已读取 `TODO.md`，第一个未完成任务是 `M3.4 副作用工具`：实现 `apply_patch`、`run_command`，默认需要审批，命令使用 argv，不做 shell 字符串拼接。
- 下一步将检查该任务相关模块、计划说明和最近提交，避免做开放式历史问题扫描。
- 已检查最近提交 `[M3.3] Add read-only tools`、`PLAN.md` 和现有 `tool`/`readonly` 模块；未发现需要插入的直接前置任务。
- 实施方向：新增 focused 的副作用工具模块，注册 `apply_patch` 和 `run_command`，复用 workspace 路径约束和严格 JSON 参数校验；默认权限为需要审批，`run_command` 仅接受 argv 数组并用 `std::process::Command` 执行。
- 已将参数解析和 workspace 路径展示/校验提取为 `tool` 模块内部共享逻辑，供只读与副作用工具复用。
- `apply_patch` 计划先校验 patch 中声明的路径，再通过 `git apply --check` 与 `git apply` 的 argv 调用应用 stdin patch；`run_command` 计划通过 `Command::new(argv[0]).args(&argv[1..])` 执行，不经过 shell。
- 已实现 `apply_patch` 与 `run_command` 副作用工具，并把完整内置工具注册表接入 `/tools` 输出。
- 根据 `TUI_AGENT.md` 复核，已补充 `apply_patch` 的文本文件边界：拒绝二进制 patch 和非 UTF-8 既有文件；symlink 允许解析后仍位于 workspace 内，逃逸则拒绝。
- 已将 `TODO.md` 中 M3.4 标记为 `[DONE]` 并写入完成记录。
- 最终验证通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 下一步检查 git 状态/差异并提交本次任务。
