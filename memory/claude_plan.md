# 执行计划

## 当前任务

- 首个未完成任务：`M4.1 Skill 文件格式`。
- 任务要求：解析 `SKILL.md` frontmatter 和 body，支持 `name`、`description`、`triggers`、`tools`、`mode`。
- 范围边界：只完成 M4.1，不推进 M4.2 及后续任务；搜索路径、命令加载、自动加载、prompt 注入和权限隔离仅在必要的数据结构边界上预留，不实现完整行为。

## 执行步骤

1. 检查最近提交和当前工作区状态，确认是否存在与 M4.1 直接相关的未完成事项或未提交变更。
2. 阅读 `PLAN.md`、`TUI_AGENT.md` 以及 `crates/atto-agent-app` 现有模块结构，确定 Skill 解析模块的放置位置和现有配置/工具类型可复用点。
3. 实现最小完整的 Skill 文件格式解析：读取 Markdown 文本，解析 YAML frontmatter，保留 body，并校验必需字段与支持字段。
4. 补充单元测试，覆盖有效 frontmatter、缺失/无效字段、默认值、列表字段、body 保留，以及未知 mode 或 malformed frontmatter 的错误。
5. 运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
6. 更新 `TODO.md`：将 M4.1 标记为 `[DONE]`，填写完成记录和验证命令；仅当阶段级计划改变时才更新 `PLAN.md`。
7. 检查 `git status`、`git diff`、`git log --oneline -10`，确认只提交本任务相关变更。
8. 提交 Git commit，提交信息使用 `[M4.1] Implement skill file parsing`。

## 进度记录

- 2026-07-10：已读取 `TODO.md`，确定当前任务为 M4.1，准备开始代码检查与实现。
- 2026-07-10：已新增 `atto_agent_app::skill` 解析模块、app crate `serde_yaml` 依赖和 M4.1 单元测试；下一步运行格式化、lint 和测试验证。
- 2026-07-10：`cargo test -p atto-agent-app skill` 的 lib 单测部分通过，但命令继续启动 `pty_agent` 集成测试二进制并在 120 秒超时；下一步用 `--lib` 复核新增单测，并单独诊断 PTY 过滤/耗时，确认是否为真实测试卡住。
- 2026-07-10：`cargo test -p atto-agent-app --lib skill` 通过；`cargo test -p atto-agent-app --test pty_agent -- --list` 正常列出 6 个 PTY 用例；`cargo clippy --workspace --all-targets -- -D warnings` 通过。下一步运行完整 workspace 测试。
- 2026-07-10：`cargo test --workspace --all-targets` 和 `cargo fmt --all -- --check` 通过；已将 `TODO.md` 中 M4.1 标记为 `[DONE]` 并写入完成记录。下一步检查 diff/status 后提交。
- 2026-07-10：staged diff 检查发现测试 raw string 中存在尾随空白；已改为空行形式的空 body 用例，需重新运行格式化、clippy 和完整测试后再提交。
- 2026-07-10：修正后重新运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`，全部通过。下一步重新 staging 并提交。
