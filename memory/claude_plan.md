# 执行计划

本文件记录本次调用的可执行计划与进度。内容只包含可检查的计划、决策和结果摘要，不记录私有推理过程。

## 本次计划

1. 读取 `TODO.md`，找出标题未以 `[DONE]` 开头的第一个任务，并以该任务为本轮唯一执行范围。
2. 检查最新提交标题和说明，只处理与当前任务直接相关的未完成事项。
3. 阅读当前任务正文、依赖、验证要求和完成记录要求，判断是否可直接实现，或是否仍被前置确认/缺失能力阻塞。
4. 如任务可执行，实施最小正确变更，按要求先运行 `cargo fmt`，再运行 `cargo clippy --workspace --all-targets -- -D warnings`，最后运行必要测试。
5. 如任务不可执行，保持该任务未完成，并在 `TODO.md` 与本文件记录具体阻塞原因；只在确有阶段级变化时更新 `PLAN.md`。
6. 完成或记录阻塞后，检查工作区，提交本轮所有相关变更，然后停止，不进入下一个任务。

## 本次进度

- 已读取 `TODO.md`；第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 已检查最新提交：`1a2cdc1 [T13A] Record confirmation still unresolved`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 在收到确认前，不执行 T13，也不把 T13A 标记为 `[DONE]`。
- 已在 `TODO.md` 追加本轮复核 16 记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 本轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上，且仅修改 Markdown 任务/计划记录。
- 已恢复并保留本文件既有历史记录，仅更新本轮计划与进度。

## 历史记录

