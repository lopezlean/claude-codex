use std::time::Duration;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::Value;

use crate::backend::provider::{BackendProvider, UpstreamResponse, UpstreamStream};
use crate::models::EffortLevel;
use crate::protocol::codex::{build_codex_request, CodexSseToOpenAiBridge};
use crate::protocol::openai::OpenAiChatRequest;

const DEFAULT_CODEX_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
const RESPONSES_BETA_HEADER: &str = "responses=experimental";
const RESPONSES_ORIGINATOR: &str = "pi";
const RESPONSES_USER_AGENT: &str = "claude-codex (Rust)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiBackendConfig {
    pub base_url: String,
    pub chat_completions_path: String,
    pub codex_responses_url: String,
}

#[derive(Debug, Clone)]
pub struct OpenAiBackendProvider {
    client: Client,
    config: OpenAiBackendConfig,
}

impl OpenAiBackendProvider {
    pub fn new(config: OpenAiBackendConfig) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("OpenAI backend HTTP client should build"),
            config,
        }
    }

    fn should_use_codex(access_token: &str) -> bool {
        access_token.starts_with("ey")
    }

    async fn send_chat_completions(
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
        let body = response
            .json()
            .await
            .context("failed to decode chat completions response body")?;
        Ok(UpstreamResponse { status, body })
    }

    async fn send_codex_non_stream(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
        effort: EffortLevel,
    ) -> Result<UpstreamResponse> {
        let response = self
            .build_codex_request(access_token, request, effort)
            .send()
            .await
            .context("failed to call the Codex Responses API")?;
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read Codex error body".to_string());
            anyhow::bail!("Codex Responses API error ({}): {}", status, body);
        }

        let mut bridge = CodexSseToOpenAiBridge::default();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.context("failed to read Codex SSE chunk")?;
            let _ = bridge.push_bytes(&bytes)?;
        }

        Ok(UpstreamResponse {
            status,
            body: bridge.into_chat_response(),
        })
    }

    async fn send_codex_stream(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
        effort: EffortLevel,
    ) -> Result<UpstreamStream> {
        let response = self
            .build_codex_request(access_token, request, effort)
            .send()
            .await
            .context("failed to call the Codex Responses API")?
            .error_for_status()
            .context("Codex Responses API returned a non-success status")?;
        let mut bridge = CodexSseToOpenAiBridge::default();
        Ok(Box::pin(response.bytes_stream().map(move |chunk| {
            let bytes = chunk.map_err(anyhow::Error::from)?;
            let translated = bridge.push_bytes(&bytes)?;
            Ok(bytes::Bytes::from(translated))
        })))
    }

    fn build_codex_request(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
        effort: EffortLevel,
    ) -> reqwest::RequestBuilder {
        let mut builder = self
            .client
            .post(self.codex_responses_url())
            .bearer_auth(access_token)
            .header("OpenAI-Beta", RESPONSES_BETA_HEADER)
            .header("originator", RESPONSES_ORIGINATOR)
            .header("User-Agent", RESPONSES_USER_AGENT)
            .header("accept", "text/event-stream")
            .json(&build_codex_request(request, effort.into()));

        if let Some(account_id) = extract_account_id(access_token) {
            builder = builder.header("chatgpt-account-id", account_id);
        }

        builder
    }

    fn codex_responses_url(&self) -> &str {
        if self.config.codex_responses_url.is_empty() {
            DEFAULT_CODEX_RESPONSES_URL
        } else {
            &self.config.codex_responses_url
        }
    }
}

#[async_trait::async_trait]
impl BackendProvider for OpenAiBackendProvider {
    async fn send_chat(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
        effort: EffortLevel,
    ) -> Result<UpstreamResponse> {
        if Self::should_use_codex(access_token) {
            self.send_codex_non_stream(access_token, request, effort)
                .await
        } else {
            self.send_chat_completions(access_token, request).await
        }
    }

    async fn send_chat_stream(
        &self,
        access_token: &str,
        request: &OpenAiChatRequest,
        effort: EffortLevel,
    ) -> Result<UpstreamStream> {
        if Self::should_use_codex(access_token) {
            self.send_codex_stream(access_token, request, effort).await
        } else {
            let response = self
                .client
                .post(format!(
                    "{}{}",
                    self.config.base_url, self.config.chat_completions_path
                ))
                .bearer_auth(access_token)
                .json(request)
                .send()
                .await?
                .error_for_status()?;
            Ok(Box::pin(
                response
                    .bytes_stream()
                    .map(|chunk| chunk.map_err(anyhow::Error::from)),
            ))
        }
    }
}

impl From<EffortLevel> for crate::protocol::codex::CodexEffortLevel {
    fn from(value: EffortLevel) -> Self {
        match value {
            EffortLevel::Low => Self::Low,
            EffortLevel::Medium => Self::Medium,
            EffortLevel::High => Self::High,
        }
    }
}

