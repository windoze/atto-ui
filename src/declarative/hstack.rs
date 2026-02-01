use ratatui::Frame;
use ratatui::layout::Rect;

use crate::reactive::Binding;
use crate::view::{View, ViewContext};
use crate::views::{EdgeInsets, LayoutParams, ScrollConfig};

use super::stack_view::HStackView;
use super::view::{DeclarativeView, EmptyView};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ChildLayout {
    layout: LayoutParams,
}

struct ChildSpec {
    view: Box<dyn DeclarativeView>,
    layout: ChildLayout,
}

/// Horizontal stack container (SwiftUI-style `HStack`).
pub struct HStack {
    children: Vec<ChildSpec>,
    spacing: Binding<u16>,
    padding: Binding<EdgeInsets>,
    scrollable: Binding<bool>,
    scroll_config: Binding<ScrollConfig>,
}

impl Default for HStack {
    fn default() -> Self {
        Self::new()
    }
}

impl HStack {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            spacing: 0u16.into(),
            padding: EdgeInsets::ZERO.into(),
            scrollable: false.into(),
            scroll_config: ScrollConfig::default().into(),
        }
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

    pub fn spacing(mut self, spacing: impl Into<Binding<u16>>) -> Self {
        self.spacing = spacing.into();
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

    pub fn scrollable(mut self, scrollable: impl Into<Binding<bool>>) -> Self {
        self.scrollable = scrollable.into();
        self
    }

    pub fn scroll_config(mut self, config: impl Into<Binding<ScrollConfig>>) -> Self {
        self.scroll_config = config.into();
        self
    }
}

impl DeclarativeView for HStack {
    fn body(&self) -> Box<dyn DeclarativeView> {
        Box::new(EmptyView)
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let padding = self.padding.get();
        let spacing = self.spacing.get();

        let content_area = apply_padding(area, padding);
        if content_area.width == 0 || content_area.height == 0 {
            return;
        }

        let mut x = content_area.x;
        let right = content_area.x.saturating_add(content_area.width);

        for (idx, child) in self.children.iter().enumerate() {
            if x >= right {
                break;
            }

            let width_left = right.saturating_sub(x);
            let child_area = Rect {
                x,
                y: content_area.y,
                width: 1.min(width_left),
                height: content_area.height,
            };

            child.view.render(frame, child_area, ctx);

            x = x.saturating_add(child_area.width);
            if idx + 1 < self.children.len() {
                x = x.saturating_add(spacing);
            }
        }
    }

    fn build_view(&self) -> Box<dyn View> {
        let mut hstack = HStackView::new()
            .with_padding(self.padding.clone())
            .with_spacing(self.spacing.clone())
            .with_scrollable(self.scrollable.clone())
            .with_scroll_config(self.scroll_config.clone());

        for child in &self.children {
            hstack.add_child_with_layout(child.view.build_view(), child.layout.layout);
        }

        Box::new(hstack)
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
