use std::cell::Cell;
use std::sync::Arc;

use atto_ui::composable::{
    Component, ComponentAction, ComponentContext, EdgeInsets, EventResult, HStack, Identifiable,
    LayoutParams, MouseCoordinateSpace, ScrollConfig, Scrollable, ScrollbarVisibility, Size,
    Spacer, Text, VStack,
};
use atto_ui::reactive::{Binding, DirtyObserver};
use atto_ui::widgets::{Disclosure, DisclosureStatus};
use atto_ui::{ComponentError, ComponentValue, ComponentValueCodec};
use atto_ui_markdown::MarkdownViewer;
use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::dynamic::{messages_to_component_value, parse_messages_value};
use crate::message::{
    ArtifactBlock, ArtifactId, ArtifactKind, AttachmentBlock, ChatAlignment, ChatBlock,
    ChatBlockId, ChatMessage, ChatMessageId, ChatMessageMeta, ChatRole, ChatTurnStatus, DiffBlock,
    DiffData, EditDecision, NoticeBlock, NoticeLevel, TextBlock, ThinkingBlock, TodoBlock,
    TodoItem, TodoState, ToolInput, ToolOutput, ToolResultBlock, ToolStatus, ToolUseBlock,
};
use crate::store::ChatMessageStore;
use crate::viewer::diff_line_style;

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
        let messages = store.binding();
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
        self.load_more_armed = false;
        self.suppress_auto_scroll_once = true;
        callback();
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
            "messages" => Some(
                self.messages
                    .with(|messages| messages_to_component_value(messages)),
            ),
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
    store: ChatMessageStore,
    config: &ChatMessageListConfig,
) -> atto_ui::composable::ForEachIdentifiable<ChatRowKey, ChatMessageRow> {
    let row_config = ChatMessageRowConfig {
        wrap_width: config.wrap_width,
        in_progress_suffix: config.in_progress_suffix.clone(),
        show_timestamps: config.show_timestamps,
        on_open_artifact: config.on_open_artifact.clone(),
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
enum ChatRowRef {
    Header(ChatMessageId),
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

    fn row_ref(&self) -> ChatRowRef {
        match self {
            ChatRowKey::Header { message_id } => ChatRowRef::Header(*message_id),
            ChatRowKey::Block { block_id, .. } => ChatRowRef::Block(*block_id),
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
            .with_message(message_id, |message| {
                build_row_view(message, row_ref, &config)
            })
            .unwrap_or_else(|| {
                let message = key.placeholder();
                build_row_view(&message, row_ref, &config)
            });
        let last_message_version = match row_ref {
            ChatRowRef::Header(message_id) => store.message_version(message_id),
            ChatRowRef::Block(_) => 0,
        };
        let last_block_version = match row_ref {
            ChatRowRef::Header(_) => 0,
            ChatRowRef::Block(block_id) => store.block_version(block_id),
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
    row_ref: ChatRowRef,
    config: &ChatMessageRowConfig,
) -> (VStack, ChatMessageRowBindings) {
    let mut column = VStack::new().with_spacing(1);
    let row_layout = LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    };

    match row_ref {
        ChatRowRef::Header(_) => {
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
        ChatRowRef::Block(block_id) => {
            let block = find_block(message, block_id);
            let (bubble, body_bindings) = build_aligned_block(message, block, config);
            column = column.child_with_layout(bubble, row_layout);
            (column, body_bindings)
        }
    }
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
    Some(markdown_for_render(markdown, streaming, config))
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
    Text(Text),
    Disclosure(Disclosure),
    Diff(DiffView),
    Todo(TodoListView),
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
            Some(ChatBlock::Thinking(thinking)) => {
                let content = Binding::new(markdown_for_render(
                    &thinking.markdown,
                    thinking.streaming,
                    config,
                ));
                let status = Binding::new(thinking_status_to_disclosure(thinking));
                let viewer = MarkdownViewer::new(content.clone())
                    .streaming_tolerant(true)
                    .wrap_width(config.wrap_width)
                    .text_color(Color::DarkGray)
                    .vertical_scrollbar(ScrollbarVisibility::Never);
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
                let input = Binding::new(tool_use_content(tool));
                let status = Binding::new(tool_status_to_disclosure(&tool.status));
                let view = Disclosure::new(tool.name.clone())
                    .expanded(!tool.collapsed)
                    .status(status.clone())
                    .content(input);
                (
                    ChatMessageBody::Disclosure(view),
                    ChatMessageRowBindings {
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
                    ChatMessageBody::Diff(DiffView::new(Some(diff_block_title(diff)), content)),
                    ChatMessageRowBindings::default(),
                )
            }
            Some(ChatBlock::Todo(TodoBlock { items, .. })) => (
                ChatMessageBody::Todo(TodoListView::new(items.clone())),
                ChatMessageRowBindings::default(),
            ),
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

fn tool_result_title(result: &ToolResultBlock) -> String {
    match result.exit_code {
        Some(code) => format!("Tool result: {} (exit {code})", result.call_id),
        None => format!("Tool result: {}", result.call_id),
    }
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

fn tool_output_component(
    output: &ToolOutput,
    content: Binding<String>,
    config: &ChatMessageRowConfig,
) -> Box<dyn Component> {
    match output {
        ToolOutput::Ansi(_) => Box::new(AnsiOutputView::new(content)),
        ToolOutput::Markdown(_) => Box::new(
            MarkdownViewer::new(content)
                .streaming_tolerant(true)
                .wrap_width(config.wrap_width)
                .vertical_scrollbar(ScrollbarVisibility::Never),
        ),
        ToolOutput::Diff(_) => Box::new(DiffView::new(None, content)),
    }
}

fn tool_use_content(tool: &ToolUseBlock) -> String {
    let mut sections = vec![tool_input_to_text(&tool.input)];
    if let Some(approval) = &tool.approval {
        sections.push(approval_request_text(approval));
    }
    sections
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn tool_input_to_text(input: &ToolInput) -> String {
    match input {
        ToolInput::Text(text) => text.clone(),
        ToolInput::Json(value) => component_value_lines(value).join("\n"),
    }
}

fn approval_request_text(approval: &crate::message::ApprovalRequest) -> String {
    let mut lines = vec![format!("Approval: {}", approval.prompt)];
    lines.extend(approval.options.iter().map(|option| {
        let marker = if approval.resolved.as_deref() == Some(option.id.as_str()) {
            "[x]"
        } else {
            "[ ]"
        };
        format!("{marker} {}", option.label)
    }));
    lines.join("\n")
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
}

impl DiffView {
    fn new(title: Option<String>, diff: impl Into<Binding<String>>) -> Self {
        Self {
            title,
            diff: diff.into(),
        }
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
}

impl Component for DiffView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let base = ctx.theme.widget.normal;
        let mut lines = Vec::new();
        if let Some(title) = &self.title {
            lines.push(Line::styled(title.clone(), ctx.theme.widget.dim));
        }
        let diff = self.diff.get();
        lines.extend(diff_display_lines(&diff, base));
        frame.render_widget(Paragraph::new(lines), area);
    }
}

impl ::atto_ui::composable::DragAndDrop for DiffView {}
impl ::atto_ui::composable::Scrollable for DiffView {}
impl ::atto_ui::composable::FocusNav for DiffView {}
impl ::atto_ui::composable::DynamicTree for DiffView {}
impl ::atto_ui::composable::EventHandling for DiffView {}

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
    items: Vec<TodoItem>,
}

impl TodoListView {
    fn new(items: Vec<TodoItem>) -> Self {
        Self { items }
    }

    fn display_lines(&self) -> Vec<String> {
        if self.items.is_empty() {
            return vec!["(no todo items)".to_string()];
        }
        self.items
            .iter()
            .map(|item| format!("{} {}", todo_state_marker(item.state), item.text))
            .collect()
    }
}

impl Component for TodoListView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let lines = if self.items.is_empty() {
            vec![Line::styled("(no todo items)", ctx.theme.widget.dim)]
        } else {
            self.items
                .iter()
                .map(|item| {
                    Line::styled(
                        format!("{} {}", todo_state_marker(item.state), item.text),
                        todo_state_style(item.state, ctx),
                    )
                })
                .collect()
        };
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
}

impl AnsiOutputView {
    fn new(text: impl Into<Binding<String>>) -> Self {
        Self { text: text.into() }
    }

    fn lines(&self, base: Style) -> Vec<Line<'static>> {
        ansi_sgr_lines(&self.text.get(), base)
    }
}

impl Component for AnsiOutputView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        frame.render_widget(Paragraph::new(self.lines(ctx.theme.widget.normal)), area);
    }
}

impl ::atto_ui::composable::DragAndDrop for AnsiOutputView {}
impl ::atto_ui::composable::Scrollable for AnsiOutputView {}
impl ::atto_ui::composable::FocusNav for AnsiOutputView {}
impl ::atto_ui::composable::DynamicTree for AnsiOutputView {}
impl ::atto_ui::composable::EventHandling for AnsiOutputView {}

impl ::atto_ui::composable::Layout for AnsiOutputView {
    fn min_width(&self) -> u16 {
        1
    }

    fn min_height(&self) -> u16 {
        1
    }

    fn desired_width(&self) -> Option<u16> {
        Some(
            self.lines(Style::default())
                .iter()
                .map(Line::width)
                .max()
                .unwrap_or(1)
                .max(1)
                .min(u16::MAX as usize) as u16,
        )
    }

    fn desired_height(&self) -> Option<u16> {
        Some(
            self.lines(Style::default())
                .len()
                .max(1)
                .min(u16::MAX as usize) as u16,
        )
    }
}

fn todo_state_style(state: TodoState, ctx: ComponentContext<'_>) -> Style {
    match state {
        TodoState::Pending => ctx.theme.widget.normal,
        TodoState::InProgress => ctx.theme.widget.accent,
        TodoState::Done => ctx.theme.widget.dim,
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
    use std::collections::BTreeMap;

    use super::*;
    use crate::message::{ChatMessageId, ChatRole};

    #[test]
    fn tool_input_json_map_renders_key_value_lines() {
        let mut input = BTreeMap::new();
        input.insert(
            "path".to_string(),
            ComponentValue::String("src/lib.rs".to_string()),
        );
        input.insert("count".to_string(), ComponentValue::U64(2));

        let text = tool_input_to_text(&ToolInput::Json(ComponentValue::Map(input)));

        assert_eq!(text, "count: 2\npath: \"src/lib.rs\"");
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
