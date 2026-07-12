//! Plan-mode turn classification and plan draft parsing.
//!
//! The app uses this module to make a deterministic, local decision about
//! whether a user turn should enter plan mode before any mutating work happens,
//! and to turn model-generated plan drafts into `PlanItem` values.

use std::collections::BTreeSet;

use atto_ui_chat::{ChatError, ChatErrorKind, PlanItem};
use serde_json::{Value, json};

use crate::config::PlanMode;
use crate::deepseek::{ChatTool, ToolChoice, ToolChoiceFunction};
use crate::tool::ToolRegistry;

/// Virtual function name used when the model submits a plan draft.
pub const SUBMIT_PLAN_TOOL_NAME: &str = "submit_plan";

/// Minimum number of actionable items requested from plan mode.
pub const MIN_PLAN_ITEMS: usize = 3;

/// Maximum number of actionable items accepted from plan mode.
pub const MAX_PLAN_ITEMS: usize = 7;

/// System instruction prepended when a turn is drafting a plan.
pub const PLAN_MODE_SYSTEM_PROMPT: &str = "You are in plan mode. Do not modify files, run commands with side effects, or call mutating tools.\nProduce a concise execution plan with 3 to 7 actionable and verifiable items.\nWait for user approval before execution.";

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

/// Builds the OpenAI-compatible virtual tool used only for plan submission.
pub fn submit_plan_chat_tool() -> ChatTool {
    ChatTool::function(
        SUBMIT_PLAN_TOOL_NAME,
        "Submit a concise execution plan for user approval before doing mutating work.",
        json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "description": "Actionable, verifiable plan steps.",
                    "items": { "type": "string", "minLength": 1 },
                    "minItems": MIN_PLAN_ITEMS,
                    "maxItems": MAX_PLAN_ITEMS
                }
            },
            "required": ["items"],
            "additionalProperties": false
        }),
    )
}

/// Forces the chat-completions request to call the virtual plan tool.
pub fn submit_plan_tool_choice() -> ToolChoice {
    ToolChoice::Function(ToolChoiceFunction::named(SUBMIT_PLAN_TOOL_NAME))
}

/// Parses `submit_plan({ items })` arguments into UI plan items.
pub fn parse_submit_plan_arguments(arguments: &Value) -> Result<Vec<PlanItem>, ChatError> {
    let Value::Object(object) = arguments else {
        return Err(plan_tool_error(
            "submit_plan arguments must be a JSON object.",
            "Expected object shape: { \"items\": [\"...\"] }.",
        ));
    };
    for key in object.keys() {
        if key != "items" {
            return Err(plan_tool_error(
                format!("submit_plan received unknown argument `{key}`."),
                "Only the `items` array is supported.",
            ));
        }
    }
    let Some(Value::Array(items)) = object.get("items") else {
        return Err(plan_tool_error(
            "submit_plan requires an `items` array.",
            "Each item must be a non-empty string.",
        ));
    };

    let mut texts = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let Some(text) = item.as_str() else {
            return Err(plan_tool_error(
                format!("submit_plan item {} must be a string.", index + 1),
                "The `items` array must contain only strings.",
            ));
        };
        texts.push(text.to_string());
    }
    plan_items_from_texts(texts, "submit_plan")
}

/// Parses markdown ordered, unordered, or checklist lists into UI plan items.
pub fn parse_markdown_plan_items(markdown: &str) -> Result<Vec<PlanItem>, ChatError> {
    let mut in_fence = false;
    let mut texts = Vec::new();
    for raw_line in markdown.lines() {
        let line = raw_line.trim_start();
        if line.starts_with("```") || line.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(item) = markdown_list_item_text(line) {
            texts.push(item.to_string());
        }
    }
    plan_items_from_texts(texts, "markdown fallback")
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
        .filter(|spec| spec.can_have_side_effects())
        .find(|spec| text_mentions_tool(lower, tokens, &spec.name))
        .map(|spec| spec.name.clone())
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

fn markdown_list_item_text(line: &str) -> Option<&str> {
    let after_marker = unordered_list_item_text(line).or_else(|| ordered_list_item_text(line))?;
    let text = strip_markdown_checkbox(after_marker).trim();
    (!text.is_empty()).then_some(text)
}

fn unordered_list_item_text(line: &str) -> Option<&str> {
    let marker = line.chars().next()?;
    if !matches!(marker, '-' | '*' | '+') {
        return None;
    }
    let rest = line[marker.len_utf8()..].trim_start();
    (!rest.is_empty()).then_some(rest)
}

fn ordered_list_item_text(line: &str) -> Option<&str> {
    let marker_end = line
        .char_indices()
        .find_map(|(index, ch)| matches!(ch, '.' | ')').then_some(index))?;
    let number = &line[..marker_end];
    if number.is_empty() || !number.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let rest = line[marker_end + 1..].trim_start();
    (!rest.is_empty()).then_some(rest)
}

fn strip_markdown_checkbox(text: &str) -> &str {
    let trimmed = text.trim_start();
    for marker in ["[ ]", "[x]", "[X]"] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            return rest.trim_start();
        }
    }
    text
}

