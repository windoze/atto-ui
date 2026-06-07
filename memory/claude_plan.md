# 执行计划

## 当前约束

- 先依据 `TODO.md` 找到第一个标题未以 `[DONE]` 开头的任务。
- 只完成该任务，验证通过后更新 `TODO.md` 并提交，然后停止。
- 若发现当前任务被未跟踪的前置问题阻塞，优先在 `TODO.md` 中插入最小必要前置任务，提交后停止。
- `PLAN.md` 只在阶段级计划、依赖或完成标准变化时更新。
- 不使用 workaround；若实现路径存在缺口，修复缺口或把它明确排入 `TODO.md`。

## 步骤计划

1. 读取 `TODO.md`，按顺序确认第一个未完成任务及其要求、依赖和验证标准。
2. 检查最近提交是否显式提到与该任务直接相关的未完成问题。
3. 针对当前任务读取必要源码、测试和文档，限定范围，不做无关历史问题扫查。
4. 如任务可直接执行，实施最小正确改动；如遇阻塞，更新 `TODO.md` 记录前置任务并停止。
5. 按要求运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`，随后运行相关测试；如代码发生实质变更，再运行完整测试套件。
6. 处理所有观察到且未显式排期的测试/fixture 失败。
7. 将当前任务标题标记为 `[DONE]`，更新完成记录；仅在阶段计划变化时更新 `PLAN.md`。
8. 检查 `git status`、`git diff`、最近提交，确认只提交本次任务相关文件。
9. 使用描述性提交信息提交改动，然后停止，不进入下一任务。

## 进度记录

- 已创建本计划文件，下一步读取 `TODO.md` 并识别第一个未完成任务。
- 已确认第一个未完成任务为 `NT14 — host 组件库 + JSX 类型 + 受控输入（U.6）`。
- 最近提交为 `NR13` 审阅 React desktop/window mapping，未发现最新提交显式标注与 NT14 直接相关的未完成 blocker；下一步限定在 `packages/react`、`packages/core` 类型和相关 PTY/JS 测试中分析实现面。
- 已分析现有 React reconciler：host 事件名默认由 `onXxx` 映射为 runtime `xxx`，但 TextBox/ListBox/Table 等 `change` 事件当前无 payload，无法可靠实现受控 wrapper 的值回传。
- 调整计划：先补运行时 change/select 类事件 payload（TextBox/TextArea string、选择类 u64、Checkbox bool、Slider f64），再新增 React typed wrappers 与 JSX intrinsic 类型；测试覆盖 wrapper lowering、payload 分发、headless/PTY 受控 TextBox。
- 已完成主要代码改动：新增 React host wrapper 组件、JSX intrinsic 声明、TSX 类型用例、components PTY 用例，并补齐 runtime change 事件 payload 与相关 Rust 回归测试。下一步运行格式化、类型检查与测试，根据失败继续修正。
- 验证已完成并通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、React typecheck/build、`cargo test --all --all-targets`、napi build、core/node/react JS 测试、`git diff --check`。`tools/run_fixtures.py` 不存在，独立 fixture 套件无可运行入口。
- 已将 NT14 在 `TODO.md` 与 `TODO-1.md` 标记为 `[DONE]` 并写入完成记录。下一步检查 diff/status 后提交本次任务相关文件。
