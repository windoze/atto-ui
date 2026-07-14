# 执行计划

## 当前状态
- 已读取 `TODO.md`，第一个标题未带 `[DONE]` 的任务是 `M4-2 Unix socket server + 主循环请求分发`。
- 最新提交为 `[M4-1] Define scripting protocol messages`，未发现明确提到与 `M4-2` 直接相关的未完成 blocker。
- 当前仅处理 `M4-2`，完成并提交后停止，不继续 `M4-3`。
- 已新增 `src/ipc.rs`，实现 Unix socket listener、JSON line request/response、UI 请求 channel、pending `wait_for` 非阻塞轮询，以及测试客户端 helper。
- 已在 `AppHost::step` 与 crossterm runner 主循环中接入每帧 drain；`AppHost` 新增显式 IPC 启停方法，runner 支持 `ATTO_UI_SOCKET` 自动绑定。
- 已给 `DesktopInspector` 增加 crate 内部单次 wait 条件检查入口，供 IPC pending wait 每帧轮询使用。

## 约束与依据
- `TODO.md` 是任务排序和完成状态的唯一权威来源。
- 只处理第一个标题未带 `[DONE]` 的任务，完成后停止。
- 若遇到阻塞当前任务的规格不匹配、失败测试或缺失前置条件，优先修复；无法直接修复时，在 `TODO.md` 中插入最小必要前置任务并提交后停止。
- 完成任务后必须更新 `TODO.md`，标题加 `[DONE]`，填写完成记录，必要时才更新 `PLAN.md`。
- 验证顺序为 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、完整测试套件。若仅文档变化且已有可复用的绿色完整测试结果，可按要求跳过完整测试并记录原因。
- 完成后创建清晰 Git 提交，不继续下一个任务。

## 初始步骤
1. 已完成：读取 `TODO.md`，定位第一个未完成任务为 `M4-2`。
2. 已完成：查看最新提交信息，未发现直接相关未完成问题。
3. 读取 M4-2 涉及的协议、inspect、Desktop 主循环、测试宿主和现有 socket/channel 模式。
4. 已完成初版：设计并实现 Unix socket server：监听路径来自环境变量，接收线程解析 M4-1 协议，经 channel 转交 UI 线程，UI 线程 drain 请求并用 `desktop.inspect()` 执行。
5. 已完成初版：实现响应回传和错误映射，确保畸形请求、执行错误返回协议 `error`，不 panic。
6. 已完成初版：服务端 `wait_for` 不调用阻塞式 inspector wait，而是保存 pending wait，每帧按 poll interval 检查一次，满足或超时后回响应，因此不阻塞其他请求。
7. 已完成初版：新增 IPC 测试覆盖 query/invoke、wait_for 不阻塞其他 query、Focused target 在 modal 激活时命中 modal 焦点。
8. 已完成：`cargo test -p atto-ui ipc -- --nocapture`、`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、30 分钟超时保护下的完整 `cargo test --workspace --all-targets` 均通过。
9. 已完成：更新 `TODO.md`，将 `M4-2` 标为 `[DONE]` 并填写完成记录和验证命令。
10. 说明：完整测试后只修改了 `TODO.md` 和本计划文件，未改动编译输出，因此不需要重跑完整测试。
11. 已完成：检查 Git 状态并提交本次任务相关全部未提交文件，提交信息为 `[M4-2] Add IPC socket server dispatch`。
12. 本轮停止，不处理下一项任务。
