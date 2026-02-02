# Chatty Framework Demos

欢迎来到 Chatty 框架的教学演示！这些演示从简单到复杂，循序渐进地展示框架的各个特性。

## 演示列表

### 1. [Hello World](./01-hello-world/) ⭐ 入门必看
学习 Chatty 应用的基本结构：终端初始化、Desktop 创建、窗口创建和事件循环。

```bash
cargo run --bin demo-01-hello-world
```

### 2. [Window Management](./02-window-management/) ⭐ 入门必看
学习如何动态创建、关闭和管理多个窗口，了解不同窗口类型（Normal、Modal、Floating）。

```bash
cargo run --bin demo-02-window-management
```

### 3. [Event Handling](./03-event-handling/)
深入了解事件处理机制：键盘事件、鼠标事件、事件冒泡和捕获。

```bash
cargo run --bin demo-03-event-handling
```

### 4. [Menu Creation](./04-menu-creation/)
学习如何创建应用菜单、处理菜单选择和设置键盘快捷键。

```bash
cargo run --bin demo-04-menu-creation
```

### 5. [Basic Controls](./05-basic-controls/) ⭐ 推荐
学习使用框架提供的基础控件：Button、TextBox、Checkbox、RadioGroup、ListBox、TableView。

```bash
cargo run --bin demo-05-basic-controls
```

### 6. [Data Binding](./06-data-binding/) ⭐ 推荐
学习反应式数据绑定：Property、Binding、Observable，实现数据和 UI 的自动同步。

```bash
cargo run --bin demo-06-data-binding
```

### 7. [Layout Management](./07-layout-management/) ⭐ 推荐
学习声明式布局系统：VStack、HStack、Grid、Padding、Alignment、Anchor 等。

```bash
cargo run --bin demo-07-layout-management
```

### 8. [ForEach Demo](./08-foreach-demo/)
学习使用 ForEach 创建动态列表（增删/重排/复用稳定 ID），并结合滚动条展示滚动体验。

```bash
cargo run --bin demo-08-foreach-demo
```

### 9. [Custom Components](./09-custom-components/)
学习如何创建自定义可复用组件，封装复杂功能。

```bash
cargo run --bin demo-09-custom-components
```

### 10. [File Dialog](./10-file-dialog/)
学习如何使用 `FileDialog` 创建 Open/Save 文件对话框，并在模态窗口中返回选择结果。

```bash
cargo run --bin demo-10-file-dialog
```

## 学习路径

### 初学者路径
1. Hello World → 2. Window Management → 5. Basic Controls → 7. Layout Management

这条路径覆盖了创建基本 TUI 应用所需的核心知识。

### 进阶路径
3. Event Handling → 4. Menu Creation → 6. Data Binding → 8. ForEach Demo → 9. Custom Components

这条路径深入框架的高级特性，帮助你构建复杂的应用。

## 通用操作

所有演示都支持以下操作：
- `Ctrl+W` - 进入窗口管理模式
- `Ctrl+Q` - 退出应用（总是生效）
- `q` - 退出应用（仅当该按键没有被当前聚焦的 UI 消费时；例如：在 TextBox 中输入 `q` 不会退出）
- `F10` - 激活菜单栏（如果有）
- 鼠标点击窗口标题栏可拖动窗口
- 鼠标点击窗口边角可调整大小
- 支持鼠标滚轮滚动（当视图可滚动时）
- 部分演示启用了 Bracketed Paste（可直接粘贴包含换行/Unicode 的文本）

## 额外资源

- [主 README](../README.md) - 项目概述
- [IMPLEMENTATION_PLAN.md](../IMPLEMENTATION_PLAN.md) - 详细的功能清单
- [SWIFTUI_STYLE_REFACTOR.md](../SWIFTUI_STYLE_REFACTOR.md) - 声明式 API 设计
- [examples/demo.rs](../examples/demo.rs) - 功能齐全的综合演示

## 获取帮助

如果遇到问题或有疑问：
1. 查看各演示目录中的 README.md
2. 阅读项目根目录的 CLAUDE.md
3. 查看源代码中的文档注释
4. 运行 `examples/demo.rs` 这个综合示例查看更多功能

Happy coding!
