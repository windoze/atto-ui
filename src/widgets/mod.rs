//! Reusable widgets that can be embedded inside windows.
//!
//! The initial MVP keeps widgets intentionally simple: a widget is responsible for rendering
//! itself into a given `Rect` and handling input events when focused.

mod button;
mod checkbox;
mod disclosure;
mod label;
mod list;
mod progress_bar;
mod radio;
mod slider;
mod spinner;
mod styled_label;
mod tab_view;
mod table;
mod textarea;
mod textbox;
mod util;

pub use button::Button;
pub use checkbox::Checkbox;
pub use disclosure::{Disclosure, DisclosureStatus};
pub use label::Label;
pub use list::ListBox;
pub use progress_bar::ProgressBar;
pub use radio::RadioGroup;
pub use slider::Slider;
pub use spinner::{FlowDirection, Spinner, SpinnerIconStyle, SpinnerLayout, SpinnerTextEffect};
pub use styled_label::StyledLabel;
pub use tab_view::{TabHeaderPosition, TabView};
pub use table::TableView;
pub use textarea::TextArea;
pub use textbox::TextBox;
