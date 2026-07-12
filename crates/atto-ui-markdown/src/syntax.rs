//! Syntax highlighting primitives for markdown code blocks and chat renderers.
//!
//! This module hides the syntect parser behind crate-owned data types so callers can map syntax
//! classes to their own UI styles without depending on syntect APIs.

use std::sync::LazyLock;

use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

/// A normalized language hint extracted from a fenced code block info string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageHint {
    token: String,
    extension: Option<String>,
}

impl LanguageHint {
    /// Extracts the first word from a fence info string and records a file extension fallback.
    pub fn from_info_string(info: &str) -> Option<Self> {
        let token = info.split_whitespace().next()?.trim();
        if token.is_empty() {
            return None;
        }

        let token = token
            .strip_prefix("language-")
            .unwrap_or(token)
            .trim_start_matches('.');
        if token.is_empty() {
            return None;
        }

        let extension = token
            .rsplit_once('.')
            .and_then(|(_, ext)| (!ext.is_empty()).then(|| ext.to_ascii_lowercase()));

        Some(Self {
            token: token.to_ascii_lowercase(),
            extension,
        })
    }

    /// Returns the primary token used for syntax lookup.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Returns the extension fallback, if the primary token looked like a filename.
    pub fn extension(&self) -> Option<&str> {
        self.extension.as_deref()
    }
}

/// A syntax-highlighted source line, preserving its plain text for width calculations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightedLine {
    pub plain: String,
    pub spans: Vec<HighlightedSpan>,
}

/// A contiguous source segment with a neutral syntax class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightedSpan {
    pub text: String,
    pub class: SyntaxClass,
}

/// Neutral syntax categories used by renderers to choose their own colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SyntaxClass {
    Text,
    Comment,
    String,
    Keyword,
    Function,
    Type,
    Number,
    Constant,
    Variable,
    Operator,
    Punctuation,
}

/// Highlights a fenced code block using the first word of the info string as the language hint.
///
/// Returns `None` when the hint is missing, unknown, or parsing fails, allowing callers to keep
/// their existing plain-text rendering path.
pub fn highlight_code_block(info: Option<&str>, text: &str) -> Option<Vec<HighlightedLine>> {
    let hint = LanguageHint::from_info_string(info?)?;
    let syntax = find_syntax(&hint)?;
    highlight_with_syntax(syntax, text)
}

fn find_syntax(hint: &LanguageHint) -> Option<&'static SyntaxReference> {
    let syntax_set = &*SYNTAX_SET;
    syntax_set.find_syntax_by_token(hint.token()).or_else(|| {
        hint.extension()
            .and_then(|ext| syntax_set.find_syntax_by_extension(ext))
    })
}

fn highlight_with_syntax(syntax: &SyntaxReference, text: &str) -> Option<Vec<HighlightedLine>> {
    let syntax_set = &*SYNTAX_SET;
    let mut parser = ParseState::new(syntax);
    let mut scope_stack = ScopeStack::new();
    let raw_lines: Vec<&str> = text.split('\n').collect();
    let last_index = raw_lines.len().saturating_sub(1);
    let mut lines = Vec::with_capacity(raw_lines.len().max(1));

    for (idx, raw_line) in raw_lines.iter().enumerate() {
        let parse_line = if idx < last_index {
            format!("{raw_line}\n")
        } else {
            (*raw_line).to_string()
        };
        let ops = parser.parse_line(&parse_line, syntax_set).ok()?;
        let mut highlighted = HighlightedLine {
            plain: normalize_tabs(raw_line),
            spans: Vec::new(),
        };
        let mut cursor = 0usize;

        for (offset, op) in ops {
            let visible_offset = offset.min(raw_line.len());
            if visible_offset > cursor {
                push_span(
                    &mut highlighted.spans,
                    &raw_line[cursor..visible_offset],
                    class_for_scope_stack(&scope_stack),
                );
                cursor = visible_offset;
            }
            scope_stack.apply(&op).ok()?;
        }

        if cursor < raw_line.len() {
            push_span(
                &mut highlighted.spans,
                &raw_line[cursor..],
                class_for_scope_stack(&scope_stack),
            );
        }

        lines.push(highlighted);
    }

    Some(lines)
}

fn push_span(spans: &mut Vec<HighlightedSpan>, text: &str, class: SyntaxClass) {
    let text = normalize_tabs(text);
    if text.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut()
        && last.class == class
    {
        last.text.push_str(&text);
        return;
    }
    spans.push(HighlightedSpan { text, class });
}

fn normalize_tabs(text: &str) -> String {
    text.replace('\t', "    ")
}

fn class_for_scope_stack(stack: &ScopeStack) -> SyntaxClass {
    for scope in stack.as_slice().iter().rev() {
        let scope = scope.to_string();
        if scope.contains("comment") {
            return SyntaxClass::Comment;
        }
        if scope.contains("string") || scope.contains("quoted") {
            return SyntaxClass::String;
        }
        if scope.contains("constant.numeric") || scope.contains("numeric") {
            return SyntaxClass::Number;
        }
        if scope.contains("entity.name.function") || scope.contains("support.function") {
            return SyntaxClass::Function;
        }
        if scope.contains("entity.name.type") || scope.contains("support.type") {
            return SyntaxClass::Type;
        }
        if scope.contains("constant") {
            return SyntaxClass::Constant;
        }
        if scope.contains("variable") {
            return SyntaxClass::Variable;
        }
        if scope.contains("keyword.operator") || scope.contains("operator") {
            return SyntaxClass::Operator;
        }
        if scope.contains("keyword") || scope.contains("storage") {
            return SyntaxClass::Keyword;
        }
        if scope.contains("punctuation") {
            return SyntaxClass::Punctuation;
        }
    }
    SyntaxClass::Text
}

#[cfg(test)]
mod tests {
    use super::{LanguageHint, SyntaxClass, highlight_code_block};

    #[test]
    fn language_hint_uses_first_word_and_extension_fallback() {
        let hint = LanguageHint::from_info_string("language-main.rs ignore").unwrap();
        assert_eq!(hint.token(), "main.rs");
        assert_eq!(hint.extension(), Some("rs"));
    }

    #[test]
    fn unknown_or_missing_language_falls_back_to_plain_text() {
        assert!(highlight_code_block(None, "let x = 1;").is_none());
        assert!(highlight_code_block(Some("not-a-real-language"), "let x = 1;").is_none());
    }

    #[test]
    fn rust_code_highlights_without_losing_plain_text() {
        let lines = highlight_code_block(Some("rust"), "fn main() {\n    let answer = 42;\n}")
            .expect("rust should be highlighted");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].plain, "fn main() {");
        assert_eq!(lines[1].plain, "    let answer = 42;");

        let classes = lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.class))
            .collect::<Vec<_>>();
        assert!(classes.contains(&SyntaxClass::Keyword));
        assert!(classes.contains(&SyntaxClass::Number));
    }
}
