//! Skill file parsing and discovery for local prompt packages.
//!
//! Skills are Markdown instruction files with YAML frontmatter. This module owns
//! deterministic discovery from the default workspace and user skill roots, plus
//! runtime tracking for loaded skills, deterministic prompt matching for
//! auto-mode skills, and bounded prompt injection for active skills.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use walkdir::WalkDir;

pub const WORKSPACE_SKILLS_DIR: &str = ".atto/skills";
pub const USER_SKILLS_DIR: &str = ".config/atto-agent/skills";

const SKILL_FILE_NAME: &str = "SKILL.md";

/// Maximum number of skills automatically loaded from one user prompt.
pub const DEFAULT_MAX_AUTO_LOADED_SKILLS: usize = 4;
/// Maximum bytes from one skill body included in the model prompt.
pub const DEFAULT_MAX_SKILL_BODY_BYTES: usize = 6 * 1024;
/// Maximum bytes for the complete `<skills>` prompt block.
pub const DEFAULT_MAX_SKILL_PROMPT_BYTES: usize = 20 * 1024;

const SKILLS_OPEN_TAG: &str = "<skills>\n";
const SKILLS_CLOSE_TAG: &str = "</skills>";

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

/// Byte budget used when rendering loaded skills into a system prompt block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillPromptBudget {
    pub max_skill_body_bytes: usize,
    pub max_total_bytes: usize,
}

impl Default for SkillPromptBudget {
    fn default() -> Self {
        Self {
            max_skill_body_bytes: DEFAULT_MAX_SKILL_BODY_BYTES,
            max_total_bytes: DEFAULT_MAX_SKILL_PROMPT_BYTES,
        }
    }
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

    /// Returns auto-mode skill names whose metadata shares a word with `prompt`.
    pub fn matching_auto_skill_names(
        &self,
        prompt: &str,
        loaded_skills: &LoadedSkillSet,
        limit: usize,
    ) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }

        let prompt_terms = matching_terms(prompt);
        if prompt_terms.is_empty() {
            return Vec::new();
        }

        let mut matches = Vec::new();
        for skill in self.skills.values() {
            if matches.len() >= limit {
                break;
            }
            if skill.definition.mode != SkillMode::Auto {
                continue;
            }
            if loaded_skills.contains(&skill.definition.name) {
                continue;
            }
            if skill_matches_prompt(&skill.definition, &prompt_terms) {
                matches.push(skill.definition.name.clone());
            }
        }
        matches
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

/// Shared runtime state for skills loaded into the current agent session.
#[derive(Clone, Debug, Default)]
pub struct LoadedSkillSet {
    names: Arc<Mutex<BTreeSet<String>>>,
}

