use anyhow::Result;
use futures_core::Stream;

use crate::models::EffortLevel;
use crate::protocol::openai::OpenAiChatRequest;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpstreamHeaders {
    pub entries: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexPromptMetrics {
    pub estimated_prompt_tokens_before: usize,
    pub estimated_prompt_tokens_after: usize,
    pub trimmed_message_count: usize,
    pub dropped_message_count: usize,
    pub trimmed_text_messages: usize,
    pub trimmed_tool_result_messages: usize,
}

pub struct UpstreamResponse {
    pub status: reqwest::StatusCode,
    pub body: serde_json::Value,
    pub headers: UpstreamHeaders,
}

pub type UpstreamStream = std::pin::Pin<Box<dyn Stream<Item = Result<bytes::Bytes>> + Send>>;

pub struct UpstreamStreamResponse {
    pub stream: UpstreamStream,
    pub headers: UpstreamHeaders,
}

#[async_trait::async_trait]
pub trait BackendProvider: Send + Sync {
    async fn send_chat(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
        effort: EffortLevel,
    ) -> Result<UpstreamResponse>;

    async fn send_chat_stream(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
        effort: EffortLevel,
    ) -> Result<UpstreamStreamResponse>;
}
