执行计划

1. 读取 `TODO.md`，按文件顺序定位第一个标题未以 `[DONE]` 开头的任务，并确认该任务的要求、依赖、验证标准和完成记录格式。
2. 查看最近提交信息，判断是否存在与该首个未完成任务直接相关的未完成事项；若存在且会阻塞当前任务，按要求把它纳入当前任务或在 `TODO.md` 中添加最小 prerequisite。
3. 只阅读与当前任务相关的代码、测试和文档，避免进行开放式历史问题扫查。
4. 实现当前任务；若发现 spec mismatch、缺失功能或测试/fixture 失败会阻塞任务，则优先修复，或在 `TODO.md` 中新增最小 prerequisite 后停止。
5. 运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，通过后运行相关测试；如代码变更需要完整验证，则运行完整测试套件并设置足够超时。
6. 任务完成后，在 `TODO.md` 中把任务标题前缀改为 `[DONE]`，更新 completion record；仅当阶段计划确实变化时才更新 `PLAN.md`。
7. 提交本次任务涉及的所有更改，提交信息包含任务编号和简要说明，然后停止，不处理下一个任务。

当前状态

- 已读取 `TODO.md` 与 `TODO-1.md`，首个未完成任务为 `NR11 — 审阅 NT11`。
- 最新提交为 `[NR10] Review React render loop`，未显示直接声明未完成 `NR11` 的提交信息；工作区已有 `NT11` 相关未提交改动，需要作为本次审阅输入处理，避免覆盖未知用户改动。
- 已审阅 `NT11` 事件桥主路径：React host 使用 `CallbackEventDispatcher` 维护 `callbackId -> 最新 handler`，tick 后 drain 并分发，卸载/事件清理时释放 handler 与 native handle，Node `drainCallbacks` 会过滤已释放 callback。
- 已补充 `packages/react/__test__/render.cjs` 的 `for await` 流式 `setState` 回归测试，覆盖模拟 LLM chunk 流与 tick loop 共存。
- 已通过验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`npm run typecheck --prefix packages/react`；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm test --prefix packages/core`；`npm test --prefix packages/react`；`git diff --check`。
- 已将 `NR11` 在 `TODO-1.md` 与 `TODO.md` 标记为 `[DONE]` 并写入完成记录。
- 已提交本次任务相关改动：`919e37a [NR11] Review React event bridge`。当前仅剩与本任务无关的未跟踪脚本 `notification.sh`、`run_agent.sh`，未纳入提交。
