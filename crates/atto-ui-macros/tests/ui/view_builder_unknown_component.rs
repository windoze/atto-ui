#![allow(dead_code)]

extern crate self as atto_ui;

use atto_ui_macros::view_builder;

pub mod composable {
    pub struct Text;
    pub struct VStack;

    impl Text {
        pub fn new(_value: impl Into<String>) -> Self {
            Self
        }
    }

    impl VStack {
        pub fn new() -> Self {
            Self
        }

        pub fn child<T>(self, _child: T) -> Self {
            self
        }
    }
}

fn main() {
    let _view = view_builder! {
        VStack {
            UnknownWidget("boom")
        }
    };
}
