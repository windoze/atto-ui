"""Ergonomic Python wrapper for atto-ui."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Dict, List, Optional, Sequence, Tuple, Union

from . import _native

AppHost = _native.AppHost


Callback = Callable[["Event", Optional["ComponentRef"]], None]


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
        self.app._native.apply_tree_ops(self.id, ops)

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
    def __init__(self) -> None:
        self._native = _native.AppHost()
        self._next_callback_id = 1
        self._next_component_id = 1
        self._callbacks: Dict[int, Callback] = {}
        self._windows: List[Window] = []
        self._cid_to_window: Dict[str, Window] = {}

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
        while self.step():
            pass

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


def Button(
    *,
    label: str,
    on_click: Optional[Callback] = None,
    disabled: Optional[bool] = None,
    cid: Optional[str] = None,
) -> Component:
    props: Dict[str, Any] = {"label": label}
    if disabled is not None:
        props["disabled"] = disabled
    events = {"click": on_click} if on_click else {}
    return Component("Button", cid=cid, props=props, events=events)


def Text(text: str, *, cid: Optional[str] = None) -> Component:
    return Component("Text", cid=cid, props={"text": text})


def Label(text: str, *, cid: Optional[str] = None) -> Component:
    return Component("Label", cid=cid, props={"text": text})


def TextBox(
    *,
    title: str,
    text: str = "",
    on_change: Optional[Callback] = None,
    on_submit: Optional[Callback] = None,
    cid: Optional[str] = None,
) -> Component:
    events: Dict[str, Callback] = {}
    if on_change:
        events["change"] = on_change
    if on_submit:
        events["submit"] = on_submit
    props: Dict[str, Any] = {"title": title, "text": text}
    return Component("TextBox", cid=cid, props=props, events=events)


def VStack(
    *,
    children: Sequence[Component],
    spacing: int = 0,
    padding: Optional[Union[int, List[int], Tuple[int, int, int, int], Dict[str, int]]] = None,
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
    padding: Optional[Union[int, List[int], Tuple[int, int, int, int], Dict[str, int]]] = None,
    scrollable: Optional[bool] = None,
    cid: Optional[str] = None,
) -> Component:
    props: Dict[str, Any] = {"spacing": spacing}
    if padding is not None:
        props["padding"] = padding
    if scrollable is not None:
        props["scrollable"] = scrollable
    return Component("HStack", cid=cid, props=props, children=children)


__all__ = [
    "App",
    "AppHost",
    "Button",
    "Component",
    "ComponentRef",
    "Event",
    "HStack",
    "Label",
    "Text",
    "TextBox",
    "VStack",
    "Window",
]
