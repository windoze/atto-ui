use chatty::declarative::DeclarativeView;
use chatty_macros::view_builder;

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

    assert!(!view.is_primitive());
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

    assert!(!view.is_primitive());
}
