#![allow(dead_code)]

extern crate self as atto_ui;

use atto_ui_macros::Reactive;

pub mod reactive {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    pub struct Property<T> {
        value: Rc<RefCell<T>>,
        dirty: Rc<Cell<bool>>,
    }

    #[derive(Clone)]
    pub struct Binding<T> {
        value: Rc<RefCell<T>>,
        dirty: Rc<Cell<bool>>,
    }

    impl<T: Clone + PartialEq> Property<T> {
        pub fn new(value: T) -> Self {
            Self {
                value: Rc::new(RefCell::new(value)),
                dirty: Rc::new(Cell::new(false)),
            }
        }

        pub fn get(&self) -> T {
            self.value.borrow().clone()
        }

        pub fn set(&self, value: T) {
            if *self.value.borrow() != value {
                *self.value.borrow_mut() = value;
                self.dirty.set(true);
            }
        }

        pub fn binding(&self) -> Binding<T> {
            Binding {
                value: Rc::clone(&self.value),
                dirty: Rc::clone(&self.dirty),
            }
        }

        pub fn is_dirty(&self) -> bool {
            self.dirty.get()
        }

        pub fn mark_clean(&self) {
            self.dirty.set(false);
        }
    }

    impl<T: Clone + PartialEq> Binding<T> {
        pub fn get(&self) -> T {
            self.value.borrow().clone()
        }

        pub fn set(&self, value: T) {
            if *self.value.borrow() != value {
                *self.value.borrow_mut() = value;
                self.dirty.set(true);
            }
        }
    }
}

use reactive::Property;

#[derive(Reactive)]
struct Model {
    #[reactive]
    title: Property<String>,
    #[reactive]
    count: Property<u64>,
    ignored: u64,
}

fn main() {
    let model = Model {
        title: Property::new("initial".to_string()),
        count: Property::new(1),
        ignored: 99,
    };

    assert_eq!(model.get_title(), "initial");
    model.set_title("updated".to_string());
    assert_eq!(model.get_title(), "updated");
    assert!(model.is_dirty());

    model.mark_clean();
    assert!(!model.is_dirty());

    let count = model.count_binding();
    count.set(7);
    assert_eq!(model.get_count(), 7);
    assert_eq!(count.get(), 7);
}
