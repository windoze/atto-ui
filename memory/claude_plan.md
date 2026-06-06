# 当前执行计划

## 约束说明

- 本文件记录可公开的执行计划、关键判断摘要和进度更新；不记录不可公开的内部推理细节。
- `TODO.md` 是任务顺序与完成状态的权威来源。
- 本次只完成首个未标记 `[DONE]` 的任务，然后停止。

## 初始步骤

1. 读取 `TODO.md`，按文档顺序找出第一个标题未带 `[DONE]` 的任务。
2. 仅检查与该任务直接相关的最近 Git 上下文，不做开放式历史问题扫查。
3. 阅读所选任务的需求、依赖、验证要求和完成记录格式。

## 执行步骤

1. 检查所选任务涉及的代码和测试。
2. 以最小完整变更实现任务，不引入 workaround 或行为偏移。
3. 运行 `cargo fmt`。
4. 运行 `cargo clippy --workspace --all-targets -- -D warnings`。
5. 按任务要求运行 `cargo test --all --all-targets`。
6. 如发现未调度的测试或夹具失败，先修复，或在 `TODO.md` 中加入最小必要前置任务并停止。
7. 成功后在 `TODO.md` 中把当前任务标题改为 `[DONE]`，并更新完成记录。
8. 提交本任务相关变更。

## 当前状态

- 已读取 `TODO.md`，首个未完成任务为 `T14A — 拆分 editor view 巨型文件（M8）`。
- 已检查最近提交；最新提交 `[T14] Split giant-file refactor tasks` 与当前任务拆分直接一致，未发现需要插入的新阻塞任务。
- 工作区存在预先已有的未跟踪文件 `notification.sh` 与 `run_agent.sh`；它们与 T14A 无关，本次不修改、不提交。

## T14A 拆分方案

- 保留 `view/mod.rs` 中的公开数据类型、私有状态类型和 `EditorView::new`。
- 新增 `view/state.rs`，承载状态同步和跨模块共享的小 helper。
- 新增 `view/editing.rs`，承载文本编辑、剪贴板、缩进相关操作。
- 新增 `view/scrolling.rs`，承载视口、滚动、折叠、光标移动相关操作。
- 新增 `view/actions.rs`，承载 keymap action 分发。
- 新增 `view/component_impl.rs`，承载 `Component`、layout、scroll、focus、event trait impl。
- 新增 `view/tests.rs`，承载原 `mod.rs` 内联单元测试。
- 复用既有 `view/lsp.rs`，移入 LSP hover/completion/goto 调度 helper。
- 复用既有 `view/render.rs`，移入语法/语义样式映射 helper。
- 跨子模块调用只提升为 `pub(super)`，不新增外部公开 API。

## 进度记录

- 已完成 `view/mod.rs` 机械拆分，`mod.rs` 现在只保留模块声明、类型定义和构造函数。
- `cargo fmt` 已通过。
- `cargo clippy --workspace --all-targets -- -D warnings` 已通过。
- `cargo test --all --all-targets` 已通过。
- `TODO.md` 已更新：T14A 标记为 `[DONE]` 并补充完成记录。
- 下一步：提交本任务相关文件，然后停止。
