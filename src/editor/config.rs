use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use crate::reactive::Binding;
use crate::composable::ScrollConfig;

use super::keymap::EditorKeymap;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum EditorSyntaxConfig {
    #[default]
    None,
    /// Regex-based lightweight highlighting (no folding).
    SimpleJson,
    /// Regex-based lightweight highlighting (no folding).
    SimpleIni,
    /// Sublime Text `.sublime-syntax` highlighting + folding.
    Sublime {
        /// Path to the primary `.sublime-syntax` file.
        syntax_file: PathBuf,
        /// Additional search paths for included syntaxes.
        include_paths: Vec<PathBuf>,
    },
}

#[derive(Clone, Debug)]
pub struct EditorIndentConfig {
    pub tab_width: Binding<usize>,
    pub insert_spaces: Binding<bool>,
}

impl Default for EditorIndentConfig {
    fn default() -> Self {
        Self {
            tab_width: 4usize.into(),
            insert_spaces: true.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EditorScrollConfig {
    pub config: Binding<ScrollConfig>,
}

impl Default for EditorScrollConfig {
    fn default() -> Self {
        Self {
            config: ScrollConfig::default().into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EditorHoverConfig {
    pub enabled: Binding<bool>,
    pub delay: Binding<Duration>,
}

impl Default for EditorHoverConfig {
    fn default() -> Self {
        Self {
            enabled: true.into(),
            delay: Duration::from_millis(350).into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EditorCompletionConfig {
    pub enabled: Binding<bool>,
    pub max_items: Binding<usize>,
}

impl Default for EditorCompletionConfig {
    fn default() -> Self {
        Self {
            enabled: true.into(),
            max_items: 64usize.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorLspGotoKind {
    Definition,
    Declaration,
    TypeDefinition,
    Implementation,
    References,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditorLspConfig {
    /// Command to start the LSP server (`program` + args).
    pub command: Vec<String>,
    /// Document URI for the active buffer.
    pub document_uri: String,
    /// LSP `languageId` for the active buffer (e.g. `rust`, `typescript`, ...).
    pub language_id: String,
    /// Root URI used for `initialize` (optional but recommended).
    pub root_uri: Option<String>,
    /// Workspace folders (`initialize.workspaceFolders`), as file URIs.
    pub workspace_folders: Vec<String>,
    /// Timeout waiting for the `initialize` response.
    pub initialize_timeout: Duration,
    /// Enable LSP-derived styles (semantic tokens).
    pub semantic_tokens: bool,
    /// Enable LSP-derived folding ranges.
    pub folding_ranges: bool,
}

impl Default for EditorLspConfig {
    fn default() -> Self {
        Self {
            command: Vec::new(),
            document_uri: String::new(),
            language_id: "plaintext".to_string(),
            root_uri: None,
            workspace_folders: Vec::new(),
            initialize_timeout: Duration::from_secs(3),
            semantic_tokens: true,
            folding_ranges: true,
        }
    }
}

impl EditorLspConfig {
    /// Convenience constructor for `file://`-backed documents.
    pub fn for_file_path(
        file_path: impl AsRef<Path>,
        language_id: impl Into<String>,
        command: Vec<String>,
    ) -> Self {
        let file_path = file_path.as_ref();
        let document_uri = editor_core_lsp::path_to_file_uri(file_path);
        let root_uri = file_path.parent().map(editor_core_lsp::path_to_file_uri);
        let workspace_folders = root_uri.iter().cloned().collect::<Vec<_>>();

        Self {
            command,
            document_uri,
            language_id: language_id.into(),
            root_uri,
            workspace_folders,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum EditorLspMode {
    #[default]
    Disabled,
    Enabled(EditorLspConfig),
}

#[derive(Clone, Debug)]
pub struct EditorConfig {
    /// Backing document text (two-way binding).
    pub text: Binding<String>,
    /// Clipboard backing store (two-way binding).
    pub clipboard: Binding<String>,

    /// Language id used for per-language theming and LSP document open (if enabled).
    pub language_id: Binding<String>,

    /// Optional syntax highlighting provider (when LSP is disabled or as fallback).
    pub syntax: Binding<EditorSyntaxConfig>,

    pub indent: EditorIndentConfig,

    pub show_line_numbers: Binding<bool>,
    pub show_folding_markers: Binding<bool>,

    pub scroll: EditorScrollConfig,

    /// Keyboard shortcuts (single-chord mapping).
    pub keymap: Binding<EditorKeymap>,

    pub hover: EditorHoverConfig,
    pub completion: EditorCompletionConfig,

    pub lsp: Binding<EditorLspMode>,
}

impl EditorConfig {
    pub fn new(text: impl Into<Binding<String>>) -> Self {
        Self {
            text: text.into(),
            clipboard: String::new().into(),
            language_id: "plaintext".into(),
            syntax: EditorSyntaxConfig::None.into(),
            indent: EditorIndentConfig::default(),
            show_line_numbers: true.into(),
            show_folding_markers: true.into(),
            scroll: EditorScrollConfig::default(),
            keymap: EditorKeymap::default().into(),
            hover: EditorHoverConfig::default(),
            completion: EditorCompletionConfig::default(),
            lsp: EditorLspMode::Disabled.into(),
        }
    }
}
