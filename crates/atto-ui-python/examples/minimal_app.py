import atto_ui


def on_click(event: atto_ui.Event, source: atto_ui.ComponentRef):
    if source is None:
        return
    source.window.elements["message"].set_text("Button Clicked")
    source.window.elements["progress"].set_value(1.0)
    source.window.elements["progress"].set_text("Done")


def main():
    app = atto_ui.App(headless=False)
    app.set_theme("dark")

    root = atto_ui.VStack(
        spacing=1,
        children=[
            atto_ui.StyledLabel("Atto UI Python", cid="title"),
            atto_ui.TextArea(title="Prompt", placeholder="Type here...", height=4, cid="prompt"),
            atto_ui.Button(label="Click me", on_click=on_click, disabled=False, cid="button"),
            atto_ui.ProgressBar(value=0.25, show_text=True, text="Ready", cid="progress"),
            atto_ui.Text("Hello World", cid="message"),
        ],
    )

    app.add_dynamic_window(title="My Window", content=root)
    app.run()


if __name__ == "__main__":
    main()
