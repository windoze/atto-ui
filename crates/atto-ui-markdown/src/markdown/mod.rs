use std::sync::Arc;

use parking_lot::RwLock;
use ratatui::style::Color;

use atto_ui::composable::{ScrollbarDrag, ScrollbarVisibility};
use atto_ui::reactive::{Binding, DirtyObserver};

mod cache;
mod embedded_scrollbar;
mod events;
mod layout;
mod parser;
mod render;
mod styles;
mod text;
mod viewer;

pub use viewer::MarkdownViewer;

#[cfg(test)]
mod tests;

const DEFAULT_CODE_BLOCK_MAX_HEIGHT: u16 = 8;
const DEFAULT_TABLE_MAX_HEIGHT: u16 = 8;
const DEFAULT_SCROLL_STEP: u16 = 3;
const LIST_INDENT_SPACES: usize = 2;

type LinkCallBackType = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone)]
struct LinkCallback(Arc<RwLock<Option<LinkCallBackType>>>);

impl LinkCallback {
    fn new() -> Self {
        Self(Arc::new(RwLock::new(None)))
    }

    fn set(&self, cb: Option<LinkCallBackType>) {
        *self.0.write() = cb;
    }

    fn fire(&self, url: &str) {
        if let Some(cb) = &*self.0.read() {
            cb(url);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmbeddedScrollbarTarget {
    Code(usize),
    Table(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EmbeddedScrollbarDragState {
    target: EmbeddedScrollbarTarget,
    drag: ScrollbarDrag,
}

#[derive(Clone)]
struct MarkdownShared {
    markdown: Binding<String>,
    width: Binding<Option<u16>>,
    show_markers: Binding<bool>,
    vertical_scrollbar: Binding<ScrollbarVisibility>,
    max_code_height: Binding<u16>,
    max_table_height: Binding<u16>,
    fg_override: Binding<Option<Color>>,
    bg_override: Binding<Option<Color>>,
    link_callback: LinkCallback,

    cache: MarkdownCache,
    embedded_scrollbar_drag: Option<EmbeddedScrollbarDragState>,
}

#[derive(Clone)]
struct MarkdownCache {
    md_dirty: DirtyObserver,
    markers_dirty: DirtyObserver,
    max_code_dirty: DirtyObserver,
    max_table_dirty: DirtyObserver,

    parsed: Vec<parser::MdBlock>,
    code_blocks: Vec<embedded_scrollbar::CodeBlockState>,
    tables: Vec<embedded_scrollbar::TableBlockState>,
    layout: Option<layout::Layout>,
    last_wrap_width: Option<u16>,
}
