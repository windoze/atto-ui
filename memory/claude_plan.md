# Claude 执行计划

## 范围

- 任务来源：`TODO.md`。
- 本轮目标：只完成第一个未完成任务，标记 `[DONE]`，完成验证并提交，然后停止。
- 说明：本文件记录可审查的计划、决策、阻塞和进度，不记录隐藏推理链。

## 初始计划

1. 先读取 `TODO.md`，定位第一个标题未带 `[DONE]` 的任务。
2. 仅检查最近提交是否明确提到与该任务直接相关的未完成事项。
3. 阅读所选任务的要求、依赖和验证条件。
4. 只检查当前任务所需的代码、测试和文档。
5. 按任务原意实现，不缩小范围，不引入 workaround。
6. 按顺序运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、相关 JS 验证、完整构建和测试。
7. 如出现未排期失败测试或具体阻塞，先修复或在 `TODO.md` 插入最小前置任务后停止。
8. 完成后更新 `TODO.md`，给任务标题加 `[DONE]` 并补全完成记录。
9. 提交本轮所有应提交改动。
10. 停止，不开始下一项任务。

## 进度

- 已在读取 `TODO.md` 前初始化本计划文件。
- 已选择第一个未完成任务：`P2.4 运行时/JS 侧同步`。
- 最近提交信息未包含与 P2.4 直接相关的未完成事项。
- 协议决策：`ChatInputPanel` 暴露 `slash_commands`、`mention_candidates` 属性，以及 `slash_command`、`mention_query` 事件。
- mention provider 方案：JS 通过 `mention_query` 收到 `{ draft, query, cursor, replacement_start, replacement_end }`，随后更新 `mention_candidates` / `mentionCandidates`；Rust 同步读取当前候选，兼容静态候选和异步刷新。
- 已实现 Rust dynamic schema、属性解析、`set_property`、slash command 回调和 mention query 桥接。
- 已同步 `crates/atto-ui-node`、`packages/core`、`packages/react` 的类型、构造器、React wrapper/raw JSX 和文档。
- 发现并修复本地 runtime 兼容测试问题：Bun 会优先加载全局缓存的旧 `@atto-ui/node-<platform>`，因此 `packages/core/native.js` 在 optional platform 包前优先尝试 workspace `crates/atto-ui-node` fallback；已同步 README 和 API 文档加载顺序。
- 验证已完成并通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、JS core/react typecheck 与测试、`npm run smoke --prefix examples/react-tsx`、`npm run test:runtime --prefix packages/core`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 已更新 `TODO.md`，将 `P2.4` 标记为 `[DONE]` 并补全完成记录。
