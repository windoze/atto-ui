# Claude 执行计划

> 说明：本文件记录可审查的执行计划、关键决策和进度更新；不记录不可见的内部推理链。

## 当前目标

- 以 `TODO.md` 为唯一任务排序与完成状态来源。
- 找出第一个标题未带 `[DONE]` 的任务。
- 完成且只完成该任务，验证后更新 `TODO.md` 并提交。

## 通用执行步骤

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

## 历史进度记录

- 已建立上一轮执行计划并完成 T19。
- T19 首个未完成任务识别为 `T19 — A.2 P1/P2 测试补齐 + 一致性收尾（含 L2）`。
- T19 最新提交检查时最新提交为 `[R18] Record completion plan`，未发现直接指向 T19 的未完成阻塞项。
- T19 已确认 `Button` 的 L2 命中判断并补充 disabled 状态单测。
- T19 已修复 `RadioGroup` 鼠标命中只看行不看列的问题，改为基于组件区域 contains 后再选择选项。
- T19 已补充核心控件/响应式/theme/window manager/Grid 行为单测，并新增 T19 core widgets PTY fixture 与 Markdown block PTY fixture。
- T19 验证通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui`、`cargo test -p atto-ui-markdown`、`cargo test --workspace --all-targets`。
- T19 已安装 `cargo-llvm-cov` 并完成覆盖率验证：`cargo llvm-cov -p atto-ui --summary-only --ignore-filename-regex '(^|/)(demos|src/bin)/'` 通过，核心源码行覆盖率为 70.74%。

## R19 执行计划

1. 读取 `TODO.md` 并确认第一个未完成任务。
2. 检查最新提交信息，确认是否有与 R19 直接相关的未完成问题。
3. 审阅 T19 相关改动，重点确认覆盖率记录、Button 命中判断和其他 widget 命中一致性。
4. 如发现审阅问题，做最小正确修复并补充回归测试。
5. 依次运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`。
6. 运行 `cargo llvm-cov -p atto-ui --summary-only --ignore-filename-regex '(^|/)(demos|src/bin)/'`，确认核心源码行覆盖率仍不低于 70%。
7. 将 `R19` 标记为 `[DONE]` 并写入完成记录。
8. 提交本轮 R19 相关改动并停止。

## R19 进度记录

- 已读取 `TODO.md`：首个未完成任务为 `R19 — 审阅 T19`。
- 已检查最近提交摘要：最新提交为 `[T19] Add P1 P2 test coverage`，与 R19 直接相关，无额外未完成阻塞项。
- 已审阅 T19 的覆盖率记录、Button L2 命中判断、core widget PTY/单测、Markdown block 覆盖与 theme/reactive/window manager 测试补齐。
- 审阅发现同类命中一致性缺口：`Checkbox` 未校验左键事件是否命中自身绘制区域。
- 已修复 `Checkbox`：保存 `last_area`，左键按下前使用统一命中判断，并新增 `mouse_down_requires_last_area_hit` 单测覆盖区域外点击不切换、区域内点击切换。
- 验证通过：`cargo fmt`。
- 验证通过：`cargo clippy --workspace --all-targets -- -D warnings`。
- 验证通过：`cargo test --workspace --all-targets`。
- 覆盖率验证通过：`cargo llvm-cov -p atto-ui --summary-only --ignore-filename-regex '(^|/)(demos|src/bin)/'`，核心源码行覆盖率为 70.81%。
- 已将 `R19` 标记为 `[DONE]` 并更新 `TODO.md` 完成记录。
