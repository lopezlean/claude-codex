use anyhow::Result;
use serde_json::Value;

pub fn translate_openai_sse_frame(frame: &str) -> Result<String> {
    let payload = frame.trim().strip_prefix("data: ").unwrap_or(frame.trim());
    if payload == "[DONE]" {
        return Ok(
            "event: content_block_stop\ndata: {\"index\":0}\n\nevent: message_stop\ndata: {}\n\n"
                .to_string(),
        );
    }

    let parsed: Value = serde_json::from_str(payload)?;
    let delta = parsed
        .pointer("/choices/0/delta/content")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    Ok(format!(
        "event: message_start\ndata: {{\"type\":\"message\"}}\n\nevent: content_block_start\ndata: {{\"index\":0,\"content_block\":{{\"type\":\"text\",\"text\":\"\"}}}}\n\nevent: content_block_delta\ndata: {{\"type\":\"text_delta\",\"text\":\"{}\"}}\n\n",
        delta
    ))
}

#[cfg(test)]
mod tests {
    use super::translate_openai_sse_frame;

    #[test]
    fn converts_openai_content_delta_to_anthropic_events() {
        let frame = r#"data: {"choices":[{"delta":{"content":"Hel"}}]}"#;
        let translated = translate_openai_sse_frame(frame).expect("translation should work");
        assert!(translated.contains("event: message_start"));
        assert!(translated.contains("event: content_block_start"));
        assert!(translated.contains("event: content_block_delta"));
        assert!(translated.contains("\"text\":\"Hel\""));
    }

    #[test]
    fn converts_done_marker_to_message_stop() {
        let translated = translate_openai_sse_frame("data: [DONE]").expect("done marker");
        assert!(translated.contains("event: content_block_stop"));
        assert!(translated.contains("event: message_stop"));
    }
}
