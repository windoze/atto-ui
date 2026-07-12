# 执行计划

## 当前约束
- 以 `TODO.md` 为唯一任务顺序与完成状态来源。
- 只完成第一个标题未带 `[DONE]` 的任务，完成后提交并停止。
- 在执行过程中如计划变化、关键步骤完成或遇到阻塞，及时更新本文件。
- 本文件记录可审计的执行计划与进度摘要，不包含隐藏思维链。

## 步骤
1. 读取 `TODO.md`，识别第一个未完成任务及其验证要求。
2. 检查最新提交信息，确认是否有与该任务直接相关的未完成事项。
3. 按任务要求阅读相关源码与测试，避免进行无关历史问题扫查。
4. 如任务可直接完成，实施最小正确修改；如存在具体阻塞，按要求更新 `TODO.md` 并停止。
5. 运行格式化、clippy 与相关测试；需要完整验证时运行完整测试套件并设置合理超时。
6. 更新 `TODO.md`：给完成任务标题加 `[DONE]`，填写完成记录与验证结果。
7. 检查 git 状态与差异，提交本次任务涉及的所有变更。
8. 停止，不继续下一个任务。

## 进度
- 已读取 `TODO.md`，首个未完成任务为 `M6.1 分屏`：在单个终端窗口内实现 tmux 式 split panes，并与既有 WM 浮动窗口并存。
- 最新提交 `5646558 [M5.R] Update execution log` 未指向 M6.1 的直接未完成缺陷。
- 已读取 `PLAN.md`、终端组件、终端 viewer、PTY fixture、窗口管理事件分发与既有 PTY 测试。
- 实现方案：新增终端 pane 容器组件，在单个 `Window` view 内维护 pane 树、布局和焦点；容器拦截默认 `Ctrl+B` pane 命令（`%` 左右分屏、`"` 上下分屏、`o`/Tab 切换），未命中的前缀组合回放给当前 `TerminalEmulator`，保留已有终端前缀命令。
- 下一步编码：新增 `pane` 模块和导出类型，替换 demo/PTY fixture 的单终端 view，补充 M6.1 PTY 覆盖与完成记录。
- 已完成初版编码与 PTY 测试补充；首次 clippy 暴露两个风格级 lint，正在修正后重跑验证。
- 复跑窗口 PTY 测试时发现默认并发会触发 `openpty` 资源竞争；已在该 PTY 测试文件内加入串行锁，并禁用 fixture 的真实系统剪贴板后端以避免宿主剪贴板日志干扰断言。
- M6.1 实现已完成：新增 `TerminalPaneGroup`、接入 demo/PTY fixture、补充 pane split PTY 覆盖，并在 `TODO.md` 标记 `[DONE]`。
- 最终验证通过：`cargo fmt --all`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-terminal --test pty_terminal_window_interactions -- --nocapture`、`cargo test --workspace --all-targets`。
