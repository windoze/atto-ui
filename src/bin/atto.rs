//! External CLI client for the atto-ui scripting IPC socket.

use std::env;
use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result, anyhow};
use atto_ui::ipc::{IPC_SOCKET_ENV, send_protocol_request};
use atto_ui::protocol::{ProtocolRequest, ProtocolResponse, ProtocolResult};
use atto_ui::runtime::Rect;
use atto_ui::{ComponentCommand, ComponentError, ComponentTarget, ComponentValue};

fn main() {
    let code = match run(env::args().collect()) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err}");
            2
        }
    };
    process::exit(code);
}

fn run(args: Vec<String>) -> Result<i32> {
    let options = parse_args(args)?;
    match options.command {
        CliCommand::Help => {
            println!("{}", usage());
            Ok(0)
        }
        CliCommand::Request(request) => {
            let response =
                send_protocol_request(&options.socket_path, &request).with_context(|| {
                    format!(
                        "failed to send IPC request to {}",
                        options.socket_path.display()
                    )
                })?;
            print_response(response, options.json)
        }
    }
}

#[derive(Debug)]
struct CliOptions {
    socket_path: PathBuf,
    json: bool,
    command: CliCommand,
}

#[derive(Debug)]
enum CliCommand {
    Help,
    Request(ProtocolRequest),
}

fn parse_args(args: Vec<String>) -> Result<CliOptions> {
    let mut iter = args.into_iter();
    let _program = iter.next();

    let mut socket_path = env::var_os(IPC_SOCKET_ENV).map(PathBuf::from);
    let mut json = false;
    let mut screen = default_screen();
    let mut command_name = None;

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                return Ok(CliOptions {
                    socket_path: socket_path.unwrap_or_default(),
                    json,
                    command: CliCommand::Help,
                });
            }
            "--json" => json = true,
            "--socket" => {
                let value = iter
                    .next()
                    .ok_or_else(|| usage_error("--socket requires a path"))?;
                socket_path = Some(PathBuf::from(value));
            }
            "--screen" => {
                let value = iter
                    .next()
                    .ok_or_else(|| usage_error("--screen requires WIDTHxHEIGHT or X,Y,W,H"))?;
                screen = parse_screen(&value)?;
            }
            _ if arg.starts_with("--socket=") => {
                socket_path = Some(PathBuf::from(&arg["--socket=".len()..]));
            }
            _ if arg.starts_with("--screen=") => {
                screen = parse_screen(&arg["--screen=".len()..])?;
            }
            _ if arg.starts_with('-') => return Err(usage_error(format!("unknown option {arg}"))),
            _ => {
                command_name = Some(arg);
                break;
            }
        }
    }

    let Some(command_name) = command_name else {
        return Err(usage_error("missing command"));
    };
    let remaining = iter.collect::<Vec<_>>();
    let command = parse_command(command_name, remaining, screen)?;
    let Some(socket_path) = socket_path else {
        return Err(usage_error(format!(
            "missing socket path; pass --socket PATH or set {IPC_SOCKET_ENV}"
        )));
    };
    if socket_path.as_os_str().is_empty() {
        return Err(usage_error("socket path must not be empty"));
    }

    Ok(CliOptions {
        socket_path,
        json,
        command,
    })
}

fn parse_command(name: String, args: Vec<String>, screen: Rect) -> Result<CliCommand> {
    match name.as_str() {
        "query" => {
            if args.len() != 2 {
                return Err(usage_error("query requires <tag> <prop>"));
            }
            Ok(CliCommand::Request(ProtocolRequest::query(
                "cli-query",
                ComponentTarget::Id(args[0].clone()),
                args[1].clone(),
            )))
        }
        "invoke" => {
            if args.len() < 2 {
                return Err(usage_error("invoke requires <tag> <action>"));
            }
            let tag = args[0].clone();
            let action = parse_action(&args[1], &args[2..])?;
            Ok(CliCommand::Request(ProtocolRequest::invoke(
                "cli-invoke",
                screen,
                ComponentTarget::Id(tag),
                action,
            )))
        }
        "tree" => {
            if !args.is_empty() {
                return Err(usage_error("tree does not accept positional arguments"));
            }
            Ok(CliCommand::Request(ProtocolRequest::tree(
                "cli-tree", screen,
            )))
        }
        other => Err(usage_error(format!("unknown command {other}"))),
    }
}

fn parse_action(name: &str, args: &[String]) -> Result<ComponentCommand> {
    let (verb, inline_value) = split_action_value(name);
    let verb = verb.replace('_', "-").to_ascii_lowercase();
    match verb.as_str() {
        "click" => no_action_args(&verb, args).map(|()| ComponentCommand::Click),
        "toggle" => no_action_args(&verb, args).map(|()| ComponentCommand::Toggle),
        "submit" => no_action_args(&verb, args).map(|()| ComponentCommand::Submit),
        "input-text" => {
            let text = action_text_value("input-text", inline_value, args)?;
            Ok(ComponentCommand::InputText(text))
        }
        "select-index" => {
            let value = action_text_value("select-index", inline_value, args)?;
            let index = value
                .parse::<usize>()
                .with_context(|| format!("invalid select-index value {value:?}"))?;
            Ok(ComponentCommand::SelectIndex(index))
        }
        "custom" => {
            let custom_name = match inline_value {
                Some(value) if !value.is_empty() => value.to_string(),
                _ => args
                    .first()
                    .cloned()
                    .ok_or_else(|| usage_error("custom requires a name"))?,
            };
            let payload_start = usize::from(inline_value.is_none());
            let payload = args
                .get(payload_start..)
                .unwrap_or_default()
                .join(" ")
                .into_bytes();
            Ok(ComponentCommand::Custom {
                name: custom_name,
                payload,
            })
        }
        other => Err(usage_error(format!("unknown action {other}"))),
    }
}

