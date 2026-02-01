use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

/// Virtual buffer for double-buffered rendering.
pub struct VirtualBuffer {
    previous: Buffer,
    current: Buffer,
}

impl VirtualBuffer {
    pub fn new(area: Rect) -> Self {
        Self {
            previous: Buffer::empty(area),
            current: Buffer::empty(area),
        }
    }

    pub fn current_mut(&mut self) -> &mut Buffer {
        &mut self.current
    }

    pub fn current(&self) -> &Buffer {
        &self.current
    }

    pub fn previous(&self) -> &Buffer {
        &self.previous
    }

    pub fn swap(&mut self) {
        std::mem::swap(&mut self.previous, &mut self.current);
    }

    pub fn resize(&mut self, area: Rect) {
        // Keep the implementation compatible across ratatui versions by reinitializing.
        self.previous = Buffer::empty(area);
        self.current = Buffer::empty(area);
    }

    pub fn is_unchanged(&self) -> bool {
        if self.previous.area != self.current.area {
            return false;
        }
        self.previous.content == self.current.content
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Style;

    #[test]
    fn virtual_buffer_new() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 5,
        };
        let vb = VirtualBuffer::new(area);

        assert_eq!(vb.current().area, area);
        assert_eq!(vb.previous().area, area);
    }

    #[test]
    fn virtual_buffer_swap() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 5,
        };
        let mut vb = VirtualBuffer::new(area);

        vb.current_mut().set_string(0, 0, "test", Style::default());
        let before = vb.current().content[0].symbol().to_string();

        vb.swap();

        let after = vb.previous().content[0].symbol().to_string();
        assert_eq!(before, after);
    }

    #[test]
    fn virtual_buffer_is_unchanged() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 5,
        };
        let mut vb = VirtualBuffer::new(area);

        assert!(vb.is_unchanged(), "empty buffers should be unchanged");

        vb.current_mut().set_string(0, 0, "test", Style::default());
        assert!(!vb.is_unchanged(), "modified buffer should be changed");
    }

    #[test]
    fn virtual_buffer_resize() {
        let area1 = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 5,
        };
        let mut vb = VirtualBuffer::new(area1);

        let area2 = Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 10,
        };
        vb.resize(area2);

        assert_eq!(vb.current().area, area2);
        assert_eq!(vb.previous().area, area2);
    }
}
