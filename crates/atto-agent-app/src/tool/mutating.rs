//! Built-in mutating tools that require explicit approval before execution.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::{
    ToolArgs, ToolContext, ToolExecutor, ToolOutputKind, ToolPermission, ToolRegistry, ToolResult,
    ToolSpec, canonical_workspace_root, display_workspace_path, ensure_workspace_path,
    resolve_existing_workspace_path,
};

const COMMAND_OUTPUT_MAX_BYTES: usize = 256 * 1024;
const PIPE_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

/// Registers the built-in tools that can mutate the workspace or run processes.
pub fn register_mutating_tools(registry: &mut ToolRegistry) -> Result<()> {
    registry.register(ApplyPatchTool)?;
    registry.register(RunCommandTool)?;
    Ok(())
}

/// Builds a registry containing only the built-in mutating tools.
pub fn mutating_tool_registry() -> Result<ToolRegistry> {
    let mut registry = ToolRegistry::new();
    register_mutating_tools(&mut registry)?;
    Ok(registry)
}

#[derive(Clone, Copy, Debug)]
struct ApplyPatchTool;

impl ToolExecutor for ApplyPatchTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "apply_patch",
            "Apply a unified diff patch to files under the workspace root after approval.",
            json!({
                "type": "object",
                "properties": {
                    "patch": {
                        "type": "string",
                        "description": "Unified diff patch text. All patched paths must be relative to the workspace."
                    }
                },
                "required": ["patch"],
                "additionalProperties": false
            }),
            ToolPermission::ApproveForProject,
            ToolOutputKind::Diff,
        )
        .expect("built-in apply_patch spec must be valid")
    }

    fn execute(&self, ctx: ToolContext, args: Value) -> Result<ToolResult> {
        let args = ToolArgs::parse("apply_patch", args, &["patch"])?;
        let patch = args.required_string("patch")?;
        if patch.trim().is_empty() {
            bail!("tool `apply_patch` argument `patch` must not be empty");
        }

        let workspace_root = canonical_workspace_root(&ctx)?;
        let touched_paths = validate_patch_paths(&workspace_root, patch)?;
        let check = git_apply(&workspace_root, patch, true, ctx.timeout)?;
        if !check.status.success() {
            return Ok(ToolResult::failure(
                format_process_output("git apply --check", &check),
                ToolOutputKind::Markdown,
            )
            .with_exit_code(exit_code(&check)));
        }

        let applied = git_apply(&workspace_root, patch, false, ctx.timeout)?;
        if !applied.status.success() {
            return Ok(ToolResult::failure(
                format_process_output("git apply", &applied),
                ToolOutputKind::Markdown,
            )
            .with_exit_code(exit_code(&applied)));
        }

        Ok(ToolResult::success(
            format_applied_patch_output(&touched_paths, patch),
            ToolOutputKind::Diff,
        ))
    }
}

#[derive(Clone, Copy, Debug)]
struct RunCommandTool;

impl ToolExecutor for RunCommandTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "run_command",
            "Run a local command from an argv array inside the workspace after approval.",
            json!({
                "type": "object",
                "properties": {
                    "argv": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1,
                        "description": "Command argv. argv[0] is the executable; shell strings are not accepted."
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Workspace-relative or absolute directory to run from. Defaults to the workspace root."
                    }
                },
                "required": ["argv"],
                "additionalProperties": false
            }),
            ToolPermission::ApproveForProject,
            ToolOutputKind::Ansi,
        )
        .expect("built-in run_command spec must be valid")
    }

    fn execute(&self, ctx: ToolContext, args: Value) -> Result<ToolResult> {
        let args = ToolArgs::parse("run_command", args, &["argv", "cwd"])?;
        let argv = args.required_string_array("argv")?;
        let requested_cwd = args.optional_string("cwd")?.unwrap_or(".");
        let workspace_root = canonical_workspace_root(&ctx)?;
        let cwd = resolve_existing_workspace_path(&workspace_root, requested_cwd)?;
        if !cwd.is_dir() {
            bail!(
                "tool `run_command` cwd `{}` is not a directory",
                cwd.display()
            );
        }

        let timed_output = match command_output_with_timeout(
            Command::new(&argv[0]).args(&argv[1..]).current_dir(&cwd),
            ctx.timeout,
        ) {
            Ok(output) => output,
            Err(error) => {
                return Ok(ToolResult::failure(
                    format!("failed to spawn argv {}: {error}", format_argv(&argv)),
                    ToolOutputKind::Ansi,
                ));
            }
        };
        if timed_output.timed_out {
            return Ok(ToolResult::failure(
                format_command_timeout_output(
                    &workspace_root,
                    &cwd,
                    &argv,
                    ctx.timeout,
                    &timed_output.output,
                ),
                ToolOutputKind::Ansi,
            )
            .with_exit_code(exit_code(&timed_output.output)));
        }

        let rendered = format_command_output(&workspace_root, &cwd, &argv, &timed_output.output);
        let result = if timed_output.output.status.success() {
            ToolResult::success(rendered, ToolOutputKind::Ansi)
        } else {
            ToolResult::failure(rendered, ToolOutputKind::Ansi)
        };
        Ok(result.with_exit_code(exit_code(&timed_output.output)))
    }
}

