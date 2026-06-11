use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use atto_ui::reactive::{Binding, Property};

use crate::message::{
    ChatBlock, ChatBlockId, ChatMessage, ChatMessageId, ChatMessageMeta, ChatTurnStatus,
    EditDecision, TodoItem, ToolResultBlock, ToolStatus,
};

#[derive(Clone, Debug)]
pub struct ChatMessageStore {
    messages: Property<Vec<ChatMessage>>,
    next_id: Arc<AtomicU64>,
    next_block_id: Arc<AtomicU64>,
    versions: Arc<ChatMessageVersions>,
}

#[derive(Debug)]
struct ChatMessageVersions {
    next: AtomicU64,
    messages: RwLock<HashMap<ChatMessageId, u64>>,
    blocks: RwLock<HashMap<ChatBlockId, u64>>,
}

impl ChatMessageVersions {
    fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
            messages: RwLock::new(HashMap::new()),
            blocks: RwLock::new(HashMap::new()),
        }
    }

    fn message_version(&self, id: ChatMessageId) -> u64 {
        *self
            .messages
            .read()
            .expect("message version lock poisoned")
            .get(&id)
            .unwrap_or(&0)
    }

    fn block_version(&self, id: ChatBlockId) -> u64 {
        *self
            .blocks
            .read()
            .expect("block version lock poisoned")
            .get(&id)
            .unwrap_or(&0)
    }

    fn register_messages(&self, messages: &[ChatMessage]) {
        if messages.is_empty() {
            return;
        }
        let version = self.next_version();
        let mut message_versions = self
            .messages
            .write()
            .expect("message version lock poisoned");
        let mut block_versions = self.blocks.write().expect("block version lock poisoned");
        for message in messages {
            message_versions.insert(message.id, version);
            for block in &message.blocks {
                block_versions.insert(block.id(), version);
            }
        }
    }

    fn replace_all(&self, messages: &[ChatMessage]) {
        let version = self.next_version();
        let message_ids = messages
            .iter()
            .map(|message| message.id)
            .collect::<HashSet<_>>();
        let block_ids = messages
            .iter()
            .flat_map(|message| message.blocks.iter().map(ChatBlock::id))
            .collect::<HashSet<_>>();

        let mut message_versions = self
            .messages
            .write()
            .expect("message version lock poisoned");
        message_versions.retain(|id, _| message_ids.contains(id));
        for id in message_ids {
            message_versions.insert(id, version);
        }

        let mut block_versions = self.blocks.write().expect("block version lock poisoned");
        block_versions.retain(|id, _| block_ids.contains(id));
        for id in block_ids {
            block_versions.insert(id, version);
        }
    }

    fn bump_message(&self, id: ChatMessageId) {
        let version = self.next_version();
        self.messages
            .write()
            .expect("message version lock poisoned")
            .insert(id, version);
    }

    fn bump_block(&self, id: ChatBlockId) {
        let version = self.next_version();
        self.blocks
            .write()
            .expect("block version lock poisoned")
            .insert(id, version);
    }

    fn bump_message_and_blocks(&self, message_id: ChatMessageId, block_ids: &[ChatBlockId]) {
        let version = self.next_version();
        self.messages
            .write()
            .expect("message version lock poisoned")
            .insert(message_id, version);
        let mut block_versions = self.blocks.write().expect("block version lock poisoned");
        for id in block_ids {
            block_versions.insert(*id, version);
        }
    }

    fn next_version(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
}

impl ChatMessageStore {
    pub fn new() -> Self {
        Self {
            messages: Property::new(Vec::new()),
            next_id: Arc::new(AtomicU64::new(1)),
            next_block_id: Arc::new(AtomicU64::new(1)),
            versions: Arc::new(ChatMessageVersions::new()),
        }
    }

    pub fn binding(&self) -> Binding<Vec<ChatMessage>> {
        self.messages.binding()
    }

    pub fn messages(&self) -> Vec<ChatMessage> {
        self.messages.get()
    }

    pub fn replace_all(&self, messages: Vec<ChatMessage>) {
        self.bump_next_ids(&messages);
        if self.messages.with(|items| items == &messages) {
            return;
        }
        self.versions.replace_all(&messages);
        self.messages.set(messages);
    }

