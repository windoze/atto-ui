use std::io::{self, BufReader};

use serde_json::{Map, Value, json};

fn respond<W: io::Write>(writer: &mut W, id: u64, result: Value) -> io::Result<()> {
    editor_core_lsp::write_lsp_message(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }),
    )
}

fn respond_error<W: io::Write>(
    writer: &mut W,
    id: u64,
    code: i64,
    message: &str,
) -> io::Result<()> {
    editor_core_lsp::write_lsp_message(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message },
        }),
    )
}

fn publish_mock_diagnostics<W: io::Write>(
    writer: &mut W,
    uri: &str,
    version: Option<i64>,
) -> io::Result<()> {
    editor_core_lsp::write_lsp_message(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": uri,
                "version": version,
                "diagnostics": [
                    {
                        "range": {
                            "start": { "line": 0, "character": 4 },
                            "end": { "line": 0, "character": 8 }
                        },
                        "severity": 1,
                        "source": "atto-ui-mock-lsp",
                        "message": "mock error"
                    }
                ]
            }
        }),
    )
}

fn publish_command_executed_diagnostic<W: io::Write>(writer: &mut W, uri: &str) -> io::Result<()> {
    editor_core_lsp::write_lsp_message(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": uri,
                "diagnostics": [
                    {
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 0, "character": 3 }
                        },
                        "severity": 3,
                        "source": "atto-ui-mock-lsp",
                        "message": "command executed"
                    }
                ]
            }
        }),
    )
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut stdout = io::stdout();

    while let Some(msg) = editor_core_lsp::read_lsp_message(&mut reader)? {
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            continue;
        };

        let id = msg.get("id").and_then(Value::as_u64);
        match (method, id) {
            ("initialize", Some(id)) => {
                // Minimal capabilities for exercising `editor_core_lsp::LspSession`:
                // - semantic tokens (full)
                // - folding ranges
                // - hover
                let result = json!({
                    "serverInfo": { "name": "atto-ui-mock-lsp", "version": "0.1.0" },
                    "capabilities": {
                        "hoverProvider": true,
                        "signatureHelpProvider": {
                            "triggerCharacters": ["(", ","]
                        },
                        "semanticTokensProvider": {
                            "legend": {
                                "tokenTypes": [
                                    "string",
                                    "comment",
                                    "keyword",
                                    "function",
                                    "type"
                                ],
                                "tokenModifiers": []
                            },
                            "full": true
                        },
                        "foldingRangeProvider": true,
                        "codeActionProvider": true,
                        "documentFormattingProvider": true,
                        "renameProvider": {
                            "prepareProvider": true
                        },
                        "executeCommandProvider": {
                            "commands": ["atto-ui.mock.command"]
                        },
                    }
                });
                respond(&mut stdout, id, result)?;
            }
            ("shutdown", Some(id)) => {
                respond(&mut stdout, id, Value::Null)?;
            }
            ("textDocument/semanticTokens/full", Some(id)) => {
                // Return a single `string` token for `hello` in:
                // `fn main() {\n    let s = "hello";\n}\n`
                //
                // Encoding: (deltaLine, deltaStart, length, tokenType, tokenModifiers).
                // - line 1 (second line), col 13 (0-based), length 5, tokenType 0 ("string").
                let result = json!({
                    "data": [1, 13, 5, 0, 0],
                });
                respond(&mut stdout, id, result)?;
            }
            ("textDocument/foldingRange", Some(id)) => {
                // Fold the function body: lines 0..=2.
                let result = json!([
                    { "startLine": 0, "endLine": 2, "kind": "region" }
                ]);
                respond(&mut stdout, id, result)?;
            }
            ("textDocument/hover", Some(id)) => {
                // Deterministic hover response used by PTY tests.
                let result = json!({
                    "contents": { "kind": "plaintext", "value": "HOVER" }
                });
                respond(&mut stdout, id, result)?;
            }
            ("textDocument/signatureHelp", Some(id)) => {
                let uri = msg
                    .get("params")
                    .and_then(|params| params.get("textDocument"))
                    .and_then(|text_document| text_document.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if uri.ends_with("/signature_empty.rs") || uri == "file:///signature_empty.rs" {
                    respond(&mut stdout, id, Value::Null)?;
                } else {
                    let result = json!({
                        "signatures": [
                            {
                                "label": "mock_fn(arg: i32, next: i32)",
                                "parameters": [
                                    { "label": [8, 16] },
                                    { "label": "next: i32" }
                                ]
                            }
                        ],
                        "activeSignature": 0,
                        "activeParameter": 0
                    });
                    respond(&mut stdout, id, result)?;
                }
            }
            ("textDocument/codeAction", Some(id)) => {
                let uri = msg
                    .get("params")
                    .and_then(|params| params.get("textDocument"))
                    .and_then(|text_document| text_document.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let result = if uri.ends_with("/code_action_command.rs")
                    || uri == "file:///code_action_command.rs"
                {
                    json!([
                        {
                            "title": "Run mock command",
                            "kind": "quickfix",
                            "command": {
                                "title": "Run mock command",
                                "command": "atto-ui.mock.command",
                                "arguments": [{ "uri": uri }]
                            }
                        }
                    ])
                } else {
                    let target_uri = if uri.ends_with("/code_action_cross.rs")
                        || uri == "file:///code_action_cross.rs"
                    {
                        "file:///other.rs"
                    } else {
                        uri
                    };
                    let mut changes = Map::new();
                    changes.insert(
                        target_uri.to_string(),
                        json!([
                            {
                                "range": {
                                    "start": { "line": 0, "character": 4 },
                                    "end": { "line": 0, "character": 7 }
                                },
                                "newText": "good"
                            }
                        ]),
                    );
                    json!([
                        {
                            "title": "Replace bad with good",
                            "kind": "quickfix",
                            "isPreferred": true,
                            "edit": {
                                "changes": Value::Object(changes)
                            }
                        }
                    ])
                };
                respond(&mut stdout, id, result)?;
            }
            ("textDocument/formatting", Some(id)) => {
                let params = msg.get("params");
                let uri = params
                    .and_then(|params| params.get("textDocument"))
                    .and_then(|text_document| text_document.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if uri.ends_with("/formatting_disconnect.rs")
                    || uri == "file:///formatting_disconnect.rs"
                {
                    return Ok(());
                } else if uri.ends_with("/formatting_error.rs")
                    || uri == "file:///formatting_error.rs"
                {
                    respond_error(&mut stdout, id, -32000, "mock formatting error")?;
                } else if uri.ends_with("/formatting_empty.rs")
                    || uri == "file:///formatting_empty.rs"
                {
                    respond(&mut stdout, id, json!([]))?;
                } else if uri.ends_with("/formatting_options.rs")
                    || uri == "file:///formatting_options.rs"
                {
                    let options = params.and_then(|params| params.get("options"));
                    let tab_size = options
                        .and_then(|options| options.get("tabSize"))
                        .and_then(Value::as_u64)
                        .unwrap_or_default();
                    let insert_spaces = options
                        .and_then(|options| options.get("insertSpaces"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    respond(
                        &mut stdout,
                        id,
                        json!([
                            {
                                "range": {
                                    "start": { "line": 0, "character": 0 },
                                    "end": { "line": 1, "character": 0 }
                                },
                                "newText": format!("tabSize={tab_size} insertSpaces={insert_spaces}\n")
                            }
                        ]),
                    )?;
                } else if uri.ends_with("/formatting_multi.rs")
                    || uri == "file:///formatting_multi.rs"
                {
                    respond(
                        &mut stdout,
                        id,
                        json!([
                            {
                                "range": {
                                    "start": { "line": 0, "character": 4 },
                                    "end": { "line": 0, "character": 7 }
                                },
                                "newText": "good"
                            },
                            {
                                "range": {
                                    "start": { "line": 1, "character": 4 },
                                    "end": { "line": 1, "character": 9 }
                                },
                                "newText": "better"
                            }
                        ]),
                    )?;
                } else {
                    respond(
                        &mut stdout,
                        id,
                        json!([
                            {
                                "range": {
                                    "start": { "line": 0, "character": 0 },
                                    "end": { "line": 1, "character": 0 }
                                },
                                "newText": "formatted\n"
                            }
                        ]),
                    )?;
                }
            }
            ("textDocument/prepareRename", Some(id)) => {
                let uri = msg
                    .get("params")
                    .and_then(|params| params.get("textDocument"))
                    .and_then(|text_document| text_document.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if uri.ends_with("/rename_error.rs") || uri == "file:///rename_error.rs" {
                    respond_error(&mut stdout, id, -32000, "mock prepare rename error")?;
                } else if uri.ends_with("/rename_unavailable.rs")
                    || uri == "file:///rename_unavailable.rs"
                {
                    respond(&mut stdout, id, Value::Null)?;
                } else {
                    respond(
                        &mut stdout,
                        id,
                        json!({
                            "range": {
                                "start": { "line": 0, "character": 4 },
                                "end": { "line": 0, "character": 7 }
                            },
                            "placeholder": "bad"
                        }),
                    )?;
                }
            }
            ("textDocument/rename", Some(id)) => {
                let params = msg.get("params");
                let uri = params
                    .and_then(|params| params.get("textDocument"))
                    .and_then(|text_document| text_document.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let new_name = params
                    .and_then(|params| params.get("newName"))
                    .and_then(Value::as_str)
                    .unwrap_or("renamed");

                let mut changes = Map::new();
                changes.insert(
                    uri.to_string(),
                    json!([
                        {
                            "range": {
                                "start": { "line": 0, "character": 4 },
                                "end": { "line": 0, "character": 7 }
                            },
                            "newText": new_name
                        }
                    ]),
                );

                if uri.ends_with("/rename_cross.rs") {
                    let sibling =
                        uri.trim_end_matches("/rename_cross.rs").to_string() + "/rename_other.rs";
                    changes.insert(
                        sibling,
                        json!([
                            {
                                "range": {
                                    "start": { "line": 0, "character": 4 },
                                    "end": { "line": 0, "character": 7 }
                                },
                                "newText": new_name
                            }
                        ]),
                    );
                }

                respond(
                    &mut stdout,
                    id,
                    json!({ "changes": Value::Object(changes) }),
                )?;
            }
            ("workspace/executeCommand", Some(id)) => {
                let command = msg
                    .get("params")
                    .and_then(|params| params.get("command"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if command == "atto-ui.mock.command" {
                    let uri = msg
                        .get("params")
                        .and_then(|params| params.get("arguments"))
                        .and_then(Value::as_array)
                        .and_then(|args| args.first())
                        .and_then(|arg| arg.get("uri"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if !uri.is_empty() {
                        publish_command_executed_diagnostic(&mut stdout, uri)?;
                    }
                }
                respond(&mut stdout, id, Value::Null)?;
            }
            // Notifications.
            ("textDocument/didOpen", None) => {
                let Some(text_document) = msg
                    .get("params")
                    .and_then(|params| params.get("textDocument"))
                else {
                    continue;
                };
                let Some(uri) = text_document.get("uri").and_then(Value::as_str) else {
                    continue;
                };
                if uri.ends_with("/diagnostics.rs") || uri == "file:///diagnostics.rs" {
                    let version = text_document.get("version").and_then(Value::as_i64);
                    publish_mock_diagnostics(&mut stdout, uri, version)?;
                }
            }
            ("exit", None) => break,
            (_, None) => {
                // Ignore unknown notifications.
            }
            (unknown, Some(id)) => {
                respond_error(
                    &mut stdout,
                    id,
                    -32601,
                    &format!("Unknown method: {unknown}"),
                )?;
            }
        }
    }

    Ok(())
}
