# 当前任务计划：M4.4 自动加载

## 范围

- 只完成 `TODO.md` 中第一个未完成任务：`M4.4 自动加载`。
- 目标：对用户 prompt 与 skill 的 `name`、`description`、`triggers` 做确定性词匹配，并限制最多自动加载数量。
- 不推进 `M4.5` 及后续任务；不修改 `PLAN.md`，除非发现阶段级依赖或完成标准需要调整。

## 执行步骤

1. 检查最近提交与当前 worktree，确认是否存在与 `M4.4` 直接相关的未完成事项或冲突。
2. 阅读 skill registry、loaded skill set、agent submit/turn loop、slash 命令与状态栏相关代码，定位自动加载应接入的位置。
3. 设计最小实现：新增确定性词匹配逻辑、自动加载上限常量或配置入口，并保证手动加载的 skill 不被重复计数。
4. 在用户提交 prompt 时执行自动匹配和加载，更新 loaded skill 数与可见反馈；保持 `/skill` 手动加载行为不变。
5. 添加或更新单元测试，覆盖 name、description、triggers 匹配、大小写/分词、上限限制、重复已加载 skill 等核心行为。
6. 运行 `cargo fmt --all`，再运行 `cargo clippy --workspace --all-targets -- -D warnings`，最后运行 `cargo test --workspace --all-targets`。
7. 将 `TODO.md` 中 `M4.4` 标记为 `[DONE]` 并写入完成记录与验证命令。
8. 检查 `git status` / `git diff` / 最近提交，提交本次任务相关全部变更，然后停止。

## 进度记录

- 已确认首个未完成任务为 `M4.4 自动加载`。
- 已检查仓库状态：当前只有本计划文件变更；最近提交为 `[M4.3] Add manual skill loading`，未发现直接阻塞 `M4.4` 的未完成事项。
- 已阅读相关代码：`skill.rs` 负责解析、发现和 loaded set，`lib.rs::submit_input_response` 是普通用户 prompt 提交入口，`/skill` 已经更新 `skill_count_state`。
- 实现决策：只让 `mode: auto` 的 skill 参与自动加载；用大小写不敏感的确定性词 token 交集匹配 `name`、`description`、`triggers`；按 registry 名称顺序最多加载固定数量；不做 M4.5 prompt 注入，不改变工具权限。
- 已在 `skill.rs` 添加默认自动加载上限、匹配 API、token 化辅助函数和单元测试；已在 `lib.rs::submit_input_response` 接入自动加载，并新增 app 层提交测试。
- 已运行 `cargo fmt --all` 和 `cargo clippy --workspace --all-targets -- -D warnings` 通过。
- 首次完整测试有 1 个新增测试失败：测试 prompt 使用 `regressions`，而实现按任务要求做确定性词匹配、不做词干化。下一步修正测试输入为同一词形后重新验证。
- 已修正新增单测输入，并重新运行验证通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 已更新 `TODO.md`，`M4.4` 已标记 `[DONE]` 并写入完成记录。已检查 git 状态、diff 和最近提交，变更范围符合本任务。
- 下一步：提交本次任务变更并停止。
