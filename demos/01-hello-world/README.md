# Demo 01: Hello World

## 目标

用最少的代码展示一个 Chatty 应用的骨架：

- 终端初始化（raw mode + alternate screen）
- 创建 `Desktop`（含主题 + 菜单栏）
- 创建一个窗口 + 视图
- 事件循环（draw + handle_event）
- 正确清理终端状态

## 运行

```bash
cargo run --bin demo-01-hello-world
```

## 退出

- `Ctrl+Q`：总是退出
- `q`：仅当该按键没有被当前聚焦的 UI 消费时才退出（本演示没有文本输入控件，因此 `q` 会直接退出）

## 关键 API（对应 `main.rs`）

- 创建桌面：`Desktop::new(Theme::dark(), MenuBar::new(vec![]))`
- 添加窗口：`desktop.add_window(window, screen_rect)`
- 渲染：`desktop.draw(frame)`
- 事件：`let res = desktop.handle_event(&ev, screen_rect)`（返回 `DesktopEventResult { outcome, action }`）

## 下一步

继续看 [Demo 02: Window Management](../02-window-management/) 学习如何动态创建/关闭/聚焦窗口。
