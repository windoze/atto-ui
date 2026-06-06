# AGENT_UI_ROADMAP 实现计划

> 本计划把 `AGENT_UI_ROADMAP.md` 的能力点落到**具体 crate**，确立清晰的依赖边界。
> 三条架构约束：
> 1. 通用能力放入核心 crate `atto-ui`。
> 2. async（tokio）相关放入**独立新 crate** `atto-ui-async`，核心 crate 不依赖 tokio。
> 3. 仅与 Agent message 相关的功能尽量收敛到 `atto-ui-chat`。
> 4. editor/diff 先用最简实现（独立窗口 + 只读文本框），消息列表里只放 anchor/link，接口先行、UI 后补。

---

## 0. 依赖关系总览

```
                 atto-ui (core, std-only, 无 tokio)
                 ├─ 通用 widgets / composable / 任务取消抽象 / 剪贴板 / typeahead / toast
                 ▲
      ┌──────────┼───────────────────────────┐
      │          │                            │
atto-ui-async  atto-ui-chat              atto-ui-editor
(tokio,      (Agent message:            (diff/code 富 UI，
 feature-     streaming/anchor/          后续解锁)
 gated)        tool-block)                    │
      │          │                            │
      └──────────┴───────────┬────────────────┘
                             ▼
                   atto-ui-components (feature-gated 聚合)
                             ▼
                     atto-ui-python (binding)
```

**铁律**
- `atto-ui` 永不依赖 tokio；async-await 能力只在 `atto-ui-async` 出现。
- 任务取消的**抽象**（token / handle / registry）是 std-only，留在 core；tokio 运行时与 `EventStream` 集成在 `atto-ui-async`。
- `atto-ui-chat` 只承载「与会话消息相关」的逻辑；通用 UI 组件（disclosure、typeahead、多行输入、toast）下沉到 core，chat 仅消费它们。
- editor/diff 的富 UI 在 `atto-ui-editor`；最简文本版 artifact viewer 先放 chat（仅依赖 core widgets），通过统一接口后续被 editor 实现替换。

---

## 1. 能力点 → crate 映射

### 1.1 核心 crate `atto-ui`（通用，无 tokio）

**A.2 P0 缺陷修复 —— 已完成（归档于 `docs/archive/2026-06-06-code-review`）**
- ✅ S1 `app/status.rs`：改用 `UnicodeWidthStr::width` + grapheme 边界截断（T1/R1）。
- ✅ S2 `runtime/tree.rs`：`move_node` 先校验后摘除，失败不丢节点；spec 层 Move 原子化（T2/R2）。
- ✅ S4 `widgets/textbox.rs` + `text/buffer.rs`：选区锚点 grapheme 对齐（T3/R3）。
- ✅ S3 `composable/stack`：`bounds_intersects_viewport` 相交裁剪渲染与命中（T4/R4）。
- 备注：`widgets/button.rs` 鼠标命中判断（L2）属 P3 一致性项，留待本计划 M4 收尾。

**C.1 任务取消抽象（std-only）**
- 新增 `src/task/`（或 `reactive/task.rs`）：
  - `CancellationToken`（`Arc<AtomicBool>` 协作式取消）。
  - `TaskHandle` / `TaskRegistry`（注册表 + 「当前是否有任务运行」状态 `Property<bool>`）。
  - 与现有动作通道（`EventQueue::channel` + `run_crossterm_desktop_with_actions`）集成：默认 std 线程模型（ASYNC.md Option A）。
  - 事件循环集成：Esc 中断当前运行任务。
- 验收：spawn → spinner → Esc 取消 → UI 立即可交互；PTY 覆盖中断路径。

**C.2 / C.3 / C.4 通用 UI 组件**
- `widgets/disclosure.rs`：可折叠 disclosure / accordion，带 running/done/error 状态（工具调用块的通用底座，chat 复用）。
- 系统剪贴板：`src/clipboard.rs`（OSC52 写出，std-only），与现有应用内 `Binding<String>` 并存。
- 渲染文本框选复制：composable/text 层选区能力（可借鉴 editor 选区，但实现在 core 通用文本）。
- 多行输入：`widgets/textarea.rs`（真正多行编辑 + 输入历史上下翻 + kill-ring）。
- 键盘增强标志：`app/run.rs` host 层 push `KeyboardEnhancementFlags`，区分 Enter / Shift+Enter。
- 通用 typeahead：`widgets/typeahead.rs` + 复用模糊匹配器 `src/fuzzy.rs` + 命令面板（slash 命令 / `@file`）。
- toast 通知队列：`app/` 内 transient toast / 后台完成提醒队列。
- 单块超大输出 windowing / 软截断 + 「展开全部」：composable 层。
- 多模态：图片协议（sixel/kitty/iterm）+ OSC8 超链接，drawing 层。

