# 执行计划

当前状态：已读取 `TODO.md`，确认第一项未完成任务是 `M2-2 Button apply_command`。最新提交为 `[M2-1] Implement checkbox apply command`，未发现需要改变当前任务排序的直接未完成问题。后续每完成关键步骤或调整计划都会更新。

## 步骤

1. 阅读 `src/widgets/button.rs`、`src/component_api.rs` 和相邻组件实现，确认 `Button` 既有鼠标 / 键盘激活路径与禁用态语义。
2. 为 `Button::apply_command` 实现 `ComponentCommand::Click` / `ComponentCommand::Submit`，复用现有激活路径触发回调；禁用态返回 `ignored()`。
3. 新增进程内单测，覆盖 `Click` 触发激活回调、`Submit` 触发同一路径、禁用按钮不触发且返回 ignored。
4. 运行相关按钮测试，再按要求运行 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
5. 若验证全部通过，更新 `TODO.md`：把 `M2-2` 标为 `[DONE]`，填写完成记录和验证结果。
6. 提交本次所有相关更改，提交信息包含 `M2-2`，然后停止。

## 进度记录

- 已创建本计划文件。
- 已确认当前任务：`M2-2 Button apply_command`。
- 已实现 `Button::apply_command` 的 `Click` / `Submit` 语义派发，并新增进程内单测覆盖启用态回调与禁用态 ignored。
- 聚焦验证 `cargo test -p atto-ui button -- --nocapture` 已通过。
- `cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings` 已通过。
- 完整验证 `python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets` 已通过。
- 已将 `TODO.md` 中 `M2-2` 标记为 `[DONE]`，并补充完成记录与验证命令。
- 提交前检查 `git diff --check` 已通过；下一步提交本任务变更。
