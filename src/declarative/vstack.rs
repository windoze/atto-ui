use ratatui::Frame;
use ratatui::layout::Rect;

use crate::view::{View, ViewContext};
use crate::views::{EdgeInsets, VBox};

use super::view::{DeclarativeView, EmptyView};

/// Vertical stack container (SwiftUI-style `VStack`).
pub struct VStack {
    children: Vec<Box<dyn DeclarativeView>>,
    spacing: u16,
    padding: EdgeInsets,
}

impl VStack {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            spacing: 0,
            padding: EdgeInsets::ZERO,
        }
    }

    pub fn child(mut self, view: impl DeclarativeView + 'static) -> Self {
        self.children.push(Box::new(view));
        self
    }

    pub fn spacing(mut self, spacing: u16) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn padding(mut self, padding: u16) -> Self {
        self.padding = EdgeInsets::all(padding);
        self
    }

    pub fn padding_insets(mut self, padding: EdgeInsets) -> Self {
        self.padding = padding;
        self
    }
}

impl Default for VStack {
    fn default() -> Self {
        Self::new()
    }
}

impl DeclarativeView for VStack {
    fn body(&self) -> Box<dyn DeclarativeView> {
        Box::new(EmptyView)
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let content_area = apply_padding(area, self.padding);
        if content_area.width == 0 || content_area.height == 0 {
            return;
        }

        let mut y = content_area.y;
        let bottom = content_area.y.saturating_add(content_area.height);

        for (idx, child) in self.children.iter().enumerate() {
            if y >= bottom {
                break;
            }

            let height_left = bottom.saturating_sub(y);
            let child_area = Rect {
                x: content_area.x,
                y,
                width: content_area.width,
                height: 1.min(height_left),
            };

            child.render(frame, child_area, ctx);

            y = y.saturating_add(child_area.height);
            if idx + 1 < self.children.len() {
                y = y.saturating_add(self.spacing);
            }
        }
    }

    fn build_view(&self) -> Box<dyn View> {
        let mut vbox = VBox::new()
            .with_padding(self.padding)
            .with_spacing(self.spacing);

        for child in &self.children {
            vbox.add_child(child.build_view());
        }

        Box::new(vbox)
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
