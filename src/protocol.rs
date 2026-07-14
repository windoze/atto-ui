//! Serializable IPC protocol shapes for the scripting control plane.
//!
//! The protocol mirrors the in-process `DesktopInspector` API without
//! embedding transport, socket, or event-loop concerns. M4 server code can
//! deserialize these values, execute the corresponding inspector call on the UI
//! thread, and serialize a matching response.

use serde::{Deserialize, Serialize};

use crate::runtime::Rect;
use crate::{
    ComponentCommand, ComponentError, ComponentTarget, ComponentValue, DesktopSnapshot,
    InvokeResult, WaitCondition, WaitResult,
};

/// Request id used to correlate JSON-RPC-like requests and responses.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProtocolId {
    Number(u64),
    String(String),
}

impl From<u64> for ProtocolId {
    fn from(value: u64) -> Self {
        Self::Number(value)
    }
}

impl From<String> for ProtocolId {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for ProtocolId {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

/// Top-level request envelope: `{ id, method, params }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProtocolRequest {
    pub id: ProtocolId,
    #[serde(flatten)]
    pub method: ProtocolMethod,
}

impl ProtocolRequest {
    /// Build a `query` request for `DesktopInspector::query`.
    pub fn query(
        id: impl Into<ProtocolId>,
        target: ComponentTarget,
        property: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            method: ProtocolMethod::Query(QueryParams {
                target,
                property: property.into(),
            }),
        }
    }

    /// Build an `invoke` request for `DesktopInspector::invoke`.
    pub fn invoke(
        id: impl Into<ProtocolId>,
        screen: Rect,
        target: ComponentTarget,
        action: ComponentCommand,
    ) -> Self {
        Self {
            id: id.into(),
            method: ProtocolMethod::Invoke(InvokeParams {
                screen,
                target,
                action,
            }),
        }
    }

    /// Build a `wait_for` request for `DesktopInspector::wait_for`.
    pub fn wait_for(
        id: impl Into<ProtocolId>,
        screen: Rect,
        condition: WaitCondition,
        timeout_ms: u64,
    ) -> Self {
        Self {
            id: id.into(),
            method: ProtocolMethod::WaitFor(WaitForParams {
                screen,
                condition,
                timeout_ms,
                poll_interval_ms: None,
            }),
        }
    }

    /// Build a `tree` request for `DesktopInspector::export_snapshot`.
    pub fn tree(id: impl Into<ProtocolId>, screen: Rect) -> Self {
        Self {
            id: id.into(),
            method: ProtocolMethod::Tree(TreeParams { screen }),
        }
    }

    /// Build a `property_names` request for `DesktopInspector::property_names`.
    pub fn property_names(id: impl Into<ProtocolId>, component_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            method: ProtocolMethod::PropertyNames(PropertyNamesParams {
                id: component_id.into(),
            }),
        }
    }
}

/// Supported method names and their typed parameter payloads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum ProtocolMethod {
    Query(QueryParams),
    Invoke(InvokeParams),
    WaitFor(WaitForParams),
    Tree(TreeParams),
    PropertyNames(PropertyNamesParams),
}

/// Parameters for `query`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueryParams {
    pub target: ComponentTarget,
    pub property: String,
}

/// Parameters for `invoke`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InvokeParams {
    pub screen: Rect,
    pub target: ComponentTarget,
    pub action: ComponentCommand,
}

/// Parameters for `wait_for`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WaitForParams {
    pub screen: Rect,
    pub condition: WaitCondition,
    pub timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_interval_ms: Option<u64>,
}

/// Parameters for `tree`, which maps to `DesktopInspector::export_snapshot`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeParams {
    pub screen: Rect,
}

/// Parameters for `property_names`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyNamesParams {
    pub id: String,
}

/// Top-level response envelope: `{ id, result }` or `{ id, error }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProtocolResponse {
    pub id: ProtocolId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ProtocolResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ComponentError>,
}

impl ProtocolResponse {
    /// Build a success response and leave `error` empty.
    pub fn success(id: impl Into<ProtocolId>, result: ProtocolResult) -> Self {
        Self {
            id: id.into(),
            result: Some(result),
            error: None,
        }
    }