fn extract_account_id(access_token: &str) -> Option<String> {
    let payload_b64 = access_token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let json: Value = serde_json::from_slice(&decoded).ok()?;
    json.get("https://api.openai.com/auth")?
        .get("chatgpt_account_id")?
        .as_str()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{OpenAiBackendConfig, OpenAiBackendProvider};
    use crate::backend::provider::BackendProvider;
    use crate::models::EffortLevel;
    use crate::protocol::openai::{
        OpenAiChatMessage, OpenAiChatRequest, OpenAiToolDefinition, OpenAiToolFunction,
    };
    use crate::test_support::lock_network_test;

    #[tokio::test]
    async fn oauth_tokens_use_codex_responses_api() {
        let _guard = lock_network_test();
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/backend-api/codex/responses"))
            .and(header("OpenAI-Beta", "responses=experimental"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                "event: response.output_text.delta\ndata: {\"delta\":\"Hello from Codex\"}\n\n\
                 event: response.completed\ndata: {\"type\":\"response.completed\"}\n\n",
                "text/event-stream",
            ))
            .mount(&upstream)
            .await;

        let provider = OpenAiBackendProvider::new(OpenAiBackendConfig {
            base_url: upstream.uri(),
            chat_completions_path: "/v1/chat/completions".to_string(),
            codex_responses_url: format!("{}/backend-api/codex/responses", upstream.uri()),
        });
        let response = provider
            .send_chat(
                "eyJhbGciOiJub25lIn0.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsia2V5IjoidmFsdWUifX0.",
                &sample_request("gpt-4o"),
                EffortLevel::Medium,
            )
            .await
            .expect("oauth requests should use codex");

        assert_eq!(
            response.body["choices"][0]["message"]["content"],
            "Hello from Codex"
        );
    }

    #[tokio::test]
    async fn api_keys_keep_using_chat_completions() {
        let _guard = lock_network_test();
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl_1",
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "Hello from chat completions",
                        "tool_calls": []
                    }
                }]
            })))
            .mount(&upstream)
            .await;

        let provider = OpenAiBackendProvider::new(OpenAiBackendConfig {
            base_url: upstream.uri(),
            chat_completions_path: "/v1/chat/completions".to_string(),
            codex_responses_url: format!("{}/backend-api/codex/responses", upstream.uri()),
        });
        let response = provider
            .send_chat("sk-test", &sample_request("gpt-4o"), EffortLevel::Medium)
            .await
            .expect("api key requests should keep chat completions");

        assert_eq!(
            response.body["choices"][0]["message"]["content"],
            "Hello from chat completions"
        );
    }

    #[tokio::test]
    async fn codex_stream_is_translated_to_openai_chunks() {
        let _guard = lock_network_test();
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/backend-api/codex/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                "event: response.output_text.delta\ndata: {\"delta\":\"Hello\"}\n\n\
                 event: response.completed\ndata: {\"type\":\"response.completed\"}\n\n",
                "text/event-stream",
            ))
            .mount(&upstream)
            .await;

        let provider = OpenAiBackendProvider::new(OpenAiBackendConfig {
            base_url: upstream.uri(),
            chat_completions_path: "/v1/chat/completions".to_string(),
            codex_responses_url: format!("{}/backend-api/codex/responses", upstream.uri()),
        });
        let mut stream = provider
            .send_chat_stream(
                "ey.token.value",
                &sample_request("gpt-4o"),
                EffortLevel::Medium,
            )
            .await
            .expect("oauth stream should use codex");

        let mut collected = Vec::new();
        while let Some(chunk) = stream.next().await {
            collected.push(chunk.expect("chunk should decode"));
        }

        let raw = String::from_utf8(collected.concat().to_vec()).expect("utf8");
        assert!(
            raw.contains("\"content\":\"Hello\""),
            "unexpected body: {raw}"
        );
        assert!(
            raw.contains("\"finish_reason\":\"stop\""),
            "unexpected body: {raw}"
        );
    }

    fn sample_request(model: &str) -> OpenAiChatRequest {
        OpenAiChatRequest {
            model: model.to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: Some("You are concise.".to_string()),
                    tool_call_id: None,
                    tool_calls: vec![],
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: Some("Hello".to_string()),
                    tool_call_id: None,
                    tool_calls: vec![],
                },
            ],
            tools: vec![OpenAiToolDefinition {
                kind: "function".to_string(),
                function: OpenAiToolFunction {
                    name: "lookup_weather".to_string(),
                    description: Some("Lookup the weather".to_string()),
                    parameters: json!({"type":"object"}),
                },
            }],
            tool_choice: None,
            stream: false,
            max_tokens: Some(128),
        }
    }
}