fn validate_patch_paths(workspace_root: &Path, patch: &str) -> Result<Vec<String>> {
    reject_binary_patch(patch)?;

    let mut paths = BTreeSet::new();
    for (line_index, line) in patch.lines().enumerate() {
        let line_number = line_index + 1;
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let mut parts = rest.split_whitespace();
            let old_path = parts.next().with_context(|| {
                format!("apply_patch diff header on line {line_number} is missing old path")
            })?;
            let new_path = parts.next().with_context(|| {
                format!("apply_patch diff header on line {line_number} is missing new path")
            })?;
            add_patch_path(&mut paths, workspace_root, old_path, line_number)?;
            add_patch_path(&mut paths, workspace_root, new_path, line_number)?;
            continue;
        }

        if let Some(rest) = line
            .strip_prefix("--- ")
            .or_else(|| line.strip_prefix("+++ "))
        {
            add_patch_path(
                &mut paths,
                workspace_root,
                patch_header_token(rest),
                line_number,
            )?;
            continue;
        }

        for prefix in ["rename from ", "rename to ", "copy from ", "copy to "] {
            if let Some(rest) = line.strip_prefix(prefix) {
                add_patch_path(&mut paths, workspace_root, rest.trim(), line_number)?;
                break;
            }
        }
    }

    if paths.is_empty() {
        bail!("tool `apply_patch` patch must declare at least one file path");
    }
    Ok(paths.into_iter().collect())
}

fn add_patch_path(
    paths: &mut BTreeSet<String>,
    workspace_root: &Path,
    raw_path: &str,
    line_number: usize,
) -> Result<()> {
    let Some(path) = normalize_patch_path(raw_path, line_number)? else {
        return Ok(());
    };
    validate_relative_patch_path(workspace_root, path, line_number)?;
    paths.insert(path.to_string());
    Ok(())
}

fn normalize_patch_path(raw_path: &str, line_number: usize) -> Result<Option<&str>> {
    let raw_path = raw_path.trim();
    if raw_path == "/dev/null" {
        return Ok(None);
    }
    if raw_path.starts_with('"') || raw_path.ends_with('"') {
        bail!("apply_patch quoted paths are not supported on line {line_number}: {raw_path}");
    }
    let path = raw_path
        .strip_prefix("a/")
        .or_else(|| raw_path.strip_prefix("b/"))
        .unwrap_or(raw_path);
    if path.is_empty() {
        bail!("apply_patch empty path on line {line_number}");
    }
    Ok(Some(path))
}

fn patch_header_token(rest: &str) -> &str {
    rest.split(['\t', ' '])
        .find(|token| !token.is_empty())
        .unwrap_or(rest)
}

fn validate_relative_patch_path(
    workspace_root: &Path,
    relative_path: &str,
    line_number: usize,
) -> Result<()> {
    if relative_path.contains('\0') {
        bail!("apply_patch path on line {line_number} contains a NUL byte");
    }

    let path = Path::new(relative_path);
    if path.is_absolute() {
        bail!("apply_patch path `{relative_path}` on line {line_number} must be relative");
    }

    let mut has_normal_component = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => has_normal_component = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => bail!(
                "apply_patch path `{relative_path}` on line {line_number} escapes the workspace"
            ),
        }
    }
    if !has_normal_component {
        bail!("apply_patch path `{relative_path}` on line {line_number} is not a file path");
    }

    let joined = workspace_root.join(path);
    let resolved = resolve_patch_target_path(workspace_root, &joined)?;
    if resolved.exists() {
        ensure_utf8_text_file(&resolved, relative_path, line_number)?;
    }
    Ok(())
}

