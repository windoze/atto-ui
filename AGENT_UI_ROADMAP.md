# Atto UI 演进计划：通用 Agent UI 基础库

> 目标：把 atto-ui 从「多窗口 TUI 框架」推进为可承载 Claude Code 式 agent 的**通用 Agent UI 基础库**。
> 本计划聚焦三块：**(A) 测试覆盖扩展**、**(B) Python binding 完整性**、**(C) Agent UI 能力缺口**。
> 仅针对功能本身，不涉及表现形式。

## 范围与边界（重要）

- **延后项（本计划暂不展开）**：editor 控件目前功能不完整，与之相关的能力一律延后：
  - 代码编辑 / 代码展示（语法高亮的可编辑视图）
  - **Diff / patch UI**：先由 `editor-core`（headless，不含显示）补齐 diff 基础支持，再由本库添加 UI 包装层。本计划只占位，不实现。
- 以上延后项在 C 部分仅作记录与依赖说明，不计入近期里程碑。

## 原则

- 核心运行时保持轻量、无强制 tokio 依赖（async 用 feature gate）。
- 所有新功能必须**可测**：能力点落地的同时补对应测试（见 A 部分）。
- UI 行为类测试统一走 PTY 包装库 `atto-ui-test-host`，保证确定性。
- 保持 `#![forbid(unsafe_code)]`。

---

## A. 测试覆盖扩展

### A.0 现状（基线）

| 模块 | 测试数 | 评估 |
|---|---|---|
| 根 crate (atto-ui) | ~189 | 布局/滚动/窗口/foreach 覆盖较好 |
| atto-ui-terminal | 3 | **严重不足**：鼠标编码/DSR/resize/粘贴几乎未测 |
| atto-ui-chat | 3 | 不足：流式/输入模式/自动滚动未充分覆盖 |
| atto-ui-markdown | 4 | 不足：表格/代码块/嵌套未系统覆盖 |
| atto-ui-runtime | 4 | tree-ops 边界（move 丢节点等）未覆盖 |
| atto-ui-python | 4 | 仅解析层单测，无端到端 |
| atto-ui-macros | 0 | **完全没有测试** |
| atto-ui-file-tree | 9 | 尚可 |

`atto-ui-test-host` 现有 PTY 能力：`send/send_str/send_ctrl/send_paste`、`click/wheel_*/drag_left`、`screen_contents/cell_contents/cell_fgcolor/cell_bgcolor`、`wait_for_text/wait_for_exit`。

### A.1 测试基础设施增强

- [ ] **test-host 补全输入能力**：组合键（带 modifier 的 click/key）、`mouse_move`（无按键移动）、`scroll_left/right` 已有需校验、右键/中键、`resize(cols, rows)` 运行时改尺寸。
- [ ] **test-host 断言增强**：整屏快照比对（行 trim + 归一）、矩形区域快照、光标位置查询、`wait_for_screen(predicate)`。
- [ ] **macros 单测**：为 `#[reactive]` / `view_builder!` / `component_properties` 增加 `trybuild` 风格的展开/编译失败用例（目前为 0）。
- [ ] **Python 端到端测试 host**：见 B.4（用 AppHost step + 事件注入 + inspect 快照做断言）。

### A.2 按模块的测试缺口（优先级 P0–P2）

**P0 — 已知 bug / 零覆盖区**
- [ ] terminal：鼠标编码全矩阵（Down/Up/Drag/Move/Scroll × SGR/X10 × modifier × 协议模式 None/PressRelease/ButtonMotion/AnyMotion）。
- [ ] terminal：DSR 应答（CPR 6n / 状态 5n，含分包到达）、bracketed paste、resize 传递到 PTY、application cursor 模式下方向键编码。
- [ ] runtime：tree-ops 边界——`move_node` 目标父不存在时**不得丢节点**（当前为 bug，见 CODE_REVIEW.md S2）、insert/remove/replace 越界、多动态根报错。
- [ ] app/status：CJK/emoji 列宽与截断（当前 `String::len` + `truncate` 有 panic 风险，见 CODE_REVIEW.md S1）。
- [ ] composable：超视口高度的单个子项滚动（当前 `bounds_fully_visible` 整块丢弃，见 CODE_REVIEW.md S3）。
- [ ] textbox：选区锚点 grapheme 对齐（Shift+点击宽字符，见 CODE_REVIEW.md S4）。

