# 任务索引

> 各任务列表的总览索引,仅含 **任务 ID / 状态 / 标题 / 来源位置**;完整的文件、步骤、验收见各来源文件。
> 两组任务编号命名空间不重叠:`NT*/NR*` → `TODO-1.md`(Node binding + React UI 库),`T*/R*` → `TODO-2.md`(atto-editor-app)。
> 状态取值:`TODO` / `DONE`。

---

## TODO-1.md — Node binding + React 风格 UI 库（来源:PLAN-1.md）

| ID | 状态 | 标题 | 来源位置 |
|----|------|------|----------|
| NT1 | DONE | [DONE] `atto-ui-node` crate 脚手架 + napi build（B.0） | TODO-1.md · 阶段一 M0+M1 |
| NR1 | DONE | 审阅 NT1 | TODO-1.md · 阶段一 M0+M1 |
| NT2 | DONE | [DONE] serde 数据转换层（B.2） | TODO-1.md · 阶段一 M0+M1 |
| NR2 | DONE | 审阅 NT2 | TODO-1.md · 阶段一 M0+M1 |
| NT3 | DONE | id handle 包装 + 错误映射（B.3 / B.4） | TODO-1.md · 阶段一 M0+M1 |
| NR3 | DONE | 审阅 NT3 | TODO-1.md · 阶段一 M0+M1 |
| NT4 | DONE | `#[napi] AppHost` 全方法暴露（B.1） | TODO-1.md · 阶段一 M0+M1 |
| NR4 | DONE | [DONE] 审阅 NT4 | TODO-1.md · 阶段一 M0+M1 |
| NT5 | DONE | [DONE] `@atto-ui/core` native 加载（L.1） | TODO-1.md · 阶段一 M0+M1 |
| NR5 | DONE | [DONE] 审阅 NT5 | TODO-1.md · 阶段一 M0+M1 |
| NT6 | DONE | [DONE] `TreeOp::InsertBefore` 锚点版插入（R.1） | TODO-1.md · 阶段二 M2 |
| NR6 | DONE | [DONE] 审阅 NT6 | TODO-1.md · 阶段二 M2 |
| NT7 | DONE | [DONE] `RichText` + `TextSpan` 结构化富文本（R.2） | TODO-1.md · 阶段二 M2 |
| NR7 | DONE | [DONE] 审阅 NT7 | TODO-1.md · 阶段二 M2 |
| NT8 | DONE | [DONE] react-reconciler HostConfig 骨架 + 节点 id + 静态渲染（U.1） | TODO-1.md · 阶段三 M3+M4 |
| NR8 | DONE | [DONE] 审阅 NT8 | TODO-1.md · 阶段三 M3+M4 |
| NT9 | DONE | [DONE] props/子节点增删/事件 op 映射（U.1） | TODO-1.md · 阶段三 M3+M4 |
| NR9 | DONE | [DONE] 审阅 NT9 | TODO-1.md · 阶段三 M3+M4 |
| NT10 | DONE | [DONE] `render()` + tick 主循环（U.2） | TODO-1.md · 阶段三 M3+M4 |
| NR10 | DONE | [DONE] 审阅 NT10 | TODO-1.md · 阶段三 M3+M4 |
| NT11 | DONE | [DONE] 事件分发桥（U.3） | TODO-1.md · 阶段三 M3+M4 |
| NR11 | DONE | [DONE] 审阅 NT11 | TODO-1.md · 阶段三 M3+M4 |
| NT12 | DONE | [DONE] React 文本组件（U.5） | TODO-1.md · 阶段四 M5 |
| NR12 | DONE | [DONE] 审阅 NT12 | TODO-1.md · 阶段四 M5 |
| NT13 | DONE | [DONE] 虚拟 DesktopContainer + `<Window>` host 节点 + op 分桶（U.4） | TODO-1.md · 阶段五 M6 |
| NR13 | DONE | [DONE] 审阅 NT13 | TODO-1.md · 阶段五 M6 |
| NT14 | DONE | [DONE] host 组件库 + JSX 类型 + 受控输入（U.6） | TODO-1.md · 阶段六 M7 |
| NR14 | DONE | [DONE] 审阅 NT14 | TODO-1.md · 阶段六 M7 |
| NT15 | DONE | [DONE] `@atto-ui/core` 命令式构造器（L.2） | TODO-1.md · 阶段六 M7 |
| NR15 | DONE | [DONE] 审阅 NT15 | TODO-1.md · 阶段六 M7 |
| NT16 | DONE | [DONE] reconciler 单测矩阵（T.1） | TODO-1.md · 阶段七 M8 |
| NR16 | DONE | [DONE] 审阅 NT16 | TODO-1.md · 阶段七 M8 |
| NT17 | DONE | [DONE] PTY 端到端（T.2） | TODO-1.md · 阶段七 M8 |
| NR17 | DONE | [DONE] 审阅 NT17 | TODO-1.md · 阶段七 M8 |
| NT18 | DONE | [DONE] 示例 app（含流式聊天）（T.3） | TODO-1.md · 阶段七 M8 |
| NR18 | DONE | [DONE] 审阅 NT18 | TODO-1.md · 阶段七 M8 |
| NT19 | DONE | [DONE] 跨平台预编译 + npm 包（P.1 / P.2） | TODO-1.md · 阶段八 M9 |
| NR19 | DONE | [DONE] 审阅 NT19 | TODO-1.md · 阶段八 M9 |
| NT20 | DONE | [DONE] CI 流水线 + Bun/Deno + 文档（P.3） | TODO-1.md · 阶段八 M9 |
| NR20 | DONE | [DONE] 审阅 NT20 | TODO-1.md · 阶段八 M9 |

