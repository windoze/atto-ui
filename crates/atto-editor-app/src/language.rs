#![allow(clippy::match_same_arms)]

use std::collections::BTreeMap;
use std::env;
use std::path::Path;

use atto_ui_editor::{
    EditorLspConfig, EditorLspMode, EditorSyntaxConfig, EditorTreeSitterConfig, TS_STYLE_COMMENT,
    TS_STYLE_CONSTANT, TS_STYLE_FUNCTION, TS_STYLE_KEYWORD, TS_STYLE_NUMBER, TS_STYLE_STRING,
    TS_STYLE_TYPE, TS_STYLE_VARIABLE,
};
use editor_core::intervals::StyleId;

pub fn guess_language_id(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "rs" => "rust",
        "toml" => "toml",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "md" | "markdown" => "markdown",
        "py" => "python",
        "js" => "javascript",
        "jsx" => "javascriptreact",
        "ts" => "typescript",
        "tsx" => "typescriptreact",
        _ => "plaintext",
    }
    .to_string()
}

pub fn syntax_config_for_file(path: &Path, language_id: &str) -> EditorSyntaxConfig {
    if let Some(ts) = treesitter_config_for_language(language_id) {
        return EditorSyntaxConfig::TreeSitter(ts);
    }

    // Optional Sublime fallback: users can point `ATTO_EDITOR_SUBLIME_SYNTAX_FILE` at a
    // `.sublime-syntax` file and optionally set include search paths.
    if let Some(cfg) = sublime_fallback_config(language_id) {
        return cfg;
    }

    // Otherwise: no syntax highlighting (LSP semantic tokens may still apply if enabled).
    let _ = path;
    EditorSyntaxConfig::None
}

pub fn lsp_mode_for_file(path: &Path, language_id: &str) -> EditorLspMode {
    let Some(cmd) = lsp_command_for_language(language_id) else {
        return EditorLspMode::Disabled;
    };

    let cfg = EditorLspConfig::for_file_path(path, language_id.to_string(), cmd);
    EditorLspMode::Enabled(cfg)
}

fn lsp_command_for_language(language_id: &str) -> Option<Vec<String>> {
    let key = format!("ATTO_EDITOR_LSP_CMD_{}", language_id.to_ascii_uppercase());
    parse_cmd_env(&key).or_else(|| parse_cmd_env("ATTO_EDITOR_LSP_CMD"))
}

fn parse_cmd_env(var: &str) -> Option<Vec<String>> {
    let raw = env::var(var).ok()?;
    let parts = raw
        .split_whitespace()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    (!parts.is_empty()).then_some(parts)
}

fn treesitter_config_for_language(language_id: &str) -> Option<EditorTreeSitterConfig> {
    let capture_styles = default_treesitter_capture_styles();

    match language_id {
        "rust" => Some(
            EditorTreeSitterConfig::new(
                tree_sitter_rust::LANGUAGE.into(),
                tree_sitter_rust::HIGHLIGHTS_QUERY,
            )
            .with_folds_query(default_rust_like_folds_query())
            .with_capture_styles(capture_styles),
        ),
        "json" => Some(
            EditorTreeSitterConfig::new(
                tree_sitter_json::LANGUAGE.into(),
                tree_sitter_json::HIGHLIGHTS_QUERY,
            )
            .with_folds_query(default_json_folds_query())
            .with_capture_styles(capture_styles),
        ),
        "python" => Some(
            EditorTreeSitterConfig::new(
                tree_sitter_python::LANGUAGE.into(),
                tree_sitter_python::HIGHLIGHTS_QUERY,
            )
            .with_folds_query(default_python_folds_query())
            .with_capture_styles(capture_styles),
        ),
        "javascript" | "javascriptreact" => Some(
            EditorTreeSitterConfig::new(
                tree_sitter_javascript::LANGUAGE.into(),
                if language_id == "javascriptreact" {
                    tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
                } else {
                    tree_sitter_javascript::HIGHLIGHT_QUERY
                },
            )
            .with_folds_query(default_rust_like_folds_query())
            .with_capture_styles(capture_styles),
        ),
        "toml" => Some(
            EditorTreeSitterConfig::new(
                tree_sitter_toml_ng::LANGUAGE.into(),
                tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            )
            .with_folds_query(default_toml_folds_query())
            .with_capture_styles(capture_styles),
        ),
        "yaml" => Some(
            EditorTreeSitterConfig::new(
                tree_sitter_yaml::LANGUAGE.into(),
                tree_sitter_yaml::HIGHLIGHTS_QUERY,
            )
            .with_folds_query(default_yaml_folds_query())
            .with_capture_styles(capture_styles),
        ),
        "typescript" => Some(
            EditorTreeSitterConfig::new(
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            )
            .with_folds_query(default_rust_like_folds_query())
            .with_capture_styles(capture_styles),
        ),
        "typescriptreact" => Some(
            EditorTreeSitterConfig::new(
                tree_sitter_typescript::LANGUAGE_TSX.into(),
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            )
            .with_folds_query(default_rust_like_folds_query())
            .with_capture_styles(capture_styles),
        ),
        _ => None,
    }
}

