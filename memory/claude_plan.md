# 当前执行计划

## 约束

- 以 `TODO.md` 为唯一任务顺序与完成状态来源。
- 只处理第一个标题未带 `[DONE]` 的任务，完成后提交并停止。
- 不做开放式历史问题清扫；只处理当前任务的直接依赖、阻塞项或测试失败政策要求的问题。
- 若遇到无法按规格完成的阻塞项，只添加最小必要前置任务并提交，不继续绕过。
- 计划文件只记录可审查的执行步骤和进度，不记录私有推理细节。

## 步骤

1. 读取 `TODO.md`，确认第一个未完成任务及其验证要求。
2. 查看最近提交是否明确提到与该任务直接相关的未完成问题。
3. 针对该任务读取必要代码与文档，确认实现范围。
4. 如任务可直接执行，进行最小正确实现；如存在具体阻塞，更新 `TODO.md` 添加最小前置任务并停止。
5. 按要求更新测试或文档。
6. 运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再按需要运行完整测试套件。
7. 若验证通过，在 `TODO.md` 中给任务标题加 `[DONE]` 并填写完成记录。
8. 检查 `git status`、`git diff`、`git log --oneline -10`，提交本次任务相关变更。

## 进度

- 已建立初始执行计划。
- 已读取 `TODO.md`，当前第一个未完成任务为收尾阶段 `Docs 更新`：根据实际实现更新 `TUI_AGENT.md`、`README.md` 或新增 app README。
- 已检查最近提交 `[M6.R] Complete context session review`，未发现直接相关的未完成事项。
- 已对照 app crate 实现，准备更新根 README、`TUI_AGENT.md`，并新增 `crates/atto-agent-app/README.md`。
- 已完成文档主体更新：根 README 增加 agent app 入口，新增 app README，`TUI_AGENT.md` 修正当前实现状态、模块表、命令、配置、plan/context/compact 和后续扩展点。
- 已将 `TODO.md` 中 `Docs 更新` 标记为 `[DONE]` 并补充完成记录；本次仅文档/记录变更，验证复用上一轮 M6.R 的绿色完整结果。
