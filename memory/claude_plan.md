# Claude Plan

## 执行边界

- 本次只处理 `TODO.md` 中第一个标题未以 `[DONE]` 开头的任务，完成后停止。
- `TODO.md` 是任务排序、依赖、验证和完成记录的权威来源；`PLAN.md` 只在阶段级计划变化时更新。
- 若发现阻塞当前任务的缺陷、规格不匹配或测试失败，将优先修复；若无法在当前任务内完成，则把最小必要前置任务插入 `TODO.md` 并停止。
- 不采用绕过方案；若实现路径暴露同类根因，会修复已确认的整类问题。
- 本文件记录可审计的计划、关键决策和进度更新；不包含无法验证的内部推理。

## 初始步骤计划

1. 读取 `TODO.md`，按文件顺序定位第一个未 `[DONE]` 的任务。
2. 检查最近提交信息，判断是否明确提到与当前任务直接相关的未完成事项。
3. 阅读当前任务正文、依赖、验收标准和完成记录，必要时查看相关代码和测试。
4. 按任务要求做最小正确实现；编辑前先确认上下文，使用小补丁逐步修改。
5. 运行验证：先 `cargo fmt`，再 `cargo clippy --all-targets -- -D warnings`，最后运行相关测试；如需要完整套件，使用足够长超时。
6. 若验证通过，更新 `TODO.md`：在任务标题前加 `[DONE]`，补充完成记录和验证结果。
7. 若阶段级计划没有变化，不更新 `PLAN.md`。
8. 提交所有本任务相关变更，提交信息包含任务编号和简明说明。
9. 停止，不继续处理下一个任务。

## 当前状态

- 状态：已确认第一个未完成任务为 `P2.1 补全 overlay 组件`。
- 最近提交：`d38dd92 [P1.R] Complete P1 review`，未发现提交信息中有与 P2.1 直接相关的未完成事项。

## P2.1 当前执行计划

1. 查看 `PLAN.md`、`AGENT_GAP.md` 中 P2/A1/A2 的约束，以及 `crates/atto-ui-chat` 的输入和渲染结构。
2. 找到适合放置可复用 completion popup 的模块边界，优先保持独立组件，不提前实现 slash/mention provider 协议。
3. 实现候选列表、匹配高亮、上下键选择、Enter 确认、Esc 关闭、锚点上下方定位、空候选、超长列表滚动和宽字符处理。
4. 增加单测和必要快照/渲染测试，覆盖 P2.1 明确边界。
5. 运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
6. 通过后更新 `TODO.md` 完成记录，并提交本任务相关变更。

## P2.1 进度更新

- 已确认 P2.1 不要求 slash/mention provider、动态协议或 JS 侧同步；这些属于 P2.2–P2.4。
- 设计落点：在 `crates/atto-ui-chat/src/completion.rs` 新增独立 overlay 组件，并从 `lib.rs` 导出公共 API。
- 实现策略：组件使用 `Binding` 管理 `query/items/open/selection/accepted`，通过 `Rect` anchor 在给定屏幕区域内自动选择上方或下方绘制；事件只处理打开状态下的 Up/Down/Enter/Esc。
- 为了安全处理宽字符和组合字符，将给 `atto-ui-chat` 增加 `unicode-segmentation` 依赖。
- 已新增 `CompletionPopup`、`CompletionItem`、`CompletionAnchor`、`CompletionPlacement`，并从 `atto-ui-chat` 导出。
- 已补单测覆盖：键盘上下选择、Enter 确认、Esc 关闭、空候选、超长列表滚动、自动上下方锚定、匹配高亮、宽字符裁剪、Release/Ctrl 导航不捕获。
- 快速验证：`cargo fmt --all && cargo test -p atto-ui-chat completion` 已通过。
- 全量验证已通过：`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 已更新 `TODO.md`：P2.1 标题已加 `[DONE]`，并补充完成记录、测试覆盖和验证记录。
- 已检查 `git status --short`、`git diff --cached --stat`、`git diff --cached`、`git log --oneline -10`；暂存变更范围仅包含 P2.1 相关实现、依赖、任务记录和计划记录。
- 下一步：提交本任务变更后停止。
