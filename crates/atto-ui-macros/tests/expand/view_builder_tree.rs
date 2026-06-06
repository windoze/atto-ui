#![allow(dead_code)]

extern crate self as atto_ui;

use atto_ui_macros::view_builder;

pub mod composable {
    #[derive(Debug)]
    pub enum Node {
        Text(Text),
        VStack(VStack),
    }

    #[derive(Debug)]
    pub struct Text {
        pub value: String,
        pub color: Option<&'static str>,
    }

    #[derive(Debug, Default)]
    pub struct VStack {
        pub children: Vec<Node>,
        pub spacing: u16,
    }

    impl Text {
        pub fn new(value: impl Into<String>) -> Self {
            Self {
                value: value.into(),
                color: None,
            }
        }

        pub fn fg(mut self, color: &'static str) -> Self {
            self.color = Some(color);
            self
        }
    }

    impl From<Text> for Node {
        fn from(value: Text) -> Self {
            Node::Text(value)
        }
    }

    impl From<VStack> for Node {
        fn from(value: VStack) -> Self {
            Node::VStack(value)
        }
    }

    impl VStack {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn child(mut self, child: impl Into<Node>) -> Self {
            self.children.push(child.into());
            self
        }

        pub fn spacing(mut self, spacing: u16) -> Self {
            self.spacing = spacing;
            self
        }
    }
}

fn main() {
    let view = view_builder! {
        VStack {
            Text("hello").fg("green")
            VStack {
                Text(format!("nested {}", 1))
            }.spacing(1)
        }.spacing(2)
    };

    assert_eq!(view.spacing, 2);
    assert_eq!(view.children.len(), 2);

    match &view.children[0] {
        composable::Node::Text(text) => {
            assert_eq!(text.value, "hello");
            assert_eq!(text.color, Some("green"));
        }
        _ => panic!("first child should be text"),
    }
}
