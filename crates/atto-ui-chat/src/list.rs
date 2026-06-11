use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use atto_ui::composable::{
    Component, ComponentAction, ComponentContext, EdgeInsets, EventResult, HStack, Identifiable,
    LayoutParams, MouseCoordinateSpace, ScrollConfig, Scrollable, ScrollbarVisibility, Size,
    Spacer, Text, VStack,
};
use atto_ui::reactive::{Binding, DirtyObserver};
use atto_ui::widgets::{Button, Disclosure, DisclosureStatus};
use atto_ui::{ComponentError, ComponentValue, ComponentValueCodec};
use atto_ui_markdown::MarkdownViewer;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::dynamic::{messages_to_component_value, parse_messages_value};
use crate::message::{
    ApprovalOption, ApprovalRequest, ArtifactBlock, ArtifactId, ArtifactKind, AttachmentBlock,
    ChatAlignment, ChatBlock, ChatBlockId, ChatErrorKind, ChatMessage, ChatMessageId,
    ChatMessageMeta, ChatRole, ChatTurnStatus, DiffBlock, DiffData, EditDecision, NoticeBlock,
    NoticeLevel, StopReason, TextBlock, ThinkingBlock, TodoBlock, TodoItem, TodoState, ToolInput,
    ToolOutput, ToolResultBlock, ToolStatus, ToolUseBlock,
};
use crate::store::ChatMessageStore;
use crate::viewer::diff_line_style;

const DEFAULT_IN_PROGRESS_SUFFIX: &str = " ▍";
const ANSI_OUTPUT_TAIL_LINES: usize = 12;
const ANSI_OUTPUT_EXPAND_LABEL: &str = "展开全部";

type ArtifactOpenCallback = Arc<dyn Fn(ArtifactId) + Send + Sync>;
type ApprovalCallback = Arc<dyn Fn(ApprovalDecision) + Send + Sync>;
type EditDecisionCallback = Arc<dyn Fn(EditDecisionEvent) + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalDecision {
    pub message_id: ChatMessageId,
    pub block_id: ChatBlockId,
    pub approval_id: String,
    pub option_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditDecisionEvent {
    pub message_id: ChatMessageId,
    pub block_id: ChatBlockId,
    pub decision: EditDecision,
}

#[derive(Clone)]
struct ChatMessageListConfig {
    wrap_width: Option<u16>,
    responsive_wrap_width: Binding<Option<u16>>,
    in_progress_suffix: String,
    show_timestamps: bool,
    spacing: Binding<u16>,
    padding: Binding<EdgeInsets>,
    scroll_config: Binding<ScrollConfig>,
    on_open_artifact: Option<ArtifactOpenCallback>,
    on_approve: Option<ApprovalCallback>,
    on_edit_decision: Option<EditDecisionCallback>,
}

#[derive(Clone)]
struct ChatMessageRowConfig {
    wrap_width: Option<u16>,
    responsive_wrap_width: Binding<Option<u16>>,
    in_progress_suffix: String,
    show_timestamps: bool,
    on_open_artifact: Option<ArtifactOpenCallback>,
    on_approve: Option<ApprovalCallback>,
    on_edit_decision: Option<EditDecisionCallback>,
}

pub struct ChatMessageList {
    store: ChatMessageStore,
    messages: Binding<Vec<ChatMessage>>,
    row_keys: Binding<Vec<ChatRowKey>>,
    list: atto_ui::composable::ForEachIdentifiable<ChatRowKey, ChatMessageRow>,
    config: ChatMessageListConfig,
    on_load_more: Option<Arc<dyn Fn() + Send + Sync>>,
    load_more_armed: bool,
    auto_scroll: bool,
    follow_tail: bool,
    suppress_auto_scroll_once: bool,
    pending_scroll_to_bottom: bool,
    messages_observer: DirtyObserver,
}

impl ChatMessageList {
    pub fn new(store: ChatMessageStore) -> Self {
        let config = ChatMessageListConfig {
            wrap_width: None,
            responsive_wrap_width: Binding::new(None),
            in_progress_suffix: DEFAULT_IN_PROGRESS_SUFFIX.to_string(),
            show_timestamps: false,
            spacing: 1u16.into(),
            padding: EdgeInsets::symmetric(0, 1).into(),
            scroll_config: ScrollConfig::default().into(),
            on_open_artifact: None,
            on_approve: None,
            on_edit_decision: None,
        };
        let messages = store.binding();
        let has_initial_messages = messages.with(|messages| !messages.is_empty());
        let row_keys = Binding::new(messages.with(|messages| row_keys_from_messages(messages)));
        let list = build_list(row_keys.clone(), store.clone(), &config);
        let messages_observer = messages.dirty_observer();
        Self {
            store,
            messages,
            row_keys,
            list,
            config,
            on_load_more: None,
            load_more_armed: true,
            auto_scroll: true,
            follow_tail: true,
            suppress_auto_scroll_once: false,
            pending_scroll_to_bottom: has_initial_messages,
            messages_observer,
        }
    }

    pub fn messages(&self) -> Binding<Vec<ChatMessage>> {
        self.messages.clone()
    }

    pub fn spacing(mut self, spacing: impl Into<Binding<u16>>) -> Self {
        self.config.spacing = spacing.into();
        self.rebuild_list();
        self
    }

    pub fn padding(mut self, padding: u16) -> Self {
        self.config.padding = EdgeInsets::all(padding).into();
        self.rebuild_list();
        self
    }

    pub fn padding_insets(mut self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.config.padding = padding.into();
        self.rebuild_list();
        self
    }

    pub fn scroll_config(mut self, config: impl Into<Binding<ScrollConfig>>) -> Self {
        self.config.scroll_config = config.into();
        self.rebuild_list();
        self
    }

    pub fn wrap_width(mut self, width: u16) -> Self {
        self.config.wrap_width = Some(width.max(1));
        self.rebuild_list();
        self
    }

    pub fn in_progress_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.config.in_progress_suffix = suffix.into();
        self.rebuild_list();
        self
    }

    pub fn show_timestamps(mut self, show: bool) -> Self {
        self.config.show_timestamps = show;
        self.rebuild_list();
        self
    }

    pub fn on_load_more<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_load_more = Some(Arc::new(callback));
        self
    }

    pub fn on_open_artifact<F>(mut self, callback: F) -> Self
    where
        F: Fn(ArtifactId) + Send + Sync + 'static,
    {
        self.config.on_open_artifact = Some(Arc::new(callback));
        self.rebuild_list();
        self
    }

    pub fn on_approve<F>(mut self, callback: F) -> Self
    where
        F: Fn(ApprovalDecision) + Send + Sync + 'static,
    {
        self.config.on_approve = Some(Arc::new(callback));
        self.rebuild_list();
        self
    }

    pub fn on_edit_decision<F>(mut self, callback: F) -> Self
    where
        F: Fn(EditDecisionEvent) + Send + Sync + 'static,
    {
        self.config.on_edit_decision = Some(Arc::new(callback));
        self.rebuild_list();
        self
    }

    pub fn auto_scroll(mut self, enabled: bool) -> Self {
        self.auto_scroll = enabled;
        if !enabled {
            self.pending_scroll_to_bottom = false;
        }
        self
    }

    pub fn scroll_to_bottom(&mut self) {
        self.pending_scroll_to_bottom = true;
        self.follow_tail = true;
    }

    fn rebuild_list(&mut self) {
        self.row_keys.set(
            self.messages
                .with(|messages| row_keys_from_messages(messages)),
        );
        self.list = build_list(self.row_keys.clone(), self.store.clone(), &self.config);
    }

    fn maybe_trigger_load_more(&mut self) -> bool {
        let Some(callback) = &self.on_load_more else {
            return false;
        };
        let scroll_y = self.list.scroll_offset().1;
        let viewport_h = self.list.viewport_size().1;
        let content_h = self.list.content_size().1;
        if content_h <= viewport_h {
            return false;
        }
        if scroll_y > 0 {
            self.load_more_armed = true;
            return false;
        }
        if !self.load_more_armed {
            return false;
        }
        let previous_content_h = content_h;
        let previous_scroll_y = scroll_y;
        self.load_more_armed = false;
        self.suppress_auto_scroll_once = true;
        callback();
        self.list
            .preserve_scroll_y_after_next_layout(previous_content_h, previous_scroll_y);
        true
    }

    fn track_message_changes(&mut self) {
        if !self.messages.check_dirty(&mut self.messages_observer) {
            return;
        }
        self.row_keys.set(
            self.messages
                .with(|messages| row_keys_from_messages(messages)),
        );
        if self.suppress_auto_scroll_once {
            self.suppress_auto_scroll_once = false;
            return;
        }
        if self.auto_scroll && self.follow_tail {
            self.pending_scroll_to_bottom = true;
        }
    }

    fn max_scroll_y(&self) -> u16 {
        let viewport_h = self.list.viewport_size().1;
        let content_h = self.list.content_size().1;
        content_h.saturating_sub(viewport_h)
    }

    fn is_scrolled_to_bottom(&self) -> bool {
        self.list.scroll_offset().1 >= self.max_scroll_y()
    }

    fn sync_follow_tail_from_scroll(&mut self) {
        self.follow_tail = self.is_scrolled_to_bottom();
    }

    fn queue_pending_scroll_to_bottom(&mut self) {
        if !self.pending_scroll_to_bottom {
            return;
        }
        self.list.scroll_to_bottom_on_next_layout();
        self.follow_tail = true;
        self.pending_scroll_to_bottom = false;
        self.load_more_armed = true;
    }

    fn update_responsive_wrap_width(&self, area_width: u16) {
        self.config
            .responsive_wrap_width
            .set(estimated_bubble_content_width(
                area_width,
                self.config.padding.get(),
            ));
    }
}

impl ::atto_ui::composable::Component for ChatMessageList {
    fn property_names(&self) -> Vec<&'static str> {
        vec![
            "messages",
            "spacing",
            "padding",
            "wrap_width",
            "show_timestamps",
            "auto_scroll",
        ]
    }

    fn get_property(&self, name: &str) -> Option<ComponentValue> {
        match name {
            "messages" => Some(
                self.messages
                    .with(|messages| messages_to_component_value(messages)),
            ),
            "spacing" => Some(ComponentValue::U64(self.config.spacing.get() as u64)),
            "padding" => Some(self.config.padding.get().to_component_value()),
            "wrap_width" => Some(ComponentValue::U64(
                self.config
                    .wrap_width
                    .or_else(|| self.config.responsive_wrap_width.get())
                    .unwrap_or(0) as u64,
            )),
            "show_timestamps" => Some(ComponentValue::Bool(self.config.show_timestamps)),
            "auto_scroll" => Some(ComponentValue::Bool(self.auto_scroll)),
            _ => None,
        }
    }

    fn set_property(&mut self, name: &str, value: ComponentValue) -> Result<(), ComponentError> {
        match name {
            "messages" => {
                let messages = parse_messages_value(&value)
                    .map_err(|_| ComponentError::invalid_value(name, "chat messages"))?;
                self.store.replace_all(messages);
                Ok(())
            }
            "spacing" => {
                let spacing = <u16 as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.spacing.set(spacing);
                self.rebuild_list();
                Ok(())
            }
            "padding" => {
                let padding =
                    <EdgeInsets as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.padding.set(padding);
                self.rebuild_list();
                Ok(())
            }
            "wrap_width" => {
                let width = <u16 as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.wrap_width = (width > 0).then_some(width);
                self.rebuild_list();
                Ok(())
            }
            "show_timestamps" => {
                let show = <bool as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.show_timestamps = show;
                self.rebuild_list();
                Ok(())
            }
            "auto_scroll" => {
                let enabled = <bool as ComponentValueCodec>::from_component_value(value, name)?;
                self.auto_scroll = enabled;
                if !enabled {
                    self.pending_scroll_to_bottom = false;
                }
                Ok(())
            }
            _ => Err(ComponentError::unsupported_property(name)),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.update_responsive_wrap_width(area.width);
        self.track_message_changes();
        self.queue_pending_scroll_to_bottom();
        self.list.draw(frame, area, ctx);
    }
}

fn estimated_bubble_content_width(area_width: u16, padding: EdgeInsets) -> Option<u16> {
    let list_width = area_width.saturating_sub(padding.sum_horizontal());
    if list_width == 0 {
        return None;
    }
    Some((((list_width as u32) * 3) / 4).max(1).min(u16::MAX as u32) as u16)
}

impl ::atto_ui::composable::DragAndDrop for ChatMessageList {}

impl ::atto_ui::composable::Layout for ChatMessageList {
    fn min_width(&self) -> u16 {
        self.list.min_width()
    }

    fn min_height(&self) -> u16 {
        self.list.min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.list.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.list.desired_height()
    }
}

impl ::atto_ui::composable::Scrollable for ChatMessageList {
    fn is_scrollable(&self) -> bool {
        self.list.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.list.content_size()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.list.viewport_size()
    }

    fn scroll_config(&self) -> ScrollConfig {
        Scrollable::scroll_config(&self.list)
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.list.scroll_offset()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.list.set_scroll_offset(x, y);
    }
}

impl ::atto_ui::composable::FocusNav for ChatMessageList {}

impl ::atto_ui::composable::DynamicTree for ChatMessageList {}

impl ::atto_ui::composable::EventHandling for ChatMessageList {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.track_message_changes();
        let before_scroll_y = self.list.scroll_offset().1;
        let mut res = self.list.handle_event(event, ctx);
        if self.list.scroll_offset().1 != before_scroll_y {
            self.sync_follow_tail_from_scroll();
        }
        if self.maybe_trigger_load_more() && matches!(res.action, ComponentAction::None) {
            self.sync_follow_tail_from_scroll();
            res = EventResult::changed();
        }
        res
    }
}