---

## TODO-2.md — atto-editor-app 全功能编辑器（来源:PLAN-2.md）

| ID | 状态 | 标题 | 来源位置 |
|----|------|------|----------|
| T1 | DONE | [DONE] C1 通用拖拽数据模型与 Component hooks | TODO-2.md · 阶段一 |
| R1 | DONE | [DONE] 审阅 T1 | TODO-2.md · 阶段一 |
| T2 | DONE | [DONE] C1 WindowManager 全局拖拽会话与反馈绘制 | TODO-2.md · 阶段一 |
| R2 | DONE | [DONE] 审阅 T2 | TODO-2.md · 阶段一 |
| T3 | DONE | [DONE] C2 Docking 类型、work area reserve 与基础绘制 | TODO-2.md · 阶段一 |
| R3 | DONE | [DONE] 审阅 T3 | TODO-2.md · 阶段一 |
| T4 | DONE | [DONE] C2 Dock resize / auto-hide / hit-test | TODO-2.md · 阶段一 |
| R4 | DONE | [DONE] 审阅 T4 | TODO-2.md · 阶段一 |
| T5 | DONE | [DONE] C2 atto-editor-app Explorer 改用 WM Docking | TODO-2.md · 阶段一 |
| R5 | DONE | [DONE] 审阅 T5 | TODO-2.md · 阶段一 |
| T6 | DONE | [DONE] 阶段三首批编辑动作接线 | TODO-2.md · 阶段一 |
| R6 | DONE | [DONE] 审阅 T6 | TODO-2.md · 阶段一 |
| T7 | DONE | [DONE] L1 LSP diagnostics 数据接收与状态模型 | TODO-2.md · 阶段一 |
| R7 | DONE | [DONE] 审阅 T7 | TODO-2.md · 阶段一 |
| T8 | DONE | [DONE] L1 diagnostics gutter/statusbar 渲染与 F8 跳转 | TODO-2.md · 阶段一 |
| R8 | DONE | [DONE] 审阅 T8 | TODO-2.md · 阶段一 |
| T9 | DONE | [DONE] L2 Code Action 请求、列表 popup 与单文档应用 | TODO-2.md · 阶段一 |
| R9 | DONE | [DONE] 审阅 T9 | TODO-2.md · 阶段一 |
| T10 | DONE | [DONE] C4 MenuBar mnemonic/accelerator 与 Turbo Vision 绘制 | TODO-2.md · 阶段二 |
| R10 | TODO | 审阅 T10 | TODO-2.md · 阶段二 |
| T11 | TODO | C4 分段式 StatusBar 与 editor diagnostics 接入 | TODO-2.md · 阶段二 |
| R11 | TODO | 审阅 T11 | TODO-2.md · 阶段二 |
| T12 | TODO | C3 框架级多键序列 keymap engine | TODO-2.md · 阶段二 |
| R12 | TODO | 审阅 T12 | TODO-2.md · 阶段二 |
| T13 | TODO | Command registry 与 which-key popup | TODO-2.md · 阶段二 |
| R13 | TODO | 审阅 T13 | TODO-2.md · 阶段二 |
| T14 | TODO | 通用 Picker component 与 Command Palette | TODO-2.md · 阶段二 |
| R14 | TODO | 审阅 T14 | TODO-2.md · 阶段二 |
| T15 | TODO | File picker 与 Buffer/tab picker | TODO-2.md · 阶段二 |
| R15 | TODO | 审阅 T15 | TODO-2.md · 阶段二 |
| T16 | TODO | Document symbols / Workspace symbols / Global search pickers | TODO-2.md · 阶段二 |
| R16 | TODO | 审阅 T16 | TODO-2.md · 阶段二 |
| T17 | TODO | Workspace / LSP Bridge 状态层 | TODO-2.md · 阶段三 |
| R17 | TODO | 审阅 T17 | TODO-2.md · 阶段三 |
| T18 | TODO | L3 Rename UI 与跨已打开文件 WorkspaceEdit 应用 | TODO-2.md · 阶段三 |
| R18 | TODO | 审阅 T18 | TODO-2.md · 阶段三 |
| T19 | TODO | L4 Signature Help | TODO-2.md · 阶段三 |
| R19 | TODO | 审阅 T19 | TODO-2.md · 阶段三 |
| T20 | TODO | L5 Formatting 手动格式化与保存前格式化接口 | TODO-2.md · 阶段三 |
| R20 | TODO | 审阅 T20 | TODO-2.md · 阶段三 |
| T21 | TODO | L6 Inlay Hints 与 composed grid 渲染 | TODO-2.md · 阶段三 |
| R21 | TODO | 审阅 T21 | TODO-2.md · 阶段三 |
| T22 | TODO | F-FT FileTree 节点模型、git status 样式与多选 | TODO-2.md · 阶段四 |
| R22 | TODO | 审阅 T22 | TODO-2.md · 阶段四 |
| T23 | TODO | F-FT Context menu 与 inline new/rename | TODO-2.md · 阶段四 |
| R23 | TODO | 审阅 T23 | TODO-2.md · 阶段四 |
| T24 | TODO | F-FT Drag move、剪贴板与 Git status 刷新 | TODO-2.md · 阶段四 |
| R24 | TODO | 审阅 T24 | TODO-2.md · 阶段四 |
| T25 | TODO | Auto-pairs / auto-indent 改用 editor-core 原语 | TODO-2.md · 阶段五 |
| R25 | TODO | 审阅 T25 | TODO-2.md · 阶段五 |
| T26 | TODO | Trim trailing whitespace 与 save 流程整理 | TODO-2.md · 阶段五 |
| R26 | TODO | 审阅 T26 | TODO-2.md · 阶段五 |
| T27 | TODO | Jumplist / registers 设计占位与 WorkspaceEditorView 决策 | TODO-2.md · 阶段五 |
| R27 | TODO | 审阅 T27 | TODO-2.md · 阶段五 |
| T28 | TODO | 更新测试 fixture 与 mock LSP 覆盖矩阵 | TODO-2.md · 全局验证与维护 |
| R28 | TODO | 审阅 T28 | TODO-2.md · 全局验证与维护 |
| T29 | TODO | 文档与实施顺序维护 | TODO-2.md · 全局验证与维护 |
| R29 | TODO | 审阅 T29 | TODO-2.md · 全局验证与维护 |
