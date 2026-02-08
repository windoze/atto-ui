use std::sync::Arc;

use atto_ui::composable::{
    Component, ComponentAction, ComponentContext, EdgeInsets, EventResult, HStack, LayoutParams,
    ScrollConfig, ScrollbarVisibility, Size, Spacer, Text, VStack,
};
use atto_ui::reactive::{Binding, DirtyObserver};
use atto_ui::widgets::{Spinner, SpinnerIconStyle};
use atto_ui_markdown::MarkdownViewer;
use crossterm::event::Event;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::message::{ChatAlignment, ChatMessage, ChatMessageContent, ChatMessageStatus};

const DEFAULT_WRAP_WIDTH: u16 = 72;
const DEFAULT_IN_PROGRESS_SUFFIX: &str = " ▍";

#[derive(Clone, Debug)]
struct ChatMessageListConfig {
    wrap_width: u16,
    in_progress_suffix: String,
    show_timestamps: bool,
    spacing: Binding<u16>,
    padding: Binding<EdgeInsets>,
    scroll_config: Binding<ScrollConfig>,
}

#[derive(Clone, Debug)]
struct ChatMessageRowConfig {
    wrap_width: u16,
    in_progress_suffix: String,
    show_timestamps: bool,
}

pub struct ChatMessageList {
    messages: Binding<Vec<ChatMessage>>,
    list: atto_ui::composable::ForEachIdentifiable<ChatMessage, ChatMessageRow>,
    config: ChatMessageListConfig,
    on_load_more: Option<Arc<dyn Fn() + Send + Sync>>,
    load_more_armed: bool,
    auto_scroll: bool,
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
        };
        let list = build_list(messages.clone(), &config);
        let messages_observer = messages.dirty_observer();
        Self {
            messages,
            list,
            config,
            on_load_more: None,
            load_more_armed: true,
            auto_scroll: true,
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

    pub fn auto_scroll(mut self, enabled: bool) -> Self {
        self.auto_scroll = enabled;
        self
    }

    fn rebuild_list(&mut self) {
        self.list = build_list(self.messages.clone(), &self.config);
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
        if self.suppress_auto_scroll_once {
            self.suppress_auto_scroll_once = false;
            return;
        }
        if self.auto_scroll {
            self.pending_scroll_to_bottom = true;
        }
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
        self.pending_scroll_to_bottom = false;
        self.load_more_armed = true;
    }
}

impl Component for ChatMessageList {
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
        Component::scroll_config(&self.list)
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.list.scroll_offset()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.list.set_scroll_offset(x, y);
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.track_message_changes();
        let mut res = self.list.handle_event(event, ctx);
        if self.maybe_trigger_load_more() && matches!(res.action, ComponentAction::None) {
            res = EventResult::changed();
        }
        res
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.track_message_changes();
        self.list.draw(frame, area, ctx);
        self.apply_pending_scroll();
    }
}

fn build_list(
    messages: Binding<Vec<ChatMessage>>,
    config: &ChatMessageListConfig,
) -> atto_ui::composable::ForEachIdentifiable<ChatMessage, ChatMessageRow> {
    let row_config = ChatMessageRowConfig {
        wrap_width: config.wrap_width,
        in_progress_suffix: config.in_progress_suffix.clone(),
        show_timestamps: config.show_timestamps,
    };
    let list = atto_ui::composable::ForEach::new(messages, move |message, _| {
        ChatMessageRow::new(message.clone(), row_config.clone())
    })
    .spacing(config.spacing.clone())
    .padding_insets(config.padding.clone())
    .scrollable(true)
    .scroll_config(config.scroll_config.clone());
    list.with_id()
}

struct ChatMessageRow {
    view: VStack,
}

impl ChatMessageRow {
    fn new(message: ChatMessage, config: ChatMessageRowConfig) -> Self {
        let mut column = VStack::new().with_spacing(1);
        let row_layout = LayoutParams {
            height: Size::Content,
            ..LayoutParams::default()
        };

        if config.show_timestamps {
            if let Some(ts) = &message.timestamp {
                column = column.child_with_layout(ChatTimestampDivider::new(ts.clone()), row_layout);
            }
        }

        let bubble = build_aligned_bubble(&message, &config);
        column = column.child_with_layout(bubble, row_layout);

        Self { view: column }
    }
}

impl Component for ChatMessageRow {
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

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        self.view.handle_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        self.view.draw(frame, area, ctx);
    }
}

