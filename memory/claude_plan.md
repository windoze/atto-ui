# 执行计划

## 当前状态
- 本轮目标：按照 `TODO.md` 的顺序完成第一个未完成任务，然后停止。
- 约束：`TODO.md` 是任务顺序、要求、依赖和完成记录的唯一权威来源；只完成一个任务；完成后必须更新 `TODO.md` 并提交 Git。
- 说明：这里记录的是可公开的执行思路、计划和进度，不包含不可见的内部推理细节。
- 已识别当前任务：`M2-6 用 wait_for / invoke 迁移一批 chat 逻辑测试`。
- 最新提交：`f195d6c [M2-5] Add in-process semantic scripting API`，与当前任务直接相关，作为本任务可用前置能力处理。

## 步骤计划
1. 读取 `TODO.md`，按标题是否带有 `[DONE]` 判断第一个未完成任务。
2. 查看最新提交信息，确认是否有与该任务直接相关的未完成事项；仅在其阻塞当前任务时纳入范围或写入 `TODO.md` 作为前置任务。
3. 阅读当前任务涉及的项目文档和代码区域，确定实现边界、测试要求和依赖。
4. 实现当前任务；如果发现阻塞性规格不匹配或必须新增前置任务，则更新 `TODO.md`，提交后停止。
5. 按要求运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，再运行完整测试套件；发现未调度失败时必须修复或写入 `TODO.md`。
6. 完成后将当前任务标题加上 `[DONE]`，补充完成记录；只有阶段级计划变化时才更新 `PLAN.md`。
7. 提交所有相关变更，提交信息包含任务编号和明确动作。
8. 停止，不继续处理下一项任务。

## 进度记录
- 已创建本计划文件，下一步读取 `TODO.md` 识别第一个未完成任务。
- 已读取 `TODO.md` 并确认第一个未完成任务为 `M2-6`；下一步读取 `crates/atto-ui-chat/tests/pty_chat.rs`、现有 inspect chat 测试和 chat 组件实现。
- 已实现 `ChatInputPanel` 的语义 `InputText` / `SelectIndex` / `Submit` 命令，复用现有文本粘贴、selection clamp 和 `emit_response` 路径。
- 已新增进程内 inspect 测试，使用 `invoke` / `wait_for` / `wait_for_predicate` 覆盖 text submit、choice/confirm submit、streaming queue。
- 已删除两段被迁移的 PTY 纯逻辑用例，保留模式渲染、补全、滚动和消息列表端到端 PTY 覆盖。
- 聚焦验证已通过：`cargo test -p atto-ui-chat --test inspect_chat -- --nocapture`、`cargo test -p atto-ui-chat --test pty_chat -- --nocapture`、`cargo test -p atto-ui-chat`。
- 仓库级验证已通过：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。
- 在收紧 `ChatInputPanel::supports_command` 为当前 mode 感知后，已重新通过：`cargo test -p atto-ui-chat --test inspect_chat -- --nocapture`、`cargo test -p atto-ui-chat`、`cargo fmt --all -- --check`、`git diff --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`。
- 已将 `TODO.md` 中 `M2-6` 标记为 `[DONE]` 并补充完成记录与验证记录；`PLAN.md` 未改动，因为阶段级计划无变化。
