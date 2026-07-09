# Claude Execution Plan

本文件记录本轮调用的可检查执行计划和进度摘要。不会记录不可公开的内部推理细节。

## 当前目标

根据 `TODO.md` 的顺序完成第一个标题未带 `[DONE]` 的任务：`P2.R Review：P2 阶段复核`，完成后更新记录、验证、提交并停止。

## 初始计划

1. 读取 `TODO.md`，只识别第一个未完成任务，不做开放式历史问题扫查。
2. 检查最近提交信息是否明确提到与该任务直接相关的未完成问题。
3. 阅读该任务相关代码、测试和文档，确认要求、依赖和验证方式。
4. 按任务要求做最小且完整的实现；如遇到阻塞当前任务的真实前置问题，按要求更新 `TODO.md` 并停止。
5. 运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行必要测试；如代码变更需要完整验证，则运行完整测试套件并设置足够超时。
6. 更新 `TODO.md`：在任务标题前添加 `[DONE]`，填写完成记录和验证结果；只有阶段级计划变化才更新 `PLAN.md`。
7. 检查工作区差异，提交本轮相关变更，提交信息包含任务编号和简短说明。
8. 完成一个任务后停止，不继续下一个任务。

## 进度

- 已创建初始执行计划。
- 已读取 `TODO.md`，确认第一个未完成任务为 `P2.R Review`。
- 最新提交为 `[P2.5] Add chat completion PTY snapshots`，与当前 P2 复核任务直接相关，纳入本轮复核范围。
- 已复核 P2 相关 Rust、PTY、JS 类型与文档路径；未发现需要代码修复的阻塞问题。
- 即将按顺序运行格式、lint、构建、Rust 全量测试与 JS smoke/runtime 验证。
- Rust、core/runtime、React 与 examples smoke 验证已通过。
- `napi artifacts` 暴露阻塞问题：`napi build` 会重写 `crates/atto-ui-node/index.js` / `index.d.ts` 并丢失仓库已有的 loader/type 补充，导致 CI 末尾工作区变脏；本机还存在忽略的 stale `atto_ui_node.darwin-x64.node`，没有对应 dist 目录，会阻塞 artifacts 收集。
- 已添加 native build/artifacts wrapper 并更新 CI/release/docs；`npm run build --prefix crates/atto-ui-node` 与 `npm run npm:artifacts --prefix crates/atto-ui-node` 已通过，且 `index.js` / `index.d.ts` 不再因构建变脏。
- 本机尝试 `npm pack` Linux 平台子包失败，因为 macOS 本地没有 Linux `.node` artifact；这是跨平台产物不可用导致的本地环境限制，CI Ubuntu job 会生成并打包该平台文件。本轮继续验证本机可用平台包与 JS/Rust 全套命令。
- 重新运行 Rust 全套 gate、JS runtime/React/examples smoke、native wrapper/artifacts、本机可用 package dry-run 与 `git diff --check`，均已通过。
- 已将 `TODO.md` 中 `P2.R Review` 标记为 `[DONE]`，并写入复核、修复和验证记录。

## 最终状态

- P2.R 已完成。
- 未更新 `PLAN.md`，因为阶段级计划、依赖和完成标准没有变化。
- 下一步只剩检查差异并提交本轮变更。

## 调整后的计划

1. 修复 native binding 生成后的稳定性：让仓库中的 `index.js` / `index.d.ts` 在 `napi build` 后仍保留所需 loader/type 补充，避免 CI diff。
2. 清理或规避本机 stale ignored artifact 后复跑 `napi artifacts`，确认 artifacts 收集通过。
3. 重新运行受影响的 JS/native 验证和 CI 尾部 dry-run / whitespace 检查。
4. 再更新 `TODO.md` 完成记录并提交。

## P2.R 复核步骤

1. 复核 P2.1 completion overlay：焦点/键盘语义、Esc/Enter、空候选、滚动、宽字符处理。
2. 复核 P2.2 slash command：触发条件、过滤、确认语义、回调与默认插入行为。
3. 复核 P2.3 mention：provider 降级、多次提及、range 替换、光标编辑一致性。
4. 复核 P2.4 runtime/JS/docs：Rust schema、Node/core/react 类型与事件协议一致性。
5. 复核 P2.5 snapshot/PTY：覆盖 slash 与 mention 的触发、过滤、确认、Esc 关闭。
6. 如发现问题，修复后重新验证；如未发现阻塞问题，运行格式、lint、构建和完整测试。
