# 当前执行计划

## 范围

- 本次调用只处理 `TODO.md` 中第一个标题未带 `[DONE]` 前缀的任务。
- `TODO.md` 是任务顺序、依赖、验收和完成记录的唯一权威来源。
- `PLAN.md` 只在阶段级计划、依赖或完成标准变化时更新。

## 执行步骤

1. 读取 `TODO.md`，按文件顺序识别第一个未完成任务。
2. 检查最近提交信息；仅当最近提交明确提到与当前任务直接相关的未完成问题时，将其纳入当前任务或作为前置任务记录到 `TODO.md`。
3. 针对当前任务阅读相关代码、测试和文档，确认实现范围与验收要求。
4. 如果发现阻塞当前任务的具体缺陷、缺失功能或规格不匹配，优先修复；若无法在本次任务中正确修复，则在 `TODO.md` 中插入最小必要前置任务并停止。
5. 以最小正确改动实现当前任务，避免规避、夹具专用 hack 或削弱规格。
6. 按要求运行格式化、lint 和相关/完整测试；如出现未被明确排期的失败测试或夹具，修复或在 `TODO.md` 中加入必要任务。
7. 任务完成后，在 `TODO.md` 中给任务标题加 `[DONE]` 并更新完成记录。
8. 如阶段计划未变化，不更新 `PLAN.md`。
9. 提交本次相关变更，提交信息包含任务编号和清晰描述。
10. 完成第一个未完成任务后停止，不继续处理后续任务。

## 进度记录

- 已创建本计划文件。
- 已读取 `TODO.md`，首个未完成任务为 `T17 — Python 组件覆盖 + 上层注册 + 主题（B.2/B.3）`。
- 已查看最近提交：`[R16] Review typeahead command palette`，未发现直接声明与 T17 相关的未完成前置问题。
- T17 范围：补齐 Python 内置组件构造助手，暴露上层 runtime component 注册，支持主题设置/主题文件加载，补充 `.pyi` 类型声明和 schema 驱动 `set_prop` 校验，扩充 Python e2e 到至少 15 个，并验证 maturin 打包路径和示例。
- 已审阅 `crates/atto-ui-python` 的 native 绑定、Python wrapper、e2e、README、示例，以及 core runtime/schema/theme 和上层组件注册入口。
- 已实现 native `register_all_runtime_components`、`AppHost.set_theme()`、`AppHost.load_theme()`；Python 高层 `App` 暴露 `schemas()`、`set_theme()`、`load_theme()`。
- 已补齐 core 和上层组件 Python helper、schema 驱动 `set_prop` 校验、tree-op `set_prop` 校验、`.pyi` 类型声明和 `py.typed`。
- 已扩充 Python e2e 到 15 个用例，覆盖 T17 的组件构造、上层注册、主题、typing 和 schema 校验要求。
- 验证通过：`python3 -m py_compile atto_ui/__init__.py tests/test_e2e.py examples/minimal_app.py`；`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-python`；`maturin develop`；`python -m unittest discover tests`（15 tests）；`cargo test --workspace --all-targets`。最后一次 Python helper 微调后已复跑 `python3 -m py_compile ...` 与 `python -m unittest discover tests`。
- 已将 `TODO.md` 中 T17 标题标记为 `[DONE]`，并写入完成记录与验证命令。
- 下一步：检查 git status/diff/log，确认仅包含本任务相关改动后提交。
