use std::fmt;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatAlignment {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatSender {
    User,
    Assistant,
    System,
    Tool(String),
    Custom(String),
}

impl ChatSender {
    pub fn label(&self) -> String {
        match self {
            ChatSender::User => "User".to_string(),
            ChatSender::Assistant => "Assistant".to_string(),
            ChatSender::System => "System".to_string(),
            ChatSender::Tool(name) => format!("Tool:{name}"),
            ChatSender::Custom(name) => name.clone(),
        }
    }

    pub fn alignment(&self) -> ChatAlignment {
        match self {
            ChatSender::User => ChatAlignment::Right,
            _ => ChatAlignment::Left,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatMessageStatus {
    Final,
    InProgress,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatToolCallStatus {
    Running,
    Done,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatMessageContent {
    Text {
        markdown: String,
    },
    File {
        name: String,
        url: Option<String>,
    },
    ToolCall {
        name: String,
        status: ChatToolCallStatus,
        output: String,
    },
    Artifact {
        kind: ArtifactKind,
        anchor: ArtifactId,
        title: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMessage {
    pub id: ChatMessageId,
    pub sender: ChatSender,
    pub timestamp: Option<String>,
    pub status: ChatMessageStatus,
    pub content: ChatMessageContent,
}

impl ChatMessage {
    pub fn text(
        id: impl Into<ChatMessageId>,
        sender: ChatSender,
        markdown: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            sender,
            timestamp: None,
            status: ChatMessageStatus::Final,
            content: ChatMessageContent::Text {
                markdown: markdown.into(),
            },
        }
    }

    pub fn file(
        id: impl Into<ChatMessageId>,
        sender: ChatSender,
        name: impl Into<String>,
        url: Option<String>,
    ) -> Self {
        Self {
            id: id.into(),
            sender,
            timestamp: None,
            status: ChatMessageStatus::Final,
            content: ChatMessageContent::File {
                name: name.into(),
                url,
            },
        }
    }

    pub fn tool_call(
        id: impl Into<ChatMessageId>,
        name: impl Into<String>,
        status: ChatToolCallStatus,
        output: impl Into<String>,
    ) -> Self {
        let name = name.into();
        Self {
            id: id.into(),
            sender: ChatSender::Tool(name.clone()),
            timestamp: None,
            status: ChatMessageStatus::Final,
            content: ChatMessageContent::ToolCall {
                name,
                status,
                output: output.into(),
            },
        }
    }

    pub fn artifact(
        id: impl Into<ChatMessageId>,
        sender: ChatSender,
        kind: ArtifactKind,
        anchor: impl Into<ArtifactId>,
        title: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            sender,
            timestamp: None,
            status: ChatMessageStatus::Final,
            content: ChatMessageContent::Artifact {
                kind,
                anchor: anchor.into(),
                title: title.into(),
            },
        }
    }

    pub fn with_timestamp(mut self, timestamp: impl Into<String>) -> Self {
        self.timestamp = Some(timestamp.into());
        self
    }

    pub fn with_status(mut self, status: ChatMessageStatus) -> Self {
        self.status = status;
        self
    }
}

impl Identifiable for ChatMessage {
    type Id = ChatMessageId;

    fn id(&self) -> Self::Id {
        self.id
    }
}
