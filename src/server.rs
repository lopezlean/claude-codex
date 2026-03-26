use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    routing::{get, post},
    Router,
};

use crate::auth::provider::AuthProvider;
use crate::backend::provider::BackendProvider;
use crate::handlers::{count_tokens::count_tokens, health::health, messages::create_message};
#[cfg(test)]
use crate::{
    auth::{
        openai::{OpenAiAuthConfig, OpenAiAuthProvider},
        session_store::FileSessionStore,
    },
    backend::openai::{OpenAiBackendConfig, OpenAiBackendProvider},
};

#[derive(Clone)]
pub struct AppState {
    pub auth: Arc<dyn AuthProvider>,
    pub backend: Arc<dyn BackendProvider>,
}

impl AppState {
    #[cfg(test)]
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

pub async fn serve(state: AppState, port: u16) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    axum::serve(listener, build_router(state)).await?;
    Ok(())
}

pub async fn wait_until_ready(port: u16) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let url = format!("http://127.0.0.1:{port}/healthz");
    let client = reqwest::Client::new();

    loop {
        if let Ok(response) = client.get(&url).send().await {
            if response.status().is_success() {
                return Ok(());
            }
        }

        if Instant::now() >= deadline {
            anyhow::bail!("proxy did not become ready at {url}");
        }

        tokio::time::sleep(Duration::from_millis(25)).await;
    }
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
    use crate::server::{build_router, serve, wait_until_ready};
    use crate::test_support::lock_network_test;

    #[tokio::test]
    async fn healthz_returns_ok() {
        let _guard = lock_network_test();
        let router = build_router_for_test("http://127.0.0.1:9").await;
        let response = router
            .oneshot(
                axum::http::Request::get("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn wait_until_ready_observes_the_live_server() {
        let _guard = lock_network_test();
        let upstream = MockServer::start().await;
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
        let state = crate::server::AppState::for_tests(
            FileSessionStore::new(auth_dir.join("auth.json")),
            OpenAiBackendConfig {
                base_url: upstream.uri(),
                chat_completions_path: "/v1/chat/completions".to_string(),
            },
        )
        .await;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let server_task = tokio::spawn(serve(state, port));
        wait_until_ready(port)
            .await
            .expect("server should become ready");
        server_task.abort();
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
    async fn forwards_openai_tool_calls_as_anthropic_tool_use_blocks() {
        let _guard = lock_network_test();
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{
                      "id":"chatcmpl_tool",
                      "choices":[{
                        "message":{
                          "role":"assistant",
                          "content":"I will use a tool",
                          "tool_calls":[{
                            "id":"call_lookup_weather",
                            "type":"function",
                            "function":{
                              "name":"lookup_weather",
                              "arguments":"{\"city\":\"Madrid\"}"
                            }
                          }]
                        }
                      }]
                    }"#,
                "application/json",
            ))
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
                          "messages":[{"role":"user","content":[{"type":"text","text":"What's the weather in Madrid?"}]}],
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
        assert!(raw.contains("\"type\":\"text\""), "unexpected body: {raw}");
        assert!(
            raw.contains("\"text\":\"I will use a tool\""),
            "unexpected body: {raw}"
        );
        assert!(
            raw.contains("\"type\":\"tool_use\""),
            "unexpected body: {raw}"
        );
        assert!(
            raw.contains("\"id\":\"call_lookup_weather\""),
            "unexpected body: {raw}"
        );
        assert!(
            raw.contains("\"name\":\"lookup_weather\""),
            "unexpected body: {raw}"
        );
        assert!(
            raw.contains("\"input\":{\"city\":\"Madrid\"}"),
            "unexpected body: {raw}"
        );
        assert!(
            raw.contains("\"stop_reason\":\"tool_use\""),
            "unexpected body: {raw}"
        );
    }

    #[tokio::test]
    async fn streams_openai_tool_calls_as_anthropic_sse_events() {
        let _guard = lock_network_test();
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_lookup_weather\",\"type\":\"function\",\"function\":{\"name\":\"lookup_weather\",\"arguments\":\"\"}}]}}]}\n\n\
                 data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"city\\\":\\\"Madrid\\\"}\"}}]}}]}\n\n\
                 data: {\"choices\":[{\"finish_reason\":\"tool_calls\"}]}\n\n\
                 data: [DONE]\n\n",
                "text/event-stream",
            ))
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
                          "messages":[{"role":"user","content":[{"type":"text","text":"What's the weather in Madrid?"}]}],
                          "stream":true
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/event-stream"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let raw = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            raw.contains("event: message_start"),
            "unexpected body: {raw}"
        );
        assert!(
            raw.contains("event: content_block_start"),
            "unexpected body: {raw}"
        );
        assert!(
            raw.contains("\"type\":\"tool_use\""),
            "unexpected body: {raw}"
        );
        assert!(
            raw.contains("\"partial_json\":\"{\\\"city\\\":\\\"Madrid\\\"}\""),
            "unexpected body: {raw}"
        );
        assert!(
            raw.contains("event: message_delta"),
            "unexpected body: {raw}"
        );
        assert!(
            raw.contains("\"stop_reason\":\"tool_use\""),
            "unexpected body: {raw}"
        );
        assert!(
            raw.contains("event: message_stop"),
            "unexpected body: {raw}"
        );
    }

    #[tokio::test]
    async fn does_not_fail_when_openai_tool_arguments_are_malformed() {
        let _guard = lock_network_test();
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{
                  "id":"chatcmpl_tool",
                  "choices":[{
                    "finish_reason":"length",
                    "message":{
                      "role":"assistant",
                      "content":"Partial tool call",
                      "tool_calls":[{
                        "id":"call_lookup_weather",
                        "type":"function",
                        "function":{
                          "name":"lookup_weather",
                          "arguments":"{\"city\":\"Mad"
                        }
                      }]
                    }
                  }]
                }"#,
                "application/json",
            ))
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
                          "messages":[{"role":"user","content":[{"type":"text","text":"What's the weather in Madrid?"}]}],
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
        assert!(
            raw.contains("\"stop_reason\":\"max_tokens\""),
            "unexpected body: {raw}"
        );
        assert!(
            raw.contains("\"__raw_arguments\":\"{\\\"city\\\":\\\"Mad\""),
            "unexpected body: {raw}"
        );
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
                          "system":"You are concise.",
                          "messages":[{"role":"user","content":[{"type":"text","text":"Hello world"}]}]
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let raw = String::from_utf8(body.to_vec()).unwrap();
        assert!(raw.contains("\"input_tokens\":5"), "unexpected body: {raw}");
    }

    #[tokio::test]
    async fn returns_bad_gateway_when_authentication_is_missing() {
        let _guard = lock_network_test();
        let router = build_router_without_auth_for_test("http://127.0.0.1:9").await;
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

        assert_eq!(response.status(), axum::http::StatusCode::BAD_GATEWAY);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let raw = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            raw.contains("authentication is required"),
            "unexpected body: {raw}"
        );
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

    async fn build_router_without_auth_for_test(upstream_base_url: &str) -> axum::Router {
        let dir = tempfile::tempdir().expect("temp dir");
        let auth_dir = dir.keep();
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
