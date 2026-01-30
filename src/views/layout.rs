use ratatui::layout::Rect;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EdgeInsets {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl EdgeInsets {
    pub const ZERO: Self = Self {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };

    pub const fn all(v: u16) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    pub const fn symmetric(vertical: u16, horizontal: u16) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    pub const fn horizontal(v: u16) -> Self {
        Self {
            top: 0,
            right: v,
            bottom: 0,
            left: v,
        }
    }

    pub const fn vertical(v: u16) -> Self {
        Self {
            top: v,
            right: 0,
            bottom: v,
            left: 0,
        }
    }

    pub const fn sum_horizontal(self) -> u16 {
        self.left.saturating_add(self.right)
    }

    pub const fn sum_vertical(self) -> u16 {
        self.top.saturating_add(self.bottom)
    }
}

pub fn apply_padding(area: Rect, padding: EdgeInsets) -> Rect {
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

pub fn apply_padding_local(size: (u16, u16), padding: EdgeInsets) -> (u16, u16) {
    let (w, h) = size;
    let w = w.saturating_sub(padding.left.saturating_add(padding.right));
    let h = h.saturating_sub(padding.top.saturating_add(padding.bottom));
    (w, h)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[derive(Default)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[derive(Default)]
pub enum Size {
    #[default]
    Fill,
    Fixed(u16),
    Weight(u16),
    Content,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[derive(Default)]
pub enum Anchor {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Top,
    Bottom,
    Left,
    Right,
    Center,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnchorPlacement {
    pub anchor: Anchor,
    pub offset_x: i16,
    pub offset_y: i16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutParams {
    pub width: Size,
    pub height: Size,
    pub margin: EdgeInsets,
    pub align_x: Align,
    pub align_y: Align,
    pub anchor: Option<AnchorPlacement>,
}

pub fn add_signed(v: u16, dv: i16) -> u16 {
    if dv.is_negative() {
        v.saturating_sub(dv.wrapping_abs() as u16)
    } else {
        v.saturating_add(dv as u16)
    }
}
