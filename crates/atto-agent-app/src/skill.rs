//! Skill file parsing and discovery for local prompt packages.
//!
//! Skills are Markdown instruction files with YAML frontmatter. This module owns
//! deterministic discovery from the default workspace and user skill roots;
//! activation, matching, and prompt injection are implemented in later M4 tasks.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use walkdir::WalkDir;

pub const WORKSPACE_SKILLS_DIR: &str = ".atto/skills";
pub const USER_SKILLS_DIR: &str = ".config/atto-agent/skills";

const SKILL_FILE_NAME: &str = "SKILL.md";

/// Parsed contents of one `SKILL.md` file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub tools: Vec<String>,
    pub mode: SkillMode,
    pub body: String,
}

impl SkillDefinition {
    /// Parses a `SKILL.md` document from an in-memory Markdown string.
    pub fn parse_markdown(markdown: &str) -> Result<Self> {
        parse_skill_markdown(markdown)
    }

    fn from_parts(frontmatter: SkillFrontmatter, body: &str) -> Result<Self> {
        let name = normalize_required_string("name", frontmatter.name)?;
        validate_skill_name(&name)?;
        let description = normalize_required_string("description", frontmatter.description)?;
        let triggers = normalize_string_list("triggers", frontmatter.triggers)?;
        let tools = normalize_string_list("tools", frontmatter.tools)?;
        for tool in &tools {
            validate_tool_preference(tool)?;
        }
        if body.trim().is_empty() {
            bail!("skill `{name}` body must not be empty");
        }

        Ok(Self {
            name,
            description,
            triggers,
            tools,
            mode: frontmatter.mode,
            body: body.to_string(),
        })
    }
}

/// Controls whether a skill is eligible for automatic loading.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SkillMode {
    #[default]
    Manual,
    Auto,
}

impl fmt::Display for SkillMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Manual => "manual",
            Self::Auto => "auto",
        })
    }
}

impl FromStr for SkillMode {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "manual" => Ok(Self::Manual),
            "auto" => Ok(Self::Auto),
            _ => bail!("invalid skill mode `{value}`; expected manual or auto"),
        }
    }
}

/// Default skill roots are searched in this order, so workspace skills override user skills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillSourceKind {
    Workspace,
    User,
}

impl fmt::Display for SkillSourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Workspace => "workspace",
            Self::User => "user",
        })
    }
}

/// One concrete directory scanned for `SKILL.md` files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillSearchPath {
    pub kind: SkillSourceKind,
    pub root: PathBuf,
}

impl SkillSearchPath {
    pub fn workspace(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            kind: SkillSourceKind::Workspace,
            root: workspace_root.as_ref().join(WORKSPACE_SKILLS_DIR),
        }
    }

    pub fn user(home_dir: impl AsRef<Path>) -> Self {
        Self {
            kind: SkillSourceKind::User,
            root: home_dir.as_ref().join(USER_SKILLS_DIR),
        }
    }
}

/// A parsed skill together with its source metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredSkill {
    pub definition: SkillDefinition,
    pub path: PathBuf,
    pub source: SkillSourceKind,
}

/// Non-fatal discovery issues. Invalid files do not prevent other skills from loading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillDiscoveryIssue {
    InvalidDirectory {
        path: PathBuf,
        message: String,
    },
    InvalidFile {
        path: PathBuf,
        message: String,
    },
    DuplicateName {
        name: String,
        kept_path: PathBuf,
        skipped_path: PathBuf,
    },
}

impl fmt::Display for SkillDiscoveryIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDirectory { path, message } => {
                write!(f, "invalid skill directory `{}`: {message}", path.display())
            }
            Self::InvalidFile { path, message } => {
                write!(f, "invalid skill file `{}`: {message}", path.display())
            }
            Self::DuplicateName {
                name,
                kept_path,
                skipped_path,
            } => write!(
                f,
                "duplicate skill `{name}` skipped `{}`; kept `{}`",
                skipped_path.display(),
                kept_path.display()
            ),
        }
    }
}

/// Deterministic in-memory registry of discovered skills and non-fatal scan issues.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkillRegistry {
    skills: BTreeMap<String, DiscoveredSkill>,
    issues: Vec<SkillDiscoveryIssue>,
}

impl SkillRegistry {
    /// Scans the default workspace and optional user roots for `SKILL.md` files.
    pub fn discover(workspace_root: impl AsRef<Path>, home_dir: Option<&Path>) -> Self {
        let paths = default_skill_search_paths(workspace_root.as_ref(), home_dir);
        Self::discover_from_paths(&paths)
    }

