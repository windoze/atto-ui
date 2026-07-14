执行计划（初始版）

约束说明：
- 按用户要求，输出与进度记录使用中文。
- 不记录私有推理链，只记录可审查的任务判断、执行步骤、验证结果和计划调整。
- `TODO.md` 是任务顺序与完成状态的唯一依据；只处理第一个标题未带 `[DONE]` 的任务。
- 完成当前任务后必须更新 `TODO.md`、运行要求的验证、提交 Git，然后停止，不继续下一个任务。

步骤：
1. 读取 `TODO.md`，识别第一个未完成任务，确认任务要求、依赖、验证标准和完成记录格式。
2. 查看最新提交信息；如果最新提交明确提到与当前任务直接相关的未完成事项，将其纳入当前任务或作为前置任务记录到 `TODO.md`。
3. 只读取完成当前任务所需的相关文件，避免开放式历史问题扫描。
4. 根据当前任务制定具体实现方案，并在本文件中补充任务识别结果与关键步骤。
5. 实施代码或文档变更，变更前后按需重新阅读受影响片段。
6. 按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后在需要时运行完整测试套件；若仅文档变更且可复用上次绿色结果，则在完成记录中说明跳过原因。
7. 若出现未计划的测试失败，修复它；若无法在当前任务内合理修复，则将最小前置任务插入 `TODO.md` 并停止。
8. 完成后给当前任务标题加 `[DONE]`，更新完成记录；仅在阶段级计划变化时更新 `PLAN.md`。
9. 提交所有与当前任务相关的变更，使用清晰的提交信息。
10. 最终回复总结完成内容、验证结果和提交信息。

当前任务识别：
- 第一个未完成任务：`M4-3 外部 atto CLI 客户端`。
- 任务要求：实现最小外部 CLI（新 bin / crate），通过 Unix socket 走 M4-1 协议，提供 `query <tag> <prop>`、`invoke <tag> <action>`、`tree` 子命令；输出人类可读，并支持可选 JSON。
- 验证要求：端到端测试启动带 server 的 fixture app，通过 CLI 子命令驱动 UI 并读回状态；通用验收为 fmt、clippy、workspace 全量测试。
- 最新提交：`[M4-2] Add IPC socket server dispatch`，属于当前任务直接依赖的上一任务提交；提交标题未声明与 M4-3 直接相关的未完成阻塞项。

当前任务执行计划：
1. 已读取 `src/protocol.rs`、`src/ipc.rs`、`Cargo.toml`、现有 bin 列表与 M4-2 IPC 测试，确认协议数据形状、socket env 名称、测试风格和 bin 组织方式。
2. 已设计并实现最小 `atto` CLI：解析 socket 路径（`--socket` 优先，默认 `ATTO_UI_SOCKET`）、`--json` 输出、`--screen`、`query` / `invoke` / `tree` 子命令和 action 字符串到 `ComponentCommand` 的映射。
3. 已新增 `src/bin/atto.rs`，复用 `atto_ui::ipc::send_protocol_request` 与 `atto_ui::protocol::*`，未重复实现传输协议。
4. 已新增 `tests/atto_cli.rs` 端到端测试：启动启用 IPC 的 headless `Desktop`，通过真实 `atto` bin 执行 query / invoke / tree，并断言返回内容和 UI 状态变化。
5. 若发现 CLI 需要可测试的 fixture app 或公共 helper 缺口，优先补最小、通用的测试支撑，而不是绕开协议或直接调用内部 API。
6. 已运行 `cargo fmt --all`、针对性 CLI 测试、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`，均通过。
7. 已更新 `TODO.md` 中 M4-3 标题为 `[DONE]`，并补充完成记录与验证命令；下一步提交当前任务变更并停止。
