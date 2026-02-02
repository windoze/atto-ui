# Demo 08: ForEach Demo

## 目标

演示 `ForEach` 如何把一个 `Vec<T>`（绑定到 `Property<Vec<T>>`）声明式地映射成一组子视图：

- 动态增删（Add / Delete）
- 重排（Rotate / Reverse）
- `.with_id()`：基于稳定 ID 复用子视图，保留 view-local 状态（例如 TextBox 光标位置）
- 结合滚动条展示滚动体验

## 运行

```bash
cargo run --bin demo-08-foreach-demo
```

## 演示结构

本演示创建两个窗口：

- **Controls**：创建/批量添加/重排/清理数据
- **List**：`ForEach + TextBox + Checkbox + Delete button`，支持滚动与编辑

## 退出

- `Ctrl+Q`：总是退出
- `q`：仅当按键未被当前聚焦控件消费时退出（例如 TextBox 正在输入时不会退出）

## 关键概念

### ForEach + stable id（推荐）

当你的数据类型实现了 `Identifiable`，可以启用 `.with_id()`：

- 列表重排/插入时，Chatty 会按 ID 复用已有子视图
- 这对于包含 TextBox 等“有 view-local 状态”的控件尤其重要

### 虚拟滚动（不在 ForEach 内）

`ForEach` 适合 **中等规模** 的动态列表。

如果你需要真正的“虚拟滚动”（只渲染可见区域，并把内容渲染委托给用户实现），请使用：

- `ScrollView` + `ScrollContent`（见 `examples/demo.rs` 中的 virtual scrolling demo）

## 延伸阅读

更进阶的 ForEach 示例：`examples/foreach_advanced.rs`。
