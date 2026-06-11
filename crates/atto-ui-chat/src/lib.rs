//! Chat message list components.

mod dynamic;
mod input;
mod list;
mod message;
mod panel;
mod store;
mod viewer;

pub use dynamic::{
    chat_input_panel_schema, chat_message_list_schema, register_chat_input_panel,
    register_chat_message_list, register_runtime_components,
};
pub use input::{
    ChatChoiceInputConfig, ChatConfirmInputConfig, ChatInputHandle, ChatInputMode, ChatInputPanel,
    ChatInputResponse, ChatTextInputConfig,
};
pub use list::{
    ApprovalDecision, ChatMessageList, EditDecisionEvent, MessageAction, MessageActionKind,
    PlanDecisionEvent,
};
pub use message::{
    ApprovalOption, ApprovalRequest, Artifact, ArtifactBlock, ArtifactId, ArtifactKind,
    AttachmentBlock, ChatAlignment, ChatBlock, ChatBlockId, ChatError, ChatErrorKind, ChatMessage,
    ChatMessageId, ChatMessageMeta, ChatRole, ChatTurnStatus, DiffBlock, DiffData, EditDecision,
    NoticeBlock, NoticeLevel, PlanBlock, PlanDecision, PlanItem, StopReason, TextBlock,
    ThinkingBlock, TodoBlock, TodoItem, TodoState, TokenUsage, ToolInput, ToolOutput,
    ToolResultBlock, ToolStatus, ToolUseBlock,
};
pub use panel::ChatPanel;
pub use store::ChatMessageStore;
pub use viewer::{ArtifactViewer, TextArtifactViewer};
