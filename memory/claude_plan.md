# 执行计划

## 约束摘要

- 本次只处理 `TODO.md` 中第一个标题未以 `[DONE]` 标记的任务。
- `TODO.md` 是任务顺序、依赖、验证要求和完成记录的唯一权威来源。
- 如果发现阻塞当前任务的具体前置问题，先修复；若无法在本次完成，则把最小必要前置任务插入 `TODO.md` 并停止。
- 不做开放式历史问题排查；只处理与当前任务、验证失败或明确阻塞相关的问题。
- 完成后需要更新 `TODO.md`，运行格式化、lint、相关测试和必要的完整测试，并提交 Git 提交，然后停止。

## 初始执行步骤

1. 读取 `TODO.md`，按文件顺序定位第一个标题未带 `[DONE]` 的任务。
2. 检查该任务的要求、依赖、验证标准和完成记录；必要时查看最新提交是否明确提到与该任务直接相关的未完成问题。
3. 基于任务内容读取最小必要代码和测试上下文，避免无关的历史问题扫查。
4. 如果任务可直接实现，进行最小正确代码修改并添加或更新对应测试。
5. 若遇到必须先解决的实现缺口、规格不匹配或未调度测试失败，优先修复；若无法直接修复，则在 `TODO.md` 中新增最小前置任务并停止。
6. 按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后运行当前任务要求的测试；如需要完整验证，则运行完整测试套件并设置足够超时。
7. 验证通过后，将当前任务标题加上 `[DONE]`，补充完成记录；仅当阶段级计划确实变化时才更新 `PLAN.md`。
8. 检查 Git 状态和差异，确认只提交本次相关改动；按任务编号写清晰提交信息并提交。
9. 停止，不继续下一个任务。

## 当前状态

- 已读取 `TODO.md`，第一个未完成任务是 `R12 — 审阅 T12`。
- 已读取 `TODO-2.md` 中 `T12/R12` 要求；最新提交为 `[T12] Implement key sequence engine`，未包含显式未完成事项。
- 已审阅 `src/app/keymap.rs`、framework re-export 和 `crates/atto-ui-editor/src/keymap.rs` 桥接。
- 发现并修复 timeout 语义问题：多段 key sequence 在成功推进 prefix 后应重新开始等待下一段 chord，而不是从首个 prefix chord 起累计超时。
- 已补充回归测试，覆盖多段 prefix timeout 重置，以及 `KeyModifiers` 按 crossterm bitset 精确匹配。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。
- 已更新 `TODO.md` 和 `TODO-2.md`，将 `R12` 标记为 `[DONE]` 并写入完成记录。
- 下一步：检查 Git 状态和差异，提交本次任务改动。
