//! Optional JSONL persistence for the agent transcript.
//!
//! The file format is intentionally owned by the app crate: each JSONL row is
//! one chat message plus a schema version, so reusable UI crates do not need to
//! expose their internal Rust structs as a persistence format.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use atto_ui::ComponentValue;
use atto_ui_chat::{
    ApprovalAction, ApprovalLevel, ApprovalOption, ApprovalRequest, ApprovalResolution,
    ArtifactBlock, ArtifactId, ArtifactKind, AttachmentBlock, ChatBlock, ChatBlockId, ChatError,
    ChatErrorKind, ChatMessage, ChatMessageId, ChatMessageMeta, ChatRole, ChatTurnStatus,
    CompactBlock, CompactStatus, DiffBlock, DiffData, EditDecision, NoticeBlock, NoticeLevel,
    PlanBlock, PlanDecision, PlanItem, StopReason, TaskBlock, TaskStatus, TaskTranscriptItem,
    TextBlock, ThinkingBlock, TodoBlock, TodoItem, TodoState, TokenUsage, ToolInput, ToolOutput,
    ToolResultBlock, ToolStatus, ToolUseBlock,
};
use serde::{Deserialize, Serialize};

const TRANSCRIPT_SCHEMA_VERSION: u32 = 1;

/// Saves the transcript to a JSONL file, one serialized chat message per row.
pub fn save_transcript_jsonl(path: &Path, messages: &[ChatMessage]) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create transcript directory `{}`",
                parent.display()
            )
        })?;
    }

    let temp_path = temporary_path_for(path)?;
    let file = File::create(&temp_path)
        .with_context(|| format!("failed to create transcript file `{}`", temp_path.display()))?;
    let mut writer = BufWriter::new(file);
    for message in messages {
        let row = StoredTranscriptRow {
            schema_version: TRANSCRIPT_SCHEMA_VERSION,
            message: StoredChatMessage::from(message),
        };
        serde_json::to_writer(&mut writer, &row).context("failed to serialize transcript row")?;
        writer
            .write_all(b"\n")
            .context("failed to write transcript row separator")?;
    }
    writer
        .flush()
        .with_context(|| format!("failed to flush transcript file `{}`", temp_path.display()))?;
    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "failed to replace transcript file `{}` with `{}`",
            path.display(),
            temp_path.display()
        )
    })?;
    Ok(())
}

