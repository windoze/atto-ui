# Claude 执行计划

> 说明：本文件记录可审查的执行计划、关键决策和进度更新；不记录不可见的内部推理链。

## 当前目标

- 以 `TODO.md` 为唯一任务排序与完成状态来源。
- 找出第一个标题未带 `[DONE]` 的任务。
- 完成且只完成该任务，验证后更新 `TODO.md` 并提交。

## 初始执行步骤

1. 读取 `TODO.md`，按文件顺序确认第一个未完成任务。
2. 检查最新提交信息，若明确提到与当前任务直接相关的未完成问题，则把它纳入当前任务或作为先决任务记录。
3. 阅读当前任务的要求、依赖、验证要求和完成记录。
4. 只检查与当前任务相关的代码、测试和文档，避免开放式历史问题扫描。
5. 若发现当前任务被具体缺陷、缺失功能或测试/fixture 失败阻塞，则优先修复；若无法在本轮正确修复，则在 `TODO.md` 中添加最小先决任务并停止。
6. 按仓库风格实施最小正确改动。
7. 运行格式化、lint 和任务要求的测试；如代码变更影响全局行为，则运行完整测试套件。
8. 将任务标题标记为 `[DONE]`，更新完成记录；只有文档或计划变更时按要求说明跳过测试的原因。
9. 检查 git 状态、diff 和最近提交，提交本轮全部相关改动。
10. 停止，不继续下一个任务。

## 进度记录

- 已建立本轮执行计划。下一步读取 `TODO.md` 识别第一个未完成任务。
- 已读取 `TODO.md`：首个未完成任务为 `T19 — A.2 P1/P2 测试补齐 + 一致性收尾（含 L2）`。
- 已检查最近提交摘要：最新提交为 `[R18] Record completion plan`，未发现直接指向 T19 的未完成阻塞项。
- 已确认 `Button` 的 L2 命中判断当前已实现；补充 disabled 状态单测以覆盖状态矩阵。
- 已修复 `RadioGroup` 鼠标命中只看行不看列的问题，改为基于组件区域 contains 后再选择选项。
- 已补充核心控件/响应式/theme/window manager/Grid 行为单测，并新增 T19 core widgets PTY fixture 与 Markdown block PTY fixture。
- 验证进展：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui`、`cargo test -p atto-ui-markdown`、`cargo test --workspace --all-targets` 已通过。
- 覆盖率验证受阻：`cargo llvm-cov -p atto-ui --summary-only` 失败，原因是本机未安装 `cargo-llvm-cov`。下一步尝试安装该工具后复跑覆盖率。
- 已安装 `cargo-llvm-cov` 并完成覆盖率验证：`cargo llvm-cov -p atto-ui --summary-only --ignore-filename-regex '(^|/)(demos|src/bin)/'` 通过，核心源码行覆盖率为 70.74%。

## T19 执行计划

1. 检查当前工作区状态，避免覆盖他人改动。
2. 阅读 T19 涉及的组件与现有测试：`Button`、ListBox/TableView、Grid/Splitter、Markdown、theme/reactive/window manager 等。
3. 优先修复 L2：在 `widgets/button.rs` 保存 `last_area` 并在 `Down(Left)` 前执行 contains 命中判断。
4. 按 T19 验收补足缺口测试，优先选择最小但真实覆盖公开控件行为的单测或 PTY 测试。
5. 如发现必须先修复的具体测试/fixture 失败或缺失能力，直接修复；若无法正确修复，则更新 `TODO.md` 增加最小先决任务并停止。
6. 按要求运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、相关测试、完整 workspace 测试；覆盖率命令按可用情况执行。
7. 更新 `TODO.md` 将 T19 标记 `[DONE]` 并写入完成记录，随后提交本轮相关改动。
