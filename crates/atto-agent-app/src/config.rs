//! Configuration loading for the agent app.
//!
//! The loader keeps all DeepSeek and workspace configuration inside the app
//! crate so reusable UI crates do not gain network-oriented dependencies.

use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com/v1";
pub const DEFAULT_MODEL: &str = "deepseek-chat";
pub const DEFAULT_TEMPERATURE: f32 = 0.2;
pub const DEFAULT_MAX_TOKENS: u32 = 4096;

const WORKSPACE_CONFIG_FILE: &str = ".atto-agent.toml";
const USER_CONFIG_FILE: &str = ".config/atto-agent/config.toml";

/// Plan mode requested by configuration and slash commands.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PlanMode {
    Off,
    On,
    #[default]
    Auto,
}

impl PlanMode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::On,
            Self::On => Self::Auto,
            Self::Auto => Self::Off,
        }
    }

    pub fn status(self) -> String {
        format!("plan: {self}")
    }
}

impl fmt::Display for PlanMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Off => "off",
            Self::On => "on",
            Self::Auto => "auto",
        })
    }
}

impl FromStr for PlanMode {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "on" => Ok(Self::On),
            "auto" => Ok(Self::Auto),
            _ => bail!("invalid plan mode `{value}`; expected off, on, or auto"),
        }
    }
}

/// Fully resolved runtime configuration for the app layer.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub workspace: PathBuf,
    pub plan_mode: PlanMode,
}

impl AgentConfig {
    pub fn defaults(workspace: impl Into<PathBuf>) -> Self {
        Self {
            api_key: None,
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            temperature: DEFAULT_TEMPERATURE,
            max_tokens: DEFAULT_MAX_TOKENS,
            workspace: workspace.into(),
            plan_mode: PlanMode::Auto,
        }
    }

    pub fn load() -> Result<Self> {
        load_config_from_sources(ConfigLoadSources::from_process()?)
    }

    pub fn deepseek_api_key(&self) -> Result<&str> {
        self.api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
            .context("DEEPSEEK_API_KEY is required for DeepSeek requests")
    }
}

/// Process-independent inputs used to make configuration precedence testable.
#[derive(Clone, Debug)]
pub struct ConfigLoadSources {
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub current_dir: PathBuf,
    pub home_dir: Option<PathBuf>,
}

impl ConfigLoadSources {
    pub fn from_process() -> Result<Self> {
        Ok(Self {
            args: env::args().skip(1).collect(),
            env: env::vars().collect(),
            current_dir: env::current_dir().context("failed to read current directory")?,
            home_dir: env::var_os("HOME").map(PathBuf::from),
        })
    }
}

pub fn load_config_from_sources(sources: ConfigLoadSources) -> Result<AgentConfig> {
    let cli = parse_cli_overrides(&sources.args)?;
    let env_overrides = parse_env_overrides(&sources.env)?;
    let mut builder = ConfigBuilder::default();

    if let Some(home_dir) = sources.home_dir.as_ref() {
        let user_path = home_dir.join(USER_CONFIG_FILE);
        if let Some(overrides) = read_toml_overrides(&user_path, false)? {
            builder.apply(overrides);
        }
    }

    let workspace_config_path = workspace_config_path(&sources.current_dir, &env_overrides, &cli)?;
    if let Some(overrides) = read_toml_overrides(&workspace_config_path, cli.config_path.is_some())?
    {
        builder.apply(overrides);
    }

    builder.apply(env_overrides);
    builder.apply(cli.overrides);
    builder.finish(&sources.current_dir)
}

#[derive(Clone, Debug, Default)]
struct CliConfig {
    overrides: ConfigOverrides,
    config_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Default)]
struct ConfigOverrides {
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    workspace: Option<PathBuf>,
    plan_mode: Option<PlanMode>,
}

#[derive(Clone, Debug, Deserialize)]
struct TomlConfig {
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    workspace: Option<PathBuf>,
    plan_mode: Option<PlanMode>,
}

#[derive(Clone, Debug, Default)]
struct ConfigBuilder {
    overrides: ConfigOverrides,
}

