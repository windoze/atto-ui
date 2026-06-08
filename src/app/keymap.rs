use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Default timeout used while waiting for the next chord in a key sequence.
pub const DEFAULT_KEY_SEQUENCE_TIMEOUT: Duration = Duration::from_millis(1_000);

/// A single keyboard chord: one key code plus its exact crossterm modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyChord {
    /// Creates a key chord from a crossterm key code and modifier bitset.
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    /// Converts a crossterm key press event into a chord.
    pub fn from_key_event(event: KeyEvent) -> Option<Self> {
        if event.kind != KeyEventKind::Press {
            return None;
        }

        Some(Self::new(event.code, event.modifiers))
    }

    /// Returns a user-facing label such as `Ctrl+K` or `Shift+F8`.
    pub fn label(self) -> String {
        key_chord_label(self)
    }
}

/// A VSCode-style key sequence made from one or more chords.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeySequence(pub Vec<KeyChord>);

impl KeySequence {
    /// Creates a key sequence from an ordered list of chords.
    pub fn new(chords: Vec<KeyChord>) -> Self {
        Self(chords)
    }

    /// Creates a one-chord sequence.
    pub fn single(chord: KeyChord) -> Self {
        Self(vec![chord])
    }

    /// Returns the sequence as a slice of chords.
    pub fn as_slice(&self) -> &[KeyChord] {
        &self.0
    }

    /// Returns true if the sequence has no chords.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns a user-facing label such as `Ctrl+K Ctrl+F`.
    pub fn label(&self) -> String {
        key_sequence_label(self.as_slice())
    }
}

impl From<KeyChord> for KeySequence {
    fn from(value: KeyChord) -> Self {
        Self::single(value)
    }
}

impl From<Vec<KeyChord>> for KeySequence {
    fn from(value: Vec<KeyChord>) -> Self {
        Self::new(value)
    }
}

/// One row in a which-key style prefix hint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhichKeyChoice {
    pub key_label: String,
    pub command_id: String,
    pub title: String,
}

/// Result returned after feeding a chord to a key sequence engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeymapMatch<A> {
    None,
    Prefix {
        choices: Vec<WhichKeyChoice>,
    },
    Exact(A),
    AmbiguousExact {
        action: A,
        choices: Vec<WhichKeyChoice>,
    },
    Timeout,
}

/// Stateful trie-backed matcher for single-chord and multi-chord key bindings.
#[derive(Clone, Debug)]
pub struct KeySequenceEngine<A> {
    trie: KeyTrie<A>,
    pending: Vec<KeyChord>,
    pending_since: Option<Instant>,
    timeout: Duration,
}

impl<A> KeySequenceEngine<A> {
    /// Creates an empty key sequence engine with the provided prefix timeout.
    pub fn new(timeout: Duration) -> Self {
        Self {
            trie: KeyTrie::new(),
            pending: Vec::new(),
            pending_since: None,
            timeout,
        }
    }

    /// Returns the configured prefix timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Updates the configured prefix timeout.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Returns the currently pending prefix sequence.
    pub fn pending(&self) -> &[KeyChord] {
        &self.pending
    }

    /// Clears any pending prefix state.
    pub fn clear_pending(&mut self) {
        self.pending.clear();
        self.pending_since = None;
    }

    /// Inserts a binding, using the sequence label as fallback metadata.
    pub fn insert(&mut self, sequence: impl Into<KeySequence>, action: A) {
        let sequence = sequence.into();
        let label = sequence.label();
        self.insert_with_metadata(sequence, label.clone(), label, action);
    }

    /// Inserts a binding with command metadata used by prefix choice hints.
    pub fn insert_with_metadata(
        &mut self,
        sequence: impl Into<KeySequence>,
        command_id: impl Into<String>,
        title: impl Into<String>,
        action: A,
    ) {
        let sequence = sequence.into();
        if sequence.is_empty() {
            return;
        }

        let mut node = &mut self.trie;
        for chord in sequence.as_slice() {
            node = node.child_mut_or_insert(*chord);
        }
        node.binding = Some(KeyBinding {
            action,
            command_id: command_id.into(),
            title: title.into(),
        });
    }
}