**P1 — 控件与组合层功能点**
- [ ] 每个 widget 的状态矩阵：focus / disabled / 键盘激活 / 鼠标命中（Button 缺命中判断，见 CODE_REVIEW.md L2）、min_size。
- [ ] ListBox / TableView：选择、环绕、滚动可见性、大数据（>1000 行）。
- [ ] Grid / Splitter：权重分配、最小尺寸、分隔线拖动、边框挂载滚动条。
- [ ] 滚动条边框挂载（mounted scrollbar）：root 视图直接可滚动时滚动条覆盖窗口边框的快照断言。
- [ ] chat：流式追加（A 部分 streaming API 落地后）、自动跟随到底部 + 上滚暂停、input 三模式（text/choice/confirm）提交与回调。
- [ ] markdown：标题/列表/引用/代码块/表格/嵌套，代码块与表格的内嵌滚动条交互。

**P2 — 主题 / 反应式 / 其它**
- [ ] theme：JSON/YAML 加载错误处理、命名令牌回退、运行时切换。
- [ ] reactive：Property/Binding 通知、DirtyFlag 传播、TimerWheel 周期触发。
- [ ] 窗口：模态焦点陷阱、Z 序、最小化/最大化/还原、tooltip/floating。

### A.3 验收标准

- [ ] 每个公开控件至少 1 个 PTY 行为测试 + 1 个属性/事件单测。
- [ ] CI 跑全 workspace 测试 + clippy（当前 3 条告警清零）。
- [ ] 覆盖率目标：核心 crate 行覆盖 ≥ 70%（用 `cargo llvm-cov` 度量）。

---

## B. Python Binding 完整性

### B.0 现状

`AppHost`（unsendable，轮询模型，安全）暴露：`add_dynamic_window` / `apply_tree_ops` / `step` / `run` / `drain_callbacks` / `get_property` / `schemas`。
`__init__.py` 仅为 6 个组件提供构造助手：Button / Text / Label / TextBox / VStack / HStack（其余靠裸 `Component()` 拼 spec）。

### B.1 缺失功能（核心）

- [ ] **事件注入**：`AppHost.send_event(window_id, event)` —— 从 Python 注入键盘/鼠标/粘贴事件。当前只能 `step`/`run`，无法驱动交互（也是 Python 端测试的前提）。
- [ ] **窗口管理**：`close_window` / `focus_window` / `move_window` / `resize_window` / `list_windows` / `set_title`。当前只能 `add_dynamic_window`，加了就无法再管理。
- [ ] **属性写入对称性**：已有 `get_property`，需确认 `set_property` 路径（目前靠 tree-ops 的 SetProp）是否暴露便捷方法 `set_property(id, name, value)`。
- [ ] **inspect / 快照**：暴露 `DesktopInspector`（Rust 侧已有 `inspect.rs`）为 `AppHost.snapshot()`，返回组件树 + bounds + 文本，供 Python 端断言。
- [ ] **回调载荷**：`drain_callbacks` 的事件需带完整 payload / target_id / event 元数据（校验 `callback_invocation_to_py` 是否齐全）。

### B.2 组件覆盖

- [ ] 为所有内置组件补 Python 构造助手：Checkbox / RadioGroup / Slider / Spinner / ProgressBar / ListBox / TableView / Grid / Border / Divider / Spacer / Splitter / TabView / StyledLabel。
- [ ] 暴露上层 crate 组件注册入口（`register_all_runtime_components`）：Terminal / FileTree / Chat / Markdown，使 Python 可直接使用。
- [ ] 主题控制：`AppHost.set_theme(name)` / 加载主题文件。

### B.3 打包与开发体验

- [ ] **类型存根 `.pyi`**：当前 `__init__.py` 手写、无类型提示。生成或手写 `atto_ui/__init__.pyi` + `_native.pyi`，提供 IDE 补全。
- [ ] `schemas()` 驱动的动态校验：Python 侧在 `set_prop` 时按 schema 校验属性名/类型，提前报错。
- [ ] maturin 打包验证 + `examples/minimal_app.py` 扩充为覆盖各组件的示例集。

### B.4 Python 测试

- [ ] 端到端测试 host：用 `AppHost.step` + `send_event` + `snapshot()` 写断言式测试（不依赖真实 PTY）。
- [ ] 覆盖：构树、tree-ops（增删改移）、回调往返、属性读写、窗口管理。

### B.5 验收标准

- [ ] Python 能在不写裸 dict 的情况下构建/管理一个含交互（按钮/输入/列表）的多窗口应用。
- [ ] 有 `.pyi`，IDE 补全可用。
- [ ] Python 端 e2e 测试 ≥ 15 个，CI 内运行。

---

## C. Agent UI 能力缺口

