# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

Atto UI 是一个基于 Crossterm 和 Ratatui 构建的多窗口 TUI (Terminal User Interface) 应用框架,受 Turbo Vision 启发。它提供了完整的窗口管理系统、菜单栏、状态栏以及常用组件库。

### 项目规模
- **Rust 源代码文件**: 333 个 (按 `rg --files -g '*.rs'` 统计)
- **Rust 代码行数**: 约 147,800 行非空非 `//` 开头行 (粗略统计,含测试、示例、应用与绑定)
- **测试框架**: 独立的 PTY 测试工具 crate `atto-ui-test-host`,并补充进程内 introspection / scriptable 断言路径
- **工作区 Crates**: `atto-ui-test-host` (测试框架),`atto-ui-async`,`atto-ui-macros` (过程宏),`atto-ui-chat`,`atto-ui-components`,`atto-ui-markdown`,`atto-ui-editor`,`atto-ui-terminal`,`atto-ui-python`,`atto-ui-node`,`atto-ui-file-tree`,`atto-agent-app`,`atto-editor-app`
- **JavaScript packages**: `packages/core` (`@atto-ui/core`) 与 `packages/react` (`@atto-ui/react`)

## 常用命令

### 构建和测试
```bash
# 构建项目
cargo build

# 构建并运行主示例应用
cargo run --example demo

# 构建并运行快照测试应用 (用于测试)
cargo run --bin snapshot_app

# 运行所有测试 (包括 PTY 集成测试)
cargo test

# 运行单个测试
cargo test test_name

# 运行特定测试文件中的所有测试
cargo test --test pty_desktop

# 检查代码 (快速类型检查)
cargo check

# 运行 clippy linter
cargo clippy

# 格式化代码
cargo fmt
```

### 调试测试
测试使用 PTY 来运行 TUI 应用,可以通过 `PtyTestHost::spawn()` 启动 `snapshot_app` 并模拟键盘和鼠标输入来进行集成测试。测试会验证屏幕缓冲区的内容。

## 架构概述

项目采用分层架构,从底层到高层:

```
┌─────────────────────────────────────────────────┐
│   应用层 (App Layer)                            │
│   Desktop / MenuBar / StatusBar                │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│   窗口管理层 (Window Manager)                    │
│   WindowManager / Window                       │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│   组合式组件层 (Composable Layer)               │
│   Component / VStack / Grid / ScrollContainer  │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│   控件层 (Widgets)                              │
│   Button / TextBox / Checkbox / ListBox ...    │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│   支持模块                                       │
│   Theme / Text / Reactive                      │
└─────────────────────────────────────────────────┘
```

### 1. Composable 组件层 (`src/composable/`)
- 核心抽象: `Component` trait + `ComponentContext` + `EventResult`
- 布局类型: `LayoutParams`, `EdgeInsets`, `Align`, `Anchor`, `Size`
- 容器: `VStack`, `HStack`, `Grid`, `Border`, `ScrollContainer`
- 基础组件: `Text`, `TextFn`, `Divider`, `Spacer`
- 列表: `ForEach` / `ForEachIdentifiable` (稳定 ID 复用)
- 滚动: `ScrollConfig`, `ScrollbarVisibility`, 统一滚动条与虚拟内容接口

### 2. Window Manager 层 (`src/wm/`)
- **`window.rs`**: 定义 `Window` 结构,包含窗口装饰(标题栏、边框、控件按钮、阴影)
- **`manager.rs`**: `WindowManager` 管理多个窗口的生命周期、Z 序、焦点和布局
- 支持窗口类型: Normal, Modal, Tooltip, Floating
- 支持窗口状态: Normal, Minimized, Maximized
- 处理窗口拖动、调整大小、最小化/最大化/关闭等操作
- 支持滚动条拖动 (包括轨道点击、滑块拖动、箭头按钮)

### 3. App 层 (`src/app/`)
- **`desktop.rs`**: `Desktop` 是最高层容器,组合了 MenuBar + WindowManager + StatusBar
- **`menu.rs`**: `MenuBar` 提供顶部菜单栏,支持嵌套菜单和键盘快捷键
- **`status.rs`**: `StatusBar` 提供底部状态栏,支持动态内容

### 4. Widgets (`src/widgets/`)
标准 UI 组件,直接实现 `Component`:
- **`button.rs`**: `Button` - 按钮 (Enter/Space 激活,鼠标点击支持)
- **`checkbox.rs`**: `Checkbox` - 复选框 (Space/Enter 切换)
- **`radio.rs`**: `RadioGroup` - 单选按钮组 (上下箭头切换)
- **`label.rs`**: `Label` - 静态文本标签
- **`textbox.rs`**: `TextBox` - 单行文本输入框
  - 基于 `TextBuffer` (Unicode 感知)
  - 支持光标移动、删除/退格、鼠标点击定位、粘贴
  - 内容超出时自动水平滚动
