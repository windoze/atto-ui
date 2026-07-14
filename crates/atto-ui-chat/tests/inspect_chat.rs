use atto_ui::app::{Desktop, MenuBar};
use atto_ui::runtime::ComponentValue;
use atto_ui::theme::Theme;
use atto_ui::{Window, WindowKind};
use atto_ui_chat::{ChatInputHandle, ChatInputMode, ChatMessageList, ChatMessageStore, ChatPanel};
use ratatui::layout::Rect;

const CHAT_INPUT_TAG: &str = "chat-input";

fn chat_desktop(handle: &ChatInputHandle, screen: Rect) -> Desktop {
    let list = ChatMessageList::new(ChatMessageStore::new());
    let input = handle.panel().with_tag(CHAT_INPUT_TAG);
    let panel = ChatPanel::new(list, input);

    let mut desktop = Desktop::new(Theme::dark(), MenuBar::new(vec![]));
    desktop.add_window(
        Window::new(
            WindowKind::Normal,
            "Chat",
            Rect::new(2, 2, 64, 18),
            Box::new(panel),
        )
        .with_tag("chat-window"),
        screen,
    );
    desktop
}

fn mode_type(value: ComponentValue) -> String {
    let ComponentValue::Map(fields) = value else {
        panic!("expected chat input mode map");
    };
    let Some(ComponentValue::String(kind)) = fields.get("type") else {
        panic!("expected chat input mode type string in {fields:?}");
    };
    kind.clone()
}

#[test]
fn chat_input_mode_state_is_readable_through_desktop_inspector() {
    let screen = Rect::new(0, 0, 80, 24);
    let handle = ChatInputHandle::new();
    let mut desktop = chat_desktop(&handle, screen);

    let mut inspector = desktop.inspect();
    let tree = inspector.tree(screen).expect("inspect tree");
    assert!(
        tree.find_by_id(CHAT_INPUT_TAG).is_some(),
        "tagged chat input should be discoverable through ChatPanel"
    );

    let names = inspector
        .property_names(CHAT_INPUT_TAG)
        .expect("chat input property names");
    assert!(names.iter().any(|name| name == "mode"));
    assert!(names.iter().any(|name| name == "draft"));
    assert_eq!(
        mode_type(
            inspector
                .get_property(CHAT_INPUT_TAG, "mode")
                .expect("initial chat input mode")
        ),
        "text"
    );

    handle.set_mode(ChatInputMode::choice(
        "请选择一种回应方式",
        vec!["简短回复".to_string(), "详细解释".to_string()],
    ));

    assert_eq!(
        mode_type(
            inspector
                .get_property(CHAT_INPUT_TAG, "mode")
                .expect("updated chat input mode")
        ),
        "choice"
    );
}
