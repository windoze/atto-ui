// Syntax highlighting configuration + style mapping helpers.

use super::*;

impl EditorView {
    pub(super) fn configure_syntax_processor(&mut self) {
        self.syntax_processor = crate::syntax::build_syntax_processor(self.config.syntax.get());

        if let Some(processor) = self.syntax_processor.as_mut() {
            processor.apply(&mut self.state_manager);
        }
    }

    pub(super) fn maybe_apply_syntax_highlighting(&mut self) {
        if let Some(processor) = self.syntax_processor.as_mut() {
            processor.apply(&mut self.state_manager);
        }
    }
}
