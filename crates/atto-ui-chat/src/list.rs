use std::sync::Arc;

use atto_ui::composable::{
    ComponentAction, ComponentContext, EdgeInsets, EventResult, HStack, Identifiable, LayoutParams,
    MouseCoordinateSpace, ScrollConfig, Scrollable, ScrollbarVisibility, Size, Spacer, Text,
    VStack,
};
use atto_ui::reactive::{Binding, DirtyObserver};
use atto_ui::widgets::{Disclosure, DisclosureStatus};
use atto_ui::{ComponentError, ComponentValue, ComponentValueCodec};
use atto_ui_markdown::MarkdownViewer;
use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::dynamic::{messages_to_component_value, parse_messages_value};
use crate::message::{
    ArtifactBlock, ArtifactId, ArtifactKind, AttachmentBlock, ChatAlignment, ChatBlock,
    ChatBlockId, ChatMessage, ChatMessageId, ChatMessageMeta, ChatRole, ChatTurnStatus, DiffBlock,
    DiffData, EditDecision, NoticeBlock, NoticeLevel, TextBlock, ThinkingBlock, TodoBlock,
    TodoItem, TodoState, ToolInput, ToolOutput, ToolResultBlock, ToolStatus, ToolUseBlock,
};

const DEFAULT_WRAP_WIDTH: u16 = 72;
const DEFAULT_IN_PROGRESS_SUFFIX: &str = " ▍";

type ArtifactOpenCallback = Arc<dyn Fn(ArtifactId) + Send + Sync>;

#[derive(Clone)]
struct ChatMessageListConfig {
    wrap_width: u16,
    in_progress_suffix: String,
    show_timestamps: bool,
    spacing: Binding<u16>,
    padding: Binding<EdgeInsets>,
    scroll_config: Binding<ScrollConfig>,
    on_open_artifact: Option<ArtifactOpenCallback>,
}

#[derive(Clone)]
struct ChatMessageRowConfig {
    wrap_width: u16,
    in_progress_suffix: String,
    show_timestamps: bool,
    on_open_artifact: Option<ArtifactOpenCallback>,
}