fn build_list(
    row_keys: Binding<Vec<ChatRowKey>>,
    store: ChatMessageStore,
    config: &ChatMessageListConfig,
) -> atto_ui::composable::ForEachIdentifiable<ChatRowKey, ChatMessageRow> {
    let row_config = ChatMessageRowConfig {
        wrap_width: config.wrap_width,
        responsive_wrap_width: config.responsive_wrap_width.clone(),
        in_progress_suffix: config.in_progress_suffix.clone(),
        show_timestamps: config.show_timestamps,
        on_open_artifact: config.on_open_artifact.clone(),
        on_approve: config.on_approve.clone(),
        on_edit_decision: config.on_edit_decision.clone(),
    };
    let list = atto_ui::composable::ForEach::new(row_keys, move |key, _| {
        ChatMessageRow::new(key.clone(), store.clone(), row_config.clone())
    })
    .spacing(config.spacing.clone())
    .padding_insets(config.padding.clone())
    .scrollable(true)
    .scroll_config(config.scroll_config.clone());
    list.with_id()
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ChatRowKey {
    Header {
        message_id: ChatMessageId,
    },
    Block {
        message_id: ChatMessageId,
        block_id: ChatBlockId,
        kind_tag: ChatBlockKindTag,
    },
    PendingToolResult {
        message_id: ChatMessageId,
        tool_use_id: ChatBlockId,
        call_id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ChatRowId {
    Header(ChatMessageId),
    Block {
        message_id: ChatMessageId,
        block_id: ChatBlockId,
    },
    PendingToolResult {
        message_id: ChatMessageId,
        tool_use_id: ChatBlockId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChatRowRef {
    Header(ChatMessageId),
    Block(ChatBlockId),
    PendingToolResult(ChatBlockId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ChatBlockKindTag {
    Text,
    Thinking {
        collapsed: bool,
    },
    Attachment {
        name: String,
        url: Option<String>,
        mime: Option<String>,
    },
    ToolUse {
        call_id: String,
        name: String,
    },
    ToolResult {
        call_id: String,
        ok: bool,
        exit_code: Option<i32>,
        collapsed: bool,
    },
    Diff {
        path: String,
        decision: EditDecision,
    },
    Todo,
    Notice {
        level: NoticeLevel,
        text: String,
    },
    Artifact {
        kind: ArtifactKind,
        anchor: ArtifactId,
        title: String,
    },
}

impl Identifiable for ChatRowKey {
    type Id = ChatRowId;

    fn id(&self) -> Self::Id {
        match self {
            ChatRowKey::Header { message_id } => ChatRowId::Header(*message_id),
            ChatRowKey::Block {
                message_id,
                block_id,
                ..
            } => ChatRowId::Block {
                message_id: *message_id,
                block_id: *block_id,
            },
            ChatRowKey::PendingToolResult {
                message_id,
                tool_use_id,
                ..
            } => ChatRowId::PendingToolResult {
                message_id: *message_id,
                tool_use_id: *tool_use_id,
            },
        }
    }
}

impl ChatRowKey {
    fn message_id(&self) -> ChatMessageId {
        match self {
            ChatRowKey::Header { message_id }
            | ChatRowKey::Block { message_id, .. }
            | ChatRowKey::PendingToolResult { message_id, .. } => *message_id,
        }
    }

    fn row_ref(&self) -> ChatRowRef {
        match self {
            ChatRowKey::Header { message_id } => ChatRowRef::Header(*message_id),
            ChatRowKey::Block { block_id, .. } => ChatRowRef::Block(*block_id),
            ChatRowKey::PendingToolResult { tool_use_id, .. } => {
                ChatRowRef::PendingToolResult(*tool_use_id)
            }
        }
    }

    fn placeholder(&self) -> ChatMessage {
        ChatMessage {
            id: self.message_id(),
            role: ChatRole::Assistant,
            status: ChatTurnStatus::Complete,
            meta: ChatMessageMeta::default(),
            blocks: match self {
                ChatRowKey::Header { .. } => Vec::new(),
                ChatRowKey::Block {
                    block_id, kind_tag, ..
                } => placeholder_block(*block_id, kind_tag).into_iter().collect(),
                ChatRowKey::PendingToolResult { .. } => Vec::new(),
            },
        }
    }
}

fn placeholder_block(block_id: ChatBlockId, kind_tag: &ChatBlockKindTag) -> Option<ChatBlock> {
    match kind_tag {
        ChatBlockKindTag::Text => Some(ChatBlock::Text(TextBlock {
            id: block_id,
            markdown: String::new(),
            streaming: false,
        })),
        ChatBlockKindTag::Thinking { collapsed } => Some(ChatBlock::Thinking(ThinkingBlock {
            id: block_id,
            markdown: String::new(),
            streaming: false,
            collapsed: *collapsed,
        })),
        ChatBlockKindTag::Attachment { name, url, mime } => {
            Some(ChatBlock::Attachment(AttachmentBlock {
                id: block_id,
                name: name.clone(),
                url: url.clone(),
                mime: mime.clone(),
            }))
        }
        ChatBlockKindTag::ToolUse { call_id, name } => Some(ChatBlock::ToolUse(ToolUseBlock {
            id: block_id,
            call_id: call_id.clone(),
            name: name.clone(),
            input: ToolInput::Text(String::new()),
            status: ToolStatus::Running,
            approval: None,
            collapsed: false,
        })),
        ChatBlockKindTag::ToolResult {
            call_id,
            ok,
            exit_code,
            collapsed,
        } => Some(ChatBlock::ToolResult(ToolResultBlock {
            id: block_id,
            call_id: call_id.clone(),
            ok: *ok,
            exit_code: *exit_code,
            output: ToolOutput::Ansi(String::new()),
            collapsed: *collapsed,
        })),
        ChatBlockKindTag::Diff { path, decision } => Some(ChatBlock::Diff(DiffBlock {
            id: block_id,
            path: path.clone(),
            diff: DiffData {
                unified: String::new(),
            },
            decision: *decision,
        })),
        ChatBlockKindTag::Todo => Some(ChatBlock::Todo(TodoBlock {
            id: block_id,
            items: Vec::new(),
        })),
        ChatBlockKindTag::Notice { level, text } => Some(ChatBlock::Notice(NoticeBlock {
            id: block_id,
            level: *level,
            text: text.clone(),
        })),
        ChatBlockKindTag::Artifact {
            kind,
            anchor,
            title,
        } => Some(ChatBlock::Artifact(ArtifactBlock {
            id: block_id,
            kind: kind.clone(),
            anchor: anchor.clone(),
            title: title.clone(),
        })),
    }
}

#[derive(Clone, Debug)]
struct ToolResultRowCandidate {
    message_id: ChatMessageId,
    block_id: ChatBlockId,
    order: usize,
    kind_tag: ChatBlockKindTag,
}

fn row_keys_from_messages(messages: &[ChatMessage]) -> Vec<ChatRowKey> {
    let result_candidates = collect_tool_result_candidates(messages);
    let mut paired_results = HashSet::new();
    let mut rows = Vec::new();
    let mut order = 0usize;

    for message in messages {
        let mut header_inserted = false;
        for block in &message.blocks {
            let block_order = order;
            order = order.saturating_add(1);

            if paired_results.contains(&block.id()) {
                continue;
            }

            ensure_message_header(&mut rows, &mut header_inserted, message.id);
            rows.push(block_row_key(message.id, block));

            if let ChatBlock::ToolUse(tool) = block {
                if let Some(result) = matching_tool_result_candidate(
                    &result_candidates,
                    &paired_results,
                    &tool.call_id,
                    block_order,
                ) {
                    paired_results.insert(result.block_id);
                    rows.push(ChatRowKey::Block {
                        message_id: result.message_id,
                        block_id: result.block_id,
                        kind_tag: result.kind_tag.clone(),
                    });
                } else {
                    rows.push(ChatRowKey::PendingToolResult {
                        message_id: message.id,
                        tool_use_id: tool.id,
                        call_id: tool.call_id.clone(),
                    });
                }
            }
        }
    }

    rows
}

fn collect_tool_result_candidates(
    messages: &[ChatMessage],
) -> HashMap<String, Vec<ToolResultRowCandidate>> {
    let mut candidates: HashMap<String, Vec<ToolResultRowCandidate>> = HashMap::new();
    let mut order = 0usize;
    for message in messages {
        for block in &message.blocks {
            if let ChatBlock::ToolResult(result) = block {
                candidates.entry(result.call_id.clone()).or_default().push(
                    ToolResultRowCandidate {
                        message_id: message.id,
                        block_id: result.id,
                        order,
                        kind_tag: block_kind_tag(block),
                    },
                );
            }
            order = order.saturating_add(1);
        }
    }
    candidates
}

fn matching_tool_result_candidate<'a>(
    candidates: &'a HashMap<String, Vec<ToolResultRowCandidate>>,
    paired_results: &HashSet<ChatBlockId>,
    call_id: &str,
    after_order: usize,
) -> Option<&'a ToolResultRowCandidate> {
    candidates.get(call_id)?.iter().find(|candidate| {
        candidate.order > after_order && !paired_results.contains(&candidate.block_id)
    })
}

fn ensure_message_header(
    rows: &mut Vec<ChatRowKey>,
    header_inserted: &mut bool,
    message_id: ChatMessageId,
) {
    if *header_inserted {
        return;
    }
    rows.push(ChatRowKey::Header { message_id });
    *header_inserted = true;
}

fn block_row_key(message_id: ChatMessageId, block: &ChatBlock) -> ChatRowKey {
    ChatRowKey::Block {
        message_id,
        block_id: block.id(),
        kind_tag: block_kind_tag(block),
    }
}

fn block_kind_tag(block: &ChatBlock) -> ChatBlockKindTag {
    match block {
        ChatBlock::Text(_) => ChatBlockKindTag::Text,
        ChatBlock::Thinking(ThinkingBlock { collapsed, .. }) => ChatBlockKindTag::Thinking {
            collapsed: *collapsed,
        },
        ChatBlock::Attachment(AttachmentBlock {
            name, url, mime, ..
        }) => ChatBlockKindTag::Attachment {
            name: name.clone(),
            url: url.clone(),
            mime: mime.clone(),
        },
        ChatBlock::ToolUse(ToolUseBlock { call_id, name, .. }) => ChatBlockKindTag::ToolUse {
            call_id: call_id.clone(),
            name: name.clone(),
        },
        ChatBlock::ToolResult(ToolResultBlock {
            call_id,
            ok,
            exit_code,
            collapsed,
            ..
        }) => ChatBlockKindTag::ToolResult {
            call_id: call_id.clone(),
            ok: *ok,
            exit_code: *exit_code,
            collapsed: *collapsed,
        },
        ChatBlock::Diff(DiffBlock { path, decision, .. }) => ChatBlockKindTag::Diff {
            path: path.clone(),
            decision: *decision,
        },
        ChatBlock::Todo(_) => ChatBlockKindTag::Todo,
        ChatBlock::Notice(NoticeBlock { level, text, .. }) => ChatBlockKindTag::Notice {
            level: *level,
            text: text.clone(),
        },
        ChatBlock::Artifact(ArtifactBlock {
            kind,
            anchor,
            title,
            ..
        }) => ChatBlockKindTag::Artifact {
            kind: kind.clone(),
            anchor: anchor.clone(),
            title: title.clone(),
        },
    }
}

#[derive(Default)]
struct ChatMessageRowBindings {
    header: Option<Binding<String>>,
    timestamp: Option<Binding<Option<String>>>,
    markdown: Option<Binding<String>>,
    diff: Option<Binding<String>>,
    todo_items: Option<Binding<Vec<TodoItem>>>,
    tool_use: Option<Binding<ToolUseDetails>>,
    tool_output: Option<Binding<String>>,
    disclosure_status: Option<Binding<DisclosureStatus>>,
}

struct ChatMessageRow {
    row_ref: ChatRowRef,
    store: ChatMessageStore,
    last_message_version: Cell<u64>,
    last_block_version: Cell<u64>,
    body_bindings: ChatMessageRowBindings,
    config: ChatMessageRowConfig,
    view: VStack,
}

impl ChatMessageRow {
    fn new(key: ChatRowKey, store: ChatMessageStore, config: ChatMessageRowConfig) -> Self {
        let message_id = key.message_id();
        let row_ref = key.row_ref();
        let (view, body_bindings) = store
            .with_message(message_id, |message| build_row_view(message, &key, &config))
            .unwrap_or_else(|| {
                let message = key.placeholder();
                build_row_view(&message, &key, &config)
            });
        let last_message_version = match row_ref {
            ChatRowRef::Header(message_id) => store.message_version(message_id),
            ChatRowRef::Block(_) | ChatRowRef::PendingToolResult(_) => 0,
        };
        let last_block_version = match row_ref {
            ChatRowRef::Header(_) => 0,
            ChatRowRef::Block(block_id) => store.block_version(block_id),
            ChatRowRef::PendingToolResult(tool_use_id) => store.block_version(tool_use_id),
        };
        Self {
            row_ref,
            store,
            last_message_version: Cell::new(last_message_version),
            last_block_version: Cell::new(last_block_version),
            body_bindings,
            config,
            view,
        }
    }

    fn sync_body_bindings(&self) {
        match self.row_ref {
            ChatRowRef::Header(message_id) => self.sync_header_bindings(message_id),
            ChatRowRef::Block(block_id) => self.sync_block_bindings(block_id),
            ChatRowRef::PendingToolResult(tool_use_id) => self.sync_block_bindings(tool_use_id),
        }
    }

    fn sync_header_bindings(&self, message_id: ChatMessageId) {
        let version = self.store.message_version(message_id);
        if version == self.last_message_version.get() {
            return;
        }
        self.store.with_message(message_id, |message| {
            if let Some(binding) = &self.body_bindings.header {
                binding.set(turn_header_label(message));
            }
            if let Some(binding) = &self.body_bindings.timestamp {
                binding.set(message.meta.timestamp.clone());
            }
        });
        self.last_message_version.set(version);
    }

    fn sync_block_bindings(&self, block_id: ChatBlockId) {
        let version = self.store.block_version(block_id);
        if version == self.last_block_version.get() {
            return;
        }
        self.store.with_block(block_id, |block| {
            if let Some(binding) = &self.body_bindings.markdown
                && let Some(markdown) = block_markdown_for_render(block, &self.config)
            {
                binding.set(markdown);
            }

            if let Some(binding) = &self.body_bindings.tool_output
                && let Some(output) = block_tool_output_for_render(block)
            {
                binding.set(output);
            }

            if let Some(binding) = &self.body_bindings.diff
                && let Some(diff) = block_diff_for_render(block)
            {
                binding.set(diff);
            }

            if let Some(binding) = &self.body_bindings.todo_items
                && let Some(items) = block_todo_items_for_render(block)
            {
                binding.set(items);
            }

            if let Some(binding) = &self.body_bindings.tool_use
                && let Some(details) = block_tool_use_for_render(block)
            {
                binding.set(details);
            }

            if let Some(binding) = &self.body_bindings.disclosure_status
                && let Some(status) = block_disclosure_status(block)
            {
                binding.set(status);
            }
        });
        self.last_block_version.set(version);
    }
}

fn build_row_view(
    message: &ChatMessage,
    key: &ChatRowKey,
    config: &ChatMessageRowConfig,
) -> (VStack, ChatMessageRowBindings) {
    let mut column = VStack::new().with_spacing(1);
    let row_layout = LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    };

    match key {
        ChatRowKey::Header { .. } => {
            let (bubble, mut bindings) = build_aligned_turn_header(message);
            if config.show_timestamps
                && let Some(ts) = &message.meta.timestamp
            {
                let timestamp = Binding::new(Some(ts.clone()));
                bindings.timestamp = Some(timestamp.clone());
                column = column.child_with_layout(ChatTimestampDivider::new(timestamp), row_layout);
            }
            column = column.child_with_layout(bubble, row_layout);
            (column, bindings)
        }
        ChatRowKey::Block { block_id, .. } => {
            let block = find_block(message, *block_id);
            let (bubble, body_bindings) = build_aligned_block(message, block, config);
            column = column.child_with_layout(bubble, row_layout);
            (column, body_bindings)
        }
        ChatRowKey::PendingToolResult { call_id, .. } => {
            let (bubble, body_bindings) = build_aligned_pending_tool_result(message, call_id);
            column = column.child_with_layout(bubble, row_layout);
            (column, body_bindings)
        }
    }
}

fn find_block(message: &ChatMessage, id: ChatBlockId) -> Option<&ChatBlock> {
    message.blocks.iter().find(|block| block.id() == id)
}

fn block_markdown_for_render(block: &ChatBlock, config: &ChatMessageRowConfig) -> Option<String> {
    let (markdown, streaming_cursor) = match block {
        ChatBlock::Text(text) => (&text.markdown, text.streaming),
        ChatBlock::Thinking(thinking) => (&thinking.markdown, false),
        _ => return None,
    };
    Some(markdown_for_render(markdown, streaming_cursor, config))
}

fn markdown_for_render(markdown: &str, streaming: bool, config: &ChatMessageRowConfig) -> String {
    let mut content = markdown.to_string();
    if streaming && !config.in_progress_suffix.is_empty() {
        content.push_str(&config.in_progress_suffix);
    }
    content
}

fn block_tool_output_for_render(block: &ChatBlock) -> Option<String> {
    match block {
        ChatBlock::ToolResult(result) => Some(result.output.as_text().to_string()),
        _ => None,
    }
}

fn block_diff_for_render(block: &ChatBlock) -> Option<String> {
    match block {
        ChatBlock::Diff(diff) => Some(diff.diff.unified.clone()),
        _ => None,
    }
}

fn block_todo_items_for_render(block: &ChatBlock) -> Option<Vec<TodoItem>> {
    match block {
        ChatBlock::Todo(todo) => Some(todo.items.clone()),
        _ => None,
    }
}

fn block_tool_use_for_render(block: &ChatBlock) -> Option<ToolUseDetails> {
    match block {
        ChatBlock::ToolUse(tool) => Some(ToolUseDetails::from(tool)),
        _ => None,
    }
}

fn block_disclosure_status(block: &ChatBlock) -> Option<DisclosureStatus> {
    match block {
        ChatBlock::Thinking(thinking) => Some(thinking_status_to_disclosure(thinking)),
        ChatBlock::ToolUse(tool) => Some(tool_status_to_disclosure(&tool.status)),
        ChatBlock::ToolResult(result) => Some(tool_result_to_disclosure(result)),
        _ => None,
    }
}

impl ::atto_ui::composable::Component for ChatMessageRow {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.sync_body_bindings();
        self.view.draw(frame, area, ctx);
    }
}

impl ::atto_ui::composable::DragAndDrop for ChatMessageRow {}

impl ::atto_ui::composable::Layout for ChatMessageRow {
    fn min_width(&self) -> u16 {
        self.sync_body_bindings();
        self.view.min_width()
    }

    fn min_height(&self) -> u16 {
        self.sync_body_bindings();
        self.view.min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.sync_body_bindings();
        self.view.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.sync_body_bindings();
        self.view.desired_height()
    }
}

impl ::atto_ui::composable::Scrollable for ChatMessageRow {}

impl ::atto_ui::composable::FocusNav for ChatMessageRow {}

impl ::atto_ui::composable::DynamicTree for ChatMessageRow {}

impl ::atto_ui::composable::EventHandling for ChatMessageRow {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.sync_body_bindings();
        self.view.handle_event(event, ctx)
    }
}

fn build_aligned_turn_header(message: &ChatMessage) -> (HStack, ChatMessageRowBindings) {
    let (bubble, bindings) = build_turn_header(message);
    let bubble_layout = LayoutParams {
        width: Size::Weight(3),
        height: Size::Content,
        ..LayoutParams::default()
    };
    let spacer_layout = LayoutParams {
        width: Size::Weight(1),
        ..LayoutParams::default()
    };

    let row = match message.role.alignment() {
        ChatAlignment::Left => HStack::new()
            .child_with_layout(bubble, bubble_layout)
            .child_with_layout(Spacer::new(), spacer_layout),
        ChatAlignment::Right => HStack::new()
            .child_with_layout(Spacer::new(), spacer_layout)
            .child_with_layout(bubble, bubble_layout),
    };
    (row, bindings)
}

fn build_aligned_block(
    message: &ChatMessage,
    block: Option<&ChatBlock>,
    config: &ChatMessageRowConfig,
) -> (HStack, ChatMessageRowBindings) {
    let (bubble, body_bindings) = build_block_bubble(message.id, block, config);
    let bubble_layout = LayoutParams {
        width: Size::Weight(3),
        height: Size::Content,
        ..LayoutParams::default()
    };
    let spacer_layout = LayoutParams {
        width: Size::Weight(1),
        ..LayoutParams::default()
    };

    let row = match message.role.alignment() {
        ChatAlignment::Left => HStack::new()
            .child_with_layout(bubble, bubble_layout)
            .child_with_layout(Spacer::new(), spacer_layout),
        ChatAlignment::Right => HStack::new()
            .child_with_layout(Spacer::new(), spacer_layout)
            .child_with_layout(bubble, bubble_layout),
    };
    (row, body_bindings)
}

fn build_aligned_pending_tool_result(
    message: &ChatMessage,
    call_id: &str,
) -> (HStack, ChatMessageRowBindings) {
    let body = ChatMessageBody::Disclosure(
        Disclosure::new(pending_tool_result_title(call_id))
            .expanded(true)
            .status(DisclosureStatus::Running)
            .child(Text::new("等待中")),
    );
    let bubble = VStack::new().with_spacing(1).child_with_layout(
        body,
        LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        },
    );
    let bubble_layout = LayoutParams {
        width: Size::Weight(3),
        height: Size::Content,
        ..LayoutParams::default()
    };
    let spacer_layout = LayoutParams {
        width: Size::Weight(1),
        ..LayoutParams::default()
    };

    let row = match message.role.alignment() {
        ChatAlignment::Left => HStack::new()
            .child_with_layout(bubble, bubble_layout)
            .child_with_layout(Spacer::new(), spacer_layout),
        ChatAlignment::Right => HStack::new()
            .child_with_layout(Spacer::new(), spacer_layout)
            .child_with_layout(bubble, bubble_layout),
    };
    (row, ChatMessageRowBindings::default())
}

fn build_turn_header(message: &ChatMessage) -> (VStack, ChatMessageRowBindings) {
    let header_label = Binding::new(turn_header_label(message));
    let header = Text::new(String::new()).text(header_label.clone()).style(
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let content_layout = LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    };

    let bubble = VStack::new()
        .with_spacing(1)
        .child_with_layout(header, content_layout);

    (
        bubble,
        ChatMessageRowBindings {
            header: Some(header_label),
            ..ChatMessageRowBindings::default()
        },
    )
}

fn build_block_bubble(
    message_id: ChatMessageId,
    block: Option<&ChatBlock>,
    config: &ChatMessageRowConfig,
) -> (VStack, ChatMessageRowBindings) {
    let (body, body_bindings) = ChatMessageBody::from_block(message_id, block, config);
    let content_layout = LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    };

    let bubble = VStack::new()
        .with_spacing(1)
        .child_with_layout(body, content_layout);

    (bubble, body_bindings)
}

