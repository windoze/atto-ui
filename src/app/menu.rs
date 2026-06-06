mod draw;
mod input;
mod layout;
mod minimized;
mod model;

pub use model::{MenuAction, MenuBar, MenuItem, MenuSpec};

pub type MenuCallback = std::sync::Arc<dyn Fn() + Send + Sync>;