pub struct ChatMessageList {
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
    pub fn new(messages: Binding<Vec<ChatMessage>>) -> Self {
        let config = ChatMessageListConfig {
            wrap_width: DEFAULT_WRAP_WIDTH,
            in_progress_suffix: DEFAULT_IN_PROGRESS_SUFFIX.to_string(),
            show_timestamps: true,
            spacing: 1u16.into(),
            padding: EdgeInsets::symmetric(0, 1).into(),
            scroll_config: ScrollConfig::default()
                .horizontal_scrollbar(ScrollbarVisibility::Never)
                .into(),
            on_open_artifact: None,
        };
        let row_keys = Binding::new(row_keys_from_messages(&messages.get()));
        let list = build_list(row_keys.clone(), messages.clone(), &config);
        let messages_observer = messages.dirty_observer();
        Self {
            messages,
            row_keys,
            list,
            config,
            on_load_more: None,
            load_more_armed: true,
            auto_scroll: true,
            follow_tail: true,
            suppress_auto_scroll_once: false,
            pending_scroll_to_bottom: false,
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
        self.config.wrap_width = width.max(1);
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

    pub fn auto_scroll(mut self, enabled: bool) -> Self {
        self.auto_scroll = enabled;
        self
    }

    fn rebuild_list(&mut self) {
        self.row_keys
            .set(row_keys_from_messages(&self.messages.get()));
        self.list = build_list(self.row_keys.clone(), self.messages.clone(), &self.config);
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
        self.load_more_armed = false;
        self.suppress_auto_scroll_once = true;
        callback();
        true
    }

    fn track_message_changes(&mut self) {
        if !self.messages.check_dirty(&mut self.messages_observer) {
            return;
        }
        self.row_keys
            .set(row_keys_from_messages(&self.messages.get()));
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

    fn apply_pending_scroll(&mut self) {
        if !self.pending_scroll_to_bottom {
            return;
        }
        let viewport_h = self.list.viewport_size().1;
        if viewport_h == 0 {
            return;
        }
        let content_h = self.list.content_size().1;
        let max_y = content_h.saturating_sub(viewport_h);
        self.list.set_scroll_offset(0, max_y);
        self.follow_tail = true;
        self.pending_scroll_to_bottom = false;
        self.load_more_armed = true;
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
            "messages" => Some(messages_to_component_value(&self.messages.get())),
            "spacing" => Some(ComponentValue::U64(self.config.spacing.get() as u64)),
            "padding" => Some(self.config.padding.get().to_component_value()),
            "wrap_width" => Some(ComponentValue::U64(self.config.wrap_width as u64)),
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
                self.messages.set(messages);
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
                self.config.wrap_width = width.max(1);
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
                Ok(())
            }
            _ => Err(ComponentError::unsupported_property(name)),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.track_message_changes();
        self.list.draw(frame, area, ctx);
        self.apply_pending_scroll();
    }
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
    messages: Binding<Vec<ChatMessage>>,
    config: &ChatMessageListConfig,
) -> atto_ui::composable::ForEachIdentifiable<ChatRowKey, ChatMessageRow> {
    let row_config = ChatMessageRowConfig {
        wrap_width: config.wrap_width,
        in_progress_suffix: config.in_progress_suffix.clone(),
        show_timestamps: config.show_timestamps,
        on_open_artifact: config.on_open_artifact.clone(),
    };
    let list = atto_ui::composable::ForEach::new(row_keys, move |key, _| {
        ChatMessageRow::new(key.clone(), messages.clone(), row_config.clone())
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ChatRowId {
    Header(ChatMessageId),
    Block {
        message_id: ChatMessageId,
        block_id: ChatBlockId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChatRowKind {
    Header,
    Block(ChatBlockId),
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
    Todo {
        items: Vec<TodoItem>,
    },
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
        }
    }
}

impl ChatRowKey {
    fn message_id(&self) -> ChatMessageId {
        match self {
            ChatRowKey::Header { message_id } | ChatRowKey::Block { message_id, .. } => *message_id,
        }
    }

    fn row_kind(&self) -> ChatRowKind {
        match self {
            ChatRowKey::Header { .. } => ChatRowKind::Header,
            ChatRowKey::Block { block_id, .. } => ChatRowKind::Block(*block_id),
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
        ChatBlockKindTag::Todo { items } => Some(ChatBlock::Todo(TodoBlock {
            id: block_id,
            items: items.clone(),
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

fn row_keys_from_messages(messages: &[ChatMessage]) -> Vec<ChatRowKey> {
    let mut rows = Vec::new();
    for message in messages {
        rows.push(ChatRowKey::Header {
            message_id: message.id,
        });
        rows.extend(message.blocks.iter().map(|block| ChatRowKey::Block {
            message_id: message.id,
            block_id: block.id(),
            kind_tag: block_kind_tag(block),
        }));
    }
    rows
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
        ChatBlock::Todo(TodoBlock { items, .. }) => ChatBlockKindTag::Todo {
            items: items.clone(),
        },
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
    tool_output: Option<Binding<String>>,
    tool_status: Option<Binding<DisclosureStatus>>,
}

struct ChatMessageRow {
    message_id: ChatMessageId,
    row_kind: ChatRowKind,
    messages: Binding<Vec<ChatMessage>>,
    body_bindings: ChatMessageRowBindings,
    config: ChatMessageRowConfig,
    view: VStack,
}

impl ChatMessageRow {
    fn new(
        key: ChatRowKey,
        messages: Binding<Vec<ChatMessage>>,
        config: ChatMessageRowConfig,
    ) -> Self {
        let message_id = key.message_id();
        let row_kind = key.row_kind();
        let message =
            find_message(&messages.get(), message_id).unwrap_or_else(|| key.placeholder());
        let (view, body_bindings) = build_row_view(&message, row_kind, &config);
        Self {
            message_id,
            row_kind,
            messages,
            body_bindings,
            config,
            view,
        }
    }

    fn sync_body_bindings(&self) {
        let messages = self.messages.get();
        let Some(message) = find_message(&messages, self.message_id) else {
            return;
        };
        if let Some(binding) = &self.body_bindings.header {
            binding.set(turn_header_label(&message));
        }
        if let Some(binding) = &self.body_bindings.timestamp {
            binding.set(message.meta.timestamp.clone());
        }

        let block = match self.row_kind {
            ChatRowKind::Header => None,
            ChatRowKind::Block(block_id) => find_block(&message, block_id),
        };

        if let Some(binding) = &self.body_bindings.markdown
            && let Some(markdown) =
                block.and_then(|block| block_markdown_for_render(block, &self.config))
        {
            binding.set(markdown);
        }

        if let Some(block) = block {
            if let Some(binding) = &self.body_bindings.tool_output
                && let Some(output) = block_tool_output_for_render(block)
            {
                binding.set(output);
            }

            if let Some(binding) = &self.body_bindings.tool_status
                && let ChatBlock::ToolUse(tool) = block
            {
                binding.set(tool_status_to_disclosure(&tool.status));
            }
        }
    }
}

fn build_row_view(
    message: &ChatMessage,
    row_kind: ChatRowKind,
    config: &ChatMessageRowConfig,
) -> (VStack, ChatMessageRowBindings) {
    let mut column = VStack::new().with_spacing(1);
    let row_layout = LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    };

    match row_kind {
        ChatRowKind::Header => {
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
        ChatRowKind::Block(block_id) => {
            let block = find_block(message, block_id);
            let (bubble, body_bindings) = build_aligned_block(message, block, config);
            column = column.child_with_layout(bubble, row_layout);
            (column, body_bindings)
        }
    }
}

fn find_message(messages: &[ChatMessage], id: ChatMessageId) -> Option<ChatMessage> {
    messages.iter().find(|message| message.id == id).cloned()
}

fn find_block(message: &ChatMessage, id: ChatBlockId) -> Option<&ChatBlock> {
    message.blocks.iter().find(|block| block.id() == id)
}

fn block_markdown_for_render(block: &ChatBlock, config: &ChatMessageRowConfig) -> Option<String> {
    let (markdown, streaming) = match block {
        ChatBlock::Text(text) => (&text.markdown, text.streaming),
        ChatBlock::Thinking(thinking) => (&thinking.markdown, thinking.streaming),
        _ => return None,
    };
    let mut content = markdown.clone();
    if streaming && !config.in_progress_suffix.is_empty() {
        content.push_str(&config.in_progress_suffix);
    }
    Some(content)
}

fn block_tool_output_for_render(block: &ChatBlock) -> Option<String> {
    match block {
        ChatBlock::ToolResult(result) => Some(result.output.as_text().to_string()),
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
    let (bubble, body_bindings) = build_block_bubble(block, config);
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

fn build_turn_header(message: &ChatMessage) -> (VStack, ChatMessageRowBindings) {
    let header_label = Binding::new(turn_header_label(message));
    let header = HStack::new()
        .with_spacing(1)
        .child(Text::new(String::new()).text(header_label.clone()));
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
    block: Option<&ChatBlock>,
    config: &ChatMessageRowConfig,
) -> (VStack, ChatMessageRowBindings) {
    let (body, body_bindings) = ChatMessageBody::from_block(block, config);
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
            label.push_str(&format!(" (failed: {})", error.message));
        }
        ChatTurnStatus::Canceled => {
            label.push_str(" (canceled)");
        }
        ChatTurnStatus::Complete | ChatTurnStatus::Streaming => {}
    }

    label
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
        let width = area.width as usize;
        let label = format!(" {raw_label} ");
        let label_width = label.width();
        let line = if label_width >= width {
            label.chars().take(width).collect::<String>()
        } else {
            let padding = width.saturating_sub(label_width);
            let left = padding / 2;
            let right = padding.saturating_sub(left);
            format!("{}{}{}", "─".repeat(left), label, "─".repeat(right))
        };
        let style = ctx.theme.widget.dim;
        frame.render_widget(Paragraph::new(Line::styled(line, style)), area);
    }
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

enum ChatMessageBody {
    Markdown(MarkdownViewer),
    File(VStack),
    Tool(Disclosure),
    Artifact(ArtifactLink),
}

impl ChatMessageBody {
    fn from_block(
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
                        MarkdownViewer::new(content.clone())
                            .streaming_tolerant(true)
                            .wrap_width(config.wrap_width)
                            .vertical_scrollbar(ScrollbarVisibility::Never),
                    ),
                    ChatMessageRowBindings {
                        markdown: Some(content),
                        ..ChatMessageRowBindings::default()
                    },
                )
            }
            Some(ChatBlock::Thinking(_)) => {
                let content = Binding::new(
                    block
                        .and_then(|block| block_markdown_for_render(block, config))
                        .unwrap_or_default(),
                );
                (
                    ChatMessageBody::Markdown(
                        MarkdownViewer::new(content.clone())
                            .streaming_tolerant(true)
                            .wrap_width(config.wrap_width)
                            .vertical_scrollbar(ScrollbarVisibility::Never),
                    ),
                    ChatMessageRowBindings {
                        markdown: Some(content),
                        ..ChatMessageRowBindings::default()
                    },
                )
            }
            Some(ChatBlock::Attachment(AttachmentBlock { name, url, .. })) => {
                let mut view = VStack::new().with_spacing(0);
                view = view.child(Text::new(format!("File: {name}")));
                if let Some(url) = url {
                    view = view.child(Text::new(format!("Url: {url}")));
                }
                (
                    ChatMessageBody::File(view),
                    ChatMessageRowBindings::default(),
                )
            }
            Some(ChatBlock::ToolUse(tool)) => {
                let input = Binding::new(tool_input_to_text(&tool.input));
                let status = Binding::new(tool_status_to_disclosure(&tool.status));
                let view = Disclosure::new(format!("Tool: {}", tool.name))
                    .expanded(!tool.collapsed)
                    .status(status.clone())
                    .content(input);
                (
                    ChatMessageBody::Tool(view),
                    ChatMessageRowBindings {
                        tool_status: Some(status),
                        ..ChatMessageRowBindings::default()
                    },
                )
            }
            Some(ChatBlock::ToolResult(result)) => {
                let output = Binding::new(result.output.as_text().to_string());
                let status = Binding::new(tool_result_to_disclosure(result));
                let view = Disclosure::new(format!("Tool result: {}", result.call_id))
                    .expanded(!result.collapsed)
                    .status(status)
                    .content(output.clone());
                (
                    ChatMessageBody::Tool(view),
                    ChatMessageRowBindings {
                        tool_output: Some(output),
                        ..ChatMessageRowBindings::default()
                    },
                )
            }
            Some(ChatBlock::Diff(DiffBlock { diff, .. })) => {
                let content = Binding::new(diff.unified.clone());
                (
                    ChatMessageBody::Markdown(
                        MarkdownViewer::new(content)
                            .streaming_tolerant(true)
                            .wrap_width(config.wrap_width)
                            .vertical_scrollbar(ScrollbarVisibility::Never),
                    ),
                    ChatMessageRowBindings::default(),
                )
            }
            Some(ChatBlock::Todo(TodoBlock { items, .. })) => {
                let content = Binding::new(todo_items_to_markdown(items));
                (
                    ChatMessageBody::Markdown(
                        MarkdownViewer::new(content)
                            .streaming_tolerant(true)
                            .wrap_width(config.wrap_width)
                            .vertical_scrollbar(ScrollbarVisibility::Never),
                    ),
                    ChatMessageRowBindings::default(),
                )
            }
            Some(ChatBlock::Notice(NoticeBlock { text, .. })) => {
                let content = Binding::new(text.clone());
                (
                    ChatMessageBody::Markdown(
                        MarkdownViewer::new(content)
                            .streaming_tolerant(true)
                            .wrap_width(config.wrap_width)
                            .vertical_scrollbar(ScrollbarVisibility::Never),
                    ),
                    ChatMessageRowBindings::default(),
                )
            }
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
                        MarkdownViewer::new(content.clone())
                            .streaming_tolerant(true)
                            .wrap_width(config.wrap_width)
                            .vertical_scrollbar(ScrollbarVisibility::Never),
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

fn todo_items_to_markdown(items: &[TodoItem]) -> String {
    items
        .iter()
        .map(|item| format!("{} {}", todo_state_marker(item.state), item.text))
        .collect::<Vec<_>>()
        .join("\n")
}

fn todo_state_marker(state: TodoState) -> &'static str {
    match state {
        TodoState::Pending => "[ ]",
        TodoState::InProgress => "[~]",
        TodoState::Done => "[x]",
    }
}

fn tool_status_to_disclosure(status: &ToolStatus) -> DisclosureStatus {
    match status {
        ToolStatus::Pending | ToolStatus::Running => DisclosureStatus::Running,
        ToolStatus::Done => DisclosureStatus::Done,
        ToolStatus::Error | ToolStatus::Canceled => DisclosureStatus::Error,
    }
}

fn tool_result_to_disclosure(result: &ToolResultBlock) -> DisclosureStatus {
    if result.ok {
        DisclosureStatus::Done
    } else {
        DisclosureStatus::Error
    }
}

fn tool_input_to_text(input: &ToolInput) -> String {
    match input {
        ToolInput::Text(text) => text.clone(),
        ToolInput::Json(value) => format!("{value:?}"),
    }
}

impl ::atto_ui::composable::Component for ChatMessageBody {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        match self {
            ChatMessageBody::Markdown(view) => view.draw(frame, area, ctx),
            ChatMessageBody::File(view) => view.draw(frame, area, ctx),
            ChatMessageBody::Tool(view) => view.draw(frame, area, ctx),
            ChatMessageBody::Artifact(view) => view.draw(frame, area, ctx),
        }
    }
}

impl ::atto_ui::composable::DragAndDrop for ChatMessageBody {}

impl ::atto_ui::composable::Layout for ChatMessageBody {
    fn min_width(&self) -> u16 {
        match self {
            ChatMessageBody::Markdown(view) => view.min_width(),
            ChatMessageBody::File(view) => view.min_width(),
            ChatMessageBody::Tool(view) => view.min_width(),
            ChatMessageBody::Artifact(view) => view.min_width(),
        }
    }

    fn min_height(&self) -> u16 {
        match self {
            ChatMessageBody::Markdown(view) => view.min_height(),
            ChatMessageBody::File(view) => view.min_height(),
            ChatMessageBody::Tool(view) => view.min_height(),
            ChatMessageBody::Artifact(view) => view.min_height(),
        }
    }

    fn desired_width(&self) -> Option<u16> {
        match self {
            ChatMessageBody::Markdown(view) => view.desired_width(),
            ChatMessageBody::File(view) => view.desired_width(),
            ChatMessageBody::Tool(view) => view.desired_width(),
            ChatMessageBody::Artifact(view) => view.desired_width(),
        }
    }

    fn desired_height(&self) -> Option<u16> {
        match self {
            ChatMessageBody::Markdown(view) => view.desired_height(),
            ChatMessageBody::File(view) => view.desired_height(),
            ChatMessageBody::Tool(view) => view.desired_height(),
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
            ChatMessageBody::File(view) => view.handle_event(event, ctx),
            ChatMessageBody::Tool(view) => view.handle_event(event, ctx),
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
    match coordinate_space {
        MouseCoordinateSpace::Absolute => {
            mouse.column >= area.x
                && mouse.column < area.x.saturating_add(area.width)
                && mouse.row >= area.y
                && mouse.row < area.y.saturating_add(area.height)
        }
        MouseCoordinateSpace::Local => mouse.column < area.width && mouse.row < area.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ChatMessageId, ChatRole};

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
