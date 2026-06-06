# Atto UI Agent UI 演进任务列表

> 来源：`PLAN.md`（基于 `AGENT_UI_ROADMAP.md`）
> 说明：每个「实现任务」(T) 后紧跟一个「审阅任务」(R)，R 用于审阅前一个 T 的质量与正确性。
> 通用要求（每个 T 完成前必须满足）：`cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test` 全绿。
> 架构铁律：核心 crate `atto-ui` 永不依赖 tokio；async-await 仅在新 crate `atto-ui-async`；仅会话语义功能进 `atto-ui-chat`；通用 UI 组件下沉 core。
> 行号以审查时快照为准，执行前如有偏移以函数名/符号为准。
>
> **前置说明**：CODE_REVIEW 的 P0 缺陷（S1/S2/S3/S4）已完成并归档于 `docs/archive/2026-06-06-code-review`，本列表不再重复。

---

## 阶段一：M1 基础稳固（测试基础设施 + AppHost 能力 + Python 雏形）

### [DONE] T1 — test-host 输入与断言能力补全（A.1）
**文件**：`crates/atto-ui-test-host/src/`
**现状**：现有 `send/send_str/send_ctrl/send_paste`、`click/wheel_*/drag_left`、`shift_click`、`screen_contents/cell_*`、`wait_for_text/wait_for_exit`。缺：带 modifier 的 click/key、无按键 `mouse_move`、右键/中键、运行时 `resize`、整屏/矩形快照、光标位置、`wait_for_screen(predicate)`。
**步骤**：
1. 输入补全：`click_with_mods(col,row,mods)`、`key_with_mods(key,mods)`、`right_click`/`middle_click`、`mouse_move(col,row)`（无按键移动事件）、`resize(cols,rows)`（向 PTY 下发 resize 并驱动 vt100 重设尺寸）。
2. 校验已有 `scroll_left/right` 是否正确发送 SGR 滚动编码；补齐缺失方向。
3. 断言增强：`screen_snapshot()`（按行 trim + 末尾空行归一返回 `Vec<String>`）、`region_snapshot(rect)`、`cursor_position()`（从 vt100 取）、`wait_for_screen(pred, timeout)`（轮询 predicate）。
**测试**：在 `crates/atto-ui-test-host` 内或现有 PTY 测试中，对每个新增 API 写自测（如 `resize` 后断言屏幕宽度变化、`mouse_move` 触发 hover 行为的 app 分支）。
**验收**：新 API 有最少 1 处调用覆盖；现有 PTY 测试不回归。
**完成记录（2026-06-06）**：
- 实现 `click_with_mods`、`key_with_mods`、`right_click`、`middle_click`、`mouse_move`、`resize`、`scroll_left/right`、`screen_snapshot`、`region_snapshot`、`cursor_position`、`wait_for_screen`。
- `resize(cols, rows)` 同步调用 PTY master resize，并更新 vt100 parser 尺寸；新增 `ScreenRegion` 与 `KeyCode`/`KeyModifiers` re-export 方便测试调用。
- 新增 `snapshot_app --input-api` 事件回显 fixture，并新增 `tests/pty_test_host_api.rs` 覆盖新增输入、断言、光标、水平滚动与 resize API。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --test pty_test_host_api`；`cargo test`。

### [DONE] R1 — 审阅 T1
审阅 T1 改动：
- 确认新增输入 API 的事件编码与 crossterm `MouseEvent`/`KeyEvent` 一致（modifier 位、SGR 序列）。
- 确认 `resize` 真实改变 vt100 解析尺寸且后续断言可见。
- 确认快照归一逻辑稳定（trim/空行）、`wait_for_screen` 无忙等死锁。
- 运行全 workspace `cargo test`。
**完成记录（2026-06-06）**：
- 已审阅 `crates/atto-ui-test-host/src/lib.rs`、`src/bin/snapshot_app.rs`、`tests/pty_test_host_api.rs` 的 T1 改动。
- 对照 crossterm 0.29 解析逻辑确认 SGR 鼠标按钮/modifier、水平滚动、无按键移动以及修饰键参数编码与 `MouseEvent`/`KeyEvent` 一致。
- 确认 `resize` 同步 PTY master 与 vt100 parser 尺寸，且 `--input-api` fixture 的 `size: 100x30` 断言覆盖真实 resize 可见性。
- 确认 `screen_snapshot`/`region_snapshot` 对行 trim 与尾部空行归一稳定，`wait_for_screen` 使用 10ms 轮询避免忙等。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --workspace --all-targets`。

