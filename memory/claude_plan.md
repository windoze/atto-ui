# 执行计划

## 当前目标

- 按 `TODO.md` 的顺序只完成第一个未标记 `[DONE]` 的任务，然后停止。
- `TODO.md` 是任务细节、依赖、验证要求和完成记录的唯一权威来源。
- `PLAN.md` 只在阶段级计划、依赖或完成标准发生变化时更新。

## 步骤

1. 读取 `TODO.md`，识别第一个标题未以 `[DONE]` 开头的任务，并记录任务范围、依赖和验证要求。
2. 检查最新提交信息，若其中明确提到与当前任务直接相关的未完成问题，则把它纳入当前任务或在 `TODO.md` 中作为前置任务处理。
3. 根据当前任务阅读相关代码和测试，避免无关的开放式历史问题排查。
4. 若任务可直接完成，则实施最小且完整的代码或文档修改；若发现具体阻塞项，则只添加必要的前置任务并停止。
5. 按要求运行验证：先 `cargo fmt`，再 `cargo clippy --all-targets -- -D warnings`，最后在需要时运行完整测试套件。
6. 如果发现未被计划覆盖的测试失败，立即修复，或在 `TODO.md` 中添加最小必要任务并保持当前任务未完成。
7. 完成后更新 `TODO.md`：给当前任务标题加 `[DONE]`，填写完成记录和验证结果；仅在阶段级计划变化时更新 `PLAN.md`。
8. 提交所有与本次任务相关的变更，提交信息包含任务编号或清晰描述。
9. 提交后停止，不继续处理下一个任务。

## 进度记录

- 已创建本执行计划，下一步读取 `TODO.md` 并确定第一个未完成任务。
- 已确认第一个未完成任务为 `M4.3 手动加载命令`；最近提交 `[M4.2] Add skill discovery paths` 未声明直接相关的未完成事项。
- 已定位 slash 命令、skill discovery registry 和状态栏 segment 实现；当前编辑目标是新增已加载 skill 状态、`/skill <name>` 激活命令、`/skills` 已加载标记和状态栏计数。
- 已完成核心实现与单元测试补充。首次 clippy 发现测试 helper 参数过多，已改为测试专用 `TestSkillState` 聚合参数，下一步重新运行格式化和 clippy。
- 已修复 PTY 测试旧文案断言，并修正 `/skill <name>` 帮助文本渲染。验证通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 下一步更新 `TODO.md`，将 `M4.3` 标记为 `[DONE]` 并记录完成内容与验证命令。
- 已更新 `TODO.md`，`M4.3` 已标记 `[DONE]` 并写入完成记录。下一步检查 git 状态和 diff，然后提交本次任务变更。
- 已修正 `skill.rs` 模块注释中关于 activation 的旧描述，并再次确认 `cargo fmt --all -- --check` 通过。