fn reject_binary_patch(patch: &str) -> Result<()> {
    for (line_index, line) in patch.lines().enumerate() {
        if line == "GIT binary patch" || line.starts_with("Binary files ") {
            bail!(
                "tool `apply_patch` only supports text patches; binary patch marker found on line {}",
                line_index + 1
            );
        }
    }
    Ok(())
}

fn resolve_patch_target_path(workspace_root: &Path, path: &Path) -> Result<PathBuf> {
    let mut ancestor = path;
    let mut missing_components = Vec::<OsString>::new();
    while !ancestor.exists() {
        if let Some(name) = ancestor.file_name() {
            missing_components.push(name.to_os_string());
        }
        ancestor = ancestor
            .parent()
            .with_context(|| format!("path `{}` has no existing parent", path.display()))?;
    }
    let canonical = ancestor
        .canonicalize()
        .with_context(|| format!("failed to resolve `{}`", ancestor.display()))?;
    ensure_workspace_path(workspace_root, &canonical)?;

    let mut resolved = canonical;
    for component in missing_components.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn ensure_utf8_text_file(path: &Path, display_path: &str, line_number: usize) -> Result<()> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("failed to read metadata for `{}`", path.display()))?;
    if !metadata.is_file() {
        bail!("apply_patch path `{display_path}` on line {line_number} is not a text file");
    }
    let bytes =
        std::fs::read(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    std::str::from_utf8(&bytes).with_context(|| {
        format!("apply_patch path `{display_path}` on line {line_number} is not a UTF-8 text file")
    })?;
    Ok(())
}

fn git_apply(
    workspace_root: &Path,
    patch: &str,
    check_only: bool,
    timeout: Duration,
) -> Result<Output> {
    let mut command = Command::new("git");
    command.arg("apply");
    if check_only {
        command.arg("--check");
    }
    command.arg("--whitespace=nowarn").arg("-");
    let mut child = command
        .current_dir(workspace_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn `git apply`")?;
    let mut stdin = child
        .stdin
        .take()
        .context("failed to open `git apply` stdin")?;
    match stdin.write_all(patch.as_bytes()) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {}
        Err(error) => return Err(error).context("failed to write patch to `git apply`"),
    }
    drop(stdin);
    let timed_output =
        wait_child_with_output_timeout(child, timeout).context("failed to wait for `git apply`")?;
    if timed_output.timed_out {
        bail!(
            "`git apply{}` timed out after {}",
            if check_only { " --check" } else { "" },
            format_timeout_duration(timeout)
        );
    }
    Ok(timed_output.output)
}

struct TimedOutput {
    output: Output,
    timed_out: bool,
}

fn command_output_with_timeout(command: &mut Command, timeout: Duration) -> Result<TimedOutput> {
    let child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    wait_child_with_output_timeout(child, timeout)
}

fn wait_child_with_output_timeout(mut child: Child, timeout: Duration) -> Result<TimedOutput> {
    let stdout = child.stdout.take().map(read_pipe_to_end);
    let stderr = child.stderr.take().map(read_pipe_to_end);
    let start = Instant::now();
    let mut timed_out = false;

    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if start.elapsed() >= timeout {
            timed_out = true;
            if let Err(error) = child.kill()
                && child.try_wait()?.is_none()
            {
                return Err(error).context("failed to kill timed-out process");
            }
            break;
        }
        let remaining = timeout.saturating_sub(start.elapsed());
        thread::sleep(remaining.min(Duration::from_millis(10)));
    }

    let status = child.wait()?;
    let stdout = recv_pipe_reader(stdout)?;
    let stderr = recv_pipe_reader(stderr)?;
    Ok(TimedOutput {
        output: Output {
            status,
            stdout,
            stderr,
        },
        timed_out,
    })
}

fn read_pipe_to_end<R>(mut pipe: R) -> mpsc::Receiver<io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(read_limited_pipe(&mut pipe));
    });
    receiver
}