### [DONE] T2 — macros trybuild 测试（A.1）
**文件**：`crates/atto-ui-macros/`、新增 `crates/atto-ui-macros/tests/`
**现状**：`#[reactive]` / `view_builder!` / `component_properties` 宏测试数为 0。
**步骤**：
1. 加 `trybuild` dev-dependency。
2. `tests/expand/` 放成功展开用例（`reactive` 生成属性、`view_builder!` 构树、`component_properties` 反射）。
3. `tests/ui/` 放编译失败用例（非法属性、未知类型 → 断言 `compile_error!` 友好提示，关联 L6）。
**测试**：`cargo test -p atto-ui-macros`。
**验收**：成功与失败用例各 ≥2；CI 内运行。
**完成记录（2026-06-06）**：
- 为 `atto-ui-macros` 添加 `trybuild` dev-dependency 和 `tests/trybuild.rs` harness。
- 新增 3 个成功展开 fixture：`Reactive` 访问器/dirty/binding、`view_builder!` 构树与 modifier、`ComponentProperties` + `component_properties` 属性反射和 schema。
- 新增 2 个失败 fixture 及 `.stderr`：`#[reactive]` 标在非 `Property<T>` 字段时的友好错误、`view_builder!` 未知组件时的友好错误。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-macros`；`cargo test --workspace --all-targets`。

### [ ] R2 — 审阅 T2
- 确认 trybuild 用例真实覆盖三个宏的核心展开路径与至少一类编译失败。
- 确认失败用例的错误信息对用户友好（非裸 panic）。
- 运行 `cargo test -p atto-ui-macros`。

### [ ] T3 — AppHost 事件注入与窗口管理（B.1）
**文件**：`src/app/run.rs`（`AppHost`）、必要时 `src/wm/manager/`
**现状**：`AppHost` 仅有 `add_dynamic_window`/`apply_tree_ops`/`step`/`run`/`drain_callbacks`/`get_property`/`schemas`，加了窗口无法再管理，也无法注入事件。
**步骤**：
1. `send_event(window_id, event)`：把键盘/鼠标/粘贴事件路由到指定窗口的事件分发入口（复用 desktop/wm 现有分发）。
2. 窗口管理：`close_window` / `focus_window` / `move_window(id,x,y)` / `resize_window(id,w,h)` / `list_windows()` / `set_title(id,title)`，转发到 `WindowManager`。
3. `set_property(id,name,value)` 便捷方法：内部走 tree-ops `SetProp`，与 `get_property` 对称。
**测试**：Rust 单测 + 1 个 PTY：注入点击/按键驱动按钮回调；创建窗口后 close/focus/move/resize 并用 `list_windows` 断言状态。
**验收**：事件注入能触发回调；窗口管理方法均可用且不破坏 Z 序/焦点。

### [ ] R3 — 审阅 T3
- 确认 `send_event` 的坐标系/目标窗口路由正确（0-based、相对窗口）。
- 确认窗口管理方法与现有 wm 不变量一致（模态焦点陷阱、Z 序、最小化态）。
- 确认 `set_property` 与 `get_property` 往返一致。
- 运行相关 PTY/单测。

### [ ] T4 — DesktopInspector 快照导出（B.1）
**文件**：`src/inspect.rs`、`src/app/run.rs`
**现状**：`inspect.rs` 已有 `DesktopInspector`，但未暴露为可供外部断言的快照。
**步骤**：
1. 定义可序列化快照结构：组件树（id/tag/类型）+ bounds + 文本内容。
2. `AppHost::snapshot()` 返回该结构（serde 可序列化，供 Python 侧消费）。
3. 确保不依赖真实 PTY，纯内存可取。
**测试**：单测构建小窗口树，断言 snapshot 含预期 id/bounds/文本。
**验收**：snapshot 结构稳定且足以支撑 Python e2e 断言（T5 依赖）。

### [ ] R4 — 审阅 T4
- 确认 snapshot 覆盖断言所需字段（id、bounds、text、状态）。
- 确认序列化无环、无大对象 clone 性能问题。
- 运行单测。

### [ ] T5 — Python e2e 测试 host 雏形（B.4）
**文件**：`crates/atto-ui-python/`、新增 Python 测试目录
**现状**：Python 仅解析层单测，无端到端；依赖 T3（send_event）+ T4（snapshot）。
**步骤**：
1. binding 暴露 `send_event` / 窗口管理 / `set_property` / `snapshot()`。
2. 写 e2e：构树 → `step` → `send_event` → `snapshot()` 断言；覆盖 tree-ops 增删改移、回调往返、属性读写、窗口管理。
3. 不依赖真实 PTY。
**测试**：Python 端 ≥8 个 e2e（M1 阶段雏形，M4 扩到 ≥15）。
**验收**：能用断言式 e2e 驱动一个含按钮/输入的多窗口应用。

### [ ] R5 — 审阅 T5
- 确认 e2e 真实经过 Rust 分发路径（非 Python 侧模拟）。
- 确认回调 payload/target_id/event 元数据齐全（B.1 回调载荷）。
- 运行 Python 测试套件。

---

## 阶段二：M2 Agent 核心（任务取消 + async crate + 流式）

### [ ] T6 — 任务取消抽象（core，std-only）（C.1）
**文件**：新增 `src/task/`（或 `src/reactive/task.rs`），`src/app/run.rs` 集成
**现状**：仅有 `EventQueue::channel()` + `run_crossterm_desktop_with_actions`（ASYNC.md Option A），无取消/任务注册/运行态。
**步骤**：
1. `CancellationToken`：基于 `Arc<AtomicBool>`，`cancel()` / `is_cancelled()`；协作式，不依赖 tokio。
2. `TaskHandle`（持有 token + 元信息）、`TaskRegistry`（注册/注销/遍历）、运行态 `Property<bool>`「当前是否有任务运行」。
3. 事件循环集成：Esc 中断当前运行任务（在 `run_crossterm_desktop_with_actions` 或 AppHost step 内）。
**测试**：单测取消语义；PTY：spawn 后台线程任务 → 显示 spinner → 按 Esc → 断言任务停止、UI 立即可交互、运行态归 false。
**验收**：std 线程模型下可 spawn/取消；PTY 覆盖中断路径；core 仍无 tokio。

### [ ] R6 — 审阅 T6
- 确认取消抽象不引入 tokio、不引入隐藏全局状态。
- 确认 Esc 中断与现有事件分发优先级正确（不吞掉其他 Esc 语义）。
- 确认运行态 Property 通知正确。
- 运行单测与 PTY。

### [ ] T7 — 新建 `atto-ui-async` crate（tokio，feature-gated）（C.1 / ASYNC.md Option B）
**文件**：新增 `crates/atto-ui-async/`，更新根 `Cargo.toml` workspace members
**现状**：tokio/async-await 在全工作区命中 0 次（L8）。
**步骤**：
1. 新 crate `Cargo.toml`：`atto-ui` 依赖；`tokio` 与 `crossterm` EventStream 置于可选 feature（默认关闭）。
2. tokio 运行时 helper + `EventStream`：`select!` 风格统一 await 终端事件与应用动作通道。
3. `spawn_async()` / `spawn_blocking()`：结果经 core 动作通道回灌 UI；接入 T6 的 `CancellationToken` / `TaskRegistry`。
4. async 版运行入口（对应 `run_crossterm_desktop_with_actions`）。
5. `atto-ui-components` 增加可选 `async` feature 透传（默认关闭）。
**测试**：feature 开启下的 PTY/集成测试，确定性 dispatch；feature 关闭时断言 workspace 不引入 tokio（`cargo tree` 检查或 CI 约束）。
**验收**：不开 feature 时核心编译零 tokio；开 feature 时 async-await 后台任务能驱动 UI 并可被 Esc 取消。

### [ ] R7 — 审阅 T7
- 确认 core crate 依赖图无 tokio（`cargo tree -p atto-ui` 验证）。
- 确认 feature 关闭时新 crate 不破坏默认构建。
- 确认 async 任务与 T6 取消注册表正确联动。
- 确认 PTY 测试在 feature 下仍确定性。

### [ ] T8 — ChatMessageStore 增量流式（C.1）
**文件**：`crates/atto-ui-chat/src/store.rs`、`message.rs`
**现状**：仅 `update_text` 整串重设，长回复每 token 全量重排（O(n²) 风险）。
**步骤**：
1. `append_delta(id, &str)`：对 `ChatMessageContent::Text` 增量追加，不重置整串。
2. 与 `ChatMessageStatus::InProgress` 配合：流式期间 InProgress，结束置 Final。
3. 评估 `Property<Vec<ChatMessage>>` 的 update 粒度，避免每 delta clone 整个 Vec（必要时引入单条消息级通知）。
**测试**：单测 append_delta 累积正确；模拟 >5k token 追加，断言无 O(n²)（行为正确为主）。
**验收**：增量追加内容正确，长回复无明显重排退化。

### [ ] R8 — 审阅 T8
- 确认 append_delta 仅作用于 Text 内容、对其他 content 安全 no-op。
- 确认通知粒度未导致全列表重绘退化。
- 运行 chat 测试。

### [ ] T9 — 流式 markdown 容错增量渲染（C.1）
**文件**：`crates/atto-ui-chat/src/`、必要时 `crates/atto-ui-markdown/src/`
**现状**：markdown 渲染未针对流式中途的不完整语法（未闭合代码围栏 / 半截表格）做容错。
**步骤**：
1. markdown crate 提供容错入口：未闭合 ``` 围栏按代码块渲染到当前；半截表格降级为纯文本行。
2. chat 消息渲染走容错入口，增量解析避免每 token 全量重排。
**测试**：PTY 快照：逐步追加含未闭合围栏/半截表格的文本，断言中途渲染稳定不报错、闭合后正确成块。
**验收**：流式途中不完整语法稳定渲染，闭合后转为正确块。