fn turn_header_label(message: &ChatMessage) -> String {
    let mut label = message.role.label();
    match &message.status {
        ChatTurnStatus::Failed(error) => {
            label.push_str(" · failed");
            append_turn_meta_lines(&mut label, &message.meta);
            label.push_str(&format!(
                "\nError kind: {}\nError message: {}",
                error_kind_label(&error.kind),
                error.message
            ));
            if let Some(detail) = error.detail.as_deref().filter(|detail| !detail.is_empty()) {
                label.push_str(&format!("\nError detail: {detail}"));
            }
            return label;
        }
        ChatTurnStatus::Canceled => {
            label.push_str(" · canceled");
        }
        ChatTurnStatus::Complete | ChatTurnStatus::Streaming => {}
    }

    append_turn_meta_lines(&mut label, &message.meta);
    label
}

fn append_turn_meta_lines(label: &mut String, meta: &ChatMessageMeta) {
    for part in turn_meta_parts(meta) {
        label.push('\n');
        label.push_str(&part);
    }
}

fn turn_meta_parts(meta: &ChatMessageMeta) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(model) = meta.model.as_deref().filter(|model| !model.is_empty()) {
        parts.push(format!("model: {model}"));
    }
    if let Some(usage) = &meta.usage {
        parts.push(format!(
            "usage: {} input/{} output",
            usage.input, usage.output
        ));
    }
    if let Some(elapsed_ms) = meta.elapsed_ms {
        parts.push(format!("elapsed: {elapsed_ms}ms"));
    }
    if let Some(stop_reason) = &meta.stop_reason {
        parts.push(format!("stop: {}", stop_reason_label(stop_reason)));
    }
    parts
}

fn error_kind_label(kind: &ChatErrorKind) -> &'static str {
    match kind {
        ChatErrorKind::Api => "api",
        ChatErrorKind::Tool => "tool",
        ChatErrorKind::RateLimit => "rate_limit",
        ChatErrorKind::Refusal => "refusal",
        ChatErrorKind::Network => "network",
        ChatErrorKind::Other => "other",
    }
}

fn stop_reason_label(reason: &StopReason) -> &'static str {
    match reason {
        StopReason::EndTurn => "end_turn",
        StopReason::MaxTokens => "max_tokens",
        StopReason::ToolUse => "tool_use",
        StopReason::StopSequence => "stop_sequence",
        StopReason::Refusal => "refusal",
    }
}

struct ChatTimestampDivider {
    label: Binding<Option<String>>,
}

impl ChatTimestampDivider {
    fn new(label: impl Into<Binding<Option<String>>>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

impl ::atto_ui::composable::Component for ChatTimestampDivider {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let Some(raw_label) = self.label.get() else {
            return;
        };
        let line = labeled_divider_line(&raw_label, area.width as usize);
        let style = ctx.theme.widget.dim;
        frame.render_widget(Paragraph::new(Line::styled(line, style)), area);
    }
}

fn labeled_divider_line(raw_label: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let label = format!(" {raw_label} ");
    let label_width = UnicodeWidthStr::width(label.as_str());
    if label_width >= width {
        return fit_to_display_width(raw_label, width);
    }

    let padding = width.saturating_sub(label_width);
    let left = padding / 2;
    let right = padding.saturating_sub(left);
    format!("{}{}{}", "─".repeat(left), label, "─".repeat(right))
}

fn fit_to_display_width(text: &str, width: usize) -> String {
    let mut line = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used.saturating_add(char_width) > width {
            break;
        }
        line.push(ch);
        used = used.saturating_add(char_width);
    }
    if used < width {
        line.push_str(&" ".repeat(width - used));
    }
    line
}

impl ::atto_ui::composable::DragAndDrop for ChatTimestampDivider {}

impl ::atto_ui::composable::Layout for ChatTimestampDivider {
    fn desired_height(&self) -> Option<u16> {
        Some(self.min_height())
    }

    fn min_height(&self) -> u16 {
        u16::from(self.label.get().is_some())
    }
}

impl ::atto_ui::composable::Scrollable for ChatTimestampDivider {}

impl ::atto_ui::composable::FocusNav for ChatTimestampDivider {}

impl ::atto_ui::composable::DynamicTree for ChatTimestampDivider {}

impl ::atto_ui::composable::EventHandling for ChatTimestampDivider {}

struct ResponsiveMarkdownView {
    view: MarkdownViewer,
    width: Binding<Option<u16>>,
    fallback_width: Binding<Option<u16>>,
    fallback_indent: u16,
    max_width: Option<u16>,
}

impl ResponsiveMarkdownView {
    fn new(content: Binding<String>, config: &ChatMessageRowConfig, fallback_indent: u16) -> Self {
        let width = Binding::new(markdown_width_from_fallback(
            config.responsive_wrap_width.get(),
            fallback_indent,
            config.wrap_width,
        ));
        let view = MarkdownViewer::new(content).wrap_width_binding(width.clone());
        Self {
            view,
            width,
            fallback_width: config.responsive_wrap_width.clone(),
            fallback_indent,
            max_width: config.wrap_width,
        }
    }

    fn map_view(mut self, f: impl FnOnce(MarkdownViewer) -> MarkdownViewer) -> Self {
        self.view = f(self.view);
        self
    }

    fn sync_fallback_width(&self) {
        if let Some(width) = markdown_width_from_fallback(
            self.fallback_width.get(),
            self.fallback_indent,
            self.max_width,
        ) {
            self.width.set(Some(width));
        }
    }

    fn sync_area_width(&self, area_width: u16) {
        if area_width == 0 {
            return;
        }
        self.width
            .set(Some(apply_markdown_width_cap(area_width, self.max_width)));
    }
}

fn markdown_width_from_fallback(
    fallback: Option<u16>,
    indent: u16,
    max_width: Option<u16>,
) -> Option<u16> {
    let width = fallback?.saturating_sub(indent).max(1);
    Some(apply_markdown_width_cap(width, max_width))
}

fn apply_markdown_width_cap(width: u16, max_width: Option<u16>) -> u16 {
    max_width.map_or(width, |max| width.min(max)).max(1)
}

impl Component for ResponsiveMarkdownView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.sync_area_width(area.width);
        self.view.draw(frame, area, ctx);
    }
}

impl ::atto_ui::composable::DragAndDrop for ResponsiveMarkdownView {}

impl ::atto_ui::composable::Layout for ResponsiveMarkdownView {
    fn min_width(&self) -> u16 {
        self.view.min_width()
    }

    fn min_height(&self) -> u16 {
        self.view.min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.sync_fallback_width();
        self.view.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.sync_fallback_width();
        self.view.desired_height()
    }
}

impl ::atto_ui::composable::Scrollable for ResponsiveMarkdownView {
    fn is_scrollable(&self) -> bool {
        self.view.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.view.content_size()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.view.viewport_size()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.view.scroll_config()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.view.scroll_offset()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.view.set_scroll_offset(x, y);
    }
}

impl ::atto_ui::composable::FocusNav for ResponsiveMarkdownView {
    fn is_focusable(&self) -> bool {
        self.view.is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        self.view.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.view.focus_last()
    }
}

impl ::atto_ui::composable::DynamicTree for ResponsiveMarkdownView {}

impl ::atto_ui::composable::EventHandling for ResponsiveMarkdownView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.view.handle_event(event, ctx)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ToolUseDetails {
    input: ToolInput,
    approval: Option<ApprovalRequest>,
}

impl From<&ToolUseBlock> for ToolUseDetails {
    fn from(tool: &ToolUseBlock) -> Self {
        Self {
            input: tool.input.clone(),
            approval: tool.approval.clone(),
        }
    }
}

struct ToolUseDetailsView {
    details: Binding<ToolUseDetails>,
    message_id: ChatMessageId,
    block_id: ChatBlockId,
    on_approve: Option<ApprovalCallback>,
    last_details: Option<ToolUseDetails>,
    last_area: Option<Rect>,
    view: VStack,
}

impl ToolUseDetailsView {
    fn new(
        details: impl Into<Binding<ToolUseDetails>>,
        message_id: ChatMessageId,
        block_id: ChatBlockId,
        on_approve: Option<ApprovalCallback>,
    ) -> Self {
        let details = details.into();
        let initial_details = details.get();
        let view = build_tool_use_details_stack(
            &initial_details,
            message_id,
            block_id,
            on_approve.clone(),
        );
        Self {
            details,
            message_id,
            block_id,
            on_approve,
            last_details: Some(initial_details),
            last_area: None,
            view,
        }
    }

    fn sync_view(&mut self) {
        let details = self.details.get();
        if self.last_details.as_ref() == Some(&details) {
            return;
        }
        self.view = build_tool_use_details_stack(
            &details,
            self.message_id,
            self.block_id,
            self.on_approve.clone(),
        );
        self.last_details = Some(details);
    }

    fn has_focusable_approval(&self) -> bool {
        self.details.with(|details| {
            details.approval.as_ref().is_some_and(|approval| {
                approval.resolved.is_none()
                    && !approval.options.is_empty()
                    && self.on_approve.is_some()
            })
        })
    }

