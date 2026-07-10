#![forbid(unsafe_code)]

use atto_agent_app::config::{AgentConfig, DEFAULT_BASE_URL, DEFAULT_MODEL};
use atto_agent_app::deepseek::{ChatCompletionMessage, ChatCompletionSseEvent};
use atto_agent_app::deepseek_client::DeepSeekClient;

#[tokio::test]
#[ignore = "requires DEEPSEEK_API_KEY and external DeepSeek network access"]
async fn deepseek_real_streaming_smoke() {
    let api_key = std::env::var("DEEPSEEK_API_KEY")
        .expect("set DEEPSEEK_API_KEY to run the real DeepSeek smoke test");
    let mut config = AgentConfig::defaults(std::env::current_dir().expect("read current dir"));
    config.api_key = Some(api_key);
    config.base_url =
        std::env::var("DEEPSEEK_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into());
    config.model = std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
    config.temperature = 0.0;
    config.max_tokens = 64;

    let events = DeepSeekClient::new()
        .stream_chat_completions(
            &config,
            vec![
                ChatCompletionMessage::system("Reply briefly for a smoke test."),
                ChatCompletionMessage::user("Say: atto smoke ok"),
            ],
        )
        .await
        .unwrap_or_else(|error| panic!("real DeepSeek streaming smoke failed: {error:?}"));

    let mut saw_done = false;
    let mut streamed_text = String::new();
    for event in events {
        match event {
            ChatCompletionSseEvent::Chunk(chunk) => {
                for choice in chunk.choices {
                    if let Some(reasoning) = choice.delta.reasoning_content {
                        streamed_text.push_str(&reasoning);
                    }
                    if let Some(content) = choice.delta.content {
                        streamed_text.push_str(&content);
                    }
                }
            }
            ChatCompletionSseEvent::Done => saw_done = true,
            ChatCompletionSseEvent::Error(error) => {
                panic!("DeepSeek returned an SSE error event: {error:?}");
            }
        }
    }

    assert!(saw_done, "real stream must end with [DONE]");
    assert!(
        !streamed_text.trim().is_empty(),
        "real stream should contain content or reasoning deltas"
    );
}
