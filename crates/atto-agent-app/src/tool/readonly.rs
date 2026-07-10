//! Built-in read-only tools for workspace-scoped inspection.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde_json::{Value, json};
use walkdir::WalkDir;

use super::{
    ToolArgs, ToolContext, ToolExecutor, ToolOutputKind, ToolPermission, ToolRegistry, ToolResult,
    ToolSpec, canonical_workspace_root, display_workspace_path, is_workspace_path,
    resolve_existing_workspace_path,
};

const READ_FILE_MAX_BYTES: u64 = 256 * 1024;
const SEARCH_FILE_MAX_BYTES: u64 = 256 * 1024;
const DEFAULT_LIST_MAX_RESULTS: usize = 200;
const MAX_LIST_RESULTS: usize = 1_000;
const DEFAULT_SEARCH_MAX_RESULTS: usize = 50;
const MAX_SEARCH_RESULTS: usize = 200;
const DEFAULT_GLOB_PATTERN: &str = "**/*";
const SEARCH_LINE_PREVIEW_CHARS: usize = 240;

/// Registers the built-in read-only tools available before approval support lands.
pub fn register_readonly_tools(registry: &mut ToolRegistry) -> Result<()> {
    registry.register(ReadFileTool)?;
    registry.register(ListFilesTool)?;
    registry.register(SearchTextTool)?;
    Ok(())
}

/// Builds a registry containing only the built-in read-only tools.
pub fn readonly_tool_registry() -> Result<ToolRegistry> {
    let mut registry = ToolRegistry::new();
    register_readonly_tools(&mut registry)?;
    Ok(registry)
}

#[derive(Clone, Copy, Debug)]
struct ReadFileTool;

impl ToolExecutor for ReadFileTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "read_file",
            format!(
                "Read a UTF-8 text file under the workspace root, up to {} KiB.",
                READ_FILE_MAX_BYTES / 1024
            ),
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative or absolute path to an existing file inside the workspace."
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            ToolPermission::AlwaysAllow,
            ToolOutputKind::Markdown,
        )
        .expect("built-in read_file spec must be valid")
    }

    fn execute(&self, ctx: ToolContext, args: Value) -> Result<ToolResult> {
        let args = ToolArgs::parse("read_file", args, &["path"])?;
        let requested_path = args.required_string("path")?;
        let workspace_root = canonical_workspace_root(&ctx)?;
        let path = resolve_existing_workspace_path(&workspace_root, requested_path)?;
        let metadata = fs::metadata(&path)
            .with_context(|| format!("failed to read metadata for `{}`", path.display()))?;
        if !metadata.is_file() {
            bail!("read_file path `{}` is not a file", path.display());
        }
        if metadata.len() > READ_FILE_MAX_BYTES {
            bail!(
                "read_file path `{}` is {} bytes, exceeding the {} byte limit",
                path.display(),
                metadata.len(),
                READ_FILE_MAX_BYTES
            );
        }

        let bytes =
            fs::read(&path).with_context(|| format!("failed to read `{}`", path.display()))?;
        if bytes.len() as u64 > READ_FILE_MAX_BYTES {
            bail!(
                "read_file path `{}` exceeded the {} byte limit while reading",
                path.display(),
                READ_FILE_MAX_BYTES
            );
        }
        let byte_len = bytes.len();
        let text = String::from_utf8(bytes)
            .with_context(|| format!("read_file path `{}` is not valid UTF-8", path.display()))?;

        Ok(ToolResult::success(
            format_read_file_output(&workspace_root, &path, byte_len, &text),
            ToolOutputKind::Markdown,
        ))
    }
}

#[derive(Clone, Copy, Debug)]
struct ListFilesTool;

