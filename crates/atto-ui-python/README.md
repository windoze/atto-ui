# atto-ui Python 绑定（最小可用版）

当前 Python 绑定使用 `AppHost` 作为入口，直接接收 Python 的 `dict/list/tuple` 结构（不再使用 JSON 字符串）。

## 快速示例

```python
import atto_ui

app = atto_ui.AppHost()

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
```

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

- 目前的 `callback` id 需要自行分配（后续会补注册接口）。
- `schemas()` 返回所有内置组件的动态 schema（Python list/dict 结构）。

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
