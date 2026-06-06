#![allow(dead_code)]

extern crate self as atto_ui;

use atto_ui_macros::Reactive;

pub mod reactive {
    #[derive(Clone)]
    pub struct Binding<T>(T);

    pub struct Property<T>(T);

    impl<T: Clone + PartialEq> Property<T> {
        pub fn get(&self) -> T {
            self.0.clone()
        }

        pub fn set(&self, _value: T) {}

        pub fn binding(&self) -> Binding<T> {
            Binding(self.0.clone())
        }

        pub fn is_dirty(&self) -> bool {
            false
        }

        pub fn mark_clean(&self) {}
    }
}

#[derive(Reactive)]
struct Model {
    #[reactive]
    count: u64,
}

fn main() {}
