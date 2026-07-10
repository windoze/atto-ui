//! Skill file parsing for local prompt packages.
//!
//! Skills are Markdown instruction files with YAML frontmatter. This module only
//! parses and validates the standalone `SKILL.md` format; registry discovery,
//! activation, matching, and prompt injection are implemented in later M4 tasks.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

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
}
