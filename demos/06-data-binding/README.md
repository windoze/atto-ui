# Demo 06: Data Binding

## 目标

演示 Atto-UI 的反应式数据绑定（`Property` / `Binding`）如何做到：

- 控件与数据 **双向同步**（TextBox/Checkbox/RadioGroup 等）
- **同一份数据** 被多个窗口/组件共享（任何一处修改都会实时更新）
- `enabled/disabled` 也支持绑定（展示 Focused/Normal/Disabled 三态之一）

## 运行

```bash
cargo run --bin demo-06-data-binding
```

## 演示结构

本演示创建两个窗口：

- **Editor**：可编辑表单（Name/Email/Notes/Subscribed/Role + Buttons）
- **Mirror**：绑定到同一份状态，实时显示/编辑（用来验证双向同步）

## 退出

- `Ctrl+Q`：总是退出
- `q`：仅当按键未被当前聚焦控件消费时才退出（例如：TextBox 中输入 `q` 不会退出）

## 关键概念

### Property / Binding

```rust
let name = Property::new("Alice".to_string());
let textbox = TextBox::new("Name", name.binding());
```

`Property<T>` 负责存储状态；`.binding()` 生成 `Binding<T>`，用于传给控件做双向绑定。

### 跨窗口共享状态

只要多个视图拿到的是 **同一个 `Property`（或其 `Binding`）**，它们就是同步的。

## 延伸阅读

- 更综合的示例：`examples/demo.rs`
- ForEach 相关：`examples/foreach_advanced.rs`
