# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

Chatty 是一个基于 Crossterm 和 Ratatui 构建的多窗口 TUI (Terminal User Interface) 应用框架,受 Turbo Vision 启发。它提供了完整的窗口管理系统、菜单栏、状态栏以及常用组件库。

### 项目规模
- **源代码文件**: 52 个 Rust 源文件
- **代码行数**: 约 39,000 行
- **主要模块**: 核心库 35 文件,测试二进制 4 个,集成测试 7 个,示例应用 1 个
- **测试框架**: 独立的 PTY 测试工具 crate

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

### 1. View 层 (`src/view.rs`)
- 最底层的抽象,提供基本的渲染和输入处理接口
- 定义了 `View` trait,所有可渲染的组件都实现此 trait
- 处理 `Rect` 区域内的渲染和事件分发
- 提供滚动支持的扩展接口 (`is_scrollable()`, `content_size()`, `viewport_size()`, `scroll_offset()`)
- `ViewContext` 包含主题、窗口ID、焦点状态、滚动条宿主等渲染上下文
- `ScrollbarHost` 决定滚动条由谁渲染 (视图自己 vs 窗口装饰)

### 2. Window Manager 层 (`src/wm/`)
- **`window.rs`**: 定义 `Window` 结构,包含窗口装饰(标题栏、边框、控件按钮、阴影)
- **`manager.rs`**: `WindowManager` 管理多个窗口的生命周期、Z 序、焦点和布局
- 支持窗口类型: Normal, Modal, Tooltip, Floating
- 支持窗口状态: Normal, Minimized, Maximized
- 处理窗口拖动、调整大小、最小化/最大化/关闭等操作
- 支持滚动条拖动 (包括轨道点击、滑块拖动、箭头按钮)

### 2.5. Views/Layout 层 (`src/views/`)
在 View trait 和 Window Manager 之间,提供布局容器和滚动支持:
- **`layout.rs`**: 布局约束和对齐系统
  - `EdgeInsets` - 内边距/外边距 (top/right/bottom/left)
  - `Align` - 对齐方式 (Start/Center/End/Stretch)
  - `Size` - 尺寸约束 (Fill/Fixed/Weight/Content)
  - `Anchor` - 锚点定位 (TopLeft/Center/BottomRight 等 9 种)
  - `LayoutParams` - 视图布局参数
- **`node.rs`**: 视图层次节点,支持嵌套视图和事件路由
- **`vbox.rs`**: 垂直/水平布局容器 (`VBox`/`HBox`,从上到下或从左到右排列子视图)
- **`grid.rs`**: 网格布局容器 (行列网格排列)
- **`scroll.rs`**: 滚动配置和工具函数
  - `ScrollConfig` - 滚动配置 (滚动条可见性、箭头、滚轮步长)
  - `ScrollbarVisibility` - 可见性策略 (Always/Auto/Never)
  - 滚动条布局计算和碰撞检测函数
- **`scroll_view.rs`**: 滚动视图容器 (支持虚拟滚动)
  - `ScrollView<T: ScrollContent>` - 通用滚动容器
  - `ScrollContent` trait - 委托驱动的内容渲染协议
  - 支持大规模数据集的高效虚拟滚动
- **`border.rs`**: 边框视图包装器
  - `BorderView` - 为任意视图添加可选边框
  - 支持边框挂载的滚动条 (当内部视图可滚动时)
  - 处理滚动条交互和事件坐标转换
- **`control_view.rs`**: 控件视图适配器
  - `ControlView` - 将 `Control` (表单控件) 适配为 `View`
- 支持 padding、margin、对齐、锚点定位等布局属性
- 支持键盘、鼠标滚轮和滚动条拖动交互

### 3. App 层 (`src/app/`)
- **`desktop.rs`**: `Desktop` 是最高层容器,组合了 MenuBar + WindowManager + StatusBar
- **`menu.rs`**: `MenuBar` 提供顶部菜单栏,支持嵌套菜单和键盘快捷键
- **`status.rs`**: `StatusBar` 提供底部状态栏,支持动态内容

### 4. Widgets (`src/widgets/`)
标准 UI 组件,都实现了 `Control` trait (通过 `ControlView` 适配为 `View`):
- **`primitives.rs`**: 表单控件基础接口
  - `Control` trait - 表单控件接口 (焦点、事件处理、渲染)
  - `Form` - 表单容器,管理多个控件的焦点切换 (Tab/Shift+Tab)
  - `ControlOutcome` - 事件消费状态
  - `FormAction` - 表单动作 (None/Changed/Submitted)
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

