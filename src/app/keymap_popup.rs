use super::{KeyChord, WhichKeyChoice, key_sequence_label};

/// Data model for a which-key prefix hint popup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WhichKeyModel {
    pub prefix_label: String,
    pub choices: Vec<WhichKeyChoice>,
}

impl WhichKeyModel {
    /// Creates a model from a rendered prefix label and sorted choices.
    pub fn new(prefix_label: impl Into<String>, choices: Vec<WhichKeyChoice>) -> Self {
        Self {
            prefix_label: prefix_label.into(),
            choices,
        }
    }

    /// Creates a model by formatting the pending key sequence.
    pub fn for_prefix(prefix: &[KeyChord], choices: Vec<WhichKeyChoice>) -> Self {
        Self::new(key_sequence_label(prefix), choices)
    }

    /// Returns true when the popup has no choices to display.
    pub fn is_empty(&self) -> bool {
        self.choices.is_empty()
    }
}
