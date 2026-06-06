#![forbid(unsafe_code)]

#[cfg(feature = "async")]
pub use atto_ui_async as async_support;

/// 注册 workspace 附加组件到 `atto-ui` 的全局动态组件注册表中。
///
/// 该函数是“聚合注册入口”，用于减少上层（例如 Python 绑定、示例程序、宿主应用）需要
/// 手动逐个调用 `*_crate::register_runtime_components()` 的重复代码。
pub fn register_all_runtime_components() {
    #[cfg(feature = "chat")]
    {
        let _ = atto_ui_chat::register_runtime_components();
    }
    #[cfg(feature = "editor")]
    {
        let _ = atto_ui_editor::register_runtime_components();
    }
    #[cfg(feature = "file-tree")]
    {
        let _ = atto_ui_file_tree::register_runtime_components();
    }
    #[cfg(feature = "markdown")]
    {
        let _ = atto_ui_markdown::register_runtime_components();
    }
    #[cfg(feature = "terminal")]
    {
        let _ = atto_ui_terminal::register_runtime_components();
    }
}
