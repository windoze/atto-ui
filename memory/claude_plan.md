# 执行计划

## 当前约束

- 以 `TODO.md` 为唯一任务顺序和完成状态来源。
- 只处理第一个标题未带 `[DONE]` 的任务，完成后停止。
- 不用变通方案绕过规格不匹配；如果遇到阻塞，更新 `TODO.md` 并提交后停止。
- 修改任务完成状态时必须在任务标题前加 `[DONE]`，并更新 completion record。
- 运行验证顺序为 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、完整测试。

## 步骤

1. 读取 `TODO.md`，找出第一个未完成任务及其验证要求。
2. 检查最新提交是否显式提到与该任务直接相关的未完成事项。
3. 阅读与该任务相关的代码、测试和文档，确认实现边界。
4. 实现当前任务；如发现直接阻塞当前任务的缺陷，先修复或把最小前置任务写入 `TODO.md` 后停止。
5. 按要求运行格式化、lint 和测试；发现未排期失败时修复或写入 `TODO.md`。
6. 更新 `TODO.md`：给当前任务标题加 `[DONE]`，补全 completion record。
7. 检查 git diff/status，提交本次所有相关变更。
8. 停止，不处理下一项任务。

## 进度记录

- 已创建本计划文件；下一步读取 `TODO.md` 识别第一个未完成任务。
- 已读取 `TODO.md` / `TODO-2.md`，第一个未完成任务是 `T10 — C4 MenuBar mnemonic/accelerator 与 Turbo Vision 绘制`。
- 已检查最近提交，未发现直接阻塞 T10 的未完成事项。
- 已检查工作区状态：`memory/claude_plan.md` 为本次新增；`notification.sh`、`run_agent.sh` 为既有未跟踪文件，本次不修改、不提交。
- 当前实现计划：扩展 `MenuItem` 的 `accelerator`/`mnemonic` API，统一 label mnemonic marker 解析，更新菜单输入与绘制，注册主题 named styles，迁移 editor app 菜单构建，补充单元和 PTY 测试。
- 已完成首轮实现编辑：菜单模型新增 accelerator/mnemonic，输入改为 mnemonic 优先，绘制隐藏 `&`/`_` marker 并右对齐 accelerator，主题注册 menu named styles，editor app 菜单迁移到 `.accelerator()` 与 marker title，补充单元和 PTY 测试。
- 下一步运行格式化和编译/lint，按失败结果修正。
- 已运行并通过：`cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test --all --all-targets`。
- 已将 T10 在 `TODO.md` / `TODO-2.md` 标记为 `[DONE]` 并补充完成记录。
- 复跑完整测试时观察到 `atto-ui-editor` 的 `pty_diff` 在并行负载下 3 秒初始等待偶发空屏超时；已将该 fixture 统一改为 5 秒 wait，并重新通过 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test --all --all-targets`。
- 下一步检查 git diff/status，确认只提交本次相关文件，然后提交并停止。
