use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, Default)]
pub struct TextBuffer {
    text: String,
    cursor: usize,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text(text: impl Into<String>) -> Self {
        let mut buf = Self {
            text: text.into(),
            cursor: 0,
        };
        buf.cursor = buf.text.len();
        buf
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
        self.clamp_cursor();
    }

    pub fn cursor_byte_index(&self) -> usize {
        self.cursor
    }

    pub fn cursor_display_col(&self) -> u16 {
        let prefix = &self.text[..self.cursor.min(self.text.len())];
        UnicodeWidthStr::width(prefix).min(u16::MAX as usize) as u16
    }

    pub fn set_cursor_display_col(&mut self, target_col: u16) {
        let mut col: u16 = 0;
        for (byte_idx, g) in self.text.grapheme_indices(true) {
            let w = (UnicodeWidthStr::width(g) as u16).max(1);
            let next = col.saturating_add(w);
            if target_col < next {
                self.cursor = byte_idx;
                self.clamp_cursor();
                return;
            }
            col = next;
        }

        self.cursor = self.text.len();
        self.clamp_cursor();
    }

    pub fn insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        self.clamp_cursor();
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
        self.clamp_cursor();
    }

    pub fn insert_char(&mut self, c: char) {
        self.clamp_cursor();
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.clamp_cursor();
    }

    pub fn backspace(&mut self) {
        self.clamp_cursor();
        let Some(prev) = prev_grapheme_boundary(&self.text, self.cursor) else {
            return;
        };
        if prev == self.cursor {
            return;
        }
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
        self.clamp_cursor();
    }

    pub fn delete(&mut self) {
        self.clamp_cursor();
        let Some(next) = next_grapheme_boundary(&self.text, self.cursor) else {
            return;
        };
        if next == self.cursor {
            return;
        }
        self.text.replace_range(self.cursor..next, "");
        self.clamp_cursor();
    }

    pub fn move_left(&mut self) {
        self.clamp_cursor();
        if let Some(prev) = prev_grapheme_boundary(&self.text, self.cursor) {
            self.cursor = prev;
        }
        self.clamp_cursor();
    }

    pub fn move_right(&mut self) {
        self.clamp_cursor();
        if let Some(next) = next_grapheme_boundary(&self.text, self.cursor) {
            self.cursor = next;
        }
        self.clamp_cursor();
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
        self.clamp_cursor();
    }

    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
        self.clamp_cursor();
    }

    pub fn clamp_cursor(&mut self) {
        self.cursor = self.cursor.min(self.text.len());
        if is_grapheme_boundary(&self.text, self.cursor) {
            return;
        }
        if let Some(prev) = prev_grapheme_boundary(&self.text, self.cursor) {
            self.cursor = prev;
        } else {
            self.cursor = 0;
        }
    }
}

fn is_grapheme_boundary(s: &str, idx: usize) -> bool {
    if idx == 0 || idx == s.len() {
        return true;
    }
    s.grapheme_indices(true).any(|(i, _)| i == idx)
}

fn prev_grapheme_boundary(s: &str, idx: usize) -> Option<usize> {
    let mut prev = None;
    for (i, _) in s.grapheme_indices(true) {
        if i >= idx {
            break;
        }
        prev = Some(i);
    }
    prev.or(Some(0))
}

fn next_grapheme_boundary(s: &str, idx: usize) -> Option<usize> {
    if idx >= s.len() {
        return Some(s.len());
    }
    for (i, _) in s.grapheme_indices(true) {
        if i > idx {
            return Some(i);
        }
    }
    Some(s.len())
}

#[cfg(test)]
mod tests {
    use super::TextBuffer;

    #[test]
    fn insert_and_backspace_ascii() {
        let mut b = TextBuffer::new();
        b.insert_str("abc");
        assert_eq!(b.text(), "abc");
        b.backspace();
        assert_eq!(b.text(), "ab");
    }

    #[test]
    fn cursor_moves_by_grapheme() {
        let mut b = TextBuffer::with_text("a👩‍💻b");
        b.move_home();
        b.move_right();
        assert_eq!(&b.text()[..b.cursor_byte_index()], "a");
        b.move_right();
        assert_eq!(&b.text()[..b.cursor_byte_index()], "a👩‍💻");
        b.move_right();
        assert_eq!(b.cursor_byte_index(), b.text().len());
    }

    #[test]
    fn backspace_removes_grapheme_cluster() {
        let mut b = TextBuffer::with_text("a👩‍💻b");
        b.backspace();
        assert_eq!(b.text(), "a👩‍💻");
        b.backspace();
        assert_eq!(b.text(), "a");
    }
}
