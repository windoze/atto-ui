# EDITOR_APP.md — atto-editor-app 全功能编辑器规划

把 `atto-editor-app` 扩展成全功能终端编辑器的总体规划。文档分两部分:
**Part 1** 是 `atto-ui` 框架层的通用能力(非 editor 专用,任何 app 都能用);
**Part 2** 是 editor / atto-ui-editor 专用的工作。

## 目标与方向决策

- **功能对齐 Helix**:LSP、pickers、多光标、textobject、命令面板等能力对齐 Helix;**UI / 交互不照搬**。
- **键位模型 = 非模态 (CUA)**:不做 Normal/Insert/Select 模态状态机,继续用 `Ctrl+C/V`、方向键等。Helix 的功能通过 **菜单 + 命令面板 + 组合键(含多键序列)** 暴露。改动集中在"新增功能"而非"重写输入层"。
- **优先级 = LSP / 智能先行**:底层 `editor-core-lsp` 全套已就绪但 view 层只接了子集,接线性价比最高。
- **通用能力下沉**:拖拽、docking、多键序列、menu/statusbar 都是**框架级通用能力**,实现在 `atto-ui` 核心(`src/composable`、`src/wm`、`src/app`),不写进 editor app。editor 只做这些能力的**消费者**。

## 现状摸底

### 底层引擎(`editor-core` 0.4.1 全家桶)—— 能力很完整,大半未接到 UI
- 编辑原语:`ToggleComment`、`JoinLines`、`MoveLinesUp/Down`、`DuplicateLines`、`Indent/Outdent`、`SplitLine`、snippets
- 移动/选区:`MoveWordLeft/Right`、`MoveToMatchingBracket`、`SelectWord/Line`、`ExpandSelection`(tree-sitter 语法扩选)
- 多光标:`AddCursorAbove/Below`、`AddNextOccurrence`、`AddAllOccurrences`
- `editor-core-lsp`:hover、completion、goto def/decl/type/impl/refs、**code action、rename/prepare rename、signature help、document/workspace symbols、formatting、diagnostics、call/type hierarchy、inlay hints、code lens、document links/highlights**
- `editor-core-treesitter` / `sublime`:语法高亮、折叠;`editor-core-diff` / `diff-view`:diff(T20A 已接入)

### `atto-ui-editor`(view 层)—— 目前是 CUA 风格非模态编辑器
- 已接:撤销/重做、复制/剪切/粘贴、退格/删除、方向键 + Shift 选区、翻页、折叠、查找替换、鼠标多光标/框选、hover/补全/goto 系列
- 键位是**单 chord 查表**(`HashMap<KeyChord, EditorAction>`),无模式、无多键序列、无命令行

### `atto-ui` 框架层 —— 通用能力的缺口
- **拖拽**:`src/composable` 只有**局部拖拽**(滚动条 `ScrollbarDrag`、splitter `DragState`),**没有跨组件的通用拖拽**(拖拽会话 / payload / drop target / 拖拽视觉反馈)。
- **窗口 docking**:`Window` 仅有 `movable`/`resizable` 两个 `Binding<bool>`,**没有 docking 框架**。Explorer 的"停靠"是 app 层手算 rect 模拟的,无固定边、无边缘拖拽、无 auto-hide。
- **键位**:editor 的 keymap 是单 chord;框架层没有通用的**多键序列**引擎。
- **menu / statusbar**(`src/app/menu.rs`、`status.rs`):外观偏简陋(statusbar 单行单 style),需翻新,向 **Turbo Vision** 风格对齐。

### `atto-editor-app`(应用层)
- 已有:Desktop 窗口管理、文件树 Explorer、标签页、split、文件对话框、保存、主题
- File tree(`atto-ui-file-tree`)实现简陋:仅 select/rename/delete 回调、filter、glyphs、滚动条。

---

# Part 1 — atto-ui 框架层通用能力

任务沿用 `TODO.md` 的 `T<n>` / `R<n>` 编号约定;每个任务配 PTY 测试 + `cargo fmt` / `cargo clippy --workspace --all-targets -D warnings` / 相关测试。

