mod draw;
mod input;
mod layout;
mod minimized;
mod model;
mod nav;
mod popup;
mod window_ops;

pub use model::{MenuAction, MenuBar, MenuItem, MenuSpec, WindowMenuOp};
pub use popup::{PopupMenu, popup_menu_window};
pub use window_ops::{
    WINDOW_CASCADE_ID, WINDOW_CLOSE_ALL_ID, WINDOW_CLOSE_ID, WINDOW_MAXIMIZE_ID,
    WINDOW_MINIMIZE_ALL_ID, WINDOW_MINIMIZE_ID, WINDOW_NEXT_ID, WINDOW_PREVIOUS_ID,
    WINDOW_RESTORE_ALL_ID, WINDOW_RESTORE_ID, WINDOW_TILE_ID, window_menu_op_from_id,
    window_menu_op_id,
};

pub type MenuCallback = std::sync::Arc<dyn Fn() + Send + Sync>;