fn build_aligned_bubble(message: &ChatMessage, config: &ChatMessageRowConfig) -> HStack {
    let bubble = build_bubble(message, config);
    let bubble_layout = LayoutParams {
        width: Size::Weight(3),
        height: Size::Content,
        ..LayoutParams::default()
    };
    let spacer_layout = LayoutParams {
        width: Size::Weight(1),
        ..LayoutParams::default()
    };

    match message.sender.alignment() {
        ChatAlignment::Left => HStack::new()
            .child_with_layout(bubble, bubble_layout)
            .child_with_layout(Spacer::new(), spacer_layout),
        ChatAlignment::Right => HStack::new()
            .child_with_layout(Spacer::new(), spacer_layout)
            .child_with_layout(bubble, bubble_layout),
    }
}

fn build_bubble(message: &ChatMessage, config: &ChatMessageRowConfig) -> VStack {
    let header = build_header(message);
    let body = ChatMessageBody::from_message(message, config);
    let content_layout = LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    };

    let mut bubble = VStack::new()
        .with_spacing(1)
        .child_with_layout(header, content_layout)
        .child_with_layout(body, content_layout);

    if matches!(message.status, ChatMessageStatus::InProgress) {
        let spinner = Spinner::new("Generating")
            .icon_style(SpinnerIconStyle::Dots)
            .spacing(1);
        bubble = bubble.child_with_layout(spinner, content_layout);
    }

    bubble
}

fn build_header(message: &ChatMessage) -> HStack {
    let mut header = HStack::new().with_spacing(1);
    header = header.child(Text::new(message.sender.label()));

    match &message.status {
        ChatMessageStatus::Failed(reason) => {
            header = header.child(Text::new(format!("(failed: {reason})")));
        }
        ChatMessageStatus::Final => {}
        ChatMessageStatus::InProgress => {}
    }

    header
}

struct ChatTimestampDivider {
    label: String,
}

impl ChatTimestampDivider {
    fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

impl Component for ChatTimestampDivider {
    fn desired_height(&self) -> Option<u16> {
        Some(1)
    }

    fn min_height(&self) -> u16 {
        1
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let width = area.width as usize;
        let label = format!(" {} ", self.label);
        let line = if label.len() >= width {
            label.chars().take(width).collect::<String>()
        } else {
            let padding = width.saturating_sub(label.len());
            let left = padding / 2;
            let right = padding.saturating_sub(left);
            format!(
                "{}{}{}",
                "─".repeat(left),
                label,
                "─".repeat(right)
            )
        };
        let style = ctx.theme.widget.dim;
        frame.render_widget(Paragraph::new(Line::styled(line, style)), area);
    }
}

enum ChatMessageBody {
    Markdown(MarkdownViewer),
    File(VStack),
}

impl ChatMessageBody {
    fn from_message(message: &ChatMessage, config: &ChatMessageRowConfig) -> Self {
        match &message.content {
            ChatMessageContent::Text { markdown } => {
                let mut content = markdown.clone();
                if matches!(message.status, ChatMessageStatus::InProgress)
                    && !config.in_progress_suffix.is_empty()
                {
                    content.push_str(&config.in_progress_suffix);
                }
                ChatMessageBody::Markdown(
                    MarkdownViewer::new(content)
                        .wrap_width(config.wrap_width)
                        .vertical_scrollbar(ScrollbarVisibility::Never),
                )
            }
            ChatMessageContent::File { name, url } => {
                let mut view = VStack::new().with_spacing(0);
                view = view.child(Text::new(format!("File: {name}")));
                if let Some(url) = url {
                    view = view.child(Text::new(format!("Url: {url}")));
                }
                ChatMessageBody::File(view)
            }
        }
    }
}

impl Component for ChatMessageBody {
    fn min_width(&self) -> u16 {
        match self {
            ChatMessageBody::Markdown(view) => view.min_width(),
            ChatMessageBody::File(view) => view.min_width(),
        }
    }

    fn min_height(&self) -> u16 {
        match self {
            ChatMessageBody::Markdown(view) => view.min_height(),
            ChatMessageBody::File(view) => view.min_height(),
        }
    }

    fn desired_width(&self) -> Option<u16> {
        match self {
            ChatMessageBody::Markdown(view) => view.desired_width(),
            ChatMessageBody::File(view) => view.desired_width(),
        }
    }

    fn desired_height(&self) -> Option<u16> {
        match self {
            ChatMessageBody::Markdown(view) => view.desired_height(),
            ChatMessageBody::File(view) => view.desired_height(),
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: ComponentContext<'_>) -> EventResult {
        match self {
            ChatMessageBody::Markdown(view) => view.handle_event(event, ctx),
            ChatMessageBody::File(view) => view.handle_event(event, ctx),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        match self {
            ChatMessageBody::Markdown(view) => view.draw(frame, area, ctx),
            ChatMessageBody::File(view) => view.draw(frame, area, ctx),
        }
    }
}
