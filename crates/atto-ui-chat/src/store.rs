use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use atto_ui::reactive::{Binding, Property};

use crate::message::{
    ApprovalAction, ApprovalOption, ChatBlock, ChatBlockId, ChatMessage, ChatMessageId,
    ChatMessageMeta, ChatTurnStatus, EditDecision, PlanDecision, PlanItem, TaskStatus,
    TaskTranscriptItem, TodoItem, ToolResultBlock, ToolStatus,
};

#[derive(Clone, Debug)]
pub struct ChatMessageStore {
    messages: Property<Vec<ChatMessage>>,
    next_id: Arc<AtomicU64>,
    next_block_id: Arc<AtomicU64>,
    branch_generation: Arc<AtomicU64>,
    versions: Arc<ChatMessageVersions>,
}

/// Opaque marker for the current chat branch.
///
/// Hosts can capture a token before starting a streaming generation and use
/// `push_if_branch_current` to avoid appending late messages after an edit,
/// retry, or fork has moved the transcript to a new branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChatBranchToken(u64);

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

    fn retain_registered(
        &self,
        message_ids: &HashSet<ChatMessageId>,
        block_ids: &HashSet<ChatBlockId>,
    ) {
        self.messages
            .write()
            .expect("message version lock poisoned")
            .retain(|id, _| message_ids.contains(id));
        self.blocks
            .write()
            .expect("block version lock poisoned")
            .retain(|id, _| block_ids.contains(id));
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
            branch_generation: Arc::new(AtomicU64::new(1)),
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
        self.bump_branch_generation();
        self.versions.replace_all(&messages);
        self.messages.set(messages);
    }

    /// Removes the target message and every later message, returning the removed suffix.
    pub fn truncate_from(&self, message_id: ChatMessageId) -> Option<Vec<ChatMessage>> {
        let mut removed = None;
        let mut retained_message_ids = HashSet::new();
        let mut retained_block_ids = HashSet::new();
        let changed = self.messages.update_if(|items| {
            let Some(index) = items.iter().position(|message| message.id == message_id) else {
                return false;
            };
            self.bump_branch_generation();
            removed = Some(items.split_off(index));
            (retained_message_ids, retained_block_ids) = registered_ids(items);
            true
        });
        if changed {
            self.versions
                .retain_registered(&retained_message_ids, &retained_block_ids);
        }
        removed
    }

    /// Keeps the target message as the fork point and removes every later message.
    pub fn fork_at(&self, message_id: ChatMessageId) -> Option<Vec<ChatMessage>> {
        let mut removed = None;
        let mut retained_message_ids = HashSet::new();
        let mut retained_block_ids = HashSet::new();
        let changed = self.messages.update_if(|items| {
            let Some(index) = items.iter().position(|message| message.id == message_id) else {
                return false;
            };
            let start = index.saturating_add(1);
            if start >= items.len() {
                removed = Some(Vec::new());
                return false;
            }
            self.bump_branch_generation();
            removed = Some(items.split_off(start));
            (retained_message_ids, retained_block_ids) = registered_ids(items);
            true
        });
        if changed {
            self.versions
                .retain_registered(&retained_message_ids, &retained_block_ids);
        }
        removed
    }

    pub fn next_message_id(&self) -> ChatMessageId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        ChatMessageId::new(id)
    }

    pub fn next_block_id(&self) -> ChatBlockId {
        let id = self.next_block_id.fetch_add(1, Ordering::Relaxed);
        ChatBlockId::new(id)
    }

    /// Returns a token representing the current transcript branch.
    pub fn branch_token(&self) -> ChatBranchToken {
        ChatBranchToken(self.branch_generation.load(Ordering::Acquire))
    }

    /// Returns whether a previously captured branch token is still current.
    pub fn is_branch_current(&self, token: ChatBranchToken) -> bool {
        self.branch_generation.load(Ordering::Acquire) == token.0
    }

    pub fn push(&self, message: ChatMessage) {
        self.bump_next_ids(std::slice::from_ref(&message));
        self.versions
            .register_messages(std::slice::from_ref(&message));
        self.messages.update(|items| items.push(message));
    }

    /// Appends a message only if no edit/retry/fork has changed branches since
    /// the caller captured `token`.
    pub fn push_if_branch_current(&self, token: ChatBranchToken, message: ChatMessage) -> bool {
        let registered = message.clone();
        let changed = self.messages.update_if(|items| {
            if self.branch_generation.load(Ordering::Acquire) != token.0 {
                return false;
            }
            items.push(message);
            true
        });
        if changed {
            self.bump_next_ids(std::slice::from_ref(&registered));
            self.versions
                .register_messages(std::slice::from_ref(&registered));
            if self.with_message(registered.id, |_| ()).is_none() {
                let (message_ids, block_ids) = self.messages.with(|items| registered_ids(items));
                self.versions.retain_registered(&message_ids, &block_ids);
            }
        }
        changed
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
            let Some(markdown) = find_block_mut(items, id).and_then(|block| match block {
                ChatBlock::Text(text_block) => Some(&mut text_block.markdown),
                ChatBlock::Thinking(thinking_block) => Some(&mut thinking_block.markdown),
                _ => None,
            }) else {
                return false;
            };
            found_text = true;
            if delta.is_empty() {
                return false;
            }
            markdown.push_str(delta);
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
            let Some(option) = approval
                .options
                .iter()
                .find(|option| option.id == option_id)
                .cloned()
            else {
                return false;
            };
            let next_status = approval_option_status(&option);
            let resolution = option.resolution();
            found_approval = true;
            let mut changed = false;
            if approval.resolved.as_ref() != Some(&resolution) {
                approval.resolved = Some(resolution);
                changed = true;
            }

            if tool_status_can_advance(tool.status, next_status) {
                tool.status = next_status;
                changed = true;
            }

            changed
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

    pub fn set_plan(&self, id: ChatBlockId, items: Vec<PlanItem>) -> bool {
        let mut found_plan = false;
        let changed = self.messages.update_if(|messages| {
            let Some(ChatBlock::Plan(plan)) = find_block_mut(messages, id) else {
                return false;
            };
            found_plan = true;
            if plan.items == items {
                false
            } else {
                plan.items = items;
                true
            }
        });
        if changed {
            self.versions.bump_block(id);
        }
        found_plan
    }

    pub fn set_plan_decision(&self, id: ChatBlockId, decision: PlanDecision) -> bool {
        let mut found_plan = false;
        let changed = self.messages.update_if(|messages| {
            let Some(ChatBlock::Plan(plan)) = find_block_mut(messages, id) else {
                return false;
            };
            found_plan = true;
            if plan.decision == decision {
                false
            } else {
                plan.decision = decision;
                true
            }
        });
        if changed {
            self.versions.bump_block(id);
        }
        found_plan
    }

    pub fn set_task_status(&self, id: ChatBlockId, status: TaskStatus) -> bool {
        let mut found_task = false;
        let changed = self.messages.update_if(|messages| {
            let Some(ChatBlock::Task(task)) = find_block_mut(messages, id) else {
                return false;
            };
            found_task = true;
            if task.status == status {
                false
            } else {
                task.status = status;
                true
            }
        });
        if changed {
            self.versions.bump_block(id);
        }
        found_task
    }

    pub fn set_task_summary(&self, id: ChatBlockId, summary: impl Into<String>) -> bool {
        let summary = summary.into();
        let mut found_task = false;
        let changed = self.messages.update_if(|messages| {
            let Some(ChatBlock::Task(task)) = find_block_mut(messages, id) else {
                return false;
            };
            found_task = true;
            if task.summary == summary {
                false
            } else {
                task.summary = summary;
                true
            }
        });
        if changed {
            self.versions.bump_block(id);
        }
        found_task
    }

    pub fn set_task_transcript(
        &self,
        id: ChatBlockId,
        transcript: Vec<TaskTranscriptItem>,
    ) -> bool {
        let mut found_task = false;
        let changed = self.messages.update_if(|messages| {
            let Some(ChatBlock::Task(task)) = find_block_mut(messages, id) else {
                return false;
            };
            found_task = true;
            if task.transcript == transcript {
                false
            } else {
                task.transcript = transcript;
                true
            }
        });
        if changed {
            self.versions.bump_block(id);
        }
        found_task
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
            .map(|block| max_nested_block_id(block).saturating_add(1))
            .max()
            .unwrap_or(1);
        bump_counter(&self.next_id, next_message_id);
        bump_counter(&self.next_block_id, next_block_id);
    }

    fn bump_branch_generation(&self) {
        self.branch_generation.fetch_add(1, Ordering::AcqRel);
    }
}

