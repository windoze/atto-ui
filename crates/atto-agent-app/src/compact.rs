//! Deterministic transcript compaction for bounded model context.
//!
//! The app can replace older, already-settled transcript messages with one
//! complete `CompactBlock`. `ContextBuilder` then sends that block as system
//! context, so later model requests keep the summary without replaying the old
//! turns in full.

use std::fmt::Write as _;

use atto_ui_chat::{
    ChatBlock, ChatBlockId, ChatMessage, ChatMessageId, ChatMessageStore, ChatRole, ChatTurnStatus,
    CompactBlock, CompactStatus, DiffBlock, EditDecision, PlanBlock, PlanDecision, TaskBlock,
    TaskStatus, ToolStatus,
};

const DEFAULT_CONTEXT_WINDOW_TOKENS: u64 = 64 * 1024;
const DEFAULT_COMPACT_THRESHOLD_PERCENT: u64 = 70;
const DEFAULT_RECENT_MESSAGE_LIMIT: usize = 20;
const DEFAULT_SUMMARY_MAX_BYTES: usize = 8 * 1024;
const ESTIMATED_BYTES_PER_TOKEN: usize = 4;
const BLOCK_EXCERPT_MAX_CHARS: usize = 240;

/// Policy controlling when and how much transcript history is compacted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CompactPolicy {
    pub threshold_tokens: u64,
    pub recent_message_limit: usize,
    pub summary_max_bytes: usize,
}

impl Default for CompactPolicy {
    fn default() -> Self {
        Self {
            threshold_tokens: DEFAULT_CONTEXT_WINDOW_TOKENS * DEFAULT_COMPACT_THRESHOLD_PERCENT
                / 100,
            recent_message_limit: DEFAULT_RECENT_MESSAGE_LIMIT,
            summary_max_bytes: DEFAULT_SUMMARY_MAX_BYTES,
        }
    }
}

/// Metadata describing a completed compaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CompactStats {
    pub compacted_messages: usize,
    pub before_tokens: u64,
    pub after_tokens: u64,
}

/// Result of compacting an immutable transcript snapshot.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompactedTranscript {
    pub messages: Vec<ChatMessage>,
    pub stats: CompactStats,
}

/// Replaces older store messages with a complete `CompactBlock` when over budget.
pub(crate) fn compact_store_if_needed(
    store: &ChatMessageStore,
    policy: CompactPolicy,
) -> Option<CompactStats> {
    let transcript = store.messages();
    if !should_compact_transcript(&transcript, policy) {
        return None;
    }

    let compacted = compact_transcript_if_needed(
        &transcript,
        store.next_message_id(),
        store.next_block_id(),
        policy,
    )?;
    let stats = compacted.stats;
    store.replace_all(compacted.messages);
    Some(stats)
}

/// Builds a compacted transcript by summarizing older settled messages.
pub(crate) fn compact_transcript_if_needed(
    transcript: &[ChatMessage],
    compact_message_id: ChatMessageId,
    compact_block_id: ChatBlockId,
    policy: CompactPolicy,
) -> Option<CompactedTranscript> {
    if !should_compact_transcript(transcript, policy) || policy.summary_max_bytes == 0 {
        return None;
    }
    let split = compact_split_index(transcript, policy)?;
    let compacted_messages = &transcript[..split];
    if compacted_messages.is_empty() {
        return None;
    }

    let summary = local_compact_summary(compacted_messages, policy.summary_max_bytes);
    if summary.is_empty() {
        return None;
    }
    let before_tokens = estimate_transcript_tokens(compacted_messages);
    let after_tokens = estimate_text_tokens(&summary);
    let compact_message = ChatMessage::new(
        compact_message_id,
        ChatRole::System,
        vec![ChatBlock::Compact(CompactBlock {
            id: compact_block_id,
            status: CompactStatus::Complete,
            before_tokens: Some(before_tokens),
            after_tokens: Some(after_tokens),
            summary,
        })],
    );

    let mut messages = Vec::with_capacity(transcript.len() - split + 1);
    messages.push(compact_message);
    messages.extend_from_slice(&transcript[split..]);
    Some(CompactedTranscript {
        messages,
        stats: CompactStats {
            compacted_messages: compacted_messages.len(),
            before_tokens,
            after_tokens,
        },
    })
}

