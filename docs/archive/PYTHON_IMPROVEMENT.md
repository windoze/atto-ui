# Python Wrapper Investigation (atto-ui-python)

## Scope
Investigate how to add an ergonomic Python wrapper around the existing dynamic component system (no code changes yet). This report summarizes the current Python binding capabilities, relevant data formats, and the challenges/solutions for implementing a higher-level wrapper API like:

```python
def on_click(event: Event, source: Component):
    source.window.elements["text1"].set_text("Button Clicked")

root = VStack(spacing=1, children=[
    Button(label="Click me", on_click=on_click, disabled=False),
    Text("Hello World", id="text1"),
])
window1 = app.add_dynamic_window(title="My Window", content=root)
app.run()
```

## Current State (What Exists Today)

### Python binding surface
- `atto_ui.AppHost` is the only exposed class (PyO3).  
  - `add_dynamic_window(title, rect, root_spec)` accepts **dict/list/tuple** structures and builds a dynamic window.  
  - `apply_tree_ops(window_id, ops)` applies `TreeOp` patches.  
  - `step()` / `run()` drive the event loop.  
  - `drain_callbacks()` returns a list of callback invocation dicts.  
  - `schemas()` returns component schemas (properties/actions/events).  
  - Source: `crates/atto-ui-python/src/lib.rs`.

### Dict formats already supported
The Python binding parses dicts directly into runtime types:

**ComponentSpec** (root/normal node)
```python
{
  "type": "VStack",        # required (or "type_name")
  "id": "root",           # optional
  "props": { ... },        # optional
  "events": { "click": 1 }, # optional (callback id)
  "children": [ ... ]      # optional (ComponentSpec or ComponentSpecChild)
}
```

**ComponentSpecChild** (for layout/meta)
```python
{
  "node": { ... },
  "layout": {
    "width": "fill" | "content" | {"fixed": 10} | {"weight": 1},
    "height": "fill" | "content" | {"fixed": 8} | {"weight": 1},
    "margin": 1 | [1,2,3,4] | {"top":1,"right":2,"bottom":3,"left":4},
    "align_x": "start" | "center" | "end" | "stretch",
    "align_y": "start" | "center" | "end" | "stretch",
    "anchor": {"anchor": "top_left", "offset_x": 0, "offset_y": 0},
    "tab_index": 0,
  },
  "meta": { "title": "Tab 1" }
}
```
Note: the parser also accepts `{"type":..., "layout":...}` without an explicit `node` key.

**TreeOp**
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
The parser is lenient about `op` vs `type` vs `kind` keys, and for `set_tree` accepts `tree`/`spec`/`root`.

**Rect** accepted as tuple/list/dict.

**ComponentValue conversion** accepts Python primitives and converts to:
- `bool`, `int`, `float`, `str`, `bytes/bytearray`
- list of strings -> `StringList`
- list of list-of-strings -> `Table`
- dict with `{x,y,width,height}` -> `Rect`
- else -> `Map`

### Built-in components and schema
`AppHost.schemas()` is already wired to the built-in runtime registry (`src/runtime/mod.rs`). Built-ins include:
- Button, Checkbox, Label, StyledLabel, Text, TextBox, Slider, ProgressBar
- RadioGroup, ListBox, TableView, Spinner, TabView
- VStack, HStack, Grid, Splitter, Divider, Spacer, Border, Visibility

Each schema includes property metadata (name/type/readable/writable), actions, and events.

### Callbacks
Runtime callbacks are queued via `CallbackRegistry`, and Python can `drain_callbacks()` to get:
```python
{"callback_id": 1, "target_id": "ok", "event": "click", "payload": None}
```
Only StyledLabel currently emits a payload (link URL). Most widgets emit callbacks **without payload**.

## How a Thin Python Wrapper Could Work

### 1) Wrapper layering
**Recommended:** A pure-Python wrapper layered on top of the existing `atto_ui.AppHost`.
- Keeps Python callables in Python (no GIL crossing in Rust).
- Reuses the existing dict protocol unchanged.

Alternative: implement wrapper classes in Rust with PyO3. This avoids packaging conflicts but is heavier and still needs Python-callable dispatch logic.

### 2) Proposed Python API surface
- `App` (wraps `atto_ui.AppHost`)
  - `add_dynamic_window(title, content, rect=(...)) -> Window`
  - `run()` / `step()` dispatch callbacks and drive rendering
  - `dispatch_callbacks()` maps callback IDs to Python callables
- `Window`
  - `id` (numeric WindowId from Rust)
  - `elements` dict mapping `id -> ComponentRef`
  - `apply_tree_ops(...)` (optional convenience)
- `Component` base
  - `type_name`, `id`, `props`, `events`, `children`, `layout`, `meta`
  - `.to_dict()` / `.from_dict()`
  - `.set_prop(name, value)` -> emits TreeOp
- `ComponentRef` (proxy, id-based)
  - `set_prop`, `get_prop` (if exposed), `bind_event`, etc.

### 3) Event routing
Wrapper should keep its own mapping:
```
callback_id -> (callable, source_component_id, window)
```
Then, on each `drain_callbacks()`:
- Look up callback ID
- Construct `Event` object with `event`, `payload`, `target_id`
- Provide `source` as the component proxy (`window.elements[target_id]`)

