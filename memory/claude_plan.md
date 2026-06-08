# 执行计划

说明：本文件记录可审查的执行计划与进度，不包含隐藏推理过程。

1. 以 `TODO.md` 为索引确认首个标题未带 `[DONE]` 的任务，并读取 `TODO-2.md` 中对应任务正文。
2. 检查最近提交是否提示与当前审阅任务直接相关的未完成问题；若存在，纳入审阅或作为前置任务记录。
3. 审阅 T11 改动涉及的状态栏、Desktop 事件路由、主题注册和 `atto-editor-app` 状态栏接入代码。
4. 针对 R11 明确检查：grapheme/列宽截断、click hit-test 与绘制坐标一致性、`Desktop::layout` 是否未被分段实现改变、Explorer focused 时是否仍显示 last focused editor 状态。
5. 若发现缺陷，做最小正确修复并补充覆盖测试；若发现阻塞性前置问题，更新 `TODO.md` 并提交后停止。
6. 按要求运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`，然后运行完整测试 `cargo test --all --all-targets`。
7. 将 `TODO-2.md` 中 `R11` 标题标记为 `[DONE]`，填写完成记录，并同步更新 `TODO.md` 索引状态。
8. 更新本文件进展记录，检查 git 状态、diff 和最近提交，提交本轮任务涉及的全部变更。
9. 完成 `R11` 后停止，不继续 `T12`。

## 当前任务

- 首个未完成任务：`R11 — 审阅 T11`。
- 任务来源：`TODO-2.md` 阶段二，审阅 T11 分段式 StatusBar 与 editor diagnostics 接入。
- 当前目标：完成审阅、必要修复、验证、任务记录和提交。

## 进展记录

- 已读取 `TODO.md` 和 `TODO-2.md`，确认 `R11` 是首个未完成任务。
- 已检查最近提交，当前 HEAD 为 `[T11] Implement segmented status bar`，未提示额外未完成事项；未跟踪的 `notification.sh`、`run_agent.sh` 与本任务无关，将保持不触碰。
- 审阅发现 `StatusBar::handle_mouse` fallback 布局固定使用 1 列 separator，可能与主题自定义的多列 `status-separator` 绘制坐标不一致；该问题直接影响 R11 click hit-test 要求，需修复并补测试。
- 已修复：`StatusBar` 缓存最近一次绘制使用的 separator 显示宽度，fallback hit-test 使用该宽度；新增回归测试覆盖 `set_segments` 清空 hit boxes 后的多列 separator 点击坐标。
- 已验证通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`。
- 已在 `TODO-2.md` 将 `R11` 标记为 `[DONE]` 并填写完成记录；已同步 `TODO.md` 索引状态。
