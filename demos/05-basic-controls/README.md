# Demo 05: Basic Controls

## 目标

演示 Atto-UI 内置控件（widgets）如何和反应式绑定（`Property`/`Binding`）配合使用：

- `Button`
- `TextBox`（单行编辑，Unicode/grapheme-aware）
- `Checkbox`
- `RadioGroup`
- `ListBox`
- `TableView`

## 运行

```bash
cargo run --bin demo-05-basic-controls
```

## 操作

- `Tab` / `Shift+Tab`：控件间切换焦点
- `Enter` / `Space`：激活按钮 / 切换复选框
- `↑` `↓`：在列表/表格/单选组中导航
- 鼠标点击：直接操作控件
- `Ctrl+Q`：退出（总是生效）
- `q`：退出（仅当按键未被当前聚焦控件消费时；例如 TextBox 正在输入时不会退出）

## 控件与绑定（核心用法）

Atto-UI 的控件普遍使用 **双向绑定**：

### TextBox（文本输入）

```rust
let text = Property::new(String::new());
let textbox = TextBox::new("Name", text.binding());
```

### Checkbox（布尔开关）

```rust
let checked = Property::new(false);
let cb = Checkbox::new("Enable feature", checked.binding());
```

### RadioGroup（单选组）

```rust
let selection = Property::new(0usize);
let rg = RadioGroup::new(
    "Mode",
    vec!["A".into(), "B".into(), "C".into()],
    selection.binding(),
);
```

### Button（回调）

```rust
let clicks = Property::new(0u32);
let btn = Button::new("Count +1").on_click(move || {
    clicks.update(|c| *c = c.saturating_add(1));
});
```

### enabled/disabled（三态之一）

大多数控件支持 `.enabled(...)`，既可以传 `bool`，也可以传 `Binding<bool>`：

```rust
let enabled = Property::new(true);
let textbox = TextBox::new("Name", text.binding()).enabled(enabled.binding());
```

## 说明

本演示启用了鼠标捕获和 Bracketed Paste，方便直接粘贴包含 Unicode/emoji 的文本。

## 下一步

继续看 [Demo 06: Data Binding](../06-data-binding/) 学习跨窗口/跨组件共享绑定的写法。
