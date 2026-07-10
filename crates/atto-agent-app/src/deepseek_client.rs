//! HTTP streaming client for DeepSeek chat completions.
//!
//! The client lives in the app crate so network dependencies do not leak into
//! reusable UI crates. Default tests use a local mock HTTP server; the real API
//! smoke test is ignored and must be run manually with `DEEPSEEK_API_KEY`.

use std::str::{self, Utf8Error};

use atto_ui_chat::{ChatError, ChatErrorKind};
use futures_util::StreamExt;

use crate::config::AgentConfig;
use crate::deepseek::{
    ChatCompletionMessage, ChatCompletionSseEvent, ChatCompletionSseParser,
    build_chat_completions_request, chat_error_from_http_status, chat_error_from_json_error,
    chat_error_from_network_failure, chat_error_from_stream_disconnect,
};

/// Result type returned by the DeepSeek HTTP client.
pub type DeepSeekClientResult<T> = std::result::Result<T, ChatError>;

/// Minimal DeepSeek HTTP client that streams SSE chat completion events.
#[derive(Clone, Debug)]
pub struct DeepSeekClient {
    http: reqwest::Client,
}

impl DeepSeekClient {
    /// Creates a client with reqwest's default async HTTP configuration.
    pub fn new() -> Self {
        Self::with_http_client(reqwest::Client::new())
    }

    /// Creates a client around a caller-provided reqwest client.
    pub fn with_http_client(http: reqwest::Client) -> Self {
        Self { http }
    }

    /// Posts a streaming chat completion request and collects parsed SSE events.
    pub async fn stream_chat_completions(
        &self,
        config: &AgentConfig,
        messages: Vec<ChatCompletionMessage>,
    ) -> DeepSeekClientResult<Vec<ChatCompletionSseEvent>> {
        let api_key = config.deepseek_api_key().map_err(|error| {
            ChatError::new(ChatErrorKind::Api, "DeepSeek API key is required.")
                .with_detail(error.to_string())
        })?;
        let request = build_chat_completions_request(config, messages).map_err(|error| {
            ChatError::new(ChatErrorKind::Api, "Failed to build DeepSeek request.")
                .with_detail(error.to_string())
        })?;

        let response = self
            .http
            .post(&request.url)
            .bearer_auth(api_key)
            .json(&request.body)
            .send()
            .await
            .map_err(|error| chat_error_from_network_failure(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|error| format!("failed to read response body: {error}"));
            return Err(chat_error_from_http_status(status.as_u16(), &body));
        }

        collect_sse_events(response).await
    }
}

impl Default for DeepSeekClient {
    fn default() -> Self {
        Self::new()
    }
}

async fn collect_sse_events(
    response: reqwest::Response,
) -> DeepSeekClientResult<Vec<ChatCompletionSseEvent>> {
    let mut parser = ChatCompletionSseParser::new();
    let mut events = Vec::new();
    let mut pending_bytes = Vec::new();
    let mut saw_done = false;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| chat_error_from_network_failure(error.to_string()))?;
        pending_bytes.extend_from_slice(&chunk);
        while let Some(text) = drain_valid_utf8(&mut pending_bytes)? {
            push_parser_text(&mut parser, &mut events, &mut saw_done, &text)?;
        }
    }

    if !pending_bytes.is_empty() {
        return Err(ChatError::new(
            ChatErrorKind::Api,
            "DeepSeek stream ended with an incomplete UTF-8 sequence.",
        ));
    }
    let remaining = parser
        .finish()
        .map_err(|error| chat_error_from_json_error(error, ""))?;
    for event in remaining {
        saw_done |= matches!(event, ChatCompletionSseEvent::Done);
        events.push(event);
    }

    if saw_done {
        Ok(events)
    } else {
        Err(chat_error_from_stream_disconnect())
    }
}

fn drain_valid_utf8(pending_bytes: &mut Vec<u8>) -> DeepSeekClientResult<Option<String>> {
    if pending_bytes.is_empty() {
        return Ok(None);
    }

    match str::from_utf8(pending_bytes) {
        Ok(text) => {
            let text = text.to_string();
            pending_bytes.clear();
            Ok(Some(text))
        }
        Err(error) if error.error_len().is_none() => {
            let valid_up_to = error.valid_up_to();
            if valid_up_to == 0 {
                return Ok(None);
            }
            let text = str::from_utf8(&pending_bytes[..valid_up_to])
                .expect("valid UTF-8 prefix reported by from_utf8")
                .to_string();
            let rest = pending_bytes.split_off(valid_up_to);
            *pending_bytes = rest;
            Ok(Some(text))
        }
        Err(error) => Err(chat_error_from_invalid_utf8(error)),
    }
}

