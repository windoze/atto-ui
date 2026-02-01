use ratatui::Frame;
use ratatui::layout::Rect;

use crate::reactive::Binding;
use crate::view::{View, ViewContext};
use crate::views::{EdgeInsets, LayoutParams, ScrollConfig};

use super::grid_view::GridView;
use super::view::{DeclarativeView, EmptyView};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ChildLayout {
    layout: LayoutParams,
}

struct ChildSpec {
    view: Box<dyn DeclarativeView>,
    layout: ChildLayout,
}

/// Grid container (SwiftUI-style `Grid`).
///
/// - Children are placed in row-major order.
/// - Column count is configurable via [`Grid::columns`].
pub struct Grid {
    children: Vec<ChildSpec>,
    columns: Binding<usize>,
    padding: Binding<EdgeInsets>,
    row_gap: Binding<u16>,
    column_gap: Binding<u16>,
    scrollable: Binding<bool>,
    scroll_config: Binding<ScrollConfig>,
}

impl Default for Grid {
    fn default() -> Self {
        Self::new()
    }
}

impl Grid {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            columns: 1usize.into(),
            padding: EdgeInsets::ZERO.into(),
            row_gap: 0u16.into(),
            column_gap: 0u16.into(),
            scrollable: false.into(),
            scroll_config: ScrollConfig::default().into(),
        }
    }

    pub fn columns(mut self, columns: impl Into<Binding<usize>>) -> Self {
        self.columns = columns.into();
        self
    }

    pub fn child(mut self, view: impl DeclarativeView + 'static) -> Self {
        self.children.push(ChildSpec {
            view: Box::new(view),
            layout: ChildLayout::default(),
        });
        self
    }

    pub fn child_with_layout(
        mut self,
        view: impl DeclarativeView + 'static,
        layout: LayoutParams,
    ) -> Self {
        self.children.push(ChildSpec {
            view: Box::new(view),
            layout: ChildLayout { layout },
        });
        self
    }

    pub fn padding(mut self, padding: u16) -> Self {
        self.padding = EdgeInsets::all(padding).into();
        self
    }

    pub fn padding_insets(mut self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.padding = padding.into();
        self
    }

    pub fn row_gap(mut self, gap: impl Into<Binding<u16>>) -> Self {
        self.row_gap = gap.into();
        self
    }

    pub fn column_gap(mut self, gap: impl Into<Binding<u16>>) -> Self {
        self.column_gap = gap.into();
        self
    }

    pub fn scrollable(mut self, scrollable: impl Into<Binding<bool>>) -> Self {
        self.scrollable = scrollable.into();
        self
    }

    pub fn scroll_config(mut self, config: impl Into<Binding<ScrollConfig>>) -> Self {
        self.scroll_config = config.into();
        self
    }
}

impl DeclarativeView for Grid {
    fn body(&self) -> Box<dyn DeclarativeView> {
        Box::new(EmptyView)
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let padding = self.padding.get();
        let cols = self.columns.get().max(1);

        let content_area = apply_padding(area, padding);
        if content_area.width == 0 || content_area.height == 0 {
            return;
        }

        // Minimal render implementation: show one row per child, similar to `VStack`.
        // Real layout behavior is implemented by the imperative view produced in `build_view()`.
        let mut y = content_area.y;
        for (idx, child) in self.children.iter().enumerate() {
            if y >= content_area.y.saturating_add(content_area.height) {
                break;
            }

            let col = idx % cols;
            let x = content_area.x.saturating_add(col as u16);
            let child_area = Rect {
                x,
                y,
                width: 1.min(content_area.width),
                height: 1,
            };
            child.view.render(frame, child_area, ctx);
            if col + 1 == cols {
                y = y.saturating_add(1);
            }
        }
    }

    fn build_view(&self) -> Box<dyn View> {
        let mut grid = GridView::new()
            .with_columns(self.columns.clone())
            .with_padding(self.padding.clone())
            .with_row_gap(self.row_gap.clone())
            .with_column_gap(self.column_gap.clone())
            .with_scrollable(self.scrollable.clone())
            .with_scroll_config(self.scroll_config.clone());

        for child in &self.children {
            grid.add_child_with_layout(child.view.build_view(), child.layout.layout);
        }

        Box::new(grid)
    }
}

fn apply_padding(area: Rect, padding: EdgeInsets) -> Rect {
    let x = area.x.saturating_add(padding.left);
    let y = area.y.saturating_add(padding.top);
    let width = area
        .width
        .saturating_sub(padding.left.saturating_add(padding.right));
    let height = area
        .height
        .saturating_sub(padding.top.saturating_add(padding.bottom));

    Rect {
        x,
        y,
        width,
        height,
    }
}