    fn event_for_inner<'a>(
        &self,
        event: &Event,
        mut ctx: ComponentContext<'a>,
    ) -> (Event, ComponentContext<'a>) {
        let Event::Mouse(mouse) = event else {
            return (event.clone(), ctx);
        };
        if ctx.mouse_coordinate_space != MouseCoordinateSpace::Local {
            return (event.clone(), ctx);
        }
        let Some(area) = self.last_area else {
            return (event.clone(), ctx);
        };

        ctx.mouse_coordinate_space = MouseCoordinateSpace::Absolute;
        (
            Event::Mouse(MouseEvent {
                column: area.x.saturating_add(mouse.column),
                row: area.y.saturating_add(mouse.row),
                ..*mouse
            }),
            ctx,
        )
    }
}

fn build_tool_use_details_stack(
    details: &ToolUseDetails,
    message_id: ChatMessageId,
    block_id: ChatBlockId,
    on_approve: Option<ApprovalCallback>,
) -> VStack {
    let content_layout = LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    };
    let mut column = VStack::new().with_spacing(1);
    let input_lines = tool_input_detail_lines(&details.input);
    if !input_lines.is_empty() {
        column = column.child_with_layout(Text::new(input_lines.join("\n")), content_layout);
    }

    let Some(approval) = &details.approval else {
        if input_lines.is_empty() {
            column = column.child_with_layout(Text::new(String::new()), content_layout);
        }
        return column;
    };

    column = column.child_with_layout(
        Text::new(format!("Approval: {}", approval.prompt)),
        content_layout,
    );

    if let Some(resolved) = &approval.resolved {
        return column.child_with_layout(
            Text::new(approval_resolved_label(approval, resolved)),
            content_layout,
        );
    }

    let mut row = HStack::new().with_spacing(1);
    let enabled = on_approve.is_some();
    for option in &approval.options {
        let label = approval_option_button_label(approval, option);
        let mut button = Button::new(label.clone()).enabled(enabled);
        if enabled {
            let decision = ApprovalDecision {
                message_id,
                block_id,
                approval_id: approval.id.clone(),
                option_id: option.id.clone(),
            };
            let callback = on_approve.clone();
            button = button.on_click(move || {
                if let Some(callback) = &callback {
                    callback(decision.clone());
                }
            });
        }
        row = row.child_with_layout(
            button,
            LayoutParams {
                width: Size::Fixed(button_width_for_label(&label)),
                height: Size::Content,
                ..LayoutParams::default()
            },
        );
    }

    if approval.options.is_empty() {
        column.child_with_layout(Text::new("No approval options"), content_layout)
    } else {
        column.child_with_layout(row, content_layout)
    }
}

fn approval_option_button_label(approval: &ApprovalRequest, option: &ApprovalOption) -> String {
    match approval.resolved.as_deref() {
        Some(resolved) if resolved == option.id => format!("[x] {}", option.label),
        Some(_) => format!("[ ] {}", option.label),
        None => option.label.clone(),
    }
}

fn approval_resolved_label(approval: &ApprovalRequest, resolved: &str) -> String {
    let label = approval
        .options
        .iter()
        .find(|option| option.id == resolved)
        .map(|option| option.label.as_str())
        .unwrap_or(resolved);
    format!("[x] {label}")
}

fn button_width_for_label(label: &str) -> u16 {
    label
        .width()
        .saturating_add(4)
        .max(3)
        .min(u16::MAX as usize) as u16
}

fn tool_use_details_desired_width(details: &ToolUseDetails) -> u16 {
    let input_width = max_line_width(&tool_input_detail_lines(&details.input));
    let Some(approval) = &details.approval else {
        return input_width;
    };

    let prompt_width = format!("Approval: {}", approval.prompt)
        .width()
        .min(u16::MAX as usize) as u16;
    if let Some(resolved) = &approval.resolved {
        let resolved_width = approval_resolved_label(approval, resolved)
            .width()
            .min(u16::MAX as usize) as u16;
        return input_width.max(prompt_width).max(resolved_width).max(1);
    }

    let options_width = if approval.options.is_empty() {
        "No approval options".width().min(u16::MAX as usize) as u16
    } else {
        approval
            .options
            .iter()
            .map(|option| button_width_for_label(&approval_option_button_label(approval, option)))
            .fold(0u16, |width, option_width| {
                if width == 0 {
                    option_width
                } else {
                    width.saturating_add(1).saturating_add(option_width)
                }
            })
    };

    input_width.max(prompt_width).max(options_width).max(1)
}

fn tool_use_details_desired_height(details: &ToolUseDetails) -> u16 {
    let input_height = tool_input_detail_lines(&details.input)
        .len()
        .max(1)
        .min(u16::MAX as usize) as u16;
    if details.approval.is_none() {
        return input_height;
    }

    // Input text, approval prompt, and one option row separated by VStack spacing.
    input_height.saturating_add(4)
}

impl Component for ToolUseDetailsView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        self.sync_view();
        self.view.draw(frame, area, ctx);
    }
}

impl ::atto_ui::composable::DragAndDrop for ToolUseDetailsView {}
impl ::atto_ui::composable::Scrollable for ToolUseDetailsView {}
impl ::atto_ui::composable::FocusNav for ToolUseDetailsView {
    fn is_focusable(&self) -> bool {
        self.has_focusable_approval()
    }

    fn focus_first(&mut self) -> bool {
        self.sync_view();
        self.view.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.sync_view();
        self.view.focus_last()
    }
}
impl ::atto_ui::composable::DynamicTree for ToolUseDetailsView {}
impl ::atto_ui::composable::EventHandling for ToolUseDetailsView {
    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.sync_view();
        let (event, ctx) = self.event_for_inner(event, ctx);
        self.view.handle_event_capture(&event, ctx)
    }

    fn handle_event_bubble(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.sync_view();
        let (event, ctx) = self.event_for_inner(event, ctx);
        self.view.handle_event_bubble(&event, ctx)
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.sync_view();
        let (event, ctx) = self.event_for_inner(event, ctx);
        self.view.handle_event(&event, ctx)
    }
}

impl ::atto_ui::composable::Layout for ToolUseDetailsView {
    fn min_width(&self) -> u16 {
        1
    }

    fn min_height(&self) -> u16 {
        1
    }

    fn desired_width(&self) -> Option<u16> {
        Some(self.details.with(tool_use_details_desired_width))
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.details.with(tool_use_details_desired_height))
    }
}

struct DiffDecisionView {
    title: Option<String>,
    diff: Binding<String>,
    message_id: ChatMessageId,
    block_id: ChatBlockId,
    decision: EditDecision,
    on_edit_decision: Option<EditDecisionCallback>,
    focused_action: usize,
    scroll_x: u16,
    viewport: (u16, u16),
    last_area: Option<Rect>,
}

impl DiffDecisionView {
    fn new(
        title: Option<String>,
        diff: impl Into<Binding<String>>,
        message_id: ChatMessageId,
        block_id: ChatBlockId,
        decision: EditDecision,
        on_edit_decision: Option<EditDecisionCallback>,
    ) -> Self {
        Self {
            title,
            diff: diff.into(),
            message_id,
            block_id,
            decision,
            on_edit_decision,
            focused_action: 0,
            scroll_x: 0,
            viewport: (0, 0),
            last_area: None,
        }
    }

    fn diff_lines(&self, base: Style, title_style: Style) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if let Some(title) = &self.title {
            lines.push(Line::styled(title.clone(), title_style));
        }
        let diff = self.diff.get();
        lines.extend(diff_display_lines(&diff, base));
        lines
    }

    fn diff_height(&self) -> u16 {
        u16::from(self.title.is_some()).saturating_add(line_count(&self.diff.get()))
    }

    fn action_height(&self) -> u16 {
        1
    }

    fn display_height(&self) -> u16 {
        self.diff_height().saturating_add(self.action_height())
    }

    fn display_width(&self) -> u16 {
        let diff_width = self
            .diff
            .get()
            .lines()
            .map(UnicodeWidthStr::width)
            .max()
            .unwrap_or(0);
        let title_width = self
            .title
            .as_deref()
            .map(UnicodeWidthStr::width)
            .unwrap_or(0);
        diff_width
            .max(title_width)
            .max(edit_decision_action_line_width(self.decision))
            .max(1)
            .min(u16::MAX as usize) as u16
    }

    fn clamp_scroll(&mut self) {
        let max_x = self.display_width().saturating_sub(self.viewport.0);
        self.scroll_x = self.scroll_x.min(max_x);
    }

    fn scroll_horizontally(&mut self, delta: i16) -> EventResult {
        let before = self.scroll_x;
        self.set_scroll_offset(add_signed_u16(self.scroll_x, delta), 0);
        if self.scroll_x == before {
            EventResult::ignored()
        } else {
            EventResult::changed()
        }
    }

    fn has_focusable_action(&self) -> bool {
        self.decision == EditDecision::Pending && self.on_edit_decision.is_some()
    }

    fn emit_decision(&self, decision: EditDecision) -> EventResult {
        let Some(callback) = &self.on_edit_decision else {
            return EventResult::ignored();
        };
        if self.decision != EditDecision::Pending {
            return EventResult::ignored();
        }
        callback(EditDecisionEvent {
            message_id: self.message_id,
            block_id: self.block_id,
            decision,
        });
        EventResult::changed()
    }

    fn focused_decision(&self) -> EditDecision {
        if self.focused_action == 0 {
            EditDecision::Accepted
        } else {
            EditDecision::Rejected
        }
    }

    fn click_decision(&self, event: &Event, ctx: ComponentContext<'_>) -> Option<EditDecision> {
        let Event::Mouse(mouse) = event else {
            return None;
        };
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return None;
        }
        let area = self.last_area?;
        let (column, row) = mouse_position_in_area(area, mouse, ctx.mouse_coordinate_space)?;
        if row != self.diff_height() {
            return None;
        }
        edit_decision_action_at_column(column)
    }
}

impl Component for DiffDecisionView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        self.viewport = (area.width, area.height);
        self.clamp_scroll();
        if area.width == 0 || area.height == 0 {
            return;
        }

        let diff_height = self.diff_height().min(area.height);
        if diff_height > 0 {
            let diff_area = Rect {
                height: diff_height,
                ..area
            };
            frame.render_widget(
                Paragraph::new(self.diff_lines(ctx.theme.widget.normal, ctx.theme.widget.dim))
                    .scroll((0, self.scroll_x)),
                diff_area,
            );
        }

        if area.height > diff_height {
            let action_area = Rect {
                y: area.y.saturating_add(diff_height),
                height: 1,
                ..area
            };
            frame.render_widget(
                Paragraph::new(edit_decision_action_line(
                    self.decision,
                    self.focused_action,
                    self.has_focusable_action() && ctx.is_focused,
                    ctx,
                )),
                action_area,
            );
        }
    }
}

impl ::atto_ui::composable::DragAndDrop for DiffDecisionView {}
impl ::atto_ui::composable::Scrollable for DiffDecisionView {
    fn is_scrollable(&self) -> bool {
        true
    }

    fn content_size(&self) -> (u16, u16) {
        (self.display_width(), self.display_height())
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.viewport
    }

    fn scroll_offset(&self) -> (u16, u16) {
        (self.scroll_x, 0)
    }

    fn set_scroll_offset(&mut self, x: u16, _y: u16) {
        self.scroll_x = x;
        self.clamp_scroll();
    }
}
impl ::atto_ui::composable::FocusNav for DiffDecisionView {
    fn is_focusable(&self) -> bool {
        self.has_focusable_action()
    }

    fn focus_first(&mut self) -> bool {
        if !self.has_focusable_action() {
            return false;
        }
        self.focused_action = 0;
        true
    }

    fn focus_last(&mut self) -> bool {
        if !self.has_focusable_action() {
            return false;
        }
        self.focused_action = 1;
        true
    }
}
impl ::atto_ui::composable::DynamicTree for DiffDecisionView {}
impl ::atto_ui::composable::EventHandling for DiffDecisionView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if let Some(decision) = self.click_decision(event, ctx) {
            return self.emit_decision(decision);
        }

        if matches!(
            event,
            Event::Key(KeyEvent {
                code: KeyCode::Enter | KeyCode::Char(' '),
                kind: KeyEventKind::Press,
                ..
            })
        ) && self.has_focusable_action()
        {
            return self.emit_decision(self.focused_decision());
        }

        horizontal_scroll_event(event, self.viewport.0).map_or_else(EventResult::ignored, |delta| {
            self.scroll_horizontally(delta)
        })
    }
}

impl ::atto_ui::composable::Layout for DiffDecisionView {
    fn min_width(&self) -> u16 {
        1
    }

    fn min_height(&self) -> u16 {
        1
    }

    fn desired_width(&self) -> Option<u16> {
        Some(self.display_width())
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.display_height())
    }
}

enum ChatMessageBody {
    Markdown(ResponsiveMarkdownView),
    Text(Text),
    Disclosure(Disclosure),
    Diff(DiffDecisionView),
    Todo(TodoListView),
    Artifact(ArtifactLink),
}

impl ChatMessageBody {
    fn from_block(
        message_id: ChatMessageId,
        block: Option<&ChatBlock>,
        config: &ChatMessageRowConfig,
    ) -> (Self, ChatMessageRowBindings) {
        match block {
            Some(ChatBlock::Text(_)) => {
                let content = Binding::new(
                    block
                        .and_then(|block| block_markdown_for_render(block, config))
                        .unwrap_or_default(),
                );
                (
                    ChatMessageBody::Markdown(
                        ResponsiveMarkdownView::new(content.clone(), config, 0).map_view(|view| {
                            view.streaming_tolerant(true)
                                .vertical_scrollbar(ScrollbarVisibility::Never)
                        }),
                    ),
                    ChatMessageRowBindings {
                        markdown: Some(content),
                        ..ChatMessageRowBindings::default()
                    },
                )
            }
            Some(ChatBlock::Thinking(thinking)) => {
                let content = Binding::new(markdown_for_render(&thinking.markdown, false, config));
                let status = Binding::new(thinking_status_to_disclosure(thinking));
                let viewer =
                    ResponsiveMarkdownView::new(content.clone(), config, 2).map_view(|view| {
                        view.streaming_tolerant(true)
                            .text_color(Color::DarkGray)
                            .vertical_scrollbar(ScrollbarVisibility::Never)
                    });
                (
                    ChatMessageBody::Disclosure(
                        Disclosure::new("Thinking")
                            .expanded(!thinking.collapsed)
                            .status(status.clone())
                            .child(viewer),
                    ),
                    ChatMessageRowBindings {
                        markdown: Some(content),
                        disclosure_status: Some(status),
                        ..ChatMessageRowBindings::default()
                    },
                )
            }
            Some(ChatBlock::Attachment(AttachmentBlock { name, url, .. })) => (
                ChatMessageBody::Text(Text::new(attachment_label(name, url.as_deref()))),
                ChatMessageRowBindings::default(),
            ),
            Some(ChatBlock::ToolUse(tool)) => {
                let details = Binding::new(ToolUseDetails::from(tool));
                let status = Binding::new(tool_status_to_disclosure(&tool.status));
                let view = Disclosure::new(tool.name.clone())
                    .expanded(!tool.collapsed)
                    .status(status.clone())
                    .child(ToolUseDetailsView::new(
                        details.clone(),
                        message_id,
                        tool.id,
                        config.on_approve.clone(),
                    ));
                (
                    ChatMessageBody::Disclosure(view),
                    ChatMessageRowBindings {
                        tool_use: Some(details),
                        disclosure_status: Some(status),
                        ..ChatMessageRowBindings::default()
                    },
                )
            }
            Some(ChatBlock::ToolResult(result)) => {
                let output = Binding::new(result.output.as_text().to_string());
                let status = Binding::new(tool_result_to_disclosure(result));
                let view = Disclosure::new(tool_result_title(result))
                    .expanded(!result.collapsed)
                    .status(status.clone())
                    .boxed_child(tool_output_component(
                        &result.output,
                        output.clone(),
                        config,
                    ));
                (
                    ChatMessageBody::Disclosure(view),
                    ChatMessageRowBindings {
                        tool_output: Some(output),
                        disclosure_status: Some(status),
                        ..ChatMessageRowBindings::default()
                    },
                )
            }
            Some(ChatBlock::Diff(diff)) => {
                let content = Binding::new(diff.diff.unified.clone());
                (
                    ChatMessageBody::Diff(DiffDecisionView::new(
                        Some(diff_block_title(diff)),
                        content.clone(),
                        message_id,
                        diff.id,
                        diff.decision,
                        config.on_edit_decision.clone(),
                    )),
                    ChatMessageRowBindings {
                        diff: Some(content),
                        ..ChatMessageRowBindings::default()
                    },
                )
            }
            Some(ChatBlock::Todo(TodoBlock { items, .. })) => {
                let items = Binding::new(items.clone());
                (
                    ChatMessageBody::Todo(TodoListView::new(items.clone())),
                    ChatMessageRowBindings {
                        todo_items: Some(items),
                        ..ChatMessageRowBindings::default()
                    },
                )
            }
            Some(ChatBlock::Notice(NoticeBlock { level, text, .. })) => (
                ChatMessageBody::Text(
                    Text::new(notice_label(*level, text)).fg(notice_color(*level)),
                ),
                ChatMessageRowBindings::default(),
            ),
            Some(ChatBlock::Artifact(ArtifactBlock {
                kind,
                anchor,
                title,
                ..
            })) => (
                ChatMessageBody::Artifact(ArtifactLink::new(
                    kind.clone(),
                    anchor.clone(),
                    title.clone(),
                    config.on_open_artifact.clone(),
                )),
                ChatMessageRowBindings::default(),
            ),
            None => {
                let content = Binding::new(String::new());
                (
                    ChatMessageBody::Markdown(
                        ResponsiveMarkdownView::new(content.clone(), config, 0).map_view(|view| {
                            view.streaming_tolerant(true)
                                .vertical_scrollbar(ScrollbarVisibility::Never)
                        }),
                    ),
                    ChatMessageRowBindings {
                        markdown: Some(content),
                        ..ChatMessageRowBindings::default()
                    },
                )
            }
        }
    }
}

