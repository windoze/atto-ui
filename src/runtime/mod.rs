//! Runtime integration layer.

mod builtins;
mod callback_handle;
mod props;
mod registry;
mod spec;
#[cfg(test)]
mod tests;
mod tree;

pub use builtins::{builtin_registry, component_schema, event_handle, wrap_with_id};
pub use callback_handle::CallbackHandle;
pub use props::{
    invalid_prop, invalid_prop_reason, prop_bool, prop_f64, prop_string, prop_table, prop_u16,
    prop_u64, prop_usize, prop_vec_string,
};
pub use registry::{RegistryExtension, global_registry, register_registry_extension};
// Listed explicitly rather than re-exported with a glob: this module is the public surface the IPC
// protocol and the language bindings are written against, so anything added to `spec` should become
// part of that contract by choice rather than by default.
pub use spec::{
    ActionMeta, AlignSpec, AnchorPlacementSpec, AnchorSpec, CallbackId, CallbackInvocation,
    CallbackRegistry, ComponentRegistry, ComponentSchema, ComponentSpec, ComponentSpecChild,
    ComponentValue, EdgeInsetsSpec, EventMeta, LayoutSpec, PropertyMeta, Rect, SizeSpec, TreeError,
    TreeOp, ValueType, apply_tree_ops,
};
pub use tree::ComponentTree;