fn read_limited_pipe(pipe: &mut dyn Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut truncated = false;
    let mut buffer = [0u8; 8192];
    loop {
        let read = pipe.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = COMMAND_OUTPUT_MAX_BYTES.saturating_sub(bytes.len());
        if remaining == 0 {
            truncated = true;
            continue;
        }
        let visible = remaining.min(read);
        bytes.extend_from_slice(&buffer[..visible]);
        if visible < read {
            truncated = true;
        }
    }
    if truncated {
        bytes.extend_from_slice(b"\n[output truncated]");
    }
    Ok(bytes)
}

fn recv_pipe_reader(receiver: Option<mpsc::Receiver<io::Result<Vec<u8>>>>) -> Result<Vec<u8>> {
    let Some(receiver) = receiver else {
        return Ok(Vec::new());
    };
    match receiver.recv_timeout(PIPE_DRAIN_TIMEOUT) {
        Ok(output) => output.context("failed to read process output"),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Ok(b"[process output pipe did not close after timeout]".to_vec())
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Ok(Vec::new()),
    }
}

fn format_applied_patch_output(paths: &[String], patch: &str) -> String {
    let mut output = format!("Applied patch touching {} path(s):\n", paths.len());
    for path in paths {
        output.push_str("- ");
        output.push_str(path);
        output.push('\n');
    }
    output.push('\n');
    output.push_str(patch);
    output
}

fn format_command_output(
    workspace_root: &Path,
    cwd: &Path,
    argv: &[String],
    output: &Output,
) -> String {
    format!(
        "argv: {}\ncwd: `{}`\nexit_code: {}\n\n[stdout]\n{}\n\n[stderr]\n{}",
        format_argv(argv),
        display_workspace_path(workspace_root, cwd),
        exit_code(output),
        limited_output(&output.stdout),
        limited_output(&output.stderr)
    )
}

fn format_command_timeout_output(
    workspace_root: &Path,
    cwd: &Path,
    argv: &[String],
    timeout: Duration,
    output: &Output,
) -> String {
    format!(
        "command timed out after {} and was terminated.\n\n{}",
        format_timeout_duration(timeout),
        format_command_output(workspace_root, cwd, argv, output)
    )
}

fn format_process_output(label: &str, output: &Output) -> String {
    format!(
        "{label} failed with exit code {}.\n\n[stdout]\n{}\n\n[stderr]\n{}",
        exit_code(output),
        limited_output(&output.stdout),
        limited_output(&output.stderr)
    )
}

fn format_argv(argv: &[String]) -> String {
    serde_json::to_string(argv).unwrap_or_else(|_| format!("{argv:?}"))
}

fn limited_output(bytes: &[u8]) -> String {
    let truncated = bytes.len() > COMMAND_OUTPUT_MAX_BYTES;
    let visible = if truncated {
        &bytes[..COMMAND_OUTPUT_MAX_BYTES]
    } else {
        bytes
    };
    let mut text = String::from_utf8_lossy(visible).into_owned();
    if truncated {
        text.push_str("\n[output truncated]");
    }
    text
}