    pub fn next_message_id(&self) -> ChatMessageId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        ChatMessageId::new(id)
    }

    pub fn next_block_id(&self) -> ChatBlockId {
        let id = self.next_block_id.fetch_add(1, Ordering::Relaxed);
        ChatBlockId::new(id)
    }

    pub fn push(&self, message: ChatMessage) {
        self.bump_next_ids(std::slice::from_ref(&message));
        self.versions
            .register_messages(std::slice::from_ref(&message));
        self.messages.update(|items| items.push(message));
    }

    pub fn prepend(&self, message: ChatMessage) {
        self.bump_next_ids(std::slice::from_ref(&message));
        self.versions
            .register_messages(std::slice::from_ref(&message));
        self.messages.update(|items| items.insert(0, message));
    }

    pub fn prepend_many(&self, mut messages: Vec<ChatMessage>) {
        if messages.is_empty() {
            return;
        }
        self.bump_next_ids(&messages);
        self.versions.register_messages(&messages);
        self.messages.update(|items| {
            messages.append(items);
            *items = messages;
        });
    }

    pub fn update_message<F>(&self, id: ChatMessageId, f: F) -> bool
    where
        F: FnOnce(&mut ChatMessage),
    {
        let mut block_ids = Vec::new();
        let changed = self.messages.update_if(|items| {
            if let Some(item) = items.iter_mut().find(|item| item.id == id) {
                let before = item.clone();
                f(item);
                if *item == before {
                    false
                } else {
                    block_ids = item.blocks.iter().map(ChatBlock::id).collect();
                    true
                }
            } else {
                false
            }
        });
        if changed {
            self.versions.bump_message_and_blocks(id, &block_ids);
        }
        changed
    }

    pub fn append_block(
        &self,
        message_id: ChatMessageId,
        mut block: ChatBlock,
    ) -> Option<ChatBlockId> {
        let block_id = self.next_block_id();
        set_block_id(&mut block, block_id);
        let changed = self.messages.update_if(|items| {
            let Some(message) = items.iter_mut().find(|message| message.id == message_id) else {
                return false;
            };
            message.blocks.push(block);
            true
        });
        if changed {
            self.versions
                .bump_message_and_blocks(message_id, &[block_id]);
        }
        changed.then_some(block_id)
    }

    pub(crate) fn with_message<R>(
        &self,
        id: ChatMessageId,
        f: impl FnOnce(&ChatMessage) -> R,
    ) -> Option<R> {
        self.messages
            .with(|items| items.iter().find(|message| message.id == id).map(f))
    }

    pub fn with_block<R>(&self, id: ChatBlockId, f: impl FnOnce(&ChatBlock) -> R) -> Option<R> {
        self.messages.with(|items| {
            items
                .iter()
                .flat_map(|message| message.blocks.iter())
                .find(|block| block.id() == id)
                .map(f)
        })
    }

    pub fn set_turn_status(&self, id: ChatMessageId, status: ChatTurnStatus) -> bool {
        let mut found = false;
        let mut block_ids = Vec::new();
        let changed = self.messages.update_if(|items| {
            let Some(item) = items.iter_mut().find(|item| item.id == id) else {
                return false;
            };
            found = true;
            if item.status == status {
                false
            } else {
                item.set_turn_status(status);
                block_ids = item
                    .blocks
                    .iter()
                    .filter_map(|block| match block {
                        ChatBlock::Text(_) | ChatBlock::Thinking(_) => Some(block.id()),
                        _ => None,
                    })
                    .collect();
                true
            }
        });
        if changed {
            self.versions.bump_message_and_blocks(id, &block_ids);
        }
        found
    }

    pub fn set_meta(&self, id: ChatMessageId, meta: ChatMessageMeta) -> bool {
        let mut found = false;
        let changed = self.messages.update_if(|items| {
            let Some(item) = items.iter_mut().find(|item| item.id == id) else {
                return false;
            };
            found = true;
            if item.meta == meta {
                false
            } else {
                item.meta = meta;
                true
            }
        });
        if changed {
            self.versions.bump_message(id);
        }
        found
    }

    pub fn append_text_delta(&self, id: ChatBlockId, delta: &str) -> bool {
        let mut found_text = false;
        let changed = self.messages.update_if(|items| {
            let Some(ChatBlock::Text(text_block)) = find_block_mut(items, id) else {
                return false;
            };
            found_text = true;
            if delta.is_empty() {
                return false;
            }
            text_block.markdown.push_str(delta);
            true
        });
        if changed {
            self.versions.bump_block(id);
        }
        found_text
    }

    pub fn append_tool_output(&self, id: ChatBlockId, delta: &str) -> bool {
        let mut found_tool = false;
        let changed = self.messages.update_if(|items| {
            let Some(ChatBlock::ToolResult(result)) = find_block_mut(items, id) else {
                return false;
            };
            found_tool = true;
            if delta.is_empty() {
                return false;
            }
            result.output.append_delta(delta);
            true
        });
        if changed {
            self.versions.bump_block(id);
        }
        found_tool
    }

    pub fn set_tool_status(&self, id: ChatBlockId, status: ToolStatus) -> bool {
        let mut found_tool = false;
        let changed = self.messages.update_if(|items| {
            let Some(ChatBlock::ToolUse(tool)) = find_block_mut(items, id) else {
                return false;
            };
            found_tool = true;
            if tool.status == status {
                false
            } else {
                tool.status = status;
                true
            }
        });
        if changed {
            self.versions.bump_block(id);
        }
        found_tool
    }

    pub fn upsert_tool_result(
        &self,
        call_id: impl Into<String>,
        mut result: ToolResultBlock,
    ) -> Option<ChatBlockId> {
        let call_id = call_id.into();
        result.call_id = call_id.clone();
        let mut result_id = None;
        let mut inserted_message_id = None;
        let changed = self.messages.update_if(|items| {
            for message in items.iter_mut() {
                if let Some(block) = message.blocks.iter_mut().find(|block| {
                    matches!(block, ChatBlock::ToolResult(existing) if existing.call_id == call_id)
                }) {
                    let ChatBlock::ToolResult(existing) = block else {
                        unreachable!();
                    };
                    result_id = Some(existing.id);
                    result.id = existing.id;
                    if *existing == result {
                        return false;
                    }
                    *existing = result;
                    return true;
                }
            }

            let Some(message) = items.iter_mut().find(|message| {
                message.blocks.iter().any(
                    |block| matches!(block, ChatBlock::ToolUse(tool) if tool.call_id == call_id),
                )
            }) else {
                return false;
            };
            let block_id = self.next_block_id();
            result.id = block_id;
            result_id = Some(block_id);
            inserted_message_id = Some(message.id);
            message.blocks.push(ChatBlock::ToolResult(result));
            true
        });
        if changed && let Some(block_id) = result_id {
            if let Some(message_id) = inserted_message_id {
                self.versions
                    .bump_message_and_blocks(message_id, &[block_id]);
            } else {
                self.versions.bump_block(block_id);
            }
        }
        result_id
    }

    pub fn resolve_approval(&self, id: ChatBlockId, option_id: impl Into<String>) -> bool {
        let option_id = option_id.into();
        let mut found_approval = false;
        let changed = self.messages.update_if(|items| {
            let Some(ChatBlock::ToolUse(tool)) = find_block_mut(items, id) else {
                return false;
            };
            let Some(approval) = &mut tool.approval else {
                return false;
            };
            found_approval = true;
            if approval.resolved.as_deref() == Some(option_id.as_str()) {
                false
            } else {
                approval.resolved = Some(option_id);
                true
            }
        });
        if changed {
            self.versions.bump_block(id);
        }
        found_approval
    }

    pub fn set_edit_decision(&self, id: ChatBlockId, decision: EditDecision) -> bool {
        let mut found_diff = false;
        let changed = self.messages.update_if(|items| {
            let Some(ChatBlock::Diff(diff)) = find_block_mut(items, id) else {
                return false;
            };
            found_diff = true;
            if diff.decision == decision {
                false
            } else {
                diff.decision = decision;
                true
            }
        });
        if changed {
            self.versions.bump_block(id);
        }
        found_diff
    }

    pub fn set_todo(&self, id: ChatBlockId, items: Vec<TodoItem>) -> bool {
        let mut found_todo = false;
        let changed = self.messages.update_if(|messages| {
            let Some(ChatBlock::Todo(todo)) = find_block_mut(messages, id) else {
                return false;
            };
            found_todo = true;
            if todo.items == items {
                false
            } else {
                todo.items = items;
                true
            }
        });
        if changed {
            self.versions.bump_block(id);
        }
        found_todo
    }

    pub(crate) fn message_version(&self, id: ChatMessageId) -> u64 {
        self.versions.message_version(id)
    }

    pub(crate) fn block_version(&self, id: ChatBlockId) -> u64 {
        self.versions.block_version(id)
    }

    fn bump_next_ids(&self, messages: &[ChatMessage]) {
        let next_message_id = messages
            .iter()
            .map(|message| message.id.0.saturating_add(1))
            .max()
            .unwrap_or(1);
        let next_block_id = messages
            .iter()
            .flat_map(|message| message.blocks.iter())
            .map(|block| block.id().0.saturating_add(1))
            .max()
            .unwrap_or(1);
        bump_counter(&self.next_id, next_message_id);
        bump_counter(&self.next_block_id, next_block_id);
    }
}

