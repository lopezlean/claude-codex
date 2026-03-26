use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;

use crate::protocol::anthropic::AnthropicMessagesRequest;
use crate::protocol::mapper::map_anthropic_to_openai;
use crate::server::AppState;

pub async fn create_message(
    State(state): State<AppState>,
    Json(request): Json<AnthropicMessagesRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let access_token = state.auth.ensure_access_token().await.map_err(internal_error)?;
    let mapped = map_anthropic_to_openai(&request).map_err(internal_error)?;
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

    Ok(Json(json!({
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
    })))
}

fn internal_error(error: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}
