use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use atto_ui::reactive::{Binding, Property};

use crate::message::{
    ChatMessage, ChatMessageContent, ChatMessageId, ChatMessageStatus, ChatToolCallStatus,
};

#[derive(Clone, Debug)]
pub struct ChatMessageStore {
    messages: Property<Vec<ChatMessage>>,
    next_id: Arc<AtomicU64>,
}

impl ChatMessageStore {
    pub fn new() -> Self {
        Self {
            messages: Property::new(Vec::new()),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn binding(&self) -> Binding<Vec<ChatMessage>> {
        self.messages.binding()
    }

    pub fn messages(&self) -> Vec<ChatMessage> {
        self.messages.get()
    }

    pub fn replace_all(&self, messages: Vec<ChatMessage>) {
        self.messages.set(messages);
    }

    pub fn next_message_id(&self) -> ChatMessageId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        ChatMessageId::new(id)
    }

    pub fn push(&self, message: ChatMessage) {
        self.messages.update(|items| items.push(message));
    }

    pub fn prepend(&self, message: ChatMessage) {
        self.messages.update(|items| items.insert(0, message));
    }

    pub fn prepend_many(&self, mut messages: Vec<ChatMessage>) {
        if messages.is_empty() {
            return;
        }
        self.messages.update(|items| {
            messages.append(items);
            *items = messages;
        });
    }

    pub fn update_message<F>(&self, id: ChatMessageId, f: F) -> bool
    where
        F: FnOnce(&mut ChatMessage),
    {
        self.messages.update_if(|items| {
            if let Some(item) = items.iter_mut().find(|item| item.id == id) {
                f(item);
                true
            } else {
                false
            }
        })
    }

    pub fn set_status(&self, id: ChatMessageId, status: ChatMessageStatus) -> bool {
        let mut found = false;
        self.messages.update_if(|items| {
            let Some(item) = items.iter_mut().find(|item| item.id == id) else {
                return false;
            };
            found = true;
            if item.status == status {
                false
            } else {
                item.status = status;
                true
            }
        });
        found
    }

    pub fn update_text(&self, id: ChatMessageId, markdown: impl Into<String>) -> bool {
        let markdown = markdown.into();
        let mut found_text = false;
        self.messages.update_if(|items| {
            let Some(item) = items.iter_mut().find(|item| item.id == id) else {
                return false;
            };
            let ChatMessageContent::Text { markdown: text } = &mut item.content else {
                return false;
            };
            found_text = true;
            if *text == markdown {
                false
            } else {
                *text = markdown;
                true
            }
        });
        found_text
    }

    pub fn append_delta(&self, id: ChatMessageId, delta: &str) -> bool {
        let mut found_text = false;
        self.messages.update_if(|items| {
            let Some(item) = items.iter_mut().find(|item| item.id == id) else {
                return false;
            };
            let ChatMessageContent::Text { markdown } = &mut item.content else {
                return false;
            };
            found_text = true;
            if delta.is_empty() {
                return false;
            }
            markdown.push_str(delta);
            true
        });
        found_text
    }

    pub fn update_tool_output(&self, id: ChatMessageId, output: impl Into<String>) -> bool {
        let output = output.into();
        let mut found_tool = false;
        self.messages.update_if(|items| {
            let Some(item) = items.iter_mut().find(|item| item.id == id) else {
                return false;
            };
            let ChatMessageContent::ToolCall {
                output: current, ..
            } = &mut item.content
            else {
                return false;
            };
            found_tool = true;
            if *current == output {
                false
            } else {
                *current = output;
                true
            }
        });
        found_tool
    }

    pub fn append_tool_delta(&self, id: ChatMessageId, delta: &str) -> bool {
        let mut found_tool = false;
        self.messages.update_if(|items| {
            let Some(item) = items.iter_mut().find(|item| item.id == id) else {
                return false;
            };
            let ChatMessageContent::ToolCall { output, .. } = &mut item.content else {
                return false;
            };
            found_tool = true;
            if delta.is_empty() {
                return false;
            }
            output.push_str(delta);
            true
        });
        found_tool
    }

    pub fn set_tool_status(&self, id: ChatMessageId, status: ChatToolCallStatus) -> bool {
        let mut found_tool = false;
        self.messages.update_if(|items| {
            let Some(item) = items.iter_mut().find(|item| item.id == id) else {
                return false;
            };
            let ChatMessageContent::ToolCall {
                status: current, ..
            } = &mut item.content
            else {
                return false;
            };
            found_tool = true;
            if *current == status {
                false
            } else {
                *current = status;
                true
            }
        });
        found_tool
    }
}

impl Default for ChatMessageStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::ChatSender;

    fn text_for(store: &ChatMessageStore, id: ChatMessageId) -> String {
        store
            .messages()
            .into_iter()
            .find(|message| message.id == id)
            .and_then(|message| match message.content {
                ChatMessageContent::Text { markdown } => Some(markdown),
                ChatMessageContent::File { .. } => None,
                ChatMessageContent::ToolCall { .. } => None,
            })
            .expect("text message should exist")
    }