### [ ] R9 — 审阅 T9
- 确认容错渲染不误吞已完成内容、闭合后无残留降级。
- 确认增量解析未引入全量重排。
- 运行 PTY 快照测试。

### [ ] T10 — chat / terminal 测试补齐（A.2 P1）
**文件**：`crates/atto-ui-chat/tests/`、`crates/atto-ui-terminal/tests/`
**现状**：chat 3 测试、terminal 3 测试，覆盖严重不足。
**步骤**：
1. chat：流式追加、自动跟随到底部 + 上滚暂停、input 三模式（text/choice/confirm）提交与回调。
2. terminal：鼠标编码矩阵（Down/Up/Drag/Move/Scroll × SGR/X10 × modifier × 协议模式）、DSR 应答（CPR/状态，含分包）、bracketed paste、resize 传递、application cursor 方向键编码。
**测试**：上述 PTY/集成测试。
**验收**：chat/terminal 关键路径有覆盖；P0 列出的 terminal 编码矩阵成体系。

### [ ] R10 — 审阅 T10
- 确认编码矩阵覆盖全面（无遗漏协议模式组合）。
- 确认 chat 自动跟随/暂停边界正确。
- 运行相关测试。

---

## 阶段三：M3 内容与输入（通用组件下沉 + 工具块 + artifact viewer）