impl LoadedSkillSet {
    pub fn len(&self) -> usize {
        self.names.lock().expect("loaded skill lock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, name: &str) -> bool {
        self.names
            .lock()
            .expect("loaded skill lock poisoned")
            .contains(name)
    }

    pub fn insert(&self, name: impl Into<String>) -> bool {
        self.names
            .lock()
            .expect("loaded skill lock poisoned")
            .insert(name.into())
    }

    pub fn names(&self) -> Vec<String> {
        self.names
            .lock()
            .expect("loaded skill lock poisoned")
            .iter()
            .cloned()
            .collect()
    }

    pub fn status(&self) -> String {
        format!("skills: {}", self.len())
    }
}

/// Renders loaded skills as the `<skills>` system prompt block using default limits.
pub fn build_skill_prompt_block(
    registry: &SkillRegistry,
    loaded_skills: &LoadedSkillSet,
) -> Option<String> {
    build_skill_prompt_block_with_budget(registry, loaded_skills, SkillPromptBudget::default())
}

/// Renders loaded skills as a bounded `<skills>` block for insertion into a system prompt.
pub fn build_skill_prompt_block_with_budget(
    registry: &SkillRegistry,
    loaded_skills: &LoadedSkillSet,
    budget: SkillPromptBudget,
) -> Option<String> {
    if budget.max_skill_body_bytes == 0
        || budget.max_total_bytes <= SKILLS_OPEN_TAG.len() + SKILLS_CLOSE_TAG.len()
    {
        return None;
    }

    let mut prompt = String::from(SKILLS_OPEN_TAG);
    for name in loaded_skills.names() {
        let Some(skill) = registry.get(&name) else {
            continue;
        };
        let remaining = budget
            .max_total_bytes
            .saturating_sub(prompt.len() + SKILLS_CLOSE_TAG.len());
        let Some(entry) = build_skill_prompt_entry(skill, budget.max_skill_body_bytes, remaining)
        else {
            continue;
        };
        prompt.push_str(&entry);
    }

    if prompt.len() == SKILLS_OPEN_TAG.len() {
        return None;
    }
    prompt.push_str(SKILLS_CLOSE_TAG);
    Some(prompt)
}

fn build_skill_prompt_entry(
    skill: &DiscoveredSkill,
    max_body_bytes: usize,
    max_entry_bytes: usize,
) -> Option<String> {
    let tool_preferences = if skill.definition.tools.is_empty() {
        String::new()
    } else {
        format!(
            " tools=\"{}\"",
            xml_attr_escape(&skill.definition.tools.join(","))
        )
    };
    let open = format!(
        "<skill name=\"{}\" source=\"{}\"{}>\n",
        xml_attr_escape(&skill.definition.name),
        xml_attr_escape(&skill.path.to_string_lossy()),
        tool_preferences
    );
    let close = "\n</skill>\n";
    let overhead = open.len() + close.len();
    if max_entry_bytes <= overhead {
        return None;
    }

    let body_budget = max_body_bytes.min(max_entry_bytes - overhead);
    let body = truncate_utf8_to_byte_limit(&skill.definition.body, body_budget);
    if body.is_empty() {
        return None;
    }

    let mut entry = open;
    entry.push_str(body);
    entry.push_str(close);
    Some(entry)
}

fn truncate_utf8_to_byte_limit(value: &str, limit: usize) -> &str {
    if value.len() <= limit {
        return value;
    }

    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn xml_attr_escape(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(ch),
        }
    }
    escaped
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

fn skill_matches_prompt(definition: &SkillDefinition, prompt_terms: &BTreeSet<String>) -> bool {
    skill_match_terms(definition)
        .iter()
        .any(|term| prompt_terms.contains(term))
}

fn skill_match_terms(definition: &SkillDefinition) -> BTreeSet<String> {
    let mut terms = matching_terms(&definition.name);
    terms.extend(matching_terms(&definition.description));
    for trigger in &definition.triggers {
        terms.extend(matching_terms(trigger));
    }
    terms
}

fn matching_terms(text: &str) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            current.extend(ch.to_lowercase());
        } else if !current.is_empty() {
            terms.insert(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        terms.insert(current);
    }
    terms
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
        skill_markdown_with_mode(name, description, SkillMode::Manual, &[])
    }

    fn skill_markdown_with_mode(
        name: &str,
        description: &str,
        mode: SkillMode,
        triggers: &[&str],
    ) -> String {
        let triggers = triggers
            .iter()
            .map(|trigger| format!("\"{trigger}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            r#"---
name: {name}
description: {description}
triggers: [{triggers}]
mode: {mode}
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

    fn discovered_skill(name: &str, path: &str, body: &str) -> DiscoveredSkill {
        discovered_skill_with_tools(name, path, body, &[])
    }

    fn discovered_skill_with_tools(
        name: &str,
        path: &str,
        body: &str,
        tools: &[&str],
    ) -> DiscoveredSkill {
        DiscoveredSkill {
            definition: SkillDefinition {
                name: name.to_string(),
                description: format!("Description for {name}."),
                triggers: Vec::new(),
                tools: tools.iter().map(|tool| tool.to_string()).collect(),
                mode: SkillMode::Manual,
                body: body.to_string(),
            },
            path: PathBuf::from(path),
            source: SkillSourceKind::Workspace,
        }
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
    fn discovery_uses_sorted_paths_for_duplicate_names_within_one_root() {
        let workspace = test_dir("discover-duplicate-sorted");
        let kept_path = workspace.join(".atto/skills/a-first/SKILL.md");
        let skipped_path = workspace.join(".atto/skills/z-later/SKILL.md");
        write(
            &skipped_path,
            skill_markdown("shared", "Later path version."),
        );
        write(
            &kept_path,
            skill_markdown("shared", "Earlier path version."),
        );

        let registry = SkillRegistry::discover(&workspace, None);

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("shared").unwrap().path, kept_path);
        assert_eq!(
            registry.issues(),
            &[SkillDiscoveryIssue::DuplicateName {
                name: "shared".to_string(),
                kept_path: registry.get("shared").unwrap().path.clone(),
                skipped_path,
            }]
        );
        let _ = fs::remove_dir_all(workspace);
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

    #[test]
    fn auto_matching_uses_name_description_and_triggers() {
        let workspace = test_dir("auto-match-fields");
        write(
            &workspace.join(".atto/skills/api/SKILL.md"),
            skill_markdown_with_mode(
                "api-audit",
                "Check service contracts.",
                SkillMode::Auto,
                &[],
            ),
        );
        write(
            &workspace.join(".atto/skills/rust/SKILL.md"),
            skill_markdown_with_mode(
                "rust-review",
                "Review implementation details.",
                SkillMode::Auto,
                &["clippy"],
            ),
        );
        write(
            &workspace.join(".atto/skills/tests/SKILL.md"),
            skill_markdown_with_mode("tests", "Design regression coverage.", SkillMode::Auto, &[]),
        );
        write(
            &workspace.join(".atto/skills/manual/SKILL.md"),
            skill_markdown("manual-match", "Audit API behavior."),
        );
        let registry = SkillRegistry::discover(&workspace, None);
        let loaded = LoadedSkillSet::default();

        let matches = registry.matching_auto_skill_names(
            "Please audit the API, run CLIPPY, and cover regression cases.",
            &loaded,
            DEFAULT_MAX_AUTO_LOADED_SKILLS,
        );

        assert_eq!(matches, vec!["api-audit", "rust-review", "tests"]);
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn auto_matching_respects_limit_and_loaded_set() {
        let workspace = test_dir("auto-match-limit");
        for name in ["alpha", "beta", "gamma", "omega"] {
            write(
                &workspace.join(format!(".atto/skills/{name}/SKILL.md")),
                skill_markdown_with_mode(name, "Review Rust code.", SkillMode::Auto, &[]),
            );
        }
        let registry = SkillRegistry::discover(&workspace, None);
        let loaded = LoadedSkillSet::default();
        assert!(loaded.insert("beta"));

        let matches = registry.matching_auto_skill_names("rust review", &loaded, 2);

        assert_eq!(matches, vec!["alpha", "gamma"]);
        assert!(
            registry
                .matching_auto_skill_names("rust review", &loaded, 0)
                .is_empty()
        );
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn skill_prompt_block_renders_loaded_skills() {
        let mut registry = SkillRegistry::default();
        registry.insert(discovered_skill(
            "rust-review",
            ".atto/skills/rust-review/SKILL.md",
            "Inspect Rust changes first.\n",
        ));
        registry.insert(discovered_skill(
            "docs",
            ".atto/skills/docs/SKILL.md",
            "Write clear docs.\n",
        ));
        let loaded = LoadedSkillSet::default();
        assert!(loaded.insert("rust-review"));

        let prompt = build_skill_prompt_block(&registry, &loaded).unwrap();

        assert!(prompt.starts_with("<skills>\n"));
        assert!(prompt.ends_with("</skills>"));
        assert!(prompt.contains(
            "<skill name=\"rust-review\" source=\".atto/skills/rust-review/SKILL.md\">\n"
        ));
        assert!(prompt.contains("Inspect Rust changes first."));
        assert!(!prompt.contains("Write clear docs."));
    }

    #[test]
    fn skill_prompt_block_renders_tool_preferences_as_metadata() {
        let mut registry = SkillRegistry::default();
        registry.insert(discovered_skill_with_tools(
            "shell-helper",
            ".atto/skills/shell-helper/SKILL.md",
            "Prefer command-line diagnostics when useful.\n",
            &["read_file", "run_command"],
        ));
        let loaded = LoadedSkillSet::default();
        assert!(loaded.insert("shell-helper"));

        let prompt = build_skill_prompt_block(&registry, &loaded).unwrap();

        assert!(prompt.contains(
            "<skill name=\"shell-helper\" source=\".atto/skills/shell-helper/SKILL.md\" tools=\"read_file,run_command\">"
        ));
        assert!(prompt.contains("Prefer command-line diagnostics"));
    }

    #[test]
    fn skill_prompt_block_respects_body_and_total_limits() {
        let mut registry = SkillRegistry::default();
        registry.insert(discovered_skill("alpha", "a/SKILL.md", "abcdefghi"));
        registry.insert(discovered_skill("beta", "b/SKILL.md", "beta body"));
        let loaded = LoadedSkillSet::default();
        assert!(loaded.insert("alpha"));
        assert!(loaded.insert("beta"));
        let budget = SkillPromptBudget {
            max_skill_body_bytes: 5,
            max_total_bytes: 100,
        };

        let prompt = build_skill_prompt_block_with_budget(&registry, &loaded, budget).unwrap();

        assert!(prompt.len() <= budget.max_total_bytes);
        assert!(prompt.contains("abcde"));
        assert!(!prompt.contains("abcdef"));
        assert!(prompt.contains("name=\"alpha\""));
        assert!(!prompt.contains("name=\"beta\""));
    }

    #[test]
    fn skill_prompt_truncation_preserves_utf8_boundaries() {
        assert_eq!(truncate_utf8_to_byte_limit("a\u{00e9}b", 2), "a");
        assert_eq!(truncate_utf8_to_byte_limit("a\u{00e9}b", 3), "a\u{00e9}");
    }
}
