use anyhow::Result;

use crate::protocol::openai::OpenAiChatRequest;

pub struct UpstreamResponse {
    pub status: reqwest::StatusCode,
    pub body: serde_json::Value,
}

#[async_trait::async_trait]
pub trait BackendProvider: Send + Sync {
    async fn send_chat(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
    ) -> Result<UpstreamResponse>;
}
