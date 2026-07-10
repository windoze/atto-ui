//! Plan-mode turn classification.
//!
//! The app uses this module to make a deterministic, local decision about
//! whether a user turn should enter plan mode before any mutating work happens.

use std::collections::BTreeSet;

use crate::config::PlanMode;
use crate::tool::{ToolPermission, ToolRegistry, ToolSpec};

/// The plan-mode decision for one user-submitted turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanTurnDecision {
    Direct,
    RequiresPlan { reason: PlanRequirementReason },
}

impl PlanTurnDecision {
    /// Reports whether the turn should generate and await a plan first.
    pub fn requires_plan(&self) -> bool {
        matches!(self, Self::RequiresPlan { .. })
    }

    /// Returns the reason attached to a required-plan decision.
    pub fn reason(&self) -> Option<&PlanRequirementReason> {
        match self {
            Self::Direct => None,
            Self::RequiresPlan { reason } => Some(reason),
        }
    }
}

/// Coarse reason explaining why plan mode was selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanRequirementReason {
    ModeOn,
    MutatingTool(String),
    CommandExecution,
    CodeMutation,
    FileMutation,
}

/// Resolves the effective plan decision for the configured mode and prompt.
pub fn decide_plan_for_turn(
    mode: PlanMode,
    prompt: &str,
    registry: &ToolRegistry,
) -> PlanTurnDecision {
    match mode {
        PlanMode::Off => PlanTurnDecision::Direct,
        PlanMode::On => requires_plan(PlanRequirementReason::ModeOn),
        PlanMode::Auto => auto_plan_decision_for_prompt(prompt, registry),
    }
}

/// Applies the auto-mode heuristic to one prompt and the available tools.
pub fn auto_plan_decision_for_prompt(prompt: &str, registry: &ToolRegistry) -> PlanTurnDecision {
    let lower = prompt.to_lowercase();
    let tokens = ascii_tokens(prompt);
    if lower.trim().is_empty() || is_explanatory_only_prompt(&lower) {
        return PlanTurnDecision::Direct;
    }

    if let Some(name) = mentioned_mutating_tool(&lower, &tokens, registry) {
        return requires_plan(PlanRequirementReason::MutatingTool(name));
    }
    if has_command_execution_intent(&lower, &tokens) {
        return requires_plan(PlanRequirementReason::CommandExecution);
    }
    if has_code_mutation_intent(&lower, &tokens) {
        return requires_plan(PlanRequirementReason::CodeMutation);
    }
    if has_file_mutation_intent(&lower, &tokens) {
        return requires_plan(PlanRequirementReason::FileMutation);
    }

    PlanTurnDecision::Direct
}

fn requires_plan(reason: PlanRequirementReason) -> PlanTurnDecision {
    PlanTurnDecision::RequiresPlan { reason }
}

fn mentioned_mutating_tool(
    lower: &str,
    tokens: &BTreeSet<String>,
    registry: &ToolRegistry,
) -> Option<String> {
    registry
        .specs()
        .filter(|spec| tool_can_have_side_effects(spec))
        .find(|spec| text_mentions_tool(lower, tokens, &spec.name))
        .map(|spec| spec.name.clone())
}

fn tool_can_have_side_effects(spec: &ToolSpec) -> bool {
    !matches!(spec.permission, ToolPermission::AlwaysAllow)
}

fn text_mentions_tool(lower: &str, tokens: &BTreeSet<String>, name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    if tokens.contains(&name) {
        return true;
    }
    let spaced = name.replace(['_', '-'], " ");
    lower.contains(&spaced)
}

fn has_command_execution_intent(lower: &str, tokens: &BTreeSet<String>) -> bool {
    contains_any_phrase(
        lower,
        &[
            "run command",
            "execute command",
            "run tests",
            "run the tests",
            "cargo test",
            "cargo clippy",
            "cargo fmt",
            "npm install",
            "pnpm install",
            "yarn install",
            "git commit",
            "docker build",
            "运行测试",
            "运行命令",
            "执行命令",
            "跑测试",
        ],
    ) || (has_any_token(tokens, &["run", "execute", "install", "build", "compile"])
        && has_any_token(
            tokens,
            &[
                "command", "cmd", "shell", "cargo", "npm", "pnpm", "yarn", "make", "git", "python",
                "node", "bash", "sh", "docker", "test", "tests", "clippy", "fmt",
            ],
        ))
        || contains_any_phrase(lower, &["运行", "执行", "编译", "构建", "安装"])
}

