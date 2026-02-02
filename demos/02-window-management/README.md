# Demo 2: Window Management

## 目标

这个演示展示如何动态管理窗口：

- 动态创建新窗口
- 关闭窗口
- 窗口焦点管理
- 不同类型的窗口（Normal, Modal, Floating）

## 运行演示

```bash
cargo run --bin demo-02-window-management
```

## 操作

- `n` - 创建新的普通窗口
- `f` - 创建浮动窗口
- `m` - 打开模态对话框
- `c` - 关闭当前窗口
- `Tab` - 切换到下一个窗口
- `Ctrl+Q` - 退出应用（总是生效）
- `q` - 退出应用（仅当该按键没有被当前聚焦的 UI 消费时）

> 说明：本演示把 `n/f/m/c/Tab` 作为“应用级快捷键”，只有当 `desktop.handle_event(...)` 返回
> `EventOutcome::Ignored` 时才会触发，避免覆盖控件（例如 TextBox）对键盘输入的处理。

## 关键概念

### 窗口类型

```rust
pub enum WindowKind {
    Normal,     // 普通窗口：可移动、可调整大小
    Floating,   // 浮动窗口：类似 Normal，但通常更小
    Modal,      // 模态窗口：阻止其他窗口交互
    Tooltip,    // 工具提示：不可聚焦
}
```

### 动态创建窗口

```rust
let window = Window::new(
    WindowKind::Normal,
    "New Window",
    Rect { x: 10, y: 5, width: 40, height: 15 },
    Box::new(MyView),
);
let window_id = desktop.add_window(window, screen);
```

### 关闭窗口

```rust
// 方法 1：请求关闭（会触发 close_hook）
desktop.wm.request_close(window_id);

// 方法 2：强制关闭
desktop.wm.close(window_id);
```

### 窗口管理器 API

- `add_window()` - 添加窗口并返回 WindowId
- `close()` - 关闭指定窗口
- `request_close()` - 请求关闭（可被 hook 阻止）
- `focus_next()` - 切换到下一个窗口
- `bring_to_front()` - 将窗口置于顶层
- `minimize_focused()` - 最小化当前窗口
- `toggle_maximize_focused()` - 切换最大化状态

## 下一步

在 [Demo 3: Event Handling](../03-event-handling/) 中，你将学习如何处理键盘和鼠标事件。