### [ ] T11 — 可折叠 disclosure / accordion 组件（core）（C.2）
**文件**：新增 `src/widgets/disclosure.rs`
**现状**：无通用可折叠块，工具调用块缺底座。
**步骤**：
1. `Disclosure` 组件：标题行 + 可展开/折叠内容，支持 running/done/error 状态指示。
2. 提供「把流式输出持续灌入内容区」的模型（内容可绑定到外部文本/store）。
3. 键盘（Enter/Space 切换）+ 鼠标点击命中。
**测试**：PTY：展开/折叠切换、状态指示渲染、内容追加可见。
**验收**：通用 disclosure 可被 chat 复用（T13 依赖）。

### [ ] R11 — 审阅 T11
- 确认组件通用（不含会话语义）、可独立于 chat 使用。
- 确认状态指示与主题样式一致。
- 运行 PTY。

### [ ] T12 — 系统剪贴板 + 文本选区复制（core）（C.2）
**文件**：新增 `src/clipboard.rs`，composable/text 选区
**现状**：仅应用内 `Binding<String>`，无系统剪贴板；渲染文本无框选复制。
**步骤**：
1. `clipboard.rs`：OSC52 写出（std-only，无系统 API 依赖），与应用内 binding 并存。
2. composable/text 层支持框选复制（借鉴 editor 选区，但实现于 core 通用文本）。
**测试**：PTY：选区后复制触发 OSC52 序列（断言输出序列）；选区渲染高亮。
**验收**：可框选并经 OSC52 复制；不破坏现有文本渲染。

