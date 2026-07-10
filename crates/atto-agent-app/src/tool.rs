//! Local tool abstractions for the agent runtime.
//!
//! This module defines the registry, permission policy, and OpenAI-compatible
//! tool schema conversion. Built-in tool executors live in focused submodules.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

use crate::deepseek::ChatTool;

mod mutating;
mod readonly;

pub use mutating::{mutating_tool_registry, register_mutating_tools};
pub use readonly::{readonly_tool_registry, register_readonly_tools};

/// Registers every built-in local tool currently available to the agent.
pub fn register_builtin_tools(registry: &mut ToolRegistry) -> Result<()> {
    register_readonly_tools(registry)?;
    register_mutating_tools(registry)?;
    Ok(())
}

/// Builds a registry containing all built-in local tools.
pub fn builtin_tool_registry() -> Result<ToolRegistry> {
    let mut registry = ToolRegistry::new();
    register_builtin_tools(&mut registry)?;
    Ok(registry)
}

/// Public metadata and policy for one local tool.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub permission: ToolPermission,
    pub output: ToolOutputKind,
}

impl ToolSpec {
    /// Builds and validates a tool specification before it can enter a registry.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
        permission: ToolPermission,
        output: ToolOutputKind,
    ) -> Result<Self> {
        let spec = Self {
            name: name.into(),
            description: description.into(),
            parameters,
            permission,
            output,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Validates the OpenAI-visible pieces of the tool definition.
    pub fn validate(&self) -> Result<()> {
        validate_tool_name(&self.name)?;
        if self.description.trim().is_empty() {
            bail!("tool `{}` description must not be empty", self.name);
        }
        validate_parameters_schema(&self.name, &self.parameters)?;
        Ok(())
    }

    /// Converts this local spec into the OpenAI-compatible function tool schema.
    pub fn to_chat_tool(&self) -> ChatTool {
        ChatTool::function(
            self.name.clone(),
            self.description.clone(),
            self.parameters.clone(),
        )
    }
}

/// Static permission requested by a tool specification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolPermission {
    AlwaysAllow,
    ApproveOnce,
    ApproveForProject,
    NeverAllow,
}

/// UI/output representation expected from a tool result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolOutputKind {
    Ansi,
    Markdown,
    Diff,
}

/// Runtime decision after combining a tool spec with project-level grants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolPermissionDecision {
    Allow,
    RequestApproval { allow_project: bool },
    Deny,
}

/// In-memory permission state for the current process and workspace.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolPermissionPolicy {
    project_allowed_tools: BTreeSet<String>,
}

impl ToolPermissionPolicy {
    /// Resolves the effective action for a tool call under the current grants.
    pub fn resolve(&self, spec: &ToolSpec) -> ToolPermissionDecision {
        match spec.permission {
            ToolPermission::AlwaysAllow => ToolPermissionDecision::Allow,
            ToolPermission::ApproveOnce => ToolPermissionDecision::RequestApproval {
                allow_project: false,
            },
            ToolPermission::ApproveForProject if self.is_project_allowed(&spec.name) => {
                ToolPermissionDecision::Allow
            }
            ToolPermission::ApproveForProject => ToolPermissionDecision::RequestApproval {
                allow_project: true,
            },
            ToolPermission::NeverAllow => ToolPermissionDecision::Deny,
        }
    }

    /// Records a process-local allow-for-project grant for a tool name.
    pub fn allow_for_project(&mut self, tool_name: impl Into<String>) {
        self.project_allowed_tools.insert(tool_name.into());
    }

    /// Removes a process-local project grant when a caller needs to reset state.
    pub fn revoke_project_approval(&mut self, tool_name: &str) -> bool {
        self.project_allowed_tools.remove(tool_name)
    }

    /// Reports whether a tool currently has a project-level grant.
    pub fn is_project_allowed(&self, tool_name: &str) -> bool {
        self.project_allowed_tools.contains(tool_name)
    }
}

/// Execution context shared with every local tool call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolContext {
    pub workspace_root: PathBuf,
}

impl ToolContext {
    /// Creates a context rooted at the resolved workspace directory.
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
        }
    }
}

/// Result returned by a local tool executor.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub ok: bool,
    pub output: String,
    pub output_kind: ToolOutputKind,
    pub exit_code: Option<i32>,
}

impl ToolResult {
    /// Builds a successful result for text-like tool output.
    pub fn success(output: impl Into<String>, output_kind: ToolOutputKind) -> Self {
        Self {
            ok: true,
            output: output.into(),
            output_kind,
            exit_code: None,
        }
    }

    /// Builds a failed result for tools that report errors as model-visible output.
    pub fn failure(output: impl Into<String>, output_kind: ToolOutputKind) -> Self {
        Self {
            ok: false,
            output: output.into(),
            output_kind,
            exit_code: None,
        }
    }

