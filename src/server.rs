use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};

use crate::auth::openai::{OpenAiAuthConfig, OpenAiAuthProvider};
use crate::auth::provider::AuthProvider;
use crate::auth::session_store::FileSessionStore;
use crate::backend::openai::{OpenAiBackendConfig, OpenAiBackendProvider};
use crate::backend::provider::BackendProvider;
use crate::handlers::{count_tokens::count_tokens, health::health, messages::create_message};

#[derive(Clone)]
pub struct AppState {
    pub auth: Arc<dyn AuthProvider>,
    pub backend: Arc<dyn BackendProvider>,
}

impl AppState {
    pub async fn for_tests(store: FileSessionStore, backend: OpenAiBackendConfig) -> Self {
        Self {
            auth: Arc::new(OpenAiAuthProvider::new(
                OpenAiAuthConfig {
                    client_id: "client-id".to_string(),
                    auth_url: "https://auth.openai.com/oauth/authorize".to_string(),
                    token_url: "https://auth.openai.com/oauth/token".to_string(),
                    redirect_port: 1455,
                    callback_timeout_secs: 1,
                    refresh_grace_period_secs: 60,
                },
                store,
            )),
            backend: Arc::new(OpenAiBackendProvider::new(backend)),
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/v1/messages", post(create_message))
        .route("/v1/messages/count_tokens", post(count_tokens))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::auth::session::{CodexAuthFile, CodexTokens};
    use crate::auth::session_store::FileSessionStore;
    use crate::backend::openai::OpenAiBackendConfig;
    use crate::server::build_router;
    use crate::test_support::lock_network_test;

    #[tokio::test]
    async fn healthz_returns_ok() {
        let _guard = lock_network_test();
        let router = build_router_for_test("http://127.0.0.1:9").await;
        let response = router
            .oneshot(axum::http::Request::get("/healthz").body(Body::empty()).unwrap())
            .await
            .expect("response");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn forwards_non_stream_messages_and_returns_anthropic_shape() {
        let _guard = lock_network_test();
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_string_contains("\"model\":\"gpt-4o\""))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(
                    r#"{
                      "id":"chatcmpl_123",
                      "choices":[{"message":{"role":"assistant","content":"Hello from OpenAI","tool_calls":[]}}],
                      "usage":{"prompt_tokens":10,"completion_tokens":4}
                    }"#,
                    "application/json",
                ),
            )
            .mount(&upstream)
            .await;

        let router = build_router_for_test(&upstream.uri()).await;
        let response = router
            .oneshot(
                axum::http::Request::post("/v1/messages")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{
                          "model":"claude-3-5-sonnet-latest",
                          "messages":[{"role":"user","content":[{"type":"text","text":"Hello"}]}],
                          "stream":false
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let raw = String::from_utf8(body.to_vec()).unwrap();
        assert!(raw.contains("\"type\":\"text\""));
        assert!(raw.contains("Hello from OpenAI"));
    }

    #[tokio::test]
    async fn count_tokens_returns_a_local_estimate() {
        let _guard = lock_network_test();
        let router = build_router_for_test("http://127.0.0.1:9").await;
        let response = router
            .oneshot(
                axum::http::Request::post("/v1/messages/count_tokens")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{
                          "model":"claude-3-5-sonnet-latest",
                          "messages":[{"role":"user","content":[{"type":"text","text":"Hello world"}]}]
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    async fn build_router_for_test(upstream_base_url: &str) -> axum::Router {
        let dir = tempfile::tempdir().expect("temp dir");
        let auth_dir = dir.keep();
        let auth_path = auth_dir.join("auth.json");
        FileSessionStore::new(auth_path)
            .save(&CodexAuthFile {
                auth_mode: Some("openai".to_string()),
                tokens: CodexTokens {
                    id_token: None,
                    access_token: Some("access-token".to_string()),
                    refresh_token: None,
                    account_id: None,
                },
                last_refresh: Some("123".to_string()),
            })
            .expect("seed auth");

        build_router(
            crate::server::AppState::for_tests(
                FileSessionStore::new(auth_dir.join("auth.json")),
                OpenAiBackendConfig {
                    base_url: upstream_base_url.to_string(),
                    chat_completions_path: "/v1/chat/completions".to_string(),
                },
            )
            .await,
        )
    }
}
