use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use editor_core::{SearchMatch, SearchOptions};
use ignore::WalkBuilder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlobalSearchConfig {
    pub max_total_matches: usize,
    pub max_file_size_bytes: u64,
}

impl Default for GlobalSearchConfig {
    fn default() -> Self {
        Self {
            max_total_matches: 10_000,
            max_file_size_bytes: 2 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalSearchResult {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub text: String,
    pub ranges: Vec<SearchMatch>,
}

pub fn search_workspace(
    roots: &[PathBuf],
    query: &str,
    options: SearchOptions,
    config: GlobalSearchConfig,
) -> Result<Vec<GlobalSearchResult>> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    for root in roots {
        search_root(root, query, options, config, &mut results)?;
        if results.len() >= config.max_total_matches {
            break;
        }
    }
    Ok(results)
}

fn search_root(
    root: &Path,
    query: &str,
    options: SearchOptions,
    config: GlobalSearchConfig,
    results: &mut Vec<GlobalSearchResult>,
) -> Result<()> {
    if !root.exists() {
        bail!("search root does not exist: {}", root.display());
    }
    if !root.is_dir() {
        bail!("search root is not a directory: {}", root.display());
    }

    let walker = WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_exclude(true)
        .ignore(true)
        .require_git(false)
        .parents(true)
        .filter_entry(|entry| {
            let Some(name) = entry.file_name().to_str() else {
                return true;
            };
            !matches!(name, ".git" | "target" | ".build")
        })
        .build();

    for entry in walker {
        let entry = entry.with_context(|| format!("walking {}", root.display()))?;
        if !entry.file_type().is_some_and(|ty| ty.is_file()) {
            continue;
        }

        let path = entry.into_path();
        if let Ok(meta) = std::fs::metadata(&path)
            && meta.len() > config.max_file_size_bytes
        {
            continue;
        }

        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::InvalidData => continue,
            Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
        };
        let text = normalize_newlines_to_lf(&text);
        for (line, line_text) in text.lines().enumerate() {
            let ranges = editor_core::search::find_all(line_text, query, options)
                .with_context(|| format!("searching {}", path.display()))?;
            let Some(first) = ranges.first().copied() else {
                continue;
            };
            results.push(GlobalSearchResult {
                path: path.clone(),
                line,
                column: first.start,
                text: line_text.to_string(),
                ranges,
            });
            if results.len() >= config.max_total_matches {
                return Ok(());
            }
        }
    }

    Ok(())
}

fn normalize_newlines_to_lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("atto_editor_app_search_{prefix}_{nanos}"))
    }

    #[test]
    fn global_search_respects_gitignore_and_finds_matches() -> Result<()> {
        let root = unique_temp_dir("gitignore");
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join(".gitignore"), "ignored.txt\n")?;
        std::fs::write(root.join("ignored.txt"), "TODO ignored\n")?;
        std::fs::write(root.join("keep.txt"), "TODO keep\n")?;

        let results = search_workspace(
            std::slice::from_ref(&root),
            "TODO",
            SearchOptions::default(),
            GlobalSearchConfig::default(),
        )?;

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].path.file_name().and_then(|name| name.to_str()),
            Some("keep.txt")
        );
        assert_eq!(results[0].line, 0);
        assert_eq!(results[0].column, 0);

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn global_search_skips_large_files_and_caps_matches() -> Result<()> {
        let root = unique_temp_dir("limits");
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("large.txt"), format!("TODO{}\n", "x".repeat(64)))?;
        std::fs::write(root.join("a.txt"), "TODO\nTODO\n")?;
        std::fs::write(root.join("b.txt"), "TODO\n")?;

        let results = search_workspace(
            std::slice::from_ref(&root),
            "TODO",
            SearchOptions::default(),
            GlobalSearchConfig {
                max_total_matches: 2,
                max_file_size_bytes: 16,
            },
        )?;

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(
            |result| result.path.file_name().and_then(|name| name.to_str()) != Some("large.txt")
        ));

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn global_search_normalizes_crlf_line_numbers() -> Result<()> {
        let root = unique_temp_dir("crlf");
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("a.txt"), "first\r\nTODO here\r\n")?;

        let results = search_workspace(
            std::slice::from_ref(&root),
            "TODO",
            SearchOptions::default(),
            GlobalSearchConfig::default(),
        )?;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].line, 1);
        assert_eq!(results[0].text, "TODO here");

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn global_search_skips_non_utf8_files_without_failing() -> Result<()> {
        let root = unique_temp_dir("non_utf8");
        std::fs::create_dir_all(&root)?;
        std::fs::write(
            root.join("binary.bin"),
            [0xff, 0xfe, b'T', b'O', b'D', b'O'],
        )?;
        std::fs::write(root.join("keep.txt"), "TODO keep\n")?;

        let results = search_workspace(
            std::slice::from_ref(&root),
            "TODO",
            SearchOptions::default(),
            GlobalSearchConfig::default(),
        )?;

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].path.file_name().and_then(|name| name.to_str()),
            Some("keep.txt")
        );

        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