    /// Adds a process exit code to command-like tool results.
    pub fn with_exit_code(mut self, exit_code: i32) -> Self {
        self.exit_code = Some(exit_code);
        self
    }
}

/// Object-safe executor implemented by each local tool.
pub trait ToolExecutor: Send + Sync {
    /// Returns the stable metadata and policy for this executor.
    fn spec(&self) -> ToolSpec;

    /// Executes the tool with already parsed JSON arguments.
    fn execute(&self, ctx: ToolContext, args: Value) -> Result<ToolResult>;
}

/// Deterministic registry of named local tools.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, RegisteredTool>,
}

impl ToolRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a concrete executor under its spec name.
    pub fn register<E>(&mut self, executor: E) -> Result<()>
    where
        E: ToolExecutor + 'static,
    {
        self.register_arc(Arc::new(executor))
    }

    /// Registers a shared executor, useful for runtime composition and tests.
    pub fn register_arc(&mut self, executor: Arc<dyn ToolExecutor>) -> Result<()> {
        let spec = executor.spec();
        spec.validate().context("invalid tool spec")?;
        if self.tools.contains_key(&spec.name) {
            bail!("tool `{}` is already registered", spec.name);
        }

        self.tools
            .insert(spec.name.clone(), RegisteredTool { spec, executor });
        Ok(())
    }

    /// Returns the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Reports whether no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Looks up a registered tool specification by name.
    pub fn spec(&self, name: &str) -> Option<&ToolSpec> {
        self.tools.get(name).map(|tool| &tool.spec)
    }

    /// Returns all specs in deterministic name order.
    pub fn specs(&self) -> impl Iterator<Item = &ToolSpec> {
        self.tools.values().map(|tool| &tool.spec)
    }

    /// Converts every registered tool into OpenAI-compatible schema objects.
    pub fn chat_tools(&self) -> Vec<ChatTool> {
        self.specs().map(ToolSpec::to_chat_tool).collect()
    }

    /// Executes a registered tool by name.
    pub fn execute(&self, name: &str, ctx: ToolContext, args: Value) -> Result<ToolResult> {
        let tool = self
            .tools
            .get(name)
            .with_context(|| format!("tool `{name}` is not registered"))?;
        tool.executor.execute(ctx, args)
    }
}

pub(super) struct ToolArgs {
    tool_name: &'static str,
    object: Map<String, Value>,
}

impl ToolArgs {
    pub(super) fn parse(
        tool_name: &'static str,
        value: Value,
        allowed_keys: &[&str],
    ) -> Result<Self> {
        let Value::Object(object) = value else {
            bail!("tool `{tool_name}` arguments must be a JSON object");
        };
        for key in object.keys() {
            if !allowed_keys.contains(&key.as_str()) {
                bail!("tool `{tool_name}` received unknown argument `{key}`");
            }
        }
        Ok(Self { tool_name, object })
    }

    pub(super) fn required_string(&self, name: &str) -> Result<&str> {
        self.optional_string(name)?.with_context(|| {
            format!(
                "tool `{}` requires string argument `{name}`",
                self.tool_name
            )
        })
    }

    pub(super) fn optional_string(&self, name: &str) -> Result<Option<&str>> {
        match self.object.get(name) {
            Some(Value::String(value)) if value.trim().is_empty() => {
                bail!(
                    "tool `{}` argument `{name}` must not be empty",
                    self.tool_name
                )
            }
            Some(Value::String(value)) => Ok(Some(value)),
            Some(_) => bail!(
                "tool `{}` argument `{name}` must be a string",
                self.tool_name
            ),
            None => Ok(None),
        }
    }

    pub(super) fn required_string_array(&self, name: &str) -> Result<Vec<String>> {
        let Some(value) = self.object.get(name) else {
            bail!("tool `{}` requires array argument `{name}`", self.tool_name);
        };
        let Value::Array(items) = value else {
            bail!(
                "tool `{}` argument `{name}` must be an array of strings; shell strings are not supported",
                self.tool_name
            );
        };
        if items.is_empty() {
            bail!(
                "tool `{}` argument `{name}` must contain at least one string",
                self.tool_name
            );
        }
        items
            .iter()
            .enumerate()
            .map(|(index, value)| match value {
                Value::String(item) if item.trim().is_empty() => bail!(
                    "tool `{}` argument `{name}` item {index} must not be empty",
                    self.tool_name
                ),
                Value::String(item) => Ok(item.clone()),
                _ => bail!(
                    "tool `{}` argument `{name}` item {index} must be a string",
                    self.tool_name
                ),
            })
            .collect()
    }