/// Loads a JSONL transcript file. Missing files restore an empty transcript.
pub fn load_transcript_jsonl(path: &Path) -> Result<Vec<ChatMessage>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to open transcript file `{}`", path.display()));
        }
    };

    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line_number = index + 1;
        let line = line.with_context(|| {
            format!(
                "failed to read transcript JSONL line {line_number} from `{}`",
                path.display()
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let row: StoredTranscriptRow = serde_json::from_str(&line).with_context(|| {
            format!(
                "failed to parse transcript JSONL line {line_number} from `{}`",
                path.display()
            )
        })?;
        if row.schema_version != TRANSCRIPT_SCHEMA_VERSION {
            bail!(
                "unsupported transcript schema version {} on line {line_number}; expected {}",
                row.schema_version,
                TRANSCRIPT_SCHEMA_VERSION
            );
        }
        messages.push(row.message.try_into().with_context(|| {
            format!(
                "failed to decode transcript message on line {line_number} from `{}`",
                path.display()
            )
        })?);
    }
    Ok(messages)
}

fn temporary_path_for(path: &Path) -> Result<PathBuf> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .context("transcript path must include a file name")?;
    Ok(path.with_file_name(format!(".{file_name}.tmp-{}", std::process::id())))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredTranscriptRow {
    schema_version: u32,
    message: StoredChatMessage,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredChatMessage {
    id: u64,
    role: StoredChatRole,
    blocks: Vec<StoredChatBlock>,
    status: StoredChatTurnStatus,
    meta: StoredChatMessageMeta,
}

impl From<&ChatMessage> for StoredChatMessage {
    fn from(message: &ChatMessage) -> Self {
        Self {
            id: message.id.0,
            role: StoredChatRole::from(&message.role),
            blocks: message.blocks.iter().map(StoredChatBlock::from).collect(),
            status: StoredChatTurnStatus::from(&message.status),
            meta: StoredChatMessageMeta::from(&message.meta),
        }
    }
}

impl TryFrom<StoredChatMessage> for ChatMessage {
    type Error = anyhow::Error;

    fn try_from(value: StoredChatMessage) -> Result<Self> {
        let blocks = value
            .blocks
            .into_iter()
            .map(ChatBlock::try_from)
            .collect::<Result<Vec<_>>>()?;
        let mut message = ChatMessage::new(ChatMessageId::new(value.id), value.role.into(), blocks);
        message.meta = value.meta.into();
        let status = match ChatTurnStatus::from(value.status) {
            ChatTurnStatus::Streaming => ChatTurnStatus::Canceled,
            status => status,
        };
        message.set_turn_status(status);
        Ok(message)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "name", rename_all = "snake_case")]
enum StoredChatRole {
    User,
    Assistant,
    System,
    Custom(String),
}

impl From<&ChatRole> for StoredChatRole {
    fn from(role: &ChatRole) -> Self {
        match role {
            ChatRole::User => Self::User,
            ChatRole::Assistant => Self::Assistant,
            ChatRole::System => Self::System,
            ChatRole::Custom(name) => Self::Custom(name.clone()),
        }
    }
}

impl From<StoredChatRole> for ChatRole {
    fn from(role: StoredChatRole) -> Self {
        match role {
            StoredChatRole::User => Self::User,
            StoredChatRole::Assistant => Self::Assistant,
            StoredChatRole::System => Self::System,
            StoredChatRole::Custom(name) => Self::Custom(name),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "state", content = "error", rename_all = "snake_case")]
enum StoredChatTurnStatus {
    Streaming,
    Complete,
    Failed(StoredChatError),
    Canceled,
}

impl From<&ChatTurnStatus> for StoredChatTurnStatus {
    fn from(status: &ChatTurnStatus) -> Self {
        match status {
            ChatTurnStatus::Streaming => Self::Streaming,
            ChatTurnStatus::Complete => Self::Complete,
            ChatTurnStatus::Failed(error) => Self::Failed(StoredChatError::from(error)),
            ChatTurnStatus::Canceled => Self::Canceled,
        }
    }
}

impl From<StoredChatTurnStatus> for ChatTurnStatus {
    fn from(status: StoredChatTurnStatus) -> Self {
        match status {
            StoredChatTurnStatus::Streaming => Self::Streaming,
            StoredChatTurnStatus::Complete => Self::Complete,
            StoredChatTurnStatus::Failed(error) => Self::Failed(error.into()),
            StoredChatTurnStatus::Canceled => Self::Canceled,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredChatError {
    kind: StoredChatErrorKind,
    message: String,
    detail: Option<String>,
}

impl From<&ChatError> for StoredChatError {
    fn from(error: &ChatError) -> Self {
        Self {
            kind: StoredChatErrorKind::from(error.kind.clone()),
            message: error.message.clone(),
            detail: error.detail.clone(),
        }
    }
}

impl From<StoredChatError> for ChatError {
    fn from(error: StoredChatError) -> Self {
        Self {
            kind: error.kind.into(),
            message: error.message,
            detail: error.detail,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StoredChatErrorKind {
    Api,
    Tool,
    RateLimit,
    Refusal,
    Network,
    Other,
}

impl From<ChatErrorKind> for StoredChatErrorKind {
    fn from(kind: ChatErrorKind) -> Self {
        match kind {
            ChatErrorKind::Api => Self::Api,
            ChatErrorKind::Tool => Self::Tool,
            ChatErrorKind::RateLimit => Self::RateLimit,
            ChatErrorKind::Refusal => Self::Refusal,
            ChatErrorKind::Network => Self::Network,
            ChatErrorKind::Other => Self::Other,
        }
    }
}

impl From<StoredChatErrorKind> for ChatErrorKind {
    fn from(kind: StoredChatErrorKind) -> Self {
        match kind {
            StoredChatErrorKind::Api => Self::Api,
            StoredChatErrorKind::Tool => Self::Tool,
            StoredChatErrorKind::RateLimit => Self::RateLimit,
            StoredChatErrorKind::Refusal => Self::Refusal,
            StoredChatErrorKind::Network => Self::Network,
            StoredChatErrorKind::Other => Self::Other,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct StoredChatMessageMeta {
    timestamp: Option<String>,
    model: Option<String>,
    usage: Option<StoredTokenUsage>,
    elapsed_ms: Option<u64>,
    stop_reason: Option<StoredStopReason>,
}

impl From<&ChatMessageMeta> for StoredChatMessageMeta {
    fn from(meta: &ChatMessageMeta) -> Self {
        Self {
            timestamp: meta.timestamp.clone(),
            model: meta.model.clone(),
            usage: meta.usage.as_ref().map(StoredTokenUsage::from),
            elapsed_ms: meta.elapsed_ms,
            stop_reason: meta
                .stop_reason
                .as_ref()
                .map(|reason| StoredStopReason::from(reason.clone())),
        }
    }
}

impl From<StoredChatMessageMeta> for ChatMessageMeta {
    fn from(meta: StoredChatMessageMeta) -> Self {
        Self {
            timestamp: meta.timestamp,
            model: meta.model,
            usage: meta.usage.map(TokenUsage::from),
            elapsed_ms: meta.elapsed_ms,
            stop_reason: meta.stop_reason.map(StopReason::from),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct StoredTokenUsage {
    input: u64,
    output: u64,
}

impl From<&TokenUsage> for StoredTokenUsage {
    fn from(usage: &TokenUsage) -> Self {
        Self {
            input: usage.input,
            output: usage.output,
        }
    }
}

impl From<StoredTokenUsage> for TokenUsage {
    fn from(usage: StoredTokenUsage) -> Self {
        Self {
            input: usage.input,
            output: usage.output,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StoredStopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    StopSequence,
    Refusal,
}

impl From<StopReason> for StoredStopReason {
    fn from(reason: StopReason) -> Self {
        match reason {
            StopReason::EndTurn => Self::EndTurn,
            StopReason::MaxTokens => Self::MaxTokens,
            StopReason::ToolUse => Self::ToolUse,
            StopReason::StopSequence => Self::StopSequence,
            StopReason::Refusal => Self::Refusal,
        }
    }
}

impl From<StoredStopReason> for StopReason {
    fn from(reason: StoredStopReason) -> Self {
        match reason {
            StoredStopReason::EndTurn => Self::EndTurn,
            StoredStopReason::MaxTokens => Self::MaxTokens,
            StoredStopReason::ToolUse => Self::ToolUse,
            StoredStopReason::StopSequence => Self::StopSequence,
            StoredStopReason::Refusal => Self::Refusal,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StoredChatBlock {
    Text {
        id: u64,
        markdown: String,
        streaming: bool,
    },
    Thinking {
        id: u64,
        markdown: String,
        streaming: bool,
        collapsed: bool,
    },
    ToolUse {
        id: u64,
        call_id: String,
        name: String,
        input: StoredToolInput,
        status: StoredToolStatus,
        approval: Option<StoredApprovalRequest>,
        collapsed: bool,
    },
    ToolResult {
        id: u64,
        call_id: String,
        ok: bool,
        exit_code: Option<i32>,
        output: StoredToolOutput,
        collapsed: bool,
    },
    Diff {
        id: u64,
        path: String,
        diff: StoredDiffData,
        decision: StoredEditDecision,
    },
    Plan {
        id: u64,
        items: Vec<StoredPlanItem>,
        decision: StoredPlanDecision,
    },
    Task {
        id: u64,
        title: String,
        status: StoredTaskStatus,
        summary: String,
        transcript: Vec<StoredTaskTranscriptItem>,
        collapsed: bool,
    },
    Todo {
        id: u64,
        items: Vec<StoredTodoItem>,
    },
    Attachment {
        id: u64,
        name: String,
        url: Option<String>,
        mime: Option<String>,
    },
    Notice {
        id: u64,
        level: StoredNoticeLevel,
        text: String,
    },
    Compact {
        id: u64,
        status: StoredCompactStatus,
        before_tokens: Option<u64>,
        after_tokens: Option<u64>,
        summary: String,
    },
    Artifact {
        id: u64,
        kind: StoredArtifactKind,
        anchor: String,
        title: String,
    },
}

impl From<&ChatBlock> for StoredChatBlock {
    fn from(block: &ChatBlock) -> Self {
        match block {
            ChatBlock::Text(block) => Self::Text {
                id: block.id.0,
                markdown: block.markdown.clone(),
                streaming: block.streaming,
            },
            ChatBlock::Thinking(block) => Self::Thinking {
                id: block.id.0,
                markdown: block.markdown.clone(),
                streaming: block.streaming,
                collapsed: block.collapsed,
            },
            ChatBlock::ToolUse(block) => Self::ToolUse {
                id: block.id.0,
                call_id: block.call_id.clone(),
                name: block.name.clone(),
                input: StoredToolInput::from(&block.input),
                status: StoredToolStatus::from(block.status),
                approval: block.approval.as_ref().map(StoredApprovalRequest::from),
                collapsed: block.collapsed,
            },
            ChatBlock::ToolResult(block) => Self::ToolResult {
                id: block.id.0,
                call_id: block.call_id.clone(),
                ok: block.ok,
                exit_code: block.exit_code,
                output: StoredToolOutput::from(&block.output),
                collapsed: block.collapsed,
            },
            ChatBlock::Diff(block) => Self::Diff {
                id: block.id.0,
                path: block.path.clone(),
                diff: StoredDiffData::from(&block.diff),
                decision: StoredEditDecision::from(block.decision),
            },
            ChatBlock::Plan(block) => Self::Plan {
                id: block.id.0,
                items: block.items.iter().map(StoredPlanItem::from).collect(),
                decision: StoredPlanDecision::from(block.decision),
            },
            ChatBlock::Task(block) => Self::Task {
                id: block.id.0,
                title: block.title.clone(),
                status: StoredTaskStatus::from(block.status),
                summary: block.summary.clone(),
                transcript: block
                    .transcript
                    .iter()
                    .map(StoredTaskTranscriptItem::from)
                    .collect(),
                collapsed: block.collapsed,
            },
            ChatBlock::Todo(block) => Self::Todo {
                id: block.id.0,
                items: block.items.iter().map(StoredTodoItem::from).collect(),
            },
            ChatBlock::Attachment(block) => Self::Attachment {
                id: block.id.0,
                name: block.name.clone(),
                url: block.url.clone(),
                mime: block.mime.clone(),
            },
            ChatBlock::Notice(block) => Self::Notice {
                id: block.id.0,
                level: StoredNoticeLevel::from(block.level),
                text: block.text.clone(),
            },
            ChatBlock::Compact(block) => Self::Compact {
                id: block.id.0,
                status: StoredCompactStatus::from(block.status),
                before_tokens: block.before_tokens,
                after_tokens: block.after_tokens,
                summary: block.summary.clone(),
            },
            ChatBlock::Artifact(block) => Self::Artifact {
                id: block.id.0,
                kind: StoredArtifactKind::from(&block.kind),
                anchor: block.anchor.as_str().to_string(),
                title: block.title.clone(),
            },
        }
    }
}

impl TryFrom<StoredChatBlock> for ChatBlock {
    type Error = anyhow::Error;

    fn try_from(block: StoredChatBlock) -> Result<Self> {
        Ok(match block {
            StoredChatBlock::Text {
                id,
                markdown,
                streaming,
            } => Self::Text(TextBlock {
                id: ChatBlockId::new(id),
                markdown,
                streaming,
            }),
            StoredChatBlock::Thinking {
                id,
                markdown,
                streaming,
                collapsed,
            } => Self::Thinking(ThinkingBlock {
                id: ChatBlockId::new(id),
                markdown,
                streaming,
                collapsed,
            }),
            StoredChatBlock::ToolUse {
                id,
                call_id,
                name,
                input,
                status,
                approval,
                collapsed,
            } => Self::ToolUse(ToolUseBlock {
                id: ChatBlockId::new(id),
                call_id,
                name,
                input: input.into(),
                status: status.into(),
                approval: approval.map(ApprovalRequest::from),
                collapsed,
            }),
            StoredChatBlock::ToolResult {
                id,
                call_id,
                ok,
                exit_code,
                output,
                collapsed,
            } => Self::ToolResult(ToolResultBlock {
                id: ChatBlockId::new(id),
                call_id,
                ok,
                exit_code,
                output: output.into(),
                collapsed,
            }),
            StoredChatBlock::Diff {
                id,
                path,
                diff,
                decision,
            } => Self::Diff(DiffBlock {
                id: ChatBlockId::new(id),
                path,
                diff: diff.into(),
                decision: decision.into(),
            }),
            StoredChatBlock::Plan {
                id,
                items,
                decision,
            } => Self::Plan(PlanBlock {
                id: ChatBlockId::new(id),
                items: items.into_iter().map(PlanItem::from).collect(),
                decision: decision.into(),
            }),
            StoredChatBlock::Task {
                id,
                title,
                status,
                summary,
                transcript,
                collapsed,
            } => Self::Task(TaskBlock {
                id: ChatBlockId::new(id),
                title,
                status: status.into(),
                summary,
                transcript: transcript
                    .into_iter()
                    .map(TaskTranscriptItem::try_from)
                    .collect::<Result<Vec<_>>>()?,
                collapsed,
            }),
            StoredChatBlock::Todo { id, items } => Self::Todo(TodoBlock {
                id: ChatBlockId::new(id),
                items: items.into_iter().map(TodoItem::from).collect(),
            }),
            StoredChatBlock::Attachment {
                id,
                name,
                url,
                mime,
            } => Self::Attachment(AttachmentBlock {
                id: ChatBlockId::new(id),
                name,
                url,
                mime,
            }),
            StoredChatBlock::Notice { id, level, text } => Self::Notice(NoticeBlock {
                id: ChatBlockId::new(id),
                level: level.into(),
                text,
            }),
            StoredChatBlock::Compact {
                id,
                status,
                before_tokens,
                after_tokens,
                summary,
            } => Self::Compact(CompactBlock {
                id: ChatBlockId::new(id),
                status: status.into(),
                before_tokens,
                after_tokens,
                summary,
            }),
            StoredChatBlock::Artifact {
                id,
                kind,
                anchor,
                title,
            } => Self::Artifact(ArtifactBlock {
                id: ChatBlockId::new(id),
                kind: kind.into(),
                anchor: ArtifactId::new(anchor),
                title,
            }),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum StoredToolInput {
    Text(String),
    Json(ComponentValue),
}

impl From<&ToolInput> for StoredToolInput {
    fn from(input: &ToolInput) -> Self {
        match input {
            ToolInput::Text(text) => Self::Text(text.clone()),
            ToolInput::Json(value) => Self::Json(value.clone()),
        }
    }
}

impl From<StoredToolInput> for ToolInput {
    fn from(input: StoredToolInput) -> Self {
        match input {
            StoredToolInput::Text(text) => Self::Text(text),
            StoredToolInput::Json(value) => Self::Json(value),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StoredToolStatus {
    Pending,
    Running,
    Done,
    Error,
    Canceled,
}

impl From<ToolStatus> for StoredToolStatus {
    fn from(status: ToolStatus) -> Self {
        match status {
            ToolStatus::Pending => Self::Pending,
            ToolStatus::Running => Self::Running,
            ToolStatus::Done => Self::Done,
            ToolStatus::Error => Self::Error,
            ToolStatus::Canceled => Self::Canceled,
        }
    }
}

impl From<StoredToolStatus> for ToolStatus {
    fn from(status: StoredToolStatus) -> Self {
        match status {
            StoredToolStatus::Pending => Self::Pending,
            StoredToolStatus::Running => Self::Running,
            StoredToolStatus::Done => Self::Done,
            StoredToolStatus::Error => Self::Error,
            StoredToolStatus::Canceled => Self::Canceled,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredApprovalRequest {
    id: String,
    prompt: String,
    options: Vec<StoredApprovalOption>,
    resolved: Option<StoredApprovalResolution>,
}

impl From<&ApprovalRequest> for StoredApprovalRequest {
    fn from(approval: &ApprovalRequest) -> Self {
        Self {
            id: approval.id.clone(),
            prompt: approval.prompt.clone(),
            options: approval
                .options
                .iter()
                .map(StoredApprovalOption::from)
                .collect(),
            resolved: approval
                .resolved
                .as_ref()
                .map(StoredApprovalResolution::from),
        }
    }
}

impl From<StoredApprovalRequest> for ApprovalRequest {
    fn from(approval: StoredApprovalRequest) -> Self {
        Self {
            id: approval.id,
            prompt: approval.prompt,
            options: approval
                .options
                .into_iter()
                .map(ApprovalOption::from)
                .collect(),
            resolved: approval.resolved.map(ApprovalResolution::from),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredApprovalOption {
    id: String,
    label: String,
    action: StoredApprovalAction,
    level: StoredApprovalLevel,
}

impl From<&ApprovalOption> for StoredApprovalOption {
    fn from(option: &ApprovalOption) -> Self {
        Self {
            id: option.id.clone(),
            label: option.label.clone(),
            action: StoredApprovalAction::from(option.action),
            level: StoredApprovalLevel::from(option.level),
        }
    }
}

impl From<StoredApprovalOption> for ApprovalOption {
    fn from(option: StoredApprovalOption) -> Self {
        Self::new(
            option.id,
            option.label,
            option.action.into(),
            option.level.into(),
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredApprovalResolution {
    option_id: String,
    action: StoredApprovalAction,
    level: StoredApprovalLevel,
}

impl From<&ApprovalResolution> for StoredApprovalResolution {
    fn from(resolution: &ApprovalResolution) -> Self {
        Self {
            option_id: resolution.option_id.clone(),
            action: StoredApprovalAction::from(resolution.action),
            level: StoredApprovalLevel::from(resolution.level),
        }
    }
}

impl From<StoredApprovalResolution> for ApprovalResolution {
    fn from(resolution: StoredApprovalResolution) -> Self {
        Self {
            option_id: resolution.option_id,
            action: resolution.action.into(),
            level: resolution.level.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StoredApprovalAction {
    Allow,
    Deny,
}

impl From<ApprovalAction> for StoredApprovalAction {
    fn from(action: ApprovalAction) -> Self {
        match action {
            ApprovalAction::Allow => Self::Allow,
            ApprovalAction::Deny => Self::Deny,
        }
    }
}

impl From<StoredApprovalAction> for ApprovalAction {
    fn from(action: StoredApprovalAction) -> Self {
        match action {
            StoredApprovalAction::Allow => Self::Allow,
            StoredApprovalAction::Deny => Self::Deny,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StoredApprovalLevel {
    Once,
    Always,
    Project,
}

impl From<ApprovalLevel> for StoredApprovalLevel {
    fn from(level: ApprovalLevel) -> Self {
        match level {
            ApprovalLevel::Once => Self::Once,
            ApprovalLevel::Always => Self::Always,
            ApprovalLevel::Project => Self::Project,
        }
    }
}

impl From<StoredApprovalLevel> for ApprovalLevel {
    fn from(level: StoredApprovalLevel) -> Self {
        match level {
            StoredApprovalLevel::Once => Self::Once,
            StoredApprovalLevel::Always => Self::Always,
            StoredApprovalLevel::Project => Self::Project,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum StoredToolOutput {
    Ansi(String),
    Markdown(String),
    Diff(StoredDiffData),
}

impl From<&ToolOutput> for StoredToolOutput {
    fn from(output: &ToolOutput) -> Self {
        match output {
            ToolOutput::Ansi(text) => Self::Ansi(text.clone()),
            ToolOutput::Markdown(text) => Self::Markdown(text.clone()),
            ToolOutput::Diff(diff) => Self::Diff(StoredDiffData::from(diff)),
        }
    }
}

impl From<StoredToolOutput> for ToolOutput {
    fn from(output: StoredToolOutput) -> Self {
        match output {
            StoredToolOutput::Ansi(text) => Self::Ansi(text),
            StoredToolOutput::Markdown(text) => Self::Markdown(text),
            StoredToolOutput::Diff(diff) => Self::Diff(diff.into()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredDiffData {
    unified: String,
}

impl From<&DiffData> for StoredDiffData {
    fn from(diff: &DiffData) -> Self {
        Self {
            unified: diff.unified.clone(),
        }
    }
}

impl From<StoredDiffData> for DiffData {
    fn from(diff: StoredDiffData) -> Self {
        Self {
            unified: diff.unified,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StoredEditDecision {
    Pending,
    Accepted,
    Rejected,
}

impl From<EditDecision> for StoredEditDecision {
    fn from(decision: EditDecision) -> Self {
        match decision {
            EditDecision::Pending => Self::Pending,
            EditDecision::Accepted => Self::Accepted,
            EditDecision::Rejected => Self::Rejected,
        }
    }
}

impl From<StoredEditDecision> for EditDecision {
    fn from(decision: StoredEditDecision) -> Self {
        match decision {
            StoredEditDecision::Pending => Self::Pending,
            StoredEditDecision::Accepted => Self::Accepted,
            StoredEditDecision::Rejected => Self::Rejected,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredPlanItem {
    text: String,
}

impl From<&PlanItem> for StoredPlanItem {
    fn from(item: &PlanItem) -> Self {
        Self {
            text: item.text.clone(),
        }
    }
}

impl From<StoredPlanItem> for PlanItem {
    fn from(item: StoredPlanItem) -> Self {
        Self { text: item.text }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StoredPlanDecision {
    Pending,
    Accepted,
    Rejected,
}

impl From<PlanDecision> for StoredPlanDecision {
    fn from(decision: PlanDecision) -> Self {
        match decision {
            PlanDecision::Pending => Self::Pending,
            PlanDecision::Accepted => Self::Accepted,
            PlanDecision::Rejected => Self::Rejected,
        }
    }
}

impl From<StoredPlanDecision> for PlanDecision {
    fn from(decision: StoredPlanDecision) -> Self {
        match decision {
            StoredPlanDecision::Pending => Self::Pending,
            StoredPlanDecision::Accepted => Self::Accepted,
            StoredPlanDecision::Rejected => Self::Rejected,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StoredTaskStatus {
    Pending,
    Running,
    Complete,
    Failed,
    Canceled,
}

impl From<TaskStatus> for StoredTaskStatus {
    fn from(status: TaskStatus) -> Self {
        match status {
            TaskStatus::Pending => Self::Pending,
            TaskStatus::Running => Self::Running,
            TaskStatus::Complete => Self::Complete,
            TaskStatus::Failed => Self::Failed,
            TaskStatus::Canceled => Self::Canceled,
        }
    }
}

impl From<StoredTaskStatus> for TaskStatus {
    fn from(status: StoredTaskStatus) -> Self {
        match status {
            StoredTaskStatus::Pending => Self::Pending,
            StoredTaskStatus::Running => Self::Running,
            StoredTaskStatus::Complete => Self::Complete,
            StoredTaskStatus::Failed => Self::Failed,
            StoredTaskStatus::Canceled => Self::Canceled,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredTaskTranscriptItem {
    role: StoredChatRole,
    blocks: Vec<StoredChatBlock>,
}

impl From<&TaskTranscriptItem> for StoredTaskTranscriptItem {
    fn from(item: &TaskTranscriptItem) -> Self {
        Self {
            role: StoredChatRole::from(&item.role),
            blocks: item.blocks.iter().map(StoredChatBlock::from).collect(),
        }
    }
}

impl TryFrom<StoredTaskTranscriptItem> for TaskTranscriptItem {
    type Error = anyhow::Error;

    fn try_from(item: StoredTaskTranscriptItem) -> Result<Self> {
        Ok(Self {
            role: item.role.into(),
            blocks: item
                .blocks
                .into_iter()
                .map(ChatBlock::try_from)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredTodoItem {
    text: String,
    state: StoredTodoState,
}

impl From<&TodoItem> for StoredTodoItem {
    fn from(item: &TodoItem) -> Self {
        Self {
            text: item.text.clone(),
            state: StoredTodoState::from(item.state),
        }
    }
}

impl From<StoredTodoItem> for TodoItem {
    fn from(item: StoredTodoItem) -> Self {
        Self {
            text: item.text,
            state: item.state.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StoredTodoState {
    Pending,
    InProgress,
    Done,
}

impl From<TodoState> for StoredTodoState {
    fn from(state: TodoState) -> Self {
        match state {
            TodoState::Pending => Self::Pending,
            TodoState::InProgress => Self::InProgress,
            TodoState::Done => Self::Done,
        }
    }
}

impl From<StoredTodoState> for TodoState {
    fn from(state: StoredTodoState) -> Self {
        match state {
            StoredTodoState::Pending => Self::Pending,
            StoredTodoState::InProgress => Self::InProgress,
            StoredTodoState::Done => Self::Done,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StoredNoticeLevel {
    Info,
    Warning,
    Error,
}

impl From<NoticeLevel> for StoredNoticeLevel {
    fn from(level: NoticeLevel) -> Self {
        match level {
            NoticeLevel::Info => Self::Info,
            NoticeLevel::Warning => Self::Warning,
            NoticeLevel::Error => Self::Error,
        }
    }
}

impl From<StoredNoticeLevel> for NoticeLevel {
    fn from(level: StoredNoticeLevel) -> Self {
        match level {
            StoredNoticeLevel::Info => Self::Info,
            StoredNoticeLevel::Warning => Self::Warning,
            StoredNoticeLevel::Error => Self::Error,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StoredCompactStatus {
    Pending,
    Running,
    Complete,
    Failed,
    Canceled,
}

impl From<CompactStatus> for StoredCompactStatus {
    fn from(status: CompactStatus) -> Self {
        match status {
            CompactStatus::Pending => Self::Pending,
            CompactStatus::Running => Self::Running,
            CompactStatus::Complete => Self::Complete,
            CompactStatus::Failed => Self::Failed,
            CompactStatus::Canceled => Self::Canceled,
        }
    }
}

impl From<StoredCompactStatus> for CompactStatus {
    fn from(status: StoredCompactStatus) -> Self {
        match status {
            StoredCompactStatus::Pending => Self::Pending,
            StoredCompactStatus::Running => Self::Running,
            StoredCompactStatus::Complete => Self::Complete,
            StoredCompactStatus::Failed => Self::Failed,
            StoredCompactStatus::Canceled => Self::Canceled,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StoredArtifactKind {
    Code,
    Diff,
    File,
}

impl From<&ArtifactKind> for StoredArtifactKind {
    fn from(kind: &ArtifactKind) -> Self {
        match kind {
            ArtifactKind::Code => Self::Code,
            ArtifactKind::Diff => Self::Diff,
            ArtifactKind::File => Self::File,
        }
    }
}

impl From<StoredArtifactKind> for ArtifactKind {
    fn from(kind: StoredArtifactKind) -> Self {
        match kind {
            StoredArtifactKind::Code => Self::Code,
            StoredArtifactKind::Diff => Self::Diff,
            StoredArtifactKind::File => Self::File,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "atto-agent-transcript-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("test dir should be created");
        path
    }

    fn fixture_messages() -> Vec<ChatMessage> {
        let mut assistant = ChatMessage::new(
            ChatMessageId::new(2),
            ChatRole::Assistant,
            vec![
                ChatBlock::Thinking(ThinkingBlock {
                    id: ChatBlockId::new(20),
                    markdown: "reasoning".to_string(),
                    streaming: false,
                    collapsed: true,
                }),
                ChatBlock::Text(TextBlock {
                    id: ChatBlockId::new(21),
                    markdown: "answer".to_string(),
                    streaming: false,
                }),
                ChatBlock::ToolUse(ToolUseBlock {
                    id: ChatBlockId::new(22),
                    call_id: "call-1".to_string(),
                    name: "read_file".to_string(),
                    input: ToolInput::Json(ComponentValue::Map(BTreeMap::from([(
                        "path".to_string(),
                        ComponentValue::String("README.md".to_string()),
                    )]))),
                    status: ToolStatus::Done,
                    approval: Some(ApprovalRequest {
                        id: "approval:call-1".to_string(),
                        prompt: "Allow tool?".to_string(),
                        options: vec![
                            ApprovalOption::allow_once("allow_once", "Allow once"),
                            ApprovalOption::deny("deny", "Deny"),
                        ],
                        resolved: Some(ApprovalResolution {
                            option_id: "allow_once".to_string(),
                            action: ApprovalAction::Allow,
                            level: ApprovalLevel::Once,
                        }),
                    }),
                    collapsed: false,
                }),
                ChatBlock::ToolResult(ToolResultBlock {
                    id: ChatBlockId::new(23),
                    call_id: "call-1".to_string(),
                    ok: true,
                    exit_code: Some(0),
                    output: ToolOutput::Markdown("file contents".to_string()),
                    collapsed: false,
                }),
                ChatBlock::Plan(PlanBlock {
                    id: ChatBlockId::new(24),
                    items: vec![PlanItem {
                        text: "Inspect file".to_string(),
                    }],
                    decision: PlanDecision::Accepted,
                }),
                ChatBlock::Diff(DiffBlock {
                    id: ChatBlockId::new(25),
                    path: "src/lib.rs".to_string(),
                    diff: DiffData {
                        unified: "@@\n+line".to_string(),
                    },
                    decision: EditDecision::Pending,
                }),
                ChatBlock::Task(TaskBlock {
                    id: ChatBlockId::new(26),
                    title: "subtask".to_string(),
                    status: TaskStatus::Complete,
                    summary: "done".to_string(),
                    transcript: vec![TaskTranscriptItem {
                        role: ChatRole::Assistant,
                        blocks: vec![ChatBlock::Text(TextBlock {
                            id: ChatBlockId::new(260),
                            markdown: "nested".to_string(),
                            streaming: false,
                        })],
                    }],
                    collapsed: true,
                }),
                ChatBlock::Todo(TodoBlock {
                    id: ChatBlockId::new(27),
                    items: vec![TodoItem {
                        text: "ship".to_string(),
                        state: TodoState::InProgress,
                    }],
                }),
                ChatBlock::Attachment(AttachmentBlock {
                    id: ChatBlockId::new(28),
                    name: "report.txt".to_string(),
                    url: Some("file://report.txt".to_string()),
                    mime: Some("text/plain".to_string()),
                }),
                ChatBlock::Notice(NoticeBlock {
                    id: ChatBlockId::new(29),
                    level: NoticeLevel::Warning,
                    text: "note".to_string(),
                }),
                ChatBlock::Compact(CompactBlock {
                    id: ChatBlockId::new(30),
                    status: CompactStatus::Complete,
                    before_tokens: Some(1000),
                    after_tokens: Some(200),
                    summary: "summary".to_string(),
                }),
                ChatBlock::Artifact(ArtifactBlock {
                    id: ChatBlockId::new(31),
                    kind: ArtifactKind::Diff,
                    anchor: ArtifactId::new("artifact-1"),
                    title: "patch".to_string(),
                }),
            ],
        );
        assistant.meta = ChatMessageMeta {
            timestamp: Some("2026-07-10T00:00:00Z".to_string()),
            model: Some("deepseek-chat".to_string()),
            usage: Some(TokenUsage {
                input: 12,
                output: 34,
            }),
            elapsed_ms: Some(56),
            stop_reason: Some(StopReason::EndTurn),
        };
        vec![
            ChatMessage::text(1, ChatRole::User, "hello"),
            assistant,
            ChatMessage::new(
                ChatMessageId::new(3),
                ChatRole::Custom("Tool".to_string()),
                vec![ChatBlock::ToolResult(ToolResultBlock {
                    id: ChatBlockId::new(32),
                    call_id: "call-2".to_string(),
                    ok: false,
                    exit_code: None,
                    output: ToolOutput::Diff(DiffData {
                        unified: "--- a\n+++ b".to_string(),
                    }),
                    collapsed: true,
                })],
            )
            .with_status(ChatTurnStatus::Failed(ChatError::new(
                ChatErrorKind::Tool,
                "tool failed",
            ))),
        ]
    }

    #[test]
    fn transcript_jsonl_round_trips_chat_messages() {
        let dir = test_dir("round-trip");
        let path = dir.join("session.jsonl");
        let messages = fixture_messages();

        save_transcript_jsonl(&path, &messages).expect("transcript should be saved");
        let loaded = load_transcript_jsonl(&path).expect("transcript should be loaded");

        assert_eq!(loaded, messages);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn loading_missing_transcript_returns_empty_messages() {
        let dir = test_dir("missing");
        let path = dir.join("missing.jsonl");

        let loaded = load_transcript_jsonl(&path).expect("missing transcript should be optional");

        assert!(loaded.is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn restored_streaming_turns_are_marked_canceled() {
        let dir = test_dir("streaming");
        let path = dir.join("session.jsonl");
        let assistant = ChatMessage::text(7, ChatRole::Assistant, "partial")
            .with_status(ChatTurnStatus::Streaming);

        save_transcript_jsonl(&path, &[assistant]).expect("streaming transcript should be saved");
        let loaded = load_transcript_jsonl(&path).expect("streaming transcript should be loaded");

        assert_eq!(loaded[0].status, ChatTurnStatus::Canceled);
        assert!(matches!(&loaded[0].blocks[0], ChatBlock::Text(block) if !block.streaming));
        let _ = fs::remove_dir_all(dir);
    }
}
