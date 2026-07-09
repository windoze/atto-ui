use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::sync::Arc;

use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::{Buffer, Cell as BufferCell};

use atto_ui::composable::{
    Capture, Component, ComponentAction, ComponentContext, ComponentId, EdgeInsets, EventHandling,
    EventResult, FocusNav, HStack, Identifiable, Layout, LayoutParams, MouseCoordinateSpace,
    ScrollConfig, ScrollContainer, ScrollContainerHost, ScrollContent, ScrollContentContext,
    Scrollable, ScrollbarVisibility, Size, Spacer, Text, VStack,
};
use atto_ui::reactive::{Binding, DirtyObserver};
use atto_ui::theme::Theme;
use atto_ui::widgets::{Button, Disclosure, DisclosureStatus};
use atto_ui::{ComponentError, ComponentValue, ComponentValueCodec};
use atto_ui_markdown::MarkdownViewer;
use atto_ui_markdown::syntax::{HighlightedLine, SyntaxClass, highlight_code_block};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Frame;
use ratatui::layout::{Position, Rect, Size as TerminalSize};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::dynamic::{messages_to_component_value, parse_messages_value};
use crate::input::{ChatInputHandle, ChatInputReference, ChatTextSubmitInterceptor};
use crate::message::{
    ApprovalOption, ApprovalRequest, ArtifactBlock, ArtifactId, ArtifactKind, AttachmentBlock,
    ChatAlignment, ChatBlock, ChatBlockId, ChatErrorKind, ChatMessage, ChatMessageId,
    ChatMessageMeta, ChatRole, ChatTurnStatus, DiffBlock, DiffData, EditDecision, NoticeBlock,
    NoticeLevel, PlanBlock, PlanDecision, PlanItem, StopReason, TaskBlock, TaskStatus,
    TaskTranscriptItem, TextBlock, ThinkingBlock, TodoBlock, TodoItem, TodoState, ToolInput,
    ToolOutput, ToolResultBlock, ToolStatus, ToolUseBlock,
};
use crate::store::ChatMessageStore;

const DEFAULT_IN_PROGRESS_SUFFIX: &str = " ▍";
/// Fraction (percent of list width) a message bubble may occupy; the rest is an
/// alignment spacer. 100 = bubbles fill the whole list width (no spacer).
const DEFAULT_BUBBLE_WIDTH_PERCENT: u16 = 75;
const ANSI_OUTPUT_TAIL_LINES: usize = 12;
const ANSI_OUTPUT_EXPAND_LABEL: &str = "展开全部";

type ArtifactOpenCallback = Arc<dyn Fn(ArtifactId) + Send + Sync>;
type ApprovalCallback = Arc<dyn Fn(ApprovalDecision) + Send + Sync>;
type EditDecisionCallback = Arc<dyn Fn(EditDecisionEvent) + Send + Sync>;
type EditAndResubmitCallback = Arc<dyn Fn(EditAndResubmitEvent) + Send + Sync>;
type PlanDecisionCallback = Arc<dyn Fn(PlanDecisionEvent) + Send + Sync>;
type MessageActionCallback = Arc<dyn Fn(MessageAction) + Send + Sync>;
type CancelCallback = Arc<dyn Fn(ChatMessageId) + Send + Sync>;

#[derive(Clone)]
pub(crate) struct StreamingCancelController {
    store: ChatMessageStore,
    callback: CancelCallback,
}

impl StreamingCancelController {
    fn new(store: ChatMessageStore, callback: CancelCallback) -> Self {
        Self { store, callback }
    }

    pub(crate) fn request_current(&self) -> bool {
        let Some(message_id) = latest_streaming_message_id(&self.store) else {
            return false;
        };
        self.request(message_id)
    }