### [ ] R12 — 审阅 T12
- 确认 OSC52 编码正确、不在不支持的终端崩溃（降级）。
- 确认选区实现于 core 通用层、未引入 editor 依赖。
- 运行 PTY。

### [ ] T13 — chat 工具调用块（消费 disclosure）（C.2）
**文件**：`crates/atto-ui-chat/src/message.rs`、`list.rs`
**现状**：`ChatMessageContent` 仅 Text/File，无工具调用建模。
**步骤**：
1. `ChatMessageContent::ToolCall { name, status, output }`（status: running/done/error）。
2. 渲染用 T11 的 `Disclosure`；输出可经 T8 的增量流式持续灌入。
**测试**：PTY：工具块 running→done 状态切换、输出流式追加、折叠展开。
**验收**：工具块在会话列表中可折叠、状态正确、输出可流式。

### [ ] R13 — 审阅 T13
- 确认 chat 仅消费 core disclosure，未在 chat 重复实现折叠逻辑。
- 确认状态机 running/done/error 转换正确。
- 运行 chat PTY。

### [ ] T14 — 消息内 Artifact link + 最简文本 viewer（核心方案）（C.0 占位实现）
**文件**：`crates/atto-ui-chat/src/message.rs`、新增 viewer 模块（仅依赖 core widgets）
**现状**：editor/diff 富 UI 未就绪；方案为消息列表只放 link，code/diff 在独立窗口呈现。
**步骤**：
1. `message.rs` 扩展：`ChatMessageContent::Artifact { kind: ArtifactKind, anchor: ArtifactId, title }`，`ArtifactKind = Code | Diff | File`。消息列表只渲染一个可点击 link，不内嵌代码/diff。
2. chat 暴露 `on_open_artifact(ArtifactId)` 事件/回调，不关心由谁/用什么窗口呈现（保持 chat 与 viewer 解耦）。
3. 定义统一接口 `trait ArtifactViewer { fn open(&mut self, artifact: Artifact) -> WindowId; }`。
4. **最简实现 `TextArtifactViewer`**：Code 用只读 TextBox/文本组件；Diff 用纯文本 unified diff（`+`/`-`/空格前缀 + 简单着色），不做 hunk 折叠；在独立 `WindowType::Normal` 窗口打开。
5. 点击消息 link → `open()` 弹出独立窗口。
**测试**：PTY：点击 Code link 打开独立窗口显示源码；点击 Diff link 显示带前缀着色的 diff 文本。
**验收**：link→独立窗口呈现链路通；接口清晰，后续富 viewer 可替换 `TextArtifactViewer` 而 chat 不改动。

