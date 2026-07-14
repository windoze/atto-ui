执行计划（初始）

当前约束：
- 使用中文记录进度和对用户输出。
- `TODO.md` 是任务顺序、完成状态和验收要求的唯一权威来源。
- 本轮只完成第一个标题未带 `[DONE]` 的任务，完成后提交并停止。
- 在修改代码前先理解当前任务、相关实现和测试；若发现当前任务被具体缺陷阻塞，优先修复阻塞项，或将最小必要前置任务写入 `TODO.md` 后提交并停止。
- 不做开放式历史问题扫描，不跳过 review 类任务，不用 workaround 代替规格实现。

步骤计划：
1. 读取 `TODO.md`，严格按标题 `[DONE]` 前缀识别第一个未完成任务。
2. 检查最新提交信息，仅在它明确提到与当前任务直接相关的未完成问题时纳入当前任务或写入前置任务。
3. 阅读当前任务涉及的计划、代码和测试上下文，确认需求、依赖和验收命令。
4. 如任务可直接实施，进行小范围、分步代码修改；修改前在本文件追加实际编辑计划。
5. 先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后运行完整测试套件（若仅文档变更且有可复用绿色结果，则按要求记录跳过原因）。
6. 若测试失败且未被后续任务明确安排，立即修复或在 `TODO.md` 中增加最小必要前置任务，并保持当前任务未完成。
7. 完成后更新 `TODO.md`：在任务标题前加 `[DONE]`，并更新 completion record；仅当阶段级计划变化时修改 `PLAN.md`。
8. 检查 git diff，提交本轮所有相关改动，提交信息包含任务编号和实际完成内容。
9. 停止，不继续处理下一项任务。

进度记录：
- 初始计划已创建，下一步读取 `TODO.md`。
- 已读取 `TODO.md`，第一个标题未带 `[DONE]` 的任务是 `M2-R Review — 第 2 层完整性与正确性复核`。
- 下一步检查最新提交信息是否明确提到与 M2-R 直接相关的未完成问题；随后复核 M2 实现与测试。
- 最新提交 `[M2-6] Migrate chat logic tests to semantic API` 未声明直接相关的未完成事项。
- 已复核 M2 关键实现：四个叶子组件的 `apply_command` 与键盘/鼠标路径复用同一状态函数；`invoke` 语义优先且返回可观测 dispatch；`query` 复用第 1 层属性读取；`wait_for` 使用进程内 `WaitCondition::PropertyEquals` 循环，不读取屏幕字符。
- 已复核 wrapper 转发与层级依赖：tag/border/visibility/runtime/min-size 等透明包装会转发 `supports_command` / `apply_command`；M2 代码未发现第 3/4 层依赖。下一步执行格式化、lint 和完整测试。
- 验证已通过：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。
- 下一步更新 `TODO.md`，将 `M2-R Review` 标记为 `[DONE]` 并写入复核结论与验证记录。
- 已更新 `TODO.md`：`M2-R Review` 标题改为 `[DONE]`，完成记录和验证记录已写入。下一步提交本轮改动。
