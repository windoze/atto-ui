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
            ("textDocument/codeAction", Some(id)) => {
                let uri = msg
                    .get("params")
                    .and_then(|params| params.get("textDocument"))
                    .and_then(|text_document| text_document.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
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
                let result = json!([
                    {
                        "title": "Replace bad with good",
                        "kind": "quickfix",
                        "isPreferred": true,
                        "edit": {
                            "changes": Value::Object(changes)
                        }
                    }
                ]);
                respond(&mut stdout, id, result)?;
            }
            ("workspace/executeCommand", Some(id)) => {
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