impl<A: Clone> KeySequenceEngine<A> {
    /// Feeds one chord into the engine at a caller-supplied timestamp.
    pub fn handle_key(&mut self, chord: KeyChord, now: Instant) -> KeymapMatch<A> {
        if self.pending_timed_out(now) {
            self.clear_pending();
            return KeymapMatch::Timeout;
        }

        let mut candidate = self.pending.clone();
        candidate.push(chord);

        let Some(node) = self.trie.find(&candidate) else {
            self.clear_pending();
            return KeymapMatch::None;
        };

        let choices = node.choices();
        match (node.binding.as_ref(), choices.is_empty()) {
            (Some(binding), true) => {
                let action = binding.action.clone();
                self.clear_pending();
                KeymapMatch::Exact(action)
            }
            (Some(binding), false) => {
                let action = binding.action.clone();
                self.set_pending(candidate, now);
                KeymapMatch::AmbiguousExact { action, choices }
            }
            (None, false) => {
                self.set_pending(candidate, now);
                KeymapMatch::Prefix { choices }
            }
            (None, true) => {
                self.clear_pending();
                KeymapMatch::None
            }
        }
    }

    fn pending_timed_out(&self, now: Instant) -> bool {
        !self.pending.is_empty()
            && self
                .pending_since
                .is_some_and(|since| now.saturating_duration_since(since) >= self.timeout)
    }

    fn set_pending(&mut self, pending: Vec<KeyChord>, now: Instant) {
        self.pending = pending;
        self.pending_since = Some(now);
    }
}

impl<A> Default for KeySequenceEngine<A> {
    fn default() -> Self {
        Self::new(DEFAULT_KEY_SEQUENCE_TIMEOUT)
    }
}

#[derive(Clone, Debug)]
struct KeyBinding<A> {
    action: A,
    command_id: String,
    title: String,
}

#[derive(Clone, Debug)]
struct KeyTrie<A> {
    binding: Option<KeyBinding<A>>,
    children: Vec<KeyTrieEdge<A>>,
}

impl<A> KeyTrie<A> {
    fn new() -> Self {
        Self {
            binding: None,
            children: Vec::new(),
        }
    }

    fn child_mut_or_insert(&mut self, chord: KeyChord) -> &mut Self {
        if let Some(index) = self.children.iter().position(|edge| edge.chord == chord) {
            return &mut self.children[index].node;
        }

        self.children.push(KeyTrieEdge {
            chord,
            node: KeyTrie::new(),
        });
        &mut self.children.last_mut().expect("just inserted child").node
    }

    fn find(&self, sequence: &[KeyChord]) -> Option<&Self> {
        let mut node = self;
        for chord in sequence {
            let edge = node.children.iter().find(|edge| edge.chord == *chord)?;
            node = &edge.node;
        }
        Some(node)
    }

    fn choices(&self) -> Vec<WhichKeyChoice> {
        let mut choices = self
            .children
            .iter()
            .map(|edge| {
                let (command_id, title) = edge.node.choice_metadata();
                WhichKeyChoice {
                    key_label: edge.chord.label(),
                    command_id,
                    title,
                }
            })
            .collect::<Vec<_>>();
        choices.sort_by(|a, b| {
            a.key_label
                .cmp(&b.key_label)
                .then_with(|| a.command_id.cmp(&b.command_id))
                .then_with(|| a.title.cmp(&b.title))
        });
        choices
    }

    fn choice_metadata(&self) -> (String, String) {
        self.first_binding_metadata()
            .unwrap_or_else(|| (String::new(), String::new()))
    }

    fn first_binding_metadata(&self) -> Option<(String, String)> {
        if let Some(binding) = &self.binding {
            return Some((binding.command_id.clone(), binding.title.clone()));
        }

        let mut child_indexes = (0..self.children.len()).collect::<Vec<_>>();
        child_indexes.sort_by(|left, right| {
            self.children[*left]
                .chord
                .label()
                .cmp(&self.children[*right].chord.label())
        });

        for index in child_indexes {
            if let Some(metadata) = self.children[index].node.first_binding_metadata() {
                return Some(metadata);
            }
        }
        None
    }
}

#[derive(Clone, Debug)]
struct KeyTrieEdge<A> {
    chord: KeyChord,
    node: KeyTrie<A>,
}

