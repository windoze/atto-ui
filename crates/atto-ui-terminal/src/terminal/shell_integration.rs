//! Shell-integration (OSC 133/7) startup: bash/zsh snippets, temp-file
//! lifecycle, tmux-shim PATH injection, and spawn-command preparation.

use super::*;

pub(crate) const BASH_SHELL_INTEGRATION_SCRIPT: &str = r#"# atto-ui OSC 133/7 shell integration for bash.
if [ -z "${ATTO_UI_SHELL_INTEGRATION_NO_USER_RC:-}" ] && [ -r "${HOME:-}/.bashrc" ]; then
  . "${HOME}/.bashrc"
fi

__atto_ui_emit_cwd() {
  printf '\033]7;file://%s%s\a' "${HOSTNAME:-localhost}" "${PWD}"
}

__atto_ui_precmd() {
  local __atto_ui_status=$?
  if [ "${__atto_ui_prompt_seen:-0}" = 1 ]; then
    printf '\033]133;D;%s\a' "${__atto_ui_status}"
  fi
  __atto_ui_prompt_seen=1
  __atto_ui_emit_cwd
  printf '\033]133;A\a'
  # Mark command-start (OSC 133 B) at the end of the prompt so it lands right
  # before the user's typed command, not on the output line. Appended
  # idempotently (this runs last in PROMPT_COMMAND) so prompt rebuilds keep it.
  case "$PS1" in
    *$'\[\033]133;B\a\]'*) ;;
    *) PS1="${PS1}"$'\[\033]133;B\a\]' ;;
  esac
}

# Output-start (OSC 133 C) is emitted after the user submits the command.
PS0=$'\[\033]133;C\a\]'
if [ -n "${PROMPT_COMMAND:-}" ]; then
  PROMPT_COMMAND="${PROMPT_COMMAND}; __atto_ui_precmd"
else
  PROMPT_COMMAND="__atto_ui_precmd"
fi
"#;

pub(crate) const ZSH_SHELL_INTEGRATION_SCRIPT: &str = r#"# atto-ui OSC 133/7 shell integration for zsh.
if [ -z "${ATTO_UI_SHELL_INTEGRATION_NO_USER_RC:-}" ] && [ -n "${ATTO_UI_ORIGINAL_ZDOTDIR:-}" ] && [ -r "${ATTO_UI_ORIGINAL_ZDOTDIR}/.zshrc" ]; then
  . "${ATTO_UI_ORIGINAL_ZDOTDIR}/.zshrc"
fi

__atto_ui_emit_cwd() {
  printf '\033]7;file://%s%s\a' "${HOST:-${HOSTNAME:-localhost}}" "${PWD}"
}

__atto_ui_precmd() {
  local __atto_ui_status=$?
  if [[ "${__atto_ui_prompt_seen:-0}" == 1 ]]; then
    printf '\033]133;D;%d\a' "${__atto_ui_status}"
  fi
  __atto_ui_prompt_seen=1
  __atto_ui_emit_cwd
  printf '\033]133;A\a'
  # Mark command-start (OSC 133 B) at the very end of the prompt so it lands
  # right before the user's typed command, not on the output line. Appended
  # idempotently after any user/framework precmd (this hook is registered last)
  # so prompt rebuilds keep the mark.
  case "$PROMPT" in
    *$'\033]133;B\a'*) ;;
    *) PROMPT="${PROMPT}"$'%{\033]133;B\a%}' ;;
  esac
}

__atto_ui_preexec() {
  printf '\033]133;C\a'
}

autoload -Uz add-zsh-hook
add-zsh-hook precmd __atto_ui_precmd
add-zsh-hook preexec __atto_ui_preexec
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShellIntegrationKind {
    Bash,
    Zsh,
}

#[derive(Debug)]
pub(crate) struct TerminalShellIntegrationFiles {
    pub(crate) root: PathBuf,
    pub(crate) entrypoint: PathBuf,
}

impl TerminalShellIntegrationFiles {
    pub(crate) fn create(kind: ShellIntegrationKind) -> Result<Self> {
        let root = create_shell_integration_temp_dir()?;
        let (entrypoint, script) = match kind {
            ShellIntegrationKind::Bash => (root.join("bashrc"), BASH_SHELL_INTEGRATION_SCRIPT),
            ShellIntegrationKind::Zsh => (root.join(".zshrc"), ZSH_SHELL_INTEGRATION_SCRIPT),
        };
        fs::write(&entrypoint, script)?;
        Ok(Self { root, entrypoint })
    }