    fn request(&self, message_id: ChatMessageId) -> bool {
        let is_streaming = self
            .store
            .with_message(message_id, |message| message.status.is_streaming())
            .unwrap_or(false);
        if !is_streaming {
            return false;
        }
        let _ = self
            .store
            .set_turn_status(message_id, ChatTurnStatus::Canceled);
        (self.callback)(message_id);
        true
    }
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanDecisionEvent {
    pub message_id: ChatMessageId,
    pub block_id: ChatBlockId,
    pub decision: PlanDecision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageAction {
    /// For Retry/Regenerate, the target assistant turn has already been
    /// truncated when this action is emitted.
    pub message_id: ChatMessageId,
    pub kind: MessageActionKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditAndResubmitEvent {
    pub message_id: ChatMessageId,
    pub original_text: String,
    pub edited_text: String,
    pub removed_messages: Vec<ChatMessage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageActionKind {
    Copy,
    Retry,
    Regenerate,
    EditUser,
    CopyBlock(ChatBlockId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingUserEdit {
    message_id: ChatMessageId,
    original_text: String,
}

#[derive(Clone)]
struct EditAndResubmitController {
    store: ChatMessageStore,
    pending: Binding<Option<PendingUserEdit>>,
    input_draft: Binding<String>,
    callback: EditAndResubmitCallback,
}

impl EditAndResubmitController {
    fn new(
        store: ChatMessageStore,
        input_draft: Binding<String>,
        callback: EditAndResubmitCallback,
    ) -> Self {
        Self {
            store,
            pending: Binding::new(None),
            input_draft,
            callback,
        }
    }

    fn begin_edit(&self, message_id: ChatMessageId, original_text: String) -> bool {
        self.pending.set(Some(PendingUserEdit {
            message_id,
            original_text: original_text.clone(),
        }));
        self.input_draft.set(original_text);
        true
    }

    fn submit_edit(&self, edited_text: String) -> bool {
        let Some(pending) = self.pending.get() else {
            return false;
        };
        let Some(removed_messages) = self.store.truncate_from(pending.message_id) else {
            self.pending.set(None);
            return true;
        };
        self.pending.set(None);
        (self.callback)(EditAndResubmitEvent {
            message_id: pending.message_id,
            original_text: pending.original_text,
            edited_text,
            removed_messages,
        });
        true
    }
}

#[derive(Clone)]
struct QuoteReplyController {
    references: Binding<Vec<ChatInputReference>>,
}

impl QuoteReplyController {
    fn new(references: Binding<Vec<ChatInputReference>>) -> Self {
        Self { references }
    }

    fn attach(&self, reference: ChatInputReference) {
        self.references.update(|items| {
            let key = (reference.message_id, reference.block_id);
            if let Some(existing) = items
                .iter_mut()
                .find(|item| (item.message_id, item.block_id) == key)
            {
                *existing = reference;
            } else {
                items.push(reference);
            }
        });
    }
}

#[derive(Clone)]
struct ChatMessageListConfig {
    wrap_width: Option<u16>,
    responsive_wrap_width: Binding<Option<u16>>,
    in_progress_suffix: String,
    show_timestamps: bool,
    bubble_width_percent: u16,
    spacing: Binding<u16>,
    padding: Binding<EdgeInsets>,
    scroll_config: Binding<ScrollConfig>,
    collapsed_turns: Binding<HashSet<ChatMessageId>>,
    on_open_artifact: Option<ArtifactOpenCallback>,
    on_approve: Option<ApprovalCallback>,
    on_edit_decision: Option<EditDecisionCallback>,
    edit_and_resubmit: Option<EditAndResubmitController>,
    quote_replies: Option<QuoteReplyController>,
    on_plan_decision: Option<PlanDecisionCallback>,
    on_message_action: Option<MessageActionCallback>,
    on_cancel: Option<CancelCallback>,
}

#[derive(Clone)]
struct ChatMessageRowConfig {
    store: ChatMessageStore,
    wrap_width: Option<u16>,
    responsive_wrap_width: Binding<Option<u16>>,
    in_progress_suffix: String,
    show_timestamps: bool,
    bubble_width_percent: u16,
    collapsed_turns: Binding<HashSet<ChatMessageId>>,
    on_open_artifact: Option<ArtifactOpenCallback>,
    on_approve: Option<ApprovalCallback>,
    on_edit_decision: Option<EditDecisionCallback>,
    edit_and_resubmit: Option<EditAndResubmitController>,
    quote_replies: Option<QuoteReplyController>,
    on_plan_decision: Option<PlanDecisionCallback>,
    on_message_action: Option<MessageActionCallback>,
    on_cancel: Option<CancelCallback>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SearchMatch {
    row_id: ChatRowId,
}

#[derive(Clone, Debug, Default)]
struct SearchState {
    active: bool,
    query: String,
    matches: Vec<SearchMatch>,
    active_index: Option<usize>,
    restore_scroll: Option<(u16, u16)>,
}

impl SearchState {
    fn active_match(&self) -> Option<&SearchMatch> {
        self.active_index.and_then(|idx| self.matches.get(idx))
    }

    fn status_label(&self) -> String {
        let count = self.matches.len();
        let position = self.active_index.map_or(0, |idx| idx.saturating_add(1));
        let suffix = if self.query.is_empty() {
            "type to search".to_string()
        } else if count == 0 {
            "no matches".to_string()
        } else {
            format!("{position}/{count}")
        };
        format!(
            "Search: {} ({suffix})  Enter/Down next  Up prev  Esc close",
            self.query
        )
    }
}

pub struct ChatMessageList {
    store: ChatMessageStore,
    messages: Binding<Vec<ChatMessage>>,
    message_ids: Vec<ChatMessageId>,
    row_keys: Binding<Vec<ChatRowKey>>,
    list: ScrollContainer,
    virtual_control: VirtualChatRowsControl,
    config: ChatMessageListConfig,
    on_load_more: Option<Arc<dyn Fn() + Send + Sync>>,
    load_more_armed: bool,
    auto_scroll: bool,
    follow_tail: bool,
    suppress_auto_scroll_once: bool,
    pending_scroll_to_bottom: bool,
    search: SearchState,
    messages_observer: DirtyObserver,
}

impl ChatMessageList {
    pub fn new(store: ChatMessageStore) -> Self {
        let collapsed_turns = Binding::new(HashSet::new());
        let config = ChatMessageListConfig {
            wrap_width: None,
            responsive_wrap_width: Binding::new(None),
            in_progress_suffix: DEFAULT_IN_PROGRESS_SUFFIX.to_string(),
            show_timestamps: false,
            bubble_width_percent: DEFAULT_BUBBLE_WIDTH_PERCENT,
            spacing: 1u16.into(),
            padding: EdgeInsets::symmetric(0, 1).into(),
            scroll_config: ScrollConfig::default().into(),
            collapsed_turns: collapsed_turns.clone(),
            on_open_artifact: None,
            on_approve: None,
            on_edit_decision: None,
            edit_and_resubmit: None,
            quote_replies: None,
            on_plan_decision: None,
            on_message_action: None,
            on_cancel: None,
        };
        let messages = store.binding();
        let has_initial_messages = messages.with(|messages| !messages.is_empty());
        let message_ids = messages.with(|messages| message_ids_from_messages(messages));
        let collapsed = collapsed_turns.get();
        let row_keys = Binding::new(
            messages.with(|messages| row_keys_from_messages_with_collapsed(messages, &collapsed)),
        );
        let (list, virtual_control) = build_list(row_keys.clone(), store.clone(), &config);
        let messages_observer = messages.dirty_observer();
        Self {
            store,
            messages,
            message_ids,
            row_keys,
            list,
            virtual_control,
            config,
            on_load_more: None,
            load_more_armed: true,
            auto_scroll: true,
            follow_tail: true,
            suppress_auto_scroll_once: false,
            pending_scroll_to_bottom: has_initial_messages,
            search: SearchState::default(),
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

    /// Fraction (1..=100 percent of list width) a message bubble may occupy.
    /// 100 makes bubbles span the full width (no alignment spacer). Default 75.
    pub fn bubble_width_percent(mut self, percent: u16) -> Self {
        self.config.bubble_width_percent = percent.clamp(1, 100);
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

    pub fn on_edit_and_resubmit<F>(mut self, input: &ChatInputHandle, callback: F) -> Self
    where
        F: Fn(EditAndResubmitEvent) + Send + Sync + 'static,
    {
        let controller = EditAndResubmitController::new(
            self.store.clone(),
            input.draft_binding(),
            Arc::new(callback),
        );
        let submit_controller = controller.clone();
        input.set_text_submit_interceptor(ChatTextSubmitInterceptor::new(move |text| {
            submit_controller.submit_edit(text)
        }));
        self.config.edit_and_resubmit = Some(controller);
        self.rebuild_list();
        self
    }

    pub fn with_quote_replies(mut self, input: &ChatInputHandle) -> Self {
        self.config.quote_replies = Some(QuoteReplyController::new(input.references_binding()));
        self.rebuild_list();
        self
    }

    pub fn on_plan_decision<F>(mut self, callback: F) -> Self
    where
        F: Fn(PlanDecisionEvent) + Send + Sync + 'static,
    {
        self.config.on_plan_decision = Some(Arc::new(callback));
        self.rebuild_list();
        self
    }

    /// Register per-message action callbacks.
    ///
    /// Retry/Regenerate callbacks run after the target assistant turn and its
    /// suffix have been truncated, so hosts should use the action as a signal
    /// to start a fresh generation from the retained prefix.
    pub fn on_message_action<F>(mut self, callback: F) -> Self
    where
        F: Fn(MessageAction) + Send + Sync + 'static,
    {
        self.config.on_message_action = Some(Arc::new(callback));
        self.rebuild_list();
        self
    }

    pub fn on_cancel<F>(mut self, callback: F) -> Self
    where
        F: Fn(ChatMessageId) + Send + Sync + 'static,
    {
        self.config.on_cancel = Some(Arc::new(callback));
        self.rebuild_list();
        self
    }

    pub(crate) fn streaming_cancel_controller(&self) -> Option<StreamingCancelController> {
        self.config
            .on_cancel
            .clone()
            .map(|callback| StreamingCancelController::new(self.store.clone(), callback))
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

    pub fn is_following_tail(&self) -> bool {
        self.follow_tail
    }

    #[cfg(test)]
    fn realized_row_count(&self) -> usize {
        self.virtual_control.realized_row_count.get()
    }

    fn rebuild_list(&mut self) {
        self.row_keys.set(self.current_row_keys());
        let (list, virtual_control) =
            build_list(self.row_keys.clone(), self.store.clone(), &self.config);
        self.list = list;
        self.virtual_control = virtual_control;
    }

    fn current_row_keys(&self) -> Vec<ChatRowKey> {
        let collapsed = self.config.collapsed_turns.get();
        self.messages
            .with(|messages| row_keys_from_messages_with_collapsed(messages, &collapsed))
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
        self.virtual_control
            .preserve_scroll_y_after_next_layout(previous_content_h, previous_scroll_y);
        true
    }

    fn track_message_changes(&mut self) {
        if !self.messages.check_dirty(&mut self.messages_observer) {
            return;
        }
        let next_message_ids = self
            .messages
            .with(|messages| message_ids_from_messages(messages));
        self.prune_collapsed_turns(&next_message_ids);
        let next_row_keys = self.current_row_keys();
        let branch_rewritten = message_ids_rewrite_branch(&self.message_ids, &next_message_ids);
        self.message_ids = next_message_ids;
        self.row_keys.set(next_row_keys);
        if self.search.active {
            self.suppress_auto_scroll_once = false;
            self.refresh_search_matches();
            self.queue_active_search_match_scroll();
            return;
        }
        if self.suppress_auto_scroll_once {
            self.suppress_auto_scroll_once = false;
            return;
        }
        if self.auto_scroll && (self.follow_tail || branch_rewritten) {
            if branch_rewritten {
                self.follow_tail = true;
            }
            self.pending_scroll_to_bottom = true;
        }
    }

    fn prune_collapsed_turns(&self, live_message_ids: &[ChatMessageId]) {
        let live = live_message_ids.iter().copied().collect::<HashSet<_>>();
        let mut collapsed = self.config.collapsed_turns.get();
        let before = collapsed.len();
        collapsed.retain(|message_id| live.contains(message_id));
        if collapsed.len() != before {
            self.config.collapsed_turns.set(collapsed);
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
        self.virtual_control.scroll_to_bottom_on_next_layout();
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
                self.config.bubble_width_percent,
            ));
    }

    fn start_search(&mut self) -> EventResult {
        self.search.active = true;
        self.search.query.clear();
        self.search.restore_scroll = Some(self.list.scroll_offset());
        self.refresh_search_matches();
        EventResult::changed()
    }

    fn exit_search(&mut self) -> EventResult {
        if !self.search.active {
            return EventResult::ignored();
        }
        let restore = self.search.restore_scroll.take();
        self.search = SearchState::default();
        if let Some((x, y)) = restore {
            self.list.set_scroll_offset(x, y);
            self.sync_follow_tail_from_scroll();
        }
        EventResult::changed()
    }

    fn refresh_search_matches(&mut self) {
        let row_keys = self.row_keys.get();
        let query = self.search.query.clone();
        self.search.matches = self
            .messages
            .with(|messages| collect_search_matches(messages, &row_keys, &query, &self.config));
        self.search.active_index = (!self.search.matches.is_empty()).then_some(0);
    }

    fn update_search_query(&mut self, query: String) -> EventResult {
        if self.search.query == query {
            return EventResult::consumed();
        }
        self.search.query = query;
        self.refresh_search_matches();
        self.queue_active_search_match_scroll();
        EventResult::changed()
    }

    fn move_search_match(&mut self, forward: bool) -> EventResult {
        let len = self.search.matches.len();
        if len == 0 {
            return EventResult::consumed();
        }
        let current = self
            .search
            .active_index
            .unwrap_or(0)
            .min(len.saturating_sub(1));
        let next = if forward {
            current.saturating_add(1) % len
        } else if current == 0 {
            len.saturating_sub(1)
        } else {
            current.saturating_sub(1)
        };
        self.search.active_index = Some(next);
        self.queue_active_search_match_scroll();
        EventResult::changed()
    }

    fn queue_active_search_match_scroll(&mut self) {
        let Some(row_id) = self
            .search
            .active_match()
            .map(|search_match| search_match.row_id)
        else {
            return;
        };
        self.virtual_control.scroll_to_row_on_next_layout(row_id);
        self.pending_scroll_to_bottom = false;
        self.follow_tail = false;
        self.load_more_armed = true;
    }

    fn handle_search_event(&mut self, event: &Event) -> Option<EventResult> {
        if is_search_shortcut(event) {
            if self.search.active {
                return Some(self.move_search_match(true));
            }
            return Some(self.start_search());
        }

        if !self.search.active {
            return None;
        }

        let Event::Key(key) = event else {
            return None;
        };
        if matches!(key.kind, KeyEventKind::Release) {
            return Some(EventResult::ignored());
        }

        match key.code {
            KeyCode::Esc => Some(self.exit_search()),
            KeyCode::Enter | KeyCode::Down | KeyCode::Tab | KeyCode::PageDown => {
                Some(self.move_search_match(true))
            }
            KeyCode::Up | KeyCode::BackTab | KeyCode::PageUp => Some(self.move_search_match(false)),
            KeyCode::Backspace => {
                let mut query = self.search.query.clone();
                query.pop();
                Some(self.update_search_query(query))
            }
            KeyCode::Char('u' | 'U') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(self.update_search_query(String::new()))
            }
            KeyCode::Char(ch) if search_input_modifiers_allow_text(key.modifiers) => {
                let mut query = self.search.query.clone();
                query.push(ch);
                Some(self.update_search_query(query))
            }
            _ => Some(EventResult::consumed()),
        }
    }

    fn draw_search_state(&self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if !self.search.active || area.width == 0 || area.height == 0 {
            return;
        }

        if !self.search.query.is_empty() {
            apply_search_highlights(
                frame.buffer_mut(),
                area,
                &self.search.query,
                search_match_style(),
            );
        }

        let label = fit_to_display_width(&self.search.status_label(), area.width as usize);
        let status_area = Rect {
            y: area.y.saturating_add(area.height.saturating_sub(1)),
            height: 1,
            ..area
        };
        frame.render_widget(
            Paragraph::new(Line::styled(label, ctx.theme.widget.focused)),
            status_area,
        );
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
            "bubble_width_percent",
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
            "bubble_width_percent" => {
                Some(ComponentValue::U64(self.config.bubble_width_percent as u64))
            }
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
            "bubble_width_percent" => {
                let percent = <u16 as ComponentValueCodec>::from_component_value(value, name)?;
                self.config.bubble_width_percent = percent.clamp(1, 100);
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
        self.draw_search_state(frame, area, ctx);
    }
}

fn estimated_bubble_content_width(
    area_width: u16,
    padding: EdgeInsets,
    bubble_width_percent: u16,
) -> Option<u16> {
    let list_width = area_width.saturating_sub(padding.sum_horizontal());
    if list_width == 0 {
        return None;
    }
    let percent = bubble_width_percent.clamp(1, 100) as u32;
    Some(
        (((list_width as u32) * percent) / 100)
            .max(1)
            .min(u16::MAX as u32) as u16,
    )
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

impl ::atto_ui::composable::FocusNav for ChatMessageList {
    fn focused_child(&self) -> Option<ComponentId> {
        self.list.focused_child()
    }

    fn is_focusable(&self) -> bool {
        self.list.is_focusable()
    }

    fn focus_first(&mut self) -> bool {
        self.list.focus_first()
    }

    fn focus_last(&mut self) -> bool {
        self.list.focus_last()
    }
}

impl ::atto_ui::composable::DynamicTree for ChatMessageList {}

impl ::atto_ui::composable::EventHandling for ChatMessageList {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.track_message_changes();
        if let Some(res) = self.handle_search_event(event) {
            return res;
        }
        let before_scroll_y = self.list.scroll_offset().1;
        let mut res = self.list.handle_event(event, ctx);
        if self.list.scroll_offset().1 != before_scroll_y {
            self.sync_follow_tail_from_scroll();
        }
        if self.maybe_trigger_load_more() && matches!(res.action, ComponentAction::None) {
            self.sync_follow_tail_from_scroll();
            res = EventResult::changed();
        }
        if !res.is_consumed()
            && is_escape_press(event)
            && self
                .streaming_cancel_controller()
                .is_some_and(|controller| controller.request_current())
        {
            res = EventResult::changed();
        }
        res
    }
}

fn build_list(
    row_keys: Binding<Vec<ChatRowKey>>,
    store: ChatMessageStore,
    config: &ChatMessageListConfig,
) -> (ScrollContainer, VirtualChatRowsControl) {
    let row_config = ChatMessageRowConfig {
        store: store.clone(),
        wrap_width: config.wrap_width,
        responsive_wrap_width: config.responsive_wrap_width.clone(),
        in_progress_suffix: config.in_progress_suffix.clone(),
        show_timestamps: config.show_timestamps,
        bubble_width_percent: config.bubble_width_percent,
        collapsed_turns: config.collapsed_turns.clone(),
        on_open_artifact: config.on_open_artifact.clone(),
        on_approve: config.on_approve.clone(),
        on_edit_decision: config.on_edit_decision.clone(),
        edit_and_resubmit: config.edit_and_resubmit.clone(),
        quote_replies: config.quote_replies.clone(),
        on_plan_decision: config.on_plan_decision.clone(),
        on_message_action: config.on_message_action.clone(),
        on_cancel: config.on_cancel.clone(),
    };
    let control = VirtualChatRowsControl::new();
    let content = VirtualChatRowsContent::new(
        row_keys,
        store,
        row_config,
        config.spacing.clone(),
        control.clone(),
    );
    let list = ScrollContainer::new(Box::new(content))
        .with_padding(config.padding.clone())
        .with_scroll_config(config.scroll_config.clone());
    (list, control)
}

fn latest_streaming_message_id(store: &ChatMessageStore) -> Option<ChatMessageId> {
    store
        .messages()
        .iter()
        .rev()
        .find(|message| message.status.is_streaming())
        .map(|message| message.id)
}

fn is_escape_press(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(KeyEvent {
            code: KeyCode::Esc,
            kind,
            ..
        }) if !matches!(kind, KeyEventKind::Release)
    )
}

fn is_search_shortcut(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(KeyEvent {
            code: KeyCode::Char('r' | 'R'),
            modifiers,
            kind,
            ..
        }) if modifiers.contains(KeyModifiers::CONTROL) && !matches!(kind, KeyEventKind::Release)
    )
}

fn search_input_modifiers_allow_text(modifiers: KeyModifiers) -> bool {
    !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
}

fn search_match_style() -> Style {
    Style::default().fg(Color::Black).bg(Color::Yellow)
}

fn apply_search_highlights(buf: &mut Buffer, area: Rect, query: &str, style: Style) {
    if query.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }

    for dy in 0..area.height {
        let row = rendered_row_from_buffer(buf, area, dy);
        for (start, end) in search_match_display_ranges(&row.text, query) {
            for (start, end) in selected_cell_ranges_for_line(&row.text, start, end) {
                for dx in start..end.min(area.width) {
                    let x = area.x.saturating_add(dx);
                    let y = area.y.saturating_add(dy);
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_style(style);
                    }
                }
            }
        }
    }
}

fn search_match_display_ranges(text: &str, query: &str) -> Vec<(u16, u16)> {
    find_case_insensitive_byte_ranges(text, query)
        .into_iter()
        .filter_map(|(start, end)| {
            let start_col = UnicodeWidthStr::width(&text[..start]).min(u16::MAX as usize) as u16;
            let end_col = UnicodeWidthStr::width(&text[..end]).min(u16::MAX as usize) as u16;
            (start_col < end_col).then_some((start_col, end_col))
        })
        .collect()
}

fn find_case_insensitive_byte_ranges(text: &str, query: &str) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut byte_idx = 0usize;
    while byte_idx < text.len() {
        if !text.is_char_boundary(byte_idx) {
            byte_idx = byte_idx.saturating_add(1);
            continue;
        }
        if let Some(end) = case_insensitive_match_end(text, byte_idx, query) {
            ranges.push((byte_idx, end));
            byte_idx = end.max(byte_idx.saturating_add(1));
        } else {
            byte_idx = next_char_boundary(text, byte_idx);
        }
    }
    ranges
}

fn case_insensitive_match_end(text: &str, start: usize, query: &str) -> Option<usize> {
    let mut text_chars = text[start..].chars();
    let mut end = start;
    for query_ch in query.chars() {
        let text_ch = text_chars.next()?;
        if text_ch != query_ch && !text_ch.eq_ignore_ascii_case(&query_ch) {
            return None;
        }
        end = end.saturating_add(text_ch.len_utf8());
    }
    Some(end)
}

fn next_char_boundary(text: &str, start: usize) -> usize {
    text[start..]
        .chars()
        .next()
        .map_or(text.len(), |ch| start.saturating_add(ch.len_utf8()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VirtualScrollAdjustment {
    ToBottom,
    ToRow {
        row_id: ChatRowId,
    },
    ToOffset {
        x: u16,
        y: u16,
    },
    PreserveYAfterContentHeightChange {
        previous_content_height: u16,
        previous_scroll_y: u16,
    },
}

#[derive(Clone)]
struct VirtualChatRowsControl {
    pending_scroll_adjustment: Binding<Option<VirtualScrollAdjustment>>,
    realized_row_count: Binding<usize>,
}

impl VirtualChatRowsControl {
    fn new() -> Self {
        Self {
            pending_scroll_adjustment: Binding::new(None),
            realized_row_count: Binding::new(0),
        }
    }

    fn scroll_to_bottom_on_next_layout(&self) {
        self.pending_scroll_adjustment
            .set(Some(VirtualScrollAdjustment::ToBottom));
    }

    fn scroll_to_row_on_next_layout(&self, row_id: ChatRowId) {
        self.pending_scroll_adjustment
            .set(Some(VirtualScrollAdjustment::ToRow { row_id }));
    }

    fn scroll_to_offset_on_next_layout(&self, x: u16, y: u16) {
        self.pending_scroll_adjustment
            .set(Some(VirtualScrollAdjustment::ToOffset { x, y }));
    }

    fn preserve_scroll_y_after_next_layout(
        &self,
        previous_content_height: u16,
        previous_scroll_y: u16,
    ) {
        self.pending_scroll_adjustment.set(Some(
            VirtualScrollAdjustment::PreserveYAfterContentHeightChange {
                previous_content_height,
                previous_scroll_y,
            },
        ));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CachedRowHeight {
    height: u16,
    version: u64,
}

#[derive(Clone, Debug)]
struct VirtualRowLayout {
    id: ChatRowId,
    key: ChatRowKey,
    y: u16,
    height: u16,
}

struct CachedVirtualRow {
    key: ChatRowKey,
    row: ChatMessageRow,
    mouse_coordinate_space: MouseCoordinateSpace,
}

struct VirtualChatRowsContent {
    row_keys: Binding<Vec<ChatRowKey>>,
    store: ChatMessageStore,
    config: ChatMessageRowConfig,
    spacing: Binding<u16>,
    control: VirtualChatRowsControl,
    row_cache: HashMap<ChatRowId, CachedVirtualRow>,
    height_cache: HashMap<ChatRowId, CachedRowHeight>,
    last_layout: Vec<VirtualRowLayout>,
    focused_row: Option<ChatRowId>,
    captured_row: Option<ChatRowId>,
    turn_restore_offsets: HashMap<ChatMessageId, (u16, u16)>,
    last_area: Option<Rect>,
}

impl VirtualChatRowsContent {
    fn new(
        row_keys: Binding<Vec<ChatRowKey>>,
        store: ChatMessageStore,
        config: ChatMessageRowConfig,
        spacing: Binding<u16>,
        control: VirtualChatRowsControl,
    ) -> Self {
        Self {
            row_keys,
            store,
            config,
            spacing,
            control,
            row_cache: HashMap::new(),
            height_cache: HashMap::new(),
            last_layout: Vec::new(),
            focused_row: None,
            captured_row: None,
            turn_restore_offsets: HashMap::new(),
            last_area: None,
        }
    }

    fn sync_responsive_width(&self, viewport_width: u16) {
        self.config
            .responsive_wrap_width
            .set(estimated_bubble_content_width(
                viewport_width,
                EdgeInsets::ZERO,
                self.config.bubble_width_percent,
            ));
    }

    fn sync_turn_collapse_change(
        &mut self,
        previous: &HashSet<ChatMessageId>,
        scroll_offset: (u16, u16),
    ) {
        let current = self.config.collapsed_turns.get();
        if &current == previous {
            return;
        }

        let messages = self.store.messages();
        self.row_keys
            .set(row_keys_from_messages_with_collapsed(&messages, &current));
        if let Some(message_id) = changed_turn_id(previous, &current) {
            if current.contains(&message_id) {
                self.turn_restore_offsets.insert(message_id, scroll_offset);
                self.control
                    .scroll_to_row_on_next_layout(ChatRowId::Header(message_id));
            } else if let Some((x, y)) = self.turn_restore_offsets.remove(&message_id) {
                self.control.scroll_to_offset_on_next_layout(x, y);
            } else {
                self.control
                    .scroll_to_row_on_next_layout(ChatRowId::Header(message_id));
            }
            self.focused_row = Some(ChatRowId::Header(message_id));
        }
    }

    fn rebuild_layout(&mut self, viewport_width: u16) -> (u16, u16) {
        self.sync_responsive_width(viewport_width);
        let keys = self.row_keys.get();
        let live_ids = keys.iter().map(ChatRowKey::id).collect::<HashSet<_>>();
        self.row_cache.retain(|id, _| live_ids.contains(id));
        self.height_cache.retain(|id, _| live_ids.contains(id));
        if self
            .focused_row
            .is_some_and(|focused| !live_ids.contains(&focused))
        {
            self.focused_row = None;
        }
        if self
            .captured_row
            .is_some_and(|captured| !live_ids.contains(&captured))
        {
            self.captured_row = None;
        }

        let spacing = self.spacing.get();
        let mut y = 0u16;
        let mut rows = Vec::with_capacity(keys.len());
        for (idx, key) in keys.into_iter().enumerate() {
            if idx > 0 {
                y = y.saturating_add(spacing);
            }
            let id = key.id();
            let height = self.row_height(&key, viewport_width).max(1);
            rows.push(VirtualRowLayout { id, key, y, height });
            y = y.saturating_add(height);
        }
        self.last_layout = rows;
        self.control.realized_row_count.set(self.row_cache.len());
        (viewport_width, y)
    }

    fn row_version(&self, key: &ChatRowKey) -> u64 {
        match key.row_ref() {
            ChatRowRef::Header {
                message_id,
                collapsed,
            } => self
                .store
                .message_version(message_id)
                .saturating_mul(2)
                .saturating_add(u64::from(collapsed)),
            ChatRowRef::Block(block_id) | ChatRowRef::PendingToolResult(block_id) => {
                self.store.block_version(block_id)
            }
        }
    }

    fn row_height(&self, key: &ChatRowKey, viewport_width: u16) -> u16 {
        let version = self.row_version(key);
        if let Some(cached) = self.height_cache.get(&key.id())
            && cached.version == version
        {
            return cached.height;
        }
        estimate_row_height(key, &self.store, &self.config, viewport_width)
    }

    fn apply_pending_scroll_adjustment(&self, host: &mut ScrollContainerHost) {
        let Some(adjustment) = self.control.pending_scroll_adjustment.get() else {
            return;
        };
        let scroll = host.scroll_offset();
        let viewport = host.viewport_size();
        let content = host.content_size();
        let (target_x, target_y) = match adjustment {
            VirtualScrollAdjustment::ToBottom => (scroll.x, content.1.saturating_sub(viewport.1)),
            VirtualScrollAdjustment::ToRow { row_id } => {
                let Some(row) = self.row_layout_by_id(row_id) else {
                    self.control.pending_scroll_adjustment.set(None);
                    return;
                };
                (scroll.x, centered_scroll_y_for_row(&row, viewport.1))
            }
            VirtualScrollAdjustment::ToOffset { x, y } => {
                let max_x = content.0.saturating_sub(viewport.0);
                let max_y = content.1.saturating_sub(viewport.1);
                (x.min(max_x), y.min(max_y))
            }
            VirtualScrollAdjustment::PreserveYAfterContentHeightChange {
                previous_content_height,
                previous_scroll_y,
            } => {
                let inserted_height = content.1.saturating_sub(previous_content_height);
                (scroll.x, previous_scroll_y.saturating_add(inserted_height))
            }
        };
        host.set_scroll_offset(target_x, target_y);
        self.control.pending_scroll_adjustment.set(None);
    }

    fn visible_row_ids(&self, scroll_y: u16, viewport_h: u16) -> HashSet<ChatRowId> {
        let start = scroll_y.saturating_sub(1);
        let end = scroll_y.saturating_add(viewport_h).saturating_add(1);
        self.last_layout
            .iter()
            .filter(|row| row_intersects(row.y, row.height, start, end))
            .map(|row| row.id)
            .collect()
    }

    fn realize_visible_rows(&mut self, scroll_y: u16, viewport_h: u16) -> bool {
        let visible_ids = self.visible_row_ids(scroll_y, viewport_h);
        if self
            .focused_row
            .is_some_and(|focused| !visible_ids.contains(&focused))
        {
            self.focused_row = None;
        }
        self.row_cache
            .retain(|id, _| visible_ids.contains(id) || self.captured_row == Some(*id));

        let mut height_changed = false;
        let visible_layouts = self
            .last_layout
            .iter()
            .filter(|row| visible_ids.contains(&row.id))
            .cloned()
            .collect::<Vec<_>>();
        for layout in visible_layouts {
            self.ensure_row_cached(&layout.key);
            let version = self.row_version(&layout.key);
            if let Some(cached) = self.row_cache.get(&layout.id) {
                let measured = cached.row.desired_height().unwrap_or(1).max(1);
                let previous = self.height_cache.insert(
                    layout.id,
                    CachedRowHeight {
                        height: measured,
                        version,
                    },
                );
                height_changed |= previous.is_none_or(|cached| cached.height != measured);
            }
        }

        self.control.realized_row_count.set(self.row_cache.len());
        height_changed
    }

    fn ensure_row_cached(&mut self, key: &ChatRowKey) {
        let id = key.id();
        let needs_rebuild = self
            .row_cache
            .get(&id)
            .is_none_or(|cached| cached.key != *key);
        if !needs_rebuild {
            return;
        }
        self.row_cache.insert(
            id,
            CachedVirtualRow {
                key: key.clone(),
                row: ChatMessageRow::new(key.clone(), self.store.clone(), self.config.clone()),
                mouse_coordinate_space: MouseCoordinateSpace::Local,
            },
        );
    }

    fn focus_visible_row(&mut self, forward: bool, scroll_y: u16, viewport_h: u16) -> bool {
        let visible_ids = self.visible_row_ids(scroll_y, viewport_h);
        let mut layouts = self
            .last_layout
            .iter()
            .filter(|row| visible_ids.contains(&row.id))
            .cloned()
            .collect::<Vec<_>>();
        if !forward {
            layouts.reverse();
        }
        for layout in layouts {
            self.ensure_row_cached(&layout.key);
            let Some(cached) = self.row_cache.get_mut(&layout.id) else {
                continue;
            };
            if !cached.row.is_focusable() {
                continue;
            }
            let focused = if forward {
                cached.row.focus_first()
            } else {
                cached.row.focus_last()
            };
            if focused {
                self.focused_row = Some(layout.id);
                return true;
            }
        }
        false
    }

    fn row_at_content_y(&self, y: u16) -> Option<VirtualRowLayout> {
        self.last_layout
            .iter()
            .find(|row| y >= row.y && y < row.y.saturating_add(row.height))
            .cloned()
    }

    fn row_layout_by_id(&self, id: ChatRowId) -> Option<VirtualRowLayout> {
        self.last_layout.iter().find(|row| row.id == id).cloned()
    }

    fn handle_mouse_event(
        &mut self,
        event: &MouseEvent,
        ctx: ScrollContentContext<'_>,
    ) -> EventResult {
        let content_y = event.row.saturating_add(ctx.info.scroll_offset.y);
        let Some(layout) = self
            .captured_row
            .and_then(|id| self.row_layout_by_id(id))
            .or_else(|| self.row_at_content_y(content_y))
        else {
            return EventResult::ignored();
        };
        self.ensure_row_cached(&layout.key);
        let Some(cached) = self.row_cache.get_mut(&layout.id) else {
            return EventResult::ignored();
        };

        let focus_changed = matches!(event.kind, MouseEventKind::Down(_))
            && cached.row.is_focusable()
            && self.focused_row != Some(layout.id);
        if focus_changed {
            self.focused_row = Some(layout.id);
            let _ = cached.row.focus_first();
        }

        let mouse_space = cached.mouse_coordinate_space;
        let (column, row) = match mouse_space {
            MouseCoordinateSpace::Absolute => {
                self.last_area.map_or((event.column, event.row), |area| {
                    (
                        area.x.saturating_add(event.column),
                        area.y.saturating_add(event.row),
                    )
                })
            }
            MouseCoordinateSpace::Local => (event.column, content_y.saturating_sub(layout.y)),
        };
        let row_event = Event::Mouse(MouseEvent {
            column,
            row,
            ..*event
        });
        let row_ctx = virtual_row_ctx(self.focused_row, layout.id, ctx, mouse_space);
        let res = cached.row.handle_event(&row_event, row_ctx);
        match res.capture {
            Capture::Request => self.captured_row = Some(layout.id),
            Capture::Release => self.captured_row = None,
            Capture::None => {}
        }
        if res.is_consumed() {
            return res;
        }
        if focus_changed {
            EventResult::consumed()
        } else {
            EventResult::ignored()
        }
    }

    fn handle_keyboard_event(
        &mut self,
        event: &Event,
        ctx: ScrollContentContext<'_>,
    ) -> EventResult {
        let scroll_y = ctx.info.scroll_offset.y;
        let viewport_h = ctx.info.viewport_size.1;
        if let Some(forward) = copy_target_tab_direction(event) {
            if self.focus_visible_row(forward, scroll_y, viewport_h) {
                return EventResult::consumed();
            }
            return EventResult::ignored();
        }

        if self.focused_row.is_none() {
            let _ = self.focus_visible_row(true, scroll_y, viewport_h);
        }
        let Some(focused) = self.focused_row else {
            return EventResult::ignored();
        };
        let Some(cached) = self.row_cache.get_mut(&focused) else {
            self.focused_row = None;
            return EventResult::ignored();
        };
        let row_ctx = virtual_row_ctx(
            self.focused_row,
            focused,
            ctx,
            cached.mouse_coordinate_space,
        );
        cached.row.handle_event(event, row_ctx)
    }
}

impl ScrollContent for VirtualChatRowsContent {
    fn is_focusable(&self) -> bool {
        self.store.messages().iter().any(has_turn_collapse_control)
            || self.config.on_open_artifact.is_some()
            || self.config.on_approve.is_some()
            || self.config.on_edit_decision.is_some()
            || self.config.edit_and_resubmit.is_some()
            || self.config.quote_replies.is_some()
            || self.config.on_message_action.is_some()
            || self.config.on_cancel.is_some()
    }

    fn content_size(&mut self, viewport: (u16, u16), _ctx: ScrollContentContext<'_>) -> (u16, u16) {
        self.rebuild_layout(viewport.0)
    }

    fn on_scrollbars(&mut self, _ctx: ScrollContentContext<'_>, host: &mut ScrollContainerHost) {
        self.apply_pending_scroll_adjustment(host);
    }

    fn handle_event(
        &mut self,
        event: &Event,
        ctx: ScrollContentContext<'_>,
        _host: &mut ScrollContainerHost,
    ) -> EventResult {
        self.rebuild_layout(ctx.info.viewport_size.0);
        let _ = self.realize_visible_rows(ctx.info.scroll_offset.y, ctx.info.viewport_size.1);
        let previous_collapsed = self.config.collapsed_turns.get();
        let result = match event {
            Event::Mouse(mouse) => self.handle_mouse_event(mouse, ctx),
            _ => self.handle_keyboard_event(event, ctx),
        };
        self.sync_turn_collapse_change(
            &previous_collapsed,
            (ctx.info.scroll_offset.x, ctx.info.scroll_offset.y),
        );
        result
    }

    fn draw(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: ScrollContentContext<'_>,
        host: &mut ScrollContainerHost,
    ) {
        self.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let scroll_y = ctx.info.scroll_offset.y;
        let height_changed = self.realize_visible_rows(scroll_y, area.height);
        if height_changed {
            let content_size = self.rebuild_layout(area.width);
            host.set_content_size(content_size);
            let _ = self.realize_visible_rows(scroll_y, area.height);
        }

        let visible_ids = self.visible_row_ids(scroll_y, area.height);
        let layouts = self
            .last_layout
            .iter()
            .filter(|row| visible_ids.contains(&row.id))
            .cloned()
            .collect::<Vec<_>>();
        for layout in layouts {
            let mouse_space = if row_fully_visible(&layout, scroll_y, area.height) {
                MouseCoordinateSpace::Absolute
            } else {
                MouseCoordinateSpace::Local
            };
            let row_ctx = virtual_row_ctx(self.focused_row, layout.id, ctx, mouse_space);
            let Some(cached) = self.row_cache.get_mut(&layout.id) else {
                continue;
            };
            cached.mouse_coordinate_space = mouse_space;
            draw_virtual_row(frame, &mut cached.row, area, &layout, scroll_y, row_ctx);
        }
        self.control.realized_row_count.set(self.row_cache.len());
    }
}

fn virtual_row_ctx<'a>(
    focused_row: Option<ChatRowId>,
    row_id: ChatRowId,
    ctx: ScrollContentContext<'a>,
    mouse_coordinate_space: MouseCoordinateSpace,
) -> ComponentContext<'a> {
    ComponentContext {
        theme: ctx.component.theme,
        window_id: ctx.component.window_id,
        is_focused: ctx.component.is_focused && focused_row == Some(row_id),
        scrollbar_host: ctx.component.scrollbar_host.for_child(),
        tab_mode: ctx.component.tab_mode.for_child(),
        mouse_coordinate_space,
        drag: None,
    }
}

fn row_fully_visible(row: &VirtualRowLayout, scroll_y: u16, viewport_h: u16) -> bool {
    row.y >= scroll_y && row.y.saturating_add(row.height) <= scroll_y.saturating_add(viewport_h)
}

fn row_intersects(row_y: u16, row_h: u16, visible_start: u16, visible_end: u16) -> bool {
    row_y < visible_end && row_y.saturating_add(row_h) > visible_start
}

fn centered_scroll_y_for_row(row: &VirtualRowLayout, viewport_h: u16) -> u16 {
    if viewport_h == 0 || row.height >= viewport_h {
        return row.y;
    }
    row.y
        .saturating_sub(viewport_h.saturating_sub(row.height) / 2)
}

fn estimate_row_height(
    key: &ChatRowKey,
    store: &ChatMessageStore,
    config: &ChatMessageRowConfig,
    viewport_width: u16,
) -> u16 {
    store
        .with_message(key.message_id(), |message| match key {
            ChatRowKey::Header { collapsed, .. } => {
                estimate_header_row_height(message, config, *collapsed)
            }
            ChatRowKey::Block { block_id, .. } => {
                let block = find_block(message, *block_id);
                estimate_block_row_height(block, config, viewport_width)
            }
            ChatRowKey::PendingToolResult { .. } => 2,
        })
        .unwrap_or_else(|| estimate_placeholder_row_height(key, config, viewport_width))
}

fn estimate_placeholder_row_height(
    key: &ChatRowKey,
    config: &ChatMessageRowConfig,
    viewport_width: u16,
) -> u16 {
    let message = key.placeholder();
    match key {
        ChatRowKey::Header { collapsed, .. } => {
            estimate_header_row_height(&message, config, *collapsed)
        }
        ChatRowKey::Block { block_id, .. } => {
            estimate_block_row_height(find_block(&message, *block_id), config, viewport_width)
        }
        ChatRowKey::PendingToolResult { .. } => 2,
    }
}

fn estimate_header_row_height(
    message: &ChatMessage,
    config: &ChatMessageRowConfig,
    collapsed: bool,
) -> u16 {
    let mut bubble_height = line_count(&turn_header_label_for_row(message, collapsed));
    if has_turn_action_row(message, config) {
        bubble_height = bubble_height.saturating_add(2);
    }

    if config.show_timestamps && message.meta.timestamp.is_some() {
        bubble_height.saturating_add(2)
    } else {
        bubble_height
    }
}

fn estimate_block_row_height(
    block: Option<&ChatBlock>,
    config: &ChatMessageRowConfig,
    viewport_width: u16,
) -> u16 {
    let bubble_width = estimated_bubble_content_width(
        viewport_width,
        EdgeInsets::ZERO,
        config.bubble_width_percent,
    )
    .unwrap_or(1);
    let mut height = match block {
        Some(ChatBlock::Text(text)) => estimate_markdown_height(
            &markdown_for_render(&text.markdown, text.streaming, config),
            bubble_width,
            0,
            config.wrap_width,
        ),
        Some(ChatBlock::Thinking(thinking)) => estimate_disclosure_height(
            !thinking.collapsed,
            estimate_markdown_height(&thinking.markdown, bubble_width, 2, config.wrap_width),
        ),
        Some(ChatBlock::Attachment(_))
        | Some(ChatBlock::Notice(_))
        | Some(ChatBlock::Artifact(_))
        | None => 1,
        Some(ChatBlock::ToolUse(tool)) => estimate_disclosure_height(
            !tool.collapsed,
            tool_use_details_desired_height(&ToolUseDetails::from(tool)),
        ),
        Some(ChatBlock::ToolResult(result)) => estimate_disclosure_height(
            !result.collapsed,
            estimate_tool_output_height(&result.output, config, bubble_width),
        ),
        Some(ChatBlock::Diff(diff)) => 1u16
            .saturating_add(line_count(&diff.diff.unified))
            .saturating_add(1),
        Some(ChatBlock::Plan(plan)) => plan
            .items
            .len()
            .max(1)
            .saturating_add(2)
            .min(u16::MAX as usize) as u16,
        Some(ChatBlock::Task(task)) => estimate_disclosure_height(
            !task.collapsed,
            task_details_desired_height(&TaskDetails::from(task)),
        ),
        Some(ChatBlock::Todo(todo)) => todo.items.len().max(1).min(u16::MAX as usize) as u16,
    };
    if block.is_some() && config.on_message_action.is_some() {
        height = height.saturating_add(2);
    }
    height.max(1)
}

fn estimate_disclosure_height(expanded: bool, child_height: u16) -> u16 {
    if expanded {
        1u16.saturating_add(child_height.max(1))
    } else {
        1
    }
}

fn estimate_tool_output_height(
    output: &ToolOutput,
    config: &ChatMessageRowConfig,
    bubble_width: u16,
) -> u16 {
    match output {
        ToolOutput::Ansi(text) => {
            let lines = ansi_sgr_lines(text, Style::default()).len();
            if lines > ANSI_OUTPUT_TAIL_LINES {
                ANSI_OUTPUT_TAIL_LINES.saturating_add(1) as u16
            } else {
                lines.max(1).min(u16::MAX as usize) as u16
            }
        }
        ToolOutput::Markdown(markdown) => {
            estimate_markdown_height(markdown, bubble_width, 2, config.wrap_width)
        }
        ToolOutput::Diff(diff) => line_count(&diff.unified),
    }
}

fn estimate_markdown_height(
    markdown: &str,
    bubble_width: u16,
    indent: u16,
    max_width: Option<u16>,
) -> u16 {
    let available = bubble_width.saturating_sub(indent).max(1);
    let width = apply_markdown_width_cap(available, max_width).max(1) as usize;
    let mut total = 0usize;
    for line in markdown.split('\n') {
        let display_width = UnicodeWidthStr::width(line);
        total = total.saturating_add(display_width.div_ceil(width).max(1));
    }
    total.max(1).min(u16::MAX as usize) as u16
}

fn draw_virtual_row(
    frame: &mut Frame<'_>,
    row: &mut ChatMessageRow,
    viewport: Rect,
    layout: &VirtualRowLayout,
    scroll_y: u16,
    ctx: ComponentContext<'_>,
) {
    let row_bottom = layout.y.saturating_add(layout.height);
    let viewport_bottom = scroll_y.saturating_add(viewport.height);
    if row_bottom <= scroll_y || layout.y >= viewport_bottom {
        return;
    }

    let source_y = scroll_y.saturating_sub(layout.y);
    let visible_h = row_bottom
        .min(viewport_bottom)
        .saturating_sub(layout.y.max(scroll_y));
    if visible_h == 0 {
        return;
    }
    let dest_y = viewport.y.saturating_add(layout.y.saturating_sub(scroll_y));
    let full_area = Rect::new(0, 0, viewport.width, layout.height);
    let dest = Rect {
        x: viewport.x,
        y: dest_y,
        width: viewport.width,
        height: visible_h,
    };
    if source_y == 0 && visible_h == layout.height {
        row.draw(frame, dest, ctx);
    } else {
        draw_component_region_local(
            frame,
            row,
            full_area,
            Rect::new(0, source_y, viewport.width, visible_h),
            dest,
            ctx,
        );
    }
}

struct VirtualOffscreenBackend {
    buffer: Buffer,
    cursor_visible: bool,
    cursor_pos: Position,
}

impl VirtualOffscreenBackend {
    fn new(width: u16, height: u16) -> Self {
        Self {
            buffer: Buffer::empty(Rect::new(0, 0, width, height)),
            cursor_visible: false,
            cursor_pos: Position::new(0, 0),
        }
    }
}

impl Backend for VirtualOffscreenBackend {
    type Error = Infallible;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a BufferCell)>,
    {
        for (x, y, cell) in content {
            if x < self.buffer.area.width && y < self.buffer.area.height {
                self.buffer[(x, y)] = cell.clone();
            }
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.cursor_visible = false;
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.cursor_visible = true;
        Ok(())
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        Ok(self.cursor_pos)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.cursor_pos = position.into();
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.buffer = Buffer::empty(self.buffer.area);
        Ok(())
    }

    fn clear_region(&mut self, _clear_type: ClearType) -> Result<(), Self::Error> {
        self.clear()
    }

    fn size(&self) -> Result<TerminalSize, Self::Error> {
        Ok(self.buffer.area.as_size())
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        Ok(WindowSize {
            columns_rows: self.buffer.area.as_size(),
            pixels: TerminalSize::new(0, 0),
        })
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn draw_component_region_local(
    frame: &mut Frame<'_>,
    component: &mut dyn Component,
    component_area: Rect,
    source: Rect,
    dest: Rect,
    ctx: ComponentContext<'_>,
) {
    if component_area.width == 0
        || component_area.height == 0
        || source.width == 0
        || source.height == 0
        || dest.width == 0
        || dest.height == 0
    {
        return;
    }

    // Seed the offscreen buffer with the parent's current cells (the window surface
    // background) so that any area the row does not paint itself preserves that color
    // instead of copying back transparent/default cells over it.
    let mut seed = Vec::with_capacity(source.width as usize * source.height as usize);
    {
        let frame_buffer = frame.buffer_mut();
        for dy in 0..source.height.min(dest.height) {
            for dx in 0..source.width.min(dest.width) {
                let dst_x = dest.x.saturating_add(dx);
                let dst_y = dest.y.saturating_add(dy);
                if let Some(cell) = frame_buffer.cell((dst_x, dst_y)) {
                    seed.push((
                        source.x.saturating_add(dx),
                        source.y.saturating_add(dy),
                        cell.clone(),
                    ));
                }
            }
        }
    }

    let backend = VirtualOffscreenBackend::new(component_area.width, component_area.height);
    let mut terminal = ratatui::Terminal::new(backend).expect("create offscreen terminal");
    terminal
        .try_draw(|f| {
            let buf = f.buffer_mut();
            for (x, y, cell) in &seed {
                if let Some(dst) = buf.cell_mut((*x, *y)) {
                    *dst = cell.clone();
                }
            }
            component.draw(f, component_area, ctx);
            Ok::<(), Infallible>(())
        })
        .expect("draw clipped virtual row");
    let buffer = &terminal.backend().buffer;
    let frame_buffer = frame.buffer_mut();
    for dy in 0..source.height.min(dest.height) {
        for dx in 0..source.width.min(dest.width) {
            let src_x = source.x.saturating_add(dx);
            let src_y = source.y.saturating_add(dy);
            let dst_x = dest.x.saturating_add(dx);
            let dst_y = dest.y.saturating_add(dy);
            if src_x < buffer.area.width && src_y < buffer.area.height {
                frame_buffer[(dst_x, dst_y)] = buffer[(src_x, src_y)].clone();
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ChatRowKey {
    Header {
        message_id: ChatMessageId,
        collapsed: bool,
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
    Header {
        message_id: ChatMessageId,
        collapsed: bool,
    },
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
    Plan {
        decision: PlanDecision,
    },
    Task {
        title: String,
        collapsed: bool,
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
            ChatRowKey::Header { message_id, .. } => ChatRowId::Header(*message_id),
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
            ChatRowKey::Header { message_id, .. }
            | ChatRowKey::Block { message_id, .. }
            | ChatRowKey::PendingToolResult { message_id, .. } => *message_id,
        }
    }

    fn row_ref(&self) -> ChatRowRef {
        match self {
            ChatRowKey::Header {
                message_id,
                collapsed,
            } => ChatRowRef::Header {
                message_id: *message_id,
                collapsed: *collapsed,
            },
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
        ChatBlockKindTag::Plan { decision } => Some(ChatBlock::Plan(PlanBlock {
            id: block_id,
            items: Vec::new(),
            decision: *decision,
        })),
        ChatBlockKindTag::Task { title, collapsed } => Some(ChatBlock::Task(TaskBlock {
            id: block_id,
            title: title.clone(),
            status: TaskStatus::Pending,
            summary: String::new(),
            transcript: Vec::new(),
            collapsed: *collapsed,
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

fn message_ids_from_messages(messages: &[ChatMessage]) -> Vec<ChatMessageId> {
    messages.iter().map(|message| message.id).collect()
}

fn message_ids_rewrite_branch(previous: &[ChatMessageId], next: &[ChatMessageId]) -> bool {
    if previous.is_empty() || previous == next {
        return false;
    }
    if next.starts_with(previous) || next.ends_with(previous) {
        return false;
    }
    true
}

fn changed_turn_id(
    previous: &HashSet<ChatMessageId>,
    current: &HashSet<ChatMessageId>,
) -> Option<ChatMessageId> {
    previous.symmetric_difference(current).copied().next()
}

#[cfg(test)]
fn row_keys_from_messages(messages: &[ChatMessage]) -> Vec<ChatRowKey> {
    row_keys_from_messages_with_collapsed(messages, &HashSet::new())
}

fn row_keys_from_messages_with_collapsed(
    messages: &[ChatMessage],
    collapsed_turns: &HashSet<ChatMessageId>,
) -> Vec<ChatRowKey> {
    let result_candidates = collect_tool_result_candidates(messages);
    let mut paired_results = HashSet::new();
    let mut rows = Vec::new();
    let mut order = 0usize;

    for message in messages {
        let mut header_inserted = false;
        let turn_collapsed = collapsed_turns.contains(&message.id);
        for block in &message.blocks {
            let block_order = order;
            order = order.saturating_add(1);

            if paired_results.contains(&block.id()) {
                continue;
            }

            ensure_message_header(&mut rows, &mut header_inserted, message.id, turn_collapsed);
            if turn_collapsed {
                if let ChatBlock::ToolUse(tool) = block
                    && let Some(result) = matching_tool_result_candidate(
                        &result_candidates,
                        &paired_results,
                        &tool.call_id,
                        block_order,
                    )
                {
                    paired_results.insert(result.block_id);
                }
                continue;
            }

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
    collapsed: bool,
) {
    if *header_inserted {
        return;
    }
    rows.push(ChatRowKey::Header {
        message_id,
        collapsed,
    });
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
        ChatBlock::Plan(PlanBlock { decision, .. }) => ChatBlockKindTag::Plan {
            decision: *decision,
        },
        ChatBlock::Task(TaskBlock {
            title, collapsed, ..
        }) => ChatBlockKindTag::Task {
            title: title.clone(),
            collapsed: *collapsed,
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

fn collect_search_matches(
    messages: &[ChatMessage],
    row_keys: &[ChatRowKey],
    query: &str,
    config: &ChatMessageListConfig,
) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for key in row_keys {
        let text = searchable_text_for_row(messages, key, config);
        let count = find_case_insensitive_byte_ranges(&text, query).len();
        matches.extend((0..count).map(|_| SearchMatch { row_id: key.id() }));
    }
    matches
}

fn searchable_text_for_row(
    messages: &[ChatMessage],
    key: &ChatRowKey,
    config: &ChatMessageListConfig,
) -> String {
    let Some(message) = messages
        .iter()
        .find(|message| message.id == key.message_id())
    else {
        return String::new();
    };

    match key {
        ChatRowKey::Header { collapsed, .. } => turn_header_label_for_row(message, *collapsed),
        ChatRowKey::Block { block_id, .. } => find_block(message, *block_id)
            .map(|block| searchable_text_for_block(block, config))
            .unwrap_or_default(),
        ChatRowKey::PendingToolResult { call_id, .. } => pending_tool_result_title(call_id),
    }
}

fn searchable_text_for_block(block: &ChatBlock, config: &ChatMessageListConfig) -> String {
    match block {
        ChatBlock::Text(text) => searchable_markdown(&text.markdown, text.streaming, config),
        ChatBlock::Thinking(thinking) => {
            let mut lines = vec!["Thinking".to_string()];
            if !thinking.collapsed {
                lines.push(searchable_markdown(&thinking.markdown, false, config));
            }
            lines.join("\n")
        }
        ChatBlock::Attachment(attachment) => {
            attachment_label(&attachment.name, attachment.url.as_deref())
        }
        ChatBlock::ToolUse(tool) => searchable_tool_use(tool),
        ChatBlock::ToolResult(result) => searchable_tool_result(result),
        ChatBlock::Diff(diff) => format!("{}\n{}", diff_block_title(diff), diff.diff.unified),
        ChatBlock::Plan(plan) => {
            let mut lines = vec![plan_block_title(plan.decision)];
            lines.extend(plan_display_lines(&plan.items));
            lines.join("\n")
        }
        ChatBlock::Task(task) => {
            if task.collapsed {
                task_block_title(task)
            } else {
                task_display_lines(&TaskDetails::from(task)).join("\n")
            }
        }
        ChatBlock::Todo(todo) => todo_display_lines(&todo.items).join("\n"),
        ChatBlock::Notice(notice) => notice_label(notice.level, &notice.text),
        ChatBlock::Artifact(artifact) => {
            format!("Artifact {}: {}", artifact.kind.label(), artifact.title)
        }
    }
}

fn searchable_markdown(markdown: &str, streaming: bool, config: &ChatMessageListConfig) -> String {
    let mut content = markdown.to_string();
    if streaming && !config.in_progress_suffix.is_empty() {
        content.push_str(&config.in_progress_suffix);
    }
    content
}

fn searchable_tool_use(tool: &ToolUseBlock) -> String {
    let mut lines = vec![tool.name.clone()];
    if tool.collapsed {
        return lines.join("\n");
    }
    lines.extend(tool_input_detail_lines(&tool.input));
    if let Some(approval) = &tool.approval {
        lines.push(format!("Approval: {}", approval.prompt));
        if let Some(resolved) = &approval.resolved {
            lines.push(approval_resolved_label(approval, resolved));
        } else if approval.options.is_empty() {
            lines.push("No approval options".to_string());
        } else {
            lines.extend(
                approval
                    .options
                    .iter()
                    .map(|option| approval_option_button_label(approval, option)),
            );
        }
    }
    lines.join("\n")
}

fn searchable_tool_result(result: &ToolResultBlock) -> String {
    let mut lines = vec![tool_result_title(result)];
    if !result.collapsed {
        lines.push(result.output.as_text().to_string());
    }
    lines.join("\n")
}

#[derive(Default)]
struct ChatMessageRowBindings {
    header: Option<Binding<String>>,
    turn_status: Option<Binding<ChatTurnStatus>>,
    timestamp: Option<Binding<Option<String>>>,
    markdown: Option<Binding<String>>,
    diff: Option<Binding<String>>,
    plan_items: Option<Binding<Vec<PlanItem>>>,
    task_details: Option<Binding<TaskDetails>>,
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
            ChatRowRef::Header { message_id, .. } => store.message_version(message_id),
            ChatRowRef::Block(_) | ChatRowRef::PendingToolResult(_) => 0,
        };
        let last_block_version = match row_ref {
            ChatRowRef::Header { .. } => 0,
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
            ChatRowRef::Header {
                message_id,
                collapsed,
            } => self.sync_header_bindings(message_id, collapsed),
            ChatRowRef::Block(block_id) => self.sync_block_bindings(block_id),
            ChatRowRef::PendingToolResult(tool_use_id) => self.sync_block_bindings(tool_use_id),
        }
    }

    fn sync_header_bindings(&self, message_id: ChatMessageId, collapsed: bool) {
        let version = self.store.message_version(message_id);
        if version == self.last_message_version.get() {
            return;
        }
        self.store.with_message(message_id, |message| {
            if let Some(binding) = &self.body_bindings.header {
                binding.set(turn_header_label_for_row(message, collapsed));
            }
            if let Some(binding) = &self.body_bindings.turn_status {
                binding.set(message.status.clone());
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

            if let Some(binding) = &self.body_bindings.plan_items
                && let Some(items) = block_plan_items_for_render(block)
            {
                binding.set(items);
            }

            if let Some(binding) = &self.body_bindings.task_details
                && let Some(details) = block_task_details_for_render(block)
            {
                binding.set(details);
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
        ChatRowKey::Header { collapsed, .. } => {
            let (bubble, mut bindings) = build_aligned_turn_header(message, config, *collapsed);
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
            let (bubble, body_bindings) =
                build_aligned_pending_tool_result(message, call_id, config);
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

fn block_plan_items_for_render(block: &ChatBlock) -> Option<Vec<PlanItem>> {
    match block {
        ChatBlock::Plan(plan) => Some(plan.items.clone()),
        _ => None,
    }
}

fn block_task_details_for_render(block: &ChatBlock) -> Option<TaskDetails> {
    match block {
        ChatBlock::Task(task) => Some(TaskDetails::from(task)),
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
        ChatBlock::Task(task) => Some(task_status_to_disclosure(task.status)),
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

impl ::atto_ui::composable::FocusNav for ChatMessageRow {
    fn focused_child(&self) -> Option<ComponentId> {
        self.view.focused_child()
    }

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

impl ::atto_ui::composable::DynamicTree for ChatMessageRow {}

impl ::atto_ui::composable::EventHandling for ChatMessageRow {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.sync_body_bindings();
        self.view.handle_event(event, ctx)
    }
}

/// Place a bubble within an alignment row. The bubble occupies `percent` of the
/// width; the remainder is a spacer on the opposite side. At 100 the bubble
/// fills the whole row (no spacer).
fn align_bubble<C>(bubble: C, alignment: ChatAlignment, percent: u16) -> HStack
where
    C: ::atto_ui::composable::Component + 'static,
{
    let percent = percent.clamp(1, 100);
    let bubble_layout = LayoutParams {
        width: Size::Weight(percent),
        height: Size::Content,
        ..LayoutParams::default()
    };
    let spacer_weight = 100u16.saturating_sub(percent);
    if spacer_weight == 0 {
        return HStack::new().child_with_layout(bubble, bubble_layout);
    }
    let spacer_layout = LayoutParams {
        width: Size::Weight(spacer_weight),
        ..LayoutParams::default()
    };
    match alignment {
        ChatAlignment::Left => HStack::new()
            .child_with_layout(bubble, bubble_layout)
            .child_with_layout(Spacer::new(), spacer_layout),
        ChatAlignment::Right => HStack::new()
            .child_with_layout(Spacer::new(), spacer_layout)
            .child_with_layout(bubble, bubble_layout),
    }
}

fn build_aligned_turn_header(
    message: &ChatMessage,
    config: &ChatMessageRowConfig,
    collapsed: bool,
) -> (HStack, ChatMessageRowBindings) {
    let (bubble, bindings) = build_turn_header(message, config, collapsed);
    let row = align_bubble(
        bubble,
        message.role.alignment(),
        config.bubble_width_percent,
    );
    (row, bindings)
}

fn build_aligned_block(
    message: &ChatMessage,
    block: Option<&ChatBlock>,
    config: &ChatMessageRowConfig,
) -> (HStack, ChatMessageRowBindings) {
    let (bubble, body_bindings) = build_block_bubble(message.id, block, config);
    let row = align_bubble(
        bubble,
        message.role.alignment(),
        config.bubble_width_percent,
    );
    (row, body_bindings)
}

fn build_aligned_pending_tool_result(
    message: &ChatMessage,
    call_id: &str,
    config: &ChatMessageRowConfig,
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
    let row = align_bubble(
        bubble,
        message.role.alignment(),
        config.bubble_width_percent,
    );
    (row, ChatMessageRowBindings::default())
}

fn build_turn_header(
    message: &ChatMessage,
    config: &ChatMessageRowConfig,
    collapsed: bool,
) -> (VStack, ChatMessageRowBindings) {
    let header_label = Binding::new(turn_header_label_for_row(message, collapsed));
    let turn_status = Binding::new(message.status.clone());
    let header = Text::new(String::new()).text(header_label.clone()).style(
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let content_layout = LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    };

    let mut bubble = VStack::new()
        .with_spacing(1)
        .child_with_layout(header, content_layout);

    if let Some(actions) = turn_action_row(message, turn_status.clone(), config) {
        bubble = bubble.child_with_layout(actions, content_layout);
    }

    (
        bubble,
        ChatMessageRowBindings {
            header: Some(header_label),
            turn_status: Some(turn_status),
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
    let body = match block.zip(config.on_message_action.clone()) {
        Some((block, callback)) => body.with_copy_shortcut(message_id, block.id(), callback),
        None => body,
    };
    let content_layout = LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    };

    let mut bubble = VStack::new()
        .with_spacing(1)
        .child_with_layout(body, content_layout);

    if let Some(block) = block
        && let Some(actions) = block_action_row(message_id, block, config)
    {
        bubble = bubble.child_with_layout(actions, content_layout);
    }

    (bubble, body_bindings)
}

fn has_turn_action_row(message: &ChatMessage, config: &ChatMessageRowConfig) -> bool {
    let show_cancel = config.on_cancel.is_some() && message.status.is_streaming();
    has_turn_collapse_control(message)
        || show_cancel
        || config.quote_replies.is_some()
        || config.on_message_action.is_some()
        || (config.edit_and_resubmit.is_some() && editable_user_message_text(message).is_some())
}

fn has_turn_collapse_control(message: &ChatMessage) -> bool {
    !message.blocks.is_empty()
}

fn turn_action_row(
    message: &ChatMessage,
    turn_status: Binding<ChatTurnStatus>,
    config: &ChatMessageRowConfig,
) -> Option<HStack> {
    let show_cancel = config.on_cancel.is_some() && turn_status.get().is_streaming();
    let editable_text = config
        .edit_and_resubmit
        .as_ref()
        .and_then(|_| editable_user_message_text(message));
    if config.on_message_action.is_none()
        && editable_text.is_none()
        && !show_cancel
        && config.quote_replies.is_none()
        && !has_turn_collapse_control(message)
    {
        return None;
    }
    let mut row = HStack::new().with_spacing(1);
    if has_turn_collapse_control(message) {
        let collapsed = config.collapsed_turns.get().contains(&message.id);
        let label = if collapsed { "Expand" } else { "Collapse" };
        row = row.child_with_layout(
            turn_collapse_button(label, message.id, collapsed, config.collapsed_turns.clone()),
            LayoutParams {
                width: Size::Fixed(button_width_for_label(label)),
                height: Size::Content,
                ..LayoutParams::default()
            },
        );
    }
    if show_cancel && let Some(callback) = config.on_cancel.clone() {
        let label = "Cancel";
        let controller = StreamingCancelController::new(config.store.clone(), callback);
        row = row.child_with_layout(
            streaming_cancel_button(label, message.id, turn_status, controller),
            LayoutParams {
                width: Size::Content,
                height: Size::Content,
                ..LayoutParams::default()
            },
        );
    }
    if let Some(controller) = config.quote_replies.clone() {
        let label = "Quote";
        row = row.child_with_layout(
            quote_message_button(label, message, controller),
            LayoutParams {
                width: Size::Fixed(button_width_for_label(label)),
                height: Size::Content,
                ..LayoutParams::default()
            },
        );
    }
    if let Some(callback) = config.on_message_action.clone() {
        for (label, kind) in turn_action_specs(&message.role) {
            if matches!(kind, MessageActionKind::EditUser)
                && let (Some(controller), Some(original_text)) =
                    (config.edit_and_resubmit.clone(), editable_text.clone())
            {
                row = row.child_with_layout(
                    edit_and_resubmit_button(label, message.id, original_text, controller),
                    LayoutParams {
                        width: Size::Fixed(button_width_for_label(label)),
                        height: Size::Content,
                        ..LayoutParams::default()
                    },
                );
                continue;
            }
            row = row.child_with_layout(
                turn_message_action_button(label, message.id, kind, config, callback.clone()),
                LayoutParams {
                    width: Size::Fixed(button_width_for_label(label)),
                    height: Size::Content,
                    ..LayoutParams::default()
                },
            );
        }
    } else if let (Some(controller), Some(original_text)) =
        (config.edit_and_resubmit.clone(), editable_text)
    {
        let label = "Edit";
        row = row.child_with_layout(
            edit_and_resubmit_button(label, message.id, original_text, controller),
            LayoutParams {
                width: Size::Fixed(button_width_for_label(label)),
                height: Size::Content,
                ..LayoutParams::default()
            },
        );
    }
    Some(row)
}

fn editable_user_message_text(message: &ChatMessage) -> Option<String> {
    if !matches!(message.role, ChatRole::User) {
        return None;
    }
    let parts = message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text(text) => Some(text.markdown.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

fn turn_action_specs(role: &ChatRole) -> Vec<(&'static str, MessageActionKind)> {
    match role {
        ChatRole::User => vec![
            ("Copy", MessageActionKind::Copy),
            ("Edit", MessageActionKind::EditUser),
        ],
        ChatRole::Assistant => vec![
            ("Copy", MessageActionKind::Copy),
            ("Retry", MessageActionKind::Retry),
            ("Regenerate", MessageActionKind::Regenerate),
        ],
        ChatRole::System | ChatRole::Custom(_) => vec![("Copy", MessageActionKind::Copy)],
    }
}

fn block_action_row(
    message_id: ChatMessageId,
    block: &ChatBlock,
    config: &ChatMessageRowConfig,
) -> Option<HStack> {
    if config.on_message_action.is_none() && config.quote_replies.is_none() {
        return None;
    }
    let mut row = HStack::new().with_spacing(1);
    if let Some(callback) = config.on_message_action.clone() {
        let label = "Copy block";
        row = row.child_with_layout(
            message_action_button(
                label,
                message_id,
                MessageActionKind::CopyBlock(block.id()),
                callback,
            ),
            LayoutParams {
                width: Size::Fixed(button_width_for_label(label)),
                height: Size::Content,
                ..LayoutParams::default()
            },
        );
    }
    if let Some(controller) = config.quote_replies.clone() {
        let label = "Quote block";
        row = row.child_with_layout(
            quote_block_button(label, message_id, block, controller),
            LayoutParams {
                width: Size::Fixed(button_width_for_label(label)),
                height: Size::Content,
                ..LayoutParams::default()
            },
        );
    }
    Some(row)
}

fn message_action_button(
    label: &'static str,
    message_id: ChatMessageId,
    kind: MessageActionKind,
    callback: MessageActionCallback,
) -> Button {
    let action = MessageAction { message_id, kind };
    Button::new(label).on_click(move || callback(action.clone()))
}

fn quote_message_button(
    label: &'static str,
    message: &ChatMessage,
    controller: QuoteReplyController,
) -> Button {
    let reference = quote_message_reference(message);
    Button::new(label).on_click(move || controller.attach(reference.clone()))
}

fn quote_block_button(
    label: &'static str,
    message_id: ChatMessageId,
    block: &ChatBlock,
    controller: QuoteReplyController,
) -> Button {
    let reference = quote_block_reference(message_id, block);
    Button::new(label).on_click(move || controller.attach(reference.clone()))
}

fn quote_message_reference(message: &ChatMessage) -> ChatInputReference {
    ChatInputReference::new(
        message.id,
        format!("{} #{}", message.role.label(), message.id.0),
        quote_message_preview(message),
    )
}

fn quote_block_reference(message_id: ChatMessageId, block: &ChatBlock) -> ChatInputReference {
    ChatInputReference::new(
        message_id,
        format!("Block #{}", block.id().0),
        quote_block_preview(block),
    )
    .block_id(block.id())
}

fn quote_message_preview(message: &ChatMessage) -> String {
    let preview = message
        .blocks
        .iter()
        .map(quote_block_preview)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
    compact_quote_preview(&preview)
}

fn quote_block_preview(block: &ChatBlock) -> String {
    let raw = match block {
        ChatBlock::Text(block) => block.markdown.clone(),
        ChatBlock::Thinking(block) => block.markdown.clone(),
        ChatBlock::ToolUse(block) => format!("Tool {} ({:?})", block.name, block.status),
        ChatBlock::ToolResult(block) => block.output.as_text().to_string(),
        ChatBlock::Diff(block) => block.diff.unified.clone(),
        ChatBlock::Plan(block) => block
            .items
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>()
            .join("; "),
        ChatBlock::Task(block) => {
            if block.summary.trim().is_empty() {
                block.title.clone()
            } else {
                format!("{} - {}", block.title, block.summary)
            }
        }
        ChatBlock::Todo(block) => block
            .items
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>()
            .join("; "),
        ChatBlock::Attachment(block) => format!("Attachment {}", block.name),
        ChatBlock::Notice(block) => block.text.clone(),
        ChatBlock::Artifact(block) => format!("Artifact {}", block.title),
    };
    compact_quote_preview(&raw)
}

fn compact_quote_preview(text: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 160;
    let mut compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() > MAX_PREVIEW_CHARS {
        compact = compact.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
        compact.push_str("...");
    }
    compact
}

fn turn_collapse_button(
    label: &'static str,
    message_id: ChatMessageId,
    collapsed: bool,
    collapsed_turns: Binding<HashSet<ChatMessageId>>,
) -> Button {
    Button::new(label).on_click(move || {
        set_turn_collapsed(&collapsed_turns, message_id, !collapsed);
    })
}

fn set_turn_collapsed(
    collapsed_turns: &Binding<HashSet<ChatMessageId>>,
    message_id: ChatMessageId,
    collapsed: bool,
) {
    let mut next = collapsed_turns.get();
    let changed = if collapsed {
        next.insert(message_id)
    } else {
        next.remove(&message_id)
    };
    if changed {
        collapsed_turns.set(next);
    }
}

fn turn_message_action_button(
    label: &'static str,
    message_id: ChatMessageId,
    kind: MessageActionKind,
    config: &ChatMessageRowConfig,
    callback: MessageActionCallback,
) -> Button {
    if matches!(
        kind,
        MessageActionKind::Retry | MessageActionKind::Regenerate
    ) {
        retry_or_regenerate_button(label, message_id, kind, config.store.clone(), callback)
    } else {
        message_action_button(label, message_id, kind, callback)
    }
}

fn retry_or_regenerate_button(
    label: &'static str,
    message_id: ChatMessageId,
    kind: MessageActionKind,
    store: ChatMessageStore,
    callback: MessageActionCallback,
) -> Button {
    let action = MessageAction { message_id, kind };
    Button::new(label).on_click(move || {
        let is_assistant = store
            .with_message(message_id, |message| {
                matches!(message.role, ChatRole::Assistant)
            })
            .unwrap_or(false);
        if is_assistant && store.truncate_from(message_id).is_some() {
            callback(action.clone());
        }
    })
}

fn edit_and_resubmit_button(
    label: &'static str,
    message_id: ChatMessageId,
    original_text: String,
    controller: EditAndResubmitController,
) -> Button {
    Button::new(label).on_click(move || {
        let _ = controller.begin_edit(message_id, original_text.clone());
    })
}

fn streaming_cancel_button(
    label: &'static str,
    message_id: ChatMessageId,
    status: Binding<ChatTurnStatus>,
    controller: StreamingCancelController,
) -> StreamingCancelButton {
    let button_controller = controller.clone();
    StreamingCancelButton {
        label,
        status,
        button: Button::new(label).on_click(move || {
            let _ = button_controller.request(message_id);
        }),
    }
}

struct StreamingCancelButton {
    label: &'static str,
    status: Binding<ChatTurnStatus>,
    button: Button,
}

impl StreamingCancelButton {
    fn active(&self) -> bool {
        self.status.get().is_streaming()
    }
}

impl ::atto_ui::composable::Component for StreamingCancelButton {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if self.active() {
            self.button.draw(frame, area, ctx);
        }
    }
}

impl ::atto_ui::composable::Layout for StreamingCancelButton {
    fn min_width(&self) -> u16 {
        if self.active() {
            button_width_for_label(self.label)
        } else {
            0
        }
    }

    fn min_height(&self) -> u16 {
        u16::from(self.active())
    }

    fn desired_height(&self) -> Option<u16> {
        Some(self.min_height())
    }
}

impl ::atto_ui::composable::FocusNav for StreamingCancelButton {
    fn is_focusable(&self) -> bool {
        self.active()
    }
}

impl ::atto_ui::composable::EventHandling for StreamingCancelButton {
    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if self.active() {
            self.button.handle_event(event, ctx)
        } else {
            EventResult::ignored()
        }
    }
}

atto_ui::impl_component_default_traits!(StreamingCancelButton => Scrollable, DynamicTree);

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

fn turn_header_label_for_row(message: &ChatMessage, collapsed: bool) -> String {
    let mut label = turn_header_label(message);
    if collapsed {
        label.push('\n');
        label.push_str(&collapsed_turn_placeholder(message));
    }
    label
}

fn collapsed_turn_placeholder(message: &ChatMessage) -> String {
    let count = message.blocks.len();
    let noun = if count == 1 { "block" } else { "blocks" };
    format!("Collapsed · {count} {noun} hidden")
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
    path: Option<String>,
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
        path: Option<String>,
        message_id: ChatMessageId,
        block_id: ChatBlockId,
        decision: EditDecision,
        on_edit_decision: Option<EditDecisionCallback>,
    ) -> Self {
        Self {
            title,
            diff: diff.into(),
            path,
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

    fn diff_lines(&self, base: Style, title_style: Style, theme: &Theme) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if let Some(title) = &self.title {
            lines.push(Line::styled(title.clone(), title_style));
        }
        let diff = self.diff.get();
        lines.extend(diff_display_lines(&diff, base, self.path.as_deref(), theme));
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
                Paragraph::new(self.diff_lines(
                    ctx.theme.widget.normal,
                    ctx.theme.widget.dim,
                    ctx.theme,
                ))
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

struct PlanDecisionView {
    items: Binding<Vec<PlanItem>>,
    message_id: ChatMessageId,
    block_id: ChatBlockId,
    decision: PlanDecision,
    on_plan_decision: Option<PlanDecisionCallback>,
    focused_action: usize,
    last_area: Option<Rect>,
}

impl PlanDecisionView {
    fn new(
        items: impl Into<Binding<Vec<PlanItem>>>,
        message_id: ChatMessageId,
        block_id: ChatBlockId,
        decision: PlanDecision,
        on_plan_decision: Option<PlanDecisionCallback>,
    ) -> Self {
        Self {
            items: items.into(),
            message_id,
            block_id,
            decision,
            on_plan_decision,
            focused_action: 0,
            last_area: None,
        }
    }

    fn plan_lines(&self, ctx: ComponentContext<'_>) -> Vec<Line<'static>> {
        let mut lines = vec![Line::styled(
            plan_block_title(self.decision),
            ctx.theme.widget.dim,
        )];
        let items = self.items.get();
        if items.is_empty() {
            lines.push(Line::styled("(no plan items)", ctx.theme.widget.dim));
        } else {
            lines.extend(
                items
                    .iter()
                    .map(|item| Line::styled(plan_display_line(item), ctx.theme.widget.normal)),
            );
        }
        lines
    }

    fn plan_height(&self) -> u16 {
        self.items
            .with(|items| items.len().max(1).saturating_add(1).min(u16::MAX as usize) as u16)
    }

    fn display_height(&self) -> u16 {
        self.plan_height().saturating_add(1)
    }

    fn display_width(&self) -> u16 {
        let item_width = self.items.with(|items| {
            plan_display_lines(items)
                .iter()
                .map(|line| UnicodeWidthStr::width(line.as_str()))
                .max()
                .unwrap_or(0)
        });
        plan_block_title(self.decision)
            .width()
            .max(item_width)
            .max(plan_decision_action_line_width(self.decision))
            .max(1)
            .min(u16::MAX as usize) as u16
    }

    fn has_focusable_action(&self) -> bool {
        self.decision == PlanDecision::Pending && self.on_plan_decision.is_some()
    }

    fn emit_decision(&self, decision: PlanDecision) -> EventResult {
        let Some(callback) = &self.on_plan_decision else {
            return EventResult::ignored();
        };
        if self.decision != PlanDecision::Pending || decision == PlanDecision::Pending {
            return EventResult::ignored();
        }
        callback(PlanDecisionEvent {
            message_id: self.message_id,
            block_id: self.block_id,
            decision,
        });
        EventResult::changed()
    }

    fn focused_decision(&self) -> PlanDecision {
        if self.focused_action == 0 {
            PlanDecision::Accepted
        } else {
            PlanDecision::Rejected
        }
    }

    fn click_decision(&self, event: &Event, ctx: ComponentContext<'_>) -> Option<PlanDecision> {
        let Event::Mouse(mouse) = event else {
            return None;
        };
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return None;
        }
        let area = self.last_area?;
        let (column, row) = mouse_position_in_area(area, mouse, ctx.mouse_coordinate_space)?;
        if row != self.plan_height() {
            return None;
        }
        plan_decision_action_at_column(column)
    }
}

impl Component for PlanDecisionView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        if area.width == 0 || area.height == 0 {
            return;
        }

        let plan_height = self.plan_height().min(area.height);
        if plan_height > 0 {
            let plan_area = Rect {
                height: plan_height,
                ..area
            };
            frame.render_widget(Paragraph::new(self.plan_lines(ctx)), plan_area);
        }

        if area.height > plan_height {
            let action_area = Rect {
                y: area.y.saturating_add(plan_height),
                height: 1,
                ..area
            };
            frame.render_widget(
                Paragraph::new(plan_decision_action_line(
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

impl ::atto_ui::composable::DragAndDrop for PlanDecisionView {}
impl ::atto_ui::composable::Scrollable for PlanDecisionView {}
impl ::atto_ui::composable::FocusNav for PlanDecisionView {
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
impl ::atto_ui::composable::DynamicTree for PlanDecisionView {}
impl ::atto_ui::composable::EventHandling for PlanDecisionView {
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

        EventResult::ignored()
    }
}

impl ::atto_ui::composable::Layout for PlanDecisionView {
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

#[derive(Clone, Debug, PartialEq)]
struct TaskDetails {
    title: String,
    status: TaskStatus,
    summary: String,
    transcript: Vec<TaskTranscriptItem>,
}

impl From<&TaskBlock> for TaskDetails {
    fn from(task: &TaskBlock) -> Self {
        Self {
            title: task.title.clone(),
            status: task.status,
            summary: task.summary.clone(),
            transcript: task.transcript.clone(),
        }
    }
}

struct TaskBlockView {
    details: Binding<TaskDetails>,
}

impl TaskBlockView {
    fn new(details: impl Into<Binding<TaskDetails>>) -> Self {
        Self {
            details: details.into(),
        }
    }

    fn display_lines(&self) -> Vec<String> {
        self.details.with(task_display_lines)
    }
}

impl Component for TaskBlockView {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let details = self.details.get();
        let lines = task_display_lines(&details)
            .into_iter()
            .enumerate()
            .map(|(idx, line)| {
                if idx == 0 {
                    Line::styled(line, ctx.theme.widget.dim)
                } else {
                    Line::styled(line, ctx.theme.widget.normal)
                }
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), area);
    }
}

impl ::atto_ui::composable::DragAndDrop for TaskBlockView {}
impl ::atto_ui::composable::Scrollable for TaskBlockView {}
impl ::atto_ui::composable::FocusNav for TaskBlockView {}
impl ::atto_ui::composable::DynamicTree for TaskBlockView {}
impl ::atto_ui::composable::EventHandling for TaskBlockView {}

impl ::atto_ui::composable::Layout for TaskBlockView {
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
        Some(self.display_lines().len().max(1).min(u16::MAX as usize) as u16)
    }
}

enum ChatMessageBody {
    Markdown(ResponsiveMarkdownView),
    Text(Text),
    Disclosure(Disclosure),
    Diff(DiffDecisionView),
    Plan(PlanDecisionView),
    Todo(TodoListView),
    Artifact(ArtifactLink),
    CopyTarget(BlockCopyTarget),
}

impl ChatMessageBody {
    fn with_copy_shortcut(
        self,
        message_id: ChatMessageId,
        block_id: ChatBlockId,
        callback: MessageActionCallback,
    ) -> Self {
        ChatMessageBody::CopyTarget(BlockCopyTarget::new(self, message_id, block_id, callback))
    }

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
                        Some(diff.path.clone()),
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
            Some(ChatBlock::Plan(plan)) => {
                let items = Binding::new(plan.items.clone());
                (
                    ChatMessageBody::Plan(PlanDecisionView::new(
                        items.clone(),
                        message_id,
                        plan.id,
                        plan.decision,
                        config.on_plan_decision.clone(),
                    )),
                    ChatMessageRowBindings {
                        plan_items: Some(items),
                        ..ChatMessageRowBindings::default()
                    },
                )
            }
            Some(ChatBlock::Task(task)) => {
                let details = Binding::new(TaskDetails::from(task));
                let status = Binding::new(task_status_to_disclosure(task.status));
                let view = Disclosure::new(task_block_title(task))
                    .expanded(!task.collapsed)
                    .status(status.clone())
                    .child(TaskBlockView::new(details.clone()));
                (
                    ChatMessageBody::Disclosure(view),
                    ChatMessageRowBindings {
                        task_details: Some(details),
                        disclosure_status: Some(status),
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

fn tool_status_label(status: &ToolStatus) -> &'static str {
    match status {
        ToolStatus::Pending => "pending",
        ToolStatus::Running => "running",
        ToolStatus::Done => "done",
        ToolStatus::Error => "error",
        ToolStatus::Canceled => "canceled",
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

fn plan_block_title(decision: PlanDecision) -> String {
    format!("Plan: {}", plan_decision_label(decision))
}

fn plan_decision_label(decision: PlanDecision) -> &'static str {
    match decision {
        PlanDecision::Pending => "pending",
        PlanDecision::Accepted => "accepted",
        PlanDecision::Rejected => "rejected",
    }
}

fn plan_decision_action_line_width(decision: PlanDecision) -> usize {
    match decision {
        PlanDecision::Pending => PLAN_ACCEPT_LABEL
            .width()
            .saturating_add(1)
            .saturating_add(PLAN_REJECT_LABEL.width()),
        PlanDecision::Accepted => PLAN_ACCEPTED_LABEL.width(),
        PlanDecision::Rejected => PLAN_REJECTED_LABEL.width(),
    }
}

const PLAN_ACCEPT_LABEL: &str = "[ Accept ]";
const PLAN_REJECT_LABEL: &str = "[ Reject ]";
const PLAN_ACCEPTED_LABEL: &str = "[x] Accepted";
const PLAN_REJECTED_LABEL: &str = "[x] Rejected";

fn plan_decision_action_at_column(column: u16) -> Option<PlanDecision> {
    let column = column as usize;
    let accept_width = PLAN_ACCEPT_LABEL.width();
    if column < accept_width {
        return Some(PlanDecision::Accepted);
    }
    let reject_start = accept_width.saturating_add(1);
    let reject_end = reject_start.saturating_add(PLAN_REJECT_LABEL.width());
    (column >= reject_start && column < reject_end).then_some(PlanDecision::Rejected)
}

fn plan_decision_action_line(
    decision: PlanDecision,
    focused_action: usize,
    focused: bool,
    ctx: ComponentContext<'_>,
) -> Line<'static> {
    match decision {
        PlanDecision::Pending => {
            let base = ctx.theme.widget.accent;
            let focused_style = base.add_modifier(Modifier::REVERSED);
            Line::from(vec![
                Span::styled(
                    PLAN_ACCEPT_LABEL,
                    if focused && focused_action == 0 {
                        focused_style
                    } else {
                        base
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    PLAN_REJECT_LABEL,
                    if focused && focused_action == 1 {
                        focused_style
                    } else {
                        base
                    },
                ),
            ])
        }
        PlanDecision::Accepted => Line::styled(PLAN_ACCEPTED_LABEL, ctx.theme.widget.dim),
        PlanDecision::Rejected => Line::styled(PLAN_REJECTED_LABEL, ctx.theme.widget.dim),
    }
}

fn plan_display_lines(items: &[PlanItem]) -> Vec<String> {
    if items.is_empty() {
        return vec!["(no plan items)".to_string()];
    }
    items.iter().map(plan_display_line).collect()
}

fn plan_display_line(item: &PlanItem) -> String {
    format!("- {}", item.text)
}

fn task_block_title(task: &TaskBlock) -> String {
    format!("Task: {}", task.title)
}

fn task_status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Complete => "complete",
        TaskStatus::Failed => "failed",
        TaskStatus::Canceled => "canceled",
    }
}

fn task_status_to_disclosure(status: TaskStatus) -> DisclosureStatus {
    match status {
        TaskStatus::Pending => DisclosureStatus::Idle,
        TaskStatus::Running => DisclosureStatus::Running,
        TaskStatus::Complete => DisclosureStatus::Done,
        TaskStatus::Failed => DisclosureStatus::Error,
        TaskStatus::Canceled => DisclosureStatus::Canceled,
    }
}

fn task_display_lines(details: &TaskDetails) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Task: {} ({})",
            details.title,
            task_status_label(details.status)
        ),
        format!("Status: {}", task_status_label(details.status)),
    ];
    if !details.summary.is_empty() {
        lines.push(format!("Summary: {}", details.summary));
    }
    if details.transcript.is_empty() {
        lines.push("(no task transcript)".to_string());
        return lines;
    }

    lines.push("Transcript:".to_string());
    for item in &details.transcript {
        lines.push(format!("  {}:", item.role.label()));
        for block in &item.blocks {
            append_task_block_lines(&mut lines, block, 4);
        }
    }
    lines
}

fn append_task_block_lines(lines: &mut Vec<String>, block: &ChatBlock, indent: usize) {
    let prefix = " ".repeat(indent);
    match block {
        ChatBlock::Text(text) => append_prefixed_text_lines(lines, &text.markdown, &prefix),
        ChatBlock::Thinking(thinking) => {
            lines.push(format!("{prefix}Thinking:"));
            append_prefixed_text_lines(lines, &thinking.markdown, &format!("{prefix}  "));
        }
        ChatBlock::ToolUse(tool) => {
            lines.push(format!(
                "{prefix}Tool use: {} ({})",
                tool.name,
                tool_status_label(&tool.status)
            ));
            for line in tool_input_detail_lines(&tool.input) {
                lines.push(format!("{prefix}  {line}"));
            }
        }
        ChatBlock::ToolResult(result) => {
            lines.push(format!("{prefix}{}", tool_result_title(result)));
            append_prefixed_text_lines(lines, result.output.as_text(), &format!("{prefix}  "));
        }
        ChatBlock::Diff(diff) => {
            lines.push(format!("{prefix}{}", diff_block_title(diff)));
            append_prefixed_text_lines(lines, &diff.diff.unified, &format!("{prefix}  "));
        }
        ChatBlock::Plan(plan) => {
            lines.push(format!("{prefix}{}", plan_block_title(plan.decision)));
            for line in plan_display_lines(&plan.items) {
                lines.push(format!("{prefix}  {line}"));
            }
        }
        ChatBlock::Task(task) => {
            lines.push(format!(
                "{prefix}Task: {} ({})",
                task.title,
                task_status_label(task.status)
            ));
            for line in task_display_lines(&TaskDetails::from(task)) {
                lines.push(format!("{prefix}  {line}"));
            }
        }
        ChatBlock::Todo(todo) => {
            lines.push(format!("{prefix}Todo:"));
            for line in todo_display_lines(&todo.items) {
                lines.push(format!("{prefix}  {line}"));
            }
        }
        ChatBlock::Attachment(attachment) => lines.push(format!(
            "{prefix}{}",
            attachment_label(&attachment.name, attachment.url.as_deref())
        )),
        ChatBlock::Notice(notice) => {
            lines.push(format!(
                "{prefix}{}",
                notice_label(notice.level, &notice.text)
            ));
        }
        ChatBlock::Artifact(artifact) => lines.push(format!(
            "{prefix}Artifact {}: {}",
            artifact.kind.label(),
            artifact.title
        )),
    }
}

fn append_prefixed_text_lines(lines: &mut Vec<String>, text: &str, prefix: &str) {
    if text.is_empty() {
        lines.push(prefix.to_string());
        return;
    }
    lines.extend(text.lines().map(|line| format!("{prefix}{line}")));
}

fn task_details_desired_height(details: &TaskDetails) -> u16 {
    task_display_lines(details)
        .len()
        .max(1)
        .min(u16::MAX as usize) as u16
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
        ToolOutput::Diff(_) => Box::new(DiffView::new(None, content, None)),
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
    path: Option<String>,
    scroll_x: u16,
    viewport: (u16, u16),
}

impl DiffView {
    fn new(title: Option<String>, diff: impl Into<Binding<String>>, path: Option<String>) -> Self {
        Self {
            title,
            diff: diff.into(),
            path,
            scroll_x: 0,
            viewport: (0, 0),
        }
    }

    fn display_lines(&self, base: Style, title_style: Style, theme: &Theme) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if let Some(title) = &self.title {
            lines.push(Line::styled(title.clone(), title_style));
        }
        let diff = self.diff.get();
        lines.extend(diff_display_lines(&diff, base, self.path.as_deref(), theme));
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
            Paragraph::new(self.display_lines(
                ctx.theme.widget.normal,
                ctx.theme.widget.dim,
                ctx.theme,
            ))
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

fn diff_display_lines(
    diff: &str,
    base: Style,
    explicit_path: Option<&str>,
    theme: &Theme,
) -> Vec<Line<'static>> {
    if diff.is_empty() {
        return vec![Line::styled(String::new(), base)];
    }

    let metas = classify_diff_lines(diff, explicit_path);
    let highlighted = highlight_diff_payloads(&metas);

    metas
        .iter()
        .enumerate()
        .map(|(idx, meta)| {
            diff_render_line(
                meta,
                base,
                theme,
                highlighted.get(idx).and_then(Option::as_ref),
            )
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiffLineKind {
    FileHeader,
    Hunk,
    Addition,
    Removal,
    Context,
    Other,
}

struct DiffLineMeta<'a> {
    text: &'a str,
    kind: DiffLineKind,
    payload: Option<&'a str>,
    hint: Option<String>,
    section: usize,
}

fn classify_diff_lines<'a>(diff: &'a str, explicit_path: Option<&str>) -> Vec<DiffLineMeta<'a>> {
    let explicit_hint = explicit_path.and_then(normalize_diff_path);
    let mut current_hint = explicit_hint.clone();
    let mut in_hunk = false;
    let mut section = 0usize;
    let mut metas = Vec::new();
    let raw_lines = diff.lines().collect::<Vec<_>>();

    // Use hunk state plus adjacent ---/+++ pairs so deleted payloads like "--- foo" stay red.
    for (idx, line) in raw_lines.iter().enumerate() {
        let line = *line;
        let prev = idx
            .checked_sub(1)
            .and_then(|idx| raw_lines.get(idx))
            .copied();
        let next = raw_lines.get(idx + 1).copied();
        let starts_file_header_pair =
            line.starts_with("--- ") && next.is_some_and(|next| next.starts_with("+++ "));
        let follows_file_header_pair =
            line.starts_with("+++ ") && prev.is_some_and(|prev| prev.starts_with("--- "));

        let (kind, payload) = if line.starts_with("diff --git ") {
            in_hunk = false;
            section = section.saturating_add(1);
            if explicit_hint.is_none()
                && let Some(path) = diff_git_new_path(line)
            {
                current_hint = Some(path);
            }
            (DiffLineKind::Other, None)
        } else if line.starts_with("@@") {
            in_hunk = true;
            (DiffLineKind::Hunk, None)
        } else if starts_file_header_pair || (!in_hunk && line.starts_with("--- ")) {
            in_hunk = false;
            section = section.saturating_add(1);
            if explicit_hint.is_none()
                && let Some(path) = diff_header_path(line)
            {
                current_hint = Some(path);
            }
            (DiffLineKind::FileHeader, None)
        } else if follows_file_header_pair || (!in_hunk && line.starts_with("+++ ")) {
            in_hunk = false;
            if explicit_hint.is_none()
                && let Some(path) = diff_header_path(line)
            {
                current_hint = Some(path);
            }
            (DiffLineKind::FileHeader, None)
        } else if let Some(payload) = line.strip_prefix('+') {
            (DiffLineKind::Addition, Some(payload))
        } else if let Some(payload) = line.strip_prefix('-') {
            (DiffLineKind::Removal, Some(payload))
        } else if let Some(payload) = line.strip_prefix(' ') {
            (DiffLineKind::Context, Some(payload))
        } else {
            (DiffLineKind::Other, None)
        };

        metas.push(DiffLineMeta {
            text: line,
            kind,
            payload,
            hint: payload.and_then(|_| current_hint.clone()),
            section,
        });
    }

    metas
}

fn highlight_diff_payloads(metas: &[DiffLineMeta<'_>]) -> Vec<Option<HighlightedLine>> {
    let mut highlighted_by_line = vec![None; metas.len()];
    let mut payload_indices = metas
        .iter()
        .enumerate()
        .filter(|(_, meta)| meta.payload.is_some())
        .peekable();

    // Highlight per file section/path to avoid carrying parser state across unrelated files.
    while let Some((start_idx, start_meta)) = payload_indices.next() {
        let Some(hint) = start_meta.hint.as_deref() else {
            continue;
        };
        let section = start_meta.section;
        let mut run_indices = vec![start_idx];

        while let Some((_, next_meta)) = payload_indices.peek()
            && next_meta.section == section
            && next_meta.hint.as_deref() == Some(hint)
        {
            let (idx, _) = payload_indices.next().expect("peeked payload");
            run_indices.push(idx);
        }

        let source = run_indices
            .iter()
            .filter_map(|idx| metas[*idx].payload)
            .collect::<Vec<_>>()
            .join("\n");
        let Some(lines) = highlight_code_block(Some(hint), &source)
            .filter(|lines| lines.len() == run_indices.len())
        else {
            continue;
        };

        for (idx, line) in run_indices.into_iter().zip(lines) {
            highlighted_by_line[idx] = Some(line);
        }
    }

    highlighted_by_line
}

fn diff_render_line(
    meta: &DiffLineMeta<'_>,
    base: Style,
    theme: &Theme,
    highlighted: Option<&HighlightedLine>,
) -> Line<'static> {
    let line_style = diff_line_style_for_kind(meta.kind, base);
    let Some(payload) = meta.payload else {
        return Line::styled(meta.text.to_string(), line_style);
    };
    let Some(highlighted) = highlighted else {
        return Line::styled(meta.text.to_string(), line_style);
    };

    let mut spans = vec![Span::styled(diff_prefix_for_kind(meta.kind), line_style)];
    let preserve_semantic_colors =
        matches!(meta.kind, DiffLineKind::Addition | DiffLineKind::Removal);
    if highlighted.spans.is_empty() && !payload.is_empty() {
        spans.push(Span::styled(payload.to_string(), line_style));
    } else {
        spans.extend(highlighted.spans.iter().map(|span| {
            let syntax = diff_syntax_style(span.class, theme);
            Span::styled(
                span.text.clone(),
                compose_diff_syntax_style(line_style, syntax, preserve_semantic_colors),
            )
        }));
    }

    let mut line = Line::from(spans);
    line.style = line_style;
    line
}

fn diff_line_style_for_kind(kind: DiffLineKind, base: Style) -> Style {
    match kind {
        DiffLineKind::Hunk => base.fg(Color::Yellow),
        DiffLineKind::FileHeader => base.fg(Color::Cyan),
        DiffLineKind::Addition => base.fg(Color::Green),
        DiffLineKind::Removal => base.fg(Color::Red),
        DiffLineKind::Context | DiffLineKind::Other => base,
    }
}

fn diff_prefix_for_kind(kind: DiffLineKind) -> &'static str {
    match kind {
        DiffLineKind::Addition => "+",
        DiffLineKind::Removal => "-",
        DiffLineKind::Context => " ",
        DiffLineKind::FileHeader | DiffLineKind::Hunk | DiffLineKind::Other => "",
    }
}

fn compose_diff_syntax_style(
    line_style: Style,
    syntax_style: Style,
    preserve_semantic_colors: bool,
) -> Style {
    let mut style = line_style.patch(syntax_style);
    if preserve_semantic_colors {
        // Syntax spans may add modifiers, but +/- foreground/background remain semantic.
        style.fg = line_style.fg;
        style.bg = line_style.bg;
    }
    style
}

fn diff_syntax_style(class: SyntaxClass, theme: &Theme) -> Style {
    match class {
        SyntaxClass::Text => theme
            .named_style("markdown-syntax-text")
            .unwrap_or_default(),
        SyntaxClass::Comment => theme
            .named_style("markdown-syntax-comment")
            .unwrap_or(Style::default().fg(Color::DarkGray)),
        SyntaxClass::String => theme
            .named_style("markdown-syntax-string")
            .unwrap_or(Style::default().fg(Color::LightGreen)),
        SyntaxClass::Keyword => theme
            .named_style("markdown-syntax-keyword")
            .unwrap_or(Style::default().fg(Color::LightMagenta)),
        SyntaxClass::Function => theme
            .named_style("markdown-syntax-function")
            .unwrap_or(Style::default().fg(Color::LightCyan)),
        SyntaxClass::Type => theme
            .named_style("markdown-syntax-type")
            .unwrap_or(Style::default().fg(Color::Yellow)),
        SyntaxClass::Number => theme
            .named_style("markdown-syntax-number")
            .unwrap_or(Style::default().fg(Color::LightYellow)),
        SyntaxClass::Constant => theme
            .named_style("markdown-syntax-constant")
            .unwrap_or(Style::default().fg(Color::LightYellow)),
        SyntaxClass::Variable => theme
            .named_style("markdown-syntax-variable")
            .unwrap_or_default(),
        SyntaxClass::Operator => theme
            .named_style("markdown-syntax-operator")
            .unwrap_or(Style::default().fg(Color::LightBlue)),
        SyntaxClass::Punctuation => theme
            .named_style("markdown-syntax-punctuation")
            .unwrap_or(Style::default().fg(Color::Gray)),
    }
}

fn diff_git_new_path(line: &str) -> Option<String> {
    let rest = line.strip_prefix("diff --git ")?;
    rest.split_whitespace().nth(1).and_then(normalize_diff_path)
}

fn diff_header_path(line: &str) -> Option<String> {
    let raw = line
        .strip_prefix("--- ")
        .or_else(|| line.strip_prefix("+++ "))?;
    normalize_diff_path(raw)
}

fn normalize_diff_path(raw: &str) -> Option<String> {
    let path = raw
        .split_whitespace()
        .next()
        .unwrap_or(raw)
        .trim()
        .trim_matches('"');
    let path = path
        .strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path);

    (!path.is_empty() && path != "/dev/null").then(|| path.to_string())
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
            ChatMessageBody::Plan(view) => view.draw(frame, area, ctx),
            ChatMessageBody::Todo(view) => view.draw(frame, area, ctx),
            ChatMessageBody::Artifact(view) => view.draw(frame, area, ctx),
            ChatMessageBody::CopyTarget(view) => view.draw(frame, area, ctx),
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
            ChatMessageBody::Plan(view) => view.min_width(),
            ChatMessageBody::Todo(view) => view.min_width(),
            ChatMessageBody::Artifact(view) => view.min_width(),
            ChatMessageBody::CopyTarget(view) => view.min_width(),
        }
    }

    fn min_height(&self) -> u16 {
        match self {
            ChatMessageBody::Markdown(view) => view.min_height(),
            ChatMessageBody::Text(view) => view.min_height(),
            ChatMessageBody::Disclosure(view) => view.min_height(),
            ChatMessageBody::Diff(view) => view.min_height(),
            ChatMessageBody::Plan(view) => view.min_height(),
            ChatMessageBody::Todo(view) => view.min_height(),
            ChatMessageBody::Artifact(view) => view.min_height(),
            ChatMessageBody::CopyTarget(view) => view.min_height(),
        }
    }

    fn desired_width(&self) -> Option<u16> {
        match self {
            ChatMessageBody::Markdown(view) => view.desired_width(),
            ChatMessageBody::Text(view) => view.desired_width(),
            ChatMessageBody::Disclosure(view) => view.desired_width(),
            ChatMessageBody::Diff(view) => view.desired_width(),
            ChatMessageBody::Plan(view) => view.desired_width(),
            ChatMessageBody::Todo(view) => view.desired_width(),
            ChatMessageBody::Artifact(view) => view.desired_width(),
            ChatMessageBody::CopyTarget(view) => view.desired_width(),
        }
    }

    fn desired_height(&self) -> Option<u16> {
        match self {
            ChatMessageBody::Markdown(view) => view.desired_height(),
            ChatMessageBody::Text(view) => view.desired_height(),
            ChatMessageBody::Disclosure(view) => view.desired_height(),
            ChatMessageBody::Diff(view) => view.desired_height(),
            ChatMessageBody::Plan(view) => view.desired_height(),
            ChatMessageBody::Todo(view) => view.desired_height(),
            ChatMessageBody::Artifact(view) => view.desired_height(),
            ChatMessageBody::CopyTarget(view) => view.desired_height(),
        }
    }
}

impl ::atto_ui::composable::Scrollable for ChatMessageBody {
    fn is_scrollable(&self) -> bool {
        match self {
            ChatMessageBody::Markdown(view) => view.is_scrollable(),
            ChatMessageBody::Text(view) => view.is_scrollable(),
            ChatMessageBody::Disclosure(view) => view.is_scrollable(),
            ChatMessageBody::Diff(view) => view.is_scrollable(),
            ChatMessageBody::Plan(view) => view.is_scrollable(),
            ChatMessageBody::Todo(view) => view.is_scrollable(),
            ChatMessageBody::Artifact(view) => view.is_scrollable(),
            ChatMessageBody::CopyTarget(view) => view.is_scrollable(),
        }
    }

    fn content_size(&self) -> (u16, u16) {
        match self {
            ChatMessageBody::Markdown(view) => view.content_size(),
            ChatMessageBody::Text(view) => view.content_size(),
            ChatMessageBody::Disclosure(view) => view.content_size(),
            ChatMessageBody::Diff(view) => view.content_size(),
            ChatMessageBody::Plan(view) => view.content_size(),
            ChatMessageBody::Todo(view) => view.content_size(),
            ChatMessageBody::Artifact(view) => view.content_size(),
            ChatMessageBody::CopyTarget(view) => view.content_size(),
        }
    }

    fn viewport_size(&self) -> (u16, u16) {
        match self {
            ChatMessageBody::Markdown(view) => view.viewport_size(),
            ChatMessageBody::Text(view) => view.viewport_size(),
            ChatMessageBody::Disclosure(view) => view.viewport_size(),
            ChatMessageBody::Diff(view) => view.viewport_size(),
            ChatMessageBody::Plan(view) => view.viewport_size(),
            ChatMessageBody::Todo(view) => view.viewport_size(),
            ChatMessageBody::Artifact(view) => view.viewport_size(),
            ChatMessageBody::CopyTarget(view) => view.viewport_size(),
        }
    }

    fn scroll_config(&self) -> ScrollConfig {
        match self {
            ChatMessageBody::Markdown(view) => view.scroll_config(),
            ChatMessageBody::Text(view) => view.scroll_config(),
            ChatMessageBody::Disclosure(view) => view.scroll_config(),
            ChatMessageBody::Diff(view) => view.scroll_config(),
            ChatMessageBody::Plan(view) => view.scroll_config(),
            ChatMessageBody::Todo(view) => view.scroll_config(),
            ChatMessageBody::Artifact(view) => view.scroll_config(),
            ChatMessageBody::CopyTarget(view) => view.scroll_config(),
        }
    }

    fn scroll_offset(&self) -> (u16, u16) {
        match self {
            ChatMessageBody::Markdown(view) => view.scroll_offset(),
            ChatMessageBody::Text(view) => view.scroll_offset(),
            ChatMessageBody::Disclosure(view) => view.scroll_offset(),
            ChatMessageBody::Diff(view) => view.scroll_offset(),
            ChatMessageBody::Plan(view) => view.scroll_offset(),
            ChatMessageBody::Todo(view) => view.scroll_offset(),
            ChatMessageBody::Artifact(view) => view.scroll_offset(),
            ChatMessageBody::CopyTarget(view) => view.scroll_offset(),
        }
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        match self {
            ChatMessageBody::Markdown(view) => view.set_scroll_offset(x, y),
            ChatMessageBody::Text(view) => view.set_scroll_offset(x, y),
            ChatMessageBody::Disclosure(view) => view.set_scroll_offset(x, y),
            ChatMessageBody::Diff(view) => view.set_scroll_offset(x, y),
            ChatMessageBody::Plan(view) => view.set_scroll_offset(x, y),
            ChatMessageBody::Todo(view) => view.set_scroll_offset(x, y),
            ChatMessageBody::Artifact(view) => view.set_scroll_offset(x, y),
            ChatMessageBody::CopyTarget(view) => view.set_scroll_offset(x, y),
        }
    }
}

impl ::atto_ui::composable::FocusNav for ChatMessageBody {
    fn is_focusable(&self) -> bool {
        match self {
            ChatMessageBody::Markdown(view) => view.is_focusable(),
            ChatMessageBody::Text(view) => view.is_focusable(),
            ChatMessageBody::Disclosure(view) => view.is_focusable(),
            ChatMessageBody::Diff(view) => view.is_focusable(),
            ChatMessageBody::Plan(view) => view.is_focusable(),
            ChatMessageBody::Todo(view) => view.is_focusable(),
            ChatMessageBody::Artifact(view) => view.is_focusable(),
            ChatMessageBody::CopyTarget(view) => view.is_focusable(),
        }
    }

    fn focus_first(&mut self) -> bool {
        match self {
            ChatMessageBody::Markdown(view) => view.focus_first(),
            ChatMessageBody::Text(view) => view.focus_first(),
            ChatMessageBody::Disclosure(view) => view.focus_first(),
            ChatMessageBody::Diff(view) => view.focus_first(),
            ChatMessageBody::Plan(view) => view.focus_first(),
            ChatMessageBody::Todo(view) => view.focus_first(),
            ChatMessageBody::Artifact(view) => view.focus_first(),
            ChatMessageBody::CopyTarget(view) => view.focus_first(),
        }
    }

    fn focus_last(&mut self) -> bool {
        match self {
            ChatMessageBody::Markdown(view) => view.focus_last(),
            ChatMessageBody::Text(view) => view.focus_last(),
            ChatMessageBody::Disclosure(view) => view.focus_last(),
            ChatMessageBody::Diff(view) => view.focus_last(),
            ChatMessageBody::Plan(view) => view.focus_last(),
            ChatMessageBody::Todo(view) => view.focus_last(),
            ChatMessageBody::Artifact(view) => view.focus_last(),
            ChatMessageBody::CopyTarget(view) => view.focus_last(),
        }
    }
}

impl ::atto_ui::composable::DynamicTree for ChatMessageBody {}

impl ::atto_ui::composable::EventHandling for ChatMessageBody {
    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        match self {
            ChatMessageBody::Markdown(view) => view.handle_event_capture(event, ctx),
            ChatMessageBody::Text(view) => view.handle_event_capture(event, ctx),
            ChatMessageBody::Disclosure(view) => view.handle_event_capture(event, ctx),
            ChatMessageBody::Diff(view) => view.handle_event_capture(event, ctx),
            ChatMessageBody::Plan(view) => view.handle_event_capture(event, ctx),
            ChatMessageBody::Todo(view) => view.handle_event_capture(event, ctx),
            ChatMessageBody::Artifact(view) => view.handle_event_capture(event, ctx),
            ChatMessageBody::CopyTarget(view) => view.handle_event_capture(event, ctx),
        }
    }

    fn handle_event_bubble(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        match self {
            ChatMessageBody::Markdown(view) => view.handle_event_bubble(event, ctx),
            ChatMessageBody::Text(view) => view.handle_event_bubble(event, ctx),
            ChatMessageBody::Disclosure(view) => view.handle_event_bubble(event, ctx),
            ChatMessageBody::Diff(view) => view.handle_event_bubble(event, ctx),
            ChatMessageBody::Plan(view) => view.handle_event_bubble(event, ctx),
            ChatMessageBody::Todo(view) => view.handle_event_bubble(event, ctx),
            ChatMessageBody::Artifact(view) => view.handle_event_bubble(event, ctx),
            ChatMessageBody::CopyTarget(view) => view.handle_event_bubble(event, ctx),
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        match self {
            ChatMessageBody::Markdown(view) => view.handle_event(event, ctx),
            ChatMessageBody::Text(view) => view.handle_event(event, ctx),
            ChatMessageBody::Disclosure(view) => view.handle_event(event, ctx),
            ChatMessageBody::Diff(view) => view.handle_event(event, ctx),
            ChatMessageBody::Plan(view) => view.handle_event(event, ctx),
            ChatMessageBody::Todo(view) => view.handle_event(event, ctx),
            ChatMessageBody::Artifact(view) => view.handle_event(event, ctx),
            ChatMessageBody::CopyTarget(view) => view.handle_event(event, ctx),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RenderedTextRow {
    text: String,
    width: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct RenderedTextPosition {
    row: u16,
    col: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RenderedTextSelectionRange {
    start: RenderedTextPosition,
    end: RenderedTextPosition,
}

#[derive(Clone, Debug, Default)]
struct RenderedTextSelectionState {
    anchor: Option<RenderedTextPosition>,
    focus: Option<RenderedTextPosition>,
}

impl RenderedTextSelectionState {
    fn start(&mut self, pos: RenderedTextPosition) {
        self.anchor = Some(pos);
        self.focus = Some(pos);
    }

    fn update(&mut self, pos: RenderedTextPosition) {
        if self.anchor.is_some() {
            self.focus = Some(pos);
        }
    }

    fn is_active(&self) -> bool {
        self.anchor.is_some()
    }

    fn range(&self) -> Option<RenderedTextSelectionRange> {
        let anchor = self.anchor?;
        let focus = self.focus?;
        if anchor == focus {
            return None;
        }
        let (start, end) = if anchor <= focus {
            (anchor, focus)
        } else {
            (focus, anchor)
        };
        Some(RenderedTextSelectionRange { start, end })
    }

    fn clear(&mut self) -> bool {
        let had_selection = self.anchor.is_some() || self.focus.is_some();
        self.anchor = None;
        self.focus = None;
        had_selection
    }
}

fn rendered_rows_from_buffer(buf: &Buffer, area: Rect) -> Vec<RenderedTextRow> {
    (0..area.height)
        .map(|dy| rendered_row_from_buffer(buf, area, dy))
        .collect()
}

fn rendered_row_from_buffer(buf: &Buffer, area: Rect, dy: u16) -> RenderedTextRow {
    let y = area.y.saturating_add(dy);
    let mut text = String::new();
    let mut dx = 0u16;
    while dx < area.width {
        let x = area.x.saturating_add(dx);
        let symbol = buf.cell((x, y)).map(|cell| cell.symbol()).unwrap_or(" ");
        let width = symbol.width().max(1).min(u16::MAX as usize) as u16;
        text.push_str(symbol);
        dx = dx.saturating_add(width.max(1));
    }
    let text = text.trim_end_matches(' ').to_string();
    let width = text.width().min(u16::MAX as usize) as u16;
    RenderedTextRow { text, width }
}

fn apply_rendered_selection(
    buf: &mut Buffer,
    area: Rect,
    rows: &[RenderedTextRow],
    range: RenderedTextSelectionRange,
    style: Style,
) {
    for (row_idx, row) in rows.iter().enumerate() {
        let Some((start, end)) = selection_cols_for_rendered_row(range, row_idx, row.width) else {
            continue;
        };
        for (start, end) in selected_cell_ranges_for_line(&row.text, start, end) {
            for dx in start..end.min(area.width) {
                let x = area.x.saturating_add(dx);
                let y = area
                    .y
                    .saturating_add(row_idx.min(usize::from(u16::MAX)) as u16);
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(style);
                }
            }
        }
    }
}

fn selection_cols_for_rendered_row(
    range: RenderedTextSelectionRange,
    row: usize,
    row_width: u16,
) -> Option<(u16, u16)> {
    let row = row.min(usize::from(u16::MAX)) as u16;
    if row < range.start.row || row > range.end.row {
        return None;
    }
    let (start, end) = if range.start.row == range.end.row {
        (range.start.col.min(row_width), range.end.col.min(row_width))
    } else if row == range.start.row {
        (range.start.col.min(row_width), row_width)
    } else if row == range.end.row {
        (0, range.end.col.min(row_width))
    } else {
        (0, row_width)
    };
    (start < end).then_some((start, end))
}

fn selected_cell_ranges_for_line(line: &str, start_col: u16, end_col: u16) -> Vec<(u16, u16)> {
    let mut ranges = Vec::new();
    let mut col = 0u16;
    for ch in line.chars() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1) as u16;
        let next = col.saturating_add(width);
        if start_col < next && end_col > col {
            ranges.push((col, next));
        }
        col = next;
    }
    ranges
}

fn selected_text_from_rendered_rows(
    rows: &[RenderedTextRow],
    range: RenderedTextSelectionRange,
) -> Option<String> {
    let start_row = usize::from(range.start.row);
    let end_row = usize::from(range.end.row);
    if start_row >= rows.len() || end_row >= rows.len() {
        return None;
    }

    let mut out = String::new();
    for (row_idx, row) in rows
        .iter()
        .enumerate()
        .take(end_row.saturating_add(1))
        .skip(start_row)
    {
        if row_idx > start_row {
            out.push('\n');
        }
        let Some((start, end)) = selection_cols_for_rendered_row(range, row_idx, row.width) else {
            continue;
        };
        out.push_str(&slice_line_by_display_cols(&row.text, start, end));
    }
    (!out.is_empty()).then_some(out)
}

fn slice_line_by_display_cols(line: &str, start_col: u16, end_col: u16) -> String {
    if start_col >= end_col {
        return String::new();
    }
    let start = byte_index_at_display_col_start(line, start_col);
    let end = byte_index_at_display_col_end(line, end_col).max(start);
    line[start..end].to_string()
}

fn byte_index_at_display_col_start(text: &str, target_col: u16) -> usize {
    let mut col = 0u16;
    for (byte_idx, ch) in text.char_indices() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1) as u16;
        let next = col.saturating_add(width);
        if target_col < next {
            return byte_idx;
        }
        col = next;
    }
    text.len()
}

fn byte_index_at_display_col_end(text: &str, target_col: u16) -> usize {
    let mut col = 0u16;
    for (byte_idx, ch) in text.char_indices() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1) as u16;
        let next = col.saturating_add(width);
        if target_col <= col {
            return byte_idx;
        }
        if target_col < next {
            return byte_idx.saturating_add(ch.len_utf8());
        }
        col = next;
    }
    text.len()
}

struct BlockCopyTarget {
    message_id: ChatMessageId,
    block_id: ChatBlockId,
    callback: MessageActionCallback,
    child_focused: bool,
    selection: RenderedTextSelectionState,
    rendered_rows: Vec<RenderedTextRow>,
    last_area: Option<Rect>,
    view: Box<ChatMessageBody>,
}

impl BlockCopyTarget {
    fn new(
        view: ChatMessageBody,
        message_id: ChatMessageId,
        block_id: ChatBlockId,
        callback: MessageActionCallback,
    ) -> Self {
        Self {
            message_id,
            block_id,
            callback,
            child_focused: false,
            selection: RenderedTextSelectionState::default(),
            rendered_rows: Vec::new(),
            last_area: None,
            view: Box::new(view),
        }
    }

    fn emit_copy(&self) {
        (self.callback)(MessageAction {
            message_id: self.message_id,
            kind: MessageActionKind::CopyBlock(self.block_id),
        });
    }

    fn copy_selected_text(&self) -> bool {
        let Some(text) = self.selected_text() else {
            return false;
        };
        let _ = atto_ui::clipboard::copy_to_system_clipboard(&text);
        true
    }

    fn selected_text(&self) -> Option<String> {
        selected_text_from_rendered_rows(&self.rendered_rows, self.selection.range()?)
    }

    fn selection_position_for_mouse(
        &self,
        mouse: &MouseEvent,
        coordinate_space: MouseCoordinateSpace,
        clamp_outside: bool,
    ) -> Option<RenderedTextPosition> {
        let area = self.last_area?;
        let (mut x, mut y) = match coordinate_space {
            MouseCoordinateSpace::Absolute => {
                if area.width == 0 || area.height == 0 {
                    return None;
                }
                if clamp_outside {
                    let max_x = area.x.saturating_add(area.width.saturating_sub(1));
                    let max_y = area.y.saturating_add(area.height.saturating_sub(1));
                    (
                        mouse.column.clamp(area.x, max_x).saturating_sub(area.x),
                        mouse.row.clamp(area.y, max_y).saturating_sub(area.y),
                    )
                } else {
                    mouse_position_in_area(area, mouse, coordinate_space)?
                }
            }
            MouseCoordinateSpace::Local => {
                if area.width == 0 || area.height == 0 {
                    return None;
                }
                if clamp_outside {
                    (
                        mouse.column.min(area.width.saturating_sub(1)),
                        mouse.row.min(area.height.saturating_sub(1)),
                    )
                } else {
                    mouse_position_in_area(area, mouse, coordinate_space)?
                }
            }
        };

        if self.rendered_rows.is_empty() {
            return None;
        }
        y = y.min(
            self.rendered_rows
                .len()
                .saturating_sub(1)
                .min(usize::from(u16::MAX)) as u16,
        );
        let row_width = self
            .rendered_rows
            .get(usize::from(y))
            .map_or(0, |row| row.width);
        x = x.min(row_width);
        Some(RenderedTextPosition { row: y, col: x })
    }

    fn handle_selection_mouse(
        &mut self,
        event: &Event,
        ctx: ComponentContext<'_>,
    ) -> Option<EventResult> {
        let Event::Mouse(mouse) = event else {
            return None;
        };

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let child_ctx = self.child_ctx(ctx);
                let child_res = self.view.handle_event(event, child_ctx);
                if child_res.is_consumed() {
                    return Some(child_res);
                }
                let pos =
                    self.selection_position_for_mouse(mouse, ctx.mouse_coordinate_space, false)?;
                self.selection.start(pos);
                Some(EventResult::consumed().with_capture(Capture::Request))
            }
            MouseEventKind::Drag(MouseButton::Left) if self.selection.is_active() => {
                if let Some(pos) =
                    self.selection_position_for_mouse(mouse, ctx.mouse_coordinate_space, true)
                {
                    self.selection.update(pos);
                }
                Some(EventResult::consumed())
            }
            MouseEventKind::Up(MouseButton::Left) if self.selection.is_active() => {
                if let Some(pos) =
                    self.selection_position_for_mouse(mouse, ctx.mouse_coordinate_space, true)
                {
                    self.selection.update(pos);
                }
                Some(EventResult::consumed().with_capture(Capture::Release))
            }
            _ => None,
        }
    }

    fn child_ctx<'a>(&self, ctx: ComponentContext<'a>) -> ComponentContext<'a> {
        ComponentContext {
            theme: ctx.theme,
            window_id: ctx.window_id,
            is_focused: ctx.is_focused && self.child_focused,
            scrollbar_host: ctx.scrollbar_host.for_child(),
            tab_mode: ctx.tab_mode.for_child(),
            mouse_coordinate_space: ctx.mouse_coordinate_space,
            drag: None,
        }
    }
}

impl Component for BlockCopyTarget {
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.last_area = Some(area);
        self.view.draw(frame, area, ctx);
        let selection = self.selection.range();
        let buf = frame.buffer_mut();
        self.rendered_rows = rendered_rows_from_buffer(buf, area);
        if let Some(range) = selection {
            apply_rendered_selection(buf, area, &self.rendered_rows, range, ctx.theme.selection);
        }
    }
}

impl ::atto_ui::composable::DragAndDrop for BlockCopyTarget {}

impl ::atto_ui::composable::Layout for BlockCopyTarget {
    fn min_width(&self) -> u16 {
        self.view.min_width()
    }

    fn min_height(&self) -> u16 {
        self.view.min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.view.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.view.desired_height()
    }
}

impl ::atto_ui::composable::Scrollable for BlockCopyTarget {
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

impl ::atto_ui::composable::FocusNav for BlockCopyTarget {
    fn is_focusable(&self) -> bool {
        true
    }

    fn focus_first(&mut self) -> bool {
        self.child_focused = false;
        true
    }

    fn focus_last(&mut self) -> bool {
        if self.view.is_focusable() {
            self.child_focused = true;
            let _ = self.view.focus_last();
        } else {
            self.child_focused = false;
        }
        true
    }
}

impl ::atto_ui::composable::DynamicTree for BlockCopyTarget {}

impl ::atto_ui::composable::EventHandling for BlockCopyTarget {
    fn handle_event_capture(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let Some(next) = copy_target_tab_direction(event) else {
            if self.child_focused {
                let child_ctx = self.child_ctx(ctx);
                return self.view.handle_event_capture(event, child_ctx);
            }
            return EventResult::ignored();
        };
        if !ctx.is_focused {
            return EventResult::ignored();
        }

        if next {
            if self.child_focused {
                let child_ctx = self.child_ctx(ctx);
                let res = self.view.handle_event_capture(event, child_ctx);
                if res.is_consumed() {
                    return res;
                }
                self.child_focused = false;
                return EventResult::ignored();
            }
            if self.view.is_focusable() {
                self.child_focused = true;
                let _ = self.view.focus_first();
                return EventResult::consumed();
            }
            return EventResult::ignored();
        }

        if self.child_focused {
            let child_ctx = self.child_ctx(ctx);
            let res = self.view.handle_event_capture(event, child_ctx);
            if res.is_consumed() {
                return res;
            }
            self.child_focused = false;
            return EventResult::consumed();
        }
        EventResult::ignored()
    }

    fn handle_event_bubble(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        let child_ctx = self.child_ctx(ctx);
        self.view.handle_event_bubble(event, child_ctx)
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        if let Some(res) = self.handle_selection_mouse(event, ctx) {
            return res;
        }

        if ctx.is_focused && is_copy_shortcut(event) {
            if self.copy_selected_text() {
                return EventResult::submitted();
            }
            self.emit_copy();
            return EventResult::submitted();
        }

        if ctx.is_focused
            && matches!(
                event,
                Event::Key(KeyEvent {
                    code: KeyCode::Esc,
                    kind: KeyEventKind::Press,
                    ..
                })
            )
            && self.selection.clear()
        {
            return EventResult::consumed();
        }

        let child_ctx = self.child_ctx(ctx);
        self.view.handle_event(event, child_ctx)
    }
}

fn is_copy_shortcut(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(KeyEvent {
            code: KeyCode::Char('c' | 'C'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER)
    )
}

fn copy_target_tab_direction(event: &Event) -> Option<bool> {
    match event {
        Event::Key(KeyEvent {
            code: KeyCode::Tab,
            modifiers,
            ..
        }) => Some(!modifiers.contains(KeyModifiers::SHIFT)),
        Event::Key(KeyEvent {
            code: KeyCode::BackTab,
            ..
        }) => Some(false),
        _ => None,
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
    use std::collections::{BTreeMap, HashSet};
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

    fn draw_component_bg_snapshot(
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
            let mut bgs = Vec::new();
            for x in 0..width {
                let cell = buf.cell((x, y)).expect("cell");
                line.push_str(cell.symbol());
                bgs.push(cell.bg);
            }
            lines.push(line);
            colors.push(bgs);
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
            store: ChatMessageStore::new(),
            wrap_width: None,
            responsive_wrap_width: Binding::new(None),
            in_progress_suffix: DEFAULT_IN_PROGRESS_SUFFIX.to_string(),
            show_timestamps: false,
            bubble_width_percent: DEFAULT_BUBBLE_WIDTH_PERCENT,
            collapsed_turns: Binding::new(HashSet::new()),
            on_open_artifact: None,
            on_approve: None,
            on_edit_decision: None,
            edit_and_resubmit: None,
            quote_replies: None,
            on_plan_decision: None,
            on_message_action: None,
            on_cancel: None,
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
    fn turn_action_specs_cover_role_appropriate_actions() {
        assert_eq!(
            turn_action_specs(&ChatRole::User),
            vec![
                ("Copy", MessageActionKind::Copy),
                ("Edit", MessageActionKind::EditUser),
            ]
        );
        assert_eq!(
            turn_action_specs(&ChatRole::Assistant),
            vec![
                ("Copy", MessageActionKind::Copy),
                ("Retry", MessageActionKind::Retry),
                ("Regenerate", MessageActionKind::Regenerate),
            ]
        );
    }

    #[test]
    fn message_action_button_emits_selected_action() {
        let message_id = ChatMessageId::new(50);
        let block_id = ChatBlockId::new(50_001);
        let actions = Arc::new(Mutex::new(Vec::new()));
        let captured = actions.clone();
        let mut button = message_action_button(
            "Copy block",
            message_id,
            MessageActionKind::CopyBlock(block_id),
            Arc::new(move |action| {
                captured.lock().expect("actions lock").push(action);
            }),
        );
        let theme = Theme::dark();

        button.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );

        assert_eq!(
            *actions.lock().expect("actions lock"),
            vec![MessageAction {
                message_id,
                kind: MessageActionKind::CopyBlock(block_id),
            }]
        );
    }

    #[test]
    fn quote_message_button_attaches_turn_reference() {
        let input = ChatInputHandle::new();
        let controller = QuoteReplyController::new(input.references_binding());
        let message =
            ChatMessage::text(ChatMessageId::new(70), ChatRole::Assistant, "hello\nthere");
        let mut button = quote_message_button("Quote", &message, controller);
        let theme = Theme::dark();

        button.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );

        assert_eq!(
            input.references(),
            vec![ChatInputReference::new(
                ChatMessageId::new(70),
                "Assistant #70",
                "hello there",
            )]
        );
    }

    #[test]
    fn quote_block_button_attaches_block_reference() {
        let input = ChatInputHandle::new();
        let controller = QuoteReplyController::new(input.references_binding());
        let block = ChatBlock::Text(TextBlock {
            id: ChatBlockId::new(71_001),
            markdown: "block quote body".to_string(),
            streaming: false,
        });
        let mut button =
            quote_block_button("Quote block", ChatMessageId::new(71), &block, controller);
        let theme = Theme::dark();

        button.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );

        assert_eq!(
            input.references(),
            vec![ChatInputReference::new(
                ChatMessageId::new(71),
                "Block #71001",
                "block quote body",
            )
            .block_id(ChatBlockId::new(71_001))]
        );
    }

    #[test]
    fn retry_and_regenerate_buttons_truncate_assistant_before_callback() {
        for (kind, label) in [
            (MessageActionKind::Retry, "Retry"),
            (MessageActionKind::Regenerate, "Regenerate"),
        ] {
            let store = ChatMessageStore::new();
            let user_id = store.next_message_id();
            let assistant_id = store.next_message_id();
            let later_id = store.next_message_id();
            store.push(ChatMessage::text(user_id, ChatRole::User, "prompt"));
            store.push(ChatMessage::text(
                assistant_id,
                ChatRole::Assistant,
                "old answer",
            ));
            store.push(ChatMessage::text(later_id, ChatRole::System, "old suffix"));
            let mut config = row_config_for_tests();
            config.store = store.clone();
            let observations = Arc::new(Mutex::new(Vec::new()));
            let captured = observations.clone();
            let store_for_callback = store.clone();
            let expected_kind = kind.clone();
            let mut button = turn_message_action_button(
                label,
                assistant_id,
                kind,
                &config,
                Arc::new(move |action| {
                    let visible_ids = store_for_callback
                        .messages()
                        .iter()
                        .map(|message| message.id)
                        .collect::<Vec<_>>();
                    captured
                        .lock()
                        .expect("observations lock")
                        .push((action, visible_ids));
                }),
            );
            let theme = Theme::dark();

            button.handle_event(
                &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                component_context(&theme),
            );

            assert_eq!(
                store
                    .messages()
                    .iter()
                    .map(|message| message.id)
                    .collect::<Vec<_>>(),
                vec![user_id]
            );
            assert_eq!(
                *observations.lock().expect("observations lock"),
                vec![(
                    MessageAction {
                        message_id: assistant_id,
                        kind: expected_kind,
                    },
                    vec![user_id]
                )]
            );
        }
    }

    #[test]
    fn retry_and_regenerate_buttons_ignore_non_assistant_or_missing_target() {
        let store = ChatMessageStore::new();
        let user_id = store.next_message_id();
        store.push(ChatMessage::text(user_id, ChatRole::User, "prompt"));
        let mut config = row_config_for_tests();
        config.store = store.clone();
        let actions = Arc::new(Mutex::new(Vec::new()));
        let captured = actions.clone();
        let callback: MessageActionCallback = Arc::new(move |action| {
            captured.lock().expect("actions lock").push(action);
        });
        let theme = Theme::dark();
        let mut user_retry = turn_message_action_button(
            "Retry",
            user_id,
            MessageActionKind::Retry,
            &config,
            callback.clone(),
        );
        let mut missing_regenerate = turn_message_action_button(
            "Regenerate",
            ChatMessageId::new(999),
            MessageActionKind::Regenerate,
            &config,
            callback,
        );

        user_retry.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );
        missing_regenerate.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );

        assert_eq!(
            store
                .messages()
                .iter()
                .map(|message| message.id)
                .collect::<Vec<_>>(),
            vec![user_id]
        );
        assert!(actions.lock().expect("actions lock").is_empty());
    }

    #[test]
    fn editable_user_message_text_extracts_only_user_text_blocks() {
        let user_id = ChatMessageId::new(60);
        let user = ChatMessage::new(
            user_id,
            ChatRole::User,
            vec![
                ChatBlock::Text(TextBlock {
                    id: ChatBlockId::new(60_001),
                    markdown: "first".to_string(),
                    streaming: false,
                }),
                ChatBlock::Attachment(AttachmentBlock {
                    id: ChatBlockId::new(60_002),
                    name: "notes.md".to_string(),
                    url: None,
                    mime: None,
                }),
                ChatBlock::Text(TextBlock {
                    id: ChatBlockId::new(60_003),
                    markdown: "second".to_string(),
                    streaming: false,
                }),
            ],
        );
        let assistant = ChatMessage::text(ChatMessageId::new(61), ChatRole::Assistant, "answer");
        let attachment_only = ChatMessage::new(
            ChatMessageId::new(62),
            ChatRole::User,
            vec![ChatBlock::Attachment(AttachmentBlock {
                id: ChatBlockId::new(62_001),
                name: "image.png".to_string(),
                url: None,
                mime: None,
            })],
        );

        assert_eq!(
            editable_user_message_text(&user),
            Some("first\n\nsecond".to_string())
        );
        assert_eq!(editable_user_message_text(&assistant), None);
        assert_eq!(editable_user_message_text(&attachment_only), None);
    }

    #[test]
    fn edit_and_resubmit_refills_input_truncates_and_emits_event() {
        let store = ChatMessageStore::new();
        let user_id = store.next_message_id();
        let assistant_id = store.next_message_id();
        store.push(ChatMessage::text(user_id, ChatRole::User, "old prompt"));
        store.push(ChatMessage::text(
            assistant_id,
            ChatRole::Assistant,
            "old answer",
        ));
        let input = ChatInputHandle::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = events.clone();
        let list = ChatMessageList::new(store.clone()).on_edit_and_resubmit(&input, move |event| {
            captured.lock().expect("events lock").push(event);
        });
        let controller = list
            .config
            .edit_and_resubmit
            .clone()
            .expect("edit controller should be registered");
        let theme = Theme::dark();
        let mut panel = input.panel();

        assert!(controller.begin_edit(user_id, "old prompt".to_string()));
        assert_eq!(input.draft_binding().get(), "old prompt");
        input.draft_binding().set("edited prompt".to_string());

        let result = panel.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );

        assert_eq!(result, EventResult::submitted());
        assert_eq!(input.draft_binding().get(), "");
        assert!(store.messages().is_empty());
        let events = events.lock().expect("events lock");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].message_id, user_id);
        assert_eq!(events[0].original_text, "old prompt");
        assert_eq!(events[0].edited_text, "edited prompt");
        assert_eq!(
            events[0]
                .removed_messages
                .iter()
                .map(|message| message.id)
                .collect::<Vec<_>>(),
            vec![user_id, assistant_id]
        );
    }

    #[test]
    fn edit_submit_consumes_pending_edit_when_target_was_removed() {
        let store = ChatMessageStore::new();
        let user_id = store.next_message_id();
        let assistant_id = store.next_message_id();
        store.push(ChatMessage::text(user_id, ChatRole::User, "old prompt"));
        store.push(ChatMessage::text(
            assistant_id,
            ChatRole::Assistant,
            "old answer",
        ));
        let input = ChatInputHandle::new();
        let edit_events = Arc::new(Mutex::new(Vec::new()));
        let ordinary_submits = Arc::new(Mutex::new(0usize));
        let list = ChatMessageList::new(store.clone()).on_edit_and_resubmit(&input, {
            let edit_events = edit_events.clone();
            move |event| edit_events.lock().expect("events lock").push(event)
        });
        let controller = list
            .config
            .edit_and_resubmit
            .clone()
            .expect("edit controller should be registered");
        let mut panel = input.panel().on_submit({
            let ordinary_submits = ordinary_submits.clone();
            move |_| {
                *ordinary_submits.lock().expect("ordinary submits lock") += 1;
            }
        });
        let theme = Theme::dark();

        assert!(controller.begin_edit(user_id, "old prompt".to_string()));
        assert!(store.truncate_from(user_id).is_some());
        input.draft_binding().set("edited prompt".to_string());

        let result = panel.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );

        assert_eq!(result, EventResult::submitted());
        assert_eq!(input.draft_binding().get(), "");
        assert!(store.messages().is_empty());
        assert!(edit_events.lock().expect("events lock").is_empty());
        assert_eq!(*ordinary_submits.lock().expect("ordinary submits lock"), 0);
    }

    #[test]
    fn edit_button_uses_dedicated_resubmit_controller_when_configured() {
        let store = ChatMessageStore::new();
        let input = ChatInputHandle::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let list = ChatMessageList::new(store.clone()).on_edit_and_resubmit(&input, {
            let events = events.clone();
            move |event| events.lock().expect("events lock").push(event)
        });
        let controller = list
            .config
            .edit_and_resubmit
            .clone()
            .expect("edit controller should be registered");
        let mut button = edit_and_resubmit_button(
            "Edit",
            ChatMessageId::new(70),
            "draft text".to_string(),
            controller,
        );
        let theme = Theme::dark();

        button.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );

        assert_eq!(input.draft_binding().get(), "draft text");
        assert!(events.lock().expect("events lock").is_empty());
    }

    #[test]
    fn block_copy_target_emits_copy_action_on_shortcut() {
        let message_id = ChatMessageId::new(51);
        let block_id = ChatBlockId::new(51_001);
        let actions = Arc::new(Mutex::new(Vec::new()));
        let captured = actions.clone();
        let mut body = ChatMessageBody::Text(Text::new("COPY-TARGET")).with_copy_shortcut(
            message_id,
            block_id,
            Arc::new(move |action| {
                captured.lock().expect("actions lock").push(action);
            }),
        );
        let theme = Theme::dark();

        assert!(body.is_focusable());
        let result = body.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            component_context(&theme),
        );

        assert!(result.is_consumed());
        assert_eq!(
            *actions.lock().expect("actions lock"),
            vec![MessageAction {
                message_id,
                kind: MessageActionKind::CopyBlock(block_id),
            }]
        );
    }

    #[test]
    fn rendered_selection_spans_visual_rows_and_wide_chars() {
        let rows = vec![
            RenderedTextRow {
                text: "alpha 你".to_string(),
                width: 8,
            },
            RenderedTextRow {
                text: "beta".to_string(),
                width: 4,
            },
        ];
        let range = RenderedTextSelectionRange {
            start: RenderedTextPosition { row: 0, col: 6 },
            end: RenderedTextPosition { row: 1, col: 2 },
        };

        assert_eq!(
            selected_text_from_rendered_rows(&rows, range).as_deref(),
            Some("你\nbe")
        );
        assert_eq!(
            selected_cell_ranges_for_line("alpha 你", 6, 7),
            vec![(6, 8)]
        );
    }

    #[test]
    fn block_copy_target_renders_drag_selection_highlight() {
        let message_id = ChatMessageId::new(52);
        let block_id = ChatBlockId::new(52_001);
        let actions = Arc::new(Mutex::new(Vec::new()));
        let captured = actions.clone();
        let mut body = ChatMessageBody::Text(Text::new("SELECT ME")).with_copy_shortcut(
            message_id,
            block_id,
            Arc::new(move |action| {
                captured.lock().expect("actions lock").push(action);
            }),
        );
        let theme = Theme::dark();
        let ctx = component_context(&theme);

        let (_, before) = draw_component_bg_snapshot(&mut body, 20, 1);
        body.handle_event(
            &Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 0,
                row: 0,
                modifiers: KeyModifiers::empty(),
            }),
            ctx,
        );
        body.handle_event(
            &Event::Mouse(MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: 6,
                row: 0,
                modifiers: KeyModifiers::empty(),
            }),
            ctx,
        );
        let (_, after) = draw_component_bg_snapshot(&mut body, 20, 1);

        assert_ne!(before[0][0], after[0][0]);
        assert_eq!(after[0][0], Color::LightBlue);
        assert_eq!(
            *actions.lock().expect("actions lock"),
            Vec::<MessageAction>::new()
        );
    }

    #[test]
    fn streaming_cancel_button_emits_only_while_streaming() {
        let message_id = ChatMessageId::new(60);
        let store = ChatMessageStore::new();
        store.push(
            ChatMessage::text(message_id, ChatRole::Assistant, "streaming")
                .with_status(ChatTurnStatus::Streaming),
        );
        let status = Binding::new(ChatTurnStatus::Streaming);
        let canceled = Arc::new(Mutex::new(Vec::new()));
        let captured = canceled.clone();
        let controller = StreamingCancelController::new(
            store.clone(),
            Arc::new(move |id| captured.lock().expect("cancel lock").push(id)),
        );
        let mut button = streaming_cancel_button("Cancel", message_id, status.clone(), controller);
        let theme = Theme::dark();

        assert!(button.is_focusable());
        button.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );
        assert_eq!(*canceled.lock().expect("cancel lock"), vec![message_id]);
        assert_eq!(store.messages()[0].status, ChatTurnStatus::Canceled);

        status.set(ChatTurnStatus::Canceled);
        assert!(!button.is_focusable());
        button.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );
        assert_eq!(*canceled.lock().expect("cancel lock"), vec![message_id]);
    }

    #[test]
    fn streaming_cancel_controller_marks_canceled_before_callback() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        store.push(
            ChatMessage::text(message_id, ChatRole::Assistant, "streaming")
                .with_status(ChatTurnStatus::Streaming),
        );
        let observations = Arc::new(Mutex::new(Vec::new()));
        let captured = observations.clone();
        let store_for_callback = store.clone();
        let controller = StreamingCancelController::new(
            store.clone(),
            Arc::new(move |id| {
                let status = store_for_callback.messages()[0].status.clone();
                captured
                    .lock()
                    .expect("observations lock")
                    .push((id, status));
            }),
        );

        assert!(controller.request(message_id));
        assert!(!controller.request(message_id));

        assert_eq!(store.messages()[0].status, ChatTurnStatus::Canceled);
        assert_eq!(
            *observations.lock().expect("observations lock"),
            vec![(message_id, ChatTurnStatus::Canceled)]
        );
    }

    #[test]
    fn list_escape_cancels_latest_streaming_turn_once() {
        let store = ChatMessageStore::new();
        let first_id = store.next_message_id();
        let current_id = store.next_message_id();
        store.push(ChatMessage::text(first_id, ChatRole::User, "prompt"));
        store.push(
            ChatMessage::text(current_id, ChatRole::Assistant, "streaming")
                .with_status(ChatTurnStatus::Streaming),
        );
        let canceled = Arc::new(Mutex::new(Vec::new()));
        let captured = canceled.clone();
        let mut list = ChatMessageList::new(store.clone()).on_cancel(move |id| {
            captured.lock().expect("cancel lock").push(id);
        });
        let theme = Theme::dark();
        draw_chat_list(&mut list, 60, 10);

        let first = list.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            component_context(&theme),
        );
        let second = list.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            component_context(&theme),
        );

        assert_eq!(first, EventResult::changed());
        assert_eq!(second, EventResult::ignored());
        assert_eq!(*canceled.lock().expect("cancel lock"), vec![current_id]);
        assert_eq!(store.messages()[1].status, ChatTurnStatus::Canceled);
    }

    #[test]
    fn turn_collapse_button_toggles_collapsed_state() {
        let message_id = ChatMessageId::new(90);
        let collapsed_turns = Binding::new(HashSet::new());
        let theme = Theme::dark();

        let mut collapse =
            turn_collapse_button("Collapse", message_id, false, collapsed_turns.clone());
        collapse.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );
        assert!(collapsed_turns.get().contains(&message_id));

        let mut expand = turn_collapse_button("Expand", message_id, true, collapsed_turns.clone());
        expand.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );
        assert!(!collapsed_turns.get().contains(&message_id));
    }

    #[test]
    fn row_keys_for_collapsed_turn_keep_only_header() {
        let message_id = ChatMessageId::new(91);
        let message = ChatMessage::new(
            message_id,
            ChatRole::Assistant,
            vec![
                ChatBlock::Text(TextBlock {
                    id: ChatBlockId::new(91_001),
                    markdown: "TURN-BODY".to_string(),
                    streaming: false,
                }),
                ChatBlock::Notice(NoticeBlock {
                    id: ChatBlockId::new(91_002),
                    level: NoticeLevel::Info,
                    text: "TURN-NOTICE".to_string(),
                }),
            ],
        );
        let collapsed = HashSet::from([message_id]);

        let keys = row_keys_from_messages_with_collapsed(&[message], &collapsed);

        assert_eq!(keys.len(), 1);
        assert!(matches!(
            keys[0],
            ChatRowKey::Header {
                message_id: id,
                collapsed: true,
            } if id == message_id
        ));
    }

    #[test]
    fn chat_turn_fold_hides_blocks_and_shows_placeholder() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        store.push(ChatMessage::text(
            message_id,
            ChatRole::Assistant,
            "TURN-FOLD-BODY",
        ));
        let mut list = ChatMessageList::new(store)
            .show_timestamps(false)
            .auto_scroll(false);
        let theme = Theme::dark();

        let (initial, _) = draw_component_snapshot(&mut list, 80, 8);
        assert!(initial.iter().any(|line| line.contains("Collapse")));
        assert!(initial.iter().any(|line| line.contains("TURN-FOLD-BODY")));

        list.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );
        let (collapsed, _) = draw_component_snapshot(&mut list, 80, 8);

        assert!(list.config.collapsed_turns.get().contains(&message_id));
        assert!(collapsed.iter().any(|line| line.contains("Expand")));
        assert!(
            collapsed
                .iter()
                .any(|line| line.contains("Collapsed · 1 block hidden"))
        );
        assert!(!collapsed.iter().any(|line| line.contains("TURN-FOLD-BODY")));

        list.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );
        let (expanded, _) = draw_component_snapshot(&mut list, 80, 8);

        assert!(!list.config.collapsed_turns.get().contains(&message_id));
        assert!(expanded.iter().any(|line| line.contains("Collapse")));
        assert!(expanded.iter().any(|line| line.contains("TURN-FOLD-BODY")));
    }

    #[test]
    fn expanding_turn_restores_pre_collapse_scroll_offset() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        let blocks = (0..8)
            .map(|idx| {
                ChatBlock::Text(TextBlock {
                    id: ChatBlockId::new(92_000 + idx),
                    markdown: format!("RESTORE-BODY-{idx}"),
                    streaming: false,
                })
            })
            .collect::<Vec<_>>();
        store.push(ChatMessage::new(message_id, ChatRole::Assistant, blocks));
        for idx in 0..10 {
            store.push(ChatMessage::text(
                store.next_message_id(),
                ChatRole::Assistant,
                format!("TAIL-{idx}"),
            ));
        }
        let mut list = ChatMessageList::new(store)
            .show_timestamps(false)
            .auto_scroll(false);
        let theme = Theme::dark();
        draw_chat_list(&mut list, 80, 6);
        list.set_scroll_offset(0, 2);
        list.sync_follow_tail_from_scroll();
        let restore_y = list.scroll_offset().1;

        list.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );
        draw_chat_list(&mut list, 80, 6);
        assert!(list.config.collapsed_turns.get().contains(&message_id));

        list.set_scroll_offset(0, 0);
        list.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            component_context(&theme),
        );
        draw_chat_list(&mut list, 80, 6);

        assert!(!list.config.collapsed_turns.get().contains(&message_id));
        assert_eq!(list.scroll_offset().1, restore_y);
    }

    #[test]
    fn chat_search_opens_filters_and_highlights_visible_matches() {
        let store = ChatMessageStore::new();
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::Assistant,
            "alpha needle 你",
        ));
        let mut list = ChatMessageList::new(store)
            .show_timestamps(false)
            .auto_scroll(false);
        let theme = Theme::dark();
        draw_chat_list(&mut list, 60, 6);

