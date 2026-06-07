# 执行计划

本文件记录本次调用的可执行计划与进度。内容为简明依据和操作步骤，不包含不可公开的内部推理。

## 本次调用计划

1. 先读取 `TODO.md`，识别第一个标题未带 `[DONE]` 或索引状态未完成的任务。
2. 仅检查最近提交中与所选任务直接相关的未完成上下文。
3. 阅读该任务的要求、依赖、验证命令和完成记录格式。
4. 只执行该任务；除非出现必须插入 `TODO.md` 的具体前置阻塞项。
5. 按要求顺序验证：`cargo fmt`，再 `cargo clippy --workspace --all-targets -- -D warnings`，再运行相关/完整测试。
6. 完成后更新 `TODO.md`/详细任务文件，将任务标记为完成并记录验证结果；如遇阻塞则记录前置任务且不标记完成。
7. 仅当阶段级顺序或验收标准变化时更新 `PLAN.md`。
8. 检查 git status/diff/log，然后用清晰的任务消息提交本次相关改动。
9. 完成一个任务后停止。

## 进度记录

- 已在读取项目任务文件前初始化计划。
- 已读取 `TODO.md`；权威索引中的首个未完成任务是 `NR1`（`审阅 NT1`）。下一步读取 `TODO-1.md` 的具体审阅范围，并只检查与 `NR1`/`NT1` 直接相关的最近提交。
- 已读取 `TODO-1.md`；`NR1` 要求审阅 NT1 Node napi 脚手架，确认 workspace/依赖选择，证明 native `.node` 可被 require，并运行 `cargo build --workspace` 与 JS 冒烟。最近提交为 `[NT1] Add Node napi binding scaffold`，与本审阅直接相关。
- 已检查 Node crate 脚手架：`crates/atto-ui-node` 是 `cdylib`，使用带 `serde-json` feature 的 `napi`，包含 `napi-build`，暴露 `version()`，包含生成的 JS/类型入口，并已加入根 workspace。下一步按要求验证。
- 已通过验证：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo build --workspace`、`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`、`node __test__/version.cjs`。接下来运行完整 Rust 测试套件。
- 完整验证 `cargo test --all --all-targets` 已通过。已在 `TODO.md` 和 `TODO-1.md` 标记 `NR1` 完成；阶段级顺序未变化，因此无需更新 `PLAN.md`。