fn split_action_value(value: &str) -> (&str, Option<&str>) {
    value
        .split_once('=')
        .or_else(|| value.split_once(':'))
        .map_or((value, None), |(name, payload)| (name, Some(payload)))
}

fn no_action_args(action: &str, args: &[String]) -> Result<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(usage_error(format!(
            "{action} does not accept extra arguments"
        )))
    }
}

fn action_text_value(action: &str, inline_value: Option<&str>, args: &[String]) -> Result<String> {
    match (inline_value, args.is_empty()) {
        (Some(value), true) => Ok(value.to_string()),
        (None, false) => Ok(args.join(" ")),
        (Some(value), false) => Ok(format!("{value} {}", args.join(" "))),
        (None, true) => Err(usage_error(format!("{action} requires a value"))),
    }
}

fn parse_screen(value: &str) -> Result<Rect> {
    let rect = if let Some((width, height)) = value.split_once('x') {
        Rect {
            x: 0,
            y: 0,
            width: parse_u16("width", width)?,
            height: parse_u16("height", height)?,
        }
    } else {
        let parts = value.split(',').collect::<Vec<_>>();
        if parts.len() != 4 {
            return Err(usage_error("--screen requires WIDTHxHEIGHT or X,Y,W,H"));
        }
        Rect {
            x: parse_u16("x", parts[0])?,
            y: parse_u16("y", parts[1])?,
            width: parse_u16("width", parts[2])?,
            height: parse_u16("height", parts[3])?,
        }
    };
    if rect.width == 0 || rect.height == 0 {
        return Err(usage_error("screen width and height must be non-zero"));
    }
    Ok(rect)
}

fn parse_u16(name: &str, value: &str) -> Result<u16> {
    value
        .parse::<u16>()
        .with_context(|| format!("invalid {name} value {value:?}"))
}

fn default_screen() -> Rect {
    Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    }
}

fn print_response(response: ProtocolResponse, json: bool) -> Result<i32> {
    if json {
        println!("{}", serde_json::to_string_pretty(&response)?);
    }

    if let Some(error) = response.error {
        if !json {
            eprintln!("error: {}", format_component_error(&error));
        }
        return Ok(1);
    }

    let result = response
        .result
        .ok_or_else(|| anyhow!("protocol response contained neither result nor error"))?;
    if !json {
        print_human_result(result)?;
    }
    Ok(0)
}

fn print_human_result(result: ProtocolResult) -> Result<()> {
    match result {
        ProtocolResult::Query(value) => {
            println!("{}", format_component_value(&value));
        }
        ProtocolResult::Invoke(result) => {
            println!(
                "dispatch={:?} outcome={:?} action={:?} capture={:?}",
                result.dispatch, result.result.outcome, result.result.action, result.result.capture
            );
        }
        ProtocolResult::WaitFor(result) => {
            let value = result
                .value
                .as_ref()
                .map(format_component_value)
                .unwrap_or_else(|| "null".to_string());
            println!("polls={} value={}", result.polls, value);
        }
        ProtocolResult::Tree(snapshot) => {
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        ProtocolResult::PropertyNames(names) => {
            for name in names {
                println!("{name}");
            }
        }
    }
    Ok(())
}

fn format_component_value(value: &ComponentValue) -> String {
    match value {
        ComponentValue::Null => "null".to_string(),
        ComponentValue::Bool(value) => value.to_string(),
        ComponentValue::I64(value) => value.to_string(),
        ComponentValue::U64(value) => value.to_string(),
        ComponentValue::F64(value) => value.to_string(),
        ComponentValue::String(value) => value.clone(),
        ComponentValue::StringList(values) => values.join("\n"),
        _ => serde_json::to_string_pretty(value).unwrap_or_else(|_| format!("{value:?}")),
    }
}

fn format_component_error(error: &ComponentError) -> String {
    match error {
        ComponentError::NotFound(id) => format!("not found: {id}"),
        ComponentError::UnsupportedProperty(name) => format!("unsupported property: {name}"),
        ComponentError::InvalidValue { name, expected } => {
            format!("invalid value for {name}: expected {expected}")
        }
        ComponentError::ActionNotSupported(action) => format!("action not supported: {action}"),
        ComponentError::RenderFailed(message) => format!("render failed: {message}"),
        ComponentError::Timeout(message) => format!("timeout: {message}"),
    }
}

fn usage_error(message: impl Into<String>) -> anyhow::Error {
    anyhow!("{}\n\n{}", message.into(), usage())
}

fn usage() -> &'static str {
    "Usage: atto [--socket PATH] [--json] [--screen WIDTHxHEIGHT|X,Y,W,H] <command> [args]\n\
\n\
Commands:\n\
  query <tag> <prop>                 Read a component property\n\
  invoke <tag> <action> [value]      Invoke click, toggle, submit, input-text, select-index, custom\n\
  tree                               Export the current desktop snapshot\n\
\n\
Socket defaults to ATTO_UI_SOCKET when --socket is omitted."
}
