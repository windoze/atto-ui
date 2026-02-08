import time
import atto_ui


def main():
    app = atto_ui.AppHost()

    root = {
        "type": "VStack",
        "id": "root",
        "props": {"spacing": 1},
        "children": [
            {"type": "Label", "id": "title", "props": {"text": "Hello from Python"}},
            {"type": "Button", "id": "ok", "props": {"label": "OK"}},
            {"type": "TextBox", "id": "input", "props": {"title": "Name"}},
        ],
    }

    win_id = app.add_dynamic_window("Python Demo", (2, 2, 50, 14), root)

    app.apply_tree_ops(
        win_id,
        [
            {"op": "bind_event", "id": "ok", "event": "click", "callback": 1},
            {"op": "bind_event", "id": "input", "event": "submit", "callback": 2},
        ],
    )

    while app.step():
        for ev in app.drain_callbacks():
            print("callback", ev)
        time.sleep(0.0)


if __name__ == "__main__":
    main()
