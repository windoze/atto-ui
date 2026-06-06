"""Ergonomic Python wrapper for atto-ui."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Dict, Iterable, List, Optional, Sequence, Tuple, Union

from . import _native

AppHost = _native.AppHost

RectLike = Union[Tuple[int, int, int, int], List[int], Dict[str, int]]
PaddingLike = Union[int, List[int], Tuple[int, int, int, int], Dict[str, int]]
SizeLike = Union[str, int, Dict[str, int]]


Callback = Callable[["Event", Optional["ComponentRef"]], None]


def register_all_runtime_components() -> None:
    _native.register_all_runtime_components()


def _short_type_name(type_name: str) -> str:
    return type_name.rsplit("::", 1)[-1]


def _normalize_name(name: str) -> str:
    return "".join(ch.lower() for ch in name if ch not in "_- ")


def _schema_property(schema: Dict[str, Any], name: str) -> Optional[Dict[str, Any]]:
    for prop in schema.get("properties", []):
        if prop.get("name") == name:
            return prop
    return None


def _is_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _is_sequence(value: Any) -> bool:
    return isinstance(value, Sequence) and not isinstance(value, (str, bytes, bytearray))


def _value_matches_schema(value: Any, value_type: str) -> bool:
    if value is None or value_type == "Unknown":
        return True
    if value_type == "Bool":
        return isinstance(value, bool)
    if value_type == "String":
        return isinstance(value, str)
    if value_type == "U64":
        return _is_int(value) and value >= 0
    if value_type == "I64":
        return _is_int(value)
    if value_type == "F64":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if value_type == "StringList":
        return _is_sequence(value) and all(isinstance(item, str) for item in value)
    if value_type == "Table":
        return _is_sequence(value) and all(
            _is_sequence(row) and all(isinstance(cell, str) for cell in row)
            for row in value
        )
    if value_type == "Rect":
        if isinstance(value, dict):
            return all(key in value and _is_int(value[key]) for key in ("x", "y", "width", "height"))
        return _is_sequence(value) and len(value) == 4 and all(_is_int(item) for item in value)
    if value_type == "Bytes":
        return isinstance(value, (bytes, bytearray))
    if value_type == "List":
        return _is_sequence(value)
    if value_type == "Map":
        return isinstance(value, dict)
    return True


def _tree_op_payload(op: Dict[str, Any]) -> Tuple[str, Optional[str], Any]:
    for key in ("op", "type", "kind"):
        value = op.get(key)
        if isinstance(value, str):
            return _normalize_name(value), None, None
    if len(op) == 1:
        key = next(iter(op))
        return _normalize_name(str(key)), str(key), op[key]
    return "", None, None


@dataclass
class Event:
    callback_id: int
    event: str
    target_id: Optional[str]
    payload: Any


class ComponentRef:
    def __init__(self, app: "App", window: "Window", cid: str) -> None:
        self.app = app
        self.window = window
        self.cid = cid

    def set_prop(self, name: str, value: Any) -> None:
        name, value = self.app._normalize_and_validate_set_prop(self.cid, name, value)
        self.window._apply_tree_ops(
            [{"op": "set_prop", "id": self.cid, "name": name, "value": value}]
        )

    def get_prop(self, name: str) -> Any:
        return self.app._native.get_property(self.cid, name)

    def bind_event(self, event: str, callback: Callback) -> int:
        callback_id = self.app.register_callback(callback)
        self.window._apply_tree_ops(
            [{"op": "bind_event", "id": self.cid, "event": event, "callback": callback_id}]
        )
        return callback_id

    def __getattr__(self, name: str) -> Any:
        if name.startswith("set_"):
            prop = name[4:]

            def _setter(value: Any, prop_name: str = prop) -> None:
                self.set_prop(prop_name, value)

            return _setter
        if name.startswith("get_"):
            prop = name[4:]

            def _getter(prop_name: str = prop) -> Any:
                return self.get_prop(prop_name)

            return _getter
        raise AttributeError(f"{self.__class__.__name__} has no attribute {name!r}")


class Component:
    def __init__(
        self,
        type_name: str,
        *,
        cid: Optional[str] = None,
        props: Optional[Dict[str, Any]] = None,
        children: Optional[Sequence["Component"]] = None,
        events: Optional[Dict[str, Union[int, Callback]]] = None,
        layout: Optional[Dict[str, Any]] = None,
        meta: Optional[Dict[str, Any]] = None,
    ) -> None:
        self.type_name = type_name
        self.cid = cid
        self.props = dict(props or {})
        self.children = list(children or [])
        self.events = dict(events or {})
        self.layout = dict(layout or {}) if layout else None
        self.meta = dict(meta or {}) if meta else None

        disabled = self.props.pop("disabled", None)
        if disabled is not None and "enabled" not in self.props:
            self.props["enabled"] = not bool(disabled)

    def with_layout(
        self,
        *,
        width: Optional[SizeLike] = None,
        height: Optional[SizeLike] = None,
        margin: Optional[PaddingLike] = None,
        align_x: Optional[str] = None,
        align_y: Optional[str] = None,
        anchor: Optional[Dict[str, Any]] = None,
        tab_index: Optional[int] = None,
    ) -> "Component":
        layout = dict(self.layout or {})
        for key, value in {
            "width": width,
            "height": height,
            "margin": margin,
            "align_x": align_x,
            "align_y": align_y,
            "anchor": anchor,
            "tab_index": tab_index,
        }.items():
            if value is not None:
                layout[key] = value
        self.layout = layout or None
        return self

    def with_meta(self, **meta: Any) -> "Component":
        current = dict(self.meta or {})
        current.update(meta)
        self.meta = current or None
        return self

    def to_dict(self) -> Dict[str, Any]:
        spec: Dict[str, Any] = {"type": self.type_name}
        if self.cid is not None:
            spec["id"] = self.cid
        if self.props:
            spec["props"] = dict(self.props)
        if self.events:
            events: Dict[str, int] = {}
            for name, handler in self.events.items():
                if callable(handler):
                    raise ValueError(
                        f"Event '{name}' uses a callable; use to_spec(app) for binding"
                    )
                events[name] = int(handler)
            if events:
                spec["events"] = events
        if self.children:
            spec["children"] = [child.to_child_dict() for child in self.children]
        return spec

    def to_child_dict(self) -> Dict[str, Any]:
        if self.layout or self.meta:
            out: Dict[str, Any] = {"node": self.to_dict()}
            if self.layout:
                out["layout"] = dict(self.layout)
            if self.meta:
                out["meta"] = dict(self.meta)
            return out
        return self.to_dict()

    def to_spec(self, app: "App") -> Dict[str, Any]:
        if self.cid is None and self.events:
            self.cid = app._allocate_component_id()

        spec: Dict[str, Any] = {"type": self.type_name}
        if self.cid is not None:
            spec["id"] = self.cid
        if self.props:
            spec["props"] = dict(self.props)
        if self.events:
            events: Dict[str, int] = {}
            for name, handler in self.events.items():
                if callable(handler):
                    callback_id = app.register_callback(handler)
                else:
                    callback_id = int(handler)
                    app._bump_callback_id(callback_id)
                events[name] = callback_id
            spec["events"] = events
        if self.children:
            spec["children"] = [child.to_child_spec(app) for child in self.children]
        return spec

    def to_child_spec(self, app: "App") -> Dict[str, Any]:
        if self.layout or self.meta:
            out: Dict[str, Any] = {"node": self.to_spec(app)}
            if self.layout:
                out["layout"] = dict(self.layout)
            if self.meta:
                out["meta"] = dict(self.meta)
            return out
        return self.to_spec(app)

    def collect_ids(self) -> List[str]:
        ids: List[str] = []
        if self.cid is not None:
            ids.append(self.cid)
        for child in self.children:
            ids.extend(child.collect_ids())
        return ids

    def collect_id_types(self) -> Dict[str, str]:
        ids: Dict[str, str] = {}
        if self.cid is not None:
            ids[self.cid] = self.type_name
        for child in self.children:
            ids.update(child.collect_id_types())
        return ids

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Component":
        if "node" in data or "layout" in data or "meta" in data:
            node = data.get("node", data)
            layout = data.get("layout")
            meta = data.get("meta")
        else:
            node = data
            layout = None
            meta = None

        type_name = node.get("type") or node.get("type_name")
        if not type_name:
            raise ValueError("component spec requires 'type'")

        children = [cls.from_dict(child) for child in node.get("children", [])]
        return cls(
            type_name=type_name,
            cid=node.get("id"),
            props=node.get("props"),
            events=node.get("events"),
            children=children,
            layout=layout,
            meta=meta,
        )


class Window:
    def __init__(
        self,
        app: "App",
        window_id: int,
        title: str,
        rect: Union[Tuple[int, int, int, int], List[int], Dict[str, int]],
        content: Component,
    ) -> None:
        self.app = app
        self.id = window_id
        self.title = title
        self.rect = rect
        self.content = content
        self.elements: Dict[str, ComponentRef] = {}
        self._refresh_elements()

    def _refresh_elements(self) -> None:
        self.elements.clear()
        for component_id in self.content.collect_ids():
            if component_id not in self.elements:
                self.elements[component_id] = ComponentRef(self.app, self, component_id)

    def _apply_tree_ops(self, ops: List[Dict[str, Any]]) -> None:
        ops = self.app._normalize_tree_ops(ops)
        self.app._native.apply_tree_ops(self.id, ops)
        self.app._refresh_component_index()

    def send_event(self, event: Dict[str, Any]) -> Dict[str, Any]:
        return self.app.send_event(self, event)

    def click(
        self,
        column: int,
        row: int,
        *,
        button: str = "left",
        modifiers: Optional[Sequence[str]] = None,
    ) -> Dict[str, Any]:
        return self.app.click(self, column, row, button=button, modifiers=modifiers)

    def key(
        self,
        key: str,
        *,
        modifiers: Optional[Sequence[str]] = None,
    ) -> Dict[str, Any]:
        return self.app.key(self, key, modifiers=modifiers)

    def paste(self, text: str) -> Dict[str, Any]:
        return self.app.paste(self, text)

    def focus(self) -> bool:
        return self.app.focus_window(self)

    def close(self) -> bool:
        return self.app.close_window(self)

    def move(self, x: int, y: int) -> bool:
        self.rect = (x, y, self.rect[2], self.rect[3]) if isinstance(self.rect, tuple) else self.rect
        return self.app.move_window(self, x, y)

    def resize(self, width: int, height: int) -> bool:
        if isinstance(self.rect, tuple):
            self.rect = (self.rect[0], self.rect[1], width, height)
        return self.app.resize_window(self, width, height)

    def set_title(self, title: str) -> bool:
        self.title = title
        return self.app.set_title(self, title)

    def to_dict(self) -> Dict[str, Any]:
        return {
            "title": self.title,
            "rect": self.rect,
            "content": self.content.to_dict(),
        }

    @classmethod
    def from_dict(cls, app: "App", data: Dict[str, Any]) -> "Window":
        content = Component.from_dict(data["content"])
        rect = data.get("rect", (2, 2, 50, 14))
        return app.add_dynamic_window(data.get("title", "Window"), content, rect=rect)


class App:
    def __init__(self, *, headless: bool = True, cols: int = 80, rows: int = 24) -> None:
        self._native = _native.AppHost(cols=cols, rows=rows, headless=headless)
        self._headless = headless
        self._next_callback_id = 1
        self._next_component_id = 1
        self._callbacks: Dict[int, Callback] = {}
        self._windows: List[Window] = []
        self._cid_to_window: Dict[str, Window] = {}
        self._cid_to_type: Dict[str, str] = {}
        self._schema_cache: Optional[Dict[str, Dict[str, Any]]] = None

    def _allocate_component_id(self) -> str:
        while True:
            component_id = f"auto_{self._next_component_id}"
            self._next_component_id += 1
            if component_id not in self._cid_to_window:
                return component_id

    def _bump_callback_id(self, callback_id: int) -> None:
        if callback_id >= self._next_callback_id:
            self._next_callback_id = callback_id + 1

    def register_callback(self, callback: Callback, callback_id: Optional[int] = None) -> int:
        if callback_id is None:
            callback_id = self._next_callback_id
            self._next_callback_id += 1
        else:
            self._bump_callback_id(callback_id)
        self._callbacks[callback_id] = callback
        return callback_id

    def add_dynamic_window(
        self,
        title: str,
        content: Union[Component, Dict[str, Any]],
        rect: Union[Tuple[int, int, int, int], List[int], Dict[str, int]] = (2, 2, 50, 14),
    ) -> Window:
        if isinstance(content, dict):
            content = Component.from_dict(content)

        self._validate_ids(content)
        spec = content.to_spec(self)
        window_id = self._native.add_dynamic_window(title, rect, spec)
        window = Window(self, window_id, title, rect, content)
        self._register_window(window)
        return window

    def _register_window(self, window: Window) -> None:
        self._windows.append(window)
        for component_id in window.elements.keys():
            self._cid_to_window[component_id] = window
        self._cid_to_type.update(window.content.collect_id_types())

    def _validate_ids(self, content: Component) -> None:
        ids = content.collect_ids()
        duplicates = {cid for cid in ids if ids.count(cid) > 1}
        if duplicates:
            dup_list = ", ".join(sorted(duplicates))
            raise ValueError(f"duplicate component ids in tree: {dup_list}")
        for cid in ids:
            if cid in self._cid_to_window:
                raise ValueError(f"component id already in use: {cid}")

    def step(self) -> bool:
        running = bool(self._native.step())
        self._dispatch_callbacks()
        return running

    def run(self) -> None:
        if self._headless:
            raise RuntimeError("run() requires App(headless=False); use step() for headless tests")
        while self.step():
            pass

    def send_event(
        self,
        window: Union[Window, int],
        event: Union[str, Dict[str, Any]],
    ) -> Dict[str, Any]:
        window_id = window.id if isinstance(window, Window) else int(window)
        result = self._native.send_event(window_id, event)
        self._dispatch_callbacks()
        return result

    def click(
        self,
        window: Union[Window, int],
        column: int,
        row: int,
        *,
        button: str = "left",
        modifiers: Optional[Sequence[str]] = None,
    ) -> Dict[str, Any]:
        return self.send_event(
            window,
            {
                "type": "mouse",
                "kind": "down",
                "button": button,
                "column": column,
                "row": row,
                "modifiers": list(modifiers or []),
            },
        )

    def key(
        self,
        window: Union[Window, int],
        key: str,
        *,
        modifiers: Optional[Sequence[str]] = None,
    ) -> Dict[str, Any]:
        return self.send_event(
            window,
            {
                "type": "key",
                "key": key,
                "modifiers": list(modifiers or []),
            },
        )

    def char(
        self,
        window: Union[Window, int],
        char: str,
        *,
        modifiers: Optional[Sequence[str]] = None,
    ) -> Dict[str, Any]:
        return self.send_event(
            window,
            {
                "type": "key",
                "char": char,
                "modifiers": list(modifiers or []),
            },
        )

    def paste(self, window: Union[Window, int], text: str) -> Dict[str, Any]:
        return self.send_event(window, {"type": "paste", "text": text})

    def snapshot(self) -> Dict[str, Any]:
        return self._native.snapshot()

    def schemas(self) -> List[Dict[str, Any]]:
        return self._native.schemas()

    def set_theme(self, name: str) -> None:
        self._native.set_theme(name)

    def load_theme(self, path: Union[str, Path], *, base: str = "dark") -> None:
        self._native.load_theme(str(path), base)

    def list_windows(self) -> List[Dict[str, Any]]:
        return self._native.list_windows()

    def close_window(self, window: Union[Window, int]) -> bool:
        window_id = window.id if isinstance(window, Window) else int(window)
        ok = bool(self._native.close_window(window_id))
        if ok:
            self._windows = [w for w in self._windows if w.id != window_id]
            for cid, mapped in list(self._cid_to_window.items()):
                if mapped.id == window_id:
                    del self._cid_to_window[cid]
                    self._cid_to_type.pop(cid, None)
        return ok

    def focus_window(self, window: Union[Window, int]) -> bool:
        window_id = window.id if isinstance(window, Window) else int(window)
        return bool(self._native.focus_window(window_id))

    def move_window(self, window: Union[Window, int], x: int, y: int) -> bool:
        window_id = window.id if isinstance(window, Window) else int(window)
        return bool(self._native.move_window(window_id, x, y))

    def resize_window(self, window: Union[Window, int], width: int, height: int) -> bool:
        window_id = window.id if isinstance(window, Window) else int(window)
        return bool(self._native.resize_window(window_id, width, height))

    def set_title(self, window: Union[Window, int], title: str) -> bool:
        window_id = window.id if isinstance(window, Window) else int(window)
        return bool(self._native.set_title(window_id, title))

    def set_property(self, cid: str, name: str, value: Any) -> None:
        name, value = self._normalize_and_validate_set_prop(cid, name, value)
        self._native.set_property(cid, name, value)

    def get_property(self, cid: str, name: str) -> Any:
        return self._native.get_property(cid, name)

    def _schema_map(self) -> Dict[str, Dict[str, Any]]:
        if self._schema_cache is None:
            cache: Dict[str, Dict[str, Any]] = {}
            for schema in self.schemas():
                type_name = schema["type"]
                cache[type_name] = schema
                cache[_short_type_name(type_name)] = schema
            self._schema_cache = cache
        return self._schema_cache

    def _schema_for_type(self, type_name: str) -> Optional[Dict[str, Any]]:
        schemas = self._schema_map()
        return schemas.get(type_name) or schemas.get(_short_type_name(type_name))

    def _component_type_for_id(self, cid: str) -> Optional[str]:
        type_name = self._cid_to_type.get(cid)
        if type_name is None:
            self._refresh_component_index()
            type_name = self._cid_to_type.get(cid)
        return type_name

    def _normalize_and_validate_set_prop(
        self, cid: str, name: str, value: Any
    ) -> Tuple[str, Any]:
        type_name = self._component_type_for_id(cid)
        if type_name is None:
            return name, value

        schema = self._schema_for_type(type_name)
        if schema is None:
            return name, value

        prop_name = name
        prop_meta = _schema_property(schema, prop_name)
        if prop_meta is None and prop_name == "disabled":
            enabled = _schema_property(schema, "enabled")
            if enabled is not None:
                prop_name = "enabled"
                value = not bool(value)
                prop_meta = enabled

        if prop_meta is None:
            raise ValueError(f"{type_name} has no property {name!r}")
        if not prop_meta.get("writable", True):
            raise ValueError(f"{type_name}.{prop_name} is not writable")
        expected = prop_meta.get("value_type", "Unknown")
        if not _value_matches_schema(value, expected):
            raise TypeError(
                f"{type_name}.{prop_name} expects {expected}, got {type(value).__name__}"
            )
        if expected == "Rect" and _is_sequence(value):
            x, y, width, height = value
            value = {"x": x, "y": y, "width": width, "height": height}
        return prop_name, value

    def _normalize_tree_ops(self, ops: Union[Dict[str, Any], Sequence[Dict[str, Any]]]) -> List[Dict[str, Any]]:
        raw_ops = [ops] if isinstance(ops, dict) else list(ops)
        return [self._normalize_tree_op(op) for op in raw_ops]

    def _normalize_tree_op(self, op: Dict[str, Any]) -> Dict[str, Any]:
        if not isinstance(op, dict):
            return op
        out = dict(op)
        op_name, payload_key, payload = _tree_op_payload(out)
        if op_name != "setprop":
            return out

        data = dict(payload) if isinstance(payload, dict) else out
        cid = data.get("id")
        name = data.get("name")
        if isinstance(cid, str) and isinstance(name, str) and "value" in data:
            name, value = self._normalize_and_validate_set_prop(cid, name, data["value"])
            data["name"] = name
            data["value"] = value
            if payload_key is not None:
                out[payload_key] = data
            else:
                out.update(data)
        return out

    def _refresh_component_index(self) -> None:
        windows_by_id = {window.id: window for window in self._windows}
        next_cid_to_window: Dict[str, Window] = {}
        next_cid_to_type: Dict[str, str] = {}

        def visit(node: Dict[str, Any]) -> None:
            cid = node.get("id")
            window_id = node.get("window_id")
            if node.get("kind") == "component" and isinstance(cid, str):
                window = windows_by_id.get(window_id)
                if window is not None:
                    next_cid_to_window[cid] = window
                    next_cid_to_type[cid] = _short_type_name(
                        node.get("name") or node.get("type_name") or ""
                    )
            for child in node.get("children", []):
                visit(child)

        visit(self.snapshot()["tree"])
        self._cid_to_window = next_cid_to_window
        self._cid_to_type = next_cid_to_type
        for window in self._windows:
            window.elements = {
                cid: ComponentRef(self, window, cid)
                for cid, mapped in next_cid_to_window.items()
                if mapped is window
            }

    def _dispatch_callbacks(self) -> None:
        for ev in self._native.drain_callbacks():
            callback_id = ev.get("callback_id")
            handler = self._callbacks.get(callback_id)
            if handler is None:
                continue
            event = Event(
                callback_id=callback_id,
                event=ev.get("event"),
                target_id=ev.get("target_id"),
                payload=ev.get("payload"),
            )
            source = None
            if event.target_id:
                window = self._cid_to_window.get(event.target_id)
                if window:
                    source = window.elements.get(event.target_id)
            handler(event, source)


def _drop_none(props: Dict[str, Any]) -> Dict[str, Any]:
    return {key: value for key, value in props.items() if value is not None}


def _callback_events(**events: Optional[Callback]) -> Dict[str, Callback]:
    return {name: callback for name, callback in events.items() if callback is not None}


def _children_or_empty(children: Optional[Sequence[Component]]) -> Sequence[Component]:
    return list(children or [])


def Button(
    *,
    label: str,
    on_click: Optional[Callback] = None,
    enabled: Optional[bool] = None,
    disabled: Optional[bool] = None,
    cid: Optional[str] = None,
) -> Component:
    props: Dict[str, Any] = {"label": label}
    if enabled is not None:
        props["enabled"] = enabled
    if disabled is not None:
        props["disabled"] = disabled
    events = {"click": on_click} if on_click else {}
    return Component("Button", cid=cid, props=props, events=events)


def Text(
    text: str,
    *,
    selectable: Optional[bool] = None,
    clipboard: Optional[str] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "Text",
        cid=cid,
        props=_drop_none({"text": text, "selectable": selectable, "clipboard": clipboard}),
    )


def Label(text: str, *, enabled: Optional[bool] = None, cid: Optional[str] = None) -> Component:
    return Component("Label", cid=cid, props=_drop_none({"text": text, "enabled": enabled}))


def TextBox(
    *,
    title: str,
    text: str = "",
    placeholder: Optional[str] = None,
    clipboard: Optional[str] = None,
    enabled: Optional[bool] = None,
    on_change: Optional[Callback] = None,
    on_submit: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    events: Dict[str, Callback] = {}
    if on_change:
        events["change"] = on_change
    if on_submit:
        events["submit"] = on_submit
    props: Dict[str, Any] = _drop_none(
        {
            "title": title,
            "text": text,
            "placeholder": placeholder,
            "clipboard": clipboard,
            "enabled": enabled,
        }
    )
    return Component("TextBox", cid=cid, props=props, events=events)


def VStack(
    *,
    children: Sequence[Component],
    spacing: int = 0,
    padding: Optional[PaddingLike] = None,
    scrollable: Optional[bool] = None,
    cid: Optional[str] = None,
) -> Component:
    props: Dict[str, Any] = {"spacing": spacing}
    if padding is not None:
        props["padding"] = padding
    if scrollable is not None:
        props["scrollable"] = scrollable
    return Component("VStack", cid=cid, props=props, children=children)


def HStack(
    *,
    children: Sequence[Component],
    spacing: int = 0,
    padding: Optional[PaddingLike] = None,
    scrollable: Optional[bool] = None,
    cid: Optional[str] = None,
) -> Component:
    props: Dict[str, Any] = {"spacing": spacing}
    if padding is not None:
        props["padding"] = padding
    if scrollable is not None:
        props["scrollable"] = scrollable
    return Component("HStack", cid=cid, props=props, children=children)


def TextArea(
    *,
    title: str,
    text: str = "",
    height: int = 5,
    enter_submits: bool = False,
    placeholder: Optional[str] = None,
    clipboard: Optional[str] = None,
    kill_ring: Optional[str] = None,
    history: Optional[Sequence[str]] = None,
    enabled: Optional[bool] = None,
    on_change: Optional[Callback] = None,
    on_submit: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "TextArea",
        cid=cid,
        props=_drop_none(
            {
                "title": title,
                "text": text,
                "height": height,
                "enter_submits": enter_submits,
                "placeholder": placeholder,
                "clipboard": clipboard,
                "kill_ring": kill_ring,
                "history": list(history) if history is not None else None,
                "enabled": enabled,
            }
        ),
        events=_callback_events(change=on_change, submit=on_submit),
    )


def Checkbox(
    *,
    label: str,
    checked: bool = False,
    enabled: Optional[bool] = None,
    on_change: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "Checkbox",
        cid=cid,
        props=_drop_none({"label": label, "checked": checked, "enabled": enabled}),
        events=_callback_events(change=on_change),
    )


def RadioGroup(
    *,
    label: str,
    options: Sequence[str],
    selection: int = 0,
    height: Optional[int] = None,
    enabled: Optional[bool] = None,
    on_change: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "RadioGroup",
        cid=cid,
        props=_drop_none(
            {
                "label": label,
                "options": list(options),
                "selection": selection,
                "height": height,
                "enabled": enabled,
            }
        ),
        events=_callback_events(change=on_change),
    )


def Slider(
    *,
    value: float,
    min: float = 0.0,
    max: float = 1.0,
    step: float = 1.0,
    enabled: Optional[bool] = None,
    on_change: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "Slider",
        cid=cid,
        props=_drop_none(
            {"min": min, "max": max, "value": value, "step": step, "enabled": enabled}
        ),
        events=_callback_events(change=on_change),
    )


def Spinner(
    text: str = "",
    *,
    running: bool = True,
    enabled: Optional[bool] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "Spinner",
        cid=cid,
        props=_drop_none({"text": text, "running": running, "enabled": enabled}),
    )


def ProgressBar(
    *,
    value: float,
    min: float = 0.0,
    max: float = 1.0,
    show_text: bool = False,
    text: Optional[str] = None,
    enabled: Optional[bool] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "ProgressBar",
        cid=cid,
        props=_drop_none(
            {
                "min": min,
                "max": max,
                "value": value,
                "show_text": show_text,
                "text": text,
                "enabled": enabled,
            }
        ),
    )


def ListBox(
    *,
    title: str,
    items: Sequence[str],
    selection: int = 0,
    height: Optional[int] = None,
    enabled: Optional[bool] = None,
    on_change: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "ListBox",
        cid=cid,
        props=_drop_none(
            {
                "title": title,
                "items": list(items),
                "selection": selection,
                "height": height,
                "enabled": enabled,
            }
        ),
        events=_callback_events(change=on_change),
    )


def TableView(
    *,
    title: str,
    headers: Sequence[str],
    rows: Sequence[Sequence[str]],
    selection: int = 0,
    height: Optional[int] = None,
    enabled: Optional[bool] = None,
    on_change: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "TableView",
        cid=cid,
        props=_drop_none(
            {
                "title": title,
                "headers": list(headers),
                "rows": [list(row) for row in rows],
                "selection": selection,
                "height": height,
                "enabled": enabled,
            }
        ),
        events=_callback_events(change=on_change),
    )


def Grid(
    *,
    children: Sequence[Component],
    columns: int = 1,
    row_gap: int = 0,
    column_gap: int = 0,
    padding: Optional[PaddingLike] = None,
    scrollable: Optional[bool] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "Grid",
        cid=cid,
        props=_drop_none(
            {
                "columns": columns,
                "row_gap": row_gap,
                "column_gap": column_gap,
                "padding": padding,
                "scrollable": scrollable,
            }
        ),
        children=children,
    )


def Border(child: Component, *, border: bool = True, cid: Optional[str] = None) -> Component:
    return Component("Border", cid=cid, props={"border": border}, children=[child])


def Divider(orientation: str = "horizontal", *, cid: Optional[str] = None) -> Component:
    return Component("Divider", cid=cid, props={"orientation": orientation})


def Spacer(*, cid: Optional[str] = None) -> Component:
    return Component("Spacer", cid=cid)


def Splitter(
    first: Component,
    second: Component,
    *,
    orientation: str = "vertical",
    split_pos: Optional[int] = None,
    min_first: Optional[int] = None,
    min_second: Optional[int] = None,
    border: Optional[bool] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "Splitter",
        cid=cid,
        props=_drop_none(
            {
                "orientation": orientation,
                "split_pos": split_pos,
                "min_first": min_first,
                "min_second": min_second,
                "border": border,
            }
        ),
        children=[first, second],
    )


def TabView(
    *,
    tabs: Optional[Sequence[Union[Component, Tuple[str, Component]]]] = None,
    children: Optional[Sequence[Component]] = None,
    selection: int = 0,
    header_position: str = "top",
    on_change: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    tab_children: List[Component] = []
    tab_items = tabs if tabs is not None else _children_or_empty(children)
    for item in tab_items:
        if isinstance(item, tuple) and len(item) == 2:
            title, child = item
            tab_children.append(child.with_meta(title=title))
        else:
            tab_children.append(item)
    return Component(
        "TabView",
        cid=cid,
        props={"selection": selection, "header_position": header_position},
        children=tab_children,
        events=_callback_events(change=on_change),
    )


def StyledLabel(
    text: str,
    *,
    enabled: Optional[bool] = None,
    on_link: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "StyledLabel",
        cid=cid,
        props=_drop_none({"text": text, "enabled": enabled}),
        events=_callback_events(link=on_link),
    )


def Disclosure(
    *,
    title: str,
    content: Optional[str] = None,
    children: Optional[Sequence[Component]] = None,
    expanded: bool = False,
    status: str = "idle",
    enabled: Optional[bool] = None,
    on_toggle: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "Disclosure",
        cid=cid,
        props=_drop_none(
            {
                "title": title,
                "content": content,
                "expanded": expanded,
                "status": status,
                "enabled": enabled,
            }
        ),
        children=_children_or_empty(children),
        events=_callback_events(toggle=on_toggle),
    )


def TypeAhead(
    *,
    title: str,
    items: Sequence[str],
    query: str = "",
    selection: int = 0,
    accepted: str = "",
    open: bool = False,
    open_on_empty: bool = False,
    placeholder: Optional[str] = None,
    height: int = 8,
    max_results: int = 8,
    enabled: Optional[bool] = None,
    on_change: Optional[Callback] = None,
    on_accept: Optional[Callback] = None,
    on_close: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "TypeAhead",
        cid=cid,
        props=_drop_none(
            {
                "title": title,
                "query": query,
                "items": list(items),
                "selection": selection,
                "accepted": accepted,
                "open": open,
                "open_on_empty": open_on_empty,
                "placeholder": placeholder,
                "height": height,
                "max_results": max_results,
                "enabled": enabled,
            }
        ),
        events=_callback_events(change=on_change, accept=on_accept, close=on_close),
    )


def CommandPalette(
    *,
    items: Sequence[str],
    title: str = "Command Palette",
    query: str = "",
    selection: int = 0,
    accepted: str = "",
    open: bool = True,
    open_on_empty: bool = True,
    placeholder: Optional[str] = None,
    height: int = 8,
    max_results: int = 8,
    enabled: Optional[bool] = None,
    on_change: Optional[Callback] = None,
    on_accept: Optional[Callback] = None,
    on_close: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "CommandPalette",
        cid=cid,
        props=_drop_none(
            {
                "title": title,
                "query": query,
                "items": list(items),
                "selection": selection,
                "accepted": accepted,
                "open": open,
                "open_on_empty": open_on_empty,
                "placeholder": placeholder,
                "height": height,
                "max_results": max_results,
                "enabled": enabled,
            }
        ),
        events=_callback_events(change=on_change, accept=on_accept, close=on_close),
    )


def MarkdownViewer(
    markdown: str,
    *,
    wrap_width: Optional[int] = None,
    show_markers: Optional[bool] = None,
    vertical_scrollbar: Optional[str] = None,
    code_block_max_height: Optional[int] = None,
    table_max_height: Optional[int] = None,
    on_link: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "MarkdownViewer",
        cid=cid,
        props=_drop_none(
            {
                "markdown": markdown,
                "wrap_width": wrap_width,
                "show_markers": show_markers,
                "vertical_scrollbar": vertical_scrollbar,
                "code_block_max_height": code_block_max_height,
                "table_max_height": table_max_height,
            }
        ),
        events=_callback_events(link=on_link),
    )


def TerminalEmulator(
    *,
    command: Optional[str] = None,
    args: Optional[Sequence[str]] = None,
    scrollback_len: Optional[int] = None,
    capture: Optional[bool] = None,
    capture_on_click: Optional[bool] = None,
    scroll_step: Optional[int] = None,
    on_input: Optional[Callback] = None,
    on_close: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "TerminalEmulator",
        cid=cid,
        props=_drop_none(
            {
                "command": command,
                "args": list(args) if args is not None else None,
                "scrollback_len": scrollback_len,
                "capture": capture,
                "capture_on_click": capture_on_click,
                "scroll_step": scroll_step,
            }
        ),
        events=_callback_events(input=on_input, close=on_close),
    )


def FileTreeNode(
    node_id: int,
    name: str,
    *,
    kind: Optional[str] = None,
    children: Optional[Sequence[Dict[str, Any]]] = None,
    expanded: bool = False,
) -> Dict[str, Any]:
    return _drop_none(
        {
            "id": node_id,
            "name": name,
            "kind": kind,
            "children": list(children or []),
            "expanded": expanded,
        }
    )


def FileTree(
    *,
    title: str,
    nodes: Sequence[Dict[str, Any]],
    selection: Optional[int] = None,
    height: Optional[int] = None,
    enabled: Optional[bool] = None,
    on_select: Optional[Callback] = None,
    on_rename: Optional[Callback] = None,
    on_delete: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "FileTree",
        cid=cid,
        props=_drop_none(
            {
                "title": title,
                "nodes": list(nodes),
                "selection": selection,
                "height": height,
                "enabled": enabled,
            }
        ),
        events=_callback_events(select=on_select, rename=on_rename, delete=on_delete),
    )


def ChatTextMessage(
    message_id: int,
    markdown: str,
    *,
    sender: str = "assistant",
    status: str = "final",
    timestamp: Optional[str] = None,
) -> Dict[str, Any]:
    return {
        "id": message_id,
        "sender": sender,
        "timestamp": timestamp,
        "status": status,
        "content": {"markdown": markdown},
    }


def ChatFileMessage(
    message_id: int,
    name: str,
    *,
    url: Optional[str] = None,
    sender: str = "assistant",
    status: str = "final",
    timestamp: Optional[str] = None,
) -> Dict[str, Any]:
    return {
        "id": message_id,
        "sender": sender,
        "timestamp": timestamp,
        "status": status,
        "content": {"file": {"name": name, "url": url}},
    }


def ChatToolCallMessage(
    message_id: int,
    name: str,
    *,
    output: str = "",
    tool_status: str = "running",
    sender: str = "assistant",
    status: str = "in_progress",
    timestamp: Optional[str] = None,
) -> Dict[str, Any]:
    return {
        "id": message_id,
        "sender": sender,
        "timestamp": timestamp,
        "status": status,
        "content": {"tool_call": {"name": name, "status": tool_status, "output": output}},
    }


def ChatArtifactMessage(
    message_id: int,
    *,
    kind: str,
    anchor: Union[int, str],
    title: str,
    sender: str = "assistant",
    status: str = "final",
    timestamp: Optional[str] = None,
) -> Dict[str, Any]:
    return {
        "id": message_id,
        "sender": sender,
        "timestamp": timestamp,
        "status": status,
        "content": {"artifact": {"kind": kind, "anchor": anchor, "title": title}},
    }


def ChatMessageList(
    *,
    messages: Sequence[Dict[str, Any]],
    spacing: Optional[int] = None,
    padding: Optional[PaddingLike] = None,
    wrap_width: Optional[int] = None,
    show_timestamps: Optional[bool] = None,
    auto_scroll: Optional[bool] = None,
    on_load_more: Optional[Callback] = None,
    on_open_artifact: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "ChatMessageList",
        cid=cid,
        props=_drop_none(
            {
                "messages": list(messages),
                "spacing": spacing,
                "padding": padding,
                "wrap_width": wrap_width,
                "show_timestamps": show_timestamps,
                "auto_scroll": auto_scroll,
            }
        ),
        events=_callback_events(load_more=on_load_more, open_artifact=on_open_artifact),
    )


def ChatInputMode(
    mode: str = "text",
    *,
    title: str = "Input",
    prompt: Optional[str] = None,
    placeholder: Optional[str] = None,
    height: Optional[int] = None,
    options: Optional[Sequence[str]] = None,
    allow_custom: Optional[bool] = None,
    submit_label: Optional[str] = None,
    yes_label: Optional[str] = None,
    no_label: Optional[str] = None,
) -> Dict[str, Any]:
    prompt_value = prompt
    if prompt_value is None and _normalize_name(mode) in {"choice", "confirm"}:
        prompt_value = title
    return _drop_none(
        {
            "type": mode,
            "title": title,
            "prompt": prompt_value,
            "placeholder": placeholder,
            "height": height,
            "options": list(options) if options is not None else None,
            "allow_custom": allow_custom,
            "submit_label": submit_label,
            "yes_label": yes_label,
            "no_label": no_label,
        }
    )


def ChatInputPanel(
    *,
    mode: Optional[Dict[str, Any]] = None,
    draft: str = "",
    custom: str = "",
    history: Optional[Sequence[str]] = None,
    selection: int = 0,
    enabled: bool = True,
    clear_on_submit: bool = True,
    on_submit: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    return Component(
        "ChatInputPanel",
        cid=cid,
        props=_drop_none(
            {
                "mode": mode or ChatInputMode(),
                "draft": draft,
                "custom": custom,
                "history": list(history) if history is not None else None,
                "selection": selection,
                "enabled": enabled,
                "clear_on_submit": clear_on_submit,
            }
        ),
        events=_callback_events(submit=on_submit),
    )


__all__ = [
    "App",
    "AppHost",
    "Border",
    "Button",
    "Callback",
    "ChatArtifactMessage",
    "ChatFileMessage",
    "ChatInputMode",
    "ChatInputPanel",
    "ChatMessageList",
    "ChatTextMessage",
    "ChatToolCallMessage",
    "Checkbox",
    "CommandPalette",
    "Component",
    "ComponentRef",
    "Disclosure",
    "Divider",
    "Event",
    "FileTree",
    "FileTreeNode",
    "Grid",
    "HStack",
    "Label",
    "ListBox",
    "MarkdownViewer",
    "ProgressBar",
    "RadioGroup",
    "Slider",
    "Spacer",
    "Spinner",
    "Splitter",
    "StyledLabel",
    "TabView",
    "TableView",
    "TerminalEmulator",
    "Text",
    "TextArea",
    "TextBox",
    "TypeAhead",
    "VStack",
    "Window",
    "register_all_runtime_components",
]