impl ConfigBuilder {
    fn apply(&mut self, overrides: ConfigOverrides) {
        if overrides.api_key.is_some() {
            self.overrides.api_key = overrides.api_key;
        }
        if overrides.base_url.is_some() {
            self.overrides.base_url = overrides.base_url;
        }
        if overrides.model.is_some() {
            self.overrides.model = overrides.model;
        }
        if overrides.temperature.is_some() {
            self.overrides.temperature = overrides.temperature;
        }
        if overrides.max_tokens.is_some() {
            self.overrides.max_tokens = overrides.max_tokens;
        }
        if overrides.workspace.is_some() {
            self.overrides.workspace = overrides.workspace;
        }
        if overrides.plan_mode.is_some() {
            self.overrides.plan_mode = overrides.plan_mode;
        }
    }

    fn finish(self, current_dir: &Path) -> Result<AgentConfig> {
        let workspace = self
            .overrides
            .workspace
            .unwrap_or_else(|| current_dir.to_path_buf());
        let workspace = resolve_workspace(current_dir, &workspace)?;
        let temperature = self.overrides.temperature.unwrap_or(DEFAULT_TEMPERATURE);
        let max_tokens = self.overrides.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
        validate_temperature(temperature)?;
        validate_max_tokens(max_tokens)?;

        Ok(AgentConfig {
            api_key: self.overrides.api_key.filter(|value| !value.is_empty()),
            base_url: self
                .overrides
                .base_url
                .unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model: self
                .overrides
                .model
                .unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            temperature,
            max_tokens,
            workspace,
            plan_mode: self.overrides.plan_mode.unwrap_or_default(),
        })
    }
}

impl From<TomlConfig> for ConfigOverrides {
    fn from(config: TomlConfig) -> Self {
        Self {
            api_key: config.api_key,
            base_url: config.base_url,
            model: config.model,
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            workspace: config.workspace,
            plan_mode: config.plan_mode,
        }
    }
}

fn parse_cli_overrides(args: &[String]) -> Result<CliConfig> {
    let mut cli = CliConfig::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--mock" {
            index += 1;
            continue;
        }

        let (flag, inline_value) = match arg.split_once('=') {
            Some((flag, value)) => (flag, Some(value.to_string())),
            None => (arg.as_str(), None),
        };
        match flag {
            "--api-key" | "--deepseek-api-key" => {
                cli.overrides.api_key = Some(cli_value(flag, inline_value, args, &mut index)?);
            }
            "--base-url" => {
                cli.overrides.base_url = Some(cli_value(flag, inline_value, args, &mut index)?);
            }
            "--model" => {
                cli.overrides.model = Some(cli_value(flag, inline_value, args, &mut index)?);
            }
            "--temperature" => {
                let value = cli_value(flag, inline_value, args, &mut index)?;
                cli.overrides.temperature = Some(parse_temperature("--temperature", &value)?);
            }
            "--max-tokens" => {
                let value = cli_value(flag, inline_value, args, &mut index)?;
                cli.overrides.max_tokens = Some(parse_max_tokens("--max-tokens", &value)?);
            }
            "--workspace" => {
                cli.overrides.workspace = Some(PathBuf::from(cli_value(
                    flag,
                    inline_value,
                    args,
                    &mut index,
                )?));
            }
            "--plan-mode" | "--plan" => {
                let value = cli_value(flag, inline_value, args, &mut index)?;
                cli.overrides.plan_mode = Some(value.parse()?);
            }
            "--config" => {
                cli.config_path = Some(PathBuf::from(cli_value(
                    flag,
                    inline_value,
                    args,
                    &mut index,
                )?));
            }
            _ => bail!("unknown argument `{arg}`"),
        }
        index += 1;
    }
    Ok(cli)
}

fn cli_value(
    flag: &str,
    inline_value: Option<String>,
    args: &[String],
    index: &mut usize,
) -> Result<String> {
    if let Some(value) = inline_value {
        ensure_non_empty(flag, value)
    } else {
        *index += 1;
        let value = args
            .get(*index)
            .with_context(|| format!("missing value for `{flag}`"))?
            .clone();
        ensure_non_empty(flag, value)
    }
}

