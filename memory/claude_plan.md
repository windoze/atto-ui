# 执行计划

## 当前任务

- 来源：`TODO.md` / `TODO-1.md`。
- 第一个未完成任务：`NT17 — PTY 端到端（T.2）`。
- 范围：新增或完善真实/headless 端到端测试，覆盖计数器、受控输入表单、列表增删、多窗口，并断言屏幕或 headless snapshot。
- 停止点：完成 `NT17`、更新完成记录、提交 Git 后停止；不进入 `NR17`。

## 约束

- 以 `TODO.md` 和 `TODO-1.md` 为任务顺序、要求和完成状态的权威来源。
- 只处理当前第一个未完成任务；除非阻塞 `NT17`，不做开放式历史问题排查。
- 若遇到未安排且会影响当前验证的失败测试或 fixture，必须修复，或在 `TODO.md` 中加入最小前置任务并停止。
- 不使用 workaround、fixture-only hack 或弱化验收；测试必须经过真实 Rust/native 分发路径或 headless runtime 路径。
- 仅当阶段级计划变化时更新 `PLAN.md`；常规完成记录只写 `TODO.md` / `TODO-1.md`。
- 本文件记录可审阅的执行计划和进度，不写入隐藏推理过程。

## 步骤

1. 检查最近提交，确认是否有与 `NT17` 直接相关的未完成事项。
2. 阅读现有 React、Node binding、PTY 测试与 test host 代码，确认当前 e2e 测试入口和可复用工具。
3. 设计 `NT17` 的最小完整测试集合：计数器、受控输入表单、列表增删、多窗口，优先复用现有 React render 与真实 `AppHost`/PTY 路径。
4. 实现必要的 e2e 测试、测试 app 或测试辅助代码；如发现 runtime/React/native 缺口，直接修正根因。
5. 先运行针对性测试，再按要求运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、完整 Rust 测试、相关 TS typecheck 与 JS 测试。
6. 若验证通过，更新 `TODO.md` 与 `TODO-1.md`：将 `NT17` 标题标记为 `[DONE]`，写入完成记录和验证命令。
7. 检查 `git status`、`git diff`、`git log --oneline -10`，只提交本任务相关变更。
8. 提交后停止，不继续后续任务。

## 进度

- 已读取 `TODO.md` 和 `TODO-1.md`，确认当前执行单元为 `NT17`。
- 已检查最近提交：最新提交为 `85c9502 [NR16] Record completion status`，未声明与 `NT17` 直接相关的未完成事项。
- 已阅读现有 React、Node binding 与 PTY/e2e 测试入口；现有覆盖分散，缺少一个同时覆盖列表增删和多窗口屏幕断言的端到端用例。
- 当前实施方案：新增 `packages/react/__test__/e2e_app.cjs` 与 `e2e.cjs`，复用真实 `render()`/native `AppHost` 路径；headless 部分直接驱动窗口事件并断言 snapshot，PTY 部分驱动真实终端并断言屏幕文本。
- 已添加 `packages/react/__test__/e2e_app.cjs` 与 `e2e.cjs`，并接入 `packages/react/package.json` 的 `npm test`。
- 新 e2e 覆盖内容：headless 直接驱动真实 `AppHost` 窗口事件并断言 snapshot；PTY 启动真实终端 app，断言多窗口屏幕、受控输入、列表新增/删除和计数器更新。
- 针对性验证已通过：`npm run build --prefix packages/react && node packages/react/__test__/e2e.cjs`。
- 完整验证已通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm run typecheck --prefix packages/core`；`npm run typecheck --prefix packages/react`；`npm test --prefix crates/atto-ui-node`；`npm test --prefix packages/core`；`npm test --prefix packages/react`；`git diff --check`。
- 已确认仓库中没有 `tools/run_fixtures.py`，无单独 fixture 套件可运行。
- 已更新 `TODO.md` 与 `TODO-1.md`，`NT17` 已标记为 `[DONE]` 并写入完成记录。
- 最终 `git diff --check` 已通过；已复查 status/diff/log，未纳入无关未跟踪文件 `notification.sh`、`run_agent.sh`。
- 已提交 NT17 变更：`325cfb0 [NT17] Add React e2e coverage`。
- 当前任务已完成，按要求停止，不继续 `NR17`。
