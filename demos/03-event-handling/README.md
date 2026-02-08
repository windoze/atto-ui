# Demo 3: Event Handling

## 目标

深入了解 Atto-UI 的事件处理机制：

- 键盘事件处理
- 鼠标事件处理（点击、拖动、滚轮）
- 事件冒泡和捕获
- 事件消费（consumed vs ignored）

## 运行演示

```bash
cargo run --bin demo-03-event-handling
```

## 操作

- 在左侧面板点击鼠标查看鼠标事件
- 按任意键查看键盘事件
- 滚动鼠标滚轮查看滚轮事件
- `c` - 清空事件日志
- `Ctrl+Q` - 退出（总是生效）
- `q` - 退出（仅当该按键没有被当前聚焦的 UI 消费时）

## 关键概念

### Component Event Handling

每个 Component 可以实现三个事件处理方法：

```rust
trait Component {
    // 捕获阶段（从父到子）
    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    // 主处理
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }

    // 冒泡阶段（从子到父）
    fn handle_event_bubble(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        EventResult::ignored()
    }
}
```

### Event Result

```rust
pub struct EventResult {
    pub outcome: EventOutcome,  // Consumed 或 Ignored
    pub action: ComponentAction,     // None 或 CloseWindow
}
```

- `EventOutcome::Consumed` - 事件已处理，停止传播
- `EventOutcome::Ignored` - 事件未处理，继续传播
- `ComponentAction::CloseWindow` - 请求关闭当前窗口

### 常见事件类型

```rust
match event {
    Event::Key(KeyEvent { code, modifiers, .. }) => {
        // 处理键盘事件
    }
    Event::Mouse(MouseEvent { kind, column, row, .. }) => {
        match kind {
            MouseEventKind::Down(button) => { /* 鼠标按下 */ }
            MouseEventKind::Up(button) => { /* 鼠标释放 */ }
            MouseEventKind::Moved => { /* 鼠标移动 */ }
            MouseEventKind::ScrollDown => { /* 向下滚动 */ }
            MouseEventKind::ScrollUp => { /* 向上滚动 */ }
            _ => {}
        }
    }
    _ => {}
}
```

## 下一步

在 [Demo 4: Menu Creation](../04-menu-creation/) 中，你将学习如何创建菜单系统。
