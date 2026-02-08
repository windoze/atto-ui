# Demo 07: Layout Management

## 目标

演示 Atto-UI 的声明式布局能力（SwiftUI 风格）：

- `VStack` / `HStack` / `Grid`
- `Spacer`
- `LayoutParams`（`Size::{Fixed,Weight,Fill,Content}`）
- `Align`、`AnchorPlacement`
- `padding` / `margin`

## 运行

```bash
cargo run --bin demo-07-layout-management
```

## 操作

- `Ctrl+Q`：退出（总是生效）
- `q`：退出（仅当按键未被当前聚焦 UI 消费时）
- `Tab`：切换到下一个窗口（仅在事件未被 UI 消费时触发）
- `c`：关闭当前聚焦窗口（仅在事件未被 UI 消费时触发）

打开额外演示窗口（同样仅在事件未被 UI 消费时触发）：

- `v`：再开一个 VStack demo
- `h`：再开一个 HStack demo
- `g`：再开一个 Grid demo
- `a`：再开一个 Alignment/Anchor demo
- `s`：打开 Size constraints demo
- `p`：打开 Padding/Margin demo

## 说明

本演示启动时会创建多个窗口分别展示不同布局。你可以用鼠标拖动/调整窗口大小来观察布局响应。

更综合的布局/滚动示例：`examples/demo.rs`。