    pub(super) fn optional_bool(&self, name: &str, default: bool) -> Result<bool> {
        match self.object.get(name) {
            Some(Value::Bool(value)) => Ok(*value),
            Some(_) => bail!(
                "tool `{}` argument `{name}` must be a boolean",
                self.tool_name
            ),
            None => Ok(default),
        }
    }

    pub(super) fn optional_usize(&self, name: &str, default: usize, max: usize) -> Result<usize> {
        match self.object.get(name) {
            Some(Value::Number(value)) => {
                let Some(value) = value.as_u64() else {
                    bail!(
                        "tool `{}` argument `{name}` must be a positive integer",
                        self.tool_name
                    );
                };
                if value == 0 || value > max as u64 {
                    bail!(
                        "tool `{}` argument `{name}` must be between 1 and {max}",
                        self.tool_name
                    );
                }
                Ok(value as usize)
            }
            Some(_) => bail!(
                "tool `{}` argument `{name}` must be an integer",
                self.tool_name
            ),
            None => Ok(default),
        }
    }
}

pub(super) fn canonical_workspace_root(ctx: &ToolContext) -> Result<PathBuf> {
    let root = ctx
        .workspace_root
        .canonicalize()
        .with_context(|| format!("workspace `{}` must exist", ctx.workspace_root.display()))?;
    if !root.is_dir() {
        bail!("workspace `{}` is not a directory", root.display());
    }
    Ok(root)
}

pub(super) fn resolve_existing_workspace_path(
    workspace_root: &Path,
    requested: &str,
) -> Result<PathBuf> {
    let raw = Path::new(requested);
    let joined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        workspace_root.join(raw)
    };
    let path = joined
        .canonicalize()
        .with_context(|| format!("path `{}` must exist", joined.display()))?;
    ensure_workspace_path(workspace_root, &path)?;
    Ok(path)
}

pub(super) fn ensure_workspace_path(workspace_root: &Path, path: &Path) -> Result<()> {
    if is_workspace_path(workspace_root, path) {
        Ok(())
    } else {
        bail!(
            "path `{}` escapes workspace `{}`",
            path.display(),
            workspace_root.display()
        )
    }
}

pub(super) fn is_workspace_path(workspace_root: &Path, path: &Path) -> bool {
    path == workspace_root || path.starts_with(workspace_root)
}

pub(super) fn display_workspace_path(workspace_root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(workspace_root).unwrap_or(path);
    if relative.as_os_str().is_empty() {
        ".".to_string()
    } else {
        relative
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/")
    }
}

#[derive(Clone)]
struct RegisteredTool {
    spec: ToolSpec,
    executor: Arc<dyn ToolExecutor>,
}

fn validate_tool_name(name: &str) -> Result<()> {
    if name.is_empty() || name.trim() != name {
        bail!("tool name must not be empty or contain surrounding whitespace");
    }
    if name.len() > 64 {
        bail!("tool `{name}` name must be at most 64 bytes");
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        bail!("tool `{name}` name may only contain ASCII letters, digits, `_`, or `-`");
    }
    Ok(())
}

