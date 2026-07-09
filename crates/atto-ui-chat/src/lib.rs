//! Chat message list components.

mod completion;
mod dynamic;
mod input;
mod list;
mod message;
mod panel;
mod store;
mod viewer;

pub use completion::{CompletionAnchor, CompletionItem, CompletionPlacement, CompletionPopup};
pub use dynamic::{
    chat_input_panel_schema, chat_message_list_schema, register_chat_input_panel,
    register_chat_message_list, register_runtime_components,
};
pub use input::{
    ChatChoiceInputConfig, ChatConfirmInputConfig, ChatInputHandle, ChatInputMode, ChatInputPanel,
    ChatInputReference, ChatInputResponse, ChatMentionCandidate, ChatMentionContext,
    ChatSlashCommand, ChatSlashCommandAction, ChatTextInputConfig,
};
pub use list::{
    ApprovalDecision, ChatMessageList, EditAndResubmitEvent, EditDecisionEvent, MessageAction,
    MessageActionKind, PlanDecisionEvent,
};
pub use message::{
    ApprovalOption, ApprovalRequest, Artifact, ArtifactBlock, ArtifactId, ArtifactKind,
    AttachmentBlock, ChatAlignment, ChatBlock, ChatBlockId, ChatError, ChatErrorKind, ChatMessage,
    ChatMessageId, ChatMessageMeta, ChatRole, ChatTurnStatus, DiffBlock, DiffData, EditDecision,
    NoticeBlock, NoticeLevel, PlanBlock, PlanDecision, PlanItem, StopReason, TaskBlock, TaskStatus,
    TaskTranscriptItem, TextBlock, ThinkingBlock, TodoBlock, TodoItem, TodoState, TokenUsage,
    ToolInput, ToolOutput, ToolResultBlock, ToolStatus, ToolUseBlock,
};
pub use panel::ChatPanel;
pub use store::{ChatBranchToken, ChatMessageStore};
pub use viewer::{ArtifactViewer, TextArtifactViewer};