fn has_code_mutation_intent(lower: &str, tokens: &BTreeSet<String>) -> bool {
    if contains_any_phrase(lower, &["实现", "修复", "重构", "修改代码", "更新代码"]) {
        return true;
    }
    if has_any_token(tokens, &["implement", "fix", "refactor", "patch"]) {
        return true;
    }
    has_any_token(
        tokens,
        &[
            "edit", "modify", "change", "update", "add", "remove", "delete", "rename", "create",
            "write",
        ],
    ) && has_any_token(
        tokens,
        &[
            "code",
            "src",
            "test",
            "tests",
            "docs",
            "readme",
            "function",
            "module",
            "crate",
            "component",
            "api",
            "bug",
            "issue",
            "repo",
            "repository",
            "workspace",
        ],
    )
}

fn has_file_mutation_intent(lower: &str, tokens: &BTreeSet<String>) -> bool {
    if contains_any_phrase(
        lower,
        &[
            "写文件",
            "修改文件",
            "编辑文件",
            "更新文件",
            "新增文件",
            "删除文件",
            "创建文件",
            "应用补丁",
        ],
    ) {
        return true;
    }
    has_any_token(
        tokens,
        &[
            "write", "edit", "modify", "change", "update", "add", "remove", "delete", "rename",
            "create",
        ],
    ) && has_any_token(
        tokens,
        &[
            "file", "files", "path", "readme", "markdown", "toml", "json", "yaml", "yml", "rs",
            "js", "ts", "tsx", "css", "html", "md",
        ],
    )
}

fn is_explanatory_only_prompt(lower: &str) -> bool {
    let trimmed = lower.trim_start();
    let question_like = starts_with_any(
        trimmed,
        &[
            "how do i ",
            "how should i ",
            "how can i ",
            "what is ",
            "what does ",
            "what command ",
            "which command ",
            "why ",
            "explain ",
            "describe ",
            "tell me ",
            "can you explain ",
            "could you explain ",
            "如何",
            "怎么",
            "为什么",
            "解释",
            "说明",
        ],
    );
    question_like
        && !contains_any_phrase(
            trimmed,
            &["and then", "go ahead", "please run", "并", "然后", "帮我"],
        )
}

fn ascii_tokens(value: &str) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    let mut current = String::new();
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.insert(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.insert(current);
    }
    tokens
}

fn has_any_token(tokens: &BTreeSet<String>, needles: &[&str]) -> bool {
    needles.iter().any(|needle| tokens.contains(*needle))
}

fn starts_with_any(value: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| value.starts_with(prefix))
}

fn contains_any_phrase(value: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|phrase| value.contains(phrase))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ToolRegistry {
        crate::tool::builtin_tool_registry().expect("built-in tool registry should be valid")
    }

    #[test]
    fn off_and_on_modes_override_auto_heuristic() {
        let registry = registry();

        assert_eq!(
            decide_plan_for_turn(PlanMode::Off, "please update README.md", &registry),
            PlanTurnDecision::Direct
        );
        assert_eq!(
            decide_plan_for_turn(PlanMode::On, "explain this file", &registry),
            requires_plan(PlanRequirementReason::ModeOn)
        );
    }

    #[test]
    fn auto_keeps_pure_questions_and_readonly_inspection_direct() {
        let registry = registry();

        assert_eq!(
            auto_plan_decision_for_prompt("What command should I run to test this?", &registry),
            PlanTurnDecision::Direct
        );
        assert_eq!(
            auto_plan_decision_for_prompt("Use read_file to inspect Cargo.toml", &registry),
            PlanTurnDecision::Direct
        );
        assert_eq!(
            auto_plan_decision_for_prompt("search text for TODO markers", &registry),
            PlanTurnDecision::Direct
        );
    }

    #[test]
    fn auto_detects_file_and_code_mutation_intent() {
        let registry = registry();

        assert_eq!(
            auto_plan_decision_for_prompt("Please update README.md with setup docs", &registry),
            requires_plan(PlanRequirementReason::CodeMutation)
        );
        assert_eq!(
            auto_plan_decision_for_prompt("请修改 src/lib.rs 并运行测试", &registry),
            requires_plan(PlanRequirementReason::CommandExecution)
        );
    }

    #[test]
    fn auto_detects_command_execution_and_mutating_tool_need() {
        let registry = registry();

        assert_eq!(
            auto_plan_decision_for_prompt("Run cargo test --workspace", &registry),
            requires_plan(PlanRequirementReason::CommandExecution)
        );
        assert_eq!(
            auto_plan_decision_for_prompt("Use run_command to execute cargo test", &registry),
            requires_plan(PlanRequirementReason::MutatingTool(
                "run_command".to_string()
            ))
        );
        assert_eq!(
            auto_plan_decision_for_prompt("Apply patch to fix the parser", &registry),
            requires_plan(PlanRequirementReason::MutatingTool(
                "apply_patch".to_string()
            ))
        );
    }
}