fn todo_state_marker(state: TodoState) -> &'static str {
    match state {
        TodoState::Pending => "[ ]",
        TodoState::InProgress => "[~]",
        TodoState::Done => "[x]",
    }
}

fn attachment_label(name: &str, url: Option<&str>) -> String {
    match url.filter(|url| !url.is_empty()) {
        Some(url) => format!("File: {name} ({url})"),
        None => format!("File: {name}"),
    }
}

fn notice_label(level: NoticeLevel, text: &str) -> String {
    format!("{}: {text}", notice_level_label(level))
}

fn notice_level_label(level: NoticeLevel) -> &'static str {
    match level {
        NoticeLevel::Info => "Info",
        NoticeLevel::Warning => "Warning",
        NoticeLevel::Error => "Error",
    }
}

fn notice_color(level: NoticeLevel) -> Color {
    match level {
        NoticeLevel::Info => Color::Cyan,
        NoticeLevel::Warning => Color::Yellow,
        NoticeLevel::Error => Color::Red,
    }
}

fn thinking_status_to_disclosure(thinking: &ThinkingBlock) -> DisclosureStatus {
    if thinking.streaming {
        DisclosureStatus::Running
    } else {
        DisclosureStatus::Idle
    }
}

fn tool_status_to_disclosure(status: &ToolStatus) -> DisclosureStatus {
    match status {
        ToolStatus::Pending => DisclosureStatus::Idle,
        ToolStatus::Running => DisclosureStatus::Running,
        ToolStatus::Done => DisclosureStatus::Done,
        ToolStatus::Error => DisclosureStatus::Error,
        ToolStatus::Canceled => DisclosureStatus::Canceled,
    }
}

fn tool_result_to_disclosure(result: &ToolResultBlock) -> DisclosureStatus {
    if result.ok {
        DisclosureStatus::Done
    } else {
        DisclosureStatus::Error
    }
}

fn tool_result_title(result: &ToolResultBlock) -> String {
    match result.exit_code {
        Some(code) => format!("Tool result: {} (exit {code})", result.call_id),
        None => format!("Tool result: {}", result.call_id),
    }
}

fn pending_tool_result_title(call_id: &str) -> String {
    format!("Tool result: {call_id} (等待中)")
}

fn diff_block_title(diff: &DiffBlock) -> String {
    format!(
        "Diff: {} ({})",
        diff.path,
        edit_decision_label(diff.decision)
    )
}

fn edit_decision_label(decision: EditDecision) -> &'static str {
    match decision {
        EditDecision::Pending => "pending",
        EditDecision::Accepted => "accepted",
        EditDecision::Rejected => "rejected",
    }
}

fn edit_decision_action_line_width(decision: EditDecision) -> usize {
    match decision {
        EditDecision::Pending => EDIT_ACCEPT_LABEL
            .width()
            .saturating_add(1)
            .saturating_add(EDIT_REJECT_LABEL.width()),
        EditDecision::Accepted => EDIT_ACCEPTED_LABEL.width(),
        EditDecision::Rejected => EDIT_REJECTED_LABEL.width(),
    }
}

const EDIT_ACCEPT_LABEL: &str = "[ Accept ]";
const EDIT_REJECT_LABEL: &str = "[ Reject ]";
const EDIT_ACCEPTED_LABEL: &str = "[x] Accepted";
const EDIT_REJECTED_LABEL: &str = "[x] Rejected";

fn edit_decision_action_at_column(column: u16) -> Option<EditDecision> {
    let column = column as usize;
    let accept_width = EDIT_ACCEPT_LABEL.width();
    if column < accept_width {
        return Some(EditDecision::Accepted);
    }
    let reject_start = accept_width.saturating_add(1);
    let reject_end = reject_start.saturating_add(EDIT_REJECT_LABEL.width());
    (column >= reject_start && column < reject_end).then_some(EditDecision::Rejected)
}

fn edit_decision_action_line(
    decision: EditDecision,
    focused_action: usize,
    focused: bool,
    ctx: ComponentContext<'_>,
) -> Line<'static> {
    match decision {
        EditDecision::Pending => {
            let base = ctx.theme.widget.accent;
            let focused_style = base.add_modifier(Modifier::REVERSED);
            Line::from(vec![
                Span::styled(
                    EDIT_ACCEPT_LABEL,
                    if focused && focused_action == 0 {
                        focused_style
                    } else {
                        base
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    EDIT_REJECT_LABEL,
                    if focused && focused_action == 1 {
                        focused_style
                    } else {
                        base
                    },
                ),
            ])
        }
        EditDecision::Accepted => Line::styled(EDIT_ACCEPTED_LABEL, ctx.theme.widget.dim),
        EditDecision::Rejected => Line::styled(EDIT_REJECTED_LABEL, ctx.theme.widget.dim),
    }
}

fn tool_output_component(
    output: &ToolOutput,
    content: Binding<String>,
    config: &ChatMessageRowConfig,
) -> Box<dyn Component> {
    match output {
        ToolOutput::Ansi(_) => Box::new(AnsiOutputView::new(content)),
        ToolOutput::Markdown(_) => Box::new(
            ResponsiveMarkdownView::new(content, config, 2).map_view(|view| {
                view.streaming_tolerant(true)
                    .vertical_scrollbar(ScrollbarVisibility::Never)
            }),
        ),
        ToolOutput::Diff(_) => Box::new(DiffView::new(None, content)),
    }
}

fn tool_input_detail_lines(input: &ToolInput) -> Vec<String> {
    match input {
        ToolInput::Text(text) => tool_text_input_lines(text),
        ToolInput::Json(value) => tool_json_input_lines(value),
    }
}

fn tool_text_input_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return vec!["Input: (empty)".to_string()];
    }
    if !text.contains('\n') {
        return vec![format!("Input: {text}")];
    }

    let mut lines = vec!["Input:".to_string()];
    lines.extend(text.lines().map(|line| format!("  {line}")));
    lines
}

fn tool_json_input_lines(value: &ComponentValue) -> Vec<String> {
    match value {
        ComponentValue::Map(map) if map.is_empty() => vec!["Input: {}".to_string()],
        ComponentValue::Map(_) => {
            let mut lines = vec!["Input:".to_string()];
            lines.extend(component_value_lines(value));
            lines
        }
        other => vec![format!("Input: {}", component_value_compact(other))],
    }
}

fn component_value_lines(value: &ComponentValue) -> Vec<String> {
    match value {
        ComponentValue::Map(map) if map.is_empty() => vec!["{}".to_string()],
        ComponentValue::Map(map) => map
            .iter()
            .map(|(key, value)| format!("{key}: {}", component_value_compact(value)))
            .collect(),
        other => vec![component_value_compact(other)],
    }
}

fn component_value_compact(value: &ComponentValue) -> String {
    match value {
        ComponentValue::Null => "null".to_string(),
        ComponentValue::Bool(value) => value.to_string(),
        ComponentValue::I64(value) => value.to_string(),
        ComponentValue::U64(value) => value.to_string(),
        ComponentValue::F64(value) => value.to_string(),
        ComponentValue::String(value) => format!("{value:?}"),
        ComponentValue::StringList(values) => format!(
            "[{}]",
            values
                .iter()
                .map(|value| format!("{value:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ComponentValue::Table(rows) => format!("{} row table", rows.len()),
        ComponentValue::Rect(rect) => format!(
            "rect({}, {}, {}, {})",
            rect.x, rect.y, rect.width, rect.height
        ),
        ComponentValue::Bytes(bytes) => format!("{} bytes", bytes.len()),
        ComponentValue::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(component_value_compact)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ComponentValue::Map(map) => format!(
            "{{{}}}",
            map.iter()
                .map(|(key, value)| format!("{key}: {}", component_value_compact(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

struct DiffView {
    title: Option<String>,
    diff: Binding<String>,
    scroll_x: u16,
    viewport: (u16, u16),
}

impl DiffView {
    fn new(title: Option<String>, diff: impl Into<Binding<String>>) -> Self {
        Self {
            title,
            diff: diff.into(),
            scroll_x: 0,
            viewport: (0, 0),
        }
    }

    fn display_lines(&self, base: Style, title_style: Style) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if let Some(title) = &self.title {
            lines.push(Line::styled(title.clone(), title_style));
        }
        let diff = self.diff.get();
        lines.extend(diff_display_lines(&diff, base));
        lines
    }

    fn display_height(&self) -> u16 {
        let title = u16::from(self.title.is_some());
        title.saturating_add(line_count(&self.diff.get()))
    }

    fn display_width(&self) -> u16 {
        let diff_width = self
            .diff
            .get()
            .lines()
            .map(UnicodeWidthStr::width)
            .max()
            .unwrap_or(0);
        let title_width = self
            .title
            .as_deref()
            .map(UnicodeWidthStr::width)
            .unwrap_or(0);
        diff_width.max(title_width).max(1).min(u16::MAX as usize) as u16
    }

    fn clamp_scroll(&mut self) {
        let max_x = self.display_width().saturating_sub(self.viewport.0);
        self.scroll_x = self.scroll_x.min(max_x);
    }

    fn scroll_horizontally(&mut self, delta: i16) -> EventResult {
        let before = self.scroll_x;
        self.set_scroll_offset(add_signed_u16(self.scroll_x, delta), 0);
        if self.scroll_x == before {
            EventResult::ignored()
        } else {
            EventResult::changed()
        }
    }
}

impl Component for DiffView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.viewport = (area.width, area.height);
        self.clamp_scroll();
        if area.width == 0 || area.height == 0 {
            return;
        }
        frame.render_widget(
            Paragraph::new(self.display_lines(ctx.theme.widget.normal, ctx.theme.widget.dim))
                .scroll((0, self.scroll_x)),
            area,
        );
    }
}

impl ::atto_ui::composable::DragAndDrop for DiffView {}
impl ::atto_ui::composable::Scrollable for DiffView {
    fn is_scrollable(&self) -> bool {
        true
    }

    fn content_size(&self) -> (u16, u16) {
        (self.display_width(), self.display_height())
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.viewport
    }

    fn scroll_offset(&self) -> (u16, u16) {
        (self.scroll_x, 0)
    }

    fn set_scroll_offset(&mut self, x: u16, _y: u16) {
        self.scroll_x = x;
        self.clamp_scroll();
    }
}
impl ::atto_ui::composable::FocusNav for DiffView {}
impl ::atto_ui::composable::DynamicTree for DiffView {}
impl ::atto_ui::composable::EventHandling for DiffView {
    fn handle_event(&mut self, event: &Event, _ctx: ComponentContext<'_>) -> EventResult {
        horizontal_scroll_event(event, self.viewport.0).map_or_else(EventResult::ignored, |delta| {
            self.scroll_horizontally(delta)
        })
    }
}

impl ::atto_ui::composable::Layout for DiffView {
    fn min_width(&self) -> u16 {
        1
    }

    fn min_height(&self) -> u16 {
        1
    }

    fn desired_width(&self) -> Option<u16> {
        Some(self.display_width())
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.display_height())
    }
}

struct TodoListView {
    items: Binding<Vec<TodoItem>>,
}

impl TodoListView {
    fn new(items: impl Into<Binding<Vec<TodoItem>>>) -> Self {
        Self {
            items: items.into(),
        }
    }

    fn display_lines(&self) -> Vec<String> {
        self.items.with(|items| todo_display_lines(items))
    }
}

fn todo_display_lines(items: &[TodoItem]) -> Vec<String> {
    if items.is_empty() {
        return vec!["(no todo items)".to_string()];
    }
    items.iter().map(todo_display_line).collect()
}

fn todo_display_line(item: &TodoItem) -> String {
    format!("{} {}", todo_state_marker(item.state), item.text)
}

impl Component for TodoListView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let lines = self.items.with(|items| {
            if items.is_empty() {
                return vec![Line::styled("(no todo items)", ctx.theme.widget.dim)];
            }
            items
                .iter()
                .map(|item| {
                    Line::styled(todo_display_line(item), todo_state_style(item.state, ctx))
                })
                .collect()
        });
        frame.render_widget(Paragraph::new(lines), area);
    }
}

impl ::atto_ui::composable::DragAndDrop for TodoListView {}
impl ::atto_ui::composable::Scrollable for TodoListView {}
impl ::atto_ui::composable::FocusNav for TodoListView {}
impl ::atto_ui::composable::DynamicTree for TodoListView {}
impl ::atto_ui::composable::EventHandling for TodoListView {}

impl ::atto_ui::composable::Layout for TodoListView {
    fn min_width(&self) -> u16 {
        1
    }

    fn min_height(&self) -> u16 {
        1
    }

    fn desired_width(&self) -> Option<u16> {
        Some(max_line_width(&self.display_lines()))
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.display_lines().len().min(u16::MAX as usize) as u16)
    }
}

struct AnsiOutputView {
    text: Binding<String>,
    scroll_x: u16,
    viewport: (u16, u16),
    show_all: bool,
    last_area: Option<Rect>,
}

impl AnsiOutputView {
    fn new(text: impl Into<Binding<String>>) -> Self {
        Self {
            text: text.into(),
            scroll_x: 0,
            viewport: (0, 0),
            show_all: false,
            last_area: None,
        }
    }

    fn lines(&self, base: Style, action: Style) -> Vec<Line<'static>> {
        let lines = ansi_sgr_lines(&self.text.get(), base);
        if self.show_all || lines.len() <= ANSI_OUTPUT_TAIL_LINES {
            return lines;
        }

        let hidden = lines.len().saturating_sub(ANSI_OUTPUT_TAIL_LINES);
        let mut visible = Vec::with_capacity(ANSI_OUTPUT_TAIL_LINES.saturating_add(1));
        visible.push(Line::styled(
            format!("已隐藏 {hidden} 行，{ANSI_OUTPUT_EXPAND_LABEL}"),
            action,
        ));
        visible.extend(lines.into_iter().skip(hidden));
        visible
    }

    fn is_tail_collapsed(&self) -> bool {
        !self.show_all
            && ansi_sgr_lines(&self.text.get(), Style::default()).len() > ANSI_OUTPUT_TAIL_LINES
    }

    fn expand_all(&mut self) -> EventResult {
        if !self.is_tail_collapsed() {
            return EventResult::ignored();
        }
        self.show_all = true;
        EventResult::changed()
    }

    fn display_width(&self) -> u16 {
        self.lines(Style::default(), Style::default())
            .iter()
            .map(Line::width)
            .max()
            .unwrap_or(1)
            .max(1)
            .min(u16::MAX as usize) as u16
    }

    fn display_height(&self) -> u16 {
        self.lines(Style::default(), Style::default())
            .len()
            .max(1)
            .min(u16::MAX as usize) as u16
    }

    fn clamp_scroll(&mut self) {
        let max_x = self.display_width().saturating_sub(self.viewport.0);
        self.scroll_x = self.scroll_x.min(max_x);
    }

    fn scroll_horizontally(&mut self, delta: i16) -> EventResult {
        let before = self.scroll_x;
        self.set_scroll_offset(add_signed_u16(self.scroll_x, delta), 0);
        if self.scroll_x == before {
            EventResult::ignored()
        } else {
            EventResult::changed()
        }
    }
}

impl Component for AnsiOutputView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        self.viewport = (area.width, area.height);
        self.clamp_scroll();
        if area.width == 0 || area.height == 0 {
            return;
        }
        frame.render_widget(
            Paragraph::new(self.lines(ctx.theme.widget.normal, ctx.theme.widget.accent))
                .scroll((0, self.scroll_x)),
            area,
        );
    }
}

