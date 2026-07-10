# 执行计划

本文件记录本次调用的执行计划和关键进度。

## 计划

1. 读取 `TODO.md`，按标题是否带 `[DONE]` 确认首个未完成任务。
2. 仅检查最近提交中是否有与该任务直接相关的未完成事项。
3. 阅读任务约束、依赖和验证要求。
4. 完整实现当前任务；若遇到具体阻塞，则在 `TODO.md` 中加入最小前置任务并停止。
5. 按要求依次运行格式化、lint 和完整测试。
6. 完成后将 `TODO.md` 中当前任务标题标记为 `[DONE]`，并填写完成记录。
7. 检查 `git status`、`git diff` 和最近提交，确认只提交本任务相关变更。
8. 使用清晰的任务提交信息提交，然后停止，不推进下一项任务。

## 进度

- 已读取 `TODO.md`，首个未完成任务为 `M4.2 Skill 搜索路径`：扫描 `.atto/skills` 和 `~/.config/atto-agent/skills`，处理重复 name 和无效文件。
- 已检查最近提交 `62f4943 [M4.1] Implement skill file parsing`，未发现与 M4.2 直接相关的未完成阻塞。
- 已实现 `SkillRegistry` discovery：默认按 workspace `.atto/skills` 优先、用户级 `~/.config/atto-agent/skills` 其次扫描 `SKILL.md`；缺失目录忽略，无效目录/文件和遍历错误记录为非致命 issue；重复 name 保留先发现项并记录冲突。
- 已将 `home_dir` 写入 `AgentConfig`，确保真实进程配置能扫描用户级 skill，同时 `AgentConfig::defaults` 和 snapshot fixture 不读取用户 HOME。
- 已将发现到的 registry 接入 agent runtime，并更新 `/skills` 输出发现数量和 discovery issue；`/skill` 激活未实现，保留给 M4.3。
- 已补充单测覆盖缺失目录、workspace/user 搜索、重复 name 和无效文件；更新 PTY slash 测试默认输出。
- 验证已通过：`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --all-targets`、`cargo fmt --all -- --check`。
- 已更新 `TODO.md`，将 `M4.2 Skill 搜索路径` 标记为 `[DONE]` 并写入完成记录和验证记录。
- 下一步：最终检查状态和 diff，然后提交 M4.2 变更。
