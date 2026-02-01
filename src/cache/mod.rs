//! Content caching and incremental rendering primitives.
//!
//! These utilities are not yet integrated into Chatty's main render loop, but provide the
//! building blocks for dirty checking and efficient terminal updates.

mod buffer;
mod diff;
mod scheduler;

pub use buffer::VirtualBuffer;
pub use diff::{DirtyRegion, diff_buffers, dirty_percentage};
pub use scheduler::RenderScheduler;
