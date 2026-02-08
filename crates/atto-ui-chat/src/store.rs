use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use atto_ui::reactive::{Binding, Property};

use crate::message::{ChatMessage, ChatMessageId, ChatMessageStatus};

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
        let mut found = false;
        self.messages.update(|items| {
            if let Some(item) = items.iter_mut().find(|item| item.id == id) {
                f(item);
                found = true;
            }
        });
        found
    }

    pub fn set_status(&self, id: ChatMessageId, status: ChatMessageStatus) -> bool {
        self.update_message(id, |item| item.status = status)
    }

    pub fn update_text(&self, id: ChatMessageId, markdown: impl Into<String>) -> bool {
        let markdown = markdown.into();
        self.update_message(id, |item| {
            if let crate::message::ChatMessageContent::Text { markdown: text } = &mut item.content {
                *text = markdown.clone();
            }
        })
    }
}

impl Default for ChatMessageStore {
    fn default() -> Self {
        Self::new()
    }
}
