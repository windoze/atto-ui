//! Context construction for DeepSeek chat completion requests.
//!
//! The builder owns conversion from the UI transcript shape to OpenAI-compatible
//! messages, including bounded file mention expansion and context-only blocks.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};
use atto_ui::ComponentValue;
use atto_ui_chat::{
    ChatBlock, ChatMessage, ChatRole, CompactBlock, NoticeBlock, NoticeLevel, ToolInput,
    ToolResultBlock, ToolUseBlock,
};
use serde_json::{Map, Number, Value};

use crate::deepseek::{
    ChatCompletionMessage, ChatFunctionCall, ChatMessageRole, ChatToolCall, ChatToolKind,
};
use crate::skill::{LoadedSkillSet, SkillRegistry, build_skill_prompt_block};
use crate::tool::{display_workspace_path, resolve_existing_workspace_path};

/// Maximum UTF-8 bytes injected for one `@path` mention.
pub const MENTION_FILE_MAX_BYTES: usize = 32 * 1024;
/// Maximum aggregate UTF-8 bytes injected for all file mentions in one user message.
pub const MENTION_FILES_MAX_BYTES: usize = 128 * 1024;
/// Maximum UTF-8 bytes sent back to the model for one tool result message.
pub const TOOL_RESULT_MAX_BYTES: usize = 16 * 1024;

/// Builds DeepSeek messages from UI transcript state plus optional active skills.
#[derive(Clone, Copy, Debug)]
pub struct ContextBuilder<'a> {
    skill_registry: Option<&'a SkillRegistry>,
    loaded_skills: Option<&'a LoadedSkillSet>,
    file_mentions_workspace: Option<&'a Path>,
    tool_result_max_bytes: usize,
}

impl<'a> Default for ContextBuilder<'a> {
    fn default() -> Self {
        Self {
            skill_registry: None,
            loaded_skills: None,
            file_mentions_workspace: None,
            tool_result_max_bytes: TOOL_RESULT_MAX_BYTES,
        }
    }
}

impl<'a> ContextBuilder<'a> {
    /// Creates a context builder without skill prompt injection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enables `<skills>` system prompt injection for the currently loaded skills.
    pub fn with_skills(
        mut self,
        skill_registry: &'a SkillRegistry,
        loaded_skills: &'a LoadedSkillSet,
    ) -> Self {
        self.skill_registry = Some(skill_registry);
        self.loaded_skills = Some(loaded_skills);
        self
    }

    /// Enables `@path` mention expansion for files contained in the workspace.
    pub fn with_file_mentions(mut self, workspace: &'a Path) -> Self {
        self.file_mentions_workspace = Some(workspace);
        self
    }

    /// Overrides the per-tool-result model context budget.
    pub fn with_tool_result_max_bytes(mut self, max_bytes: usize) -> Self {
        self.tool_result_max_bytes = max_bytes;
        self
    }

    /// Converts a UI transcript into OpenAI-compatible chat messages.
    pub fn build_messages(&self, transcript: &[ChatMessage]) -> Vec<ChatCompletionMessage> {
        let mut messages = Vec::new();
        if let Some(skill_prompt) = self.skill_prompt() {
            messages.push(ChatCompletionMessage::system(skill_prompt));
        }
        for message in transcript {
            push_transcript_message(
                &mut messages,
                message,
                self.file_mentions_workspace,
                self.tool_result_max_bytes,
            );
        }
        messages
    }

    fn skill_prompt(&self) -> Option<String> {
        let registry = self.skill_registry?;
        let loaded = self.loaded_skills?;
        build_skill_prompt_block(registry, loaded)
    }
}

#[derive(Clone, Debug)]
struct PendingRoleMessage {
    role: ChatMessageRole,
    content: String,
    tool_calls: Vec<ChatToolCall>,
}

impl PendingRoleMessage {
    fn new(role: ChatMessageRole) -> Self {
        Self {
            role,
            content: String::new(),
            tool_calls: Vec::new(),
        }
    }

    fn push_text(&mut self, text: &str) {
        push_section(&mut self.content, text);
    }

    fn push_tool_call(&mut self, tool_use: &ToolUseBlock) {
        self.tool_calls.push(chat_tool_call_from_tool_use(tool_use));
    }