fn push_parser_text(
    parser: &mut ChatCompletionSseParser,
    events: &mut Vec<ChatCompletionSseEvent>,
    saw_done: &mut bool,
    text: &str,
) -> DeepSeekClientResult<()> {
    let parsed = parser
        .push_str(text)
        .map_err(|error| chat_error_from_json_error(error, text))?;
    for event in parsed {
        *saw_done |= matches!(event, ChatCompletionSseEvent::Done);
        events.push(event);
    }
    Ok(())
}

fn chat_error_from_invalid_utf8(error: Utf8Error) -> ChatError {
    ChatError::new(
        ChatErrorKind::Api,
        "DeepSeek stream returned invalid UTF-8.",
    )
    .with_detail(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    use crate::config::AgentConfig;
    use crate::deepseek::{ChatCompletionMessage, FinishReason};

    use super::*;

    #[tokio::test]
    async fn streams_events_from_mock_http_server() {
        let sse = concat!(
            "data: {\"model\":\"mock-deepseek\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        let server = MockSseServer::spawn(sse);
        let mut config = AgentConfig::defaults(".");
        config.api_key = Some("test-key".to_string());
        config.base_url = server.base_url();
        config.model = "deepseek-chat".to_string();

        let events = DeepSeekClient::new()
            .stream_chat_completions(&config, vec![ChatCompletionMessage::user("hi")])
            .await
            .unwrap_or_else(|error| panic!("mock DeepSeek stream failed: {error:?}"));

        let request = server.join();
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(request.contains("authorization: Bearer test-key"));
        assert!(request.contains(r#""model":"deepseek-chat""#));
        assert!(request.contains(r#""stream":true"#));
        assert!(matches!(events.last(), Some(ChatCompletionSseEvent::Done)));
        let ChatCompletionSseEvent::Chunk(first) = &events[0] else {
            panic!("expected first mock event to be a chunk");
        };
        assert_eq!(first.model.as_deref(), Some("mock-deepseek"));
        assert_eq!(first.choices[0].delta.content.as_deref(), Some("hello"));
        let ChatCompletionSseEvent::Chunk(second) = &events[1] else {
            panic!("expected second mock event to be a chunk");
        };
        assert_eq!(second.choices[0].finish_reason, Some(FinishReason::Stop));
    }

    #[tokio::test]
    async fn maps_mock_http_status_to_chat_error() {
        let server = MockSseServer::spawn_response(
            401,
            "application/json",
            r#"{"error":{"message":"bad key","type":"invalid_request_error","code":"invalid_api_key","param":null}}"#,
        );
        let mut config = AgentConfig::defaults(".");
        config.api_key = Some("bad-key".to_string());
        config.base_url = server.base_url();

        let error = DeepSeekClient::new()
            .stream_chat_completions(&config, vec![ChatCompletionMessage::user("hi")])
            .await
            .expect_err("mock status failure should map to ChatError");

        let _request = server.join();
        assert_eq!(error.kind, ChatErrorKind::Api);
        assert!(error.message.contains("DEEPSEEK_API_KEY"));
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("invalid_api_key"))
        );
    }

    struct MockSseServer {
        address: String,
        handle: thread::JoinHandle<String>,
    }

    impl MockSseServer {
        fn spawn(sse: &'static str) -> Self {
            Self::spawn_response(200, "text/event-stream", sse)
        }

        fn spawn_response(status: u16, content_type: &'static str, body: &'static str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock SSE server");
            let address = listener.local_addr().expect("mock SSE server address");
            let handle = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept mock DeepSeek request");
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("set mock server read timeout");
                let request = read_http_request(&mut stream);
                write_http_response(&mut stream, status, content_type, body);
                request
            });
            Self {
                address: format!("http://{address}/v1"),
                handle,
            }
        }

        fn base_url(&self) -> String {
            self.address.clone()
        }

        fn join(self) -> String {
            self.handle.join().expect("mock SSE server thread panicked")
        }
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let read = stream.read(&mut buffer).expect("read mock HTTP request");
            assert_ne!(read, 0, "client closed before sending complete request");
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(header_end) = find_header_end(&bytes) {
                let content_length = content_length(&bytes[..header_end]);
                let body_len = bytes.len().saturating_sub(header_end + 4);
                if body_len >= content_length {
                    break;
                }
            }
        }
        String::from_utf8(bytes).expect("mock request should be UTF-8")
    }

    fn find_header_end(bytes: &[u8]) -> Option<usize> {
        bytes.windows(4).position(|window| window == b"\r\n\r\n")
    }

    fn content_length(headers: &[u8]) -> usize {
        let headers = String::from_utf8_lossy(headers);
        headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0)
    }

    fn write_http_response(stream: &mut TcpStream, status: u16, content_type: &str, body: &str) {
        let reason = if status == 200 { "OK" } else { "Error" };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write mock HTTP response");
    }
}
