import atto_ui


def on_click(event: atto_ui.Event, source: atto_ui.ComponentRef):
    if source is None:
        return
    source.window.elements["text1"].set_text("Button Clicked")


def main():
    app = atto_ui.App()

    root = atto_ui.VStack(
        spacing=1,
        children=[
            atto_ui.Button(label="Click me", on_click=on_click, disabled=False),
            atto_ui.Text("Hello World", cid="text1"),
        ],
    )

    app.add_dynamic_window(title="My Window", content=root)
    app.run()


if __name__ == "__main__":
    main()
