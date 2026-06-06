use atto_ui::composable::DynamicTree;
use atto_ui_macros::view_builder;

mod fake_atto {
    pub mod composable {
        #[derive(Debug)]
        pub struct Text(pub String);

        #[derive(Debug, Default)]
        pub struct VStack {
            children: Vec<Text>,
        }

        impl Text {
            pub fn new(text: impl Into<String>) -> Self {
                Self(text.into())
            }
        }

        impl VStack {
            pub fn new() -> Self {
                Self::default()
            }

            pub fn child(mut self, child: Text) -> Self {
                self.children.push(child);
                self
            }

            pub fn children(&self) -> &[Text] {
                &self.children
            }
        }
    }
}

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
fn view_builder_macro_supports_explicit_crate_path() {
    let view = view_builder! {
        crate_path = ::atto_ui;
        VStack {
            Text("Line 1")
            Text("Line 2")
        }
    };

    assert_eq!(view.children().len(), 2);
}

#[test]
fn view_builder_macro_uses_explicit_crate_path() {
    let view = view_builder! {
        crate_path = crate::fake_atto;
        VStack {
            Text("Line 1")
        }
    };

    assert_eq!(view.children()[0].0, "Line 1");
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
