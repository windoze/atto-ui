# Claude 执行计划

## 范围

- 以 `TODO.md` 作为权威任务列表。
- 识别标题未带 `[DONE]` 前缀的首个任务。
- 只完成该任务，提交后停止。
- 在选定当前任务前不做开放式历史问题排查。

## 分步计划

1. 读取 `TODO.md`，按标题前缀识别首个未完成任务。
2. 仅检查最近提交是否包含与该任务直接相关的未完成事项。
3. 查看理解并实现该任务所需的文件。
4. 若任务被具体前置条件阻塞，则在 `TODO.md` 增加最小前置任务，提交该记录变更后停止。
5. 若无阻塞，则以聚焦改动实现当前任务。
6. 先运行格式化，再运行 `cargo clippy --all-targets -- -D warnings`，然后按要求运行相关或完整测试。
7. 标记任务完成前，修复任何观察到且未被明确排期的测试或 fixture 失败。
8. 在 `TODO.md` 中把当前任务标题标记为 `[DONE]`，并更新完成记录与验证结果。
9. 关键步骤完成或计划变化时更新本文件。
10. 检查 git status、diff 和最近提交记录，再以描述性提交信息提交预期变更。
11. 停止，不开始下一项任务。

## 进展记录

- 已在读取任务详情前写入初始执行计划。
- 已从 `TODO-2.md` 确认首个未完成任务为 `T12 — C3 框架级多键序列 keymap engine`。
- 最近提交为 `[R11] Review segmented status bar`，未提到与 T12 直接相关的未完成事项。
- T12 实施范围：新增框架级 key sequence engine，保持现有 editor 单 chord API 不变，增加 editor/framework `KeyChord` 桥接，增加 label helper，并用单元测试覆盖 exact/prefix/ambiguous/timeout 行为。
- 已完成 T12 初始代码改动：新增 `src/app/keymap.rs`，从 `app` 和 crate root 导出 keymap API，并添加 editor/framework `KeyChord` 桥接 helper。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`。
- 文档已更新：`TODO-2.md` 中 `T12` 已标记 `[DONE]`，`TODO.md` 索引行已标记 `DONE`。