impl ::atto_ui::composable::DragAndDrop for AnsiOutputView {}
impl ::atto_ui::composable::Scrollable for AnsiOutputView {
    fn is_scrollable(&self) -> bool {
        true
    }

    fn content_size(&self) -> (u16, u16) {
        (self.display_width(), self.display_height())
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.viewport
    }

    fn scroll_offset(&self) -> (u16, u16) {
        (self.scroll_x, 0)
    }

    fn set_scroll_offset(&mut self, x: u16, _y: u16) {
        self.scroll_x = x;
        self.clamp_scroll();
    }
}
impl ::atto_ui::composable::FocusNav for AnsiOutputView {
    fn is_focusable(&self) -> bool {
        self.is_tail_collapsed() || self.display_width() > self.viewport.0.max(1)
    }
}
impl ::atto_ui::composable::DynamicTree for AnsiOutputView {}
impl ::atto_ui::composable::EventHandling for AnsiOutputView {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        match event {
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                let Some(area) = self.last_area else {
                    return EventResult::ignored();
                };
                if mouse_row_in_area(area, mouse, ctx.mouse_coordinate_space) == Some(0) {
                    return self.expand_all();
                }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Enter | KeyCode::Char(' '),
                kind: KeyEventKind::Press,
                ..
            }) => {
                let expanded = self.expand_all();
                if expanded.is_consumed() {
                    return expanded;
                }
            }
            _ => {}
        }
        horizontal_scroll_event(event, self.viewport.0).map_or_else(EventResult::ignored, |delta| {
            self.scroll_horizontally(delta)
        })
    }
}

impl ::atto_ui::composable::Layout for AnsiOutputView {
    fn min_width(&self) -> u16 {
        1
    }

    fn min_height(&self) -> u16 {
        1
    }

    fn desired_width(&self) -> Option<u16> {
        Some(self.display_width())
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.display_height())
    }
}

fn todo_state_style(state: TodoState, ctx: ComponentContext<'_>) -> Style {
    match state {
        TodoState::Pending => ctx.theme.widget.normal,
        TodoState::InProgress => ctx.theme.widget.accent,
        TodoState::Done => ctx.theme.widget.dim,
    }
}

fn horizontal_scroll_event(event: &Event, viewport_width: u16) -> Option<i16> {
    let page = viewport_width.max(1).min(i16::MAX as u16) as i16;
    match event {
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollLeft => Some(-3),
            MouseEventKind::ScrollRight => Some(3),
            _ => None,
        },
        Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) => match code {
            KeyCode::Left => Some(-1),
            KeyCode::Right => Some(1),
            KeyCode::PageUp => Some(-page),
            KeyCode::PageDown => Some(page),
            _ => None,
        },
        _ => None,
    }
}

fn add_signed_u16(value: u16, delta: i16) -> u16 {
    if delta.is_negative() {
        value.saturating_sub(delta.wrapping_abs() as u16)
    } else {
        value.saturating_add(delta as u16)
    }
}

fn diff_display_lines(diff: &str, base: Style) -> Vec<Line<'static>> {
    if diff.is_empty() {
        return vec![Line::styled(String::new(), base)];
    }
    diff.lines()
        .map(|line| Line::styled(line.to_string(), diff_line_style(line, base)))
        .collect()
}

fn line_count(text: &str) -> u16 {
    text.lines().count().max(1).min(u16::MAX as usize) as u16
}

fn max_line_width(lines: &[String]) -> u16 {
    lines
        .iter()
        .map(|line| line.width())
        .max()
        .unwrap_or(1)
        .max(1)
        .min(u16::MAX as usize) as u16
}

fn ansi_sgr_lines(input: &str, base: Style) -> Vec<Line<'static>> {
    let mut lines = vec![Vec::<Span<'static>>::new()];
    let mut text = String::new();
    let mut style = base;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && matches!(chars.peek(), Some('[')) {
            chars.next();
            let mut sequence = String::new();
            let mut terminator = None;
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    terminator = Some(next);
                    break;
                }
                sequence.push(next);
            }
            if terminator == Some('m') {
                push_ansi_span(&mut lines, &mut text, style);
                apply_sgr_sequence(&sequence, &mut style, base);
            }
            continue;
        }

        match ch {
            '\n' => {
                push_ansi_span(&mut lines, &mut text, style);
                lines.push(Vec::new());
            }
            '\r' => {}
            other => text.push(other),
        }
    }

    push_ansi_span(&mut lines, &mut text, style);
    lines
        .into_iter()
        .map(|spans| {
            if spans.is_empty() {
                Line::from(String::new())
            } else {
                Line::from(spans)
            }
        })
        .collect()
}

fn push_ansi_span(lines: &mut [Vec<Span<'static>>], text: &mut String, style: Style) {
    if text.is_empty() {
        return;
    }
    let Some(line) = lines.last_mut() else {
        return;
    };
    line.push(Span::styled(std::mem::take(text), style));
}

fn apply_sgr_sequence(sequence: &str, style: &mut Style, base: Style) {
    let params = if sequence.trim().is_empty() {
        vec![0]
    } else {
        sequence
            .split(';')
            .map(|part| part.parse::<u16>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    let mut index = 0;
    while index < params.len() {
        match params[index] {
            0 => *style = base,
            1 => *style = style.add_modifier(Modifier::BOLD),
            2 => *style = style.add_modifier(Modifier::DIM),
            3 => *style = style.add_modifier(Modifier::ITALIC),
            4 => *style = style.add_modifier(Modifier::UNDERLINED),
            9 => *style = style.add_modifier(Modifier::CROSSED_OUT),
            22 => *style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            23 => *style = style.remove_modifier(Modifier::ITALIC),
            24 => *style = style.remove_modifier(Modifier::UNDERLINED),
            29 => *style = style.remove_modifier(Modifier::CROSSED_OUT),
            30..=37 => style.fg = Some(ansi_color((params[index] - 30) as u8, false)),
            39 => style.fg = base.fg,
            40..=47 => style.bg = Some(ansi_color((params[index] - 40) as u8, false)),
            49 => style.bg = base.bg,
            90..=97 => style.fg = Some(ansi_color((params[index] - 90) as u8, true)),
            100..=107 => style.bg = Some(ansi_color((params[index] - 100) as u8, true)),
            38 => apply_extended_ansi_color(&params, &mut index, style, true),
            48 => apply_extended_ansi_color(&params, &mut index, style, false),
            _ => {}
        }
        index += 1;
    }
}

fn apply_extended_ansi_color(
    params: &[u16],
    index: &mut usize,
    style: &mut Style,
    foreground: bool,
) {
    let Some(mode) = params.get((*index).saturating_add(1)).copied() else {
        return;
    };
    let color = match mode {
        5 => {
            let Some(value) = params.get((*index).saturating_add(2)).copied() else {
                return;
            };
            *index = (*index).saturating_add(2);
            Color::Indexed(value.min(u8::MAX as u16) as u8)
        }
        2 => {
            let (Some(r), Some(g), Some(b)) = (
                params.get((*index).saturating_add(2)).copied(),
                params.get((*index).saturating_add(3)).copied(),
                params.get((*index).saturating_add(4)).copied(),
            ) else {
                return;
            };
            *index = (*index).saturating_add(4);
            Color::Rgb(
                r.min(u8::MAX as u16) as u8,
                g.min(u8::MAX as u16) as u8,
                b.min(u8::MAX as u16) as u8,
            )
        }
        _ => return,
    };
    if foreground {
        style.fg = Some(color);
    } else {
        style.bg = Some(color);
    }
}

fn ansi_color(index: u8, bright: bool) -> Color {
    match (index, bright) {
        (0, false) => Color::Black,
        (1, false) => Color::Red,
        (2, false) => Color::Green,
        (3, false) => Color::Yellow,
        (4, false) => Color::Blue,
        (5, false) => Color::Magenta,
        (6, false) => Color::Cyan,
        (7, false) => Color::Gray,
        (0, true) => Color::DarkGray,
        (1, true) => Color::LightRed,
        (2, true) => Color::LightGreen,
        (3, true) => Color::LightYellow,
        (4, true) => Color::LightBlue,
        (5, true) => Color::LightMagenta,
        (6, true) => Color::LightCyan,
        (7, true) => Color::White,
        _ => Color::Reset,
    }
}

impl ::atto_ui::composable::Component for ChatMessageBody {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        match self {
            ChatMessageBody::Markdown(view) => view.draw(frame, area, ctx),
            ChatMessageBody::Text(view) => view.draw(frame, area, ctx),
            ChatMessageBody::Disclosure(view) => view.draw(frame, area, ctx),
            ChatMessageBody::Diff(view) => view.draw(frame, area, ctx),
            ChatMessageBody::Todo(view) => view.draw(frame, area, ctx),
            ChatMessageBody::Artifact(view) => view.draw(frame, area, ctx),
        }
    }
}

impl ::atto_ui::composable::DragAndDrop for ChatMessageBody {}

impl ::atto_ui::composable::Layout for ChatMessageBody {
    fn min_width(&self) -> u16 {
        match self {
            ChatMessageBody::Markdown(view) => view.min_width(),
            ChatMessageBody::Text(view) => view.min_width(),
            ChatMessageBody::Disclosure(view) => view.min_width(),
            ChatMessageBody::Diff(view) => view.min_width(),
            ChatMessageBody::Todo(view) => view.min_width(),
            ChatMessageBody::Artifact(view) => view.min_width(),
        }
    }

    fn min_height(&self) -> u16 {
        match self {
            ChatMessageBody::Markdown(view) => view.min_height(),
            ChatMessageBody::Text(view) => view.min_height(),
            ChatMessageBody::Disclosure(view) => view.min_height(),
            ChatMessageBody::Diff(view) => view.min_height(),
            ChatMessageBody::Todo(view) => view.min_height(),
            ChatMessageBody::Artifact(view) => view.min_height(),
        }
    }

    fn desired_width(&self) -> Option<u16> {
        match self {
            ChatMessageBody::Markdown(view) => view.desired_width(),
            ChatMessageBody::Text(view) => view.desired_width(),
            ChatMessageBody::Disclosure(view) => view.desired_width(),
            ChatMessageBody::Diff(view) => view.desired_width(),
            ChatMessageBody::Todo(view) => view.desired_width(),
            ChatMessageBody::Artifact(view) => view.desired_width(),
        }
    }

    fn desired_height(&self) -> Option<u16> {
        match self {
            ChatMessageBody::Markdown(view) => view.desired_height(),
            ChatMessageBody::Text(view) => view.desired_height(),
            ChatMessageBody::Disclosure(view) => view.desired_height(),
            ChatMessageBody::Diff(view) => view.desired_height(),
            ChatMessageBody::Todo(view) => view.desired_height(),
            ChatMessageBody::Artifact(view) => view.desired_height(),
        }
    }
}

impl ::atto_ui::composable::Scrollable for ChatMessageBody {}

impl ::atto_ui::composable::FocusNav for ChatMessageBody {}

impl ::atto_ui::composable::DynamicTree for ChatMessageBody {}

impl ::atto_ui::composable::EventHandling for ChatMessageBody {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        match self {
            ChatMessageBody::Markdown(view) => view.handle_event(event, ctx),
            ChatMessageBody::Text(view) => view.handle_event(event, ctx),
            ChatMessageBody::Disclosure(view) => view.handle_event(event, ctx),
            ChatMessageBody::Diff(view) => view.handle_event(event, ctx),
            ChatMessageBody::Todo(view) => view.handle_event(event, ctx),
            ChatMessageBody::Artifact(view) => view.handle_event(event, ctx),
        }
    }
}

struct ArtifactLink {
    kind: ArtifactKind,
    anchor: ArtifactId,
    title: String,
    on_open: Option<ArtifactOpenCallback>,
    last_area: Option<Rect>,
}

impl ArtifactLink {
    fn new(
        kind: ArtifactKind,
        anchor: ArtifactId,
        title: String,
        on_open: Option<ArtifactOpenCallback>,
    ) -> Self {
        Self {
            kind,
            anchor,
            title,
            on_open,
            last_area: None,
        }
    }

    fn label(&self) -> String {
        format!("Artifact {}: {}", self.kind.label(), self.title)
    }

    fn open(&self) -> EventResult {
        let Some(on_open) = &self.on_open else {
            return EventResult::ignored();
        };
        on_open(self.anchor.clone());
        EventResult::submitted()
    }
}

impl ::atto_ui::composable::Component for ArtifactLink {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let style = ctx.theme.widget.focused.add_modifier(Modifier::UNDERLINED);
        frame.render_widget(Paragraph::new(Line::styled(self.label(), style)), area);
    }
}

impl ::atto_ui::composable::DragAndDrop for ArtifactLink {}

impl ::atto_ui::composable::Layout for ArtifactLink {
    fn min_width(&self) -> u16 {
        1
    }

    fn min_height(&self) -> u16 {
        1
    }