## 测试策略

项目使用独特的 PTY 测试方法:

1. **Test Host** (`crates/chatty-test-host/`):
   - 使用 `portable-pty` 创建伪终端
   - 使用 `vt100` 解析器捕获屏幕缓冲区
   - 提供 `PtyTestHost` API 来启动应用、发送输入、验证输出

2. **Test Binary** (`src/bin/`):
   - `snapshot_app.rs` - 主测试应用,展示各种窗口和组件
   - `snapshot_scroll_app.rs` - 垂直滚动测试应用
   - `snapshot_hscroll_app.rs` - 水平滚动测试应用
   - `snapshot_virtual_scroll_app.rs` - 虚拟滚动测试应用 (测试委托驱动的内容渲染)
   - 这些测试应用被集成测试调用,运行在 PTY 中

3. **Integration Tests** (`tests/`):
   - `pty_desktop.rs` - 测试桌面、菜单、窗口管理
   - `pty_modal.rs` - 测试模态对话框
   - `pty_mouse_support.rs` - 测试鼠标交互
   - `pty_scrolling.rs` - 测试垂直滚动功能 (键盘、鼠标滚轮、滚动条拖动)
   - `pty_horizontal_scrolling.rs` - 测试水平滚动功能
   - `pty_virtual_scrolling.rs` - 测试虚拟滚动功能 (委托驱动的大规模数据渲染)
   - `pty_view_hierarchy.rs` - 测试视图层次和布局容器

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

1. 查看 `IMPLEMENTATION_PLAN.md` 了解当前进度和待办任务
2. 按照计划逐步实现功能
3. 为每个任务/里程碑编写充分的测试(使用 PTY 测试框架)
4. 完成任务后更新 `IMPLEMENTATION_PLAN.md`
5. 使用有意义的提交信息提交变更

### 当前实现状态

根据 `IMPLEMENTATION_PLAN.md`,当前已完成的里程碑:
- **M0-M5**: 核心框架、窗口系统、渲染、菜单栏、状态栏、组件库、PTY 测试框架 (已完成)
- **M6**: 视图层次和布局管理 (VBox/HBox、Grid、padding/margin、对齐、锚点定位) (已完成)
- **M7**: 视口和滚动支持 (键盘滚动、鼠标滚轮、程序化滚动) (已完成)
- **M8**: 滚动条 (渲染、拖动、点击轨道、箭头按钮、样式配置、边框挂载、窗口角落保留) (已完成)
- **M9**: 虚拟滚动 (委托驱动的内容渲染,支持大规模数据集) (已完成)

**未来计划**:
- **M10**: 主题文件 + 命名令牌 (从 JSON/YAML 加载主题,支持字形/样式/颜色的用户定义键) (计划中)

查看 `IMPLEMENTATION_PLAN.md` 了解详细的功能清单和未来计划。

## 代码约定

- 项目使用 `#![forbid(unsafe_code)]`,严禁使用 unsafe 代码
- Edition 2024
- 支持 Unicode 和宽字符渲染
- 使用 Crossterm 处理终端 I/O 和事件
- 使用 Ratatui 作为底层渲染引擎
- 鼠标坐标使用 0-based (与 Crossterm 的 `MouseEvent` 一致)
- 布局坐标系统使用父视图相对坐标 (不是绝对屏幕坐标)
- 滚动容器使用 0-based 偏移量表示滚动位置
- 布局权重使用 `f32` 类型,表示相对比例 (如 1.0, 2.0 表示 1:2 的空间分配)

## 关键依赖

### 核心依赖
- `crossterm` (0.28) - 跨平台终端操作
- `ratatui` (0.30) - TUI 渲染框架
- `unicode-segmentation` (1) - Unicode 文本分段
- `unicode-width` (0.2) - Unicode 字符宽度计算
- `anyhow` (1) - 错误处理
- `serde`, `serde_json`, `serde_yaml` - 主题配置序列化支持

### 开发/测试依赖
- `chatty-test-host` - 自定义 PTY 测试框架
  - `portable-pty` - PTY 创建
  - `vt100` - VT100 终端模拟器

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
- VBox/HBox 垂直/水平布局
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
