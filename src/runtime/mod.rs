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
pub use spec::*;
pub use tree::ComponentTree;
