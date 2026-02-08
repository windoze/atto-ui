# Demo 04: Menu Creation

## 目标

学习如何用 Atto-UI 的新菜单 API 构建菜单栏，并用回调（callback）驱动应用行为：

- `MenuBar` / `MenuSpec` / `MenuItem` 的构建方式
- 菜单激活/导航/选择
- 菜单项通过回调写入 `EventQueue`，主循环统一消费（更容易测试/组合）

## 运行

```bash
cargo run --bin demo-04-menu-creation
```

## 操作

- `F10`：激活菜单栏
- `←` `→`：切换顶层菜单
- `↑` `↓`：选择菜单项
- `Enter`：触发菜单项
- `Esc`：关闭菜单
- `Ctrl+Q`：退出（总是生效）
- `q`：退出（仅当按键未被 UI 消费时；本演示无文本输入控件，因此 `q` 会直接退出）

## 关键概念

### 1) 用 `MenuBar::new(...)` 组合菜单

本演示使用：

- `MenuSpec::new("File", vec![...])`
- `MenuItem::action("New", || { ... })`
- `MenuItem::submenu("Theme", vec![...])`
- `MenuItem::action(...).shortcut("n")`（演示菜单内快捷键提示）

### 2) 通过 `EventQueue` 连接 UI 与业务逻辑

菜单项的回调不直接修改 UI，而是把动作推入队列：

- `let actions: EventQueue<AppAction> = EventQueue::new();`
- `MenuItem::action("Quit", move || actions.push(AppAction::Quit))`

主事件循环每帧 `drain()` 队列并执行对应逻辑（更新状态视图/退出）。

## 延伸阅读

想看一个更完整的菜单 + 主题切换例子：`examples/demo.rs`。