    fn desired_width(&self) -> Option<u16> {
        Some(self.label().width().min(u16::MAX as usize) as u16)
    }

    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }
}

impl ::atto_ui::composable::FocusNav for ArtifactLink {
    fn is_focusable(&self) -> bool {
        self.on_open.is_some()
    }
}

impl ::atto_ui::composable::Scrollable for ArtifactLink {}
impl ::atto_ui::composable::DynamicTree for ArtifactLink {}

impl ::atto_ui::composable::EventHandling for ArtifactLink {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        match event {
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                let Some(area) = self.last_area else {
                    return EventResult::ignored();
                };
                if mouse_in_area(area, mouse, ctx.mouse_coordinate_space) {
                    self.open()
                } else {
                    EventResult::ignored()
                }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Enter | KeyCode::Char(' '),
                ..
            }) => self.open(),
            _ => EventResult::ignored(),
        }
    }
}

fn mouse_in_area(area: Rect, mouse: &MouseEvent, coordinate_space: MouseCoordinateSpace) -> bool {
    mouse_row_in_area(area, mouse, coordinate_space).is_some()
}

fn mouse_position_in_area(
    area: Rect,
    mouse: &MouseEvent,
    coordinate_space: MouseCoordinateSpace,
) -> Option<(u16, u16)> {
    match coordinate_space {
        MouseCoordinateSpace::Absolute => (mouse.column >= area.x
            && mouse.column < area.x.saturating_add(area.width)
            && mouse.row >= area.y
            && mouse.row < area.y.saturating_add(area.height))
        .then(|| {
            (
                mouse.column.saturating_sub(area.x),
                mouse.row.saturating_sub(area.y),
            )
        }),
        MouseCoordinateSpace::Local => (mouse.column < area.width && mouse.row < area.height)
            .then_some((mouse.column, mouse.row)),
    }
}

