# 执行计划

日期：2026-06-07

## 当前目标

根据 `TODO.md` 的权威顺序，只完成第一个标题未带 `[DONE]` 的任务，完成后更新任务记录、验证、提交 Git，然后停止。

## 执行步骤

1. 读取 `TODO.md`，定位第一个未完成任务，并检查该任务的要求、依赖、验证标准和完成记录。
2. 查看最近提交信息，判断是否明确提到与当前任务直接相关的未完成问题；如有，将其纳入当前任务或作为必要前置项记录到 `TODO.md`。
3. 针对当前任务读取相关代码、测试和文档，限定在完成该任务所需范围内，不做无关历史问题扫查。
4. 如任务可直接完成，实施最小正确代码/文档修改；如发现阻塞当前任务的具体缺口或规格不匹配，则在 `TODO.md` 插入最小前置任务并停止。
5. 按要求运行格式化、lint 和相关测试；若代码有改动，优先运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行必要测试，完整 Rust 测试套件使用不少于 30 分钟超时。
6. 如发现未计划的测试或 fixture 失败，修复它，或在 `TODO.md` 中加入按顺序执行的前置/后续任务，且不在失败未处理时将当前任务标记为完成。
7. 完成任务后，将 `TODO.md` 中该任务标题加 `[DONE]`，更新完成记录；仅当阶段计划真实变化时更新 `PLAN.md`。
8. 检查 `git status`、`git diff` 和最近提交，确认只提交目标相关文件；使用清晰提交信息提交本次任务。
9. 提交后停止，不继续处理下一个任务。

## 进度记录

- 已写入初始执行计划。下一步读取 `TODO.md` 以确认第一个未完成任务。
- 已确认第一个未完成任务为 `NR2 — 审阅 NT2`，完整要求位于 `TODO-1.md`：检查 `TreeOp` / `ComponentValue` 覆盖、错误清晰性，并运行转换单测。
- 最近提交为 `[NT2] Add serde conversion layer`，与当前审阅任务直接相关，未在提交标题中注明额外未完成事项。
- 下一步审阅 `crates/atto-ui-node/src/convert.rs` 及核心 spec 定义，必要时修复审阅发现的问题。
- 审阅发现 `ComponentValue` 的 plain JSON 表示在若干核心分支上无法稳定 round-trip：空 `StringList` / `Table`、只含字符串或字符串数组的 `List`、矩形形状的 `Map` 等会被重新解析成其他分支；此外 `Rect` 的 `[x,y,width,height]` JS 输入尚未支持。
- 当前修复计划：为歧义分支增加 `$type` + `data` 逃逸格式，保留非歧义普通 JS 数组/对象形态；补充矩形数组输入解析；扩展单测覆盖这些审阅发现。
- 已实施转换层修复：歧义分支使用 `$type` + `data` 逃逸，普通非歧义数组/对象仍保持 JS 形态；去除转换路径中的 `expect`；新增空/歧义分支 round-trip、矩形数组输入和错误上下文测试。
- 下一步执行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings` 和 Rust 测试验证。
- 首次 `cargo clippy --workspace --all-targets -- -D warnings` 发现 `type_tag` 存在 needless lifetime；已按 lint 建议移除显式生命周期，准备重新格式化并复跑 clippy。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。
- 下一步更新 `TODO.md` / `TODO-1.md`，将 `NR2` 标记为 `[DONE]` 并记录审阅修复与验证结果。
- 已更新 `TODO.md` / `TODO-1.md`，`NR2` 已标记完成并写入完成记录；`PLAN.md` 无阶段级变化，未更新。
- 提交前检查发现工作区另有未跟踪 `notification.sh`、`run_agent.sh`，与本任务无关，将不纳入提交。