impl ToolExecutor for ListFilesTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "list_files",
            "List files under the workspace root using a glob pattern.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative or absolute directory/file path to list from. Defaults to the workspace root."
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern matched against workspace-relative paths. Defaults to the recursive pattern **/*."
                    },
                    "max_results": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": MAX_LIST_RESULTS,
                        "description": "Maximum number of paths to return."
                    }
                },
                "additionalProperties": false
            }),
            ToolPermission::AlwaysAllow,
            ToolOutputKind::Markdown,
        )
        .expect("built-in list_files spec must be valid")
    }

    fn execute(&self, ctx: ToolContext, args: Value) -> Result<ToolResult> {
        let args = ToolArgs::parse("list_files", args, &["path", "pattern", "max_results"])?;
        let requested_path = args.optional_string("path")?.unwrap_or(".");
        let pattern = args
            .optional_string("pattern")?
            .unwrap_or(DEFAULT_GLOB_PATTERN);
        let max_results =
            args.optional_usize("max_results", DEFAULT_LIST_MAX_RESULTS, MAX_LIST_RESULTS)?;
        let workspace_root = canonical_workspace_root(&ctx)?;
        let start = resolve_existing_workspace_path(&workspace_root, requested_path)?;
        let files = collect_workspace_files(&workspace_root, &start, pattern, max_results)?;

        Ok(ToolResult::success(
            format_file_list_output(&workspace_root, &files, max_results),
            ToolOutputKind::Markdown,
        ))
    }
}

#[derive(Clone, Copy, Debug)]
struct SearchTextTool;

impl ToolExecutor for SearchTextTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "search_text",
            "Search UTF-8 files under the workspace root and return matching line summaries.",
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Literal text to search for."
                    },
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative or absolute directory/file path to search from. Defaults to the workspace root."
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern matched against workspace-relative paths. Defaults to the recursive pattern **/*."
                    },
                    "case_sensitive": {
                        "type": "boolean",
                        "description": "Whether matching is case-sensitive. Defaults to true."
                    },
                    "max_results": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": MAX_SEARCH_RESULTS,
                        "description": "Maximum number of matching lines to return."
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            ToolPermission::AlwaysAllow,
            ToolOutputKind::Markdown,
        )
        .expect("built-in search_text spec must be valid")
    }

    fn execute(&self, ctx: ToolContext, args: Value) -> Result<ToolResult> {
        let args = ToolArgs::parse(
            "search_text",
            args,
            &["query", "path", "pattern", "case_sensitive", "max_results"],
        )?;
        let query = args.required_string("query")?;
        let requested_path = args.optional_string("path")?.unwrap_or(".");
        let pattern = args
            .optional_string("pattern")?
            .unwrap_or(DEFAULT_GLOB_PATTERN);
        let case_sensitive = args.optional_bool("case_sensitive", true)?;
        let max_results = args.optional_usize(
            "max_results",
            DEFAULT_SEARCH_MAX_RESULTS,
            MAX_SEARCH_RESULTS,
        )?;
        let workspace_root = canonical_workspace_root(&ctx)?;
        let start = resolve_existing_workspace_path(&workspace_root, requested_path)?;
        let files = collect_workspace_files(&workspace_root, &start, pattern, usize::MAX)?;
        let matches = search_files(&workspace_root, &files, query, case_sensitive, max_results)?;

        Ok(ToolResult::success(
            format_search_output(query, &matches, max_results),
            ToolOutputKind::Markdown,
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SearchMatch {
    path: String,
    line_number: usize,
    line: String,
}

fn collect_workspace_files(
    workspace_root: &Path,
    start: &Path,
    pattern: &str,
    max_results: usize,
) -> Result<Vec<PathBuf>> {
    let glob = compile_relative_glob(pattern)?;
    let mut files = BTreeSet::new();
    for entry in WalkDir::new(start).follow_links(false).sort_by_file_name() {
        let entry = entry.with_context(|| format!("failed to walk `{}`", start.display()))?;
        let Ok(path) = entry.path().canonicalize() else {
            continue;
        };
        if !is_workspace_path(workspace_root, &path) {
            continue;
        }
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let relative = path.strip_prefix(workspace_root).with_context(|| {
            format!(
                "path `{}` should be inside workspace `{}`",
                path.display(),
                workspace_root.display()
            )
        })?;
        if glob.is_match(relative) {
            files.insert(path);
            if files.len() >= max_results {
                break;
            }
        }
    }
    Ok(files.into_iter().collect())
}

fn compile_relative_glob(pattern: &str) -> Result<GlobSet> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        bail!("glob pattern must not be empty");
    }
    if Path::new(pattern).is_absolute() {
        bail!("glob pattern `{pattern}` must be relative to the workspace");
    }

    let mut builder = GlobSetBuilder::new();
    add_glob(&mut builder, pattern)?;
    if !pattern.contains('/') && !pattern.contains('\\') {
        add_glob(&mut builder, &format!("**/{pattern}"))?;
    }
    builder
        .build()
        .with_context(|| format!("failed to compile glob pattern `{pattern}`"))
}

fn add_glob(builder: &mut GlobSetBuilder, pattern: &str) -> Result<()> {
    let glob = Glob::new(pattern).with_context(|| format!("invalid glob pattern `{pattern}`"))?;
    builder.add(glob);
    Ok(())
}

fn search_files(
    workspace_root: &Path,
    files: &[PathBuf],
    query: &str,
    case_sensitive: bool,
    max_results: usize,
) -> Result<Vec<SearchMatch>> {
    let needle = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };
    let mut matches = Vec::new();
    for path in files {
        if matches.len() >= max_results {
            break;
        }
        let metadata = fs::metadata(path)
            .with_context(|| format!("failed to read metadata for `{}`", path.display()))?;
        if metadata.len() > SEARCH_FILE_MAX_BYTES {
            continue;
        }
        let bytes =
            fs::read(path).with_context(|| format!("failed to read `{}`", path.display()))?;
        if bytes.len() as u64 > SEARCH_FILE_MAX_BYTES {
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        for (line_index, line) in text.lines().enumerate() {
            let haystack = if case_sensitive {
                line.to_string()
            } else {
                line.to_lowercase()
            };
            if haystack.contains(&needle) {
                matches.push(SearchMatch {
                    path: display_workspace_path(workspace_root, path),
                    line_number: line_index + 1,
                    line: preview_line(line),
                });
                if matches.len() >= max_results {
                    break;
                }
            }
        }
    }
    Ok(matches)
}

fn format_read_file_output(
    workspace_root: &Path,
    path: &Path,
    byte_len: usize,
    text: &str,
) -> String {
    format!(
        "Path: `{}`\nBytes: {byte_len}\n\n{text}",
        display_workspace_path(workspace_root, path)
    )
}

fn format_file_list_output(workspace_root: &Path, files: &[PathBuf], max_results: usize) -> String {
    if files.is_empty() {
        return "No files found.".to_string();
    }

    let mut output = if files.len() >= max_results {
        format!("Found {} file(s), limited to {max_results}:\n", files.len())
    } else {
        format!("Found {} file(s):\n", files.len())
    };
    for path in files {
        output.push_str("- ");
        output.push_str(&display_workspace_path(workspace_root, path));
        output.push('\n');
    }
    output
}

fn format_search_output(query: &str, matches: &[SearchMatch], max_results: usize) -> String {
    if matches.is_empty() {
        return format!("No matches found for {:?}.", query);
    }

    let mut output = if matches.len() >= max_results {
        format!(
            "Found {} match(es) for {:?}, limited to {max_results}:\n",
            matches.len(),
            query
        )
    } else {
        format!("Found {} match(es) for {:?}:\n", matches.len(), query)
    };
    for matched in matches {
        output.push_str(&format!(
            "- {}:{}: {}\n",
            matched.path, matched.line_number, matched.line
        ));
    }
    output
}

fn preview_line(line: &str) -> String {
    let trimmed = line.trim();
    let mut preview = trimmed
        .chars()
        .take(SEARCH_LINE_PREVIEW_CHARS)
        .collect::<String>();
    if trimmed.chars().count() > SEARCH_LINE_PREVIEW_CHARS {
        preview.push_str("...");
    }
    preview
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        let dir = env::temp_dir().join(format!(
            "atto-agent-tool-{name}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("failed to create test dir");
        dir
    }

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create parent dir");
        }
        fs::write(path, text).expect("failed to write test fixture");
    }

    #[test]
    fn readonly_registry_registers_builtin_tools_in_name_order() {
        let registry = readonly_tool_registry().unwrap();

        let names = registry
            .specs()
            .map(|spec| spec.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["list_files", "read_file", "search_text"]);
        assert!(registry.specs().all(|spec| {
            spec.permission == ToolPermission::AlwaysAllow
                && spec.output == ToolOutputKind::Markdown
        }));
        let chat_tool_names = registry
            .chat_tools()
            .into_iter()
            .map(|tool| tool.function.name)
            .collect::<Vec<_>>();
        assert_eq!(
            chat_tool_names,
            vec!["list_files", "read_file", "search_text"]
        );
    }

    #[test]
    fn read_file_reads_utf8_files_inside_workspace() {
        let root = test_dir("read-file");
        write(&root.join("src/lib.rs"), "pub fn answer() -> i32 { 42 }\n");
        let registry = readonly_tool_registry().unwrap();

        let result = registry
            .execute(
                "read_file",
                ToolContext::new(root),
                json!({ "path": "src/lib.rs" }),
            )
            .unwrap();

        assert!(result.ok);
        assert_eq!(result.output_kind, ToolOutputKind::Markdown);
        assert!(result.output.contains("Path: `src/lib.rs`"));
        assert!(result.output.contains("pub fn answer()"));
    }

    #[test]
    fn read_file_rejects_workspace_escape_and_oversized_files() {
        let root = test_dir("read-bounds-root");
        let outside = test_dir("read-bounds-outside");
        write(&outside.join("secret.txt"), "secret\n");
        fs::write(
            root.join("big.txt"),
            vec![b'a'; READ_FILE_MAX_BYTES as usize + 1],
        )
        .unwrap();
        let registry = readonly_tool_registry().unwrap();

        let escaped = registry
            .execute(
                "read_file",
                ToolContext::new(root.clone()),
                json!({ "path": outside.join("secret.txt").to_str().unwrap() }),
            )
            .unwrap_err();
        assert!(escaped.to_string().contains("escapes workspace"));

        let oversized = registry
            .execute(
                "read_file",
                ToolContext::new(root),
                json!({ "path": "big.txt" }),
            )
            .unwrap_err();
        assert!(oversized.to_string().contains("exceeding"));
    }

    #[cfg(unix)]
    #[test]
    fn read_file_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = test_dir("symlink-root");
        let outside = test_dir("symlink-outside");
        write(&outside.join("secret.txt"), "secret\n");
        symlink(outside.join("secret.txt"), root.join("secret-link.txt")).unwrap();
        let registry = readonly_tool_registry().unwrap();

        let error = registry
            .execute(
                "read_file",
                ToolContext::new(root),
                json!({ "path": "secret-link.txt" }),
            )
            .unwrap_err();

        assert!(error.to_string().contains("escapes workspace"));
    }

    #[test]
    fn list_files_returns_globbed_workspace_relative_paths() {
        let root = test_dir("list-files");
        write(&root.join("src/lib.rs"), "lib\n");
        write(&root.join("src/main.txt"), "main\n");
        write(&root.join("nested/mod.rs"), "mod\n");
        write(&root.join("README.md"), "readme\n");
        let registry = readonly_tool_registry().unwrap();

        let result = registry
            .execute(
                "list_files",
                ToolContext::new(root),
                json!({ "pattern": "*.rs", "max_results": 10 }),
            )
            .unwrap();

        assert!(result.output.contains("- nested/mod.rs"));
        assert!(result.output.contains("- src/lib.rs"));
        assert!(!result.output.contains("README.md"));
        assert!(!result.output.contains("src/main.txt"));
    }

    #[test]
    fn search_text_returns_matching_line_summaries() {
        let root = test_dir("search-text");
        write(&root.join("src/lib.rs"), "Hello world\nhello again\n");
        write(&root.join("README.md"), "HELLO docs\n");
        let registry = readonly_tool_registry().unwrap();

        let result = registry
            .execute(
                "search_text",
                ToolContext::new(root),
                json!({
                    "query": "hello",
                    "pattern": "*.rs",
                    "case_sensitive": false,
                    "max_results": 5
                }),
            )
            .unwrap();

        assert!(result.output.contains("src/lib.rs:1: Hello world"));
        assert!(result.output.contains("src/lib.rs:2: hello again"));
        assert!(!result.output.contains("README.md"));
    }

    #[test]
    fn readonly_tools_reject_invalid_arguments() {
        let root = test_dir("invalid-args");
        write(&root.join("src/lib.rs"), "lib\n");
        let registry = readonly_tool_registry().unwrap();

        let wrong_type = registry
            .execute(
                "read_file",
                ToolContext::new(root.clone()),
                json!({ "path": 7 }),
            )
            .unwrap_err();
        assert!(wrong_type.to_string().contains("must be a string"));

        let unknown = registry
            .execute(
                "list_files",
                ToolContext::new(root.clone()),
                json!({ "path": ".", "surprise": true }),
            )
            .unwrap_err();
        assert!(unknown.to_string().contains("unknown argument"));

        let empty_query = registry
            .execute(
                "search_text",
                ToolContext::new(root),
                json!({ "query": " " }),
            )
            .unwrap_err();
        assert!(empty_query.to_string().contains("must not be empty"));
    }
}