fn default_treesitter_capture_styles() -> BTreeMap<String, StyleId> {
    // Tree-sitter highlight queries vary a bit by language. We map a small common subset, and
    // rely on the editor theme to keep these consistent.
    let mut map: BTreeMap<String, StyleId> = BTreeMap::new();

    // Comments / strings / numbers
    for name in [
        "comment",
        "comment.line",
        "comment.block",
        "string",
        "string.special",
        "string.escape",
        "character",
        "number",
        "float",
        "integer",
        "boolean",
    ] {
        let id = match name {
            "comment" | "comment.line" | "comment.block" => TS_STYLE_COMMENT,
            "string" | "string.special" | "string.escape" | "character" => TS_STYLE_STRING,
            _ => TS_STYLE_NUMBER,
        };
        map.insert(name.to_string(), id);
    }

    // Keywords / types / functions / variables / constants
    for (name, id) in [
        ("keyword", TS_STYLE_KEYWORD),
        ("keyword.function", TS_STYLE_KEYWORD),
        ("keyword.operator", TS_STYLE_KEYWORD),
        ("operator", TS_STYLE_KEYWORD),
        ("type", TS_STYLE_TYPE),
        ("type.builtin", TS_STYLE_TYPE),
        ("constructor", TS_STYLE_TYPE),
        ("function", TS_STYLE_FUNCTION),
        ("function.builtin", TS_STYLE_FUNCTION),
        ("method", TS_STYLE_FUNCTION),
        ("property", TS_STYLE_VARIABLE),
        ("variable", TS_STYLE_VARIABLE),
        ("variable.builtin", TS_STYLE_VARIABLE),
        ("constant", TS_STYLE_CONSTANT),
        ("constant.builtin", TS_STYLE_CONSTANT),
        ("attribute", TS_STYLE_CONSTANT),
    ] {
        map.insert(name.to_string(), id);
    }

    map
}

fn default_rust_like_folds_query() -> String {
    r#"
    (function_item) @fold
    (impl_item) @fold
    (struct_item) @fold
    (enum_item) @fold
    (mod_item) @fold
    (block) @fold
    "#
    .to_string()
}

fn default_json_folds_query() -> String {
    r#"
    (object) @fold
    (array) @fold
    "#
    .to_string()
}

fn default_python_folds_query() -> String {
    r#"
    (function_definition) @fold
    (class_definition) @fold
    (block) @fold
    "#
    .to_string()
}

fn default_toml_folds_query() -> String {
    r#"
    (table) @fold
    (array_table) @fold
    "#
    .to_string()
}

fn default_yaml_folds_query() -> String {
    r#"
    (block_mapping) @fold
    (block_sequence) @fold
    "#
    .to_string()
}

fn sublime_fallback_config(_language_id: &str) -> Option<EditorSyntaxConfig> {
    // Opt-in (paths are host/environment specific).
    let syntax_file = env::var("ATTO_EDITOR_SUBLIME_SYNTAX_FILE").ok()?;
    let include_paths = env::var("ATTO_EDITOR_SUBLIME_SYNTAX_INCLUDE_PATHS")
        .ok()
        .map(|v| {
            v.split(':')
                .filter(|s| !s.trim().is_empty())
                .map(std::path::PathBuf::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Some(EditorSyntaxConfig::Sublime {
        syntax_file: syntax_file.into(),
        include_paths,
    })
}
