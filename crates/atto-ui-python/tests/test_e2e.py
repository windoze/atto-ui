import unittest
from typing import Any, Dict, List, Optional, Tuple

import atto_ui


def find_node(node: Dict[str, Any], cid: str) -> Optional[Dict[str, Any]]:
    if node.get("id") == cid:
        return node
    for child in node.get("children", []):
        found = find_node(child, cid)
        if found is not None:
            return found
    return None


def find_node_bounds(
    node: Dict[str, Any],
    cid: str,
    parent_bounds: Optional[Dict[str, int]] = None,
    parent_kind: Optional[str] = None,
) -> Optional[Tuple[Dict[str, Any], Dict[str, int]]]:
    raw_bounds = node.get("bounds")
    bounds = raw_bounds
    if raw_bounds is not None and parent_bounds is not None and parent_kind == "component":
        bounds = {
            "x": parent_bounds["x"] + raw_bounds["x"],
            "y": parent_bounds["y"] + raw_bounds["y"],
            "width": raw_bounds["width"],
            "height": raw_bounds["height"],
        }
    if node.get("id") == cid:
        return node, bounds
    for child in node.get("children", []):
        found = find_node_bounds(child, cid, bounds, node.get("kind"))
        if found is not None:
            return found
    return None


def window_info(app: atto_ui.App, window: atto_ui.Window) -> Dict[str, Any]:
    for info in app.list_windows():
        if info["id"] == window.id:
            return info
    raise AssertionError(f"missing window {window.id}")


def relative_center(app: atto_ui.App, window: atto_ui.Window, cid: str) -> Tuple[int, int]:
    snapshot = app.snapshot()
    found = find_node_bounds(snapshot["tree"], cid)
    if found is None:
        raise AssertionError(f"missing node {cid}")
    _node, bounds = found
    rect = window_info(app, window)["rect"]
    return (
        bounds["x"] + bounds["width"] // 2 - rect["x"],
        bounds["y"] + bounds["height"] // 2 - rect["y"],
    )


def direct_child_ids(node: Dict[str, Any]) -> List[str]:
    return [child["id"] for child in node.get("children", []) if child.get("id")]


