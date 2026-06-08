# 执行计划

## 当前约束

- `TODO.md` 是任务顺序和完成状态的唯一依据。
- 只完成第一个标题未带 `[DONE]` 的任务，完成后提交并停止。
- 在确认当前任务前不做开放式历史问题清扫。
- 若遇到阻塞当前任务的缺陷、缺失能力或未排期失败测试，需要先修复，或在 `TODO.md` 中插入最小必要前置任务并提交后停止。
- 变更 `PLAN.md` 仅限阶段级计划、依赖或完成标准发生变化。
- 验证顺序为 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行相关/完整测试；完整测试使用足够超时。

## 步骤计划

1. 读取 `TODO.md`，定位第一个标题未以 `[DONE]` 标记的任务，并记录任务要求、依赖、验证要求和完成记录格式。
2. 查看最近提交信息，判断是否存在明确提到且直接影响该任务的未完成问题。
3. 针对当前任务读取最小必要的相关源码、测试和文档，避免无关范围扩张。
4. 如果任务可直接实现，按仓库既有结构做最小正确修改，并同步添加或更新相关测试。
5. 如果发现当前任务被具体未排期前置问题阻塞，更新 `TODO.md` 插入最小前置任务，必要时更新当前任务依赖说明，然后提交并停止。
6. 执行格式化、lint 和任务要求的测试；如发现未排期失败测试，修复或在 `TODO.md` 中排期后再决定是否完成当前任务。
7. 验证通过后，将当前任务标题标记为 `[DONE]`，填写完成记录；仅在阶段级计划变化时更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、最近提交，确认只提交本轮相关变更；创建清晰提交信息的 Git 提交。
9. 停止，不继续处理下一个任务。

## 进度记录

- 已创建本执行计划，下一步读取 `TODO.md` 定位第一个未完成任务。
- 已读取 `TODO.md`。第一个未完成任务是 `R10`：`审阅 T10`，来源为 `TODO-2.md · 阶段二`。下一步读取 `TODO-2.md` 中的 T10/R10 详情，并查看最近提交是否说明与 R10 直接相关的未完成问题。
- 已读取 `TODO-2.md` 的 T10/R10 条目。R10 要求审阅 shortcut 兼容、Unicode mnemonic 绘制列宽、dropdown 宽度计算、主题 named styles overlay 覆盖。最近提交 `a4d66af [T10] Implement menu mnemonics and accelerators` 未提到未完成问题。下一步检查相关实现和测试。
- 审阅发现一个与 R10 直接相关的兼容缺口：静态 `.shortcut("q")` 已设置 mnemonic，但旧公开 `shortcut` binding / `.shortcut_binding(...)` 的单字符动态路径不再参与 `handle_shortcut_char`，会改变旧调用方行为。计划修复为：explicit mnemonic/label marker 优先，其次单字符 `shortcut` fallback，最后 label 首字符 fallback；同时补充 dropdown arrow 宽度、Unicode 绘制列宽和 menu named style overlay 测试。
- 已修复动态单字符 `shortcut` 兼容 fallback，并补充 R10 覆盖测试：`shortcut_binding` legacy mnemonic、dropdown submenu arrow 宽度、Unicode mnemonic 绘制列宽、JSON/YAML menu named style overlay。下一步按要求运行 `cargo fmt`、clippy 和完整测试。
- 验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。下一步把 `R10` 标记为 `[DONE]`，填写完成记录，然后检查 diff 并提交。