    fn flush(&mut self, messages: &mut Vec<ChatCompletionMessage>) {
        if self.content.is_empty() && self.tool_calls.is_empty() {
            return;
        }
        messages.push(ChatCompletionMessage {
            role: self.role,
            content: (!self.content.is_empty()).then(|| std::mem::take(&mut self.content)),
            reasoning_content: None,
            tool_calls: std::mem::take(&mut self.tool_calls),
            tool_call_id: None,
        });
    }
}

fn push_transcript_message(
    messages: &mut Vec<ChatCompletionMessage>,
    message: &ChatMessage,
    file_mentions_workspace: Option<&Path>,
    tool_result_max_bytes: usize,
) {
    let mut pending = PendingRoleMessage::new(role_for_chat_message(&message.role));
    for block in &message.blocks {
        match block {
            ChatBlock::Text(text) if !text.markdown.is_empty() => {
                pending.push_text(&text.markdown);
            }
            ChatBlock::ToolUse(tool_use) if pending.role == ChatMessageRole::Assistant => {
                pending.push_tool_call(tool_use);
            }
            ChatBlock::ToolResult(tool_result) => {
                pending.flush(messages);
                messages.push(ChatCompletionMessage::tool(
                    tool_result.call_id.clone(),
                    tool_result_content(tool_result, tool_result_max_bytes),
                ));
            }
            ChatBlock::Notice(notice) => {
                pending.flush(messages);
                if let Some(content) = notice_context_content(notice) {
                    messages.push(ChatCompletionMessage::system(content));
                }
            }
            ChatBlock::Compact(compact) => {
                pending.flush(messages);
                if let Some(content) = compact_context_content(compact) {
                    messages.push(ChatCompletionMessage::system(content));
                }
            }
            _ => {}
        }
    }
    if pending.role == ChatMessageRole::User {
        append_file_mention_context(&mut pending.content, file_mentions_workspace);
    }
    pending.flush(messages);
}

fn role_for_chat_message(role: &ChatRole) -> ChatMessageRole {
    match role {
        ChatRole::User => ChatMessageRole::User,
        ChatRole::Assistant => ChatMessageRole::Assistant,
        ChatRole::System | ChatRole::Custom(_) => ChatMessageRole::System,
    }
}

fn notice_context_content(notice: &NoticeBlock) -> Option<String> {
    (!notice.text.is_empty()).then(|| {
        format!(
            "<notice level=\"{}\">\n{}\n</notice>",
            notice_level_name(notice.level),
            notice.text
        )
    })
}

fn notice_level_name(level: NoticeLevel) -> &'static str {
    match level {
        NoticeLevel::Info => "info",
        NoticeLevel::Warning => "warning",
        NoticeLevel::Error => "error",
    }
}

fn compact_context_content(compact: &CompactBlock) -> Option<String> {
    if compact.summary.is_empty() {
        return None;
    }
    let mut open = format!("<compact status=\"{}\"", compact.status.as_str());
    if let Some(before_tokens) = compact.before_tokens {
        open.push_str(&format!(" before_tokens=\"{before_tokens}\""));
    }
    if let Some(after_tokens) = compact.after_tokens {
        open.push_str(&format!(" after_tokens=\"{after_tokens}\""));
    }
    open.push('>');
    Some(format!("{open}\n{}\n</compact>", compact.summary))
}

