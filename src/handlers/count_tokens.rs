use axum::Json;
use serde::Serialize;

use crate::protocol::anthropic::{AnthropicContentBlock, AnthropicMessagesRequest};

#[derive(Debug, Serialize)]
pub struct CountTokensResponse {
    pub input_tokens: usize,
}

pub async fn count_tokens(
    Json(request): Json<AnthropicMessagesRequest>,
) -> Json<CountTokensResponse> {
    let mut input_tokens = request
        .system
        .map(|system| system.into_text().split_whitespace().count())
        .unwrap_or(0);
    for message in request.messages {
        for block in message.content {
            input_tokens += match block {
                AnthropicContentBlock::Text { text } => text.split_whitespace().count(),
                AnthropicContentBlock::ToolUse { name, input, .. } => {
                    name.split_whitespace().count() + input.to_string().len() / 4
                }
                AnthropicContentBlock::ToolResult { content, .. } => {
                    content.split_whitespace().count()
                }
            };
        }
    }
    Json(CountTokensResponse { input_tokens })
}
