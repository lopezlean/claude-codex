use axum::body::Body;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{extract::State, Json};
use bytes::Bytes;
use futures_util::StreamExt;

use crate::protocol::anthropic::AnthropicMessagesRequest;
use crate::protocol::mapper::{map_anthropic_to_openai, map_openai_to_anthropic_response};
use crate::protocol::stream::OpenAiSseTranslator;
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
        let mut translator = OpenAiSseTranslator::default();
        let stream = state
            .backend
            .send_chat_stream(&access_token, &mapped)
            .await
            .map_err(internal_error)?
            .map(move |chunk| {
                let bytes = chunk?;
                let raw = String::from_utf8_lossy(&bytes);
                let translated = translator.push_chunk(&raw)?;
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

    let body =
        map_openai_to_anthropic_response(&request.model, &upstream.body).map_err(internal_error)?;

    Ok(axum::Json(body).into_response())
}

fn internal_error(error: impl std::fmt::Display) -> (StatusCode, String) {
    tracing::error!("proxy request failed: {error}");
    (StatusCode::BAD_GATEWAY, error.to_string())
}
