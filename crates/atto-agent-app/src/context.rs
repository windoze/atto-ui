//! Context construction for DeepSeek chat completion requests.
//!
//! The builder owns conversion from the UI transcript shape to OpenAI-compatible
//! messages. Later M6 work can extend this module with mention expansion,
//! budgets, and compaction without scattering prompt assembly through the app.

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

/// Builds DeepSeek messages from UI transcript state plus optional active skills.
#[derive(Clone, Copy, Debug, Default)]
pub struct ContextBuilder<'a> {
    skill_registry: Option<&'a SkillRegistry>,
    loaded_skills: Option<&'a LoadedSkillSet>,
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

    /// Converts a UI transcript into OpenAI-compatible chat messages.
    pub fn build_messages(&self, transcript: &[ChatMessage]) -> Vec<ChatCompletionMessage> {
        let mut messages = Vec::new();
        if let Some(skill_prompt) = self.skill_prompt() {
            messages.push(ChatCompletionMessage::system(skill_prompt));
        }
        for message in transcript {
            push_transcript_message(&mut messages, message);
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

fn push_transcript_message(messages: &mut Vec<ChatCompletionMessage>, message: &ChatMessage) {
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
                    tool_result_content(tool_result),
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

fn tool_result_content(result: &ToolResultBlock) -> String {
    let mut content = format!("ok: {}", result.ok);
    if let Some(exit_code) = result.exit_code {
        content.push_str(&format!("\nexit_code: {exit_code}"));
    }
    let output = result.output.as_text();
    if !output.is_empty() {
        content.push_str("\n\n");
        content.push_str(output);
    }
    content
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

    fn unique_temp_dir(label: &str) -> PathBuf {
        let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("atto-agent-{label}-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }
}
