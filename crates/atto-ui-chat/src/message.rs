use std::fmt;

use atto_ui::ComponentValue;
use atto_ui::composable::Identifiable;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArtifactId(String);

impl ArtifactId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ArtifactId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ArtifactId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<u64> for ArtifactId {
    fn from(value: u64) -> Self {
        Self(value.to_string())
    }
}

impl fmt::Display for ArtifactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ArtifactKind {
    Code,
    Diff,
    File,
}

impl ArtifactKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactKind::Code => "code",
            ArtifactKind::Diff => "diff",
            ArtifactKind::File => "file",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ArtifactKind::Code => "Code",
            ArtifactKind::Diff => "Diff",
            ArtifactKind::File => "File",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub id: ArtifactId,
    pub kind: ArtifactKind,
    pub title: String,
    pub content: String,
}

impl Artifact {
    pub fn new(
        id: impl Into<ArtifactId>,
        kind: ArtifactKind,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            title: title.into(),
            content: content.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChatMessageId(pub u64);

impl ChatMessageId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

impl From<u64> for ChatMessageId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<ChatMessageId> for u64 {
    fn from(value: ChatMessageId) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChatBlockId(pub u64);

impl ChatBlockId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

impl From<u64> for ChatBlockId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<ChatBlockId> for u64 {
    fn from(value: ChatBlockId) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatAlignment {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Assistant,
    System,
    Custom(String),
}

impl ChatRole {
    pub fn label(&self) -> String {
        match self {
            ChatRole::User => "User".to_string(),
            ChatRole::Assistant => "Assistant".to_string(),
            ChatRole::System => "System".to_string(),
            ChatRole::Custom(name) => name.clone(),
        }
    }

    pub fn alignment(&self) -> ChatAlignment {
        match self {
            ChatRole::User => ChatAlignment::Right,
            _ => ChatAlignment::Left,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatTurnStatus {
    Streaming,
    Complete,
    Failed(ChatError),
    Canceled,
}

impl ChatTurnStatus {
    pub fn is_streaming(&self) -> bool {
        matches!(self, Self::Streaming)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatError {
    pub kind: ChatErrorKind,
    pub message: String,
    pub detail: Option<String>,
}

impl ChatError {
    pub fn new(kind: ChatErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatErrorKind {
    Api,
    Tool,
    RateLimit,
    Refusal,
    Network,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    StopSequence,
    Refusal,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatMessageMeta {
    pub timestamp: Option<String>,
    pub model: Option<String>,
    pub usage: Option<TokenUsage>,
    pub elapsed_ms: Option<u64>,
    pub stop_reason: Option<StopReason>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChatBlock {
    Text(TextBlock),
    ToolUse(ToolUseBlock),
    ToolResult(ToolResultBlock),
    Attachment(AttachmentBlock),
    Artifact(ArtifactBlock),
}

impl ChatBlock {
    pub fn id(&self) -> ChatBlockId {
        match self {
            ChatBlock::Text(block) => block.id,
            ChatBlock::ToolUse(block) => block.id,
            ChatBlock::ToolResult(block) => block.id,
            ChatBlock::Attachment(block) => block.id,
            ChatBlock::Artifact(block) => block.id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextBlock {
    pub id: ChatBlockId,
    pub markdown: String,
    pub streaming: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolUseBlock {
    pub id: ChatBlockId,
    pub call_id: String,
    pub name: String,
    pub input: ToolInput,
    pub status: ToolStatus,
    pub approval: Option<ApprovalRequest>,
    pub collapsed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolInput {
    Text(String),
    Json(ComponentValue),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolStatus {
    Pending,
    Running,
    Done,
    Error,
    Canceled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub id: String,
    pub prompt: String,
    pub options: Vec<ApprovalOption>,
    pub resolved: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalOption {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolResultBlock {
    pub id: ChatBlockId,
    pub call_id: String,
    pub ok: bool,
    pub exit_code: Option<i32>,
    pub output: ToolOutput,
    pub collapsed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolOutput {
    Ansi(String),
    Markdown(String),
}

impl ToolOutput {
    pub fn as_text(&self) -> &str {
        match self {
            ToolOutput::Ansi(output) | ToolOutput::Markdown(output) => output,
        }
    }

    pub fn set_text(&mut self, output: String) {
        match self {
            ToolOutput::Ansi(current) | ToolOutput::Markdown(current) => *current = output,
        }
    }

    pub fn append_delta(&mut self, delta: &str) {
        match self {
            ToolOutput::Ansi(output) | ToolOutput::Markdown(output) => output.push_str(delta),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentBlock {
    pub id: ChatBlockId,
    pub name: String,
    pub url: Option<String>,
    pub mime: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactBlock {
    pub id: ChatBlockId,
    pub kind: ArtifactKind,
    pub anchor: ArtifactId,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChatMessage {
    pub id: ChatMessageId,
    pub role: ChatRole,
    pub blocks: Vec<ChatBlock>,
    pub status: ChatTurnStatus,
    pub meta: ChatMessageMeta,
}

impl ChatMessage {
    pub fn new(id: impl Into<ChatMessageId>, role: ChatRole, blocks: Vec<ChatBlock>) -> Self {
        Self {
            id: id.into(),
            role,
            blocks,
            status: ChatTurnStatus::Complete,
            meta: ChatMessageMeta::default(),
        }
    }

    pub fn text(id: impl Into<ChatMessageId>, role: ChatRole, markdown: impl Into<String>) -> Self {
        let id = id.into();
        Self::new(
            id,
            role,
            vec![ChatBlock::Text(TextBlock {
                id: derived_block_id(id, 0),
                markdown: markdown.into(),
                streaming: false,
            })],
        )
    }

    pub fn file(
        id: impl Into<ChatMessageId>,
        role: ChatRole,
        name: impl Into<String>,
        url: Option<String>,
    ) -> Self {
        let id = id.into();
        Self::new(
            id,
            role,
            vec![ChatBlock::Attachment(AttachmentBlock {
                id: derived_block_id(id, 0),
                name: name.into(),
                url,
                mime: None,
            })],
        )
    }

    pub fn tool_call(
        id: impl Into<ChatMessageId>,
        name: impl Into<String>,
        status: ToolStatus,
        output: impl Into<String>,
    ) -> Self {
        let id = id.into();
        let name = name.into();
        let call_id = format!("tool-{}", id.0);
        let mut blocks = vec![ChatBlock::ToolUse(ToolUseBlock {
            id: derived_block_id(id, 0),
            call_id: call_id.clone(),
            name,
            input: ToolInput::Text(String::new()),
            status,
            approval: None,
            collapsed: false,
        })];
        let output = output.into();
        if !output.is_empty() {
            blocks.push(ChatBlock::ToolResult(ToolResultBlock {
                id: derived_block_id(id, 1),
                call_id,
                ok: status != ToolStatus::Error,
                exit_code: None,
                output: ToolOutput::Ansi(output),
                collapsed: false,
            }));
        }
        Self::new(id, ChatRole::Assistant, blocks)
    }

    pub fn artifact(
        id: impl Into<ChatMessageId>,
        role: ChatRole,
        kind: ArtifactKind,
        anchor: impl Into<ArtifactId>,
        title: impl Into<String>,
    ) -> Self {
        let id = id.into();
        Self::new(
            id,
            role,
            vec![ChatBlock::Artifact(ArtifactBlock {
                id: derived_block_id(id, 0),
                kind,
                anchor: anchor.into(),
                title: title.into(),
            })],
        )
    }

    pub fn with_timestamp(mut self, timestamp: impl Into<String>) -> Self {
        self.meta.timestamp = Some(timestamp.into());
        self
    }

    pub fn with_status(mut self, status: ChatTurnStatus) -> Self {
        self.set_turn_status(status);
        self
    }

    pub fn set_turn_status(&mut self, status: ChatTurnStatus) {
        let streaming = status.is_streaming();
        self.status = status;
        for block in &mut self.blocks {
            if let ChatBlock::Text(text) = block {
                text.streaming = streaming;
            }
        }
    }

    pub(crate) fn first_text(&self) -> Option<&TextBlock> {
        self.blocks.iter().find_map(|block| match block {
            ChatBlock::Text(text) => Some(text),
            _ => None,
        })
    }

    pub(crate) fn first_text_mut(&mut self) -> Option<&mut TextBlock> {
        self.blocks.iter_mut().find_map(|block| match block {
            ChatBlock::Text(text) => Some(text),
            _ => None,
        })
    }

    pub(crate) fn first_tool_use(&self) -> Option<&ToolUseBlock> {
        self.blocks.iter().find_map(|block| match block {
            ChatBlock::ToolUse(tool) => Some(tool),
            _ => None,
        })
    }

    pub(crate) fn first_tool_use_mut(&mut self) -> Option<&mut ToolUseBlock> {
        self.blocks.iter_mut().find_map(|block| match block {
            ChatBlock::ToolUse(tool) => Some(tool),
            _ => None,
        })
    }

    pub(crate) fn first_tool_result(&self) -> Option<&ToolResultBlock> {
        let call_id = self.first_tool_use().map(|tool| tool.call_id.as_str())?;
        self.blocks.iter().find_map(|block| match block {
            ChatBlock::ToolResult(result) if result.call_id == call_id => Some(result),
            _ => None,
        })
    }

    pub(crate) fn first_tool_result_mut(&mut self) -> Option<&mut ToolResultBlock> {
        let call_id = self.first_tool_use().map(|tool| tool.call_id.clone())?;
        self.blocks.iter_mut().find_map(|block| match block {
            ChatBlock::ToolResult(result) if result.call_id == call_id => Some(result),
            _ => None,
        })
    }

    pub(crate) fn ensure_first_tool_result_mut(&mut self) -> Option<&mut ToolResultBlock> {
        let call_id = self.first_tool_use().map(|tool| tool.call_id.clone())?;
        let existing = self.blocks.iter().position(
            |block| matches!(block, ChatBlock::ToolResult(result) if result.call_id == call_id),
        );
        let index = match existing {
            Some(index) => index,
            None => {
                let index = self.blocks.len();
                self.blocks.push(ChatBlock::ToolResult(ToolResultBlock {
                    id: derived_block_id(self.id, index as u64),
                    call_id,
                    ok: true,
                    exit_code: None,
                    output: ToolOutput::Ansi(String::new()),
                    collapsed: false,
                }));
                index
            }
        };
        match self.blocks.get_mut(index) {
            Some(ChatBlock::ToolResult(result)) => Some(result),
            _ => None,
        }
    }
}

impl Identifiable for ChatMessage {
    type Id = ChatMessageId;

    fn id(&self) -> Self::Id {
        self.id
    }
}

fn derived_block_id(message_id: ChatMessageId, ordinal: u64) -> ChatBlockId {
    ChatBlockId(
        message_id
            .0
            .saturating_mul(1_000)
            .saturating_add(ordinal + 1),
    )
}
