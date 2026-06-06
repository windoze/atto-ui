use std::collections::BTreeMap;
use std::sync::OnceLock;

use parking_lot::Mutex;

use crate::composable::Component;

use super::{CallbackRegistry, ComponentRegistry, builtin_registry};

/// 全局组件注册表扩展。
///
/// 通过扩展注册，可以让上层 crate（例如 `atto-ui-file-tree`）在不修改基础框架的情况下
/// 将自己的组件注册到动态系统中。
pub type RegistryExtension = fn(&mut ComponentRegistry<Box<dyn Component>>, CallbackRegistry);

static REGISTRY_EXTENSIONS: OnceLock<Mutex<BTreeMap<String, RegistryExtension>>> = OnceLock::new();

fn registry_extensions() -> &'static Mutex<BTreeMap<String, RegistryExtension>> {
    REGISTRY_EXTENSIONS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// 注册一个全局组件注册表扩展（幂等）。
///
/// 返回：
/// - `true`：本次注册成功（此前没有同名扩展）
/// - `false`：已存在同名扩展，本次调用不会覆盖
pub fn register_registry_extension(name: impl Into<String>, register: RegistryExtension) -> bool {
    let name = name.into();
    let mut guard = registry_extensions().lock();
    if guard.contains_key(&name) {
        return false;
    }
    guard.insert(name, register);
    true
}

/// 动态组件注册表：内置组件 + 全局扩展组件。
pub fn global_registry(callbacks: CallbackRegistry) -> ComponentRegistry<Box<dyn Component>> {
    let mut registry = builtin_registry(callbacks.clone());
    let extensions = {
        let guard = registry_extensions().lock();
        guard.values().copied().collect::<Vec<_>>()
    };
    for register in extensions {
        register(&mut registry, callbacks.clone());
    }
    registry
}
