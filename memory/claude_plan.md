# 执行计划

## 约束说明
- 不记录不可公开的逐字内部推理；记录可审计的判断依据、执行步骤、验证顺序和进度。
- `TODO.md` 是任务顺序和完成状态的唯一来源；只处理第一个标题未带 `[DONE]` 的任务。
- 先确认最新提交是否直接提到与当前任务相关的未完成问题；若直接阻塞当前任务，则纳入当前任务或作为前置任务写入 `TODO.md`。
- 每次只完成一个任务；完成后更新 `TODO.md`、运行规定验证、提交 Git，然后停止。

## 初始步骤
1. 读取 `TODO.md`，定位第一个未完成任务。已完成：当前任务为 `M2-3 TextBox apply_command`。
2. 查看最新提交信息，仅判断是否存在与该任务直接相关的未完成事项。已完成：最新提交为 `[M2-2] Implement button apply command`，未发现直接阻塞 `M2-3` 的未完成事项。
3. 阅读当前任务涉及的代码、测试和文档，明确实现边界。已完成：`InputText` 按现有粘贴语义实现为在当前光标处插入，若存在选区则先替换选区；禁用态保持 ignored。

## 执行步骤
1. 按任务要求实现最小但完整的代码或文档变更。已完成：`TextBox::apply_command(ComponentCommand::InputText)` 复用新私有 `insert_text_at_cursor` helper；`Event::Paste` 和 Ctrl+V 也走同一 helper。
2. 若发现阻塞当前任务的规格不匹配、已存在缺陷或测试失败，优先修复；无法在本任务内正确修复时，将最小前置任务插入 `TODO.md` 并停止。
3. 变更过程中在关键节点更新本文件，记录已完成步骤和计划调整。已完成：已补 TextBox 进程内单测，验证全部通过。
4. 使用小而聚焦的补丁修改文件，并避免回退用户已有改动。

## 验证顺序
1. 运行 `cargo fmt`。已完成：`cargo fmt --all` 通过。
2. 运行 `cargo clippy --all-targets -- -D warnings`。已完成：`cargo clippy --workspace --all-targets -- -D warnings` 通过。
3. 运行完整测试套件，优先使用 `cargo test --all --all-targets`，超时不超过 30 分钟。已完成：`python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets` 通过。
4. 若本次只改文档且自上次完整绿测后无代码变化，可按任务说明跳过完整测试并在完成记录中注明。

## 完成步骤
1. 在 `TODO.md` 对当前任务标题加 `[DONE]`，并填写完成记录和验证结果。已完成。
2. 仅当阶段级计划变化时更新 `PLAN.md`。
3. 检查工作树，提交所有与本任务相关的未提交改动。提交前检查已完成：`git diff --check` 通过，当前变更范围为 `TODO.md`、`memory/claude_plan.md`、`src/widgets/textbox.rs`；下一步执行 Git 提交。
4. 停止，不继续处理下一个任务。
