# 执行计划

## 当前约束

- 先读取 `TODO.md`，按文件中的顺序选择第一个标题未带 `[DONE]` 的任务。
- 本轮只完成一个任务；完成后更新 `TODO.md` 并提交，然后停止。
- 不做开放式历史问题扫描；只处理当前任务范围内的阻塞问题，或测试中发现且未被明确排期的失败。
- 如果遇到无法按规格完成的具体阻塞，更新 `TODO.md` 增加最小必要前置任务，保持当前任务未完成，提交后停止。
- `PLAN.md` 只在阶段级计划、依赖或完成标准变化时更新。

## 步骤

1. 读取 `TODO.md`，识别第一个未完成任务及其验证要求、依赖和完成记录格式。
2. 查看最近提交信息，判断是否明确提到与该任务直接相关的未完成问题；若有，将其纳入当前任务或作为前置项记录到 `TODO.md`。
3. 按任务内容最小范围阅读相关代码、测试和文档，确认需要修改的位置。
4. 实现当前任务；若需要编辑文件，使用小而聚焦的补丁分步修改。
5. 根据任务要求补充或更新测试，并先运行 `cargo fmt`。
6. 运行 `cargo clippy --all-targets -- -D warnings`，修复所有警告。
7. 运行相关测试；若代码变更需要完整验证，再运行完整测试套件并使用足够长的超时。
8. 若发现未排期的测试或夹具失败，修复它，或在 `TODO.md` 中增加最小必要前置/后续任务并按规则停止。
9. 更新 `TODO.md`：给完成任务标题加 `[DONE]`，填写完成记录、验证结果和必要说明。
10. 如执行过程中计划或关键状态变化，更新本文件记录进展。
11. 提交所有与本任务相关的变更，提交信息包含任务编号和简明说明。
12. 停止，不继续处理下一个任务。

## 当前状态

- 已写入初始执行计划。
- 已从 `TODO.md` 确认本轮任务为 `T4 — C2 Dock resize / auto-hide / hit-test`，详细要求位于 `TODO-2.md`。
- 最近提交为 `[R3] Review docking layout`，未发现需要先处理的相关未完成问题。
- 已阅读 T4 涉及的 WM docking、hit-test、事件、绘制和测试代码。
- 实施要点：新增 dock resize/handle hit region；dock resize 只写 `WindowDock.size`；auto-hide hidden 只保留 handle reserve，visible 状态按 overlay 处理并阻止事件穿透；绘制 handle；补充 WM 单测。
- 已完成实现并新增 WM 单测。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。
- 已将 T4 在 `TODO.md` / `TODO-2.md` 标记为 `[DONE]` 并写入完成记录。
- 下一步：检查工作区 diff，然后提交本轮变更。