### [ ] R14 — 审阅 T14
- 确认 chat 不直接依赖 editor，仅通过 `ArtifactViewer` 接口与 `on_open_artifact` 解耦。
- 确认最简 viewer 接口足以被未来富实现替换（签名稳定）。
- 确认 link 点击命中与窗口打开/关闭正确。
- 运行 PTY。

### [ ] T15 — 多行输入 + 历史 + 键盘增强（core）（C.3）
**文件**：新增 `src/widgets/textarea.rs`，`src/app/run.rs`（键盘增强标志）
**现状**：仅单行 TextBox；全工作区未启用 KeyboardEnhancementFlags，无法区分 Enter/Shift+Enter。
**步骤**：
1. `TextArea`：多行编辑（复用 TextBuffer/grapheme），输入历史上下翻，kill-ring。
2. host 层 push `KeyboardEnhancementFlags`，区分 Enter（提交）/ Shift+Enter（换行）。
3. 在 chat 输入面板接入（chat 侧仅消费）。
**测试**：PTY：多行编辑、Enter 提交 vs Shift+Enter 换行、历史上下翻、kill-ring。
**验收**：多行输入可用；Enter/Shift+Enter 语义正确（需终端支持增强标志，附降级路径）。

### [ ] R15 — 审阅 T15
- 确认键盘增强标志的启用/恢复（退出时还原终端状态）。
- 确认不支持增强的终端有合理降级。
- 确认 textarea 在 core、chat 仅消费。
- 运行 PTY。

### [ ] T16 — 通用 typeahead / 命令面板 / 模糊匹配（core）（C.3）
**文件**：新增 `src/widgets/typeahead.rs`、`src/fuzzy.rs`
**现状**：editor 内有 LSP 补全弹窗但不可复用；无通用 typeahead/模糊匹配。
**步骤**：
1. `fuzzy.rs`：可复用模糊匹配器（子序列打分）。
2. `typeahead.rs`：可挂在输入框上的补全弹层（slash 命令、`@file` 引用），键盘上下选择/回车确认。
3. 命令面板组合（typeahead + 命令列表）。
**测试**：PTY：输入触发弹层、模糊过滤、选择确认、Esc 关闭。
**验收**：通用 typeahead 可挂到任意输入框；模糊匹配可复用。

### [ ] R16 — 审阅 T16
- 确认 typeahead 与输入框解耦、可复用（非 editor 专用）。
- 确认弹层焦点/命中与底层组件不冲突。
- 运行 PTY。

---

## 阶段四：M4 完善（Python 覆盖 + 通知/超大块/多模态 + P1/P2 测试 + 一致性收尾）

### [ ] T17 — Python 组件覆盖 + 上层注册 + 主题（B.2/B.3）
**文件**：`crates/atto-ui-python/`、`atto_ui/__init__.py` + `.pyi`
**步骤**：
1. 为所有内置组件补构造助手：Checkbox/RadioGroup/Slider/Spinner/ProgressBar/ListBox/TableView/Grid/Border/Divider/Spacer/Splitter/TabView/StyledLabel。
2. 暴露 `register_all_runtime_components`（Terminal/FileTree/Chat/Markdown）。
3. `set_theme(name)` / 加载主题文件。
4. 生成/手写 `atto_ui/__init__.pyi` + `_native.pyi`；schema 驱动 `set_prop` 校验；maturin 打包验证 + 扩充 `examples/minimal_app.py`。
**测试**：Python e2e 扩到 ≥15；覆盖各组件构造、上层组件、主题切换。
**验收**：Python 不写裸 dict 即可构建/管理含交互的多窗口应用；IDE 补全可用。

### [ ] R17 — 审阅 T17
- 确认构造助手覆盖全部内置组件、参数与 schema 一致。
- 确认 `.pyi` 正确、补全可用。
- 运行 Python e2e（≥15）。

