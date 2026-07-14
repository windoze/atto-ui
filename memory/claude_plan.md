# 执行计划

## 约束理解

- 本轮只处理 `TODO.md` 中第一个标题未以 `[DONE]` 开头的任务，完成后立即停止。
- `TODO.md` 是任务顺序、依赖、验证和完成记录的唯一权威来源；`PLAN.md` 只在阶段级计划变化时更新。
- 在确认当前任务前不做开放式历史问题扫查；只处理会阻塞当前任务、破坏当前任务行为或测试失败策略要求处理的问题。
- 如遇无法按原规格完成的具体阻塞，保留当前任务未完成，在 `TODO.md` 中插入最少必要前置任务，必要时更新依赖说明，提交后停止。
- 完成任务后必须更新 `TODO.md`，在任务标题前加 `[DONE]` 并填写完成记录，然后提交 Git。
- 验证顺序遵循要求：先 `cargo fmt`，再 `cargo clippy --all-targets -- -D warnings`，最后在需要时运行完整测试套件且超时不超过 30 分钟。
- 不使用规避、窄化范围或偏离规格的方式让任务看似完成。

## 初始步骤计划

1. 读取 `TODO.md`，按标题是否带 `[DONE]` 找出第一个未完成任务。
2. 检查最近一次提交信息；如果其中明确提到与当前任务直接相关的未完成问题，则把它纳入当前任务或作为前置任务记录到 `TODO.md`。
3. 读取当前任务相关的代码、测试和必要文档，避免无关历史扫查。
4. 根据任务要求实施最小但完整的代码或文档修改；如计划变化或完成关键步骤，及时更新本文件。
5. 运行格式化、lint 和相关测试；若发现未被后续任务明确排期的测试失败，按测试失败策略修复或插入前置任务。
6. 完成后更新 `TODO.md`：任务标题加 `[DONE]`，补充完成记录和验证结果。
7. 查看 Git 状态，提交本轮所有相关改动，提交信息包含任务编号和动作。
8. 停止，不处理下一个任务。

## 当前状态

- 已读取 `TODO.md`，第一个未完成任务是 `M1-4 变更信号聚合（为 M2 wait_for 预留）`。
- 最近提交为 `6ea183b [M1-3] Add untagged interactive diagnostics`，未发现与 M1-4 直接相关的未完成阻塞说明。
- 设计决策：新增基于 `DirtyObserver` 的拉模型 `DirtySignal` / `DirtySignalSet`，组件 trait 暴露 `dirty_signals()`，`DesktopInspector` 创建 `DesktopChangeTracker` 并提供 `changed_since_last_poll()`；不实现等待循环，不引入 push 订阅。
- 已实现 reactive 信号、组件 dirty 信号入口、宏自动生成、透明 wrapper 转发、DesktopInspector 聚合器和 M1-4 聚焦单测。下一步运行格式化与验证。
- 验证已通过：`cargo test -p atto-ui desktop_change_tracker -- --nocapture`、`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`（通过 Python 30 分钟 timeout 包裹）。
- 已更新 `TODO.md`，M1-4 标题改为 `[DONE]`，并补充完成记录与验证命令。下一步检查 diff/status 并提交。

## M1-4 任务执行计划

1. 阅读 `src/reactive/dirty.rs`、`Desktop` / `DesktopInspector` 定义和现有 dirty flag 使用点，确认聚合入口应挂载位置。
2. 设计只读/拉模型 API，优先保持第 1 层边界：只暴露 `changed_since_last_poll()` 之类原语，不实现等待循环、不引入 push 订阅。
3. 实现变更检测封装，聚合 desktop 关注的 dirty 信号；必要时让 `DesktopInspector` 创建该聚合器。
4. 新增单测覆盖：修改 `Binding` 后报告 changed；poll 后回落 false；clean 状态再次 poll 仍 false。
5. 运行 `cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
6. 更新 `TODO.md` 的 M1-4 标题为 `[DONE]` 并填写完成记录与验证命令。
7. 提交本轮改动后停止。
