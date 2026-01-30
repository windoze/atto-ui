use ratatui::layout::Rect;

use crate::view::View;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

#[derive(Clone, Debug)]
pub struct WindowDecorations {
    pub border: bool,
    pub shadow: bool,
    pub buttons: WindowButtons,
}

#[derive(Clone, Debug)]
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
            border: true,
            shadow: true,
            buttons: WindowButtons::default(),
        }
    }
}

pub struct Window {
    pub id: WindowId,
    pub kind: WindowKind,
    pub title: String,
    pub rect: Rect,
    pub state: WindowState,
    pub decorations: WindowDecorations,
    pub view: Box<dyn View>,
    pub min_size: (u16, u16),
    pub movable: bool,
    pub resizable: bool,
    pub closable: bool,
    close_hook: Option<WindowCloseHook>,
    pub(crate) restore_rect: Option<Rect>,
}

impl Window {
    pub fn new(
        kind: WindowKind,
        title: impl Into<String>,
        rect: Rect,
        view: Box<dyn View>,
    ) -> Self {
        Self {
            id: WindowId(0),
            kind,
            title: title.into(),
            rect,
            state: WindowState::Normal,
            decorations: WindowDecorations::default(),
            view,
            min_size: (12, 5),
            movable: !kind.is_modal(),
            resizable: !kind.is_modal(),
            closable: true,
            close_hook: None,
            restore_rect: None,
        }
    }

    pub fn with_decorations(mut self, decorations: WindowDecorations) -> Self {
        self.decorations = decorations;
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
        self.min_size = (width, height);
        self
    }

    pub fn inner_rect(&self) -> Rect {
        if !self.decorations.border {
            return self.rect;
        }
        let mut inner = self.rect;
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
        if !self.decorations.border {
            return None;
        }
        if self.rect.height < 1 {
            return None;
        }
        Some(Rect {
            x: self.rect.x,
            y: self.rect.y,
            width: self.rect.width,
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
