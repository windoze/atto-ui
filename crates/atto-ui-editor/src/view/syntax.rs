// Syntax highlighting configuration + style mapping helpers.

use super::*;

impl EditorView {
    pub(super) fn configure_syntax_processor(&mut self) {
        let cfg = self.config.syntax.get();
        self.syntax_processor = match cfg {
            EditorSyntaxConfig::None => None,
            EditorSyntaxConfig::SimpleJson => {
                RegexHighlightProcessor::json_default(SimpleJsonStyles::default())
                    .ok()
                    .map(SyntaxProcessor::Regex)
            }
            EditorSyntaxConfig::SimpleIni => {
                RegexHighlightProcessor::ini_default(SimpleIniStyles::default())
                    .ok()
                    .map(SyntaxProcessor::Regex)
            }
            EditorSyntaxConfig::TreeSitter(cfg) => {
                let mut ts = TreeSitterProcessorConfig::new(cfg.language, cfg.highlights_query);
                ts.folds_query = cfg.folds_query;
                ts.capture_styles = cfg.capture_styles;
                ts.style_layer = cfg.style_layer;
                ts.preserve_collapsed_folds = cfg.preserve_collapsed_folds;

                TreeSitterProcessor::new(ts)
                    .ok()
                    .map(SyntaxProcessor::TreeSitter)
            }
            EditorSyntaxConfig::Sublime {
                syntax_file,
                include_paths,
            } => {
                let mut set = SublimeSyntaxSet::new();
                for p in include_paths {
                    set.add_search_path(p);
                }
                match set.load_from_path(&syntax_file) {
                    Ok(syntax) => {
                        Some(SyntaxProcessor::Sublime(SublimeProcessor::new(syntax, set)))
                    }
                    Err(_) => None,
                }
            }
        };

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

pub(super) fn style_for_sublime_scope(theme: &EditorTheme, scope: &str) -> Option<Style> {
    let trimmed = scope.trim();
    if trimmed.is_empty() {
        return None;
    }

    // A scope string can contain multiple scopes separated by spaces.
    for scope in trimmed.split_whitespace() {
        let mut candidate = scope;
        loop {
            if let Some(style) = theme.sublime_scopes.get(candidate) {
                return Some(*style);
            }

            let Some((parent, _tail)) = candidate.rsplit_once('.') else {
                break;
            };
            candidate = parent;
        }
    }

    None
}
