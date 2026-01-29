//! Reusable widgets that can be embedded inside windows.
//!
//! The initial MVP keeps widgets intentionally simple: a widget is responsible for rendering
//! itself into a given `Rect` and handling input events when focused.

mod primitives;
mod button;
mod checkbox;
mod label;
mod list;
mod radio;
mod table;
mod textbox;

pub use primitives::{Control, ControlOutcome, Form, FormAction};
pub use button::Button;
pub use checkbox::Checkbox;
pub use label::Label;
pub use list::ListBox;
pub use radio::RadioGroup;
pub use table::TableView;
pub use textbox::TextBox;