### C1 — 通用拖拽 (drag-and-drop) 基础设施 【底层目前完全没有】
在 `src/composable`(必要时配合 `src/wm`)引入一套**通用拖拽会话**,任何组件都能作为拖拽源或放置目标:
- **拖拽会话**:鼠标按下并移动超过阈值时进入 drag 状态,携带**类型化 payload**(如文件路径、节点 id、标签 id 等)。
- **drop target**:组件可声明自己是放置目标,在 drag-over / drop 时收到 payload 并决定是否接受。
- **视觉反馈**:拖拽中的"幽灵"/插入指示线/高亮 drop 区(终端下用样式/字符表达)。
- **事件模型**:扩展 `EventResult` / `ComponentContext`,支持 drag-start / drag-over / drop / drag-cancel 的分发(可能需要 WM 级的全局 drag 状态,跨窗口拖拽时尤甚)。
- 现有滚动条、splitter 的局部拖拽保持不变或逐步收敛到统一模型。
- **消费者**:file tree 的拖拽移动(F-FT)、标签页重排、docking 窗口拖拽都基于此。

### C2 — Docking window 框架 (`src/wm/`)
把 docking 从 app 层临时方案下沉为 WM 级能力:
- **停靠位**:窗口可固定在桌面 **左 / 右 / 下**(可扩展上),停靠后**不可自由移动**。
- **边缘调整**:只能通过停靠区**朝内的那条边**拖拽调整大小(复用 C1 或独立的边缘 resize 逻辑)。
- **work area 联动**:停靠窗口占据的空间从其他窗口可用区域中扣除(reserve);`Desktop::layout` / work_area 需感知停靠窗口。
- **Auto-hide**:停靠窗口可设为自动隐藏 —— 收起为边缘一条"标签/把手",鼠标悬停或点击时滑出,失焦后收回。
- 设计上在 `Window` / `WindowManager` 引入 dock 状态(如 `Dock { side, size, auto_hide, pinned }`)。
- **消费者**:Explorer 改用新 docking 框架;后续 LSP 诊断面板、搜索结果面板等也可停靠。

### C3 — 多键序列 keymap 引擎
**最终要做**,独立于模态。价值:单快捷键数量有限,高级编辑器命令多,需要前缀链(如 VSCode `Ctrl+K Ctrl+F` chord)。
- 提供框架级 keymap 抽象:从 `HashMap<KeyChord, Action>` 升级为 **trie / 待定前缀状态机** —— 按下前缀键进入"等待后续键"状态,超时或匹配后落定。
- 配套 **which-key 风格弹窗**:进入前缀状态后展示后续可选键及动作。
- 与命令面板(P2)共享 action 注册表(命令既能从面板触发,也能绑定 chord)。
- 不绑定模态;默认仍是非模态,多键序列只是额外的绑定能力。editor keymap 迁移到此引擎。

### C4 — Menu / StatusBar 外观翻新(向 Turbo Vision 对齐)
当前 menu/statusbar 外观偏简陋。目标 Turbo Vision 风格(实现于 `src/app`,全框架受益):
- **MenuBar**:经典高亮(热键字符高亮/下划线)、选中项反色、下拉菜单带边框/阴影、快捷键右对齐显示。
- **StatusBar**:分段式(多 segment 不同 style)、左右分区、可点击的功能键提示条(`F1 Help` 等 Turbo Vision 按钮条)。
- 复用主题系统(`Theme`),提供 Turbo Vision 配色预设。
- editor 的状态栏在此基础上叠加语言/位置/编码/诊断计数等内容。

---

# Part 2 — editor / atto-ui-editor 专用工作

### 阶段一:LSP / 智能(先行)