- 上一轮已读取 `TODO.md`；第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 上一轮已检查最新提交：`a6fb7a2 [T13A] Record confirmation still blocked`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 上一轮当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 上一轮在收到确认前，不执行 T13，也不把 T13A 标记为 `[DONE]`。
- 上一轮已在 `TODO.md` 追加本轮复核 15 记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 上一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 上一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上，且仅修改 Markdown 任务/计划记录。
- 上一轮已恢复并保留本文件既有历史记录，仅更新本轮计划与进度。
- 上一轮已读取 `TODO.md`；第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 上一轮已检查最新提交：`97cefd3 [T13A] Record confirmation remains blocked`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 上一轮当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 上一轮在收到确认前，不执行 T13，也不把 T13A 标记为 `[DONE]`。
- 上一轮已在 `TODO.md` 追加本轮复核 14 记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 上一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 上一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上，且仅修改 Markdown 任务/计划记录。
- 最近一轮已读取 `TODO.md`；第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 最近一轮已检查最新提交：`0abd510 [T13A] Record confirmation remains pending`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 最近一轮当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 最近一轮在收到确认前，不执行 T13，也不把 T13A 标记为 `[DONE]`。
- 最近一轮已在 `TODO.md` 追加本轮复核 13 记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 最近一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 最近一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上，且仅修改 Markdown 任务/计划记录。
- 更早一轮已读取 `TODO.md`；第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 更早一轮已检查最新提交：`14c5074 [T13A] Record confirmation still pending`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 更早一轮当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 更早一轮在收到确认前，不执行 T13，也不把 T13A 标记为 `[DONE]`。
- 更早一轮已在 `TODO.md` 追加本轮复核 12 记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 更早一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 更早一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上，且仅修改 Markdown 任务/计划记录。
- 更早一轮已恢复并保留本文件既有历史记录，仅更新本轮计划与进度。
- 上一轮已读取 `TODO.md`；第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 上一轮已检查最新提交：`a14f8e4 [T13A] Record confirmation still required`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 上一轮当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 上一轮在收到确认前，不执行 T13，也不把 T13A 标记为 `[DONE]`。
- 上一轮已在 `TODO.md` 追加本轮复核 11 记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 上一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 上一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上，且仅修改 Markdown 任务/计划记录。
- 上一轮已恢复并保留本文件既有历史记录，仅更新本轮计划与进度。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 更早一轮已检查最新提交：`a348da4 [T13A] Record continued confirmation pending`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 更早一轮当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 更早一轮在收到确认前，不执行 T13，也不把 T13A 标记为 `[DONE]`。
- 更早一轮已在 `TODO.md` 追加本轮复核记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 更早一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 更早一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上，且仅修改 Markdown 任务/计划记录。
- 更早一轮已恢复并保留本文件既有历史记录，仅更新本轮计划与进度。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 更早一轮已检查最新提交：`dd0488c [T13A] Record confirmation still pending`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 更早一轮判断当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 更早一轮在收到确认前，未执行 T13，也未把 T13A 标记为 `[DONE]`。
- 更早一轮已在 `TODO.md` 追加本轮复核记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 更早一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 更早一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上，且仅修改 Markdown 任务/计划记录。
- 更早一轮已恢复并保留本文件既有历史记录，仅更新本轮计划与进度。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 更早一轮已检查最新提交：`951b517 [T13A] Record confirmation remains pending`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 更早一轮判断当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 更早一轮在收到确认前，未执行 T13，也未把 T13A 标记为 `[DONE]`。
- 更早一轮已在 `TODO.md` 追加本轮复核记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 更早一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 更早一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上，且仅修改 Markdown 任务/计划记录。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 更早一轮已检查最新提交：`659e2ce [T13A] Record confirmation still waiting`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 更早一轮判断当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 更早一轮在收到确认前，未执行 T13，也未把 T13A 标记为 `[DONE]`。
- 更早一轮已在 `TODO.md` 追加本轮复核记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 更早一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 更早一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上，且仅修改 Markdown 任务/计划记录。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 更早一轮已检查最新提交：`0c1e31e [T13A] Record confirmation still blocked`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 更早一轮判断当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 更早一轮在收到确认前，未执行 T13，也未把 T13A 标记为 `[DONE]`。
- 更早一轮已在 `TODO.md` 追加本轮复核记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 更早一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 更早一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上，且仅修改 Markdown 任务/计划记录。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 更早一轮已检查最新提交：`f4c3670 [T13A] Record confirmation still pending`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 更早一轮判断当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 更早一轮在收到确认前，未执行 T13，也未把 T13A 标记为 `[DONE]`。
- 更早一轮已在 `TODO.md` 追加本轮复核记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 更早一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 更早一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上，且仅修改 Markdown 任务/计划记录。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 更早一轮已检查最新提交：`9a87d6d [T13A] Record continued confirmation wait`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 更早一轮判断当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 更早一轮在收到确认前，未执行 T13，也未把 T13A 标记为 `[DONE]`。
- 更早一轮已在 `TODO.md` 追加本轮复核记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 更早一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 更早一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上，且仅修改 Markdown 任务/计划记录。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 更早一轮已检查最新提交：`6f7a49c [T13A] Record continued confirmation blocker`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 更早一轮判断当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 更早一轮在收到确认前，未执行 T13，也未把 T13A 标记为 `[DONE]`。
- 更早一轮已在 `TODO.md` 追加本轮复核记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 更早一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 更早一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 更早一轮已检查最新提交：`783313b [T13A] Record maintainer confirmation blocker`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 更早一轮判断当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 更早一轮在收到确认前，未执行 T13，也未把 T13A 标记为 `[DONE]`。
- 更早一轮已在 `TODO.md` 追加本轮复核记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 更早一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 更早一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务仍是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 更早一轮已检查最新提交：`21ce775 [T13A] Record continued maintainer confirmation wait`，该提交与当前任务直接相关，并确认 T13A 仍等待维护者决策。
- 更早一轮判断当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 更早一轮在收到确认前，未执行 T13，也未把 T13A 标记为 `[DONE]`。
- 更早一轮已在 `TODO.md` 追加本轮复核记录，明确 T13A 继续等待维护者确认且 T13 不执行。
- 更早一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 更早一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 更早一轮已检查最新提交：`4f5f400 [T13A] Record waiting for maintainer confirmation`，该提交与当前任务直接相关，并确认 T13A 当前等待维护者决策。
- 更早一轮判断当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 更早一轮在收到确认前，未执行 T13，也未把 T13A 标记为 `[DONE]`。
- 更早一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 更早一轮没有代码实现，也没有运行 Rust 验证套件，因为任务仍阻塞在确认输入上。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 更早一轮已检查最新提交：`9ff2254 [T13A] Record pending naming confirmation`，该提交本身记录了 T13A 等待维护者确认的状态，不包含可由代码修复直接完成的未完成实现项。
- 更早一轮判断当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 更早一轮在收到确认前，未执行 T13，也未把 T13A 标记为 `[DONE]`。
- 更早一轮已在 `TODO.md` 为本轮追加当前等待确认记录，任务保持未完成。
- 更早一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，本轮不修改、不纳入提交。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务是 `T13A — 确认命名消歧义方案（T13 前置）`。
- 更早一轮已检查最新提交：`0c3d861 [T13A] Add naming confirmation prerequisite`，该提交本身即为 T13A 前置任务的引入，不包含可由代码修复直接完成的未完成实现项。
- 更早一轮判断当前阻塞在维护者决策：必须确认是否将 `atto-editor` 改名为 `atto-editor-app`、若改名是否同步更新 workspace 依赖/import/文档/CI/发布引用、以及 `atto-ui-runtime` 是保持独立核心共享 crate 还是合并进 `atto-ui`。
- 更早一轮在收到确认前，未执行 T13，也未把 T13A 标记为 `[DONE]`。
- 更早一轮已发现两个既有未跟踪脚本 `notification.sh`、`run_agent.sh`，未修改、未纳入提交。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务是 `T13 — 命名消歧义（命名建议，需单独评估）`。
- 更早一轮最新提交是 `[R12] Review P3 cleanup`，未声明与 T13 直接相关的未完成事项。
- 更早一轮判断 T13 正文明确要求执行前与维护者确认，不能直接执行 workspace 改名或 `atto-ui-runtime` 定位调整。
- 更早一轮已在 `TODO.md` 中加入最小前置任务 `T13A — 确认命名消歧义方案（T13 前置）`，让 T13 显式依赖 T13A，并同步更新执行顺序建议。
- 更早一轮仅修改 Markdown 任务/计划文件，不改 Rust 编译产物；提交后停止。
- 更早一轮已读取 `TODO.md`；当时第一个未完成任务是 `R12 — 审阅 T12`。
- 更早一轮范围限定为审阅 T12 的 P3 清理项、运行必需验证、更新 R12 完成记录、提交并在 T13 前停止。
- 更早一轮最新提交是 `[T12] Complete P3 cleanup`；提交标题未声明未完成事项。两个既有未跟踪脚本不属于该任务，保持不动。
- 更早一轮审阅发现并修复两个必须在 R12 完成前修复的 T12 直接问题：批量增量 tree ops 会用最终 root 重建节点后再重复应用后续结构 op；`NamedStyleCache` 用 `Theme` 地址判断失效，可能在同一字段替换主题时沿用旧样式。
- 更早一轮已补充回归测试：增量局部替换使用当前 op 后的 root、主题存储替换后命名样式缓存刷新、终端鼠标坐标显式空间、非 ASCII hex 返回错误、`view_builder!` 显式 crate path 真正生效。
- 更早一轮验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --all --all-targets`。
- 更早一轮已更新 `TODO.md`，将 `R12` 标记为 `[DONE]` 并写入完成记录。
