# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

Chatty 是一个基于 Crossterm 和 Ratatui 构建的多窗口 TUI (Terminal User Interface) 应用框架,受 Turbo Vision 启发。它提供了完整的窗口管理系统、菜单栏、状态栏以及常用组件库。

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

### 2. Window Manager 层 (`src/wm/`)
- **`window.rs`**: 定义 `Window` 结构,包含窗口装饰(标题栏、边框、控件按钮、阴影)
- **`manager.rs`**: `WindowManager` 管理多个窗口的生命周期、Z 序、焦点和布局
- 支持窗口类型: Normal, Modal, Tooltip, Floating
- 支持窗口状态: Normal, Minimized, Maximized
- 处理窗口拖动、调整大小、最小化/最大化/关闭等操作

### 3. App 层 (`src/app/`)
- **`desktop.rs`**: `Desktop` 是最高层容器,组合了 MenuBar + WindowManager + StatusBar
- **`menu.rs`**: `MenuBar` 提供顶部菜单栏,支持嵌套菜单和键盘快捷键
- **`status.rs`**: `StatusBar` 提供底部状态栏,支持动态内容

### 4. Widgets (`src/widgets/`)
标准 UI 组件,都实现了 `View` trait:
- `Button` - 按钮
- `Checkbox` - 复选框
- `Radio` - 单选按钮
- `Label` - 文本标签
- `Textbox` - 文本输入框
- `List` - 列表
- `Table` - 表格
- `primitives.rs` - 基础绘制原语

### 5. 支持模块
- **`text/`**: 文本缓冲区和 Unicode 处理
- **`theme/`**: 主题和样式系统

## 测试策略

项目使用独特的 PTY 测试方法:

1. **Test Host** (`crates/chatty-test-host/`):
   - 使用 `portable-pty` 创建伪终端
   - 使用 `vt100` 解析器捕获屏幕缓冲区
   - 提供 `PtyTestHost` API 来启动应用、发送输入、验证输出

2. **Test Binary** (`src/bin/snapshot_app.rs`):
   - 一个完整的测试应用,展示各种窗口和组件
   - 被集成测试调用,运行在 PTY 中

3. **Integration Tests** (`tests/`):
   - `pty_desktop.rs` - 测试桌面、菜单、窗口管理
   - `pty_modal.rs` - 测试模态对话框
   - `pty_mouse_support.rs` - 测试鼠标交互

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

## 代码约定

- 项目使用 `#![forbid(unsafe_code)]`,严禁使用 unsafe 代码
- Edition 2024
- 支持 Unicode 和宽字符渲染
- 使用 Crossterm 处理终端 I/O 和事件
- 使用 Ratatui 作为底层渲染引擎
- 鼠标坐标使用 0-based (与 Crossterm 的 `MouseEvent` 一致)

## 关键依赖

- `crossterm` - 跨平台终端操作
- `ratatui` - TUI 渲染框架
- `unicode-segmentation` - Unicode 文本分段
- `unicode-width` - Unicode 字符宽度计算
- `portable-pty` (测试) - PTY 创建
- `vt100` (测试) - VT100 终端模拟器