### [ ] T18 — 通知队列 + 超大块 windowing + 多模态（C.4）
**文件**：`src/app/`（toast）、`src/composable/`（windowing）、`src/drawing.rs`（图片/OSC8）
**步骤**：
1. transient toast / 后台完成提醒队列（StatusBar 之外）。
2. 单块超大文本（万行级）块内 windowing 或软截断 + 「展开全部」。
3. 多模态：图片协议（sixel/kitty/iterm）+ OSC8 可点击超链接。
**测试**：PTY：toast 出现/消失；超大块仅渲染可见窗口 + 展开；OSC8 链接序列断言（图片协议按终端能力降级）。
**验收**：toast/超大块可用；多模态在支持的终端生效、否则降级。

### [ ] R18 — 审阅 T18
- 确认 toast 队列不阻塞主循环、不与状态栏冲突。
- 确认超大块 windowing 不丢内容、展开正确。
- 确认多模态降级安全。
- 运行 PTY。

### [ ] T19 — A.2 P1/P2 测试补齐 + 一致性收尾（含 L2）
**文件**：`tests/`、各 crate tests、`src/widgets/button.rs`
**步骤**：
1. widget 状态矩阵（focus/disabled/键盘激活/鼠标命中/min_size）；ListBox/TableView 选择环绕/大数据；Grid/Splitter 权重/最小尺寸/拖动/挂载滚动条；markdown 标题/列表/引用/代码/表格/嵌套 + 内嵌滚动条。
2. theme JSON/YAML 错误处理/命名令牌回退/运行时切换；reactive Property/Binding/DirtyFlag/TimerWheel；窗口模态焦点陷阱/Z序/最小化最大化还原/tooltip/floating。
3. L2：`widgets/button.rs` 保存 `last_area` + `Down(Left)` 前 contains 命中判断。
**测试**：上述 PTY/单测。
**验收**：每个公开控件 ≥1 PTY 行为测试 + ≥1 属性/事件单测；clippy 清零；`cargo llvm-cov` 核心 crate 行覆盖 ≥70%。

### [ ] R19 — 审阅 T19
- 确认覆盖率达标（`cargo llvm-cov` 报告）。
- 确认 Button 命中判断与其他 widget 一致。
- 运行全 workspace 测试 + clippy。

---

## 阶段五：M5 依赖就绪后（解锁 C.0 富 diff/code UI）

### [ ] T20 — editor 完整化 → editor-core diff → 富 ArtifactViewer
**文件**：`crates/atto-ui-editor/`、依赖 `editor-core` diff 基础
**现状**：T14 已提供最简 `TextArtifactViewer` 与稳定接口；本任务待 editor 控件完整化后实现富版本。
**步骤**：
1. editor 控件功能完整化（语法高亮可编辑视图）。
2. editor-core（headless）补齐 diff 基础（差异计算、hunk 模型，不含显示）。
3. `atto-ui-editor` 实现富 `ArtifactViewer`：语法高亮 code 视图 + hunk 折叠 diff UI，实现 T14 同一接口。
4. chat 侧不改动，仅替换注入的 viewer 实现。
**测试**：PTY：code 视图语法高亮、diff hunk 折叠/展开。
**验收**：富 viewer 替换最简实现，chat 接口零改动；C.0 解锁。

### [ ] R20 — 审阅 T20
- 确认富 viewer 实现的接口与 T14 完全一致（chat 无需改动验证）。
- 确认 diff hunk 模型来自 editor-core headless 层。
- 运行 PTY。

---

## 测试与回归约定

- 每个 T 完成前：`cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test` 全绿。
- UI 行为类测试统一走 `atto-ui-test-host` PTY 框架，保证确定性。
- 纯逻辑（取消语义、append_delta、fuzzy）用 Rust 单测，无需 PTY。
- async（T7）测试在 feature 开启下运行，并验证 feature 关闭时 core 不引入 tokio。
- 保持 `#![forbid(unsafe_code)]`。

## 执行顺序

1. M1：T1→R1→T2→R2→T3→R3→T4→R4→T5→R5
2. M2：T6→R6→T7→R7→T8→R8→T9→R9→T10→R10
3. M3：T11→R11→T12→R12→T13→R13→T14→R14→T15→R15→T16→R16
4. M4：T17→R17→T18→R18→T19→R19
5. M5（依赖就绪后）：T20→R20
</content>
