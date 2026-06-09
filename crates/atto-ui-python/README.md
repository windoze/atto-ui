# atto-ui Python 绑定

当前 Python 绑定提供底层 `AppHost` 和高层 `App` wrapper。`AppHost` 直接接收 Python 的 `dict/list/tuple` 结构；`App` 提供组件构造助手、回调注册、schema 驱动属性校验、主题切换和 headless e2e API。两者默认使用 headless 模式，适合端到端测试；交互式终端应用请显式传入 `headless=False`。

## 快速示例

```python
import atto_ui

app = atto_ui.AppHost(headless=True)

root = {
    "type": "VStack",
    "id": "root",
    "props": {"spacing": 1},
    "children": [
        {"type": "Label", "id": "title", "props": {"text": "Hello"}},
        {"type": "Button", "id": "ok", "props": {"label": "OK"}},
    ],
}

win_id = app.add_dynamic_window("Demo", (2, 2, 40, 12), root)

ops = [
    {"op": "set_prop", "id": "title", "name": "text", "value": "Hi"},
    {"op": "bind_event", "id": "ok", "event": "click", "callback": 1},
]

app.apply_tree_ops(win_id, ops)

while app.step():
    for ev in app.drain_callbacks():
        print("callback", ev)
    break
```

高层 wrapper 可用组件 helper 构建应用，不需要手写裸 `dict`：

```python
app = atto_ui.App(headless=True, cols=80, rows=24)
app.set_theme("dark")

window = app.add_dynamic_window(
    "Demo",
    atto_ui.VStack(
        spacing=1,
        children=[
            atto_ui.TextArea(title="Prompt", cid="prompt"),
            atto_ui.Button(label="OK", cid="ok"),
            atto_ui.ProgressBar(value=0.5, show_text=True, text="50%", cid="progress"),
        ],
    ),
)
app.step()
snapshot = app.snapshot()
window.key("enter")
```

已覆盖的 core helper 包括：`Button`、`Text`、`Label`、`TextBox`、`TextArea`、`Checkbox`、`RadioGroup`、`Slider`、`Spinner`、`ProgressBar`、`ListBox`、`TableView`、`VStack`、`HStack`、`Grid`、`Border`、`Visibility`、`Divider`、`Spacer`、`Splitter`、`TabView`、`StyledLabel`、`Disclosure`、`TypeAhead`、`CommandPalette`。

附加组件可通过聚合入口注册并直接构造：`register_all_runtime_components()`、`MarkdownViewer`、`TerminalEmulator`、`FileTree`、`ChatMessageList`、`ChatInputPanel`。

## 结构约定

### ComponentSpec

```python
{
  "type": "Label",          # 必填
  "id": "title",            # 可选
  "props": { ... },          # 可选
  "events": { "click": 1 }, # 可选 (callback id)
  "children": [ ... ]        # 可选 (ComponentSpec 或 ComponentSpecChild)
}
```

### ComponentSpecChild（可选）

```python
{
  "node": { ... },
  "layout": {
    "width": "fill" | {"fixed": 10} | {"weight": 1},
    "height": "content" | {"fixed": 8},
    "margin": 1 | [1,2,3,4] | {"top":1,"right":2,"bottom":3,"left":4},
    "align_x": "start" | "center" | "end" | "stretch",
    "align_y": "start" | "center" | "end" | "stretch",
    "anchor": {"anchor": "top_left", "offset_x": 0, "offset_y": 0},
    "tab_index": 0,
  },
  "meta": { "title": "Tab 1" }
}
```

### TreeOp

```python
{"op": "set_tree", "tree": spec}
{"op": "insert", "parent_id": "root", "index": 0, "child": spec_or_child}
{"op": "remove", "id": "node"}
{"op": "replace", "id": "node", "node": spec_or_child}
{"op": "move", "id": "node", "new_parent_id": "root", "index": 0}
{"op": "set_prop", "id": "node", "name": "text", "value": "Hi"}
{"op": "bind_event", "id": "node", "event": "click", "callback": 1}
{"op": "clear_event", "id": "node", "event": "click"}
```

## 回调事件

`app.drain_callbacks()` 返回 Python `list[dict]`：

```python
{
  "callback_id": 1,
  "target_id": "ok",
  "event": "click",
  "payload": None | <value>
}
```

## 说明

- 高层 `App` 会自动为 callable 回调分配 callback id；底层 `AppHost` 仍可直接使用显式 id。
- `App.schemas()` / `AppHost.schemas()` 返回所有内置和已注册上层组件的动态 schema（Python list/dict 结构）。
- `ComponentRef.set_prop()`、`App.set_property()` 和高层 tree-op `set_prop` 会按 schema 校验属性是否存在、是否可写以及值类型是否匹配。
- `App.set_theme("dark" | "light" | "turbo")` 可切换内置主题；`App.load_theme(path, base="dark")` 可加载 JSON/YAML 主题覆盖文件，主题文件也可声明 `base: turbo`。
- 包内附带 `atto_ui/__init__.pyi`、`atto_ui/_native.pyi` 和 `py.typed`，供 IDE 补全和类型检查工具使用。
- `send_event()` 的鼠标坐标与 Rust `AppHost::send_event` 一致，使用目标窗口内的 0-based 相对坐标。
- `snapshot()`、`list_windows()`、窗口 focus/move/resize/close/set_title 和 `set_property()` 均不依赖真实 PTY。

## 构建与运行示例

在 `crates/atto-ui-python` 目录下执行：

```bash
maturin develop
python examples/minimal_app.py
```

也可以直接运行脚本：

```bash
examples/build_and_run.sh
```

运行 Python e2e：

```bash
maturin develop
python -m unittest discover tests
```
