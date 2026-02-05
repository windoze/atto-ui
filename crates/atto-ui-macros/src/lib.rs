//! Procedural macros for Chatty.
//!
//! This crate is intentionally small: it provides optional ergonomics on top of Chatty's
//! composable component API without being required for the core library to function.

use proc_macro::TokenStream;

mod reactive;
mod view_builder;

/// Generates convenience accessors around `atto_ui::reactive::Property<_>` fields.
///
/// Notes:
/// - Due to Rust's macro system, this derive does **not** transform field types. Fields marked
///   with `#[reactive]` must already be `Property<T>`.
#[proc_macro_derive(Reactive, attributes(reactive))]
pub fn derive_reactive(input: TokenStream) -> TokenStream {
    reactive::derive_reactive_impl(input)
}

/// SwiftUI-ish composable builder DSL.
///
/// Currently supports a `Container { Child(args...) Child2(args...) }` form with optional chained
/// modifiers on both the root container and any child node:
///
/// ```ignore
/// view_builder! {
///   VStack {
///     Text("Hello")
///     Text("World").fg(Color::Green)
///     Button("OK").on_click(|| println!("clicked"))
///   }
///   .spacing(1)
///   .padding(2)
/// }
/// ```
#[proc_macro]
pub fn view_builder(input: TokenStream) -> TokenStream {
    view_builder::view_builder_impl(input)
}