fn max_nested_block_id(block: &ChatBlock) -> u64 {
    let mut max_id = block.id().0;
    if let ChatBlock::Task(task) = block {
        for nested in task.transcript.iter().flat_map(|item| item.blocks.iter()) {
            max_id = max_id.max(max_nested_block_id(nested));
        }
    }
    max_id
}

fn registered_ids(messages: &[ChatMessage]) -> (HashSet<ChatMessageId>, HashSet<ChatBlockId>) {
    let message_ids = messages
        .iter()
        .map(|message| message.id)
        .collect::<HashSet<_>>();
    let block_ids = messages
        .iter()
        .flat_map(|message| message.blocks.iter().map(ChatBlock::id))
        .collect::<HashSet<_>>();
    (message_ids, block_ids)
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
        ChatBlock::Plan(block) => block.id = id,
        ChatBlock::Task(block) => block.id = id,
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

fn approval_option_status(option: &ApprovalOption) -> ToolStatus {
    match option.action {
        ApprovalAction::Allow => ToolStatus::Running,
        ApprovalAction::Deny => ToolStatus::Canceled,
    }
}

fn tool_status_can_advance(current: ToolStatus, next: ToolStatus) -> bool {
    match next {
        ToolStatus::Running => current == ToolStatus::Pending,
        ToolStatus::Canceled => matches!(current, ToolStatus::Pending | ToolStatus::Running),
        ToolStatus::Pending | ToolStatus::Done | ToolStatus::Error => false,
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
    use crate::message::{
        ApprovalAction, ApprovalLevel, ApprovalOption, ApprovalRequest, ApprovalResolution,
        ChatRole, DiffBlock, DiffData, PlanBlock, TaskBlock, TaskStatus, TaskTranscriptItem,
        TextBlock, ThinkingBlock, TodoBlock, TodoState, ToolInput, ToolOutput, ToolUseBlock,
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

    fn thinking_for(store: &ChatMessageStore, id: ChatBlockId) -> String {
        store
            .with_block(id, |block| match block {
                ChatBlock::Thinking(thinking) => thinking.markdown.clone(),
                _ => panic!("expected thinking block"),
            })
            .expect("thinking block should exist")
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

    fn approval_resolution_for(
        store: &ChatMessageStore,
        id: ChatBlockId,
    ) -> Option<ApprovalResolution> {
        store
            .with_block(id, |block| match block {
                ChatBlock::ToolUse(tool) => tool
                    .approval
                    .as_ref()
                    .and_then(|approval| approval.resolved.clone()),
                _ => panic!("expected tool use block"),
            })
            .expect("tool use block should exist")
    }

    fn message_ids(store: &ChatMessageStore) -> Vec<ChatMessageId> {
        store
            .messages()
            .into_iter()
            .map(|message| message.id)
            .collect()
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
    fn append_text_delta_targets_text_and_thinking_block_ids() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        store.push(
            ChatMessage::new(
                message_id,
                ChatRole::Assistant,
                vec![
                    ChatBlock::Text(TextBlock {
                        id: ChatBlockId::new(10),
                        markdown: String::new(),
                        streaming: false,
                    }),
                    ChatBlock::Thinking(ThinkingBlock {
                        id: ChatBlockId::new(11),
                        markdown: String::new(),
                        streaming: false,
                        collapsed: true,
                    }),
                ],
            )
            .with_status(ChatTurnStatus::Streaming),
        );
        let text_id = block_id_for(&store, message_id, |block| {
            matches!(block, ChatBlock::Text(_))
        });
        let thinking_id = block_id_for(&store, message_id, |block| {
            matches!(block, ChatBlock::Thinking(_))
        });

        assert!(store.append_text_delta(text_id, "hel"));
        assert!(store.append_text_delta(text_id, "lo"));
        assert!(store.append_text_delta(thinking_id, "rea"));
        assert!(store.append_text_delta(thinking_id, "soning"));

        assert_eq!(text_for(&store, text_id), "hello");
        assert_eq!(thinking_for(&store, thinking_id), "reasoning");
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
    fn truncate_from_first_message_removes_all_messages_and_versions() {
        let store = ChatMessageStore::new();
        let first_id = ChatMessageId::new(1);
        let second_id = ChatMessageId::new(2);
        let third_id = ChatMessageId::new(3);
        store.push(ChatMessage::text(first_id, ChatRole::User, "one"));
        store.push(ChatMessage::text(second_id, ChatRole::Assistant, "two"));
        store.push(ChatMessage::text(third_id, ChatRole::User, "three"));
        let first_block_id = block_id_for(&store, first_id, |block| {
            matches!(block, ChatBlock::Text(_))
        });
        let third_block_id = block_id_for(&store, third_id, |block| {
            matches!(block, ChatBlock::Text(_))
        });
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        let removed = store
            .truncate_from(first_id)
            .expect("first message should truncate");

        assert_eq!(message_ids(&store), Vec::<ChatMessageId>::new());
        assert_eq!(
            removed.iter().map(|message| message.id).collect::<Vec<_>>(),
            vec![first_id, second_id, third_id]
        );
        assert!(binding.check_dirty(&mut observer));
        assert_eq!(store.message_version(first_id), 0);
        assert_eq!(store.block_version(first_block_id), 0);
        assert_eq!(store.message_version(third_id), 0);
        assert_eq!(store.block_version(third_block_id), 0);
    }

    #[test]
    fn truncate_from_middle_preserves_prefix_versions_and_next_ids() {
        let store = ChatMessageStore::new();
        let ids = (0..4).map(|_| store.next_message_id()).collect::<Vec<_>>();
        for id in &ids {
            store.push(ChatMessage::text(
                *id,
                ChatRole::Assistant,
                format!("message {}", id.0),
            ));
        }
        let kept_block_id =
            block_id_for(&store, ids[1], |block| matches!(block, ChatBlock::Text(_)));
        let removed_block_id =
            block_id_for(&store, ids[2], |block| matches!(block, ChatBlock::Text(_)));
        let kept_message_version = store.message_version(ids[1]);
        let kept_block_version = store.block_version(kept_block_id);
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        let removed = store
            .truncate_from(ids[2])
            .expect("middle message should truncate");

        assert_eq!(message_ids(&store), vec![ids[0], ids[1]]);
        assert_eq!(
            removed.iter().map(|message| message.id).collect::<Vec<_>>(),
            vec![ids[2], ids[3]]
        );
        assert!(binding.check_dirty(&mut observer));
        assert_eq!(store.message_version(ids[1]), kept_message_version);
        assert_eq!(store.block_version(kept_block_id), kept_block_version);
        assert_eq!(store.message_version(ids[2]), 0);
        assert_eq!(store.block_version(removed_block_id), 0);
        assert_eq!(store.next_message_id(), ChatMessageId::new(5));
    }

    #[test]
    fn truncate_from_last_message_removes_only_tail_message() {
        let store = ChatMessageStore::new();
        let first_id = ChatMessageId::new(10);
        let second_id = ChatMessageId::new(11);
        let third_id = ChatMessageId::new(12);
        store.push(ChatMessage::text(first_id, ChatRole::User, "one"));
        store.push(ChatMessage::text(second_id, ChatRole::Assistant, "two"));
        store.push(ChatMessage::text(third_id, ChatRole::Assistant, "three"));
        let third_block_id = block_id_for(&store, third_id, |block| {
            matches!(block, ChatBlock::Text(_))
        });
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        let removed = store
            .truncate_from(third_id)
            .expect("last message should truncate");

        assert_eq!(message_ids(&store), vec![first_id, second_id]);
        assert_eq!(
            removed.iter().map(|message| message.id).collect::<Vec<_>>(),
            vec![third_id]
        );
        assert!(binding.check_dirty(&mut observer));
        assert_eq!(store.message_version(third_id), 0);
        assert_eq!(store.block_version(third_block_id), 0);
    }

    #[test]
    fn fork_at_middle_keeps_anchor_and_removes_later_messages() {
        let store = ChatMessageStore::new();
        let first_id = ChatMessageId::new(20);
        let anchor_id = ChatMessageId::new(21);
        let removed_id = ChatMessageId::new(22);
        store.push(ChatMessage::text(first_id, ChatRole::User, "one"));
        store.push(ChatMessage::text(anchor_id, ChatRole::Assistant, "two"));
        store.push(ChatMessage::text(removed_id, ChatRole::User, "three"));
        let anchor_block_id = block_id_for(&store, anchor_id, |block| {
            matches!(block, ChatBlock::Text(_))
        });
        let removed_block_id = block_id_for(&store, removed_id, |block| {
            matches!(block, ChatBlock::Text(_))
        });
        let anchor_message_version = store.message_version(anchor_id);
        let anchor_block_version = store.block_version(anchor_block_id);
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        let removed = store.fork_at(anchor_id).expect("anchor should fork");

        assert_eq!(message_ids(&store), vec![first_id, anchor_id]);
        assert_eq!(
            removed.iter().map(|message| message.id).collect::<Vec<_>>(),
            vec![removed_id]
        );
        assert!(binding.check_dirty(&mut observer));
        assert_eq!(store.message_version(anchor_id), anchor_message_version);
        assert_eq!(store.block_version(anchor_block_id), anchor_block_version);
        assert_eq!(store.message_version(removed_id), 0);
        assert_eq!(store.block_version(removed_block_id), 0);
    }

    #[test]
    fn fork_at_last_message_noops_without_dirty() {
        let store = ChatMessageStore::new();
        let first_id = ChatMessageId::new(30);
        let last_id = ChatMessageId::new(31);
        store.push(ChatMessage::text(first_id, ChatRole::User, "one"));
        store.push(ChatMessage::text(last_id, ChatRole::Assistant, "two"));
        let last_block_id =
            block_id_for(&store, last_id, |block| matches!(block, ChatBlock::Text(_)));
        let last_message_version = store.message_version(last_id);
        let last_block_version = store.block_version(last_block_id);
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        let removed = store
            .fork_at(last_id)
            .expect("last message should be a fork point");

        assert!(removed.is_empty());
        assert_eq!(message_ids(&store), vec![first_id, last_id]);
        assert!(!binding.check_dirty(&mut observer));
        assert_eq!(store.message_version(last_id), last_message_version);
        assert_eq!(store.block_version(last_block_id), last_block_version);
        assert!(store.fork_at(ChatMessageId::new(999)).is_none());
        assert!(!binding.check_dirty(&mut observer));
    }

    #[test]
    fn truncate_from_missing_message_noops_without_dirty_or_branch_change() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        store.push(ChatMessage::text(message_id, ChatRole::Assistant, "seed"));
        let token = store.branch_token();
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        let removed = store.truncate_from(ChatMessageId::new(999));

        assert!(removed.is_none());
        assert_eq!(message_ids(&store), vec![message_id]);
        assert!(!binding.check_dirty(&mut observer));
        assert!(store.is_branch_current(token));
    }

    #[test]
    fn branch_token_blocks_late_pushes_after_truncate_fork_or_replace() {
        let store = ChatMessageStore::new();
        let first_id = store.next_message_id();
        let second_id = store.next_message_id();
        store.push(ChatMessage::text(first_id, ChatRole::User, "prompt"));
        store.push(ChatMessage::text(second_id, ChatRole::Assistant, "old"));
        let initial = store.branch_token();

        assert!(store.truncate_from(second_id).is_some());
        assert!(!store.is_branch_current(initial));
        assert!(!store.push_if_branch_current(
            initial,
            ChatMessage::text(store.next_message_id(), ChatRole::Assistant, "stale")
        ));
        assert_eq!(message_ids(&store), vec![first_id]);

        let fork_token = store.branch_token();
        let third_id = store.next_message_id();
        store.push(ChatMessage::text(third_id, ChatRole::Assistant, "branch"));
        assert!(store.fork_at(first_id).is_some());
        assert!(!store.is_branch_current(fork_token));
        assert!(!store.push_if_branch_current(
            fork_token,
            ChatMessage::text(store.next_message_id(), ChatRole::Assistant, "late fork")
        ));
        assert_eq!(message_ids(&store), vec![first_id]);

        let replace_token = store.branch_token();
        store.replace_all(vec![ChatMessage::text(
            ChatMessageId::new(100),
            ChatRole::System,
            "replacement",
        )]);
        assert!(!store.is_branch_current(replace_token));
        assert!(!store.push_if_branch_current(
            replace_token,
            ChatMessage::text(store.next_message_id(), ChatRole::Assistant, "late replace")
        ));
        assert_eq!(message_ids(&store), vec![ChatMessageId::new(100)]);
    }

    #[test]
    fn current_branch_token_allows_pushes_before_branch_changes() {
        let store = ChatMessageStore::new();
        let token = store.branch_token();
        let message_id = store.next_message_id();

        assert!(store.push_if_branch_current(
            token,
            ChatMessage::text(message_id, ChatRole::Assistant, "fresh")
        ));

        assert_eq!(message_ids(&store), vec![message_id]);
        assert!(store.is_branch_current(token));
    }

    #[test]
    fn truncate_from_streaming_turn_removes_streaming_blocks_and_versions() {
        let store = ChatMessageStore::new();
        let user_id = ChatMessageId::new(40);
        let streaming_id = ChatMessageId::new(41);
        let later_id = ChatMessageId::new(42);
        let text_id = ChatBlockId::new(410);
        let thinking_id = ChatBlockId::new(411);
        store.push(ChatMessage::text(user_id, ChatRole::User, "prompt"));
        store.push(
            ChatMessage::new(
                streaming_id,
                ChatRole::Assistant,
                vec![
                    ChatBlock::Text(TextBlock {
                        id: text_id,
                        markdown: "partial".to_string(),
                        streaming: false,
                    }),
                    ChatBlock::Thinking(ThinkingBlock {
                        id: thinking_id,
                        markdown: "reasoning".to_string(),
                        streaming: false,
                        collapsed: true,
                    }),
                ],
            )
            .with_status(ChatTurnStatus::Streaming),
        );
        store.push(ChatMessage::text(later_id, ChatRole::Assistant, "stale"));
        let branch_token = store.branch_token();
        let binding = store.binding();
        let mut observer = binding.dirty_observer();

        let removed = store
            .truncate_from(streaming_id)
            .expect("streaming turn should truncate");

        assert_eq!(message_ids(&store), vec![user_id]);
        assert_eq!(
            removed.iter().map(|message| message.id).collect::<Vec<_>>(),
            vec![streaming_id, later_id]
        );
        assert_eq!(removed[0].status, ChatTurnStatus::Streaming);
        assert!(matches!(&removed[0].blocks[0], ChatBlock::Text(block) if block.streaming));
        assert!(matches!(&removed[0].blocks[1], ChatBlock::Thinking(block) if block.streaming));
        assert!(binding.check_dirty(&mut observer));
        assert!(store.with_block(text_id, |_| ()).is_none());
        assert!(store.with_block(thinking_id, |_| ()).is_none());
        assert_eq!(store.message_version(streaming_id), 0);
        assert_eq!(store.block_version(text_id), 0);
        assert_eq!(store.block_version(thinking_id), 0);
        assert!(!store.append_text_delta(text_id, " late"));
        assert!(!store.set_turn_status(streaming_id, ChatTurnStatus::Complete));
        assert!(!store.push_if_branch_current(
            branch_token,
            ChatMessage::text(store.next_message_id(), ChatRole::Assistant, "late push")
        ));
        assert_eq!(message_ids(&store), vec![user_id]);
        assert!(!binding.check_dirty(&mut observer));
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
                        options: vec![
                            ApprovalOption::allow_once("allow", "Allow"),
                            ApprovalOption::deny("deny", "Deny"),
                        ],
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
        assert_eq!(tool_status_for(&store, approval_id), ToolStatus::Running);
        assert!(store.set_edit_decision(diff_id, EditDecision::Accepted));
        assert!(store.set_todo(todo_id, next_items.clone()));

        let binding = store.binding();
        let mut observer = binding.dirty_observer();
        assert!(store.resolve_approval(approval_id, "allow"));
        assert!(store.set_edit_decision(diff_id, EditDecision::Accepted));
        assert!(store.set_todo(todo_id, next_items));
        assert!(!binding.check_dirty(&mut observer));

        assert!(!store.resolve_approval(approval_id, "missing"));
        assert!(!binding.check_dirty(&mut observer));
    }

    #[test]
    fn plan_updates_skip_unchanged_values() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        let plan_id = ChatBlockId::new(30);
        let next_items = vec![PlanItem {
            text: "verify".to_string(),
        }];
        store.push(ChatMessage::new(
            message_id,
            ChatRole::Assistant,
            vec![ChatBlock::Plan(PlanBlock {
                id: plan_id,
                items: vec![PlanItem {
                    text: "design".to_string(),
                }],
                decision: PlanDecision::Pending,
            })],
        ));

        assert!(store.set_plan(plan_id, next_items.clone()));
        assert!(store.set_plan_decision(plan_id, PlanDecision::Accepted));

        let binding = store.binding();
        let mut observer = binding.dirty_observer();
        assert!(store.set_plan(plan_id, next_items));
        assert!(store.set_plan_decision(plan_id, PlanDecision::Accepted));
        assert!(!binding.check_dirty(&mut observer));

        assert!(!store.set_plan(ChatBlockId::new(999), Vec::new()));
        assert!(!binding.check_dirty(&mut observer));
    }

    #[test]
    fn task_updates_skip_unchanged_values() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        let task_id = ChatBlockId::new(40);
        let transcript = vec![TaskTranscriptItem {
            role: ChatRole::Assistant,
            blocks: vec![ChatBlock::Text(TextBlock {
                id: ChatBlockId::new(41),
                markdown: "TASK-NESTED".to_string(),
                streaming: false,
            })],
        }];
        store.push(ChatMessage::new(
            message_id,
            ChatRole::Assistant,
            vec![ChatBlock::Task(TaskBlock {
                id: task_id,
                title: "subagent".to_string(),
                status: TaskStatus::Running,
                summary: "searching".to_string(),
                transcript: Vec::new(),
                collapsed: false,
            })],
        ));

        assert!(store.set_task_status(task_id, TaskStatus::Complete));
        assert!(store.set_task_summary(task_id, "done"));
        assert!(store.set_task_transcript(task_id, transcript.clone()));

        let binding = store.binding();
        let mut observer = binding.dirty_observer();
        assert!(store.set_task_status(task_id, TaskStatus::Complete));
        assert!(store.set_task_summary(task_id, "done"));
        assert!(store.set_task_transcript(task_id, transcript));
        assert!(!binding.check_dirty(&mut observer));

        assert!(!store.set_task_status(ChatBlockId::new(999), TaskStatus::Failed));
        assert!(!binding.check_dirty(&mut observer));
    }

    #[test]
    fn resolve_approval_deny_option_cancels_pending_tool() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        let approval_id = ChatBlockId::new(20);
        store.push(ChatMessage::new(
            message_id,
            ChatRole::Assistant,
            vec![ChatBlock::ToolUse(ToolUseBlock {
                id: approval_id,
                call_id: "call-deny".to_string(),
                name: "bash".to_string(),
                input: ToolInput::Text("rm -rf build".to_string()),
                status: ToolStatus::Pending,
                approval: Some(ApprovalRequest {
                    id: "approval-deny".to_string(),
                    prompt: "Run command?".to_string(),
                    options: vec![ApprovalOption::deny("deny", "Deny")],
                    resolved: None,
                }),
                collapsed: false,
            })],
        ));

        assert!(store.resolve_approval(approval_id, "deny"));

        assert_eq!(tool_status_for(&store, approval_id), ToolStatus::Canceled);
    }

    #[test]
    fn resolve_approval_records_structured_action_and_level() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        let approval_id = ChatBlockId::new(21);
        store.push(ChatMessage::new(
            message_id,
            ChatRole::Assistant,
            vec![ChatBlock::ToolUse(ToolUseBlock {
                id: approval_id,
                call_id: "call-project".to_string(),
                name: "bash".to_string(),
                input: ToolInput::Text("cargo test".to_string()),
                status: ToolStatus::Pending,
                approval: Some(ApprovalRequest {
                    id: "approval-project".to_string(),
                    prompt: "Run project command?".to_string(),
                    options: vec![ApprovalOption::allow_project(
                        "allow_project",
                        "Allow for project",
                    )],
                    resolved: None,
                }),
                collapsed: false,
            })],
        ));

        assert!(store.resolve_approval(approval_id, "allow_project"));

        assert_eq!(tool_status_for(&store, approval_id), ToolStatus::Running);
        assert_eq!(
            approval_resolution_for(&store, approval_id),
            Some(ApprovalResolution {
                option_id: "allow_project".to_string(),
                action: ApprovalAction::Allow,
                level: ApprovalLevel::Project,
            })
        );
    }

    #[test]
    fn resolve_approval_uses_explicit_action_instead_of_label_heuristics() {
        let store = ChatMessageStore::new();
        let message_id = store.next_message_id();
        let approval_id = ChatBlockId::new(22);
        store.push(ChatMessage::new(
            message_id,
            ChatRole::Assistant,
            vec![ChatBlock::ToolUse(ToolUseBlock {
                id: approval_id,
                call_id: "call-structured".to_string(),
                name: "bash".to_string(),
                input: ToolInput::Text("cargo clippy".to_string()),
                status: ToolStatus::Pending,
                approval: Some(ApprovalRequest {
                    id: "approval-structured".to_string(),
                    prompt: "Run structured command?".to_string(),
                    options: vec![ApprovalOption::new(
                        "remember",
                        "Stop asking for this command",
                        ApprovalAction::Allow,
                        ApprovalLevel::Always,
                    )],
                    resolved: None,
                }),
                collapsed: false,
            })],
        ));

        assert!(store.resolve_approval(approval_id, "remember"));

        assert_eq!(tool_status_for(&store, approval_id), ToolStatus::Running);
        assert_eq!(
            approval_resolution_for(&store, approval_id),
            Some(ApprovalResolution {
                option_id: "remember".to_string(),
                action: ApprovalAction::Allow,
                level: ApprovalLevel::Always,
            })
        );
    }
}
