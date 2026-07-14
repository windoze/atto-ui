# 执行计划

## 当前状态

- 已读取 `TODO.md`，第一个未完成任务是 `M1-R Review — 第 1 层完整性与正确性复核`。
- 已检查最新提交：`b08b4fe [M1-5] Add chat inspector assertion sample`。该提交对应已完成的 M1-5，没有明确提出阻塞 M1-R 的未完成问题。
- 不记录隐藏推理链，只记录可审计的决策、依据、步骤和进度。

## 步骤计划

1. 复核公共 `find_by_tag` / `find_by_tag_mut` 的 root-first DFS 语义、同名 tag 首个匹配、深层嵌套和 mutable 路径；确认 `inspect.rs` 没有残留重复递归实现。
2. 复核 `DesktopInspector` 第 1 层只读门面：`tree` / `export_snapshot` / `get_property` / `set_property` / `property_names` 的寻址收敛情况，并确认没有把 M2 动作语义混入第 1 层新增能力。
3. 复核 `untagged_interactive_nodes` 与 dirty change tracker 均为进程内诊断 / 拉模型信号，不改变事件、绘制或交互语义。
4. 复核 chat 示例迁移：新增测试应通过 `DesktopInspector` 读属性断言状态，不依赖 `find_text_position`、字形推断或屏幕 OCR；原 PTY 端到端覆盖保留。
5. 确认 `#![forbid(unsafe_code)]` 仍保留在相关 crate 根。
6. 运行验证，顺序为：
   - `cargo fmt`
   - `cargo clippy --all-targets -- -D warnings`
   - 完整测试套件，按项目需要使用不超过 30 分钟的超时
7. 若发现未排期的测试失败，必须修复，或在 `TODO.md` 中排入正确顺序的前置/后续任务，不能把当前任务标记完成。
8. 完成后更新 `TODO.md`：给 M1-R 标题加 `[DONE]`，并填写复核结论与验证记录；本次若只完成任务复核，不更新 `PLAN.md`。
9. 检查工作区变更，提交所有与本次任务相关且需要纳入的文件，提交信息包含任务编号和清晰描述。
10. 提交后停止，不继续处理下一个任务。

## 进度记录

- 计划文件已初始化。
- 已定位当前任务：M1-R Review。
- 已完成代码层复核：
  - `find_by_tag` / `find_by_tag_mut` 是 root-first DFS；`inspect.rs` 中 `component_find` / `component_find_mut` 仅委托公共 API。
  - `property_names` / `get_property` / `set_property` 的组件路径复用公共寻址；M1 新增只读 / 诊断 / change tracker API 不调用动作语义。
  - `untagged_interactive_nodes` 基于绘制后的 inspect tree 只读筛选；dirty change tracker 基于 `DirtyObserver` 做 per-consumer 拉模型检测，不清除全局 dirty。
  - chat 示例测试通过 `DesktopInspector` 读取 `mode` / `draft` 等属性；原 PTY OCR/helper 覆盖仍保留在端到端测试中。
  - `src/lib.rs` 仍保留 `#![forbid(unsafe_code)]`。
- 下一步开始验证：`cargo fmt`、`cargo clippy`、完整 workspace test。
- 验证已完成并通过：
  - `cargo fmt --all`
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `python3 -c 'import subprocess, sys; subprocess.run(sys.argv[1:], timeout=1800, check=True)' cargo test --workspace --all-targets`
- 已更新 `TODO.md`，将 M1-R 标记为 `[DONE]` 并补充完成记录与验证记录。
- 下一步检查工作区变更并提交。