    /// Build an error response and leave `result` empty.
    pub fn error(id: impl Into<ProtocolId>, error: ComponentError) -> Self {
        Self {
            id: id.into(),
            result: None,
            error: Some(error),
        }
    }

    /// Returns true when this response carries a `result` and no `error`.
    pub fn is_success(&self) -> bool {
        self.result.is_some() && self.error.is_none()
    }

    /// Returns true when this response carries an `error` and no `result`.
    pub fn is_error(&self) -> bool {
        self.error.is_some() && self.result.is_none()
    }
}

/// Method-specific success payloads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "value", rename_all = "snake_case")]
pub enum ProtocolResult {
    Query(ComponentValue),
    Invoke(InvokeResult),
    WaitFor(WaitResult),
    Tree(DesktopSnapshot),
    PropertyNames(Vec<String>),
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;

    use super::*;
    use crate::composable::EventResult;
    use crate::{DesktopSnapshotNode, InvokeDispatch, NodeKind};

    fn assert_json_roundtrip<T>(value: T)
    where
        T: Clone + std::fmt::Debug + PartialEq + Serialize + DeserializeOwned,
    {
        let json = serde_json::to_string(&value).expect("serialize");
        let decoded = serde_json::from_str::<T>(&json).expect("deserialize");
        assert_eq!(decoded, value);
    }

    fn screen() -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        }
    }

    fn sample_snapshot() -> DesktopSnapshot {
        DesktopSnapshot {
            bounds: screen(),
            tree: DesktopSnapshotNode {
                kind: NodeKind::Desktop,
                id: Some("desktop".to_string()),
                tag: None,
                name: "Desktop".to_string(),
                type_name: "Desktop".to_string(),
                bounds: Some(screen()),
                text: None,
                state: None,
                window_id: None,
                properties: Default::default(),
                children: Vec::new(),
            },
        }
    }

    #[test]
    fn protocol_requests_round_trip_as_json() {
        let requests = vec![
            ProtocolRequest::query(1, ComponentTarget::Id("input".to_string()), "text"),
            ProtocolRequest::invoke(
                2,
                screen(),
                ComponentTarget::Id("submit".to_string()),
                ComponentCommand::Click,
            ),
            ProtocolRequest::wait_for(
                3,
                screen(),
                WaitCondition::property_equals(
                    ComponentTarget::Id("status".to_string()),
                    "state",
                    ComponentValue::String("ready".to_string()),
                ),
                1_000,
            ),
            ProtocolRequest::tree(4, screen()),
            ProtocolRequest::property_names(5, "input"),
        ];

        for request in requests {
            assert_json_roundtrip(request);
        }
    }

    #[test]
    fn protocol_success_responses_round_trip_as_json() {
        let responses = vec![
            ProtocolResponse::success(
                1,
                ProtocolResult::Query(ComponentValue::String("hello".to_string())),
            ),
            ProtocolResponse::success(
                2,
                ProtocolResult::Invoke(InvokeResult {
                    dispatch: InvokeDispatch::Semantic,
                    result: EventResult::consumed(),
                }),
            ),
            ProtocolResponse::success(
                3,
                ProtocolResult::WaitFor(WaitResult {
                    polls: 2,
                    value: Some(ComponentValue::Bool(true)),
                }),
            ),
            ProtocolResponse::success(4, ProtocolResult::Tree(sample_snapshot())),
            ProtocolResponse::success(
                5,
                ProtocolResult::PropertyNames(vec!["text".to_string(), "selection".to_string()]),
            ),
        ];

        for response in responses {
            assert!(response.is_success());
            assert!(!response.is_error());
            assert_json_roundtrip(response);
        }
    }

    #[test]
    fn component_errors_round_trip_directly_and_in_error_responses() {
        let errors = vec![
            ComponentError::not_found("missing"),
            ComponentError::unsupported_property("text"),
            ComponentError::invalid_value("value", "number"),
            ComponentError::action_not_supported("SelectIndex"),
            ComponentError::render_failed("draw failed"),
            ComponentError::timeout("condition timed out"),
        ];

        for error in errors {
            assert_json_roundtrip(error.clone());

            let response = ProtocolResponse::error("err", error);
            assert!(response.is_error());
            assert!(!response.is_success());
            assert_json_roundtrip(response);
        }
    }
}
