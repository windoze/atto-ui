use std::collections::BTreeMap;
use std::time::Duration;

use atto_ui::app::{Desktop, MenuBar};
use atto_ui::theme::Theme;
use atto_ui::{
    ComponentCommand, ComponentTarget, ComponentValue, InvokeDispatch, WaitCondition, Window,
    WindowKind,
};
use atto_ui_chat::{
    ChatBlock, ChatChoiceInputConfig, ChatConfirmInputConfig, ChatInputHandle, ChatInputMode,
    ChatInputResponse, ChatMessage, ChatMessageList, ChatMessageStore, ChatPanel, ChatRole,
};
use ratatui::layout::Rect;

const CHAT_INPUT_TAG: &str = "chat-input";

fn chat_desktop(handle: &ChatInputHandle, screen: Rect) -> Desktop {
    chat_desktop_with_store(handle, ChatMessageStore::new(), screen)
}

fn chat_desktop_with_store(
    handle: &ChatInputHandle,
    store: ChatMessageStore,
    screen: Rect,
) -> Desktop {
    let list = ChatMessageList::new(store.clone());
    let input = handle.panel().with_tag(CHAT_INPUT_TAG).on_submit({
        let store = store.clone();
        move |response| {
            store.push(ChatMessage::text(
                store.next_message_id(),
                ChatRole::System,
                submit_response_text(response),
            ));
        }
    });
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

fn submit_response_text(response: ChatInputResponse) -> String {
    match response {
        ChatInputResponse::Text(text) => format!("SUBMIT: text={text}"),
        ChatInputResponse::Choice { index, label } => {
            format!("SUBMIT: choice index={index} label={label}")
        }
        ChatInputResponse::Custom(text) => format!("SUBMIT: custom={text}"),
    }
}

fn target() -> ComponentTarget {
    ComponentTarget::Id(CHAT_INPUT_TAG.to_string())
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

fn choice_mode_value(prompt: &str, options: &[&str]) -> ComponentValue {
    let mut fields = BTreeMap::new();
    fields.insert(
        "type".to_string(),
        ComponentValue::String("choice".to_string()),
    );
    fields.insert(
        "prompt".to_string(),
        ComponentValue::String(prompt.to_string()),
    );
    fields.insert(
        "options".to_string(),
        ComponentValue::StringList(options.iter().map(|item| item.to_string()).collect()),
    );
    fields.insert("allow_custom".to_string(), ComponentValue::Bool(false));
    fields.insert(
        "submit_label".to_string(),
        ComponentValue::String("Submit".to_string()),
    );
    ComponentValue::Map(fields)
}

fn confirm_mode_value(prompt: &str, yes: &str, no: &str) -> ComponentValue {
    let mut fields = BTreeMap::new();
    fields.insert(
        "type".to_string(),
        ComponentValue::String("confirm".to_string()),
    );
    fields.insert(
        "prompt".to_string(),
        ComponentValue::String(prompt.to_string()),
    );
    fields.insert(
        "yes_label".to_string(),
        ComponentValue::String(yes.to_string()),
    );
    fields.insert(
        "no_label".to_string(),
        ComponentValue::String(no.to_string()),
    );
    fields.insert("allow_custom".to_string(), ComponentValue::Bool(false));
    ComponentValue::Map(fields)
}

fn text_messages(store: &ChatMessageStore) -> Vec<String> {
    store
        .messages()
        .into_iter()
        .flat_map(|message| message.blocks)
        .filter_map(|block| match block {
            ChatBlock::Text(text) => Some(text.markdown),
            _ => None,
        })
        .collect()
}

fn store_contains_text(store: &ChatMessageStore, needle: &str) -> bool {
    text_messages(store).iter().any(|text| text == needle)
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

#[test]
fn chat_text_submit_uses_invoke_and_wait_for_read_values() {
    let screen = Rect::new(0, 0, 80, 24);
    let handle = ChatInputHandle::new();
    let store = ChatMessageStore::new();
    let mut desktop = chat_desktop_with_store(&handle, store.clone(), screen);
    let mut inspector = desktop.inspect();

    let input = inspector
        .invoke(
            screen,
            target(),
            ComponentCommand::InputText("hello".to_string()),
        )
        .expect("invoke input text");
    assert_eq!(input.dispatch, InvokeDispatch::Semantic);
    assert_eq!(
        inspector
            .wait_for(
                screen,
                WaitCondition::property_equals(
                    target(),
                    "draft",
                    ComponentValue::String("hello".to_string()),
                ),
                Duration::from_millis(250),
            )
            .expect("wait for draft")
            .value,
        Some(ComponentValue::String("hello".to_string()))
    );

    let submit = inspector
        .invoke(screen, target(), ComponentCommand::Submit)
        .expect("invoke submit");
    assert_eq!(submit.dispatch, InvokeDispatch::Semantic);
    assert_eq!(
        inspector
            .wait_for(
                screen,
                WaitCondition::property_equals(
                    target(),
                    "draft",
                    ComponentValue::String(String::new()),
                ),
                Duration::from_millis(250),
            )
            .expect("wait for cleared draft")
            .value,
        Some(ComponentValue::String(String::new()))
    );
    inspector
        .wait_for_predicate(screen, Duration::from_millis(250), |_| {
            Ok(store_contains_text(&store, "SUBMIT: text=hello"))
        })
        .expect("wait for submitted text message");
}

#[test]
fn chat_choice_and_confirm_submit_use_invoke_without_pty_coordinates() {
    let screen = Rect::new(0, 0, 80, 24);
    let handle = ChatInputHandle::new();
    let store = ChatMessageStore::new();
    let mut desktop = chat_desktop_with_store(&handle, store.clone(), screen);
    let mut inspector = desktop.inspect();

    handle.set_mode(ChatInputMode::Choice(ChatChoiceInputConfig::new(
        "请选择一种回应方式",
        vec!["简短回复".to_string(), "详细解释".to_string()],
    )));
    inspector
        .wait_for(
            screen,
            WaitCondition::property_equals(
                target(),
                "mode",
                choice_mode_value("请选择一种回应方式", &["简短回复", "详细解释"]),
            ),
            Duration::from_millis(250),
        )
        .expect("wait for choice mode");

    let select = inspector
        .invoke(screen, target(), ComponentCommand::SelectIndex(1))
        .expect("invoke choice selection");
    assert_eq!(select.dispatch, InvokeDispatch::Semantic);
    inspector
        .wait_for(
            screen,
            WaitCondition::property_equals(target(), "selection", ComponentValue::U64(1)),
            Duration::from_millis(250),
        )
        .expect("wait for selected choice");
    let choice_submit = inspector
        .invoke(screen, target(), ComponentCommand::Submit)
        .expect("invoke choice submit");
    assert_eq!(choice_submit.dispatch, InvokeDispatch::Semantic);
    inspector
        .wait_for_predicate(screen, Duration::from_millis(250), |_| {
            Ok(store_contains_text(
                &store,
                "SUBMIT: choice index=1 label=详细解释",
            ))
        })
        .expect("wait for choice submit message");

    handle.set_mode(ChatInputMode::Confirm(
        ChatConfirmInputConfig::new("是否继续执行?")
            .yes_label("继续")
            .no_label("停止"),
    ));
    inspector
        .wait_for(
            screen,
            WaitCondition::property_equals(
                target(),
                "mode",
                confirm_mode_value("是否继续执行?", "继续", "停止"),
            ),
            Duration::from_millis(250),
        )
        .expect("wait for confirm mode");
    inspector
        .invoke(screen, target(), ComponentCommand::SelectIndex(usize::MAX))
        .expect("invoke confirm selection");
    inspector
        .wait_for(
            screen,
            WaitCondition::property_equals(target(), "selection", ComponentValue::U64(1)),
            Duration::from_millis(250),
        )
        .expect("wait for clamped confirm selection");
    inspector
        .invoke(screen, target(), ComponentCommand::Submit)
        .expect("invoke confirm submit");
    inspector
        .wait_for_predicate(screen, Duration::from_millis(250), |_| {
            Ok(store_contains_text(
                &store,
                "SUBMIT: choice index=1 label=停止",
            ))
        })
        .expect("wait for confirm submit message");
}

#[test]
fn chat_streaming_queue_logic_uses_invoke_and_wait_for() {
    let screen = Rect::new(0, 0, 80, 24);
    let handle = ChatInputHandle::new();
    handle.streaming_binding().set(true);
    let store = ChatMessageStore::new();
    let mut desktop = chat_desktop_with_store(&handle, store.clone(), screen);
    let mut inspector = desktop.inspect();

    inspector
        .invoke(
            screen,
            target(),
            ComponentCommand::InputText("queued one".to_string()),
        )
        .expect("invoke queued text");
    inspector
        .invoke(screen, target(), ComponentCommand::Submit)
        .expect("invoke queue submit");
    inspector
        .wait_for(
            screen,
            WaitCondition::property_equals(
                target(),
                "draft",
                ComponentValue::String(String::new()),
            ),
            Duration::from_millis(250),
        )
        .expect("wait for queued draft clear");
    assert!(
        text_messages(&store).is_empty(),
        "streaming submit should queue without dispatching immediately"
    );

    handle.streaming_binding().set(false);
    inspector
        .invoke(screen, target(), ComponentCommand::Submit)
        .expect("invoke queued response submit");
    inspector
        .wait_for_predicate(screen, Duration::from_millis(250), |_| {
            Ok(store_contains_text(&store, "SUBMIT: text=queued one"))
        })
        .expect("wait for queued submit message");
}