- **`list.rs`**: `ListBox` - 列表框 (上下箭头选择,鼠标点击)
- **`table.rs`**: `TableView` - 表格视图 (表头 + 数据行)

### 5. 支持模块
- **`text/`**: 文本缓冲区和 Unicode 处理
  - `buffer.rs` - `TextBuffer` 基于 grapheme cluster 的文本编辑缓冲区
- **`theme/`**: 主题和样式系统
  - `mod.rs` - `Theme` 主题定义 (深色/浅色主题,字形映射,样式表)
  - `config.rs` - `ThemeConfig` 主题配置文件格式 (支持 JSON/YAML)
  - `tests.rs` - 主题测试
- **`reactive/`**: 反应式属性和事件队列支持
  - `property.rs` - `Property<T>` 和 `Binding<T>` 反应式属性
  - `dirty.rs` - `DirtyFlag` 内部状态标记工具
  - `queue.rs` - `EventQueue` 事件队列
- **`runtime/`**: 语言无关动态组件树桥接层
  - `mod.rs` - 内置组件注册、动态组件构建、回调注册表与增量 tree-ops 应用
  - `spec.rs` - `ComponentSpec` / `TreeOp` / schema / value 类型；原独立 runtime crate 已合并到 `atto-ui::runtime`
- **`macros/`**: 过程宏支持 (`crates/atto-ui-macros/`)
  - `reactive.rs` - `#[reactive]` 宏 - 自动生成反应式属性
  - `view_builder.rs` - `view_builder!` 宏 - 组合式组件构建助手

### 6. Workspace 应用与扩展 Crates
- **`crates/atto-editor-app/`**: 终端编辑器应用 crate,包名 `atto-editor-app`,库入口 `atto_editor_app`
- **`crates/atto-ui-editor/`**: 基于 `editor-core` 的编辑器组件库
- **`crates/atto-ui-python/`**: Python 绑定,通过 `atto-ui::runtime` 与核心动态组件树交互
- **`crates/atto-ui-components/`**: workspace 附加组件的动态注册聚合入口

## 测试策略

项目使用独特的 PTY 测试方法:

1. **Test Host** (`crates/atto-ui-test-host/`):
   - 使用 `portable-pty` 创建伪终端
   - 使用 `vt100` 解析器捕获屏幕缓冲区
   - 提供 `PtyTestHost` API 来启动应用、发送输入、验证输出

2. **Test Binary** (`src/bin/`):
   - `snapshot_app.rs` - 主测试应用,展示各种窗口和组件
   - `snapshot_scroll_app.rs` - 垂直滚动测试应用
   - `snapshot_hscroll_app.rs` - 水平滚动测试应用
   - `snapshot_virtual_scroll_app.rs` - 虚拟滚动测试应用 (测试委托驱动的内容渲染)
   - 这些测试应用被集成测试调用,运行在 PTY 中

