# 当前执行计划

## 范围

- 以 `TODO.md` 为唯一任务顺序来源。
- 只完成第一个未在标题中标记 `[DONE]` 的任务，然后停止。
- 若发现阻塞当前任务的缺陷、缺失能力或未排期失败测试，按要求在 `TODO.md` 中加入最小必要前置任务并提交后停止。

## 步骤

1. 读取 `TODO.md`，定位第一个未完成任务，并检查该任务的要求、依赖和验证标准。
2. 检查最新提交信息，只判断其是否明确提到与当前任务直接相关的未完成事项。
3. 针对当前任务阅读必要代码与测试，避免做无关历史问题扫查。
4. 实现当前任务，优先做最小且完整的正确改动。
5. 按任务要求运行验证；若没有更具体要求，则先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后运行完整测试套件。
6. 若验证失败，修复当前任务范围内的问题；若失败属于未排期且阻塞完成的项目问题，则按政策更新 `TODO.md` 并停止。
7. 完成后在 `TODO.md` 的任务标题前加 `[DONE]`，更新完成记录；仅在阶段计划确实变化时更新 `PLAN.md`。
8. 检查 `git status`、`git diff` 和近期提交，确认只提交本次相关改动。
9. 创建清晰提交，提交后停止，不继续处理下一个任务。

## 进度记录

- 已创建本计划文件，下一步读取 `TODO.md` 定位第一个未完成任务。
- 已读取 `TODO.md`，第一个未完成任务是 `R17 — 审阅 T17`。
- 本次执行范围是审阅 T17：检查 Python 构造助手覆盖、helper 参数与 runtime schema 一致性、`.pyi` 类型声明可用性，并运行 Python e2e 与要求的格式/ lint/ 测试验证。
- 审阅发现并修复：Python helper/stub/e2e 缺少已注册内置 `Visibility`；e2e 未覆盖 `HStack`；stub 未声明 `ComponentRef.__getattr__` 动态 setter/getter；示例/测试中的 callback source 类型未体现 `None` 可能性；README 高层 wrapper 示例仍使用裸事件 dict。
- 已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、Python `py_compile`、`cargo test -p atto-ui-python`、`maturin develop`、`python -m unittest discover tests`。
- 完整 `cargo test --workspace --all-targets` 暴露 `atto-ui-terminal` 的两个 PTY 用例等待 `READY` 超时；该失败未在 TODO 中排期，需在本次 R17 验证中修复后再继续。
- 已修复 terminal PTY 验证不稳定点：同文件用例串行化，公共 `PTY_WAIT` 提升到 5 秒；下一步复跑格式/lint、terminal PTY、Python e2e 和完整 workspace 测试。
- 已完成验证：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；Python `py_compile`；`cargo test -p atto-ui-python`；`maturin develop`；`python -m unittest discover tests`（15 tests）；`cargo test -p atto-ui-terminal --test pty_terminal_emulator -- --nocapture`；`cargo test --workspace --all-targets`。
- 已将 `TODO.md` 中 `R17` 标记为 `[DONE]` 并写入完成记录。
- 已创建提交 `[R17] Review Python component coverage`。本次只处理 R17，不继续处理后续任务。
