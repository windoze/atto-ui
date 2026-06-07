# Claude 执行计划

本文件记录本次调用的可审计执行计划、关键进度和验证结果；不记录私有推理链。

## 范围

- 以 `TODO.md` 为任务顺序、任务要求和完成状态的权威来源。
- 本次只完成首个未完成任务，完成后提交并停止。
- 在识别当前任务前不做开放式历史问题排查。
- 如遇到阻塞当前任务的缺陷或未计划的失败测试，优先修复；无法在本任务内修复时，向 `TODO.md` 添加最小前置任务并停止。

## 执行步骤

1. 读取 `TODO.md`，找到标题未带 `[DONE]` 的首个任务。
2. 仅检查最新提交是否明确提到与该任务直接相关的未完成问题。
3. 读取当前任务在 `TODO-1.md` / `PLAN-1.md` 中的详细要求、依赖和验收标准。
4. 只检查与当前任务相关的 npm/Rust/TS 包配置和 native 加载路径。
5. 用最小改动实现 npm 平台矩阵、平台子包和主包依赖配置。
6. 运行 `cargo fmt`。
7. 运行 `cargo clippy --workspace --all-targets -- -D warnings`。
8. 运行完整 Rust 测试、相关 TS/JS 验证、napi 本地构建和 npm pack dry-run。
9. 在 `TODO.md` 和 `TODO-1.md` 中只将当前任务标记为 `[DONE]` 并写入完成记录。
10. 关键步骤完成或计划变化时更新本文件。
11. 提交前检查 `git status`、`git diff` 和最近提交。
12. 只暂存并提交本次任务相关文件。
13. 停止，不开始下一项任务。

## 进度记录

- 已在读取任务详情前初始化本计划文件。
- 已识别首个未完成任务：`NT19 — 跨平台预编译 + npm 包（P.1 / P.2）`。
- 最新提交 `eed389f [NR18] Review React streaming example` 未指向与 `NT19` 直接相关的未完成前置问题。
- 已为 `@atto-ui/node`、`@atto-ui/core`、`@atto-ui/react` 添加 npm 发布元数据；已新增 `@atto-ui/node-*` 四个平台子包目录。
- `napi create-npm-dirs --dry-run` 已确认平台矩阵可被 `@napi-rs/cli` 解析；`npm pack --dry-run` 已确认 `@atto-ui/node`、`@atto-ui/core`、`@atto-ui/react` 主包结构。
- 已完成验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`、Node native 测试、core/react TS 检查和 JS 测试、darwin-arm64/darwin-x64 本地 napi 构建、artifact copy、平台包 pack dry-run、`git diff --check` 均通过。
- 已在 `TODO.md` 和 `TODO-1.md` 只将 `NT19` 标记为 `[DONE]`；`NR19` 保持为下一项未完成任务。
