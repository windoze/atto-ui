use atto_ui::composable::Component;
use atto_ui_macros::view_builder;

#[test]
fn view_builder_macro_builds_vstack() {
    let view = view_builder! {
        VStack {
            Text("Line 1")
            Text("Line 2")
            Text("Line 3")
        }
        .spacing(1)
        .padding(2)
    };

    assert_eq!(view.children().len(), 3);
}

#[test]
fn view_builder_macro_supports_nesting() {
    let count = 42;

    let view = view_builder! {
        VStack {
            Text(format!("Count: {}", count))
            VStack {
                Text("Nested 1")
                Text("Nested 2")
            }
        }
        .spacing(1)
    };

    assert_eq!(view.children().len(), 2);
}

#[test]
fn view_builder_macro_supports_child_modifiers() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_button = Arc::clone(&calls);
    let calls_for_text = Arc::clone(&calls);

    let view = view_builder! {
        VStack {
            Button("Click").on_click(move || {
                calls_for_button.fetch_add(1, Ordering::SeqCst);
            })
            TextFn(move || format!("calls={}", calls_for_text.load(Ordering::SeqCst)))
        }
        .spacing(1)
    };

    assert_eq!(view.children().len(), 2);
}
