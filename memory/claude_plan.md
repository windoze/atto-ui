# 执行计划

> 说明：本文件记录可审计的执行计划、关键决策和进度更新；不包含私有推理链。

## 初始步骤

1. 读取 `TODO.md`，按标题是否带 `[DONE]` 识别第一个未完成任务。
2. 检查最近提交是否明确提到与该任务直接相关的未完成问题。
3. 阅读当前任务的要求、依赖、验证要求和完成记录。
4. 仅围绕该任务检查相关代码与测试，不做开放式历史问题扫查。
5. 实现当前任务；如遇阻塞性前置问题，按要求更新 `TODO.md` 并停止。
6. 运行格式化、lint 和相关测试；若有未排期失败，修复或把最小必要任务加入 `TODO.md`。
7. 将任务标题标记为 `[DONE]` 并更新完成记录。
8. 检查变更，提交 Git commit，然后停止，不处理下一项任务。

## 当前状态

- 状态：已读取 `TODO.md`，第一个未完成任务为 `M5.6 副作用工具门控`。
- 最近提交：`[M5.5] Implement plan accept reject flow`，未声明与 M5.6 直接相关的未完成事项。
- 已定位：`ToolCallsReady` 在 `crates/atto-agent-app/src/lib.rs` 中进入 `prepare_tool_call`，当前只按工具权限审批；`stream_ui.rs` 在 plan turn 中对非 `submit_plan` tool call 直接报错。
- 执行计划更新：新增 per-turn mutating-tool gate；在 plan 未接受阶段把 mutating tool 转为 `ToolUseBlock` + 失败 `ToolResultBlock`，文本使用设计文档指定的 `Plan mode blocks mutating tools until the plan is accepted.`；项目级审批不得绕过该 gate。
- 进度更新：已实现 gate 并补充单测。首次 `cargo clippy --workspace --all-targets -- -D warnings` 发现测试常量导入、测试专用构造函数 dead code、启动函数参数过多问题；已改为导入常量、给测试构造函数加 `cfg(test)`，并用 `AgentTurnStartRequest` 封装启动参数。
- 进度更新：第二次 clippy 发现新增测试 helper 参数过多；已移除该 helper，并在 gate=false 单测内联 action 构造。
- 测试更新：完整 workspace 测试首次发现两个旧 PTY approval 用例仍按默认 `plan: auto` 期待审批；M5.6 后默认 auto 下 `run_command` 会被 gate 拦截。已将这两个审批用例显式切换 `/plan off` 后再验证审批路径。
- 验证结果：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、新增门控单测、`cargo test -p atto-agent-app --test pty_agent`、`cargo fmt --all -- --check`、`cargo test --workspace --all-targets` 均已通过。
- TODO 更新：`M5.6 副作用工具门控` 已在 `TODO.md` 中标记 `[DONE]` 并填写完成记录。
- 下一步：检查 git diff/status，确认仅包含本任务相关变更，然后提交。
