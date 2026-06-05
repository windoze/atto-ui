use std::io::{self, BufReader};

use serde_json::{Value, json};

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
            // Notifications.
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
