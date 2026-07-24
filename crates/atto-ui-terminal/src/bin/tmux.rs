//! Minimal `tmux` command shim backed by the atto-ui IPC socket.
//!
//! This binary is intentionally a client-side translator: it parses common tmux
//! subcommands and sends the existing atto-ui pane protocol methods. It does
//! not implement tmux's server protocol or control mode.

use std::collections::VecDeque;
use std::env;
use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result, anyhow, bail, ensure};
use atto_ui::ComponentError;
use atto_ui::ipc::{IPC_SOCKET_ENV, send_protocol_request};
use atto_ui::protocol::{
    PaneInfo, PaneSelectDirection, PaneSplitDirection, ProtocolRequest, ProtocolResponse,
    ProtocolResult,
};
use atto_ui::runtime::Rect;

fn main() {
    let code = match run(env::args().collect()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("tmux: {error}");
            1
        }
    };
    process::exit(code);
}

fn run(args: Vec<String>) -> Result<i32> {
    match parse_args(args)? {
        ParsedCommand::Help => {
            println!("{}", usage());
            Ok(0)
        }
        ParsedCommand::Request(command) => {
            let response = send_protocol_request(&command.socket_path, &command.request)
                .with_context(|| {
                    format!(
                        "failed to send IPC request to {}",
                        command.socket_path.display()
                    )
                })?;
            print_response(response, command.output)
        }
    }
}

#[derive(Debug)]
enum ParsedCommand {
    Help,
    Request(ShimCommand),
}

#[derive(Debug)]
struct ShimCommand {
    socket_path: PathBuf,
    request: ProtocolRequest,
    output: OutputMode,
}

#[derive(Debug)]
enum OutputMode {
    Quiet,
    CapturePane,
    ListPanes { format: Option<String> },
}

fn parse_args(args: Vec<String>) -> Result<ParsedCommand> {
    let mut iter = args.into_iter();
    let _program = iter.next();
    let mut socket_override = None;
    let mut command_name = None;

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(ParsedCommand::Help),
            "-S" => {
                let value = iter
                    .next()
                    .ok_or_else(|| usage_error("-S requires a socket path"))?;
                socket_override = Some(PathBuf::from(value));
            }
            "-CC" => bail!("control mode (-CC) is not supported by the atto-ui tmux shim"),
            "--" => {
                command_name = Some(
                    iter.next()
                        .ok_or_else(|| usage_error("missing tmux subcommand after --"))?,
                );
                break;
            }
            _ if arg.starts_with('-') => bail!("unsupported tmux global option {arg}"),
            _ => {
                command_name = Some(arg);
                break;
            }
        }
    }

    let Some(command_name) = command_name else {
        return Err(usage_error("missing tmux subcommand"));
    };
    let args = iter.collect::<Vec<_>>();
    let (request, output) = parse_subcommand(&command_name, args)?;
    let socket_path = socket_override
        .or_else(socket_from_tmux_env)
        .or_else(|| env::var_os(IPC_SOCKET_ENV).map(PathBuf::from))
        .ok_or_else(|| {
            usage_error(format!(
                "missing socket path; set TMUX, set {IPC_SOCKET_ENV}, or pass -S PATH"
            ))
        })?;
    ensure!(
        !socket_path.as_os_str().is_empty(),
        "tmux socket path must not be empty"
    );

    Ok(ParsedCommand::Request(ShimCommand {
        socket_path,
        request,
        output,
    }))
}

fn parse_subcommand(name: &str, args: Vec<String>) -> Result<(ProtocolRequest, OutputMode)> {
    match normalize_subcommand(name).as_str() {
        "send-keys" => parse_send_keys(args),
        "capture-pane" => parse_capture_pane(args),
        "list-panes" => parse_list_panes(args),
        "split-window" => parse_split_window(args),
        "select-pane" => parse_select_pane(args),
        "break-pane" => parse_break_pane(args),
        "display-popup" => parse_display_popup(args),
        other => bail!("unsupported tmux subcommand {other}"),
    }
}

fn normalize_subcommand(name: &str) -> String {
    name.replace('_', "-").to_ascii_lowercase()
}