fn parse_env_overrides(env: &BTreeMap<String, String>) -> Result<ConfigOverrides> {
    let mut overrides = ConfigOverrides {
        api_key: env_value(env, "DEEPSEEK_API_KEY"),
        base_url: env_value(env, "DEEPSEEK_BASE_URL"),
        model: env_value(env, "DEEPSEEK_MODEL"),
        workspace: env_value(env, "ATTO_AGENT_WORKSPACE").map(PathBuf::from),
        ..Default::default()
    };
    if let Some(value) = env_value(env, "DEEPSEEK_TEMPERATURE") {
        overrides.temperature = Some(parse_temperature("DEEPSEEK_TEMPERATURE", &value)?);
    }
    if let Some(value) = env_value(env, "DEEPSEEK_MAX_TOKENS") {
        overrides.max_tokens = Some(parse_max_tokens("DEEPSEEK_MAX_TOKENS", &value)?);
    }
    if let Some(value) = env_value(env, "ATTO_AGENT_PLAN_MODE") {
        overrides.plan_mode = Some(value.parse()?);
    }
    Ok(overrides)
}

fn env_value(env: &BTreeMap<String, String>, key: &str) -> Option<String> {
    env.get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn workspace_config_path(
    current_dir: &Path,
    env_overrides: &ConfigOverrides,
    cli: &CliConfig,
) -> Result<PathBuf> {
    if let Some(config_path) = cli.config_path.as_ref() {
        return Ok(resolve_relative_path(current_dir, config_path));
    }

    let workspace = cli
        .overrides
        .workspace
        .as_ref()
        .or(env_overrides.workspace.as_ref())
        .map(|path| resolve_relative_path(current_dir, path))
        .unwrap_or_else(|| current_dir.to_path_buf());
    Ok(workspace.join(WORKSPACE_CONFIG_FILE))
}

fn read_toml_overrides(path: &Path, required: bool) -> Result<Option<ConfigOverrides>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read `{}`", path.display()));
        }
    };
    let config = toml::from_str::<TomlConfig>(&text)
        .with_context(|| format!("failed to parse `{}`", path.display()))?;
    Ok(Some(config.into()))
}

fn resolve_workspace(current_dir: &Path, workspace: &Path) -> Result<PathBuf> {
    let path = resolve_relative_path(current_dir, workspace);
    path.canonicalize()
        .with_context(|| format!("workspace `{}` must exist", path.display()))
}

fn resolve_relative_path(current_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    }
}

fn parse_temperature(source: &str, value: &str) -> Result<f32> {
    let temperature = value
        .parse::<f32>()
        .with_context(|| format!("invalid {source} `{value}`"))?;
    validate_temperature(temperature)?;
    Ok(temperature)
}

fn validate_temperature(value: f32) -> Result<()> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        bail!("temperature must be a finite non-negative number")
    }
}

fn parse_max_tokens(source: &str, value: &str) -> Result<u32> {
    let max_tokens = value
        .parse::<u32>()
        .with_context(|| format!("invalid {source} `{value}`"))?;
    validate_max_tokens(max_tokens)?;
    Ok(max_tokens)
}

fn validate_max_tokens(value: u32) -> Result<()> {
    if value > 0 {
        Ok(())
    } else {
        bail!("max_tokens must be greater than zero")
    }
}

