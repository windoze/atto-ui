use atto_ui::composable::Identifiable;

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
pub enum ChatMessageContent {
    Text { markdown: String },
    File { name: String, url: Option<String> },
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
