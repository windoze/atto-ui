# Claude 执行计划

## 当前约束
- 先读取 TODO.md，按文件中第一个未以 [DONE] 标记的任务执行。
- 每次只完成一个任务，完成后更新 TODO.md、验证、提交并停止。
- 如遇阻塞，不用变通方案；在 TODO.md 中加入最小必要前置任务并提交后停止。
- PLAN.md 只在阶段级计划变化时更新。

## 初始计划
1. 读取 TODO.md，确定第一个未完成任务，并检查任务的依赖、验证要求和完成记录要求。
2. 检查最新提交信息，只判断它是否明确提到与当前任务直接相关的未完成问题。
3. 根据当前任务读取最小必要的代码和测试上下文。
4. 实现当前任务或处理直接阻塞当前任务的前置问题。
5. 运行 cargo fmt、cargo clippy --all-targets -- -D warnings，再运行必要测试；若代码变更影响面较大，运行完整测试套件。
6. 更新 TODO.md：给完成任务标题加 [DONE]，补充完成记录；仅在阶段级变化时更新 PLAN.md。
7. 检查 git diff，提交本次任务相关的所有未提交变更。
8. 停止，不继续下一个 TODO 任务。

## 进度
- 已读取 TODO.md，确定第一个未完成任务为 `M1-5 进程内读值断言范式 + 示范迁移一例 chat 逻辑测试`。
- 已检查最新提交 `14e8dc5 [M1-4] Add desktop change tracker`，未发现直接关联 M1-5 的未完成问题；初始未提交变更只有本计划文件。
- 已定位示范迁移目标：把 `pty_chat.rs` 中输入模式状态依赖屏幕文字判断的一例，补成进程内 `DesktopInspector` 读 `ChatInputPanel.mode` / `property_names` 的断言样板。
- 已实现 `ChatInputPanel::with_tag` 与 `DynamicTree::tag()`，并让 `ChatPanel` 透明转发内部 `VStack` 的 `children()` / `children_mut()`，使 Desktop introspection 可按 tag 找到 chat input。
- 已新增 `crates/atto-ui-chat/tests/inspect_chat.rs`，构造带 tagged chat input 的 `Desktop`，通过 `desktop.inspect()` 读取属性并断言 mode 从 `text` 更新为 `choice`。
- 聚焦测试 `cargo test -p atto-ui-chat chat_input_mode_state_is_readable_through_desktop_inspector -- --nocapture` 已通过；修复了一个 unused import warning。
- `cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings` 已通过。
- `cargo test -p atto-ui-chat` 已通过，覆盖新增 `inspect_chat.rs` 和既有 `pty_chat.rs`。
- 完整 workspace 测试失败于 `atto-ui --test pty_virtual_scrolling`，6 个用例均在初始 `wait_for_text` 超时且屏幕为空。该失败未在 TODO 中显式排期，按测试失败策略必须修复或排期后才能完成 M1-5。

## 当前阻塞处理计划
1. 单独复现 `cargo test -p atto-ui --test pty_virtual_scrolling -- --nocapture`，判断失败是否稳定。
2. 阅读 `tests/pty_virtual_scrolling.rs` 与对应 snapshot app，确认是否存在同一 PTY fixture 并发运行导致空屏的隔离缺口。
3. 若是测试隔离问题，采用和 chat PTY 测试一致的文件级互斥锁，修复整类同文件 PTY 并发失败；若是实际 app 渲染问题，定位并修复对应渲染/启动路径。
4. 重跑该测试文件，再重跑完整 workspace 测试。

## 阻塞处理进展
- 单独运行 `cargo test -p atto-ui --test pty_virtual_scrolling -- --nocapture` 已通过，说明 virtual scrolling 功能路径本身未失败。
- 已在 `tests/pty_virtual_scrolling.rs` 添加文件级 `VIRTUAL_SCROLL_PTY_LOCK`，序列化同一 snapshot PTY fixture 的 6 个测试，修复完整套件负载下并发启动空屏超时的隔离缺口。
- `cargo fmt --all && cargo test -p atto-ui --test pty_virtual_scrolling -- --nocapture` 已通过。
- 修复后复跑 `cargo fmt --all -- --check` 与 `cargo clippy --workspace --all-targets -- -D warnings` 已通过。
- 修复后完整测试 `python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets` 已通过。
- 下一步更新 `TODO.md`：将 M1-5 标为 `[DONE]`，补完成记录和验证记录；`PLAN.md` 不需要更新，因为阶段级计划未变化。

## 当前任务具体计划
1. 检查最新提交信息与工作区状态，只确认是否存在直接关联 `M1-5` 的未完成问题。
2. 阅读 `crates/atto-ui-chat/tests/pty_chat.rs`、chat crate 结构和相关组件源码，找出一个适合从 PTY/OCR/字形推断迁移到进程内 `DesktopInspector` 读值断言的逻辑用例。
3. 检查目标 chat 组件是否已有稳定 `tag` 和可读属性；如缺少，按 M1-3 约定补标 tag 或暴露现有 `Binding` 属性，不改变交互语义。
4. 新增或改写一例进程内逻辑测试：构造 `Desktop` 或 chat 根组件，调用 `desktop.inspect()`，通过 `property_names` / `get_property` 读取活值断言；保留原 PTY 中真正覆盖渲染 / 端到端行为的部分。
5. 运行聚焦测试 `cargo test -p atto-ui-chat`，再按要求运行 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
6. 更新 `TODO.md`：给 `M1-5` 标题加 `[DONE]`，补完成记录和验证命令；仅当阶段计划变化时才更新 `PLAN.md`。
7. 检查 diff 并提交本次任务所有相关变更，然后停止。
