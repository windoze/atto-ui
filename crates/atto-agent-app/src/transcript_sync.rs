//! Transcript status synchronization: thinking-delta appending, token/tool-count
//! status formatting, and error-summary derivation.

use crate::*;

pub(crate) fn append_thinking_delta(
    store: &ChatMessageStore,
    message_id: ChatMessageId,
    delta: &str,
) -> bool {
    if let Some(block_id) = thinking_block_id(store, message_id) {
        return store.append_text_delta(block_id, delta);
    }
    if delta.is_empty() {
        return store
            .messages()
            .iter()
            .any(|message| message.id == message_id);
    }

    let block_id = store.next_block_id();
    let mut inserted = false;
    store.update_message(message_id, |message| {
        if message
            .blocks
            .iter()
            .any(|block| matches!(block, ChatBlock::Thinking(_)))
        {
            return;
        }
        let insert_at = message
            .blocks
            .iter()
            .position(|block| matches!(block, ChatBlock::Text(_)))
            .unwrap_or(message.blocks.len());
        message.blocks.insert(
            insert_at,
            ChatBlock::Thinking(ThinkingBlock {
                id: block_id,
                markdown: delta.to_string(),
                streaming: message.status.is_streaming(),
                collapsed: true,
            }),
        );
        inserted = true;
    });
    if inserted {
        true
    } else {
        thinking_block_id(store, message_id)
            .is_some_and(|block_id| store.append_text_delta(block_id, delta))
    }
}

pub(crate) fn thinking_block_id(
    store: &ChatMessageStore,
    message_id: ChatMessageId,
) -> Option<ChatBlockId> {
    store
        .messages()
        .iter()
        .find(|message| message.id == message_id)
        .and_then(|message| {
            message.blocks.iter().find_map(|block| match block {
                ChatBlock::Thinking(thinking) => Some(thinking.id),
                _ => None,
            })
        })
}

pub(crate) fn sync_transcript_status(
    store: &ChatMessageStore,
    token_estimate_state: &Property<String>,
    error_summary_state: &Property<String>,
) {
    let messages = store.messages();
    token_estimate_state.set(format_token_estimate_status(estimate_transcript_tokens(
        &messages,
    )));
    error_summary_state.set(error_summary_status(&messages));
}

pub(crate) fn format_tool_count_status(count: usize) -> String {
    format!("tools: {count}")
}

pub(crate) fn format_token_estimate_status(tokens: u64) -> String {
    format!("tokens~{tokens}")
}

pub(crate) fn error_summary_status(messages: &[ChatMessage]) -> String {
    latest_error_summary(messages).unwrap_or_else(|| "err:ok".to_string())
}

pub(crate) fn latest_error_summary(messages: &[ChatMessage]) -> Option<String> {
    for message in messages.iter().rev() {
        if let ChatTurnStatus::Failed(error) = &message.status {
            return Some(format_status_error_summary(
                chat_error_kind_label(&error.kind),
                &error.message,
            ));
        }
        for block in message.blocks.iter().rev() {
            if let ChatBlock::ToolResult(result) = block
                && !result.ok
            {
                return Some(format_status_error_summary("tool", result.output.as_text()));
            }
        }
    }
    None
}

pub(crate) fn format_status_error_summary(kind: &str, message: &str) -> String {
    truncate_status_text(
        &format!("err:{kind} {}", normalize_status_text(message)),
        36,
    )
}

pub(crate) fn normalize_status_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn truncate_status_text(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let prefix = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_none() {
        return prefix;
    }
    let keep = max_chars.saturating_sub(3);
    format!("{}...", text.chars().take(keep).collect::<String>())
}

pub(crate) fn chat_error_kind_label(kind: &ChatErrorKind) -> &'static str {
    match kind {
        ChatErrorKind::Api => "api",
        ChatErrorKind::Tool => "tool",
        ChatErrorKind::RateLimit => "rate",
        ChatErrorKind::Refusal => "refusal",
        ChatErrorKind::Network => "network",
        ChatErrorKind::Other => "other",
    }
}
