# 执行计划

## 当前状态

- 已开始本轮任务执行。
- 计划文件仅记录可审阅的执行步骤、决策和进度，不包含隐藏推理链。

## 初始步骤

1. 读取 `TODO.md`，按文档顺序识别第一个标题未以 `[DONE]` 开头的任务。
2. 检查最新提交是否明确提到与该任务直接相关的未完成问题。
3. 阅读该任务相关代码、测试和项目约束，只处理当前任务范围内的问题。
4. 实现当前任务；如遇到阻塞当前任务的真实前置缺口，则最小化更新 `TODO.md` 并停止。
5. 按要求运行格式化、lint 和相关/完整测试；若发现未排期失败，修复或在 `TODO.md` 中排期。
6. 在 `TODO.md` 中将完成的任务标题加上 `[DONE]`，更新完成记录。
7. 检查 git 状态和差异，提交本轮全部相关变更，然后停止，不继续下一个任务。

## 进度记录

- 已读取 `TODO.md` 与 `TODO-1.md`，首个未完成任务为 `NT20 — CI 流水线 + Bun/Deno + 文档（P.3）`。
- 最新提交为 `35acecf [NR19] Review npm platform packaging`，未从提交标题发现直接相关的未完成前置问题。

## NT20 执行计划

1. 检查现有 CI/workflow、package scripts、README/API 文档与 Bun/Deno 相关测试覆盖。
2. 补齐 CI：Rust/TS/JS 测试、napi 平台构建/pack 校验、tag 发布链路，并覆盖 reconciler 单测与 e2e。
3. 补齐 Bun/Deno 冒烟脚本或测试，并记录 raw-mode/终端行为差异。
4. 更新根 README 与 API/快速开始文档，使安装、Node/React 使用、发布和多运行时验证可上手。
5. 运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、Rust/TS/JS 测试和 Bun/Deno 冒烟；若本机缺少运行时则安装/执行或记录明确跳过原因。
6. 将 `TODO-1.md` 中 `NT20` 标题标为 `[DONE]` 并写入完成记录，同步更新根 `TODO.md` 索引状态。
7. 检查 `git status`/`git diff`/最近提交，提交所有本轮相关变更后停止。

## NT20 进度

- 已确认仓库当前没有 `.github/workflows`；需要新增 CI 与 tag 发布 workflow。
- 已确认现有 Node/Core/React 包和测试结构；可在 `packages/core` 增加跨 Node/Bun/Deno 的 headless 与 PTY raw-mode 冒烟脚本。
- 已发现工作树中存在与本任务无关的未跟踪 `notification.sh`、`run_agent.sh`，不会修改或提交。
- 已新增 `packages/core` 的 Node/Bun/Deno headless + PTY 兼容性冒烟脚本，并验证 Node、Deno、Bun 本地通过。
- 已新增 `.github/workflows/ci.yml` 与 `.github/workflows/release.yml`，覆盖常规验证、Bun/Deno 冒烟、pack dry-run 与 tag 发布流程。
- 已更新根 `README.md`，新增 `docs/NODE_API.md`、`docs/RELEASE.md`，并同步 `NODE_BINDING.md` 的已定实现与兼容性记录。
- 已运行 required validation：Rust fmt/clippy/full tests、N-API build、core/react typecheck、Node/core/react tests、Bun/Deno compatibility smokes、npm artifact/pack dry-runs 和 `git diff --check` 均通过。
- 已将 `TODO.md` 索引与 `TODO-1.md` 的 `NT20` 标记为 `[DONE]` 并写入完成记录；下一项 `NR20` 保持未开始。
