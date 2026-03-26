use anyhow::Result;
use reqwest::Client;

use crate::backend::provider::{BackendProvider, UpstreamResponse};
use crate::protocol::openai::OpenAiChatRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiBackendConfig {
    pub base_url: String,
    pub chat_completions_path: String,
}

#[derive(Debug, Clone)]
pub struct OpenAiBackendProvider {
    client: Client,
    config: OpenAiBackendConfig,
}

impl OpenAiBackendProvider {
    pub fn new(config: OpenAiBackendConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }
}

#[async_trait::async_trait]
impl BackendProvider for OpenAiBackendProvider {
    async fn send_chat(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
    ) -> Result<UpstreamResponse> {
        let response = self
            .client
            .post(format!(
                "{}{}",
                self.config.base_url, self.config.chat_completions_path
            ))
            .bearer_auth(access_token)
            .json(request)
            .send()
            .await?;
        let status = response.status();
        let body = response.json().await?;
        Ok(UpstreamResponse { status, body })
    }
}