| ID | 内容 | 依赖原语(已就绪) |
|---|---|---|
| L1 诊断显示 | gutter 标记 + 行内下划线 + 状态栏错误/警告计数;`F8`/`Shift+F8` 在诊断间跳转 | `request_document_diagnostic`、`lsp_diagnostics_to_processing_edits`、`DiagnosticsState` |
| L2 Code Action | 光标处拉取 code action,列表弹窗(复用补全弹窗 UI),应用 workspace edit | `request_code_action`、`apply_plan_for_code_action_item`、`apply_workspace_edit` |
| L3 Rename | `F2` prepare rename → 输入框 → 应用跨文件 edit | `request_prepare_rename`、`request_rename`、`apply_workspace_edit_to_workspace` |
| L4 Signature Help | 输入 `(`/`,` 时弹参数提示 | `request_signature_help`、`signature_help_from_value` |
| L5 Formatting | 手动格式化 + 保存时可选格式化 | `request_formatting`、`apply_text_edits` |
| L6 Inlay Hints | 行内类型/参数提示(可配置开关) | `lsp_inlay_hints_to_decorations` |

> **架构注意**:当前 LSP session 内嵌在 `EditorView`(单文档绑定)。rename / workspace symbol 等跨文件能力需要 app 层共享的 workspace LSP 管理。建议 L1-L2 先做(单文档够用),L3 前先理清 workspace LSP 归属。

### 阶段二:Pickers / 导航(Helix `space` 菜单的非模态版)
复用 T16 的 fuzzy matcher + `CommandPalette`,做一组弹窗 picker:
- 文件 picker(`Ctrl+P`)、buffer/标签 picker、document symbol(`Ctrl+Shift+O`)、workspace symbol(`Ctrl+T`)
- 全局搜索(ripgrep 后端,`Ctrl+Shift+F`)、最近文件
- **命令面板**(`Ctrl+Shift+P`):所有命令的非模态出口,等价于 Helix `:` 命令行;与 C3 共享 action 注册表

### 阶段三:编辑动作(接线为主,原语已全有)
- 词移动、`%` 匹配括号跳转、注释切换(`Ctrl+/`)、合并行、移动行(`Alt+↑/↓`)、复制行、缩进
- 多光标:`Ctrl+D`(加下一个匹配)、`Ctrl+Shift+L`(全部匹配)、`Ctrl+Alt+↑/↓`(上下加光标)
- Textobject 语法扩选(靠 `ExpandSelection`)、surround

### 阶段四:编辑体验打磨
auto-pairs、auto-indent、trim trailing whitespace、寄存器/jumplist(非模态下优先级低)。

### F-FT — File tree 功能补齐(对齐 VSCode / Zed)
`atto-ui-file-tree` 补齐(拖拽部分**消费 C1 通用拖拽**,不自造):
- **右键上下文菜单**:New File / New Folder / Rename / Delete / Cut / Copy / Paste / Copy Path / Reveal
- **内联编辑**:新建/重命名就地输入(而非外部对话框)
- **多选**:`Ctrl`/`Shift` 多选,批量操作
- **拖拽移动**:基于 C1,节点间拖拽移动文件/目录
- **剪贴板**:cut/copy/paste 文件
- **Git 状态着色**:modified/added/untracked/ignored 颜色区分(需 Git 状态源)
- **模糊过滤**:输入即过滤可见节点
- **图标/缩进引导线**:与 VSCode/Zed 视觉对齐(可选 nerd font glyph)
- **文件系统监听**:外部变更自动刷新(若可行)

---

## 建议起步顺序

1. **C1 通用拖拽** —— 多个上层功能(file tree、docking、标签重排)的底座,且目前完全缺失,先补。
2. **C2 Docking 框架** —— Explorer / 面板体验的底座(部分依赖 C1 的边缘拖拽或独立实现)。
3. **L1 诊断显示** —— editor 侧验证整条 LSP 接线链路,视觉反馈强、单文档自洽。
4. **C4 menu/statusbar 翻新** —— 改善整体观感,相对独立,可穿插。
5. **C3 多键序列**、**阶段二 pickers**、**F-FT file tree**、**LSP L2+** 并行推进。

各阶段内任务按 `TODO.md` 流程逐个落地。