/// Formats a single key chord for menu accelerators and which-key choices.
pub fn key_chord_label(chord: KeyChord) -> String {
    let mut parts = Vec::new();
    if chord.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("Ctrl".to_string());
    }
    if chord.modifiers.contains(KeyModifiers::ALT) {
        parts.push("Alt".to_string());
    }
    if chord.modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("Shift".to_string());
    }
    if chord.modifiers.contains(KeyModifiers::SUPER) {
        parts.push("Super".to_string());
    }
    if chord.modifiers.contains(KeyModifiers::HYPER) {
        parts.push("Hyper".to_string());
    }
    if chord.modifiers.contains(KeyModifiers::META) {
        parts.push("Meta".to_string());
    }

    parts.push(key_code_label(chord.code));
    parts.join("+")
}

/// Formats an ordered sequence of chords.
pub fn key_sequence_label(chords: &[KeyChord]) -> String {
    chords
        .iter()
        .map(|chord| chord.label())
        .collect::<Vec<_>>()
        .join(" ")
}

fn key_code_label(code: KeyCode) -> String {
    match code {
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "BackTab".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::F(n) => format!("F{n}"),
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => c.to_uppercase().collect::<String>(),
        KeyCode::Null => "Null".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::CapsLock => "CapsLock".to_string(),
        KeyCode::ScrollLock => "ScrollLock".to_string(),
        KeyCode::NumLock => "NumLock".to_string(),
        KeyCode::PrintScreen => "PrintScreen".to_string(),
        KeyCode::Pause => "Pause".to_string(),
        KeyCode::Menu => "Menu".to_string(),
        KeyCode::KeypadBegin => "KeypadBegin".to_string(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Action {
        Save,
        Format,
        Clear,
        Prefix,
    }

    fn ctrl(ch: char) -> KeyChord {
        KeyChord::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
    }

    #[test]
    fn single_key_exact_clears_pending() {
        let mut engine = KeySequenceEngine::new(Duration::from_secs(1));
        engine.insert_with_metadata(ctrl('s'), "file.save", "Save", Action::Save);

        let result = engine.handle_key(ctrl('s'), Instant::now());

        assert_eq!(result, KeymapMatch::Exact(Action::Save));
        assert!(engine.pending().is_empty());
    }

    #[test]
    fn prefix_returns_sorted_which_key_choices() {
        let mut engine = KeySequenceEngine::new(Duration::from_secs(1));
        engine.insert_with_metadata(
            vec![ctrl('k'), ctrl('f')],
            "editor.format",
            "Format Document",
            Action::Format,
        );
        engine.insert_with_metadata(
            vec![ctrl('k'), ctrl('c')],
            "editor.clear",
            "Clear Selection",
            Action::Clear,
        );

        let result = engine.handle_key(ctrl('k'), Instant::now());

        assert_eq!(
            result,
            KeymapMatch::Prefix {
                choices: vec![
                    WhichKeyChoice {
                        key_label: "Ctrl+C".to_string(),
                        command_id: "editor.clear".to_string(),
                        title: "Clear Selection".to_string(),
                    },
                    WhichKeyChoice {
                        key_label: "Ctrl+F".to_string(),
                        command_id: "editor.format".to_string(),
                        title: "Format Document".to_string(),
                    },
                ]
            }
        );
        assert_eq!(engine.pending(), &[ctrl('k')]);
    }

    #[test]
    fn multi_chord_exact_matches_after_prefix() {
        let mut engine = KeySequenceEngine::new(Duration::from_secs(1));
        engine.insert(vec![ctrl('k'), ctrl('f')], Action::Format);
        let now = Instant::now();

        assert!(matches!(
            engine.handle_key(ctrl('k'), now),
            KeymapMatch::Prefix { .. }
        ));
        let result = engine.handle_key(ctrl('f'), now + Duration::from_millis(100));

        assert_eq!(result, KeymapMatch::Exact(Action::Format));
        assert!(engine.pending().is_empty());
    }

    #[test]
    fn ambiguous_exact_keeps_pending_prefix() {
        let mut engine = KeySequenceEngine::new(Duration::from_secs(1));
        engine.insert_with_metadata(
            ctrl('k'),
            "selection.clear",
            "Clear Selection",
            Action::Clear,
        );
        engine.insert_with_metadata(
            vec![ctrl('k'), ctrl('f')],
            "editor.format",
            "Format Document",
            Action::Format,
        );

        let result = engine.handle_key(ctrl('k'), Instant::now());

        assert_eq!(
            result,
            KeymapMatch::AmbiguousExact {
                action: Action::Clear,
                choices: vec![WhichKeyChoice {
                    key_label: "Ctrl+F".to_string(),
                    command_id: "editor.format".to_string(),
                    title: "Format Document".to_string(),
                }]
            }
        );
        assert_eq!(engine.pending(), &[ctrl('k')]);
    }

    #[test]
    fn timeout_clears_pending_without_wall_clock_global() {
        let mut engine = KeySequenceEngine::new(Duration::from_millis(50));
        engine.insert(vec![ctrl('k'), ctrl('f')], Action::Format);
        let now = Instant::now();

        assert!(matches!(
            engine.handle_key(ctrl('k'), now),
            KeymapMatch::Prefix { .. }
        ));
        let result = engine.handle_key(ctrl('f'), now + Duration::from_millis(50));

        assert_eq!(result, KeymapMatch::Timeout);
        assert!(engine.pending().is_empty());
    }

    #[test]
    fn timeout_resets_after_each_successful_prefix_chord() {
        let mut engine = KeySequenceEngine::new(Duration::from_millis(50));
        engine.insert(vec![ctrl('k'), ctrl('f'), ctrl('s')], Action::Format);
        let now = Instant::now();

        assert!(matches!(
            engine.handle_key(ctrl('k'), now),
            KeymapMatch::Prefix { .. }
        ));
        assert!(matches!(
            engine.handle_key(ctrl('f'), now + Duration::from_millis(40)),
            KeymapMatch::Prefix { .. }
        ));
        let result = engine.handle_key(ctrl('s'), now + Duration::from_millis(80));

        assert_eq!(result, KeymapMatch::Exact(Action::Format));
        assert!(engine.pending().is_empty());
    }

    #[test]
    fn modifier_comparison_uses_exact_crossterm_bitset() {
        let mut engine = KeySequenceEngine::new(Duration::from_secs(1));
        engine.insert(ctrl('s'), Action::Save);

        let result = engine.handle_key(
            KeyChord::new(
                KeyCode::Char('s'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
            Instant::now(),
        );

        assert_eq!(result, KeymapMatch::None);
    }

    #[test]
    fn invalid_key_after_prefix_clears_pending() {
        let mut engine = KeySequenceEngine::new(Duration::from_secs(1));
        engine.insert(vec![ctrl('k'), ctrl('f')], Action::Format);
        let now = Instant::now();

        assert!(matches!(
            engine.handle_key(ctrl('k'), now),
            KeymapMatch::Prefix { .. }
        ));
        let result = engine.handle_key(ctrl('x'), now + Duration::from_millis(10));

        assert_eq!(result, KeymapMatch::None);
        assert!(engine.pending().is_empty());
    }

    #[test]
    fn labels_format_common_chords_and_sequences() {
        let format_sequence = KeySequence::new(vec![ctrl('k'), ctrl('f')]);
        let shifted_f8 = KeyChord::new(KeyCode::F(8), KeyModifiers::SHIFT);

        assert_eq!(ctrl('k').label(), "Ctrl+K");
        assert_eq!(shifted_f8.label(), "Shift+F8");
        assert_eq!(format_sequence.label(), "Ctrl+K Ctrl+F");
    }

    #[test]
    fn from_key_event_uses_press_events_only() {
        let event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);

        assert_eq!(KeyChord::from_key_event(event), Some(ctrl('s')));
    }

    #[test]
    fn single_chord_that_is_also_prefix_can_continue() {
        let mut engine = KeySequenceEngine::new(Duration::from_secs(1));
        engine.insert(ctrl('k'), Action::Prefix);
        engine.insert(vec![ctrl('k'), ctrl('f')], Action::Format);
        let now = Instant::now();

        assert!(matches!(
            engine.handle_key(ctrl('k'), now),
            KeymapMatch::AmbiguousExact { .. }
        ));
        let result = engine.handle_key(ctrl('f'), now + Duration::from_millis(10));

        assert_eq!(result, KeymapMatch::Exact(Action::Format));
    }
}