class PythonHostE2ETest(unittest.TestCase):
    def test_native_host_snapshot_without_pty(self) -> None:
        host = atto_ui.AppHost(headless=True, cols=80, rows=24)
        root = {
            "type": "VStack",
            "id": "root",
            "children": [
                {"type": "Label", "id": "title", "props": {"text": "Native"}},
            ],
        }

        window_id = host.add_dynamic_window("Native Window", (2, 2, 32, 8), root)

        self.assertTrue(host.step())
        self.assertEqual(host.list_windows()[0]["id"], window_id)
        snapshot = host.snapshot()
        self.assertEqual(snapshot["bounds"], {"x": 0, "y": 0, "width": 80, "height": 24})
        self.assertEqual(find_node(snapshot["tree"], "title")["text"], "Native")

    def test_app_snapshot_contains_component_tree_and_text(self) -> None:
        app = atto_ui.App(headless=True, cols=90, rows=28)
        app.add_dynamic_window(
            "Snapshot",
            atto_ui.VStack(
                cid="root",
                spacing=1,
                children=[
                    atto_ui.Label("Title", cid="title"),
                    atto_ui.Text("Body", cid="body"),
                ],
            ),
            rect=(3, 3, 40, 10),
        )

        app.step()
        snapshot = app.snapshot()

        self.assertEqual(snapshot["bounds"]["width"], 90)
        self.assertEqual(find_node(snapshot["tree"], "root")["kind"], "component")
        self.assertEqual(find_node(snapshot["tree"], "title")["text"], "Title")
        self.assertEqual(find_node(snapshot["tree"], "body")["text"], "Body")

    def test_send_event_click_dispatches_callback_metadata(self) -> None:
        app = atto_ui.App(headless=True)
        events: List[Tuple[str, str, str]] = []

        def on_click(event: atto_ui.Event, source: atto_ui.ComponentRef) -> None:
            events.append((event.event, event.target_id, source.cid if source else ""))
            source.window.elements["status"].set_text("Clicked")

        window = app.add_dynamic_window(
            "Callbacks",
            atto_ui.VStack(
                cid="root",
                spacing=1,
                children=[
                    atto_ui.Button(label="Click", cid="button", on_click=on_click),
                    atto_ui.Text("Ready", cid="status"),
                ],
            ),
            rect=(2, 2, 38, 10),
        )
        app.step()

        x, y = relative_center(app, window, "button")
        result = app.click(window, x, y)

        self.assertTrue(result["consumed"])
        self.assertEqual(events, [("click", "button", "button")])
        self.assertEqual(find_node(app.snapshot()["tree"], "status")["text"], "Clicked")

    def test_textbox_key_input_submit_and_property_roundtrip(self) -> None:
        app = atto_ui.App(headless=True)
        changes: List[str] = []
        submits: List[str] = []

        window = app.add_dynamic_window(
            "Input",
            atto_ui.VStack(
                cid="root",
                children=[
                    atto_ui.TextBox(
                        title="Prompt",
                        text="",
                        cid="input",
                        on_change=lambda event, _source: changes.append(event.target_id),
                        on_submit=lambda event, _source: submits.append(event.target_id),
                    ),
                ],
            ),
            rect=(2, 2, 42, 8),
        )
        app.step()

        x, y = relative_center(app, window, "input")
        app.click(window, x, y)
        for ch in "abc":
            app.char(window, ch)
        app.key(window, "enter")

        self.assertEqual(app.get_property("input", "text"), "abc")
        self.assertEqual(changes, ["input", "input", "input"])
        self.assertEqual(submits, ["input"])
        app.set_property("input", "text", "reset")
        self.assertEqual(app.get_property("input", "text"), "reset")

    def test_tree_ops_insert_replace_move_remove(self) -> None:
        app = atto_ui.App(headless=True)
        window = app.add_dynamic_window(
            "Tree Ops",
            atto_ui.VStack(
                cid="root",
                children=[atto_ui.Text("A", cid="a"), atto_ui.Text("B", cid="b")],
            ),
            rect=(2, 2, 42, 10),
        )

        window._apply_tree_ops(
            [
                {
                    "op": "insert",
                    "parent_id": "root",
                    "index": 1,
                    "child": atto_ui.Text("Inserted", cid="inserted").to_dict(),
                }
            ]
        )
        self.assertEqual(find_node(app.snapshot()["tree"], "inserted")["text"], "Inserted")

        window._apply_tree_ops(
            [
                {
                    "op": "replace",
                    "id": "inserted",
                    "node": atto_ui.Text("Replaced", cid="replaced").to_dict(),
                }
            ]
        )
        self.assertIsNone(find_node(app.snapshot()["tree"], "inserted"))
        self.assertEqual(find_node(app.snapshot()["tree"], "replaced")["text"], "Replaced")

        window._apply_tree_ops([{"op": "move", "id": "b", "new_parent_id": "root", "index": 0}])
        root = find_node(app.snapshot()["tree"], "root")
        self.assertEqual(direct_child_ids(root), ["b", "a", "replaced"])

        window._apply_tree_ops([{"op": "remove", "id": "replaced"}])
        self.assertIsNone(find_node(app.snapshot()["tree"], "replaced"))

    def test_component_ref_callback_can_mutate_source_window(self) -> None:
        app = atto_ui.App(headless=True)

        def on_click(_event: atto_ui.Event, source: atto_ui.ComponentRef) -> None:
            source.set_label("Done")
            source.window.elements["message"].set_text("Callback OK")

        window = app.add_dynamic_window(
            "Roundtrip",
            atto_ui.VStack(
                cid="root",
                spacing=1,
                children=[
                    atto_ui.Button(label="Start", cid="button", on_click=on_click),
                    atto_ui.Text("Waiting", cid="message"),
                ],
            ),
            rect=(2, 2, 40, 10),
        )
        app.step()

        x, y = relative_center(app, window, "button")
        window.click(x, y)
        snapshot = app.snapshot()

        self.assertEqual(find_node(snapshot["tree"], "button")["text"], "Done")
        self.assertEqual(find_node(snapshot["tree"], "message")["text"], "Callback OK")

    def test_window_management_methods(self) -> None:
        app = atto_ui.App(headless=True, cols=100, rows=35)
        first = app.add_dynamic_window("First", atto_ui.Text("One", cid="one"), rect=(2, 2, 24, 8))
        second = app.add_dynamic_window("Second", atto_ui.Text("Two", cid="two"), rect=(30, 2, 24, 8))

        self.assertTrue(app.focus_window(first))
        self.assertTrue(app.move_window(first, 5, 6))
        self.assertTrue(app.resize_window(first, 32, 12))
        self.assertTrue(app.set_title(first, "Renamed"))

        info = window_info(app, first)
        self.assertTrue(info["is_focused"])
        self.assertEqual(info["title"], "Renamed")
        self.assertEqual(info["rect"], {"x": 5, "y": 6, "width": 32, "height": 12})

        self.assertTrue(second.close())
        self.assertEqual([w["id"] for w in app.list_windows()], [first.id])

    def test_multi_window_event_routing(self) -> None:
        app = atto_ui.App(headless=True)
        clicked: List[str] = []

        first = app.add_dynamic_window(
            "First",
            atto_ui.Button(label="One", cid="one", on_click=lambda _event, _source: clicked.append("one")),
            rect=(2, 2, 24, 8),
        )
        second = app.add_dynamic_window(
            "Second",
            atto_ui.Button(label="Two", cid="two", on_click=lambda _event, _source: clicked.append("two")),
            rect=(30, 2, 24, 8),
        )
        app.step()

        x2, y2 = relative_center(app, second, "two")
        second.click(x2, y2)
        x1, y1 = relative_center(app, first, "one")
        first.click(x1, y1)

        self.assertEqual(clicked, ["two", "one"])


if __name__ == "__main__":
    unittest.main()