fn validate_parameters_schema(tool_name: &str, parameters: &Value) -> Result<()> {
    let object = parameters
        .as_object()
        .with_context(|| format!("tool `{tool_name}` parameters must be a JSON schema object"))?;
    match object.get("type").and_then(Value::as_str) {
        Some("object") => Ok(()),
        Some(other) => bail!("tool `{tool_name}` parameters type must be `object`, got `{other}`"),
        None => bail!("tool `{tool_name}` parameters must declare type `object`"),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[derive(Clone)]
    struct MockTool {
        spec: ToolSpec,
        output: &'static str,
    }

    impl MockTool {
        fn new(name: &str, permission: ToolPermission, output: &'static str) -> Self {
            Self {
                spec: ToolSpec::new(
                    name,
                    format!("Run the {name} test tool."),
                    json!({
                        "type": "object",
                        "properties": {
                            "value": { "type": "string" }
                        },
                        "required": ["value"]
                    }),
                    permission,
                    ToolOutputKind::Markdown,
                )
                .unwrap(),
                output,
            }
        }
    }

    impl ToolExecutor for MockTool {
        fn spec(&self) -> ToolSpec {
            self.spec.clone()
        }

        fn execute(&self, ctx: ToolContext, args: Value) -> Result<ToolResult> {
            let value = args
                .get("value")
                .and_then(Value::as_str)
                .context("value argument is required")?;
            Ok(ToolResult::success(
                format!("{}:{}:{value}", ctx.workspace_root.display(), self.output),
                self.spec.output,
            ))
        }
    }

    #[test]
    fn tool_spec_converts_to_openai_function_schema() {
        let spec = ToolSpec::new(
            "read_file",
            "Read a UTF-8 file under the workspace root.",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
            ToolPermission::AlwaysAllow,
            ToolOutputKind::Markdown,
        )
        .unwrap();

        let tool = spec.to_chat_tool();

        assert_eq!(
            serde_json::to_value(tool).unwrap(),
            json!({
                "type": "function",
                "function": {
                    "name": "read_file",
                    "description": "Read a UTF-8 file under the workspace root.",
                    "parameters": {
                        "type": "object",
                        "properties": { "path": { "type": "string" } },
                        "required": ["path"]
                    }
                }
            })
        );
    }

    #[test]
    fn registry_registers_executes_and_lists_tools_in_name_order() {
        let mut registry = ToolRegistry::new();
        registry
            .register(MockTool::new(
                "search_text",
                ToolPermission::AlwaysAllow,
                "search",
            ))
            .unwrap();
        registry
            .register(MockTool::new(
                "read_file",
                ToolPermission::AlwaysAllow,
                "read",
            ))
            .unwrap();

        assert_eq!(registry.len(), 2);
        assert!(registry.spec("read_file").is_some());
        let names = registry
            .specs()
            .map(|spec| spec.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["read_file", "search_text"]);
        let chat_tool_names = registry
            .chat_tools()
            .into_iter()
            .map(|tool| tool.function.name)
            .collect::<Vec<_>>();
        assert_eq!(chat_tool_names, vec!["read_file", "search_text"]);

        let result = registry
            .execute(
                "read_file",
                ToolContext::new("/workspace"),
                json!({ "value": "src/lib.rs" }),
            )
            .unwrap();
        assert!(result.ok);
        assert_eq!(result.output, "/workspace:read:src/lib.rs");
    }

    #[test]
    fn registry_rejects_duplicate_names_and_unknown_execution() {
        let mut registry = ToolRegistry::new();
        registry
            .register(MockTool::new(
                "read_file",
                ToolPermission::AlwaysAllow,
                "read",
            ))
            .unwrap();

        let duplicate = registry
            .register(MockTool::new(
                "read_file",
                ToolPermission::AlwaysAllow,
                "again",
            ))
            .unwrap_err();
        assert!(duplicate.to_string().contains("already registered"));

        let missing = registry
            .execute("missing", ToolContext::new("/workspace"), json!({}))
            .unwrap_err();
        assert!(missing.to_string().contains("not registered"));
    }

    #[test]
    fn permission_policy_resolves_static_and_project_grants() {
        let allow = MockTool::new("read_file", ToolPermission::AlwaysAllow, "read").spec;
        let approve_once = MockTool::new("apply_patch", ToolPermission::ApproveOnce, "patch").spec;
        let approve_project =
            MockTool::new("run_command", ToolPermission::ApproveForProject, "cmd").spec;
        let deny = MockTool::new("secret_tool", ToolPermission::NeverAllow, "secret").spec;
        let mut policy = ToolPermissionPolicy::default();

        assert_eq!(policy.resolve(&allow), ToolPermissionDecision::Allow);
        assert_eq!(
            policy.resolve(&approve_once),
            ToolPermissionDecision::RequestApproval {
                allow_project: false
            }
        );
        assert_eq!(
            policy.resolve(&approve_project),
            ToolPermissionDecision::RequestApproval {
                allow_project: true
            }
        );
        assert_eq!(policy.resolve(&deny), ToolPermissionDecision::Deny);

        policy.allow_for_project("run_command");
        assert!(policy.is_project_allowed("run_command"));
        assert_eq!(
            policy.resolve(&approve_project),
            ToolPermissionDecision::Allow
        );
        assert!(policy.revoke_project_approval("run_command"));
        assert_eq!(
            policy.resolve(&approve_project),
            ToolPermissionDecision::RequestApproval {
                allow_project: true
            }
        );
    }

    #[test]
    fn spec_validation_rejects_invalid_openai_schema_inputs() {
        let bad_name = ToolSpec::new(
            "read file",
            "Read a file.",
            json!({ "type": "object" }),
            ToolPermission::AlwaysAllow,
            ToolOutputKind::Markdown,
        )
        .unwrap_err();
        assert!(bad_name.to_string().contains("may only contain"));

        let empty_description = ToolSpec::new(
            "read_file",
            " ",
            json!({ "type": "object" }),
            ToolPermission::AlwaysAllow,
            ToolOutputKind::Markdown,
        )
        .unwrap_err();
        assert!(empty_description.to_string().contains("description"));

        let bad_parameters = ToolSpec::new(
            "read_file",
            "Read a file.",
            json!({ "type": "string" }),
            ToolPermission::AlwaysAllow,
            ToolOutputKind::Markdown,
        )
        .unwrap_err();
        assert!(bad_parameters.to_string().contains("parameters type"));
    }
}