3. **Integration Tests** (`tests/`, 11 个测试文件):
   - `pty_desktop.rs` - 测试桌面、菜单、窗口管理
   - `pty_modal.rs` - 测试模态对话框
   - `pty_mouse_support.rs` - 测试鼠标交互
   - `pty_scrolling.rs` - 测试垂直滚动功能 (键盘、鼠标滚轮、滚动条拖动)
   - `pty_horizontal_scrolling.rs` - 测试水平滚动功能
   - `pty_virtual_scrolling.rs` - 测试虚拟滚动功能 (委托驱动的大规模数据渲染)
   - `pty_view_hierarchy.rs` - 测试视图层次和布局容器
   - `composable_primitives.rs` - 测试基础组件 (Text, Divider, Spacer)
   - `composable_vstack.rs` - 测试 VStack 布局
   - `macro_reactive.rs` - 测试反应式宏 (#[reactive])
   - `macro_view_builder.rs` - 测试视图构建宏 (#[view_builder])

测试模式:
```rust
// 启动应用
let bin = env!("CARGO_BIN_EXE_snapshot_app");
let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

// 等待文本出现
host.wait_for_text("Expected text", Duration::from_secs(2))?;

// 发送键盘输入
host.send_str("hello")?;
host.send_ctrl('q')?;

// 发送鼠标点击 (0-based 坐标)
host.click(10, 5)?;

// 模拟粘贴
host.send_paste("你好👋")?;

// 验证屏幕内容
let screen = host.screen_contents()?;
assert!(screen.contains("Expected"));
```

## 开发流程

根据 README.md 的说明,开发应遵循以下流程:

1. 查看 `TODO.md` 获取当前任务顺序、依赖、验收要求和完成记录;`PLAN.md` 只记录阶段级计划。
2. 按照 `TODO.md` 中第一个未完成任务逐步实现功能。
3. 为每个任务编写充分测试:逻辑 / 状态优先用进程内 introspection / scriptable 断言,渲染和端到端行为继续使用 PTY 测试框架。
4. 完成任务后在 `TODO.md` 中把任务标题标为 `[DONE]` 并补完成记录;仅当阶段级计划变化时更新 `PLAN.md`。
5. 使用有意义的提交信息提交变更。

### 当前实现状态

当前主线是脚本化 / introspection 控制平面,详见 `TODO.md`、`PLAN.md` 和 `SCRIPTING_LAYERS.md`。已完成的核心能力包括:

- **第 1 层 introspection**:公共 `find_by_tag` / `find_by_tag_mut`,`DesktopInspector` 门面(可变句柄,读方法也会跑 layout/draw,非类型级只读)、属性名查询、tag 覆盖诊断、dirty change tracker。
- **第 2 层 scriptable**:关键控件 `apply_command`,进程内 `query` / `invoke` / `wait_for`,以及测试侧 scriptable helper。
- **第 3 层 IPC**:Unix socket + JSON-RPC 类协议,`ATTO_UI_SOCKET`,外部 `atto` CLI。
- **第 4 层 tmux adapter**:tmux 环境注入、DCS passthrough、terminal pane IPC 方法、client-side `tmux` shim、本地 pane 方向导航 / resize / zoom / close。

早期 M0-M7 终端 app 计划已归档到 `docs/archive/2026-07-12-terminal-app/`。`IMPLEMENTATION_PLAN.md` / `SWIFTUI_STYLE_REFACTOR.md` 仍可作为历史设计资料参考,但 routine 任务执行不再以它们为准。

## 声明式 API (SwiftUI 风格)

项目提供 SwiftUI 风格的声明式 API 来构建容器组合和常规 UI。

**注意**: 早期版本曾有命令式 API (VBox/HBox/Grid),现已完全移除并迁移到声明式 API。

### 声明式构建方式
使用 `VStack`、`HStack`、`Grid` 等声明式构建器,代码简洁清晰:

```rust
VStack::new()
    .padding(EdgeInsets::all(1))
    .children(vec![
        Text::new("Hello").into(),
        Text::new("World").into(),
    ])
    .build()
```

**声明式 API 的优势**:
- 更简洁的代码,减少样板代码
- 链式调用,更符合现代 UI 框架风格
- 自动处理布局参数和类型转换
- 与 SwiftUI/Jetpack Compose 等现代框架理念一致

**分层约定**:
- 容器组合优先使用 `VStack`、`HStack`、`Grid` 等声明式 API。
- 叶子级高频重绘组件可以手写 `impl Component`,例如 editor/file-tree 这类需要精细控制绘制、命中测试或状态更新的组件。

**声明式组件**:
- `VStack` / `HStack` - 垂直/水平堆栈布局
- `Grid` - 网格布局
- `Text` - 文本元素
- `Divider` - 分隔线
- `Spacer` - 空白占位符

## 反应式状态管理

项目提供反应式状态管理基础设施,用于属性通知、绑定和宏辅助:

- `Property<T>` - 反应式属性,值变化时自动通知观察者
- `Binding<T>` - 双向绑定,连接数据模型和 UI 组件
- `DirtyFlag` - 内部状态标记工具
- `#[reactive]` 宏 - 自动为结构体生成反应式属性

## 代码约定

- 项目使用 `#![forbid(unsafe_code)]`,严禁使用 unsafe 代码
- Edition 2024
- 支持 Unicode 和宽字符渲染
- 使用 Crossterm 处理终端 I/O 和事件
- 使用 Ratatui 作为底层渲染引擎
- 鼠标坐标使用 0-based (与 Crossterm 的 `MouseEvent` 一致)
- 布局坐标系统使用父视图相对坐标 (不是绝对屏幕坐标)
- 滚动容器使用 0-based 偏移量表示滚动位置
- 布局权重使用 `u16` 类型,表示相对比例 (如 1, 2 表示 1:2 的空间分配)
- **容器组合优先使用声明式 API** (`VStack`/`HStack`/`Grid`);叶子级高频重绘组件可手写 `impl Component`

## 关键依赖

### 核心依赖
- `crossterm` (0.28) - 跨平台终端操作
- `ratatui` (0.30) - TUI 渲染框架
- `unicode-segmentation` (1) - Unicode 文本分段
- `unicode-width` (0.2) - Unicode 字符宽度计算
- `anyhow` (1) - 错误处理
- `serde`, `serde_json`, `serde_yaml` - 主题配置序列化支持
- `once_cell` (1) - 延迟初始化
- `parking_lot` (0.12) - 同步原语

### 开发/测试依赖
- `atto-ui-test-host` - 自定义 PTY 测试框架 (工作区 crate)
  - `portable-pty` - PTY 创建
  - `vt100` - VT100 终端模拟器
- `atto-ui-macros` - 过程宏库 (工作区 crate)
  - `proc-macro2`, `quote`, `syn` - 过程宏基础

## 示例应用

### Demo 应用 (`examples/demo.rs`)
功能齐全的演示应用,展示框架的所有核心能力:

**主要功能**:
- 多窗口管理 (创建、移动、调整大小、最小化/最大化/关闭)
- 菜单系统 (File/Edit/Windows 菜单,支持嵌套子菜单)
- 状态栏显示
- 主题切换 (深色/浅色主题)
- 模态对话框
- 工具提示窗口
- 浮动窗口

**组件演示窗口**:
- 按钮、复选框、单选按钮演示
- 文本输入框 (支持 Unicode 和粘贴)
- 列表框和表格视图
- 禁用状态控件演示

**布局演示窗口**:
- VStack/HStack 垂直/水平堆栈布局
- Grid 网格布局
- 锚点定位演示 (9 种锚点)
- Padding/Margin 演示

**滚动演示窗口**:
- 垂直/水平滚动
- 滚动条交互 (拖动、点击轨道、箭头按钮)
- 键盘和鼠标滚轮滚动

**虚拟滚动演示窗口**:
- 大规模数据集渲染 (1000+ 行)
- 委托驱动的内容渲染
- 高效的虚拟滚动

**声明式 UI 演示** (如已实现):
- VStack/HStack 堆栈布局示例
- Grid 网格布局示例
- 声明式构建器模式演示

**键盘快捷键**:
- `F10` - 激活菜单
- `Ctrl+W` - 进入窗口管理模式
- `F2` - 切换主题 (深色/浅色)
- `Ctrl+Q` - 退出应用
- `n` - 新建窗口
- `a` - 显示关于对话框
- `t` - 显示工具提示
- `d` - 组件状态演示
- `v` - 布局演示
- `s` - 滚动演示
- `z` - 虚拟滚动演示

## 技术亮点

### 1. 完全类型安全
- 使用 Rust 的类型系统确保编译时安全
- 没有使用 unsafe 代码 (`#![forbid(unsafe_code)]`)
- 泛型和 trait 实现灵活的抽象

### 2. 高性能渲染
- 虚拟滚动支持数千行数据的流畅渲染
- 渲染输出依赖 Ratatui 的双缓冲 diff 机制减少终端更新
- 滚动视口只渲染可见区域,适配大规模列表和表格

### 3. Unicode 完整支持
- 基于 grapheme cluster 的文本处理
- 正确处理宽字符 (CJK 字符、emoji 等)
- 支持复杂的 Unicode 组合字符

### 4. 确定性测试
- 基于 PTY 的集成测试框架
- 屏幕缓冲区快照测试
- 模拟键盘、鼠标、粘贴等所有交互
- 11 个综合集成测试套件

### 5. 现代 UI 范式
- SwiftUI 风格的声明式 API
- 反应式属性与绑定基础设施
- 过程宏简化常见模式
- 容器组合优先声明式,叶子级高频组件可手写 `impl Component`

### 6. 灵活的主题系统
- 深色/浅色主题内置支持
- 命名令牌系统 (字形、样式、颜色)
- JSON/YAML 主题文件加载
- 运行时主题切换

## 文档资源

- **TODO.md** - 当前任务顺序、验收要求、依赖和完成记录
- **PLAN.md** - 当前阶段级计划和完成标准
- **SCRIPTING_LAYERS.md** - 脚本化 / introspection 控制平面分层设计与最终决策
- **docs/archive/2026-07-12-terminal-app/** - 已归档的全功能多窗口终端 app 阶段资料
- **SWIFTUI_STYLE_REFACTOR.md** - SwiftUI 风格重构历史设计文档
- **IMPLEMENTATION_PLAN.md** - 历史开发里程碑资料
- **README.md** - 项目概述和快速开始
- **AGENTS.md** - 代理工具使用说明
