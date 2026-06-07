# 执行计划

## 当前状态

- 已读取 `TODO.md`，第一个未完成任务是 `TODO-1.md` 中的 `NR20 — 审阅 NT20`。
- 最近提交 `a04f9d0 [NT20] Add CI runtime compatibility docs` 与当前任务直接相关；本轮审阅该提交范围，不处理后续 `TODO-2.md` 任务。
- 工作区已有未跟踪文件 `notification.sh`、`run_agent.sh`，本轮不会读取、修改或提交它们，除非后续发现其与当前任务直接相关。

## 执行步骤

1. 审阅 NT20 改动范围：`.github/workflows/ci.yml`、`.github/workflows/release.yml`、运行时兼容测试、package scripts、`README.md`、`docs/NODE_API.md`、`docs/RELEASE.md`、`NODE_BINDING.md`。
2. 确认 CI 覆盖编译、测试、runtime 兼容、pack dry-run 和 tag 发布链路；如发现真实缺口，直接修复。
3. 确认 Bun/Deno 兼容测试行为与 Node 一致，或文档明确记录差异；如发现脚本或测试不可运行，直接修复。
4. 确认文档中的 API、命令和发布流程与实际 package scripts / 类型定义同步；如发现偏差，直接修复。
5. 按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，通过后运行相关 JS/runtime 测试与必要的完整 Rust 测试。
6. 若发现未被显式排期的测试或夹具失败，按策略修复，或在 `TODO.md` 中加入最小必要前置任务并停止。
7. 完成后在 `TODO-1.md` 和索引 `TODO.md` 中将 `NR20` 标记为 `[DONE]`，更新完成记录；仅当阶段计划变化时才更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、`git log --oneline -10`，确认只提交应提交内容。
9. 使用清晰提交信息提交本轮相关变更，然后停止，不处理下一个任务。

## 进度记录

- 已创建本计划文件并定位当前任务为 `NR20`。
- 已审阅 NT20 的 CI、运行时兼容测试与文档同步性。
- 已修复 release workflow 缺少 tag 发布前完整测试门禁的问题，并同步 README / release 文档 / Node binding 设计记录 / Node API 文档中的命令、API 和运行时差异说明。
- Rust 格式化、clippy 和全量 Rust 测试已通过。
- JS native build 验证发现 `npm run build --prefix crates/atto-ui-node` 依赖本地 `node_modules` 中的 `napi`，不适合 clean checkout；也验证了 `npm --prefix ... exec` 不会改变 napi CLI 的工作目录。已将 README / release 文档改为可从仓库根目录运行的 `npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --cwd crates/atto-ui-node --platform`。
- npm pack dry-run 验证发现 workflow 中的 `npm pack --prefix ...` 在无根 `package.json` 的仓库会失败；已将 CI/release workflow 改为 `npm pack --dry-run --json ./path` 本地包路径形式。
- native build、JS typecheck/test、Node/Bun/Deno runtime 兼容测试、React 测试、可用本机平台 pack dry-run 与 `git diff --check` 已通过。
- 已在 `TODO-1.md` 和 `TODO.md` 将 `NR20` 标记为 `[DONE]` 并写入完成记录。
- 下一步复查 diff/status/log，随后提交本轮相关变更并停止。
