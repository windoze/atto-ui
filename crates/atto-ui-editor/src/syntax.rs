use editor_core::StyleLayerId;
use editor_core_highlight_simple::{
    RegexHighlightProcessor, RegexHighlighter, RegexRule, SimpleIniStyles, SimpleJsonStyles,
};
use editor_core_sublime::SublimeProcessor;
use editor_core_treesitter::{TreeSitterProcessor, TreeSitterProcessorConfig};
use ratatui::style::Style;

use crate::config::EditorSyntaxConfig;
use crate::theme::{
    EditorTheme, TS_STYLE_COMMENT, TS_STYLE_FUNCTION, TS_STYLE_KEYWORD, TS_STYLE_NUMBER,
    TS_STYLE_STRING, TS_STYLE_TYPE,
};

#[allow(clippy::large_enum_variant)]
pub(crate) enum SyntaxProcessor {
    Regex(RegexHighlightProcessor),
    Sublime(SublimeProcessor),
    TreeSitter(TreeSitterProcessor),
}

impl SyntaxProcessor {
    pub(crate) fn apply(&mut self, state: &mut editor_core::EditorStateManager) {
        match self {
            SyntaxProcessor::Regex(p) => {
                let _ = state.apply_processor(p);
            }
            SyntaxProcessor::Sublime(p) => {
                let _ = state.apply_processor(p);
            }
            SyntaxProcessor::TreeSitter(p) => {
                let _ = state.apply_processor(p);
            }
        }
    }

    pub(crate) fn sublime_scope_for_style_id(&self, style_id: u32) -> Option<&str> {
        match self {
            SyntaxProcessor::Sublime(p) => p.scope_mapper.scope_for_style_id(style_id),
            _ => None,
        }
    }
}

pub(crate) fn build_syntax_processor(cfg: EditorSyntaxConfig) -> Option<SyntaxProcessor> {
    match cfg {
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
        EditorSyntaxConfig::SimpleRust => simple_rust_processor().ok().map(SyntaxProcessor::Regex),
        EditorSyntaxConfig::TreeSitter(cfg) => {
            let mut ts = TreeSitterProcessorConfig::new(
                editor_core_treesitter::TreeSitterLanguage::Native(cfg.language),
                cfg.highlights_query,
            );
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
            let mut set = editor_core_sublime::SublimeSyntaxSet::new();
            for p in include_paths {
                set.add_search_path(p);
            }
            match set.load_from_path(&syntax_file) {
                Ok(syntax) => Some(SyntaxProcessor::Sublime(SublimeProcessor::new(syntax, set))),
                Err(_) => None,
            }
        }
    }
}

pub(crate) fn style_for_sublime_scope(theme: &EditorTheme, scope: &str) -> Option<Style> {
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

fn simple_rust_processor() -> Result<RegexHighlightProcessor, regex::Error> {
    let highlighter = RegexHighlighter::new(vec![
        RegexRule::new(r#"//.*$"#, TS_STYLE_COMMENT)?,
        RegexRule::new(r#"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)"#, TS_STYLE_FUNCTION)?
            .with_capture_group(1),
        RegexRule::new(r#"\b([A-Za-z_][A-Za-z0-9_]*)!?\s*\("#, TS_STYLE_FUNCTION)?
            .with_capture_group(1),
        RegexRule::new(r#"\"(?:\\.|[^\"\\])*\""#, TS_STYLE_STRING)?,
        RegexRule::new(r#"\b(?:0x[0-9A-Fa-f_]+|\d[\d_]*)\b"#, TS_STYLE_NUMBER)?,
        RegexRule::new(
            r#"\b(?:as|async|await|break|const|continue|crate|dyn|else|enum|extern|false|fn|for|if|impl|in|let|loop|match|mod|move|mut|pub|ref|return|self|Self|static|struct|super|trait|true|type|unsafe|use|where|while)\b"#,
            TS_STYLE_KEYWORD,
        )?,
        RegexRule::new(r#"\b[A-Z][A-Za-z0-9_]*\b"#, TS_STYLE_TYPE)?,
    ]);
    Ok(RegexHighlightProcessor::new(
        StyleLayerId::SIMPLE_SYNTAX,
        highlighter,
    ))
}