> 已具备的底座（无需新建）：dynamic 运行时 + Python 驱动、chat store、虚拟化自动跟随列表、input 三模式（text/choice/confirm = 交互提问/权限确认）、markdown（标题/代码/表格/引用/列表）、PTY 终端、文件树、TimerWheel、`run_crossterm_desktop_with_actions` 后台动作通道。

### C.0 延后项（依赖 editor 完善，本计划不实现）

- [ ] 代码编辑 / 代码展示视图（待 editor 控件功能完整）。
- [ ] **Diff / patch UI**：依赖 `editor-core`（headless）**先补齐 diff 基础支持**（差异计算、hunk 模型，不含显示），本库随后添加 UI 包装层。当前仅登记依赖关系，不动工。

### C.1 第一梯队：Agent 运行机制核心（P0）

- [ ] **可取消的结构化后台任务 / 中断**
  - 任务句柄 + 协作式 cancellation token；任务注册表；「当前是否有任务运行」状态。
  - 与事件循环集成：Esc 中断正在运行的任务（LLM 流 / 工具）。
  - async 用 feature gate（ASYNC.md Option B：可选 tokio + EventStream），默认走 std 线程 + 动作通道（Option A）。
  - 验收：能 spawn → 显示 spinner → Esc 取消 → UI 立即回到可交互；PTY 测试覆盖中断路径。
- [ ] **增量流式文本（append + 容错局部 markdown）**
  - `ChatMessageStore.append_delta(id, &str)`（替代整串 `update_text` 重设）。
  - 流式 markdown 渲染：容忍未闭合代码围栏 / 半截表格，增量解析避免每 token 全量重排。
  - 验收：长回复（>5k token 模拟）流式追加无 O(n²) 重排；PTY 快照验证途中不完整语法的稳定渲染。

### C.2 第二梯队：内容呈现（P1）

- [ ] **工具调用块抽象**：通用可折叠 disclosure / accordion 组件 + running/done/error 状态 + 「把流式输出持续灌入某个块」的模型。
- [ ] **系统剪贴板 + 跨块文本选区复制**
  - 接系统剪贴板（OSC52 或系统 API），替换/补充现有应用内 `Binding<String>`。
  - 渲染后的会话 / markdown 文本支持框选复制（editor 选区能力可借鉴）。

### C.3 第三梯队：输入与交互（P1）

- [ ] **多行输入 + 历史 + Enter/Shift+Enter 语义**
  - 真正的多行输入编辑、输入历史上下翻、kill-ring。
  - host 层 push **键盘增强标志**（KeyboardEnhancementFlags），以区分 Enter（提交）/ Shift+Enter（换行）。当前全工作区未启用。
- [ ] **通用自动补全 / 命令面板 / @-mention / 模糊匹配**
  - 可挂在输入框上的补全弹层（slash 命令、`@文件` 引用）+ 复用的模糊匹配器 + 命令面板。
  - 注：editor 内部已有 LSP 补全弹窗，但不可复用——需抽出通用 typeahead。

### C.4 第四梯队：补充能力（P2）

- [ ] **通知 / 瞬时提示队列**：StatusBar 之外的 transient toast / 后台完成提醒队列。
- [ ] **单块超大输出的窗口化渲染**：单个超大文本块（万行级工具输出）的块内 windowing 或软截断 + 「展开全部」。
- [ ] **多模态**：图片协议（sixel / kitty / iterm）+ OSC 8 可点击超链接。

### C.5 验收标准

- [ ] 能用本库 + Python binding 搭出一个最小 Claude Code 式 agent 壳：流式输出 + 可中断 + 工具调用块 + 权限确认 + 复制输出。
- [ ] C.1 两项各有 PTY 测试与压力测试。

---

## 建议里程碑排期

- **M1（基础稳固）**：A.1 测试基础设施 + A.2 P0（修已知 bug 并补测）+ B.1 事件注入/窗口管理 + B.4 Python e2e host。
- **M2（Agent 核心）**：C.1 可取消任务 + C.1 流式增量；同步补 A 的 chat/terminal 测试。
- **M3（内容与输入）**：C.2 工具块 + 剪贴板/选区；C.3 多行输入 + 键盘增强 + 自动补全。
- **M4（完善）**：B 剩余（组件覆盖 / .pyi / 主题）+ C.4 通知/超大块/多模态。
- **M5（依赖就绪后）**：editor 完整化 → editor-core diff 基础 → 本库 diff UI 包装（C.0 解锁）。

## 跟踪

- 每个 [ ] 项落地需：实现 + 测试 + 文档；完成后勾选并在对应 PR 注明。
- 与 CODE_REVIEW.md 的 P0 缺陷联动：A.2 P0 即为其测试化收口。
