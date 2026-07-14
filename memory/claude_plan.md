# 执行计划

## 当前目标

按 `TODO.md` 的权威顺序完成第一个标题尚未带 `[DONE]` 的任务，完成后更新任务记录、运行要求的验证、提交 Git 提交，然后停止，不继续处理下一个任务。

## 关键约束

- 输出和进度记录使用中文。
- `TODO.md` 是任务顺序、完成状态、依赖和验证要求的唯一权威来源。
- 任务只有标题显式带 `[DONE]` 才算完成。
- 不做开放式历史问题扫描；只处理会阻塞当前任务、破坏当前任务指定行为、或当前执行引入的直接回归。
- 不通过缩小范围、替换表示方式、临时兼容层或特例绕过规格问题。
- 如发现未安排的测试失败，必须修复，或在 `TODO.md` 中加入最小必要的前置任务并提交后停止。
- 常规每任务记录只更新 `TODO.md` 和本文件；仅当阶段级计划变化时才更新 `PLAN.md`。
- 编辑代码前先说明将要修改的内容；手工文件修改使用小而明确的补丁。
- 验证顺序为 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、完整测试；完整测试最长 30 分钟。
- 完成后必须提交 Git 提交，并停止。

## 步骤计划

1. 读取 `TODO.md`，定位第一个标题未带 `[DONE]` 的任务，并记录任务编号、标题、依赖、要求和验证标准。
2. 查看最近提交信息，判断是否明确提到与当前任务直接相关的未完成问题；如有，将其纳入当前任务或作为前置任务写入 `TODO.md`。
3. 只读取与当前任务直接相关的项目文件，理解现有实现、测试结构和局部约定。
4. 如果当前任务能直接实施，则按现有代码风格完成实现；如果发现具体前置阻塞，则向 `TODO.md` 插入最小必要前置任务，保持当前任务未完成，提交后停止。
5. 为实现添加或调整聚焦测试，覆盖任务要求和相关边界。
6. 运行 `cargo fmt`。
7. 运行 `cargo clippy --all-targets -- -D warnings`，修复所有警告。
8. 运行完整测试套件，优先使用 `cargo test --all --all-targets` 并设置不超过 30 分钟的超时；如任务自身指定更严格验证，一并执行。
9. 验证通过后，在 `TODO.md` 中给当前任务标题加 `[DONE]`，并更新完成记录，包含变更摘要、验证命令和结果。
10. 检查 Git 工作区，确认只包含本次任务相关变更以及需要纳入的既有未提交恢复状态。
11. 创建清晰的 Git 提交。
12. 停止，不处理下一个任务。

## 进度记录

- 已创建初始计划文件；下一步读取 `TODO.md` 识别当前任务。
- 已读取 `TODO.md`，本轮首个未完成任务确定为 `M4-1 协议定义（可序列化请求 / 响应）`。
- 已检查最近一次提交 `cc51320`，提交主题为 `[M3-R] Review tmux L0 L1 layer`，未明确提到与 `M4-1` 直接相关的未完成问题。
- 当前未提交变更只有本计划文件；下一步读取 `ComponentCommand` / `ComponentTarget` / `ComponentError`、`DesktopInspector` 的 M2 API 和 crate 导出结构。
- 已读取相关 API：M2 的 `ComponentCommand`、`ComponentTarget`、`ComponentValue`、`DesktopSnapshot`、`InvokeResult`、`WaitCondition`、`WaitResult` 均已 serde 化；`ComponentError` 尚未 serde 化，`InvalidValue.expected` 当前是 `&'static str`。
- 实施方案：新增 `src/protocol.rs`，定义 JSON-RPC 类 `ProtocolRequest` / `ProtocolResponse`，请求采用顶层 `id` + `method` + `params` 的 tagged enum，响应采用顶层 `id` + `result` 或 `error`。method 覆盖 `query`、`invoke`、`wait_for`、`tree`、`property_names`。`invoke` / `wait_for` / `tree` 参数包含可序列化 `runtime::Rect` 屏幕区域与毫秒 timeout，供 M4-2 转换为进程内 API 使用。
- 同步方案：把 `ComponentError::InvalidValue.expected` 改为拥有的 `String`，并为 `ComponentError` 派生 `Serialize` / `Deserialize`，使所有错误变体可直接 roundtrip；构造函数继续接收字符串字面量等 `Into<String>` 输入。
- 测试方案：在 `src/protocol.rs` 单测中覆盖每种请求 roundtrip、每种成功响应 roundtrip、每种 `ComponentError` 错误响应 roundtrip，并检查响应构造器不会同时设置 `result` 和 `error`。
- 已实现 `src/protocol.rs`、`ComponentError` serde 化和 `src/lib.rs` 模块导出。
- 聚焦验证 `cargo test -p atto-ui protocol -- --nocapture` 通过。
- `cargo fmt --all` 和 `cargo fmt --all -- --check` 通过。
- `cargo clippy --workspace --all-targets -- -D warnings` 通过，无 warning。
- 30 分钟超时保护下的完整 `python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets` 通过。
- 下一步更新 `TODO.md`：将 `M4-1` 标记为 `[DONE]`，补完成记录和验证记录；`PLAN.md` 不需要更新。
- 已更新 `TODO.md` 中 `M4-1` 的 `[DONE]` 状态、完成记录和验证记录；之后只修改了 Markdown 任务记录，不需要重新运行完整测试。
- 下一步执行 diff 检查、查看 git status，然后提交本任务变更。
- 已创建本任务提交，提交信息为 `[M4-1] Define scripting protocol messages`；本轮停止，不继续处理 `M4-2`。
