use chatty::reactive::Property;
use chatty_macros::Reactive;

#[derive(Reactive)]
struct TestViewModel {
    #[reactive]
    text: Property<String>,
    #[reactive]
    count: Property<i32>,
    #[allow(dead_code)]
    cache: Vec<String>,
}

#[test]
fn reactive_macro_getters_setters() {
    let vm = TestViewModel {
        text: Property::new("hello".to_string()),
        count: Property::new(0),
        cache: Vec::new(),
    };

    assert_eq!(vm.get_text(), "hello".to_string());
    assert_eq!(vm.get_count(), 0);

    vm.set_text("world".to_string());
    assert_eq!(vm.get_text(), "world".to_string());
}

#[test]
fn reactive_macro_dirty_tracking() {
    let vm = TestViewModel {
        text: Property::new("hello".to_string()),
        count: Property::new(0),
        cache: Vec::new(),
    };

    vm.mark_clean();
    assert!(!vm.is_dirty());

    vm.set_count(42);
    assert!(vm.is_dirty());
}

#[test]
fn reactive_macro_bindings() {
    let vm = TestViewModel {
        text: Property::new("hello".to_string()),
        count: Property::new(0),
        cache: Vec::new(),
    };

    let binding = vm.text_binding();
    binding.set("bound".to_string());

    assert_eq!(vm.get_text(), "bound".to_string());
}
