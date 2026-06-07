# 执行计划

本文件记录本轮执行计划与进度。为避免暴露不可审计的内部推理，这里记录可执行步骤、决策依据与验证结果。

1. 读取 `TODO.md`，按标题是否带 `[DONE]` 判断第一个未完成任务。
2. 检查最近提交是否明确提到与该任务直接相关的未完成问题；只处理会阻塞当前任务的内容。
3. 阅读当前任务涉及的代码、测试与文档，确认依赖、完成标准与验证要求。
4. 如任务可直接完成，进行最小且完整的实现；如存在必须先修复的具体阻塞项，则更新 `TODO.md` 插入最小前置任务并停止。
5. 按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后运行相关或完整测试；若发现未排期的失败测试，修复或在 `TODO.md` 中排期。
6. 更新 `TODO.md`：把当前完成任务标题加上 `[DONE]`，填写完成记录；仅在阶段计划变化时更新 `PLAN.md`。
7. 检查 git 状态与差异，提交本轮所有相关改动，然后停止，不继续下一个任务。

## 进度

- 已写入初始执行计划。
- 已读取 `TODO.md` 与最近提交，确定第一个未完成任务为 `NT9 — props/子节点增删/事件 op 映射（U.1）`；最近提交 `NR8` 未声明与 `NT9` 直接相关的未完成问题。
- 下一步读取 `packages/react/src/host.ts`、`packages/react/src/reconciler.ts` 与现有 reconciler 测试，确认当前静态提交模型后实现 props diff、事件 bind/clear、子节点增删/重排 op 缓冲与 flush。
- 已修改 React host/reconciler：容器增加 pending op 缓冲，初始/根变更仍使用 `set_tree`，后续 props/text 更新使用 `set_prop`，子节点插入/重排使用 `insert_before`，删除使用 `remove`，事件 prop 增删使用 `bind_event`/`clear_event`。
- 已扩展 `packages/react/__test__/reconciler.cjs`，覆盖 `useState` 文本更新、列表新增/重排/删除、事件 bind/handler 更新/clear。
- `npm run typecheck --prefix packages/react` 通过；`npm test --prefix packages/react` 首次显示列表重排实际 op 为移动已挂载 `A` 到 `B` 前，已按正确 move 语义调整断言，重跑通过。
- `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets` 均通过。
- 下一步执行 Node/native 与 packages TS/JS 验证：napi build、Node crate npm test、`packages/core` typecheck/test、`packages/react` typecheck/test。
- Node/native 与 packages 验证通过：`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`、`npm test`（`crates/atto-ui-node`）、`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`、`npm test --prefix packages/core`、`npm run typecheck --prefix packages/react`、`npm test --prefix packages/react`。
- 未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。下一步更新 `TODO.md`/`TODO-1.md` 的 `NT9` 完成状态与完成记录。
- 已更新 `TODO.md` 与 `TODO-1.md`，将 `NT9` 标记为 `[DONE]` 并写入完成记录；`git diff --check` 通过。下一步仅暂存本任务相关文件并提交。
