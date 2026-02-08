use ratatui::layout::Rect;

use super::min_size_view::WindowMinSizeView;
use crate::composable::Component;
use crate::reactive::Binding;
use crate::runtime::ComponentTree;
use crate::{CallbackRegistry, ComponentSpec, TreeError, TreeOp};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowId(pub(crate) u64);

pub type WindowCloseHook = Box<dyn FnMut(WindowId) -> bool + Send>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    Normal,
    Floating,
    Modal,
    Tooltip,
}

impl WindowKind {
    pub fn is_focusable(self) -> bool {
        !matches!(self, Self::Tooltip)
    }

    pub fn is_modal(self) -> bool {
        matches!(self, Self::Modal)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowState {
    Normal,
    Minimized,
    Maximized,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowBorderStyle {
    #[default]
    Normal,
    /// Always use a single-line border set, even when the window is focused.
    Thin,
    /// No window chrome (no border, no titlebar).
    Borderless,
}

impl WindowBorderStyle {
    pub fn has_border(self) -> bool {
        !matches!(self, Self::Borderless)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowMinSizeMode {
    /// Enforce the window's minimum size constraint during resize (default behavior).
    #[default]
    Enforce,
    /// Allow resizing below the minimum size; content is clipped.
    Clip,
    /// Allow resizing below the minimum size; content can be accessed via scrollbars.
    Scroll,
}

impl WindowMinSizeMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "Enforce" | "enforce" => Some(Self::Enforce),
            "Clip" | "clip" => Some(Self::Clip),
            "Scroll" | "scroll" => Some(Self::Scroll),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowDecorations {
    pub border: WindowBorderStyle,
    pub shadow: bool,
    pub buttons: WindowButtons,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowButtons {
    pub minimize: bool,
    pub maximize: bool,
    pub close: bool,
}

impl Default for WindowButtons {
    fn default() -> Self {
        Self {
            minimize: true,
            maximize: true,
            close: true,
        }
    }
}

impl Default for WindowDecorations {
    fn default() -> Self {
        Self {
            border: WindowBorderStyle::Normal,
            shadow: true,
            buttons: WindowButtons::default(),
        }
    }
}

pub struct Window {
    pub id: WindowId,
    pub tag: Option<String>,
    pub kind: WindowKind,
    pub title: Binding<String>,
    pub rect: Binding<Rect>,
    pub state: Binding<WindowState>,
    pub decorations: Binding<WindowDecorations>,
    pub view: Box<dyn Component>,
    pub min_size: Binding<(u16, u16)>,
    pub min_size_mode: Binding<WindowMinSizeMode>,
    pub movable: Binding<bool>,
    pub resizable: Binding<bool>,
    pub closable: Binding<bool>,
    close_hook: Option<WindowCloseHook>,
    pub(crate) restore_rect: Option<Rect>,
}

impl Window {
    pub fn new(
        kind: WindowKind,
        title: impl Into<Binding<String>>,
        rect: impl Into<Binding<Rect>>,
        view: Box<dyn Component>,
    ) -> Self {
        let min_size_mode: Binding<WindowMinSizeMode> = WindowMinSizeMode::Enforce.into();
        let view = Box::new(WindowMinSizeView::new(view, min_size_mode.clone()));

        Self {
            id: WindowId(0),
            tag: None,
            kind,
            title: title.into(),
            rect: rect.into(),
            state: WindowState::Normal.into(),
            decorations: WindowDecorations::default().into(),
            view,
            min_size: (12, 5).into(),
            min_size_mode,
            movable: (!matches!(kind, WindowKind::Modal | WindowKind::Tooltip)).into(),
            resizable: (!matches!(kind, WindowKind::Tooltip)).into(),
            closable: true.into(),
            close_hook: None,
            restore_rect: None,
        }
    }

    pub fn new_dynamic(
        kind: WindowKind,
        title: impl Into<Binding<String>>,
        rect: impl Into<Binding<Rect>>,
        root: ComponentSpec,
        callbacks: CallbackRegistry,
    ) -> Result<Self, TreeError> {
        let tree = ComponentTree::new(root, callbacks)?;
        Ok(Self::new(kind, title, rect, Box::new(tree)))
    }

    pub fn with_tag(mut self, id: impl Into<String>) -> Self {
        self.tag = Some(id.into());
        self
    }

    pub fn tag(&self) -> Option<&str> {
        self.tag.as_deref()
    }

    pub fn with_decorations(mut self, decorations: impl Into<Binding<WindowDecorations>>) -> Self {
        self.decorations = decorations.into();
        self
    }

    pub fn with_close_hook<F>(mut self, hook: F) -> Self
    where
        F: FnMut(WindowId) -> bool + Send + 'static,
    {
        self.close_hook = Some(Box::new(hook));
        self
    }

    pub fn with_min_size(mut self, width: u16, height: u16) -> Self {
        self.min_size = (width, height).into();
        self
    }

    pub fn with_min_size_mode(self, mode: WindowMinSizeMode) -> Self {
        self.min_size_mode.set(mode);
        self
    }

    pub fn set_view(&mut self, view: Box<dyn Component>) {
        let min_size_mode = self.min_size_mode.clone();
        self.view = Box::new(WindowMinSizeView::new(view, min_size_mode));
    }

    pub fn apply_tree_ops(&mut self, ops: &[TreeOp]) -> Result<bool, TreeError> {
        self.view.apply_tree_ops(ops)
    }

    pub fn rebuild_dynamic(&mut self) -> Result<(), TreeError> {
        self.view.rebuild_tree()
    }

    pub fn dynamic_root_spec(&self) -> Option<&ComponentSpec> {
        self.view.dynamic_root_spec()
    }

    pub fn dynamic_callbacks(&self) -> Option<&CallbackRegistry> {
        self.view.dynamic_callbacks()
    }

    pub fn inner_rect(&self) -> Rect {
        let decorations = self.decorations.get();
        let rect = self.rect.get();
        if !decorations.border.has_border() {
            return rect;
        }
        let mut inner = rect;
        if inner.width >= 2 {
            inner.x += 1;
            inner.width -= 2;
        } else {
            inner.width = 0;
        }
        if inner.height >= 2 {
            inner.y += 1;
            inner.height -= 2;
        } else {
            inner.height = 0;
        }
        inner
    }

    pub fn titlebar_rect(&self) -> Option<Rect> {
        let decorations = self.decorations.get();
        let rect = self.rect.get();
        if !decorations.border.has_border() {
            return None;
        }
        if rect.height < 1 {
            return None;
        }
        Some(Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: 1,
        })
    }

    pub(crate) fn allow_close(&mut self) -> bool {
        match self.close_hook.as_mut() {
            Some(hook) => hook(self.id),
            None => true,
        }
    }
}
