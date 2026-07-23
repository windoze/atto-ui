#![forbid(unsafe_code)]

extern crate self as atto_ui;

pub mod app;
pub mod clipboard;
pub mod component_api;
pub mod composable;
pub mod dialogs;
pub mod drawing;
pub mod fuzzy;
pub mod inspect;
pub mod ipc;
pub mod protocol;
pub mod reactive;
pub mod runtime;
pub mod task;
pub mod text;
pub mod theme;
pub mod widgets;
pub mod wm;

pub use app::{
    CommandDescriptor, CommandRegistry, CommandRegistryError, DEFAULT_KEY_SEQUENCE_TIMEOUT,
    KeyChord, KeySequence, KeySequenceEngine, KeymapMatch, WhichKeyChoice, WhichKeyModel,
    key_chord_label, key_sequence_label,
};
pub use component_api::{
    ComponentCommand, ComponentError, ComponentPropertySchema, ComponentTarget, ComponentValueCodec,
};
pub use composable::{find_by_tag, find_by_tag_mut};
pub use inspect::{
    DesktopChangeTracker, DesktopInspector, DesktopSnapshot, DesktopSnapshotNode, InspectNode,
    InspectSnapshot, InvokeDispatch, InvokeResult, NodeKind, WaitCondition, WaitResult,
};
pub use ipc::{
    IPC_SOCKET_ENV, IpcMethodHandler, IpcServer, IpcServerConfig, send_protocol_request,
};
pub use runtime::{
    ActionMeta, CallbackId, CallbackInvocation, CallbackRegistry, ComponentRegistry,
    ComponentSchema, ComponentSpec, ComponentSpecChild, ComponentValue, EventMeta, PropertyMeta,
    TreeError, TreeOp, ValueType,
};
pub use task::{CancellationToken, TaskHandle, TaskId, TaskMetadata, TaskRegistry};
pub use wm::{
    DockAutoHide, DockSide, Window, WindowBorderStyle, WindowButtons, WindowDecorations,
    WindowDock, WindowId, WindowKind, WindowManager, WindowMinSizeMode, WindowState,
};
