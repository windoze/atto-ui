# 当前执行计划

## 约束
- 以 `TODO.md` 作为任务顺序、任务要求、依赖和完成状态的唯一来源。
- 本次只处理第一个标题未以 `[DONE]` 开头的任务，完成后停止。
- 不做开放式历史问题扫描；只处理会阻塞当前任务、使当前任务行为无效，或由当前任务引入的直接回归。
- 若发现未被明确排期的测试失败，必须修复，或在 `TODO.md` 中加入最小必要的前置/后续任务，且不能把当前任务标为完成。
- 修改代码后按顺序运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、完整测试；如果仅文档变更且已有可复用的绿色结果，则记录跳过原因。
- 完成或因阻塞调整任务列表后，提交 Git commit。

## 步骤计划
1. 读取 `TODO.md`，定位第一个标题未标记 `[DONE]` 的任务，并记录任务要求、验证要求和依赖。
2. 检查最近提交和当前工作区状态，判断是否存在与当前任务直接相关的未完成问题或已有未提交变更；不回退用户已有变更。
3. 按任务要求检查相关代码与测试，确定最小正确实现路径。
4. 实现当前任务；若发现具体阻塞且无法在当前任务内正确完成，则更新 `TODO.md` 增加最小必要前置任务，保持当前任务未完成并停止。
5. 为实现补充或更新测试，覆盖任务指定行为和相关边界。
6. 运行格式化、lint 和必要测试；若有失败，按测试失败策略处理。
7. 更新 `TODO.md`：给完成任务标题加 `[DONE]`，填写 completion record；仅当阶段级计划改变时更新 `PLAN.md`。
8. 检查 diff、状态和最近提交，确认只提交本次任务相关内容。
9. 创建清晰的 Git commit，然后停止，不处理下一项任务。

## 进度记录
- 已创建本执行计划，下一步读取 `TODO.md` 定位当前任务。
- 已定位第一个未完成任务：`M3.8 快照与测试`。任务要求：PTY 覆盖 allow、deny、tool result；单测覆盖非法参数、路径越界、工具不存在。下一步检查最近提交、工作区状态和相关测试/实现现状。
- 已检查相关实现和测试：现有 app 单测覆盖审批状态和 read_file 执行结果，工具模块已有多数非法参数/路径越界覆盖，但缺少 app 层工具不存在单测，以及 agent app 自身的审批/tool result PTY 场景。执行路径调整为：扩展 snapshot mock provider 的确定性 prompt 触发工具调用，再补充 PTY 与缺失单测。
- 已完成初版修改：snapshot fixture workspace 固定到 app crate 根目录；mock provider 对 `agent-pty-read-file` 生成 `read_file` tool call，对 `agent-pty-run-command` 生成 `run_command` tool call；新增 app 层未注册工具单测；新增三个 agent PTY 场景覆盖自动 tool result、allow once 和 deny。下一步运行格式化和针对性测试。
- 针对性单测已通过；首次 `pty_agent` 运行中 allow 场景通过，read_file/deny 场景因断言使用 Markdown 原文反引号而失败，实际屏幕渲染会去掉反引号并重排文本。已将断言改为屏幕可见文本，准备重新运行 PTY 测试。
- 重新运行 `cargo test -p atto-agent-app --test pty_agent` 后 6 个 PTY 测试全部通过。下一步重新运行 `cargo fmt --all`、workspace clippy 和完整 workspace 测试。
- 最终验证已通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。`TODO.md` 已将 `M3.8` 标记为 `[DONE]` 并写入完成记录。下一步检查 diff/status/log 后提交。
