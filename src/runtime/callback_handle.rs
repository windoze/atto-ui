use std::fmt;

use super::{CallbackId, CallbackInvocation, CallbackRegistry, ComponentValue};

#[derive(Clone)]
pub struct CallbackHandle {
    registry: CallbackRegistry,
    callback_id: CallbackId,
    target_id: Option<String>,
    event: String,
}

impl CallbackHandle {
    pub fn new(
        registry: CallbackRegistry,
        callback_id: CallbackId,
        target_id: Option<String>,
        event: impl Into<String>,
    ) -> Self {
        Self {
            registry,
            callback_id,
            target_id,
            event: event.into(),
        }
    }

    pub fn emit(&self) {
        self.emit_with(None);
    }

    pub fn emit_with(&self, payload: Option<ComponentValue>) {
        self.registry.emit(CallbackInvocation {
            callback_id: self.callback_id,
            target_id: self.target_id.clone(),
            event: self.event.clone(),
            payload,
        });
    }
}

impl fmt::Debug for CallbackHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallbackHandle")
            .field("callback_id", &self.callback_id)
            .field("target_id", &self.target_id)
            .field("event", &self.event)
            .finish()
    }
}