    /// Scans explicit roots in order. The first valid skill for a name wins.
    pub fn discover_from_paths(paths: &[SkillSearchPath]) -> Self {
        let mut registry = Self::default();
        for search_path in paths {
            scan_skill_path(&mut registry, search_path);
        }
        registry
    }

    pub fn len(&self) -> usize {
        self.skills.len()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&DiscoveredSkill> {
        self.skills.get(name)
    }

    pub fn skills(&self) -> impl Iterator<Item = &DiscoveredSkill> {
        self.skills.values()
    }

    pub fn issues(&self) -> &[SkillDiscoveryIssue] {
        &self.issues
    }

    fn insert(&mut self, skill: DiscoveredSkill) {
        let name = skill.definition.name.clone();
        if let Some(existing) = self.skills.get(&name) {
            self.issues.push(SkillDiscoveryIssue::DuplicateName {
                name,
                kept_path: existing.path.clone(),
                skipped_path: skill.path,
            });
            return;
        }
        self.skills.insert(name, skill);
    }

    fn push_issue(&mut self, issue: SkillDiscoveryIssue) {
        self.issues.push(issue);
    }
}

/// Builds the default search path list from a workspace and optional home directory.
pub fn default_skill_search_paths(
    workspace_root: &Path,
    home_dir: Option<&Path>,
) -> Vec<SkillSearchPath> {
    let mut paths = vec![SkillSearchPath::workspace(workspace_root)];
    if let Some(home_dir) = home_dir {
        paths.push(SkillSearchPath::user(home_dir));
    }
    paths
}

/// Reads and parses a UTF-8 `SKILL.md` file from disk.
pub fn parse_skill_file(path: impl AsRef<Path>) -> Result<SkillDefinition> {
    let path = path.as_ref();
    let markdown = fs::read_to_string(path)
        .with_context(|| format!("failed to read skill file `{}`", path.display()))?;
    parse_skill_markdown(&markdown)
        .with_context(|| format!("failed to parse skill file `{}`", path.display()))
}

/// Parses a `SKILL.md` document into validated frontmatter fields and body text.
pub fn parse_skill_markdown(markdown: &str) -> Result<SkillDefinition> {
    let (frontmatter, body) = split_frontmatter(markdown)?;
    let frontmatter = serde_yaml::from_str::<SkillFrontmatter>(frontmatter)
        .context("failed to parse skill frontmatter")?;
    SkillDefinition::from_parts(frontmatter, body)
}

fn scan_skill_path(registry: &mut SkillRegistry, search_path: &SkillSearchPath) {
    match fs::metadata(&search_path.root) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            registry.push_issue(SkillDiscoveryIssue::InvalidDirectory {
                path: search_path.root.clone(),
                message: "path is not a directory".to_string(),
            });
            return;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            registry.push_issue(SkillDiscoveryIssue::InvalidDirectory {
                path: search_path.root.clone(),
                message: error.to_string(),
            });
            return;
        }
    }

    let mut skill_files = Vec::new();
    for entry in WalkDir::new(&search_path.root).follow_links(false) {
        match entry {
            Ok(entry) => {
                if entry.file_type().is_file() && entry.file_name() == OsStr::new(SKILL_FILE_NAME) {
                    skill_files.push(entry.path().to_path_buf());
                }
            }
            Err(error) => {
                registry.push_issue(SkillDiscoveryIssue::InvalidDirectory {
                    path: error
                        .path()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| search_path.root.clone()),
                    message: error.to_string(),
                });
            }
        }
    }
    skill_files.sort();

    for path in skill_files {
        match parse_skill_file(&path) {
            Ok(definition) => registry.insert(DiscoveredSkill {
                definition,
                path,
                source: search_path.kind,
            }),
            Err(error) => registry.push_issue(SkillDiscoveryIssue::InvalidFile {
                path,
                message: format!("{error:#}"),
            }),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    triggers: Vec<String>,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    mode: SkillMode,
}

fn split_frontmatter(markdown: &str) -> Result<(&str, &str)> {
    let markdown = markdown.strip_prefix('\u{feff}').unwrap_or(markdown);
    let mut lines = markdown.split_inclusive('\n');
    let first = lines
        .next()
        .context("skill file must start with YAML frontmatter delimiter `---`")?;
    if !is_frontmatter_delimiter(first) {
        bail!("skill file must start with YAML frontmatter delimiter `---`");
    }

    let frontmatter_start = first.len();
    let mut cursor = frontmatter_start;
    for line in lines {
        let line_start = cursor;
        cursor += line.len();
        if is_frontmatter_delimiter(line) {
            return Ok((
                &markdown[frontmatter_start..line_start],
                &markdown[cursor..],
            ));
        }
    }

    bail!("skill frontmatter must end with delimiter `---`")
}

fn is_frontmatter_delimiter(line: &str) -> bool {
    let line = line.strip_suffix('\n').unwrap_or(line);
    let line = line.strip_suffix('\r').unwrap_or(line);
    line == "---"
}

fn normalize_required_string(field: &str, value: String) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("skill frontmatter field `{field}` must not be empty");
    }
    Ok(value.to_string())
}