        list.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL)),
            component_context(&theme),
        );
        for ch in "needle".chars() {
            list.handle_event(
                &Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
                component_context(&theme),
            );
        }

        let (lines, bgs) = draw_component_bg_snapshot(&mut list, 60, 6);
        let (row, col) = lines
            .iter()
            .enumerate()
            .find_map(|(row, line)| line.find("needle").map(|col| (row, col)))
            .expect("search match should be visible");

        assert!(list.search.active);
        assert_eq!(list.search.query, "needle");
        assert_eq!(list.search.matches.len(), 1);
        assert_eq!(bgs[row][col], Color::Yellow);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Search: needle (1/1)"))
        );
    }

    #[test]
    fn chat_search_next_previous_jump_to_offscreen_matches() {
        let store = ChatMessageStore::new();
        for idx in 0..32 {
            let text = match idx {
                1 => "TARGET-FIRST".to_string(),
                28 => "TARGET-SECOND".to_string(),
                _ => format!("message-{idx:02}"),
            };
            store.push(ChatMessage::text(
                store.next_message_id(),
                ChatRole::Assistant,
                text,
            ));
        }
        let mut list = ChatMessageList::new(store)
            .show_timestamps(false)
            .auto_scroll(false);
        let theme = Theme::dark();
        draw_chat_list(&mut list, 50, 6);

        list.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL)),
            component_context(&theme),
        );
        for ch in "target".chars() {
            list.handle_event(
                &Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
                component_context(&theme),
            );
        }
        let (first, _) = draw_component_snapshot(&mut list, 50, 6);
        assert!(first.iter().any(|line| line.contains("TARGET-FIRST")));

        list.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            component_context(&theme),
        );
        let (second, _) = draw_component_snapshot(&mut list, 50, 6);
        assert!(second.iter().any(|line| line.contains("TARGET-SECOND")));
        assert!(!second.iter().any(|line| line.contains("TARGET-FIRST")));
        assert!(list.scroll_offset().1 > 0);
        assert!(!list.is_following_tail());

        list.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            component_context(&theme),
        );
        let (previous, _) = draw_component_snapshot(&mut list, 50, 6);
        assert!(previous.iter().any(|line| line.contains("TARGET-FIRST")));
    }

    #[test]
    fn chat_search_escape_restores_scroll_and_clears_overlay() {
        let store = store_with_text_messages(30);
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::Assistant,
            "RESTORE-TARGET",
        ));
        let mut list = ChatMessageList::new(store)
            .show_timestamps(false)
            .auto_scroll(false);
        let theme = Theme::dark();
        draw_chat_list(&mut list, 50, 6);
        list.set_scroll_offset(0, 4);
        list.sync_follow_tail_from_scroll();
        let restore_y = list.scroll_offset().1;

        list.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL)),
            component_context(&theme),
        );
        for ch in "restore-target".chars() {
            list.handle_event(
                &Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
                component_context(&theme),
            );
        }
        draw_chat_list(&mut list, 50, 6);
        assert!(list.scroll_offset().1 > restore_y);

        list.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            component_context(&theme),
        );
        let (lines, bgs) = draw_component_bg_snapshot(&mut list, 50, 6);

        assert!(!list.search.active);
        assert_eq!(list.scroll_offset().1, restore_y);
        assert!(!lines.iter().any(|line| line.contains("Search:")));
        assert!(bgs.iter().flatten().all(|bg| *bg != Color::Yellow));
    }

    #[test]
    fn host_can_detect_unfollowed_tail_and_request_scroll_to_bottom() {
        let store = store_with_text_messages(20);
        let mut list = ChatMessageList::new(store).show_timestamps(false);

        draw_chat_list(&mut list, 40, 6);
        assert!(list.is_following_tail());

        list.set_scroll_offset(0, 0);
        list.sync_follow_tail_from_scroll();
        assert!(!list.is_following_tail());

        list.scroll_to_bottom();
        draw_chat_list(&mut list, 40, 6);
        assert!(list.is_following_tail());
        assert_eq!(list.scroll_offset().1, list.max_scroll_y());
    }

    #[test]
    fn branch_truncate_restores_tail_following_after_user_scrolled_up() {
        let store = store_with_text_messages(40);
        let mut list = ChatMessageList::new(store.clone()).show_timestamps(false);
        draw_chat_list(&mut list, 40, 6);
        list.set_scroll_offset(0, 0);
        list.sync_follow_tail_from_scroll();
        assert!(!list.is_following_tail());

        assert!(store.truncate_from(ChatMessageId::new(30)).is_some());
        draw_chat_list(&mut list, 40, 6);

        assert!(list.is_following_tail());
        assert_eq!(list.scroll_offset().1, list.max_scroll_y());
    }

    #[test]
    fn branch_tail_rewrite_restores_tail_following_even_when_row_count_matches() {
        let store = store_with_text_messages(40);
        let mut list = ChatMessageList::new(store.clone()).show_timestamps(false);
        draw_chat_list(&mut list, 40, 6);
        list.set_scroll_offset(0, 0);
        list.sync_follow_tail_from_scroll();
        assert!(!list.is_following_tail());

        assert!(store.fork_at(ChatMessageId::new(25)).is_some());
        for idx in 0..15 {
            store.push(ChatMessage::text(
                store.next_message_id(),
                ChatRole::Assistant,
                format!("NEW-{idx:02}"),
            ));
        }
        draw_chat_list(&mut list, 40, 6);

        assert!(list.is_following_tail());
        assert_eq!(list.scroll_offset().1, list.max_scroll_y());
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
    fn plan_body_renders_items_and_binding_updates() {
        let items = Binding::new(vec![
            PlanItem {
                text: "PLAN-STEP-1".to_string(),
            },
            PlanItem {
                text: "PLAN-STEP-2".to_string(),
            },
        ]);
        let mut view = PlanDecisionView::new(
            items.clone(),
            ChatMessageId::new(1),
            ChatBlockId::new(1_001),
            PlanDecision::Pending,
            None,
        );

        let (initial, _) = draw_component_snapshot(&mut view, 40, 4);
        assert!(initial[0].contains("Plan: pending"));
        assert!(initial[1].starts_with("- PLAN-STEP-1"));
        assert!(initial[2].starts_with("- PLAN-STEP-2"));
        assert!(initial[3].contains("Accept"));

        items.set(vec![PlanItem {
            text: "PLAN-STEP-3".to_string(),
        }]);
        let (updated, _) = draw_component_snapshot(&mut view, 40, 3);
        assert!(updated[1].starts_with("- PLAN-STEP-3"));
        assert!(!updated.iter().any(|line| line.contains("PLAN-STEP-2")));
    }

    #[test]
    fn task_body_renders_nested_transcript_and_binding_updates() {
        let details = Binding::new(TaskDetails {
            title: "SUBAGENT-SEARCH".to_string(),
            status: TaskStatus::Running,
            summary: "SUBAGENT-INITIAL".to_string(),
            transcript: vec![TaskTranscriptItem {
                role: ChatRole::Assistant,
                blocks: vec![ChatBlock::Text(TextBlock {
                    id: ChatBlockId::new(90_001),
                    markdown: "NESTED-SEARCH".to_string(),
                    streaming: false,
                })],
            }],
        });
        let mut view = TaskBlockView::new(details.clone());

        let (initial, _) = draw_component_snapshot(&mut view, 80, 6);
        assert!(initial[0].contains("Task: SUBAGENT-SEARCH (running)"));
        assert!(initial.iter().any(|line| line.contains("Status: running")));
        assert!(
            initial
                .iter()
                .any(|line| line.contains("Summary: SUBAGENT-INITIAL"))
        );
        assert!(initial.iter().any(|line| line.contains("Assistant:")));
        assert!(initial.iter().any(|line| line.contains("NESTED-SEARCH")));

        details.set(TaskDetails {
            title: "SUBAGENT-SEARCH".to_string(),
            status: TaskStatus::Complete,
            summary: "SUBAGENT-DONE".to_string(),
            transcript: vec![TaskTranscriptItem {
                role: ChatRole::Assistant,
                blocks: vec![ChatBlock::Text(TextBlock {
                    id: ChatBlockId::new(90_002),
                    markdown: "NESTED-FINAL".to_string(),
                    streaming: false,
                })],
            }],
        });
        let (updated, _) = draw_component_snapshot(&mut view, 80, 6);
        assert!(updated[0].contains("Task: SUBAGENT-SEARCH (complete)"));
        assert!(updated.iter().any(|line| line.contains("Status: complete")));
        assert!(
            updated
                .iter()
                .any(|line| line.contains("Summary: SUBAGENT-DONE"))
        );
        assert!(updated.iter().any(|line| line.contains("NESTED-FINAL")));
        assert!(!updated.iter().any(|line| line.contains("NESTED-SEARCH")));
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
    fn chat_list_syncs_task_details_from_store_updates() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        let task_id = ChatBlockId::new(71_001);
        store.push(ChatMessage::new(
            message_id,
            ChatRole::Assistant,
            vec![ChatBlock::Task(TaskBlock {
                id: task_id,
                title: "SUBAGENT-SEARCH".to_string(),
                status: TaskStatus::Running,
                summary: "SUBAGENT-INITIAL".to_string(),
                transcript: vec![TaskTranscriptItem {
                    role: ChatRole::Assistant,
                    blocks: vec![ChatBlock::Text(TextBlock {
                        id: ChatBlockId::new(71_101),
                        markdown: "NESTED-SEARCH".to_string(),
                        streaming: false,
                    })],
                }],
                collapsed: false,
            })],
        ));
        let mut list = ChatMessageList::new(store.clone())
            .show_timestamps(false)
            .auto_scroll(false);

        let (initial, _) = draw_component_snapshot(&mut list, 80, 12);
        assert!(initial.iter().any(|line| line.contains("Status: running")));
        assert!(initial.iter().any(|line| line.contains("NESTED-SEARCH")));

        assert!(store.set_task_status(task_id, TaskStatus::Complete));
        assert!(store.set_task_summary(task_id, "SUBAGENT-DONE"));
        assert!(store.set_task_transcript(
            task_id,
            vec![TaskTranscriptItem {
                role: ChatRole::Assistant,
                blocks: vec![ChatBlock::Text(TextBlock {
                    id: ChatBlockId::new(71_102),
                    markdown: "NESTED-FINAL".to_string(),
                    streaming: false,
                })],
            }]
        ));
        let (updated, _) = draw_component_snapshot(&mut list, 80, 12);
        assert!(updated.iter().any(|line| line.contains("Status: complete")));
        assert!(updated.iter().any(|line| line.contains("SUBAGENT-DONE")));
        assert!(updated.iter().any(|line| line.contains("NESTED-FINAL")));
        assert!(!updated.iter().any(|line| line.contains("NESTED-SEARCH")));
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
        list.virtual_control
            .preserve_scroll_y_after_next_layout(previous_content_h, previous_scroll_y);

        draw_chat_list(&mut list, 40, 6);

        let inserted_height = list.content_size().1.saturating_sub(previous_content_h);
        assert!(inserted_height > 0, "prepended rows should increase height");
        assert_eq!(list.scroll_offset().1, inserted_height);
    }

    #[test]
    fn chat_list_virtualizes_long_sessions_to_visible_rows() {
        let store = ChatMessageStore::new();
        for idx in 0..300 {
            store.push(ChatMessage::tool_call(
                store.next_message_id(),
                format!("tool-{idx}"),
                ToolStatus::Done,
                format!("output-{idx}"),
            ));
        }
        let total_rows = store
            .binding()
            .with(|messages| row_keys_from_messages(messages).len());
        let mut list = ChatMessageList::new(store)
            .show_timestamps(false)
            .auto_scroll(false);

        draw_chat_list(&mut list, 80, 10);

        assert!(total_rows >= 900, "fixture should contain many chat rows");
        assert!(
            list.realized_row_count() < 80,
            "only the visible window should be realized, got {} of {total_rows}",
            list.realized_row_count()
        );

        list.set_scroll_offset(0, list.content_size().1);
        draw_chat_list(&mut list, 80, 10);

        assert!(
            list.realized_row_count() < 80,
            "scrolling should prune offscreen rows instead of retaining all {total_rows}"
        );
    }

    #[test]
    fn chat_list_virtualizes_nested_task_rows_to_visible_window() {
        let store = ChatMessageStore::new();
        for idx in 0..180 {
            let id = store.next_message_id();
            store.push(ChatMessage::new(
                id,
                ChatRole::Assistant,
                vec![ChatBlock::Task(TaskBlock {
                    id: ChatBlockId::new(id.0.saturating_mul(1_000).saturating_add(1)),
                    title: format!("task-{idx}"),
                    status: TaskStatus::Running,
                    summary: format!("summary-{idx}"),
                    transcript: vec![TaskTranscriptItem {
                        role: ChatRole::Assistant,
                        blocks: vec![ChatBlock::Text(TextBlock {
                            id: ChatBlockId::new(id.0.saturating_mul(1_000).saturating_add(2)),
                            markdown: format!("nested-{idx}"),
                            streaming: false,
                        })],
                    }],
                    collapsed: false,
                })],
            ));
        }
        let total_rows = store
            .binding()
            .with(|messages| row_keys_from_messages(messages).len());
        let mut list = ChatMessageList::new(store)
            .show_timestamps(false)
            .auto_scroll(false);

        draw_chat_list(&mut list, 80, 10);

        assert!(total_rows >= 360, "fixture should contain many task rows");
        assert!(
            list.realized_row_count() < 80,
            "task rows should be virtualized to the visible window, got {} of {total_rows}",
            list.realized_row_count()
        );
    }

    #[test]
    fn virtual_chat_rows_dispatch_mouse_to_visible_buttons() {
        let store = ChatMessageStore::new();
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "ACTION-USER-MESSAGE",
        ));
        let actions = Arc::new(Mutex::new(Vec::new()));
        let captured = actions.clone();
        let mut list = ChatMessageList::new(store)
            .show_timestamps(false)
            .auto_scroll(false)
            .on_message_action(move |action| {
                captured.lock().expect("actions lock").push(action);
            });
        let theme = Theme::dark();
        let ctx = component_context(&theme);
        let area = Rect::new(2, 3, 80, 12);
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).expect("terminal");
        terminal.draw(|f| list.draw(f, area, ctx)).expect("draw");
        let buf = terminal.backend().buffer();
        let mut lines = Vec::new();
        for y in 0..20u16 {
            let mut line = String::new();
            for x in 0..100u16 {
                line.push_str(buf.cell((x, y)).expect("cell").symbol());
            }
            lines.push(line);
        }
        let (y, x) = lines
            .iter()
            .enumerate()
            .find_map(|(y, line)| line.find("Edit").map(|x| (y as u16, x as u16)))
            .expect("edit button should render");
        let down = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::empty(),
        });
        let up = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::empty(),
        });

        list.handle_event(&down, component_context(&theme));
        list.handle_event(&up, component_context(&theme));

        assert_eq!(
            *actions.lock().expect("actions lock"),
            vec![MessageAction {
                message_id: ChatMessageId::new(1),
                kind: MessageActionKind::EditUser,
            }]
        );
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
            Some("src/lib.rs".to_string()),
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
            Some("src/lib.rs".to_string()),
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
    fn plan_decision_view_emits_decision_and_locks_when_resolved() {
        let message_id = ChatMessageId::new(41);
        let block_id = ChatBlockId::new(41_001);
        let decisions = Arc::new(Mutex::new(Vec::new()));
        let captured = decisions.clone();
        let mut view = PlanDecisionView::new(
            Binding::new(vec![PlanItem {
                text: "PLAN-STEP".to_string(),
            }]),
            message_id,
            block_id,
            PlanDecision::Pending,
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
            vec![PlanDecisionEvent {
                message_id,
                block_id,
                decision: PlanDecision::Accepted,
            }]
        );

        let locked_view = PlanDecisionView::new(
            Binding::new(vec![PlanItem {
                text: "PLAN-STEP".to_string(),
            }]),
            message_id,
            block_id,
            PlanDecision::Accepted,
            Some(Arc::new(|_| {
                panic!("resolved plan decision must be locked")
            })),
        );

        assert!(!locked_view.is_focusable());
        assert_eq!(
            line_text(&plan_decision_action_line(
                PlanDecision::Accepted,
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
        let theme = Theme::dark();
        let lines = diff_display_lines("+added\n-removed\n@@ hunk", Style::default(), None, &theme);

        assert_eq!(lines[0].style.fg, Some(Color::Green));
        assert_eq!(lines[1].style.fg, Some(Color::Red));
        assert_eq!(lines[2].style.fg, Some(Color::Yellow));
    }

    #[test]
    fn diff_display_lines_highlights_payload_from_explicit_path() {
        let theme = Theme::dark();
        let lines = diff_display_lines(
            " fn main() {",
            Style::default(),
            Some("src/main.rs"),
            &theme,
        );
        let keyword = lines[0]
            .spans
            .iter()
            .find(|span| span.content.as_ref() == "fn")
            .expect("rust keyword span");

        assert_eq!(line_text(&lines[0]), " fn main() {");
        assert_eq!(lines[0].spans[0].content.as_ref(), " ");
        assert_eq!(keyword.style.fg, Some(Color::LightMagenta));
    }

    #[test]
    fn diff_display_lines_preserves_addition_semantics_over_syntax() {
        let theme = Theme::dark();
        let lines = diff_display_lines(
            "+fn main() {}",
            Style::default(),
            Some("src/main.rs"),
            &theme,
        );
        let keyword = lines[0]
            .spans
            .iter()
            .find(|span| span.content.as_ref() == "fn")
            .expect("rust keyword span");

        assert_eq!(lines[0].style.fg, Some(Color::Green));
        assert_eq!(keyword.style.fg, Some(Color::Green));
    }

    #[test]
    fn diff_display_lines_infers_payload_language_from_headers() {
        let theme = Theme::dark();
        let diff = "diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n fn main() {}";
        let lines = diff_display_lines(diff, Style::default(), None, &theme);
        let context = lines
            .iter()
            .find(|line| line_text(line) == " fn main() {}")
            .expect("context payload line");

        assert!(context.spans.iter().any(|span| {
            span.content.as_ref() == "fn" && span.style.fg == Some(Color::LightMagenta)
        }));
    }

    #[test]
    fn diff_display_lines_keeps_dash_payloads_in_hunks_as_removals() {
        let theme = Theme::dark();
        let lines = diff_display_lines("@@ -1 +1 @@\n--- payload", Style::default(), None, &theme);

        assert_eq!(lines[1].style.fg, Some(Color::Red));
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
        let mut view = DiffView::new(None, Binding::new("+0123456789".to_string()), None);

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
    fn row_keys_ignore_plan_items_but_track_plan_decision() {
        let id = ChatMessageId::new(17);
        let mut message = ChatMessage::new(
            id,
            ChatRole::Assistant,
            vec![ChatBlock::Plan(PlanBlock {
                id: ChatBlockId::new(17_001),
                items: vec![PlanItem {
                    text: "draft".to_string(),
                }],
                decision: PlanDecision::Pending,
            })],
        );
        let first_key = row_keys_from_messages(&[message.clone()]);

        if let ChatBlock::Plan(plan) = &mut message.blocks[0] {
            plan.items = vec![
                PlanItem {
                    text: "draft".to_string(),
                },
                PlanItem {
                    text: "verify".to_string(),
                },
            ];
        }
        let items_key = row_keys_from_messages(&[message.clone()]);
        assert_eq!(first_key, items_key);

        if let ChatBlock::Plan(plan) = &mut message.blocks[0] {
            plan.decision = PlanDecision::Accepted;
        }
        let decision_key = row_keys_from_messages(&[message]);

        assert_ne!(items_key, decision_key);
    }

    #[test]
    fn row_keys_ignore_task_runtime_details_but_track_identity() {
        let id = ChatMessageId::new(18);
        let mut message = ChatMessage::new(
            id,
            ChatRole::Assistant,
            vec![ChatBlock::Task(TaskBlock {
                id: ChatBlockId::new(18_001),
                title: "subagent".to_string(),
                status: TaskStatus::Running,
                summary: "initial".to_string(),
                transcript: Vec::new(),
                collapsed: true,
            })],
        );
        let first_key = row_keys_from_messages(&[message.clone()]);

        if let ChatBlock::Task(task) = &mut message.blocks[0] {
            task.status = TaskStatus::Complete;
            task.summary = "done".to_string();
            task.transcript = vec![TaskTranscriptItem {
                role: ChatRole::Assistant,
                blocks: vec![ChatBlock::Text(TextBlock {
                    id: ChatBlockId::new(18_101),
                    markdown: "nested".to_string(),
                    streaming: false,
                })],
            }];
        }
        let updated_key = row_keys_from_messages(&[message.clone()]);
        assert_eq!(first_key, updated_key);

        if let ChatBlock::Task(task) = &mut message.blocks[0] {
            task.title = "renamed subagent".to_string();
        }
        let renamed_key = row_keys_from_messages(&[message]);

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
            ChatRowKey::Header { message_id, .. } if *message_id == id
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
            .any(|key| matches!(key, ChatRowKey::Header { message_id, .. } if *message_id == result_message_id)));
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
