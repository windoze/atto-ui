# 当前执行计划

## 约束
- 以 `TODO.md` 为任务顺序和完成状态的唯一依据。
- 只处理第一个标题未带 `[DONE]` 的任务，完成后停止。
- 如遇阻塞当前任务的具体前置问题，只添加最小前置任务并提交，不继续绕过。
- 代码变更后按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后运行完整测试。
- 完成时更新 `TODO.md` 标题和完成记录，并提交全部相关变更。

## 步骤
1. 读取 `TODO.md`，定位第一个未完成任务，并检查其依赖、验证要求和完成记录。
2. 检查最近提交是否明确提到与当前任务直接相关的未完成问题。
3. 针对当前任务读取相关源码、测试和计划文档，避免开放式历史问题扫描。
4. 实现当前任务要求；如发现直接阻塞任务的规格不匹配或缺失能力，优先修复，或在 `TODO.md` 中插入最小前置任务后停止。
5. 添加或调整覆盖当前任务行为的测试。
6. 运行格式化、lint 和测试；发现未排期失败时修复或排入 `TODO.md`。
7. 将当前任务标题标记为 `[DONE]`，更新完成记录。
8. 检查 git 状态和差异，提交本次任务的全部相关文件，然后停止。

## 进度
- 已读取 `TODO.md`；第一个未完成任务为 `M1.1 新建 crates/atto-agent-app`。
- 当前任务目标：创建 `crates/atto-agent-app`，加入 workspace，配置对 `atto-ui`、`atto-ui-chat`、`atto-ui-async` 的依赖，并确认核心 crate 不新增网络依赖。
- 已检查 workspace 配置、现有 app crate 风格和最近提交；未发现与当前任务直接相关的未完成提交问题。
- 已新增 `crates/atto-agent-app`，注册 workspace 成员，并添加最小 `run`/`main` 骨架。
- 验证已通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 已将 `TODO.md` 中 `M1.1` 标记为 `[DONE]` 并写入完成记录。
- 已检查 git 状态、差异和最近提交；下一步提交本次任务变更。