fn parse_send_keys(args: Vec<String>) -> Result<(ProtocolRequest, OutputMode)> {
    let mut args = VecDeque::from(args);
    let mut target = None;
    let mut literal = false;
    let mut repeat = 1usize;
    let mut keys = Vec::new();

    while let Some(arg) = args.pop_front() {
        match arg.as_str() {
            "-t" => target = Some(parse_target_arg(args.pop_front(), "send-keys -t")?),
            "-l" => literal = true,
            "-N" => {
                let value = args
                    .pop_front()
                    .ok_or_else(|| usage_error("send-keys -N requires a repeat count"))?;
                repeat = value
                    .parse::<usize>()
                    .with_context(|| format!("invalid send-keys repeat count {value:?}"))?;
                ensure!(repeat > 0, "send-keys repeat count must be non-zero");
            }
            _ if arg.starts_with('-') => bail!("unsupported send-keys option {arg}"),
            _ => {
                keys.push(arg);
                keys.extend(args);
                break;
            }
        }
    }

    ensure!(!keys.is_empty(), "send-keys requires at least one key");
    let pane_id = target.unwrap_or(default_pane_id()?);
    let mut bytes = Vec::new();
    for _ in 0..repeat {
        for key in &keys {
            append_key_bytes(key, literal, &mut bytes)?;
        }
    }
    Ok((
        ProtocolRequest::send_keys("tmux-send-keys", pane_id, bytes),
        OutputMode::Quiet,
    ))
}

fn parse_capture_pane(args: Vec<String>) -> Result<(ProtocolRequest, OutputMode)> {
    let mut args = VecDeque::from(args);
    let mut target = None;

    while let Some(arg) = args.pop_front() {
        match arg.as_str() {
            "-t" => target = Some(parse_target_arg(args.pop_front(), "capture-pane -t")?),
            "-p" | "-e" | "-J" | "-C" => {}
            "-S" | "-E" => {
                args.pop_front()
                    .ok_or_else(|| usage_error(format!("capture-pane {arg} requires a value")))?;
            }
            _ if arg.starts_with('-') => bail!("unsupported capture-pane option {arg}"),
            _ => bail!("capture-pane does not accept positional argument {arg:?}"),
        }
    }

    let pane_id = target.unwrap_or(default_pane_id()?);
    Ok((
        ProtocolRequest::capture_pane("tmux-capture-pane", pane_id),
        OutputMode::CapturePane,
    ))
}

fn parse_list_panes(args: Vec<String>) -> Result<(ProtocolRequest, OutputMode)> {
    let mut args = VecDeque::from(args);
    let mut format = None;

    while let Some(arg) = args.pop_front() {
        match arg.as_str() {
            "-F" => {
                format = Some(
                    args.pop_front()
                        .ok_or_else(|| usage_error("list-panes -F requires a format string"))?,
                );
            }
            "-a" => {}
            "-t" => {
                args.pop_front()
                    .ok_or_else(|| usage_error("list-panes -t requires a target"))?;
            }
            _ if arg.starts_with('-') => bail!("unsupported list-panes option {arg}"),
            _ => bail!("list-panes does not accept positional argument {arg:?}"),
        }
    }

    Ok((
        ProtocolRequest::list_panes("tmux-list-panes"),
        OutputMode::ListPanes { format },
    ))
}

fn parse_split_window(args: Vec<String>) -> Result<(ProtocolRequest, OutputMode)> {
    let mut args = VecDeque::from(args);
    let mut target = None;
    let mut direction = PaneSplitDirection::Horizontal;

    while let Some(arg) = args.pop_front() {
        match arg.as_str() {
            "-t" => target = Some(parse_target_arg(args.pop_front(), "split-window -t")?),
            "-h" => direction = PaneSplitDirection::Vertical,
            "-v" => direction = PaneSplitDirection::Horizontal,
            "-d" => {}
            _ if arg.starts_with('-') => bail!("unsupported split-window option {arg}"),
            _ => bail!(
                "split-window command argv is not supported by the atto-ui tmux shim: {arg:?}"
            ),
        }
    }

    Ok((
        ProtocolRequest::split_window("tmux-split-window", target, direction),
        OutputMode::Quiet,
    ))
}

fn parse_select_pane(args: Vec<String>) -> Result<(ProtocolRequest, OutputMode)> {
    let mut args = VecDeque::from(args);
    let mut target = None;
    let mut direction = None;

    while let Some(arg) = args.pop_front() {
        match arg.as_str() {
            "-t" => target = Some(parse_target_arg(args.pop_front(), "select-pane -t")?),
            "-L" => direction = Some(PaneSelectDirection::Left),
            "-R" => direction = Some(PaneSelectDirection::Right),
            "-U" => direction = Some(PaneSelectDirection::Up),
            "-D" => direction = Some(PaneSelectDirection::Down),
            _ if arg.starts_with('-') => bail!("unsupported select-pane option {arg}"),
            _ => bail!("select-pane does not accept positional argument {arg:?}"),
        }
    }

    let direction =
        direction.ok_or_else(|| usage_error("select-pane requires -L, -R, -U, or -D"))?;
    Ok((
        ProtocolRequest::select_pane("tmux-select-pane", target, direction),
        OutputMode::Quiet,
    ))
}