    pub(crate) fn entrypoint(&self) -> &Path {
        &self.entrypoint
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for TerminalShellIntegrationFiles {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.entrypoint);
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn create_shell_integration_temp_dir() -> Result<PathBuf> {
    for _ in 0..16 {
        let id = SHELL_INTEGRATION_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            env::temp_dir().join(format!("atto-ui-shell-integration-{}-{id}", process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    bail!("failed to allocate a unique shell integration temp directory")
}

pub(crate) fn prepare_spawn_command(
    cmd: &mut CommandBuilder,
    tmux_environment: &TerminalTmuxEnvironmentConfig,
) -> Result<()> {
    tmux_environment.validate()?;
    let term = if tmux_environment.inject && tmux_environment.override_term {
        TMUX_TERM_ENV
    } else {
        DEFAULT_TERM_ENV
    };

    cmd.env("TERM", term);
    cmd.env("COLORTERM", DEFAULT_COLORTERM_ENV);
    if tmux_environment.inject {
        cmd.env("TMUX", tmux_environment.tmux_env_value());
        cmd.env("TMUX_PANE", tmux_environment.tmux_pane_env_value());
        prepend_tmux_shim_to_path(cmd, tmux_environment)?;
    }
    if cmd.get_cwd().is_none() {
        let cwd = env::current_dir()?;
        cmd.cwd(cwd.as_os_str());
    }
    Ok(())
}

pub(crate) fn prepend_tmux_shim_to_path(
    cmd: &mut CommandBuilder,
    tmux_environment: &TerminalTmuxEnvironmentConfig,
) -> Result<()> {
    let shim_path = tmux_environment
        .shim_path
        .as_ref()
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(|| {
            let current_exe = env::current_exe()?;
            current_exe
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| anyhow!("failed to resolve current executable directory"))
        })?;
    let mut paths = vec![shim_path];
    if let Some(existing_path) = cmd.get_env("PATH") {
        paths.extend(env::split_paths(existing_path).filter(|path| !path.as_os_str().is_empty()));
    }
    cmd.env("PATH", env::join_paths(paths)?);
    Ok(())
}

pub(crate) fn prepare_shell_integration(
    cmd: &mut CommandBuilder,
    integration: TerminalShellIntegration,
) -> Result<Option<TerminalShellIntegrationFiles>> {
    if !integration.is_enabled() {
        return Ok(None);
    }

    let argv = cmd.get_argv();
    let Some(program) = argv.first() else {
        return Ok(None);
    };
    let Some(kind) = shell_integration_kind(program) else {
        return Ok(None);
    };
    if !shell_integration_accepts_args(argv) {
        return Ok(None);
    }

    let files = TerminalShellIntegrationFiles::create(kind)?;
    match kind {
        ShellIntegrationKind::Bash => {
            let program = cmd
                .get_argv()
                .first()
                .cloned()
                .expect("program exists for bash shell integration");
            let argv = cmd.get_argv_mut();
            argv.clear();
            argv.push(program);
            argv.push(OsString::from("--rcfile"));
            argv.push(files.entrypoint().as_os_str().to_os_string());
            argv.push(OsString::from("-i"));
        }
        ShellIntegrationKind::Zsh => {
            if let Some(original_zdotdir) = cmd
                .get_env("ZDOTDIR")
                .map(OsStr::to_os_string)
                .or_else(|| env::var_os("ZDOTDIR"))
                .or_else(|| env::var_os("HOME"))
            {
                cmd.env("ATTO_UI_ORIGINAL_ZDOTDIR", original_zdotdir);
            }
            cmd.env("ZDOTDIR", files.root().as_os_str());
            ensure_interactive_shell_arg(cmd);
        }
    }
    cmd.env("ATTO_UI_SHELL_INTEGRATION", "1");
    Ok(Some(files))
}

pub(crate) fn shell_integration_kind(program: &OsStr) -> Option<ShellIntegrationKind> {
    let name = Path::new(program).file_name()?.to_string_lossy();
    let name = name.strip_prefix('-').unwrap_or(&name);
    match name {
        "bash" => Some(ShellIntegrationKind::Bash),
        "zsh" => Some(ShellIntegrationKind::Zsh),
        _ => None,
    }
}

pub(crate) fn shell_integration_accepts_args(argv: &[OsString]) -> bool {
    match &argv[1..] {
        [] => true,
        [arg] => arg.as_os_str() == OsStr::new("-i"),
        _ => false,
    }
}

pub(crate) fn ensure_interactive_shell_arg(cmd: &mut CommandBuilder) {
    if cmd
        .get_argv()
        .iter()
        .skip(1)
        .any(|arg| arg.as_os_str() == OsStr::new("-i"))
    {
        return;
    }
    cmd.arg("-i");
}
