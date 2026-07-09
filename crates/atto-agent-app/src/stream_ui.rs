//! Maps typed DeepSeek stream events into main-thread UI actions.

use atto_ui_chat::{
    ChatBlockId, ChatBranchToken, ChatError, ChatMessageId, ChatMessageMeta, StopReason, TokenUsage,
};

use crate::AppAction;
use crate::deepseek::{
    ChatCompletionChunk, ChatCompletionSseEvent, CompletionUsage, FinishReason,
    chat_error_from_api_error,
};

/// Stateful mapper for one assistant turn's DeepSeek stream.
#[derive(Clone, Debug)]
pub(crate) struct DeepSeekUiStream {
    branch: ChatBranchToken,
    message_id: ChatMessageId,
    text_block_id: ChatBlockId,
    meta: ChatMessageMeta,
    finished: bool,
}

impl DeepSeekUiStream {
    /// Creates a mapper bound to the current transcript branch and assistant turn.
    pub(crate) fn new(
        branch: ChatBranchToken,
        message_id: ChatMessageId,
        text_block_id: ChatBlockId,
        model: impl Into<String>,
    ) -> Self {
        Self {
            branch,
            message_id,
            text_block_id,
            meta: ChatMessageMeta {
                model: Some(model.into()),
                ..ChatMessageMeta::default()
            },
            finished: false,
        }
    }

    /// Converts one parsed SSE event into UI actions for the main thread.
    pub(crate) fn map_event(&mut self, event: ChatCompletionSseEvent) -> Vec<AppAction> {
        if self.finished {
            return Vec::new();
        }

        match event {
            ChatCompletionSseEvent::Chunk(chunk) => self.map_chunk(chunk),
            ChatCompletionSseEvent::Done => {
                self.finished = true;
                vec![AppAction::TurnDone {
                    branch: self.branch,
                    message_id: self.message_id,
                    meta: Some(self.meta.clone()),
                }]
            }
            ChatCompletionSseEvent::Error(error) => {
                self.map_error(chat_error_from_api_error(error))
            }
        }
    }

    /// Converts a prepared chat error into a failed assistant turn.
    pub(crate) fn map_error(&mut self, error: ChatError) -> Vec<AppAction> {
        if self.finished {
            return Vec::new();
        }
        self.fail(error)
    }

    fn map_chunk(&mut self, chunk: ChatCompletionChunk) -> Vec<AppAction> {
        if let Some(model) = chunk.model.filter(|model| !model.is_empty()) {
            self.meta.model = Some(model);
        }
        if let Some(usage) = chunk.usage {
            self.meta.usage = Some(token_usage_from_completion(usage));
        }

        let mut actions = Vec::new();
        for choice in chunk.choices {
            if let Some(reasoning) = choice.delta.reasoning_content
                && !reasoning.is_empty()
            {
                actions.push(AppAction::ThinkingDelta {
                    branch: self.branch,
                    message_id: self.message_id,
                    delta: reasoning,
                });
            }
            if let Some(content) = choice.delta.content
                && !content.is_empty()
            {
                actions.push(AppAction::TextDelta {
                    branch: self.branch,
                    block_id: self.text_block_id,
                    delta: content,
                });
            }
            if let Some(finish_reason) = choice.finish_reason {
                self.meta.stop_reason = stop_reason_from_finish(finish_reason);
            }
        }
        actions
    }

    fn fail(&mut self, error: ChatError) -> Vec<AppAction> {
        self.finished = true;
        vec![AppAction::TurnFailed {
            branch: self.branch,
            message_id: self.message_id,
            error,
        }]
    }
}

fn token_usage_from_completion(usage: CompletionUsage) -> TokenUsage {
    TokenUsage {
        input: usage.prompt_tokens.into(),
        output: usage.completion_tokens.into(),
    }
}

fn stop_reason_from_finish(reason: FinishReason) -> Option<StopReason> {
    match reason {
        FinishReason::Stop => Some(StopReason::EndTurn),
        FinishReason::Length => Some(StopReason::MaxTokens),
        FinishReason::ToolCalls => Some(StopReason::ToolUse),
        FinishReason::ContentFilter => Some(StopReason::Refusal),
        FinishReason::Other => None,
    }
}