fn parse_break_pane(args: Vec<String>) -> Result<(ProtocolRequest, OutputMode)> {
    let mut args = VecDeque::from(args);
    let mut target = None;

    while let Some(arg) = args.pop_front() {
        match arg.as_str() {
            "-t" => target = Some(parse_target_arg(args.pop_front(), "break-pane -t")?),
            "-d" => {}
            _ if arg.starts_with('-') => bail!("unsupported break-pane option {arg}"),
            _ => bail!("break-pane does not accept positional argument {arg:?}"),
        }
    }

    let pane_id = target.unwrap_or(default_pane_id()?);
    Ok((
        ProtocolRequest::break_pane("tmux-break-pane", pane_id),
        OutputMode::Quiet,
    ))
}

fn parse_display_popup(args: Vec<String>) -> Result<(ProtocolRequest, OutputMode)> {
    let mut args = VecDeque::from(args);
    let mut title = None;
    let mut x = None;
    let mut y = None;
    let mut width = None;
    let mut height = None;
    let mut command = Vec::new();

    while let Some(arg) = args.pop_front() {
        match arg.as_str() {
            "--" => {
                command.extend(args);
                break;
            }
            "-E" => {}
            "-T" => {
                title = Some(
                    args.pop_front()
                        .ok_or_else(|| usage_error("display-popup -T requires a title"))?,
                );
            }
            "-x" => x = Some(parse_u16_arg(args.pop_front(), "display-popup -x")?),
            "-y" => y = Some(parse_u16_arg(args.pop_front(), "display-popup -y")?),
            "-w" => width = Some(parse_u16_arg(args.pop_front(), "display-popup -w")?),
            "-h" => height = Some(parse_u16_arg(args.pop_front(), "display-popup -h")?),
            "-d" => {
                args.pop_front()
                    .ok_or_else(|| usage_error("display-popup -d requires a path"))?;
            }
            _ if arg.starts_with('-') => bail!("unsupported display-popup option {arg}"),
            _ => {
                command.push(arg);
                command.extend(args);
                break;
            }
        }
    }

    let rect = match (x, y, width, height) {
        (None, None, None, None) => None,
        (Some(x), Some(y), Some(width), Some(height)) => Some(Rect {
            x,
            y,
            width,
            height,
        }),
        _ => bail!("display-popup requires -x, -y, -w, and -h when any rectangle option is set"),
    };
    let command = (!command.is_empty()).then_some(command);
    Ok((
        ProtocolRequest::display_popup("tmux-display-popup", title, rect, command),
        OutputMode::Quiet,
    ))
}

fn parse_target_arg(value: Option<String>, option: &str) -> Result<u64> {
    let value = value.ok_or_else(|| usage_error(format!("{option} requires a pane target")))?;
    parse_pane_id(&value)
}

fn parse_u16_arg(value: Option<String>, option: &str) -> Result<u16> {
    let value = value.ok_or_else(|| usage_error(format!("{option} requires a value")))?;
    value
        .parse::<u16>()
        .with_context(|| format!("invalid {option} value {value:?}"))
}

fn parse_pane_id(value: &str) -> Result<u64> {
    let pane = value.strip_prefix('%').unwrap_or(value);
    pane.parse::<u64>()
        .with_context(|| format!("unsupported pane target {value:?}; expected %<id>"))
}

fn default_pane_id() -> Result<u64> {
    let pane = env::var("TMUX_PANE")
        .map_err(|_| usage_error("missing target pane; pass -t %<id> or set TMUX_PANE"))?;
    parse_pane_id(&pane)
}

