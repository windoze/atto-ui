执行计划与进度记录

约束说明：此文件记录可审计的执行计划、关键决策和进度更新；不会记录不可公开的内部推理链。

当前目标：按 `TODO.md` 的顺序完成第一个标题未带 `[DONE]` 的任务，完成后更新记录、验证、提交，并停止。

步骤：
1. 读取 `TODO.md`，只识别第一个未完成任务，不进行开放式历史问题扫查。
2. 查看最新提交信息；若它明确提到与当前任务直接相关的未完成问题，将其纳入当前任务或作为前置任务记录到 `TODO.md`。
3. 阅读当前任务相关的计划、源码和测试，确认要求、依赖与验证方式。
4. 若任务可直接完成，做最小正确实现；若发现阻塞当前任务的真实前置问题，按要求更新 `TODO.md` 并停止。
5. 运行格式化、lint 和相关测试；若代码改动影响全局行为，则按要求运行完整验证。
6. 将任务标题标记为 `[DONE]`，更新 `TODO.md` 的 completion record；仅当阶段计划变化时更新 `PLAN.md`。
7. 检查 `git status`、`git diff`、近期提交，提交本次任务相关改动。
8. 提交后停止，不进入下一个任务。

进度：
- 已创建本执行计划文件。
- 已读取 `TODO.md`/`TODO-1.md`，确认当前任务为 `NR14 — 审阅 NT14`。
- 已检查最新提交 `[NT14] Add React host component wrappers`，与当前审阅任务直接相关；审阅范围限定为该提交及其相关实现/测试。
- 已审阅 React host wrapper、JSX 类型、runtime change payload 与受控 TextBox 测试。
- 发现并纳入当前审阅修复：`TextBox` 未忽略 key release 可能重复输入；受控 `TextBox` 在 `onChange` 拒绝/转换输入时不会回写 React 受控值；raw `<grid>` JSX 类型允许 wrapper-only camelCase gap props；`MenuItem.onClick` 类型不能接收事件对象。
- 已完成最小范围修复并补充 Rust/JS/TS 回归测试。
- 已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`npm run typecheck --prefix packages/react`、`cargo test -p atto-ui widgets::textbox::tests::key_release_does_not_insert_text`、`npm test --prefix packages/react`。
- 已通过完整验证：`cargo test --all --all-targets`、`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）、`npm test --prefix crates/atto-ui-node`、`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`、`npm test --prefix packages/core`、`npm run typecheck --prefix packages/react`、`npm test --prefix packages/react`、`git diff --check`。
- 已确认未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。
- 已将 `NR14` 在 `TODO-1.md` 和 `TODO.md` 标记为 `[DONE]`/`DONE` 并写入完成记录。
- 下一步：检查 diff/status 后提交本任务改动并停止。
