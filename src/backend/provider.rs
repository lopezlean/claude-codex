use anyhow::Result;
use futures_core::Stream;

use crate::models::EffortLevel;
use crate::protocol::openai::OpenAiChatRequest;

pub struct UpstreamResponse {
    pub status: reqwest::StatusCode,
    pub body: serde_json::Value,
}

pub type UpstreamStream = std::pin::Pin<Box<dyn Stream<Item = Result<bytes::Bytes>> + Send>>;

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
    ) -> Result<UpstreamStream>;
}