fn should_compact_transcript(transcript: &[ChatMessage], policy: CompactPolicy) -> bool {
    let Some(split) = compact_split_index(transcript, policy) else {
        return false;
    };
    split > 0
        && estimate_transcript_tokens(transcript) > policy.threshold_tokens
        && policy.summary_max_bytes > 0
}

fn compact_split_index(transcript: &[ChatMessage], policy: CompactPolicy) -> Option<usize> {
    let recent_limit = policy.recent_message_limit.max(1);
    if transcript.len() <= recent_limit {
        return None;
    }

    let desired_split = transcript.len().saturating_sub(recent_limit);
    let split = transcript[..desired_split]
        .iter()
        .position(|message| !message_is_compactable(message))
        .unwrap_or(desired_split);
    (split > 0).then_some(split)
}

fn message_is_compactable(message: &ChatMessage) -> bool {
    if message.status.is_streaming() {
        return false;
    }
    message.blocks.iter().all(block_is_compactable)
}

fn block_is_compactable(block: &ChatBlock) -> bool {
    match block {
        ChatBlock::ToolUse(tool) => {
            !matches!(tool.status, ToolStatus::Pending | ToolStatus::Running)
        }
        ChatBlock::Diff(diff) => diff.decision != EditDecision::Pending,
        ChatBlock::Plan(plan) => plan.decision != PlanDecision::Pending,
        ChatBlock::Task(task) => !matches!(task.status, TaskStatus::Pending | TaskStatus::Running),
        ChatBlock::Compact(compact) => !matches!(
            compact.status,
            CompactStatus::Pending | CompactStatus::Running
        ),
        _ => true,
    }
}

pub(crate) fn estimate_transcript_tokens(messages: &[ChatMessage]) -> u64 {
    messages.iter().map(estimate_message_tokens).sum()
}

fn estimate_message_tokens(message: &ChatMessage) -> u64 {
    let role_bytes = message.role.label().len();
    let status_bytes = turn_status_label(&message.status).len();
    let block_bytes = message
        .blocks
        .iter()
        .map(estimate_block_bytes)
        .sum::<usize>();
    estimate_bytes_as_tokens(role_bytes + status_bytes + block_bytes + 16)
}

fn estimate_block_bytes(block: &ChatBlock) -> usize {
    match block {
        ChatBlock::Text(text) => text.markdown.len(),
        ChatBlock::Thinking(thinking) => thinking.markdown.len(),
        ChatBlock::ToolUse(tool) => {
            tool.name.len() + tool.call_id.len() + format!("{:?}", tool.input).len()
        }
        ChatBlock::ToolResult(result) => result.call_id.len() + result.output.as_text().len() + 32,
        ChatBlock::Diff(diff) => diff.path.len() + diff.diff.unified.len(),
        ChatBlock::Plan(plan) => plan.items.iter().map(|item| item.text.len()).sum::<usize>() + 16,
        ChatBlock::Task(task) => task.title.len() + task.summary.len() + 32,
        ChatBlock::Todo(todo) => todo.items.iter().map(|item| item.text.len()).sum::<usize>() + 16,
        ChatBlock::Attachment(attachment) => {
            attachment.name.len()
                + attachment.url.as_deref().unwrap_or_default().len()
                + attachment.mime.as_deref().unwrap_or_default().len()
        }
        ChatBlock::Notice(notice) => notice.text.len(),
        ChatBlock::Compact(compact) => compact.summary.len() + 32,
        ChatBlock::Artifact(artifact) => artifact.title.len() + artifact.anchor.as_str().len() + 16,
    }
}

fn estimate_text_tokens(text: &str) -> u64 {
    estimate_bytes_as_tokens(text.len())
}