fn mouse_row_in_area(
    area: Rect,
    mouse: &MouseEvent,
    coordinate_space: MouseCoordinateSpace,
) -> Option<u16> {
    mouse_position_in_area(area, mouse, coordinate_space).map(|(_, row)| row)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use atto_ui::composable::{
        EventHandling, FocusNav, MouseCoordinateSpace, ScrollbarHost, TabMode,
    };
    use atto_ui::theme::Theme;
    use atto_ui::wm::WindowId;
    use crossterm::event::KeyModifiers;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::message::{ChatMessageId, ChatRole};

    fn draw_chat_list(list: &mut ChatMessageList, width: u16, height: u16) {
        let theme = Theme::dark();
        let ctx = ComponentContext {
            theme: &theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        };
        let area = Rect::new(0, 0, width, height);
        let backend = TestBackend::new(width.max(1), height.max(1));
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| list.draw(f, area, ctx)).expect("draw");
    }

    fn draw_component_snapshot(
        component: &mut dyn Component,
        width: u16,
        height: u16,
    ) -> (Vec<String>, Vec<Vec<Color>>) {
        let theme = Theme::dark();
        let ctx = ComponentContext {
            theme: &theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        };
        let backend = TestBackend::new(width.max(1), height.max(1));
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|f| component.draw(f, Rect::new(0, 0, width, height), ctx))
            .expect("draw");
        let buf = terminal.backend().buffer();
        let mut lines = Vec::new();
        let mut colors = Vec::new();
        for y in 0..height {
            let mut line = String::new();
            let mut fgs = Vec::new();
            for x in 0..width {
                let cell = buf.cell((x, y)).expect("cell");
                line.push_str(cell.symbol());
                fgs.push(cell.fg);
            }
            lines.push(line);
            colors.push(fgs);
        }
        (lines, colors)
    }

    fn draw_component_line(component: &mut dyn Component, width: u16, height: u16) -> String {
        draw_component_snapshot(component, width, height)
            .0
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    fn component_context(theme: &Theme) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        }
    }

    fn line_text(line: &Line<'_>) -> String {
        let mut text = String::new();
        for span in &line.spans {
            text.push_str(span.content.as_ref());
        }
        text
    }

    fn store_with_text_messages(count: u64) -> ChatMessageStore {
        let store = ChatMessageStore::new();
        for idx in 0..count {
            store.push(ChatMessage::text(
                store.next_message_id(),
                ChatRole::Assistant,
                format!("MSG-{idx:02}"),
            ));
        }
        store
    }

    fn row_config_for_tests() -> ChatMessageRowConfig {
        ChatMessageRowConfig {
            wrap_width: None,
            responsive_wrap_width: Binding::new(None),
            in_progress_suffix: DEFAULT_IN_PROGRESS_SUFFIX.to_string(),
            show_timestamps: false,
            on_open_artifact: None,
            on_approve: None,
            on_edit_decision: None,
        }
    }

    #[test]
    fn chat_list_hides_timestamps_by_default_for_agent_view() {
        let store = ChatMessageStore::new();
        let list = ChatMessageList::new(store);

        assert_eq!(
            list.get_property("show_timestamps"),
            Some(ComponentValue::Bool(false))
        );
    }

    #[test]
    fn streaming_cursor_is_not_duplicated_for_running_thinking_blocks() {
        let config = row_config_for_tests();
        let text = ChatBlock::Text(TextBlock {
            id: ChatBlockId::new(1),
            markdown: "text".to_string(),
            streaming: true,
        });
        let thinking = ChatBlock::Thinking(ThinkingBlock {
            id: ChatBlockId::new(2),
            markdown: "thinking".to_string(),
            streaming: true,
            collapsed: false,
        });

        assert_eq!(
            block_markdown_for_render(&text, &config).as_deref(),
            Some("text ▍")
        );
        assert_eq!(
            block_markdown_for_render(&thinking, &config).as_deref(),
            Some("thinking")
        );
    }

    #[test]
    fn thinking_body_renders_collapsed_dim_disclosure_and_expands() {
        let config = row_config_for_tests();
        let block = ChatBlock::Thinking(ThinkingBlock {
            id: ChatBlockId::new(2),
            markdown: "THINKING-DETAIL".to_string(),
            streaming: true,
            collapsed: true,
        });
        let (mut body, bindings) =
            ChatMessageBody::from_block(ChatMessageId::new(1), Some(&block), &config);
        assert_eq!(
            bindings
                .disclosure_status
                .as_ref()
                .expect("thinking status binding")
                .get(),
            DisclosureStatus::Running
        );

        let (collapsed, _) = draw_component_snapshot(&mut body, 60, 3);
        assert!(collapsed[0].contains("Thinking"));
        assert!(
            collapsed
                .iter()
                .all(|line| !line.contains("THINKING-DETAIL"))
        );

        let theme = Theme::dark();
        assert_eq!(
            body.handle_event(
                &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                component_context(&theme),
            ),
            EventResult::changed()
        );
        let (expanded, colors) = draw_component_snapshot(&mut body, 60, 3);
        let detail_y = expanded
            .iter()
            .position(|line| line.contains("THINKING-DETAIL"))
            .expect("expanded thinking content should be visible");
        let detail_x = expanded[detail_y]
            .find("THINKING-DETAIL")
            .expect("thinking content x position");
        assert_eq!(colors[detail_y][detail_x], Color::DarkGray);
    }

    #[test]
    fn notice_body_renders_level_label_with_color() {
        let config = row_config_for_tests();
        let cases = [
            (NoticeLevel::Info, "Info: INFO-NOTICE", Color::Cyan),
            (
                NoticeLevel::Warning,
                "Warning: WARNING-NOTICE",
                Color::Yellow,
            ),
            (NoticeLevel::Error, "Error: ERROR-NOTICE", Color::Red),
        ];

        for (level, expected, color) in cases {
            let block = ChatBlock::Notice(NoticeBlock {
                id: ChatBlockId::new(3),
                level,
                text: expected
                    .split_once(": ")
                    .map(|(_, text)| text)
                    .expect("fixture label")
                    .to_string(),
            });
            let (mut body, _) =
                ChatMessageBody::from_block(ChatMessageId::new(1), Some(&block), &config);

            let (lines, colors) = draw_component_snapshot(&mut body, 40, 1);
            assert!(lines[0].starts_with(expected));
            assert_eq!(colors[0][0], color);
        }
    }

    #[test]
    fn todo_body_renders_state_markers_and_binding_updates() {
        let items = Binding::new(vec![
            TodoItem {
                text: "TODO-PENDING".to_string(),
                state: TodoState::Pending,
            },
            TodoItem {
                text: "TODO-RUNNING".to_string(),
                state: TodoState::InProgress,
            },
            TodoItem {
                text: "TODO-DONE".to_string(),
                state: TodoState::Done,
            },
        ]);
        let mut view = TodoListView::new(items.clone());

        let (initial, _) = draw_component_snapshot(&mut view, 40, 3);
        assert!(initial[0].starts_with("[ ] TODO-PENDING"));
        assert!(initial[1].starts_with("[~] TODO-RUNNING"));
        assert!(initial[2].starts_with("[x] TODO-DONE"));

        items.set(vec![TodoItem {
            text: "TODO-PENDING".to_string(),
            state: TodoState::Done,
        }]);
        let (updated, _) = draw_component_snapshot(&mut view, 40, 2);
        assert!(updated[0].starts_with("[x] TODO-PENDING"));
        assert!(!updated.iter().any(|line| line.contains("TODO-RUNNING")));
    }

    #[test]
    fn chat_list_syncs_todo_items_from_store_set_todo() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        let todo_id = ChatBlockId::new(70_001);
        store.push(ChatMessage::new(
            message_id,
            ChatRole::Assistant,
            vec![ChatBlock::Todo(TodoBlock {
                id: todo_id,
                items: vec![
                    TodoItem {
                        text: "TODO-ONE".to_string(),
                        state: TodoState::Pending,
                    },
                    TodoItem {
                        text: "TODO-TWO".to_string(),
                        state: TodoState::InProgress,
                    },
                ],
            })],
        ));
        let mut list = ChatMessageList::new(store.clone())
            .show_timestamps(false)
            .auto_scroll(false);

        let (initial, _) = draw_component_snapshot(&mut list, 80, 8);
        assert!(initial.iter().any(|line| line.contains("[ ] TODO-ONE")));
        assert!(initial.iter().any(|line| line.contains("[~] TODO-TWO")));

        assert!(store.set_todo(
            todo_id,
            vec![
                TodoItem {
                    text: "TODO-ONE".to_string(),
                    state: TodoState::Done,
                },
                TodoItem {
                    text: "TODO-THREE".to_string(),
                    state: TodoState::Pending,
                },
            ],
        ));
        let (updated, _) = draw_component_snapshot(&mut list, 80, 8);
        assert!(updated.iter().any(|line| line.contains("[x] TODO-ONE")));
        assert!(updated.iter().any(|line| line.contains("[ ] TODO-THREE")));
        assert!(!updated.iter().any(|line| line.contains("TODO-TWO")));
    }

    #[test]
    fn turn_header_label_includes_meta_and_structured_error() {
        let mut message = ChatMessage::text(ChatMessageId::new(80), ChatRole::Assistant, "body");
        message.meta = ChatMessageMeta {
            timestamp: None,
            model: Some("claude-test".to_string()),
            usage: Some(crate::message::TokenUsage {
                input: 123,
                output: 45,
            }),
            elapsed_ms: Some(6789),
            stop_reason: Some(StopReason::MaxTokens),
        };
        message.set_turn_status(ChatTurnStatus::Failed(
            crate::message::ChatError::new(ChatErrorKind::Network, "TURN-ERROR-MESSAGE")
                .with_detail("TURN-ERROR-DETAIL"),
        ));

        assert_eq!(
            turn_header_label(&message),
            "Assistant · failed\nmodel: claude-test\nusage: 123 input/45 output\nelapsed: 6789ms\nstop: max_tokens\nError kind: network\nError message: TURN-ERROR-MESSAGE\nError detail: TURN-ERROR-DETAIL"
        );
    }

    #[test]
    fn labeled_divider_uses_display_width_for_unicode_labels() {
        let centered = labeled_divider_line("时间", 10);
        assert_eq!(UnicodeWidthStr::width(centered.as_str()), 10);
        assert_eq!(centered, "── 时间 ──");

        let truncated = labeled_divider_line("时间戳", 5);
        assert_eq!(UnicodeWidthStr::width(truncated.as_str()), 5);
        assert_eq!(truncated, "时间 ");
    }

    #[test]
    fn preloaded_messages_scroll_to_bottom_on_first_draw() {
        let store = store_with_text_messages(20);
        let mut list = ChatMessageList::new(store).show_timestamps(false);

        draw_chat_list(&mut list, 40, 6);

        let max_y = list.content_size().1.saturating_sub(list.viewport_size().1);
        assert!(max_y > 0, "fixture should overflow vertically");
        assert_eq!(list.scroll_offset().1, max_y);
    }

    #[test]
    fn prepended_messages_preserve_scroll_anchor() {
        let store = store_with_text_messages(20);
        let mut list = ChatMessageList::new(store.clone()).show_timestamps(false);
        draw_chat_list(&mut list, 40, 6);
        list.set_scroll_offset(0, 0);
        list.sync_follow_tail_from_scroll();

        let previous_content_h = list.content_size().1;
        let previous_scroll_y = list.scroll_offset().1;
        let older = (0..3)
            .map(|idx| {
                ChatMessage::text(
                    store.next_message_id(),
                    ChatRole::System,
                    format!("HISTORY-{idx}"),
                )
            })
            .collect();
        store.prepend_many(older);
        list.list
            .preserve_scroll_y_after_next_layout(previous_content_h, previous_scroll_y);

        draw_chat_list(&mut list, 40, 6);

        let inserted_height = list.content_size().1.saturating_sub(previous_content_h);
        assert!(inserted_height > 0, "prepended rows should increase height");
        assert_eq!(list.scroll_offset().1, inserted_height);
    }

    #[test]
    fn tool_input_json_map_renders_key_value_lines() {
        let mut input = BTreeMap::new();
        input.insert(
            "path".to_string(),
            ComponentValue::String("src/lib.rs".to_string()),
        );
        input.insert("count".to_string(), ComponentValue::U64(2));

        let lines = tool_input_detail_lines(&ToolInput::Json(ComponentValue::Map(input)));

        assert_eq!(lines, vec!["Input:", "count: 2", "path: \"src/lib.rs\""]);
    }

    #[test]
    fn tool_input_text_renders_single_line_or_code_block() {
        assert_eq!(
            tool_input_detail_lines(&ToolInput::Text("cargo test".to_string())),
            vec!["Input: cargo test"]
        );
        assert_eq!(
            tool_input_detail_lines(&ToolInput::Text("line 1\nline 2".to_string())),
            vec!["Input:", "  line 1", "  line 2"]
        );
    }

    #[test]
    fn tool_status_canceled_uses_distinct_disclosure_status() {
        assert_eq!(
            tool_status_to_disclosure(&ToolStatus::Canceled),
            DisclosureStatus::Canceled
        );
    }

    #[test]
    fn tool_use_approval_buttons_emit_decision_and_lock_when_resolved() {
        let message_id = ChatMessageId::new(30);
        let block_id = ChatBlockId::new(30_001);
        let decisions = Arc::new(Mutex::new(Vec::new()));
        let captured = decisions.clone();
        let details = ToolUseDetails {
            input: ToolInput::Text("cargo test".to_string()),
            approval: Some(ApprovalRequest {
                id: "approval-1".to_string(),
                prompt: "Run tests?".to_string(),
                options: vec![ApprovalOption {
                    id: "allow_always".to_string(),
                    label: "Allow always".to_string(),
                }],
                resolved: None,
            }),
        };
        let mut view = ToolUseDetailsView::new(
            Binding::new(details),
            message_id,
            block_id,
            Some(Arc::new(move |decision| {
                captured.lock().expect("decisions lock").push(decision);
            })),
        );
        let theme = Theme::dark();

        assert!(view.is_focusable());
        assert!(view.focus_first());
        view.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );

        assert_eq!(
            *decisions.lock().expect("decisions lock"),
            vec![ApprovalDecision {
                message_id,
                block_id,
                approval_id: "approval-1".to_string(),
                option_id: "allow_always".to_string(),
            }]
        );

        let locked = ToolUseDetails {
            input: ToolInput::Text("cargo test".to_string()),
            approval: Some(ApprovalRequest {
                id: "approval-1".to_string(),
                prompt: "Run tests?".to_string(),
                options: vec![ApprovalOption {
                    id: "allow_always".to_string(),
                    label: "Allow always".to_string(),
                }],
                resolved: Some("allow_always".to_string()),
            }),
        };
        let locked_view = ToolUseDetailsView::new(
            Binding::new(locked),
            message_id,
            block_id,
            Some(Arc::new(|_| panic!("resolved approval must be locked"))),
        );

        assert!(!locked_view.is_focusable());
    }

    #[test]
    fn diff_decision_view_emits_decision_and_locks_when_resolved() {
        let message_id = ChatMessageId::new(40);
        let block_id = ChatBlockId::new(40_001);
        let decisions = Arc::new(Mutex::new(Vec::new()));
        let captured = decisions.clone();
        let mut view = DiffDecisionView::new(
            Some("Diff: src/lib.rs (pending)".to_string()),
            Binding::new("+new".to_string()),
            message_id,
            block_id,
            EditDecision::Pending,
            Some(Arc::new(move |decision| {
                captured.lock().expect("decisions lock").push(decision);
            })),
        );
        let theme = Theme::dark();

        assert!(view.is_focusable());
        assert!(view.focus_last());
        view.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );

        assert_eq!(
            *decisions.lock().expect("decisions lock"),
            vec![EditDecisionEvent {
                message_id,
                block_id,
                decision: EditDecision::Rejected,
            }]
        );

        let locked_view = DiffDecisionView::new(
            Some("Diff: src/lib.rs (accepted)".to_string()),
            Binding::new("+new".to_string()),
            message_id,
            block_id,
            EditDecision::Accepted,
            Some(Arc::new(|_| {
                panic!("resolved diff decision must be locked")
            })),
        );

        assert!(!locked_view.is_focusable());
        assert_eq!(
            line_text(&edit_decision_action_line(
                EditDecision::Accepted,
                0,
                false,
                component_context(&theme),
            )),
            "[x] Accepted"
        );
    }

    #[test]
    fn ansi_sgr_lines_apply_color_spans() {
        let lines = ansi_sgr_lines(
            "plain \u{1b}[31mred\u{1b}[0m \u{1b}[38;5;42mindexed",
            Style::default(),
        );

        assert_eq!(lines.len(), 1);
        let spans = &lines[0].spans;
        assert_eq!(spans[0].content.as_ref(), "plain ");
        assert_eq!(spans[1].content.as_ref(), "red");
        assert_eq!(spans[1].style.fg, Some(Color::Red));
        assert_eq!(spans[2].content.as_ref(), " ");
        assert_eq!(spans[2].style.fg, None);
        assert_eq!(spans[3].content.as_ref(), "indexed");
        assert_eq!(spans[3].style.fg, Some(Color::Indexed(42)));
    }

    #[test]
    fn diff_display_lines_reuses_unified_diff_styles() {
        let lines = diff_display_lines("+added\n-removed\n@@ hunk", Style::default());

        assert_eq!(lines[0].style.fg, Some(Color::Green));
        assert_eq!(lines[1].style.fg, Some(Color::Red));
        assert_eq!(lines[2].style.fg, Some(Color::Yellow));
    }

    #[test]
    fn markdown_wrap_width_tracks_chat_layout_width() {
        let store = ChatMessageStore::new();
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::Assistant,
            "responsive words ".repeat(18),
        ));
        let mut list = ChatMessageList::new(store)
            .show_timestamps(false)
            .auto_scroll(false);

        draw_chat_list(&mut list, 40, 20);
        let narrow_height = list.content_size().1;
        draw_chat_list(&mut list, 100, 20);
        let wide_height = list.content_size().1;

        assert!(
            narrow_height > wide_height,
            "narrow layout should wrap into more rows: narrow={narrow_height}, wide={wide_height}"
        );
    }

    #[test]
    fn diff_view_renders_horizontal_scroll_offset() {
        let mut view = DiffView::new(None, Binding::new("+0123456789".to_string()));

        assert!(draw_component_line(&mut view, 6, 1).starts_with("+0123"));
        view.set_scroll_offset(3, 0);

        assert!(draw_component_line(&mut view, 6, 1).starts_with("234567"));
    }

    #[test]
    fn ansi_output_view_renders_horizontal_scroll_offset() {
        let mut view = AnsiOutputView::new(Binding::new("0123456789".to_string()));

        assert!(draw_component_line(&mut view, 5, 1).starts_with("01234"));
        view.set_scroll_offset(4, 0);

        assert!(draw_component_line(&mut view, 5, 1).starts_with("45678"));
    }

    #[test]
    fn ansi_output_view_tails_long_output_until_expanded() {
        let output = (0..(ANSI_OUTPUT_TAIL_LINES + 3))
            .map(|idx| format!("LINE-{idx:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut view = AnsiOutputView::new(Binding::new(output));

        let tailed = view.lines(Style::default(), Style::default());
        assert_eq!(tailed.len(), ANSI_OUTPUT_TAIL_LINES + 1);
        assert!(line_text(&tailed[0]).contains(ANSI_OUTPUT_EXPAND_LABEL));
        assert_eq!(line_text(&tailed[1]), "LINE-03");
        assert_eq!(line_text(tailed.last().expect("last line")), "LINE-14");

        assert_eq!(view.expand_all(), EventResult::changed());

        let expanded = view.lines(Style::default(), Style::default());
        assert_eq!(expanded.len(), ANSI_OUTPUT_TAIL_LINES + 3);
        assert_eq!(line_text(&expanded[0]), "LINE-00");
    }

    #[test]
    fn row_keys_ignore_text_markdown_and_turn_status_for_streaming_deltas() {
        let id = ChatMessageId::new(7);
        let mut first = ChatMessage::text(id, ChatRole::Assistant, "hello")
            .with_status(ChatTurnStatus::Streaming);
        let first_key = row_keys_from_messages(&[first.clone()]);

        if let ChatBlock::Text(text) = &mut first.blocks[0] {
            text.markdown = "hello world".to_string();
        }
        let delta_key = row_keys_from_messages(&[first.clone()]);
        assert_eq!(first_key, delta_key);

        first.set_turn_status(ChatTurnStatus::Complete);
        let final_key = row_keys_from_messages(&[first]);
        assert_eq!(delta_key, final_key);
    }

    #[test]
    fn row_keys_ignore_tool_output_and_tool_status_for_streaming_updates() {
        let id = ChatMessageId::new(8);
        let mut first = ChatMessage::tool_call(id, "build", ToolStatus::Running, "starting");
        let first_key = row_keys_from_messages(&[first.clone()]);

        if let ChatBlock::ToolUse(tool) = &mut first.blocks[0] {
            tool.status = ToolStatus::Done;
        }
        if let ChatBlock::ToolResult(result) = &mut first.blocks[1] {
            result.output.set_text("starting\nfinished".to_string());
        }
        let updated_key = row_keys_from_messages(&[first.clone()]);
        assert_eq!(first_key, updated_key);

        if let ChatBlock::ToolUse(tool) = &mut first.blocks[0] {
            tool.name = "test".to_string();
        }
        let renamed_key = row_keys_from_messages(&[first]);
        assert_ne!(updated_key, renamed_key);
    }

    #[test]
    fn row_keys_ignore_todo_items_for_state_updates() {
        let id = ChatMessageId::new(16);
        let mut message = ChatMessage::new(
            id,
            ChatRole::Assistant,
            vec![ChatBlock::Todo(TodoBlock {
                id: ChatBlockId::new(16_001),
                items: vec![TodoItem {
                    text: "plan".to_string(),
                    state: TodoState::Pending,
                }],
            })],
        );
        let first_key = row_keys_from_messages(&[message.clone()]);

        if let ChatBlock::Todo(todo) = &mut message.blocks[0] {
            todo.items = vec![
                TodoItem {
                    text: "plan".to_string(),
                    state: TodoState::Done,
                },
                TodoItem {
                    text: "verify".to_string(),
                    state: TodoState::InProgress,
                },
            ];
        }
        let updated_key = row_keys_from_messages(&[message]);

        assert_eq!(first_key, updated_key);
    }

    #[test]
    fn row_keys_create_one_row_per_block_in_message_order() {
        let id = ChatMessageId::new(10);
        let message = ChatMessage::tool_call(id, "build", ToolStatus::Running, "starting");

        let keys = row_keys_from_messages(std::slice::from_ref(&message));

        assert_eq!(keys.len(), 3);
        assert!(matches!(
            &keys[0],
            ChatRowKey::Header { message_id } if *message_id == id
        ));
        assert!(matches!(
            &keys[1],
            ChatRowKey::Block {
                message_id,
                block_id,
                kind_tag: ChatBlockKindTag::ToolUse { .. },
            } if *message_id == id && *block_id == message.blocks[0].id()
        ));
        assert!(matches!(
            &keys[2],
            ChatRowKey::Block {
                message_id,
                block_id,
                kind_tag: ChatBlockKindTag::ToolResult { .. },
            } if *message_id == id && *block_id == message.blocks[1].id()
        ));
    }

    #[test]
    fn row_keys_pair_tool_result_after_matching_tool_use() {
        let id = ChatMessageId::new(12);
        let tool_use_id = ChatBlockId::new(12_001);
        let text_id = ChatBlockId::new(12_002);
        let result_id = ChatBlockId::new(12_003);
        let message = ChatMessage::new(
            id,
            ChatRole::Assistant,
            vec![
                ChatBlock::ToolUse(ToolUseBlock {
                    id: tool_use_id,
                    call_id: "call-pair".to_string(),
                    name: "build".to_string(),
                    input: ToolInput::Text("cargo build".to_string()),
                    status: ToolStatus::Running,
                    approval: None,
                    collapsed: false,
                }),
                ChatBlock::Text(TextBlock {
                    id: text_id,
                    markdown: "between".to_string(),
                    streaming: false,
                }),
                ChatBlock::ToolResult(ToolResultBlock {
                    id: result_id,
                    call_id: "call-pair".to_string(),
                    ok: true,
                    exit_code: Some(0),
                    output: ToolOutput::Ansi("done".to_string()),
                    collapsed: false,
                }),
            ],
        );

        let keys = row_keys_from_messages(&[message]);

        assert_eq!(keys.len(), 4);
        assert!(matches!(&keys[1], ChatRowKey::Block { block_id, .. } if *block_id == tool_use_id));
        assert!(matches!(&keys[2], ChatRowKey::Block { block_id, .. } if *block_id == result_id));
        assert!(matches!(&keys[3], ChatRowKey::Block { block_id, .. } if *block_id == text_id));
    }

    #[test]
    fn row_keys_pair_tool_result_from_later_message_without_duplicate_header() {
        let tool_message_id = ChatMessageId::new(13);
        let result_message_id = ChatMessageId::new(14);
        let tool_use_id = ChatBlockId::new(13_001);
        let result_id = ChatBlockId::new(14_001);
        let tool_message = ChatMessage::new(
            tool_message_id,
            ChatRole::Assistant,
            vec![ChatBlock::ToolUse(ToolUseBlock {
                id: tool_use_id,
                call_id: "call-later".to_string(),
                name: "test".to_string(),
                input: ToolInput::Text("cargo test".to_string()),
                status: ToolStatus::Running,
                approval: None,
                collapsed: false,
            })],
        );
        let result_message = ChatMessage::new(
            result_message_id,
            ChatRole::Assistant,
            vec![ChatBlock::ToolResult(ToolResultBlock {
                id: result_id,
                call_id: "call-later".to_string(),
                ok: true,
                exit_code: None,
                output: ToolOutput::Markdown("done".to_string()),
                collapsed: false,
            })],
        );

        let keys = row_keys_from_messages(&[tool_message, result_message]);

        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0].id(), ChatRowId::Header(tool_message_id));
        assert!(matches!(&keys[1], ChatRowKey::Block { block_id, .. } if *block_id == tool_use_id));
        assert!(matches!(
            &keys[2],
            ChatRowKey::Block {
                message_id,
                block_id,
                ..
            } if *message_id == result_message_id && *block_id == result_id
        ));
        assert!(!keys
            .iter()
            .any(|key| matches!(key, ChatRowKey::Header { message_id } if *message_id == result_message_id)));
    }

    #[test]
    fn row_keys_insert_pending_tool_result_when_result_is_missing() {
        let id = ChatMessageId::new(15);
        let tool_use_id = ChatBlockId::new(15_001);
        let message = ChatMessage::new(
            id,
            ChatRole::Assistant,
            vec![ChatBlock::ToolUse(ToolUseBlock {
                id: tool_use_id,
                call_id: "call-waiting".to_string(),
                name: "deploy".to_string(),
                input: ToolInput::Text("deploy".to_string()),
                status: ToolStatus::Running,
                approval: None,
                collapsed: false,
            })],
        );

        let keys = row_keys_from_messages(&[message]);

        assert_eq!(keys.len(), 3);
        assert!(matches!(&keys[1], ChatRowKey::Block { block_id, .. } if *block_id == tool_use_id));
        assert!(matches!(
            &keys[2],
            ChatRowKey::PendingToolResult {
                message_id,
                tool_use_id: pending_tool_use_id,
                call_id,
            } if *message_id == id && *pending_tool_use_id == tool_use_id && call_id == "call-waiting"
        ));
        assert_eq!(
            keys[2].id(),
            ChatRowId::PendingToolResult {
                message_id: id,
                tool_use_id,
            }
        );
        assert_eq!(
            pending_tool_result_title("call-waiting"),
            "Tool result: call-waiting (等待中)"
        );
    }

    #[test]
    fn row_keys_create_exactly_one_header_for_multi_block_message() {
        let id = ChatMessageId::new(11);
        let message = ChatMessage::tool_call(id, "build", ToolStatus::Running, "starting");

        let keys = row_keys_from_messages(&[message]);

        let header_count = keys
            .iter()
            .filter(|key| matches!(key, ChatRowKey::Header { .. }))
            .count();
        assert_eq!(header_count, 1);
        assert_eq!(keys[0].id(), ChatRowId::Header(id));
    }

    #[test]
    fn row_keys_track_artifact_link_identity() {
        let id = ChatMessageId::new(9);
        let mut first = ChatMessage::artifact(
            id,
            ChatRole::Assistant,
            ArtifactKind::Code,
            ArtifactId::new("code-1"),
            "main.rs",
        );
        let first_key = row_keys_from_messages(&[first.clone()]);

        if let ChatBlock::Artifact(artifact) = &mut first.blocks[0] {
            artifact.title = "lib.rs".to_string();
        }
        let renamed_key = row_keys_from_messages(&[first]);

        assert_ne!(first_key, renamed_key);
    }
}