    fn tool_output_for(store: &ChatMessageStore, id: ChatMessageId) -> String {
        store
            .messages()
            .into_iter()
            .find(|message| message.id == id)
            .and_then(|message| match message.content {
                ChatMessageContent::ToolCall { output, .. } => Some(output),
                ChatMessageContent::Text { .. } | ChatMessageContent::File { .. } => None,
            })
            .expect("tool message should exist")
    }

    fn tool_status_for(store: &ChatMessageStore, id: ChatMessageId) -> ChatToolCallStatus {
        store
            .messages()
            .into_iter()
            .find(|message| message.id == id)
            .and_then(|message| match message.content {
                ChatMessageContent::ToolCall { status, .. } => Some(status),
                ChatMessageContent::Text { .. } | ChatMessageContent::File { .. } => None,
            })
            .expect("tool message should exist")
    }

    fn status_for(store: &ChatMessageStore, id: ChatMessageId) -> ChatMessageStatus {
        store
            .messages()
            .into_iter()
            .find(|message| message.id == id)
            .map(|message| message.status)
            .expect("message should exist")
    }

    #[test]
    fn append_delta_accumulates_text_and_preserves_streaming_status() {
        let store = ChatMessageStore::new();
        let id = store.next_message_id();
        store.push(
            ChatMessage::text(id, ChatSender::Assistant, "")
                .with_status(ChatMessageStatus::InProgress),
        );

        assert!(store.append_delta(id, "hel"));
        assert!(store.append_delta(id, "lo"));

        assert_eq!(text_for(&store, id), "hello");
        assert_eq!(status_for(&store, id), ChatMessageStatus::InProgress);

        assert!(store.set_status(id, ChatMessageStatus::Final));
        assert_eq!(status_for(&store, id), ChatMessageStatus::Final);
    }

    #[test]
    fn append_delta_is_noop_for_non_text_content() {
        let store = ChatMessageStore::new();
        let id = store.next_message_id();
        store.push(ChatMessage::file(
            id,
            ChatSender::Assistant,
            "report.txt",
            None,
        ));
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        assert!(!store.append_delta(id, "ignored"));

        assert!(!binding.check_dirty(&mut observer));
        assert!(matches!(
            store.messages()[0].content,
            ChatMessageContent::File { .. }
        ));
    }

    #[test]
    fn append_delta_empty_delta_does_not_notify() {
        let store = ChatMessageStore::new();
        let id = store.next_message_id();
        store.push(ChatMessage::text(id, ChatSender::Assistant, "seed"));
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        assert!(store.append_delta(id, ""));

        assert_eq!(text_for(&store, id), "seed");
        assert!(!binding.check_dirty(&mut observer));
    }

    #[test]
    fn update_text_same_text_does_not_notify() {
        let store = ChatMessageStore::new();
        let id = store.next_message_id();
        store.push(ChatMessage::text(id, ChatSender::Assistant, "seed"));
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        assert!(store.update_text(id, "seed"));

        assert_eq!(text_for(&store, id), "seed");
        assert!(!binding.check_dirty(&mut observer));
    }

    #[test]
    fn set_status_same_status_does_not_notify() {
        let store = ChatMessageStore::new();
        let id = store.next_message_id();
        store.push(ChatMessage::text(id, ChatSender::Assistant, "seed"));
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        assert!(store.set_status(id, ChatMessageStatus::Final));

        assert_eq!(status_for(&store, id), ChatMessageStatus::Final);
        assert!(!binding.check_dirty(&mut observer));
    }

    #[test]
    fn append_delta_handles_long_token_stream() {
        let store = ChatMessageStore::new();
        let id = store.next_message_id();
        store.push(
            ChatMessage::text(id, ChatSender::Assistant, "")
                .with_status(ChatMessageStatus::InProgress),
        );

        for _ in 0..5_500 {
            assert!(store.append_delta(id, "x"));
        }

        let text = text_for(&store, id);
        assert_eq!(text.len(), 5_500);
        assert!(text.bytes().all(|byte| byte == b'x'));
    }

    #[test]
    fn tool_delta_accumulates_output_and_status_updates() {
        let store = ChatMessageStore::new();
        let id = store.next_message_id();
        store.push(ChatMessage::tool_call(
            id,
            "build",
            ChatToolCallStatus::Running,
            "start",
        ));

        assert!(store.append_tool_delta(id, " -> compile"));
        assert_eq!(tool_output_for(&store, id), "start -> compile");
        assert_eq!(tool_status_for(&store, id), ChatToolCallStatus::Running);

        assert!(store.set_tool_status(id, ChatToolCallStatus::Done));
        assert_eq!(tool_status_for(&store, id), ChatToolCallStatus::Done);
    }

    #[test]
    fn tool_updates_noop_without_dirty_notification_when_unchanged() {
        let store = ChatMessageStore::new();
        let id = store.next_message_id();
        store.push(ChatMessage::tool_call(
            id,
            "build",
            ChatToolCallStatus::Running,
            "same",
        ));
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        assert!(store.append_tool_delta(id, ""));
        assert!(store.update_tool_output(id, "same"));
        assert!(store.set_tool_status(id, ChatToolCallStatus::Running));

        assert!(!binding.check_dirty(&mut observer));
    }
}
