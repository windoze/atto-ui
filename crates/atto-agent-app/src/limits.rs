//! Per-turn guardrails that keep agent/tool loops bounded.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use atto_ui_chat::{ChatError, ChatErrorKind, ChatMessageId};

use crate::tool::DEFAULT_TOOL_TIMEOUT;

pub(crate) const DEFAULT_MAX_MODEL_REQUESTS: usize = 8;
pub(crate) const DEFAULT_MAX_TOOL_CALLS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AgentTurnLimits {
    pub(crate) max_model_requests: usize,
    pub(crate) max_tool_calls: usize,
    pub(crate) tool_timeout: Duration,
}

impl AgentTurnLimits {
    pub(crate) fn new(
        max_model_requests: usize,
        max_tool_calls: usize,
        tool_timeout: Duration,
    ) -> Self {
        Self {
            max_model_requests: max_model_requests.max(1),
            max_tool_calls: max_tool_calls.max(1),
            tool_timeout: if tool_timeout.is_zero() {
                Duration::from_millis(1)
            } else {
                tool_timeout
            },
        }
    }
}

impl Default for AgentTurnLimits {
    fn default() -> Self {
        Self::new(
            DEFAULT_MAX_MODEL_REQUESTS,
            DEFAULT_MAX_TOOL_CALLS,
            DEFAULT_TOOL_TIMEOUT,
        )
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TurnBudgetTracker {
    budgets: Arc<Mutex<HashMap<ChatMessageId, TurnBudget>>>,
}

impl TurnBudgetTracker {
    pub(crate) fn start_turn(&self, message_id: ChatMessageId, limits: AgentTurnLimits) {
        self.budgets
            .lock()
            .expect("turn budget lock poisoned")
            .insert(message_id, TurnBudget::new(limits));
    }

    pub(crate) fn consume_model_request(
        &self,
        message_id: ChatMessageId,
        limits: AgentTurnLimits,
    ) -> Result<(), ChatError> {
        self.with_budget(message_id, limits, TurnBudget::consume_model_request)
    }

    pub(crate) fn consume_tool_calls(
        &self,
        message_id: ChatMessageId,
        count: usize,
        limits: AgentTurnLimits,
    ) -> Result<(), ChatError> {
        self.with_budget(message_id, limits, |budget| {
            budget.consume_tool_calls(count)
        })
    }

    pub(crate) fn finish_turn(&self, message_id: ChatMessageId) {
        self.budgets
            .lock()
            .expect("turn budget lock poisoned")
            .remove(&message_id);
    }

    pub(crate) fn clear(&self) {
        self.budgets
            .lock()
            .expect("turn budget lock poisoned")
            .clear();
    }

    fn with_budget<T>(
        &self,
        message_id: ChatMessageId,
        limits: AgentTurnLimits,
        apply: impl FnOnce(&mut TurnBudget) -> Result<T, ChatError>,
    ) -> Result<T, ChatError> {
        let mut budgets = self.budgets.lock().expect("turn budget lock poisoned");
        let budget = budgets
            .entry(message_id)
            .or_insert_with(|| TurnBudget::new(limits));
        apply(budget)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TurnBudget {
    limits: AgentTurnLimits,
    model_requests: usize,
    tool_calls: usize,
}

impl TurnBudget {
    fn new(limits: AgentTurnLimits) -> Self {
        Self {
            limits,
            model_requests: 0,
            tool_calls: 0,
        }
    }

    fn consume_model_request(&mut self) -> Result<(), ChatError> {
        if self.model_requests >= self.limits.max_model_requests {
            return Err(ChatError::new(
                ChatErrorKind::Other,
                "Agent turn model request limit reached.",
            )
            .with_detail(format!(
                "This turn already used {} model request(s); the per-turn limit is {}.",
                self.model_requests, self.limits.max_model_requests
            )));
        }
        self.model_requests += 1;
        Ok(())
    }

    fn consume_tool_calls(&mut self, count: usize) -> Result<(), ChatError> {
        if count == 0 {
            return Ok(());
        }
        let requested = self.tool_calls.saturating_add(count);
        if requested > self.limits.max_tool_calls {
            return Err(
                ChatError::new(ChatErrorKind::Tool, "Agent turn tool call limit reached.")
                    .with_detail(format!(
                        "This turn requested {requested} tool call(s); the per-turn limit is {}.",
                        self.limits.max_tool_calls
                    )),
            );
        }
        self.tool_calls = requested;
        Ok(())
    }
}