fn plan_items_from_texts(
    texts: impl IntoIterator<Item = String>,
    source: &str,
) -> Result<Vec<PlanItem>, ChatError> {
    let mut items = Vec::new();
    for text in texts {
        let text = text.trim();
        if !text.is_empty() {
            items.push(PlanItem {
                text: text.to_string(),
            });
        }
    }
    validate_plan_items(items, source)
}

fn validate_plan_items(items: Vec<PlanItem>, source: &str) -> Result<Vec<PlanItem>, ChatError> {
    if items.len() < MIN_PLAN_ITEMS {
        return Err(plan_tool_error(
            "Plan mode did not produce enough plan items.",
            format!(
                "{source} produced {} item(s); expected {MIN_PLAN_ITEMS} to {MAX_PLAN_ITEMS}.",
                items.len()
            ),
        ));
    }
    if items.len() > MAX_PLAN_ITEMS {
        return Err(plan_tool_error(
            "Plan mode produced too many plan items.",
            format!(
                "{source} produced {} item(s); expected at most {MAX_PLAN_ITEMS}.",
                items.len()
            ),
        ));
    }
    Ok(items)
}

fn plan_tool_error(message: impl Into<String>, detail: impl Into<String>) -> ChatError {
    ChatError::new(ChatErrorKind::Tool, message).with_detail(detail)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

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

    #[test]
    fn submit_plan_tool_schema_and_choice_are_virtual() {
        let tool = submit_plan_chat_tool();

        assert_eq!(tool.function.name, SUBMIT_PLAN_TOOL_NAME);
        assert_eq!(
            tool.function.parameters["properties"]["items"]["minItems"],
            3
        );
        assert_eq!(
            tool.function.parameters["properties"]["items"]["maxItems"],
            7
        );
        assert_eq!(
            serde_json::to_value(submit_plan_tool_choice()).unwrap(),
            json!({ "type": "function", "function": { "name": "submit_plan" } })
        );
    }

    #[test]
    fn parses_submit_plan_arguments_into_plan_items() {
        let items = parse_submit_plan_arguments(&json!({
            "items": [
                "Inspect the relevant modules.",
                "Implement the required change.",
                "Run formatting and tests."
            ]
        }))
        .unwrap();

        assert_eq!(
            items
                .iter()
                .map(|item| item.text.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Inspect the relevant modules.",
                "Implement the required change.",
                "Run formatting and tests."
            ]
        );
    }

    #[test]
    fn rejects_invalid_submit_plan_arguments() {
        let error = parse_submit_plan_arguments(&json!({
            "items": ["one", "two", "three"],
            "extra": true
        }))
        .unwrap_err();

        assert_eq!(error.kind, ChatErrorKind::Tool);
        assert!(error.message.contains("unknown argument"));

        let error = parse_submit_plan_arguments(&json!({
            "items": ["one", "two"]
        }))
        .unwrap_err();

        assert!(error.message.contains("enough plan items"));
    }

    #[test]
    fn parses_markdown_plan_lists() {
        let items = parse_markdown_plan_items(
            "Plan:\n\n1. Inspect current behavior.\n2) Implement submit_plan mapping.\n- [ ] Add regression tests.\n* Run validation.\n",
        )
        .unwrap();

        assert_eq!(
            items
                .iter()
                .map(|item| item.text.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Inspect current behavior.",
                "Implement submit_plan mapping.",
                "Add regression tests.",
                "Run validation."
            ]
        );
    }

    #[test]
    fn markdown_plan_parser_ignores_fenced_lists_and_requires_items() {
        let error =
            parse_markdown_plan_items("```\n- not a plan\n```\n- one\n- two\n").unwrap_err();

        assert_eq!(error.kind, ChatErrorKind::Tool);
        assert!(error.message.contains("enough plan items"));
    }
}