fn format_timeout_duration(duration: Duration) -> String {
    if duration.as_secs() > 0 && duration.subsec_millis() == 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

fn exit_code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        let dir = env::temp_dir().join(format!(
            "atto-agent-mutating-tool-{name}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("failed to create test dir");
        dir
    }

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create parent dir");
        }
        fs::write(path, text).expect("failed to write test fixture");
    }

    #[test]
    fn mutating_registry_registers_builtin_tools_with_approval() {
        let registry = mutating_tool_registry().unwrap();

        let names = registry
            .specs()
            .map(|spec| spec.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["apply_patch", "run_command"]);
        assert!(
            registry
                .specs()
                .all(|spec| spec.permission == ToolPermission::ApproveForProject)
        );
        assert_eq!(
            registry.spec("apply_patch").unwrap().output,
            ToolOutputKind::Diff
        );
        assert_eq!(
            registry.spec("run_command").unwrap().output,
            ToolOutputKind::Ansi
        );
    }

    #[test]
    fn apply_patch_applies_unified_diff_inside_workspace() {
        let root = test_dir("apply-patch");
        write(&root.join("README.md"), "old\n");
        let registry = mutating_tool_registry().unwrap();

        let result = registry
            .execute(
                "apply_patch",
                ToolContext::new(root.clone()),
                json!({
                    "patch": "diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-old\n+new\n"
                }),
            )
            .unwrap();

        assert!(result.ok);
        assert_eq!(result.output_kind, ToolOutputKind::Diff);
        assert_eq!(fs::read_to_string(root.join("README.md")).unwrap(), "new\n");
        assert!(result.output.contains("README.md"));
    }

    #[test]
    fn apply_patch_rejects_workspace_escape_before_git_apply() {
        let root = test_dir("apply-escape");
        write(&root.join("README.md"), "old\n");
        let registry = mutating_tool_registry().unwrap();

        let error = registry
            .execute(
                "apply_patch",
                ToolContext::new(root),
                json!({
                    "patch": "diff --git a/../secret.txt b/../secret.txt\n--- a/../secret.txt\n+++ b/../secret.txt\n@@ -1 +1 @@\n-old\n+new\n"
                }),
            )
            .unwrap_err();

        assert!(error.to_string().contains("escapes the workspace"));
    }

    #[cfg(unix)]
    #[test]
    fn apply_patch_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = test_dir("apply-symlink-root");
        let outside = test_dir("apply-symlink-outside");
        write(&outside.join("secret.txt"), "old\n");
        symlink(outside.join("secret.txt"), root.join("secret-link.txt")).unwrap();
        let registry = mutating_tool_registry().unwrap();

        let error = registry
            .execute(
                "apply_patch",
                ToolContext::new(root),
                json!({
                    "patch": "diff --git a/secret-link.txt b/secret-link.txt\n--- a/secret-link.txt\n+++ b/secret-link.txt\n@@ -1 +1 @@\n-old\n+new\n"
                }),
            )
            .unwrap_err();

        assert!(error.to_string().contains("escapes workspace"));
    }

    #[test]
    fn apply_patch_rejects_binary_patch_and_existing_binary_files() {
        let root = test_dir("apply-binary");
        fs::write(root.join("data.bin"), [0xff, 0x00, 0x80]).unwrap();
        let registry = mutating_tool_registry().unwrap();

        let binary_marker = registry
            .execute(
                "apply_patch",
                ToolContext::new(root.clone()),
                json!({
                    "patch": "diff --git a/data.bin b/data.bin\nGIT binary patch\n"
                }),
            )
            .unwrap_err();
        assert!(
            binary_marker
                .to_string()
                .contains("only supports text patches")
        );

        let binary_file = registry
            .execute(
                "apply_patch",
                ToolContext::new(root),
                json!({
                    "patch": "diff --git a/data.bin b/data.bin\n--- a/data.bin\n+++ b/data.bin\n@@ -1 +1 @@\n-old\n+new\n"
                }),
            )
            .unwrap_err();
        assert!(binary_file.to_string().contains("UTF-8 text file"));
    }

    #[test]
    fn run_command_executes_argv_without_shell() {
        let root = test_dir("run-command");
        let registry = mutating_tool_registry().unwrap();
        let current_exe = env::current_exe().unwrap();

        let result = registry
            .execute(
                "run_command",
                ToolContext::new(root),
                json!({
                    "argv": [current_exe.to_str().unwrap(), "--help"],
                    "cwd": "."
                }),
            )
            .unwrap();

        assert!(result.ok);
        assert_eq!(result.output_kind, ToolOutputKind::Ansi);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.output.contains("argv:"));
        assert!(result.output.contains("[stdout]"));
    }

    #[test]
    fn run_command_rejects_shell_string_and_workspace_escape() {
        let root = test_dir("run-invalid-root");
        let outside = test_dir("run-invalid-outside");
        let registry = mutating_tool_registry().unwrap();

        let shell_string = registry
            .execute(
                "run_command",
                ToolContext::new(root.clone()),
                json!({ "argv": "echo should-not-use-shell" }),
            )
            .unwrap_err();
        assert!(
            shell_string
                .to_string()
                .contains("shell strings are not supported")
        );

        let escaped_cwd = registry
            .execute(
                "run_command",
                ToolContext::new(root),
                json!({
                    "argv": [env::current_exe().unwrap().to_str().unwrap(), "--help"],
                    "cwd": outside.to_str().unwrap()
                }),
            )
            .unwrap_err();
        assert!(escaped_cwd.to_string().contains("escapes workspace"));
    }
}
