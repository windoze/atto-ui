## 执行计划

状态：T5 已实现、验证并更新 TODO，准备提交。

说明：本文件记录可检查的执行计划、关键步骤和进度更新；不包含私有推理过程。

步骤：
1. 读取 `TODO.md`，按标题是否带 `[DONE]` 找出第一个未完成任务。当前任务：`T5 — Python e2e 测试 host 雏形`。
2. 检查最新提交是否明确提到与该任务直接相关的未完成事项。最新提交为 `[R4] Review desktop snapshot export`，未包含未完成事项说明。
3. 阅读当前任务相关代码、测试和计划文件，只收集完成该任务所需上下文。发现 `crates/atto-ui-python` 已有 PyO3 绑定和高层 Python wrapper，但尚未暴露 T3/T4 的 `send_event`、窗口管理、`set_property`、`snapshot()`，且测试需要可显式使用 headless `AppHost`。
4. 按任务要求实现最小正确变更；如遇阻塞的规格缺口，更新 `TODO.md` 添加最小前置任务并停止。当前计划是在 Rust 绑定层新增 headless 构造与事件/窗口/snapshot 转换，在 Python wrapper 层提供 e2e 友好 API，并新增 8 个不依赖 PTY 的 `unittest` 用例。
5. 运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行相关或完整测试；发现未排期失败时修复或写入 `TODO.md`。当前 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-python`、`maturin develop`、`python -m unittest discover tests` 均已通过；第一次 Python e2e 暴露 snapshot 子组件 bounds 需要按组件层级换算为绝对点击坐标，已在测试 helper 中处理。
6. 完成后在 `TODO.md` 的任务标题前加 `[DONE]`，更新完成记录；仅在阶段计划实际变化时更新 `PLAN.md`。已将 `T5` 标记为 `[DONE]` 并写入完成记录；本次无阶段计划变化，未更新 `PLAN.md`。
7. 提交所有本次任务相关变更，提交信息包含任务编号，然后停止，不进入下一个任务。下一步执行提交前的 git 状态、diff 和最近提交检查。