### 4) Serialization / Deserialization
Wrapper can implement:
- `Component.to_dict()` -> existing ComponentSpec/ComponentSpecChild formats
- `Component.from_dict()` -> reconstruct wrapper tree
- `WindowSpec` dataclass for `{title, rect, root}` if needed

## Challenges & Potential Solutions

### 1) **Reading live properties (runtime values)**
**Problem:** Python binding exposes `set_prop` via TreeOp but not `get_prop`. For widgets like TextBox/Slider/ListBox, the value changes due to user input; those updates are not reflected in the spec dict.

**Options:**
1. **Expose `get_property` / `set_property` via PyO3** using `DesktopInspector` (see `src/inspect.rs`).
   - Pros: accurate runtime values, works for all widgets implementing `get_property`.
   - Cons: requires adding PyO3 API and window lookup by id.
2. **Emit event payloads** from widgets (change callbacks emit current value).
   - Pros: no inspector needed, natural event-driven state.
   - Cons: requires adding `emit_with(...)` to many widgets.
3. **Maintain Python-side state only** (treat wrapper state as truth).
   - Pros: no Rust changes.
   - Cons: gets out of sync after user input; not sufficient for “read property” requirement.

**Recommendation:** add `AppHost.get_property(id, name)` / `set_property(id, name, value)` bindings using `DesktopInspector`. For windows, add a path keyed by numeric `WindowId` (see below).

### 2) **Window property access by ID**
`DesktopInspector` currently looks up windows by `Window.tag` (string), but `add_dynamic_window` does **not** set a tag; it returns a numeric `WindowId` only. This blocks `get_property` for windows.

**Options:**
- Extend inspector APIs to look up windows by `WindowId`.
- Or allow `add_dynamic_window` to accept an optional `tag` for window lookup.

### 3) **Callback ID allocation**
Python currently supplies raw integers. There is no exposed `CallbackRegistry.register()`.

**Options:**
- Wrapper-managed monotonic counter (simplest; already works).  
- Or add a PyO3 method `alloc_callback_id()` to avoid collisions.

### 4) **Packaging: extension module vs Python package**
The native module is currently named `atto_ui`. To add a pure-Python wrapper package with the same name, you will likely need to:
- rename the native module to `atto_ui._native`, and
- add `atto_ui/__init__.py` that imports `_native` and provides wrappers.

This would require adjusting `pyproject.toml` (`module-name`) and likely minor packaging tweaks.

Alternative: implement wrapper classes in Rust and keep the module name as-is.

### 5) **Tree ops can’t update layout/meta directly**
Layout and meta live in `ComponentSpecChild`. There is no TreeOp to update layout/meta; only `Replace` or `SetTree` can change them.

**Solution:** expose helper methods that replace the child node with a new spec-child (wrapper can hide this by generating a `Replace` op internally).

### 6) **ID stability and uniqueness**
Tree ops and callback routing require stable unique component IDs.
- There is no runtime enforcement of uniqueness.

**Solution:**
- Wrapper should enforce uniqueness on construction.
- Auto-generate IDs for any component that needs events or property access if user doesn’t provide one.

### 7) **Schema validation and ergonomics**
Runtime ignores unknown props (silent no-op). This can be confusing for Python users.

**Solution:**
- Use `app.schemas()` to validate property names/types before sending `set_prop`.
- Optionally generate Python component classes at runtime based on schema (dynamic DSL).

### 8) **Action invocation not exposed to Python**
Runtime supports actions (`ComponentCommand`) and widgets declare actions in schema, but Python has no API to dispatch them.

**Option:** expose a PyO3 method to send a `ComponentCommand` by component ID. This would enable `.click()`, `.input_text(...)`, `.select_index(...)` convenience helpers.

## Suggested Minimal Implementation Path

1. **Python wrapper layer (pure Python)**
   - `App`, `Window`, `Component`, `ComponentRef`, `Event` classes.
   - `Component.to_dict()` / `.from_dict()` using the existing dict format.
   - Callback dispatch loop using `AppHost.step()` + `drain_callbacks()`.

2. **Small PyO3 extensions** (to satisfy “read/write property” requirement)
   - `get_property(id, name)` / `set_property(id, name, value)` using `DesktopInspector`.
   - Optional: `alloc_callback_id()` if desired.
   - Optional: window lookup by numeric `WindowId`.

3. **Optional improvements**
   - Update widgets to emit payloads on change events (reduces need for `get_property`).
   - Expose action dispatch for a full ergonomic API.

## Key File References
- Python binding: `crates/atto-ui-python/src/lib.rs`
- Minimal Python example: `crates/atto-ui-python/examples/minimal_app.py`
- Runtime dynamic tree: `src/runtime/mod.rs` + `src/runtime/spec.rs`
- Inspector APIs (get/set/action): `src/inspect.rs`
- Window manager: `src/wm/window.rs`, `src/wm/manager/mod.rs`

---
If you want, I can turn this into a concrete implementation plan or a staged task list next.