**B.1 / B.2 AppHost 能力（供 python 包装）**
- `app/run.rs` `AppHost` 增加：`send_event`、`close_window` / `focus_window` / `move_window` / `resize_window` / `list_windows` / `set_title`、`set_property` 便捷方法。
- 暴露 `DesktopInspector` 快照（`inspect.rs` 已有）为可序列化结构，供 `snapshot()`。

### 1.2 新 crate `atto-ui-async`（tokio，feature-gated）

> 目标：把 ASYNC.md Option B 落到独立 crate，核心 crate 零 tokio 依赖。

- `Cargo.toml`：`tokio`（可选 feature）、`crossterm`（EventStream feature）、依赖 `atto-ui`。
- 内容：
  - tokio 运行时 helper；crossterm `EventStream` 与动作通道的 `select!` 风格统一循环。
  - `spawn_async()` / `spawn_blocking()`：结果经 core 的动作通道回灌 UI；接入 core 的 `CancellationToken` / `TaskRegistry`。
  - 与 `run_crossterm_desktop_with_actions` 对应的 async 版运行入口。
- 验收：PTY 测试在 feature 开启下仍确定性；不开 feature 时 workspace 编译不引入 tokio。
- workspace 注册：加入 `Cargo.toml` members；`atto-ui-components` 增加可选 `async` feature 透传。

### 1.3 `atto-ui-chat`（Agent message 专属）

**C.1 增量流式**
- `store.rs`：`append_delta(id, &str)` 增量追加（替代整串 `update_text` 重设），避免 O(n²) 重排。
- 流式 markdown：容错渲染未闭合代码围栏 / 半截表格，增量解析（依赖 `atto-ui-markdown`，必要时在 markdown crate 增加「容错/增量」入口）。
- 验收：>5k token 模拟流式追加无 O(n²)；PTY 快照验证途中不完整语法稳定渲染。

**C.2 工具调用块（消费 core 的 disclosure）**
- chat 内建模「把流式输出持续灌入某个块」：`ChatMessageContent::ToolCall { name, status, output_stream }`，渲染用 core `disclosure`。

**消息内 anchor/link（你的方案核心）**
- `message.rs` 扩展 `ChatMessageContent`：
  - `Artifact { kind: ArtifactKind, anchor: ArtifactId, title: String }`，`ArtifactKind = Code | Diff | File`。
  - 消息列表只渲染一个可点击 link（不内嵌代码/diff）。
- chat 暴露「打开 artifact」事件 / 回调（`on_open_artifact(ArtifactId)`），不关心由谁、用什么窗口呈现 —— 保持 chat 与 viewer 解耦。

**A 部分 chat 测试**
- 流式追加、自动跟随到底部 + 上滚暂停、input 三模式提交与回调（PTY）。

### 1.4 Artifact Viewer（editor/diff 最简实现，接口先行）

> 你的方案：editor/diff 放独立窗口，消息列表只放 link。在 editor/diff 完善前用最简版本。

- 定义统一接口（放 chat 或新建轻量模块，仅依赖 core widgets）：
  ```rust
  trait ArtifactViewer {
      fn open(&mut self, artifact: Artifact) -> WindowId; // 在独立窗口呈现
  }
  ```
- **最简实现 `TextArtifactViewer`**（现在就做）：
  - Code：只读 `TextBox`/只读文本组件展示源码。
  - Diff：纯文本展示 unified diff（前缀 `+`/`-`/空格 + 简单着色），不做 hunk 折叠。
  - 在独立窗口（`WindowType::Normal`）打开，点击消息 link → `open()`。
