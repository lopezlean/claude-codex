use axum::body::Body;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{extract::State, Json};
use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::json;

use crate::protocol::anthropic::AnthropicMessagesRequest;
use crate::protocol::mapper::map_anthropic_to_openai;
use crate::protocol::stream::translate_openai_sse_frame;
use crate::server::AppState;

pub async fn create_message(
    State(state): State<AppState>,
    Json(request): Json<AnthropicMessagesRequest>,
) -> Result<Response, (StatusCode, String)> {
    let access_token = state
        .auth
        .ensure_access_token()
        .await
        .map_err(internal_error)?;
    let mapped = map_anthropic_to_openai(&request).map_err(internal_error)?;

    if request.stream {
        let stream = state
            .backend
            .send_chat_stream(&access_token, &mapped)
            .await
            .map_err(internal_error)?
            .map(|chunk| {
                let bytes = chunk?;
                let raw = String::from_utf8_lossy(&bytes);
                let translated = translate_openai_sse_frame(&raw)?;
                Ok::<Bytes, anyhow::Error>(Bytes::from(translated))
            });

        let body = Body::from_stream(stream);
        let response = ([(header::CONTENT_TYPE, "text/event-stream")], body).into_response();
        return Ok(response);
    }

    let upstream = state
        .backend
        .send_chat(&access_token, &mapped)
        .await
        .map_err(internal_error)?;
    if !upstream.status.is_success() {
        return Err((StatusCode::BAD_GATEWAY, upstream.body.to_string()));
    }

    let assistant_text = upstream
        .body
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    let body = json!({
        "id": "msg_codex_proxy",
        "type": "message",
        "role": "assistant",
        "model": request.model,
        "content": [
            {
                "type": "text",
                "text": assistant_text
            }
        ],
        "stop_reason": "end_turn"
    });

    Ok(axum::Json(body).into_response())
}

fn internal_error(error: impl std::fmt::Display) -> (StatusCode, String) {
    tracing::error!("proxy request failed: {error}");
    (StatusCode::BAD_GATEWAY, error.to_string())
}
