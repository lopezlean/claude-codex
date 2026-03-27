use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
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
        let upstream = state
            .backend
            .send_chat_stream(&access_token, &mapped, state.effort)
            .await
            .map_err(internal_error)?;
        let stream = upstream.stream.map(move |chunk| {
            let bytes = chunk?;
            let translated = translator.push_bytes(&bytes)?;
            Ok::<Bytes, anyhow::Error>(Bytes::from(translated))
        });

        let body = Body::from_stream(stream);
        let mut response = body.into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        );
        append_upstream_headers(response.headers_mut(), &upstream.headers)?;
        return Ok(response);
    }

    let upstream = state
        .backend
        .send_chat(&access_token, &mapped, state.effort)
        .await
        .map_err(internal_error)?;
    if !upstream.status.is_success() {
        return Err((StatusCode::BAD_GATEWAY, upstream.body.to_string()));
    }

    let body =
        map_openai_to_anthropic_response(&request.model, &upstream.body).map_err(internal_error)?;

    let mut response = axum::Json(body).into_response();
    append_upstream_headers(response.headers_mut(), &upstream.headers)?;
    Ok(response)
}

fn internal_error(error: impl std::fmt::Display) -> (StatusCode, String) {
    tracing::error!("proxy request failed: {error}");
    (StatusCode::BAD_GATEWAY, error.to_string())
}

fn append_upstream_headers(
    headers: &mut HeaderMap,
    upstream_headers: &crate::backend::provider::UpstreamHeaders,
) -> Result<(), (StatusCode, String)> {
    for (name, value) in &upstream_headers.entries {
        let header_name = HeaderName::try_from(name.as_str()).map_err(internal_error)?;
        let header_value = HeaderValue::try_from(value.as_str()).map_err(internal_error)?;
        headers.insert(header_name, header_value);
    }
    Ok(())
}