- **后续替换**：`atto-ui-editor` 提供富实现（语法高亮可编辑视图 / hunk diff UI），实现同一接口，chat 侧无需改动。对应 roadmap C.0 / M5：待 editor 完整化 → editor-core diff 基础 → 富 viewer 插入。

### 1.5 `atto-ui-python`（binding）

- 包装 1.1 中 AppHost 新增方法：`send_event` / 窗口管理 / `set_property` / `snapshot()`。
- 补全内置组件构造助手（Checkbox/RadioGroup/Slider/Spinner/ProgressBar/ListBox/TableView/Grid/Border/Divider/Spacer/Splitter/TabView/StyledLabel）。
- 暴露上层组件注册（Terminal/FileTree/Chat/Markdown）；`set_theme`。
- `.pyi` 类型存根 + schema 驱动校验。
- Python e2e host：`step` + `send_event` + `snapshot()`，≥15 用例。

### 1.6 `atto-ui-test-host`（A.1 基础设施）

- 输入补全：带 modifier 的 click/key、`mouse_move`、右键/中键、`resize(cols,rows)`。
- 断言增强：整屏快照（trim+归一）、矩形区域快照、光标位置、`wait_for_screen(predicate)`。
- `atto-ui-macros`：`trybuild` 展开/编译失败用例。

---

## 2. 里程碑排期（对齐 roadmap M1–M5）

- **M1 基础稳固**
  - ~~A.2 P0 缺陷修复~~ —— **已完成并归档**（`docs/archive/2026-06-06-code-review`），不再计入本计划工作量。
  - A.1 test-host 增强 + macros trybuild。
  - core `AppHost`：`send_event` / 窗口管理 / `set_property` / `snapshot`。
  - python：包装上述 + e2e host 雏形。

- **M2 Agent 核心**
  - core：`CancellationToken` / `TaskRegistry` / Esc 中断（std 模型）。
  - **新建 `atto-ui-async`**：tokio 运行时 + EventStream（feature-gated）。
  - chat：`append_delta` 流式 + 容错增量 markdown。
  - 补 chat/terminal 测试。

- **M3 内容与输入**
  - core：disclosure、剪贴板(OSC52)、文本选区复制、多行输入(textarea)、键盘增强标志、typeahead/命令面板/模糊匹配。
  - chat：`ToolCall` 块（消费 disclosure）。
  - chat：**`Artifact` link + `TextArtifactViewer` 最简实现（独立窗口）**。

- **M4 完善**
  - python：组件覆盖 / 上层组件注册 / `.pyi` / 主题。
  - core：toast 队列、超大块 windowing、多模态(sixel/kitty/OSC8)。
  - A.2 P1/P2 测试补齐；CI 全 workspace + clippy 清零 + `cargo llvm-cov` ≥70%。

- **M5 依赖就绪后（解锁 C.0）**
  - editor 完整化 → editor-core diff 基础（headless）。
  - `atto-ui-editor` 提供富 `ArtifactViewer`（语法高亮 / hunk diff UI），替换最简文本实现，chat 接口不变。

---

## 3. 关键设计取舍

- **取消抽象 std-only 放 core**：token/handle/registry 不需要 tokio，留 core 保证默认线程模型可用；tokio 仅在 `atto-ui-async` 提供 async-await 体验。
- **通用组件下沉、消息逻辑上浮**：disclosure/typeahead/textarea/toast 是通用 UI → core；只有「消息流式/工具块/artifact link」这类绑定会话语义的才进 chat。
- **chat 与 viewer 解耦**：chat 只产出 `Artifact` link + `on_open_artifact` 事件，不直接依赖 editor。最简文本 viewer 与未来富 viewer 实现同一 `ArtifactViewer` 接口，做到「接口先行、UI 渐进」。
- **feature gate 不污染默认编译**：`atto-ui-async` 与 components 的 `async` feature 默认关闭，确保不开 async 时 workspace 不引入 tokio。

---

## 4. 跟踪

- 每个能力点：实现 + 测试 + 文档，完成后在对应 PR 注明。
- 与 `CODE_REVIEW.md` P0 缺陷联动：A.2 P0 即其测试化收口。
- 与 `ASYNC.md` 联动：`atto-ui-async` 落地即 Option B 的实现归宿。
</content>
</invoke>
