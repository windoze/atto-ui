# 执行计划

## 当前目标
- 按 `TODO.md` 的顺序识别并完成第一个标题未带 `[DONE]` 的任务。
- 完成后更新任务记录、运行要求的验证，并提交一次包含本任务相关改动的 Git commit。

## 步骤
1. 读取 `TODO.md`，只选择第一个未完成任务，不做开放式历史问题扫描。
2. 读取该任务相关的代码、测试和计划上下文，确认任务要求、依赖和验证命令。
3. 如遇到阻塞当前任务的具体缺陷或缺失能力，优先修复；若无法在本任务内正确修复，则在 `TODO.md` 中插入最小 prerequisite 任务并提交后停止。
4. 以最小正确改动实现当前任务，避免 workaround 或规格弱化。
5. 运行格式化、lint 和相关测试；若代码改动影响范围需要完整验证，则按要求运行完整测试。
6. 在 `TODO.md` 中将完成任务标题加 `[DONE]`，填写完成记录；仅当阶段计划实际变化时更新 `PLAN.md`。
7. 检查 Git 状态和差异，提交本任务相关全部改动，然后停止。

## 进度
- 已读取 `TODO.md` / `TODO-1.md`，首个未完成任务为 `NT1 — atto-ui-node crate 脚手架 + napi build（B.0）`。
- 最近提交为计划更新，未发现标题中直接指向 NT1 的未完成 issue。
- 已确认 `crates/atto-ui-node` 脚手架和 workspace 注册已有未提交实现；正在补齐与任务要求的细节并验证。
- 尝试使用 `#![forbid(unsafe_code)]` 时，`napi` 宏展开中的局部 `allow(unsafe_code)` 与之冲突，`cargo clippy` 报 E0453。
- 已按 NT1 允许的 napi-rs 冲突例外放宽为 `#![deny(unsafe_code)]` + `#![allow(unsafe_op_in_unsafe_fn)]`，继续保留 unsafe lint 保护。
- `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo build -p atto-ui-node`、`cargo test` 已通过。
- `npm exec --package=@napi-rs/cli -- napi build --platform` 首次失败，原因是 `package.json` 中 `napi.targets` 被写成对象；当前 CLI 期望数组或省略该字段。
- 已移除错误的 `targets` 对象，下一步重新运行 napi build 和 JS smoke。
- 重新运行 napi build 和 `node __test__/version.cjs` 已通过。
- 已将 JS smoke 改为 `require('..')`，覆盖 package 入口和生成的 napi loader；`package.json` 增加 `main` / `types` 指向生成文件。
- 重新运行 `node __test__/version.cjs` 已通过。
- 已在 `TODO-1.md` 将 NT1 标记为 `[DONE]` 并填写完成记录；已在 `TODO.md` 索引中将 NT1 状态改为 `DONE`。
- 下一步确认最终 diff、提交范围并创建 NT1 commit。
