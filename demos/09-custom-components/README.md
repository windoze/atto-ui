# Demo 09: Custom Components

## 目标

演示如何用“声明式 + 反应式”方式封装可复用组件：

- 组件是普通 Rust `struct`（实现 `DeclarativeView`）
- 组件可以接收 **bindings**（父组件持有状态，子组件读写）
- 组件可以暴露 **callbacks**（子组件把事件回传给父组件）
- 组件可以支持 **disabled** 状态（并展示主题中的 Disabled 样式）

## 运行

```bash
cargo run --bin demo-09-custom-components
```

## 演示结构

本演示创建两个窗口：

- **Custom Components**：包含多个可复用组件（字段、计数器、搜索栏）
- **Preview**：只读预览窗口，绑定到同一份状态，实时更新

## 退出

- `Ctrl+Q`：总是退出
- `q`：仅当按键未被当前聚焦控件消费时退出（例如 TextBox 正在输入时不会退出）

## 组件示例（对应 `main.rs`）

- `LabeledField`：包装 `TextBox`，把 `Binding<String>` + `enabled` 作为参数
- `CounterRow`：用 `Binding<i32>` 实现“父持有状态，子修改状态”
- `SearchBar`：组件内部持有自己的 `Property<String>`（local state），点击 Search 通过 callback 把结果回传给父组件

## 延伸阅读

更大的综合示例（含菜单/主题切换/滚动/虚拟滚动）：`examples/demo.rs`。