fn ensure_non_empty(flag: &str, value: String) -> Result<String> {
    if value.trim().is_empty() {
        bail!("empty value for `{flag}`")
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        let dir = env::temp_dir().join(format!(
            "atto-agent-config-{name}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("failed to create test dir");
        dir
    }

    fn sources(
        current_dir: &Path,
        home_dir: &Path,
        args: &[&str],
        env_pairs: &[(&str, &str)],
    ) -> ConfigLoadSources {
        ConfigLoadSources {
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            env: env_pairs
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
            current_dir: current_dir.to_path_buf(),
            home_dir: Some(home_dir.to_path_buf()),
        }
    }

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create parent dir");
        }
        fs::write(path, text).expect("failed to write test fixture");
    }

    #[test]
    fn loads_defaults_without_optional_files() {
        let current = test_dir("defaults-current");
        let home = test_dir("defaults-home");

        let config = load_config_from_sources(sources(&current, &home, &[], &[])).unwrap();

        assert_eq!(config.api_key, None);
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
        assert_eq!(config.model, DEFAULT_MODEL);
        assert_eq!(config.temperature, DEFAULT_TEMPERATURE);
        assert_eq!(config.max_tokens, DEFAULT_MAX_TOKENS);
        assert_eq!(config.workspace, current.canonicalize().unwrap());
        assert_eq!(config.plan_mode, PlanMode::Auto);
    }

    #[test]
    fn applies_user_workspace_env_and_cli_precedence() {
        let current = test_dir("precedence-current");
        let home = test_dir("precedence-home");
        let workspace = current.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        write(
            &home.join(USER_CONFIG_FILE),
            r#"
api_key = "user-key"
base_url = "https://user.example/v1"
model = "user-model"
temperature = 0.1
max_tokens = 111
plan_mode = "off"
"#,
        );
        write(
            &workspace.join(WORKSPACE_CONFIG_FILE),
            r#"
base_url = "https://workspace.example/v1"
model = "workspace-model"
temperature = 0.2
max_tokens = 222
plan_mode = "on"
"#,
        );

        let config = load_config_from_sources(sources(
            &current,
            &home,
            &[
                "--api-key",
                "cli-key",
                "--model=cli-model",
                "--max-tokens",
                "444",
                "--plan-mode",
                "off",
                "--mock",
            ],
            &[
                ("ATTO_AGENT_WORKSPACE", workspace.to_str().unwrap()),
                ("DEEPSEEK_API_KEY", "env-key"),
                ("DEEPSEEK_MODEL", "env-model"),
                ("DEEPSEEK_TEMPERATURE", "0.3"),
                ("ATTO_AGENT_PLAN_MODE", "auto"),
            ],
        ))
        .unwrap();

        assert_eq!(config.api_key.as_deref(), Some("cli-key"));
        assert_eq!(config.base_url, "https://workspace.example/v1");
        assert_eq!(config.model, "cli-model");
        assert_eq!(config.temperature, 0.3);
        assert_eq!(config.max_tokens, 444);
        assert_eq!(config.workspace, workspace.canonicalize().unwrap());
        assert_eq!(config.plan_mode, PlanMode::Off);
    }

    #[test]
    fn explicit_config_path_replaces_workspace_config_discovery() {
        let current = test_dir("explicit-current");
        let home = test_dir("explicit-home");
        let workspace = current.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        write(
            &workspace.join(WORKSPACE_CONFIG_FILE),
            r#"model = "workspace-model""#,
        );
        let explicit = current.join("agent.toml");
        write(
            &explicit,
            r#"
model = "explicit-model"
plan_mode = "on"
"#,
        );

        let config = load_config_from_sources(sources(
            &current,
            &home,
            &[
                "--workspace",
                workspace.to_str().unwrap(),
                "--config",
                explicit.to_str().unwrap(),
            ],
            &[],
        ))
        .unwrap();

        assert_eq!(config.model, "explicit-model");
        assert_eq!(config.workspace, workspace.canonicalize().unwrap());
        assert_eq!(config.plan_mode, PlanMode::On);
    }

    #[test]
    fn rejects_invalid_env_values() {
        let current = test_dir("invalid-current");
        let home = test_dir("invalid-home");

        let error = load_config_from_sources(sources(
            &current,
            &home,
            &[],
            &[("ATTO_AGENT_PLAN_MODE", "sometimes")],
        ))
        .unwrap_err();

        assert!(error.to_string().contains("invalid plan mode"));
    }

    #[test]
    fn rejects_unknown_cli_arguments() {
        let error = parse_cli_overrides(&["--surprise".to_string()]).unwrap_err();

        assert!(error.to_string().contains("unknown argument"));
    }
}