fn push_section(content: &mut String, section: &str) {
    if section.is_empty() {
        return;
    }
    if !content.is_empty() {
        content.push_str("\n\n");
    }
    content.push_str(section);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileMentionBudget {
    max_file_bytes: usize,
    max_total_bytes: usize,
}

impl Default for FileMentionBudget {
    fn default() -> Self {
        Self {
            max_file_bytes: MENTION_FILE_MAX_BYTES,
            max_total_bytes: MENTION_FILES_MAX_BYTES,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileMentionSummary {
    path: String,
    total_bytes: u64,
    included_bytes: usize,
    truncated: bool,
    text: String,
}

fn append_file_mention_context(content: &mut String, workspace: Option<&Path>) {
    let Some(workspace) = workspace else {
        return;
    };
    let mentions = parse_file_mentions(content);
    let Some(context) =
        build_file_mention_context(workspace, &mentions, FileMentionBudget::default())
    else {
        return;
    };
    push_section(content, &context);
}

fn parse_file_mentions(content: &str) -> Vec<String> {
    let mut mentions = Vec::new();
    let mut seen = BTreeSet::new();
    let mut indices = content.char_indices().peekable();
    let mut previous = None;

    while let Some((index, ch)) = indices.next() {
        if ch != '@' || !is_file_mention_start(previous) {
            previous = Some(ch);
            continue;
        }

        let start = index + ch.len_utf8();
        let mut end = start;
        while let Some(&(next_index, next_ch)) = indices.peek() {
            if is_file_mention_terminator(next_ch) {
                break;
            }
            end = next_index + next_ch.len_utf8();
            indices.next();
        }

        let candidate = trim_file_mention_token(&content[start..end]);
        if is_valid_file_mention(candidate) && seen.insert(candidate.to_string()) {
            mentions.push(candidate.to_string());
        }
        previous = content[start..end].chars().next_back().or(Some(ch));
    }

    mentions
}

fn is_file_mention_start(previous: Option<char>) -> bool {
    previous.is_none_or(|ch| {
        ch.is_whitespace() || matches!(ch, '(' | '[' | '{' | '<' | '"' | '\'' | '`')
    })
}

fn is_file_mention_terminator(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, ')' | ']' | '}' | '<' | '>' | '"' | '\'' | '`')
}

fn trim_file_mention_token(mut token: &str) -> &str {
    token = token.trim();
    while let Some(ch) = token.chars().next_back() {
        if !matches!(ch, ',' | '.' | ';' | ':' | '!' | '?') {
            break;
        }
        token = &token[..token.len() - ch.len_utf8()];
    }
    token
}

fn is_valid_file_mention(token: &str) -> bool {
    !token.is_empty() && token != "." && token != ".." && !token.starts_with('@')
}

fn build_file_mention_context(
    workspace: &Path,
    mentions: &[String],
    budget: FileMentionBudget,
) -> Option<String> {
    if mentions.is_empty() || budget.max_file_bytes == 0 || budget.max_total_bytes == 0 {
        return None;
    }

    let mut context = String::from("<context_files>\n");
    let workspace_root = match canonical_file_mention_workspace(workspace) {
        Ok(workspace_root) => workspace_root,
        Err(error) => {
            context.push_str(&format!(
                "<file error=\"{}\" />\n",
                xml_attr_escape(&format!("workspace unavailable: {error:#}"))
            ));
            context.push_str("</context_files>");
            return Some(context);
        }
    };

    let mut remaining_bytes = budget.max_total_bytes;
    for mention in mentions {
        context.push_str(&file_mention_entry(
            &workspace_root,
            mention,
            &mut remaining_bytes,
            budget.max_file_bytes,
        ));
    }
    context.push_str("</context_files>");
    Some(context)
}

fn canonical_file_mention_workspace(workspace: &Path) -> Result<std::path::PathBuf> {
    let workspace = workspace
        .canonicalize()
        .with_context(|| format!("workspace `{}` must exist", workspace.display()))?;
    if !workspace.is_dir() {
        bail!("workspace `{}` is not a directory", workspace.display());
    }
    Ok(workspace)
}

fn file_mention_entry(
    workspace_root: &Path,
    mention: &str,
    remaining_bytes: &mut usize,
    max_file_bytes: usize,
) -> String {
    if *remaining_bytes == 0 {
        return format!(
            "<file path=\"{}\" skipped=\"total_budget_exhausted\" />\n",
            xml_attr_escape(mention)
        );
    }

    let path = match resolve_existing_workspace_path(workspace_root, mention) {
        Ok(path) => path,
        Err(error) => return file_mention_error_entry(mention, error),
    };
    match read_file_mention_summary(workspace_root, &path, max_file_bytes.min(*remaining_bytes)) {
        Ok(summary) => {
            *remaining_bytes = (*remaining_bytes).saturating_sub(summary.included_bytes);
            format_file_mention_summary(&summary)
        }
        Err(error) => file_mention_error_entry(mention, error),
    }
}

fn read_file_mention_summary(
    workspace_root: &Path,
    path: &Path,
    max_bytes: usize,
) -> Result<FileMentionSummary> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to read metadata for `{}`", path.display()))?;
    if !metadata.is_file() {
        bail!("mention path `{}` is not a file", path.display());
    }
    if max_bytes == 0 {
        bail!("file mention budget is exhausted");
    }

    let mut bytes = Vec::new();
    File::open(path)
        .with_context(|| format!("failed to open `{}`", path.display()))?
        .take(max_bytes as u64 + 4)
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read `{}`", path.display()))?;

    let truncated = bytes.len() > max_bytes || metadata.len() > max_bytes as u64;
    if bytes.len() > max_bytes {
        bytes.truncate(max_bytes);
    }
    let text = utf8_mention_prefix(&bytes, truncated)
        .with_context(|| format!("mention path `{}` is not valid UTF-8", path.display()))?
        .to_string();

    Ok(FileMentionSummary {
        path: display_workspace_path(workspace_root, path),
        total_bytes: metadata.len(),
        included_bytes: text.len(),
        truncated: truncated || text.len() < metadata.len() as usize,
        text,
    })
}

fn utf8_mention_prefix(bytes: &[u8], allow_incomplete_tail: bool) -> Result<&str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok(text),
        Err(error) if allow_incomplete_tail && error.error_len().is_none() => {
            std::str::from_utf8(&bytes[..error.valid_up_to()])
                .context("valid UTF-8 prefix should decode")
        }
        Err(error) => bail!("invalid UTF-8 near byte {}", error.valid_up_to()),
    }
}

