# 执行计划

## 范围

- 以 `TODO.md` 为唯一任务顺序来源。
- 本次只完成第一个标题未带 `[DONE]` 的任务。
- 完成后提交并停止，不继续后续任务。

## 步骤

1. 读取 `TODO.md` 并确定第一个未完成任务。
2. 只检查实现该任务所需的相关文件。
3. 检查最近提交中是否有与当前任务直接相关的未完成事项。
4. 按任务要求实现，不通过缩窄范围或 workaround 规避问题。
5. 先运行 `cargo fmt`，再运行 `cargo clippy --workspace --all-targets -- -D warnings`，最后运行必要的定向测试和完整测试。
6. 如验证发现未排期失败，立即修复或在 `TODO.md` 中新增最少前置任务。
7. 在 `TODO.md` 中给已完成任务标题加 `[DONE]` 并填写完成记录。
8. 关键步骤完成或计划变化时更新本文件。
9. 检查 `git status`、`git diff`、最近提交，确认提交范围后提交并停止。

## 当前状态

- 已确定第一个未完成任务：`TODO.md` 中的 `P4.3 多行编辑增强`。
- 任务范围：检查 chat 输入实现与测试，实现多行粘贴规整，不进入 `P4.4`。
- 已在 `crates/atto-ui-chat/src/input.rs` 实现 chat 文本粘贴规整：容错剥离 bracketed-paste 包裹、统一 CRLF/CR 为 LF、去除粘贴尾部空白行，并通过 `TextArea::replace_byte_range` 插入以保持光标、binding 与内部缓冲同步。
- 已新增单测覆盖规整规则、多行粘贴后继续输入，以及多行粘贴提交/历史记录使用规整后文本。
- 已通过验证：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-chat input --lib`、`cargo fmt --all -- --check`、`cargo build --workspace --all-targets`、`cargo test --all --all-targets`。
- 已更新 `TODO.md`：`P4.3 多行编辑增强` 已标记 `[DONE]` 并补充完成记录。