fn find_block_mut(messages: &mut [ChatMessage], id: ChatBlockId) -> Option<&mut ChatBlock> {
    messages
        .iter_mut()
        .flat_map(|message| message.blocks.iter_mut())
        .find(|block| block.id() == id)
}

fn set_block_id(block: &mut ChatBlock, id: ChatBlockId) {
    match block {
        ChatBlock::Text(block) => block.id = id,
        ChatBlock::Thinking(block) => block.id = id,
        ChatBlock::ToolUse(block) => block.id = id,
        ChatBlock::ToolResult(block) => block.id = id,
        ChatBlock::Diff(block) => block.id = id,
        ChatBlock::Todo(block) => block.id = id,
        ChatBlock::Attachment(block) => block.id = id,
        ChatBlock::Notice(block) => block.id = id,
        ChatBlock::Artifact(block) => block.id = id,
    }
}

fn bump_counter(counter: &AtomicU64, next: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        (current < next).then_some(next)
    });
}

impl Default for ChatMessageStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{
        ApprovalOption, ApprovalRequest, ChatRole, DiffBlock, DiffData, TextBlock, TodoBlock,
        TodoState, ToolInput, ToolOutput, ToolUseBlock,
    };

    fn block_id_for(
        store: &ChatMessageStore,
        message_id: ChatMessageId,
        matches_kind: impl Fn(&ChatBlock) -> bool,
    ) -> ChatBlockId {
        store
            .messages()
            .into_iter()
            .find(|message| message.id == message_id)
            .and_then(|message| {
                message
                    .blocks
                    .into_iter()
                    .find(matches_kind)
                    .map(|block| block.id())
            })
            .expect("block should exist")
    }

    fn text_for(store: &ChatMessageStore, id: ChatBlockId) -> String {
        store
            .with_block(id, |block| match block {
                ChatBlock::Text(text) => text.markdown.clone(),
                _ => panic!("expected text block"),
            })
            .expect("text block should exist")
    }

    fn tool_status_for(store: &ChatMessageStore, id: ChatBlockId) -> ToolStatus {
        store
            .with_block(id, |block| match block {
                ChatBlock::ToolUse(tool) => tool.status,
                _ => panic!("expected tool use block"),
            })
            .expect("tool use block should exist")
    }

    fn tool_output_for(store: &ChatMessageStore, id: ChatBlockId) -> String {
        store
            .with_block(id, |block| match block {
                ChatBlock::ToolResult(result) => result.output.as_text().to_string(),
                _ => panic!("expected tool result block"),
            })
            .expect("tool result block should exist")
    }

    #[test]
    fn append_block_assigns_store_id_and_with_block_reads_it() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        store.push(ChatMessage::new(
            message_id,
            ChatRole::Assistant,
            Vec::new(),
        ));
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        let block_id = store
            .append_block(
                message_id,
                ChatBlock::Text(TextBlock {
                    id: ChatBlockId::new(999),
                    markdown: "hello".to_string(),
                    streaming: false,
                }),
            )
            .expect("message should exist");

        assert!(binding.check_dirty(&mut observer));
        assert_eq!(text_for(&store, block_id), "hello");
        assert!(store.with_block(ChatBlockId::new(999), |_| ()).is_none());
    }

    #[test]
    fn append_text_delta_targets_text_block_id() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        store.push(
            ChatMessage::text(message_id, ChatRole::Assistant, "")
                .with_status(ChatTurnStatus::Streaming),
        );
        let text_id = block_id_for(&store, message_id, |block| {
            matches!(block, ChatBlock::Text(_))
        });

        assert!(store.append_text_delta(text_id, "hel"));
        assert!(store.append_text_delta(text_id, "lo"));

        assert_eq!(text_for(&store, text_id), "hello");
        assert_eq!(store.messages()[0].status, ChatTurnStatus::Streaming);
    }

    #[test]
    fn versions_track_message_and_block_updates_independently() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        let first_id = ChatBlockId::new(101);
        let second_id = ChatBlockId::new(102);
        store.push(ChatMessage::new(
            message_id,
            ChatRole::Assistant,
            vec![
                ChatBlock::Text(TextBlock {
                    id: first_id,
                    markdown: "first".to_string(),
                    streaming: false,
                }),
                ChatBlock::Text(TextBlock {
                    id: second_id,
                    markdown: "second".to_string(),
                    streaming: false,
                }),
            ],
        ));
        let message_version = store.message_version(message_id);
        let first_version = store.block_version(first_id);
        let second_version = store.block_version(second_id);

        assert!(store.append_text_delta(first_id, "!"));
        assert_eq!(store.message_version(message_id), message_version);
        assert!(store.block_version(first_id) > first_version);
        assert_eq!(store.block_version(second_id), second_version);

        let first_version = store.block_version(first_id);
        let second_version = store.block_version(second_id);
        assert!(store.set_turn_status(message_id, ChatTurnStatus::Streaming));
        assert!(store.message_version(message_id) > message_version);
        assert!(store.block_version(first_id) > first_version);
        assert!(store.block_version(second_id) > second_version);
    }

    #[test]
    fn append_text_delta_noops_without_dirty_for_empty_or_non_text() {
        let store = ChatMessageStore::new();
        let text_message_id = store.next_message_id();
        let file_message_id = store.next_message_id();
        store.push(ChatMessage::text(
            text_message_id,
            ChatRole::Assistant,
            "seed",
        ));
        store.push(ChatMessage::file(
            file_message_id,
            ChatRole::Assistant,
            "report.txt",
            None,
        ));
        let text_id = block_id_for(&store, text_message_id, |block| {
            matches!(block, ChatBlock::Text(_))
        });
        let file_id = block_id_for(&store, file_message_id, |block| {
            matches!(block, ChatBlock::Attachment(_))
        });
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        assert!(store.append_text_delta(text_id, ""));
        assert!(!store.append_text_delta(file_id, "ignored"));

        assert_eq!(text_for(&store, text_id), "seed");
        assert!(!binding.check_dirty(&mut observer));
    }

    #[test]
    fn set_turn_status_and_meta_skip_unchanged_values() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        store.push(ChatMessage::text(message_id, ChatRole::Assistant, "seed"));
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        assert!(store.set_turn_status(message_id, ChatTurnStatus::Complete));
        assert!(store.set_meta(message_id, ChatMessageMeta::default()));
        assert!(!binding.check_dirty(&mut observer));

        assert!(store.set_turn_status(message_id, ChatTurnStatus::Streaming));
        assert!(binding.check_dirty(&mut observer));
        assert!(matches!(
            &store.messages()[0].blocks[0],
            ChatBlock::Text(text) if text.streaming
        ));

        let meta = ChatMessageMeta {
            model: Some("claude".to_string()),
            ..ChatMessageMeta::default()
        };
        assert!(store.set_meta(message_id, meta.clone()));
        assert!(binding.check_dirty(&mut observer));
        assert_eq!(store.messages()[0].meta, meta);
    }

    #[test]
    fn tool_result_upsert_output_and_status_are_block_scoped() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        store.push(ChatMessage::tool_call(
            message_id,
            "build",
            ToolStatus::Running,
            "",
        ));
        let tool_id = block_id_for(&store, message_id, |block| {
            matches!(block, ChatBlock::ToolUse(_))
        });
        let call_id = store
            .with_block(tool_id, |block| match block {
                ChatBlock::ToolUse(tool) => tool.call_id.clone(),
                _ => panic!("expected tool use block"),
            })
            .expect("tool use block should exist");

        let result_id = store
            .upsert_tool_result(
                call_id.clone(),
                ToolResultBlock {
                    id: ChatBlockId::new(0),
                    call_id: "ignored".to_string(),
                    ok: true,
                    exit_code: None,
                    output: ToolOutput::Ansi("start".to_string()),
                    collapsed: false,
                },
            )
            .expect("matching tool use should receive a result");

        assert_eq!(tool_output_for(&store, result_id), "start");
        assert!(store.append_tool_output(result_id, " -> done"));
        assert_eq!(tool_output_for(&store, result_id), "start -> done");
        assert!(store.set_tool_status(tool_id, ToolStatus::Done));
        assert_eq!(tool_status_for(&store, tool_id), ToolStatus::Done);

        let binding = store.binding();
        let mut observer = binding.dirty_observer();
        assert!(store.append_tool_output(result_id, ""));
        assert!(store.set_tool_status(tool_id, ToolStatus::Done));
        assert_eq!(
            store.upsert_tool_result(
                call_id,
                ToolResultBlock {
                    id: ChatBlockId::new(0),
                    call_id: "ignored".to_string(),
                    ok: true,
                    exit_code: None,
                    output: ToolOutput::Ansi("start -> done".to_string()),
                    collapsed: false,
                },
            ),
            Some(result_id)
        );
        assert!(!binding.check_dirty(&mut observer));
    }

    #[test]
    fn approval_diff_and_todo_updates_skip_unchanged_values() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        let approval_id = ChatBlockId::new(10);
        let diff_id = ChatBlockId::new(11);
        let todo_id = ChatBlockId::new(12);
        let next_items = vec![TodoItem {
            text: "ship".to_string(),
            state: TodoState::Done,
        }];
        store.push(ChatMessage::new(
            message_id,
            ChatRole::Assistant,
            vec![
                ChatBlock::ToolUse(ToolUseBlock {
                    id: approval_id,
                    call_id: "call-1".to_string(),
                    name: "bash".to_string(),
                    input: ToolInput::Text("cargo test".to_string()),
                    status: ToolStatus::Pending,
                    approval: Some(ApprovalRequest {
                        id: "approval-1".to_string(),
                        prompt: "Run command?".to_string(),
                        options: vec![ApprovalOption {
                            id: "allow".to_string(),
                            label: "Allow".to_string(),
                        }],
                        resolved: None,
                    }),
                    collapsed: false,
                }),
                ChatBlock::Diff(DiffBlock {
                    id: diff_id,
                    path: "src/lib.rs".to_string(),
                    diff: DiffData {
                        unified: "+line".to_string(),
                    },
                    decision: EditDecision::Pending,
                }),
                ChatBlock::Todo(TodoBlock {
                    id: todo_id,
                    items: vec![TodoItem {
                        text: "ship".to_string(),
                        state: TodoState::InProgress,
                    }],
                }),
            ],
        ));

        assert!(store.resolve_approval(approval_id, "allow"));
        assert!(store.set_edit_decision(diff_id, EditDecision::Accepted));
        assert!(store.set_todo(todo_id, next_items.clone()));

        let binding = store.binding();
        let mut observer = binding.dirty_observer();
        assert!(store.resolve_approval(approval_id, "allow"));
        assert!(store.set_edit_decision(diff_id, EditDecision::Accepted));
        assert!(store.set_todo(todo_id, next_items));
        assert!(!binding.check_dirty(&mut observer));
    }
}