fn estimate_bytes_as_tokens(bytes: usize) -> u64 {
    if bytes == 0 {
        0
    } else {
        bytes.div_ceil(ESTIMATED_BYTES_PER_TOKEN) as u64
    }
}

fn local_compact_summary(messages: &[ChatMessage], max_bytes: usize) -> String {
    if max_bytes == 0 {
        return String::new();
    }

    let mut summary = String::new();
    let _ = writeln!(
        summary,
        "Local summary of {} earlier transcript message{} compacted to stay within the model context budget.",
        messages.len(),
        if messages.len() == 1 { "" } else { "s" }
    );
    summary
        .push_str("Treat this as prior conversation context replacing the full earlier turns.\n");
    for message in messages {
        let _ = writeln!(
            summary,
            "- {} [{}]: {}",
            message.role.label(),
            turn_status_label(&message.status),
            message_summary(message)
        );
        if summary.len() > max_bytes {
            break;
        }
    }
    truncate_summary(summary, max_bytes)
}

fn message_summary(message: &ChatMessage) -> String {
    let parts = message
        .blocks
        .iter()
        .filter_map(block_summary)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        "no model-visible content".to_string()
    } else {
        parts.join(" | ")
    }
}

fn block_summary(block: &ChatBlock) -> Option<String> {
    match block {
        ChatBlock::Text(text) if !text.markdown.is_empty() => {
            Some(format!("text: {}", excerpt(&text.markdown)))
        }
        ChatBlock::Thinking(thinking) if !thinking.markdown.is_empty() => {
            Some(format!("thinking: {}", excerpt(&thinking.markdown)))
        }
        ChatBlock::ToolUse(tool) => Some(format!(
            "tool_use {} call_id={} status={:?} input={}",
            tool.name,
            tool.call_id,
            tool.status,
            excerpt(&format!("{:?}", tool.input))
        )),
        ChatBlock::ToolResult(result) => Some(format!(
            "tool_result call_id={} ok={} exit_code={:?} output={}",
            result.call_id,
            result.ok,
            result.exit_code,
            excerpt(result.output.as_text())
        )),
        ChatBlock::Diff(diff) => Some(diff_summary(diff)),
        ChatBlock::Plan(plan) => Some(plan_summary(plan)),
        ChatBlock::Task(task) => Some(task_summary(task)),
        ChatBlock::Todo(todo) => Some(format!(
            "todo: {}",
            excerpt(
                &todo
                    .items
                    .iter()
                    .map(|item| item.text.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        )),
        ChatBlock::Attachment(attachment) => Some(format!("attachment: {}", attachment.name)),
        ChatBlock::Notice(notice) if !notice.text.is_empty() => {
            Some(format!("notice: {}", excerpt(&notice.text)))
        }
        ChatBlock::Compact(compact) if !compact.summary.is_empty() => Some(format!(
            "previous_compact status={} summary={}",
            compact.status.as_str(),
            excerpt(&compact.summary)
        )),
        ChatBlock::Artifact(artifact) => Some(format!(
            "artifact kind={} anchor={} title={}",
            artifact.kind.label(),
            artifact.anchor,
            artifact.title
        )),
        _ => None,
    }
}

fn diff_summary(diff: &DiffBlock) -> String {
    format!(
        "diff path={} decision={:?} patch={}",
        diff.path,
        diff.decision,
        excerpt(&diff.diff.unified)
    )
}

fn plan_summary(plan: &PlanBlock) -> String {
    format!(
        "plan decision={:?} items={}",
        plan.decision,
        excerpt(
            &plan
                .items
                .iter()
                .map(|item| item.text.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        )
    )
}

fn task_summary(task: &TaskBlock) -> String {
    format!(
        "task title={} status={:?} summary={}",
        task.title,
        task.status,
        excerpt(&task.summary)
    )
}

fn turn_status_label(status: &ChatTurnStatus) -> &'static str {
    match status {
        ChatTurnStatus::Streaming => "streaming",
        ChatTurnStatus::Complete => "complete",
        ChatTurnStatus::Failed(_) => "failed",
        ChatTurnStatus::Canceled => "canceled",
    }
}

fn excerpt(text: &str) -> String {
    let normalized = normalize_whitespace(text);
    if normalized.chars().count() <= BLOCK_EXCERPT_MAX_CHARS {
        return normalized;
    }
    let mut excerpt = normalized
        .chars()
        .take(BLOCK_EXCERPT_MAX_CHARS.saturating_sub(3))
        .collect::<String>();
    excerpt.push_str("...");
    excerpt
}

fn normalize_whitespace(text: &str) -> String {
    let mut normalized = String::new();
    let mut pending_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !normalized.is_empty() {
            normalized.push(' ');
        }
        normalized.push(ch);
        pending_space = false;
    }
    normalized
}

fn truncate_summary(summary: String, max_bytes: usize) -> String {
    if summary.len() <= max_bytes {
        return summary;
    }
    let notice = "\n[Compact summary truncated by local budget.]";
    if notice.len() >= max_bytes {
        return utf8_prefix(notice, max_bytes).to_string();
    }
    let prefix = utf8_prefix(&summary, max_bytes - notice.len());
    format!("{prefix}{notice}")
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

#[cfg(test)]
mod tests {
    use atto_ui_chat::{
        ChatBlock, ChatMessage, ChatRole, ChatTurnStatus, PlanDecision, ToolInput, ToolUseBlock,
    };

    use super::*;

    fn small_policy() -> CompactPolicy {
        CompactPolicy {
            threshold_tokens: 1,
            recent_message_limit: 2,
            summary_max_bytes: 4096,
        }
    }

    #[test]
    fn compact_transcript_replaces_older_messages_and_preserves_recent_messages() {
        let long_tail = "details ".repeat(1_000);
        let transcript = vec![
            ChatMessage::text(
                1,
                ChatRole::User,
                format!("old user zero full body {long_tail}"),
            ),
            ChatMessage::text(
                2,
                ChatRole::Assistant,
                format!("old assistant zero full body {long_tail}"),
            ),
            ChatMessage::text(
                3,
                ChatRole::User,
                format!("old user one full body {long_tail}"),
            ),
            ChatMessage::text(4, ChatRole::Assistant, "recent assistant keep"),
            ChatMessage::text(5, ChatRole::User, "current prompt keep"),
        ];

        let compacted = compact_transcript_if_needed(
            &transcript,
            ChatMessageId::new(99),
            ChatBlockId::new(100),
            small_policy(),
        )
        .expect("transcript should compact");

        assert_eq!(compacted.stats.compacted_messages, 3);
        assert!(compacted.stats.before_tokens > compacted.stats.after_tokens);
        assert_eq!(compacted.messages.len(), 3);
        match &compacted.messages[0].blocks[0] {
            ChatBlock::Compact(compact) => {
                assert_eq!(compact.status, CompactStatus::Complete);
                assert_eq!(compact.before_tokens, Some(compacted.stats.before_tokens));
                assert_eq!(compact.after_tokens, Some(compacted.stats.after_tokens));
                assert!(compact.summary.contains("old user zero full body"));
                assert!(compact.summary.contains("old assistant zero full body"));
            }
            other => panic!("expected compact block, got {other:?}"),
        }
        assert_eq!(compacted.messages[1], transcript[3]);
        assert_eq!(compacted.messages[2], transcript[4]);
    }

    #[test]
    fn compact_transcript_skips_under_budget_transcripts() {
        let transcript = vec![
            ChatMessage::text(1, ChatRole::User, "hello"),
            ChatMessage::text(2, ChatRole::Assistant, "world"),
            ChatMessage::text(3, ChatRole::User, "again"),
        ];
        let policy = CompactPolicy {
            threshold_tokens: 10_000,
            recent_message_limit: 1,
            summary_max_bytes: 4096,
        };

        assert!(
            compact_transcript_if_needed(
                &transcript,
                ChatMessageId::new(99),
                ChatBlockId::new(100),
                policy,
            )
            .is_none()
        );
    }

    #[test]
    fn compact_transcript_does_not_remove_pending_interactive_blocks() {
        let pending_plan = ChatMessage::new(
            2,
            ChatRole::Assistant,
            vec![ChatBlock::Plan(atto_ui_chat::PlanBlock {
                id: ChatBlockId::new(20),
                items: vec![atto_ui_chat::PlanItem {
                    text: "Wait for approval.".to_string(),
                }],
                decision: PlanDecision::Pending,
            })],
        );
        let transcript = vec![
            ChatMessage::text(1, ChatRole::User, "old message"),
            pending_plan,
            ChatMessage::text(3, ChatRole::User, "current prompt"),
        ];

        let compacted = compact_transcript_if_needed(
            &transcript,
            ChatMessageId::new(99),
            ChatBlockId::new(100),
            CompactPolicy {
                threshold_tokens: 1,
                recent_message_limit: 1,
                summary_max_bytes: 4096,
            },
        )
        .expect("safe prefix before pending plan should compact");

        assert_eq!(compacted.stats.compacted_messages, 1);
        assert_eq!(compacted.messages.len(), 3);
        assert_eq!(compacted.messages[1].id, ChatMessageId::new(2));
    }

    #[test]
    fn compact_transcript_preserves_utf8_when_summary_budget_truncates() {
        let transcript = vec![
            ChatMessage::text(1, ChatRole::User, "旧消息".repeat(128)),
            ChatMessage::new(
                2,
                ChatRole::Assistant,
                vec![ChatBlock::ToolUse(ToolUseBlock {
                    id: ChatBlockId::new(20),
                    call_id: "call_done".to_string(),
                    name: "read_file".to_string(),
                    input: ToolInput::Text("{}".to_string()),
                    status: ToolStatus::Done,
                    approval: None,
                    collapsed: false,
                })],
            ),
            ChatMessage::text(3, ChatRole::User, "current"),
        ];

        let compacted = compact_transcript_if_needed(
            &transcript,
            ChatMessageId::new(99),
            ChatBlockId::new(100),
            CompactPolicy {
                threshold_tokens: 1,
                recent_message_limit: 1,
                summary_max_bytes: 180,
            },
        )
        .expect("transcript should compact");

        let ChatBlock::Compact(compact) = &compacted.messages[0].blocks[0] else {
            panic!("expected compact block");
        };
        assert!(compact.summary.len() <= 180);
        assert!(std::str::from_utf8(compact.summary.as_bytes()).is_ok());
        assert!(
            compact
                .summary
                .contains("Compact summary truncated by local budget")
        );
    }

    #[test]
    fn compact_store_replaces_messages_and_invalidates_old_branch() {
        let store = ChatMessageStore::new();
        store.push(ChatMessage::text(1, ChatRole::User, "old"));
        store.push(ChatMessage::text(2, ChatRole::Assistant, "middle"));
        store.push(ChatMessage::text(3, ChatRole::User, "current"));
        let old_branch = store.branch_token();

        let stats = compact_store_if_needed(&store, small_policy()).expect("store should compact");

        assert_eq!(stats.compacted_messages, 1);
        assert!(!store.is_branch_current(old_branch));
        let messages = store.messages();
        assert_eq!(messages.len(), 3);
        assert!(matches!(messages[0].blocks[0], ChatBlock::Compact(_)));
        assert_eq!(messages[1].id, ChatMessageId::new(2));
        assert_eq!(messages[2].id, ChatMessageId::new(3));
    }

    #[test]
    fn compact_transcript_skips_streaming_prefix() {
        let transcript = vec![
            ChatMessage::text(1, ChatRole::Assistant, "partial")
                .with_status(ChatTurnStatus::Streaming),
            ChatMessage::text(2, ChatRole::User, "current"),
        ];

        assert!(
            compact_transcript_if_needed(
                &transcript,
                ChatMessageId::new(99),
                ChatBlockId::new(100),
                CompactPolicy {
                    threshold_tokens: 1,
                    recent_message_limit: 1,
                    summary_max_bytes: 4096,
                },
            )
            .is_none()
        );
    }
}