fn append_key_bytes(key: &str, literal: bool, output: &mut Vec<u8>) -> Result<()> {
    if literal {
        output.extend_from_slice(key.as_bytes());
        return Ok(());
    }

    match key {
        // tmux `Enter` and `C-m` send carriage return (0x0D), not line feed;
        // shells and line-editing REPLs in raw mode submit on CR. `C-j` is the
        // one that sends LF.
        "Enter" | "ENTER" | "C-m" | "C-M" => output.push(b'\r'),
        "C-j" | "C-J" => output.push(b'\n'),
        "Space" | "SPACE" => output.push(b' '),
        // Ctrl+Space / Ctrl+@ send NUL (0x00).
        "C-Space" | "C-SPACE" | "C-@" => output.push(0x00),
        "Tab" | "TAB" | "C-i" | "C-I" => output.push(b'\t'),
        "Escape" | "Esc" | "ESC" | "C-[" => output.push(0x1b),
        "BSpace" | "Backspace" | "BACKSPACE" => output.push(0x7f),
        value if value.len() == 3 && value.starts_with("C-") => {
            // Control combinations are only defined for ASCII letters and the
            // `@ [ \ ] ^ _` symbols (which map to control codes 0x00, 0x1B..0x1F).
            // Anything else (e.g. `C-1`) has no control byte and would otherwise
            // produce a nonsense value from `& 0x1f`.
            let byte = value.as_bytes()[2].to_ascii_uppercase();
            ensure!(
                byte.is_ascii_uppercase()
                    || matches!(byte, b'@' | b'[' | b'\\' | b']' | b'^' | b'_'),
                "unsupported control key {value:?}; expected C-<letter> or C-@[\\]^_"
            );
            output.push(byte & 0x1f);
        }
        value => output.extend_from_slice(value.as_bytes()),
    }
    Ok(())
}

fn socket_from_tmux_env() -> Option<PathBuf> {
    let value = env::var_os("TMUX")?;
    let value = value.to_string_lossy();
    let (socket, _) = value.split_once(',')?;
    (!socket.is_empty()).then(|| PathBuf::from(socket))
}

fn print_response(response: ProtocolResponse, output: OutputMode) -> Result<i32> {
    if let Some(error) = response.error {
        eprintln!("tmux: {}", format_component_error(&error));
        return Ok(1);
    }

    let result = response
        .result
        .ok_or_else(|| anyhow!("protocol response contained neither result nor error"))?;
    match (result, output) {
        (ProtocolResult::SendKeys(_), OutputMode::Quiet)
        | (ProtocolResult::SplitWindow(_), OutputMode::Quiet)
        | (ProtocolResult::SelectPane(_), OutputMode::Quiet)
        | (ProtocolResult::BreakPane(_), OutputMode::Quiet)
        | (ProtocolResult::DisplayPopup(_), OutputMode::Quiet) => {}
        (ProtocolResult::CapturePane(capture), OutputMode::CapturePane) => {
            println!("{}", capture.text());
        }
        (ProtocolResult::ListPanes(panes), OutputMode::ListPanes { format }) => {
            print_panes(&panes, format.as_deref());
        }
        (result, _) => bail!("unexpected protocol result for tmux shim: {result:?}"),
    }
    Ok(0)
}

fn print_panes(panes: &[PaneInfo], format: Option<&str>) {
    for pane in panes {
        if let Some(format) = format {
            println!("{}", format_pane(format, pane));
        } else {
            let rect = pane
                .rect
                .map(|rect| format!("{},{},{},{}", rect.x, rect.y, rect.width, rect.height))
                .unwrap_or_else(|| "-".to_string());
            println!(
                "%{} index={} active={} rect={}",
                pane.pane_id,
                pane.index,
                usize::from(pane.is_active),
                rect
            );
        }
    }
}

fn format_pane(format: &str, pane: &PaneInfo) -> String {
    let mut output = format.replace("#{pane_id}", &format!("%{}", pane.pane_id));
    output = output.replace("#{pane_index}", &pane.index.to_string());
    output = output.replace("#{pane_active}", if pane.is_active { "1" } else { "0" });
    if let Some(rect) = pane.rect {
        output = output.replace("#{pane_left}", &rect.x.to_string());
        output = output.replace("#{pane_top}", &rect.y.to_string());
        output = output.replace("#{pane_width}", &rect.width.to_string());
        output = output.replace("#{pane_height}", &rect.height.to_string());
    }
    output
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
    "Usage: tmux [-S SOCKET] <subcommand> [args]\n\
\n\
Supported subcommands:\n\
  send-keys [-t %PANE] [-l] [-N COUNT] <key>...\n\
  capture-pane [-p] [-t %PANE]\n\
  list-panes [-F FORMAT]\n\
  split-window [-h|-v] [-t %PANE]\n\
  select-pane [-L|-R|-U|-D] [-t %PANE]\n\
  break-pane [-t %PANE]\n\
  display-popup [-T TITLE] [-x X -y Y -w W -h H] [--] [command ...]\n\
\n\
Socket defaults to the first field of TMUX, then ATTO_UI_SOCKET. Control mode (-CC) is not supported."
}