fn format_file_mention_summary(summary: &FileMentionSummary) -> String {
    format!(
        "<file path=\"{}\" bytes=\"{}\" included_bytes=\"{}\" truncated=\"{}\">\n{}\n</file>\n",
        xml_attr_escape(&summary.path),
        summary.total_bytes,
        summary.included_bytes,
        summary.truncated,
        summary.text
    )
}

fn file_mention_error_entry(mention: &str, error: anyhow::Error) -> String {
    format!(
        "<file path=\"{}\" error=\"{}\" />\n",
        xml_attr_escape(mention),
        xml_attr_escape(&format!("{error:#}"))
    )
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

fn chat_tool_call_from_tool_use(tool_use: &ToolUseBlock) -> ChatToolCall {
    ChatToolCall {
        id: tool_use.call_id.clone(),
        kind: ChatToolKind::Function,
        function: ChatFunctionCall {
            name: tool_use.name.clone(),
            arguments: tool_arguments_from_input(&tool_use.input),
        },
    }
}

fn tool_arguments_from_input(input: &ToolInput) -> String {
    match input {
        ToolInput::Text(text) if text.trim().is_empty() => "{}".to_string(),
        ToolInput::Text(text) => text.clone(),
        ToolInput::Json(value) => {
            serde_json::to_string(&component_value_to_json(value)).unwrap_or_else(|_| "{}".into())
        }
    }
}

fn tool_result_content(result: &ToolResultBlock, max_bytes: usize) -> String {
    let mut content = format!("ok: {}", result.ok);
    if let Some(exit_code) = result.exit_code {
        content.push_str(&format!("\nexit_code: {exit_code}"));
    }
    let output = result.output.as_text();
    if !output.is_empty() {
        content.push_str("\n\n");
        content.push_str(output);
    }
    truncate_tool_result_for_model(content, max_bytes)
}

fn truncate_tool_result_for_model(content: String, max_bytes: usize) -> String {
    if max_bytes == 0 || content.len() <= max_bytes {
        return if max_bytes == 0 {
            String::new()
        } else {
            content
        };
    }

    let total_bytes = content.len();
    let notice = tool_result_truncation_notice(total_bytes, max_bytes);
    if notice.len() >= max_bytes {
        return utf8_prefix(&notice, max_bytes).to_string();
    }

    let prefix_budget = max_bytes.saturating_sub(notice.len());
    let prefix = utf8_prefix(&content, prefix_budget);
    format!("{prefix}{notice}")
}

fn tool_result_truncation_notice(total_bytes: usize, max_bytes: usize) -> String {
    format!(
        "\n\n[Tool result truncated for model context: original_bytes={total_bytes}, max_bytes={max_bytes}. UI output retains the full result or a tail window.]"
    )
}

fn utf8_prefix(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

/// Converts UI component values into JSON for DeepSeek tool call arguments.
pub(crate) fn component_value_to_json(value: &ComponentValue) -> Value {
    match value {
        ComponentValue::Null => Value::Null,
        ComponentValue::Bool(value) => Value::Bool(*value),
        ComponentValue::I64(value) => Value::Number(Number::from(*value)),
        ComponentValue::U64(value) => Value::Number(Number::from(*value)),
        ComponentValue::F64(value) => Number::from_f64(*value)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ComponentValue::String(value) => Value::String(value.clone()),
        ComponentValue::StringList(values) => Value::Array(
            values
                .iter()
                .map(|value| Value::String(value.clone()))
                .collect(),
        ),
        ComponentValue::Table(rows) => Value::Array(
            rows.iter()
                .map(|row| {
                    Value::Array(
                        row.iter()
                            .map(|value| Value::String(value.clone()))
                            .collect(),
                    )
                })
                .collect(),
        ),
        ComponentValue::Rect(rect) => Value::Object(
            [
                (
                    "x".to_string(),
                    Value::Number(Number::from(u64::from(rect.x))),
                ),
                (
                    "y".to_string(),
                    Value::Number(Number::from(u64::from(rect.y))),
                ),
                (
                    "width".to_string(),
                    Value::Number(Number::from(u64::from(rect.width))),
                ),
                (
                    "height".to_string(),
                    Value::Number(Number::from(u64::from(rect.height))),
                ),
            ]
            .into_iter()
            .collect::<Map<_, _>>(),
        ),
        ComponentValue::Bytes(bytes) => Value::Array(
            bytes
                .iter()
                .map(|byte| Value::Number(Number::from(u64::from(*byte))))
                .collect(),
        ),
        ComponentValue::List(values) => {
            Value::Array(values.iter().map(component_value_to_json).collect())
        }
        ComponentValue::Map(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), component_value_to_json(value)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use atto_ui_chat::{
        ChatBlockId, CompactBlock, CompactStatus, NoticeLevel, TextBlock, ToolOutput, ToolStatus,
    };

    use super::*;
    use crate::skill::{LoadedSkillSet, SkillRegistry, SkillSearchPath};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn context_builder_maps_transcript_blocks_to_deepseek_messages() {
        let transcript = vec![
            ChatMessage::text(1, ChatRole::User, "Read the fixture."),
            ChatMessage::new(
                2,
                ChatRole::Assistant,
                vec![
                    ChatBlock::Text(TextBlock {
                        id: ChatBlockId::new(20),
                        markdown: "I will inspect it.".to_string(),
                        streaming: false,
                    }),
                    ChatBlock::ToolUse(ToolUseBlock {
                        id: ChatBlockId::new(21),
                        call_id: "call_read".to_string(),
                        name: "read_file".to_string(),
                        input: ToolInput::Json(ComponentValue::Map(BTreeMap::from([(
                            "path".to_string(),
                            ComponentValue::String("fixture.txt".to_string()),
                        )]))),
                        status: ToolStatus::Done,
                        approval: None,
                        collapsed: false,
                    }),
                    ChatBlock::ToolResult(ToolResultBlock {
                        id: ChatBlockId::new(22),
                        call_id: "call_read".to_string(),
                        ok: true,
                        exit_code: None,
                        output: ToolOutput::Markdown("Path: `fixture.txt`\n\nbody".to_string()),
                        collapsed: false,
                    }),
                    ChatBlock::Notice(NoticeBlock {
                        id: ChatBlockId::new(23),
                        level: NoticeLevel::Warning,
                        text: "Context was truncated.".to_string(),
                    }),
                    ChatBlock::Compact(CompactBlock {
                        id: ChatBlockId::new(24),
                        status: CompactStatus::Complete,
                        before_tokens: Some(120),
                        after_tokens: Some(30),
                        summary: "Earlier turns discussed the fixture.".to_string(),
                    }),
                ],
            ),
        ];

        let messages = ContextBuilder::new().build_messages(&transcript);

        assert_eq!(messages.len(), 5);
        assert_eq!(messages[0].role, ChatMessageRole::User);
        assert_eq!(messages[0].content.as_deref(), Some("Read the fixture."));
        assert_eq!(messages[1].role, ChatMessageRole::Assistant);
        assert_eq!(messages[1].content.as_deref(), Some("I will inspect it."));
        assert_eq!(messages[1].tool_calls.len(), 1);
        assert_eq!(messages[1].tool_calls[0].id, "call_read");
        assert_eq!(messages[1].tool_calls[0].function.name, "read_file");
        assert_eq!(
            messages[1].tool_calls[0].function.arguments,
            r#"{"path":"fixture.txt"}"#
        );
        assert_eq!(messages[2].role, ChatMessageRole::Tool);
        assert_eq!(messages[2].tool_call_id.as_deref(), Some("call_read"));
        assert!(
            messages[2]
                .content
                .as_deref()
                .is_some_and(|content| content.contains("ok: true") && content.contains("body"))
        );
        assert_eq!(messages[3].role, ChatMessageRole::System);
        assert_eq!(
            messages[3].content.as_deref(),
            Some("<notice level=\"warning\">\nContext was truncated.\n</notice>")
        );
        assert_eq!(messages[4].role, ChatMessageRole::System);
        assert_eq!(
            messages[4].content.as_deref(),
            Some(
                "<compact status=\"complete\" before_tokens=\"120\" after_tokens=\"30\">\nEarlier turns discussed the fixture.\n</compact>"
            )
        );
    }

    #[test]
    fn context_builder_truncates_long_tool_results_for_model_context() {
        let full_output = format!("{}END-OF-FULL-OUTPUT", "a".repeat(TOOL_RESULT_MAX_BYTES));
        let transcript = vec![ChatMessage::new(
            1,
            ChatRole::Assistant,
            vec![ChatBlock::ToolResult(ToolResultBlock {
                id: ChatBlockId::new(10),
                call_id: "call_long".to_string(),
                ok: true,
                exit_code: None,
                output: ToolOutput::Markdown(full_output.clone()),
                collapsed: false,
            })],
        )];

        let messages = ContextBuilder::new().build_messages(&transcript);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, ChatMessageRole::Tool);
        assert_eq!(messages[0].tool_call_id.as_deref(), Some("call_long"));
        let content = messages[0]
            .content
            .as_deref()
            .expect("tool message should contain content");
        assert!(content.len() <= TOOL_RESULT_MAX_BYTES);
        assert!(content.contains("ok: true"));
        assert!(content.contains("Tool result truncated for model context"));
        assert!(content.contains("UI output retains the full result or a tail window"));
        assert!(!content.contains("END-OF-FULL-OUTPUT"));
        match &transcript[0].blocks[0] {
            ChatBlock::ToolResult(result) => {
                assert_eq!(result.output.as_text(), full_output);
            }
            other => panic!("expected tool result block, got {other:?}"),
        }
    }

    #[test]
    fn context_builder_truncates_tool_results_on_utf8_boundaries() {
        let transcript = vec![ChatMessage::new(
            1,
            ChatRole::Assistant,
            vec![ChatBlock::ToolResult(ToolResultBlock {
                id: ChatBlockId::new(10),
                call_id: "call_utf8".to_string(),
                ok: false,
                exit_code: Some(1),
                output: ToolOutput::Markdown("中".repeat(512)),
                collapsed: false,
            })],
        )];

        let messages = ContextBuilder::new()
            .with_tool_result_max_bytes(220)
            .build_messages(&transcript);

        let content = messages[0]
            .content
            .as_deref()
            .expect("tool message should contain content");
        assert!(content.len() <= 220);
        assert!(std::str::from_utf8(content.as_bytes()).is_ok());
        assert!(content.contains("Tool result truncated for model context"));
        assert!(content.contains("ok: false"));
        assert!(content.contains("exit_code: 1"));
    }

    #[test]
    fn context_builder_injects_loaded_skills_before_transcript() {
        let workspace = unique_temp_dir("context-skill");
        let skill_dir = workspace.join(".atto/skills/rust-review");
        fs::create_dir_all(&skill_dir).expect("create skill directory");
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: rust-review\ndescription: Review Rust code.\nmode: manual\n---\nUse this skill for Rust review tasks.\n",
        )
        .expect("write skill file");
        let registry =
            SkillRegistry::discover_from_paths(&[SkillSearchPath::workspace(&workspace)]);
        let loaded = LoadedSkillSet::default();
        assert!(loaded.insert("rust-review"));

        let messages = ContextBuilder::new()
            .with_skills(&registry, &loaded)
            .build_messages(&[ChatMessage::text(1, ChatRole::User, "Review this.")]);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, ChatMessageRole::System);
        let skill_prompt = messages[0]
            .content
            .as_deref()
            .expect("skill prompt should be present");
        assert!(skill_prompt.starts_with("<skills>\n"));
        assert!(skill_prompt.contains("<skill name=\"rust-review\" source=\""));
        assert!(skill_prompt.contains("Use this skill for Rust review tasks."));
        assert_eq!(messages[1].role, ChatMessageRole::User);

        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn context_builder_expands_file_mentions_into_user_context() {
        let workspace = unique_temp_dir("context-mentions");
        fs::create_dir_all(workspace.join("src")).expect("create src directory");
        fs::write(workspace.join("src/lib.rs"), "pub fn run() {}\n").expect("write Rust file");
        fs::write(workspace.join("Makefile"), "build:\n\tcargo build\n").expect("write makefile");

        let messages = ContextBuilder::new()
            .with_file_mentions(&workspace)
            .build_messages(&[ChatMessage::text(
                1,
                ChatRole::User,
                "Review @src/lib.rs, then @Makefile.",
            )]);

        assert_eq!(messages.len(), 1);
        let content = messages[0]
            .content
            .as_deref()
            .expect("user message should contain text");
        assert!(content.starts_with("Review @src/lib.rs, then @Makefile."));
        assert!(content.contains("<context_files>"));
        assert!(content.contains("<file path=\"src/lib.rs\""));
        assert!(content.contains("pub fn run() {}"));
        assert!(content.contains("<file path=\"Makefile\""));
        assert!(content.contains("cargo build"));

        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn context_builder_records_file_mention_errors_without_leaking_escaped_files() {
        let workspace = unique_temp_dir("context-mention-escape");
        fs::create_dir_all(&workspace).expect("create workspace");
        let secret_path = workspace
            .parent()
            .expect("workspace should have parent")
            .join("context-mention-secret.txt");
        fs::write(&secret_path, "do not leak this secret").expect("write escaped file");

        let messages = ContextBuilder::new()
            .with_file_mentions(&workspace)
            .build_messages(&[ChatMessage::text(
                1,
                ChatRole::User,
                "Compare @../context-mention-secret.txt and @missing.txt",
            )]);

        let content = messages[0]
            .content
            .as_deref()
            .expect("user message should contain text");
        assert!(content.contains("<file path=\"../context-mention-secret.txt\" error=\""));
        assert!(content.contains("<file path=\"missing.txt\" error=\""));
        assert!(!content.contains("do not leak this secret"));

        let _ = fs::remove_file(secret_path);
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn context_builder_limits_file_mention_bytes() {
        let workspace = unique_temp_dir("context-mention-budget");
        fs::create_dir_all(&workspace).expect("create workspace");
        let large_file = "a".repeat(MENTION_FILE_MAX_BYTES + 10);
        for index in 0..5 {
            fs::write(workspace.join(format!("f{index}.txt")), &large_file)
                .expect("write large file");
        }

        let messages = ContextBuilder::new()
            .with_file_mentions(&workspace)
            .build_messages(&[ChatMessage::text(
                1,
                ChatRole::User,
                "Read @f0.txt @f1.txt @f2.txt @f3.txt @f4.txt",
            )]);

        let content = messages[0]
            .content
            .as_deref()
            .expect("user message should contain text");
        assert!(content.contains(&format!(
            "<file path=\"f0.txt\" bytes=\"{}\" included_bytes=\"{}\" truncated=\"true\">",
            MENTION_FILE_MAX_BYTES + 10,
            MENTION_FILE_MAX_BYTES
        )));
        assert!(content.contains("<file path=\"f3.txt\""));
        assert!(content.contains("<file path=\"f4.txt\" skipped=\"total_budget_exhausted\" />"));

        let _ = fs::remove_dir_all(workspace);
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("atto-agent-{label}-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }
}