fn normalize_string_list(field: &str, values: Vec<String>) -> Result<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::with_capacity(values.len());
    for (index, value) in values.into_iter().enumerate() {
        let value = value.trim();
        if value.is_empty() {
            bail!("skill frontmatter field `{field}` item {index} must not be empty");
        }
        if !seen.insert(value.to_string()) {
            bail!("skill frontmatter field `{field}` contains duplicate `{value}`");
        }
        result.push(value.to_string());
    }
    Ok(result)
}

fn validate_skill_name(name: &str) -> Result<()> {
    validate_identifier("skill name", name)
}

fn validate_tool_preference(tool: &str) -> Result<()> {
    validate_identifier("skill tool preference", tool)
}

fn validate_identifier(kind: &str, value: &str) -> Result<()> {
    if value.is_empty() || value.trim() != value {
        bail!("{kind} must not be empty or contain surrounding whitespace");
    }
    if value.len() > 64 {
        bail!("{kind} `{value}` must be at most 64 bytes");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        bail!("{kind} `{value}` may only contain ASCII letters, digits, `_`, or `-`");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn valid_skill() -> &'static str {
        r#"---
name: rust-review
description: Review Rust code for correctness, safety, tests, and API regressions.
triggers: ["review", "rust", "clippy"]
tools: ["read_file", "search_text"]
mode: auto
---

When this skill is active, inspect changed Rust files first.
Prefer tests that reproduce behavioral regressions.
"#
    }

    fn skill_markdown(name: &str, description: &str) -> String {
        format!(
            r#"---
name: {name}
description: {description}
---

Use this skill for {name} tasks.
"#
        )
    }

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        let dir = env::temp_dir().join(format!(
            "atto-agent-skill-{name}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("failed to create test dir");
        dir
    }

    fn write(path: &Path, text: impl AsRef<str>) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create parent dir");
        }
        fs::write(path, text.as_ref()).expect("failed to write test fixture");
    }

    #[test]
    fn parses_frontmatter_and_body() {
        let skill = parse_skill_markdown(valid_skill()).unwrap();

        assert_eq!(skill.name, "rust-review");
        assert_eq!(
            skill.description,
            "Review Rust code for correctness, safety, tests, and API regressions."
        );
        assert_eq!(skill.triggers, vec!["review", "rust", "clippy"]);
        assert_eq!(skill.tools, vec!["read_file", "search_text"]);
        assert_eq!(skill.mode, SkillMode::Auto);
        assert_eq!(
            skill.body,
            "\nWhen this skill is active, inspect changed Rust files first.\nPrefer tests that reproduce behavioral regressions.\n"
        );
    }

    #[test]
    fn defaults_optional_lists_and_mode() {
        let skill = parse_skill_markdown(
            r#"---
name: docs
description: Write clear documentation.
---
Document public behavior.
"#,
        )
        .unwrap();

        assert_eq!(skill.triggers, Vec::<String>::new());
        assert_eq!(skill.tools, Vec::<String>::new());
        assert_eq!(skill.mode, SkillMode::Manual);
        assert_eq!(skill.body, "Document public behavior.\n");
    }

    #[test]
    fn trims_metadata_items_without_trimming_body() {
        let skill = parse_skill_markdown(
            r#"---
name: " rust "
description: " Review Rust. "
triggers: [" review "]
tools: [" read_file "]
mode: manual
---
  Keep indentation in the body.
"#,
        )
        .unwrap();

        assert_eq!(skill.name, "rust");
        assert_eq!(skill.description, "Review Rust.");
        assert_eq!(skill.triggers, vec!["review"]);
        assert_eq!(skill.tools, vec!["read_file"]);
        assert_eq!(skill.body, "  Keep indentation in the body.\n");
    }

    #[test]
    fn rejects_missing_frontmatter_delimiters() {
        let error = parse_skill_markdown("name: rust\n---\nbody").unwrap_err();

        assert!(error.to_string().contains("must start"));
    }

    #[test]
    fn rejects_missing_required_fields() {
        let error = parse_skill_markdown(
            r#"---
name: rust
---
Body.
"#,
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("description"));
    }

    #[test]
    fn rejects_unknown_mode() {
        let error = parse_skill_markdown(
            r#"---
name: rust
description: Review Rust.
mode: sometimes
---
Body.
"#,
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("mode"));
    }

    #[test]
    fn rejects_invalid_name_and_tool_preference() {
        let bad_name = parse_skill_markdown(
            r#"---
name: "rust review"
description: Review Rust.
---
Body.
"#,
        )
        .unwrap_err();
        assert!(bad_name.to_string().contains("skill name"));

        let bad_tool = parse_skill_markdown(
            r#"---
name: rust
description: Review Rust.
tools: ["run command"]
---
Body.
"#,
        )
        .unwrap_err();
        assert!(bad_tool.to_string().contains("tool preference"));
    }

    #[test]
    fn rejects_duplicate_list_items_and_empty_body() {
        let duplicate = parse_skill_markdown(
            r#"---
name: rust
description: Review Rust.
triggers: ["review", "review"]
---
Body.
"#,
        )
        .unwrap_err();
        assert!(duplicate.to_string().contains("duplicate"));

        let empty_body = parse_skill_markdown(
            r#"---
name: rust
description: Review Rust.
---

"#,
        )
        .unwrap_err();
        assert!(empty_body.to_string().contains("body"));
    }

    #[test]
    fn reads_skill_file_from_disk() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        let dir = env::temp_dir().join(format!("atto-agent-skill-{}-{unique}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("SKILL.md");
        fs::write(&path, valid_skill()).unwrap();

        let skill = parse_skill_file(&path).unwrap();

        assert_eq!(skill.name, "rust-review");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn discovery_ignores_missing_default_roots() {
        let workspace = test_dir("discover-missing-workspace");
        let home = test_dir("discover-missing-home");

        let registry = SkillRegistry::discover(&workspace, Some(home.as_path()));

        assert!(registry.is_empty());
        assert!(registry.issues().is_empty());
        let _ = fs::remove_dir_all(workspace);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn discovers_workspace_and_user_skill_files() {
        let workspace = test_dir("discover-workspace");
        let home = test_dir("discover-home");
        write(
            &workspace.join(".atto/skills/rust-review/SKILL.md"),
            skill_markdown("rust-review", "Review Rust code."),
        );
        write(
            &home.join(".config/atto-agent/skills/docs/SKILL.md"),
            skill_markdown("docs", "Write docs."),
        );
        write(&workspace.join(".atto/skills/README.md"), "ignored");

        let registry = SkillRegistry::discover(&workspace, Some(home.as_path()));

        assert_eq!(registry.len(), 2);
        assert!(registry.issues().is_empty());
        let names = registry
            .skills()
            .map(|skill| skill.definition.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["docs", "rust-review"]);
        assert_eq!(
            registry.get("rust-review").map(|skill| skill.source),
            Some(SkillSourceKind::Workspace)
        );
        assert_eq!(
            registry.get("docs").map(|skill| skill.source),
            Some(SkillSourceKind::User)
        );
        let _ = fs::remove_dir_all(workspace);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn discovery_keeps_first_duplicate_name_and_records_issue() {
        let workspace = test_dir("discover-duplicate-workspace");
        let home = test_dir("discover-duplicate-home");
        let workspace_path = workspace.join(".atto/skills/shared/SKILL.md");
        let user_path = home.join(".config/atto-agent/skills/shared/SKILL.md");
        write(
            &workspace_path,
            skill_markdown("shared", "Workspace version."),
        );
        write(&user_path, skill_markdown("shared", "User version."));

        let registry = SkillRegistry::discover(&workspace, Some(home.as_path()));

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("shared").unwrap().path, workspace_path);
        assert_eq!(
            registry.issues(),
            &[SkillDiscoveryIssue::DuplicateName {
                name: "shared".to_string(),
                kept_path: registry.get("shared").unwrap().path.clone(),
                skipped_path: user_path,
            }]
        );
        let _ = fs::remove_dir_all(workspace);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn discovery_records_invalid_files_without_failing_scan() {
        let workspace = test_dir("discover-invalid-workspace");
        write(
            &workspace.join(".atto/skills/good/SKILL.md"),
            skill_markdown("good", "Valid skill."),
        );
        write(
            &workspace.join(".atto/skills/bad/SKILL.md"),
            r#"---
name: bad
---
Body.
"#,
        );

        let registry = SkillRegistry::discover(&workspace, None);

        assert_eq!(registry.len(), 1);
        assert!(registry.get("good").is_some());
        assert_eq!(registry.issues().len(), 1);
        match &registry.issues()[0] {
            SkillDiscoveryIssue::InvalidFile { path, message } => {
                assert!(path.ends_with("SKILL.md"));
                assert!(message.contains("description"));
            }
            other => panic!("expected invalid file issue, got {other:?}"),
        }
        let _ = fs::remove_dir_all(workspace);
    }
}
