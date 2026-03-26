use std::collections::BTreeMap;

use anyhow::Result;
use serde_json::{json, Value};

use crate::protocol::mapper::map_stop_reason;

#[derive(Debug, Default)]
pub struct OpenAiSseTranslator {
    buffer: String,
    started_message: bool,
    active_block: Option<ActiveBlock>,
    tool_blocks: BTreeMap<usize, ToolBlockState>,
    has_text_block: bool,
    emitted_message_stop: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveBlock {
    Text,
    Tool { upstream_index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ToolBlockState {
    anthropic_index: usize,
    id: String,
    name: String,
    started: bool,
}

impl OpenAiSseTranslator {
    pub fn push_chunk(&mut self, chunk: &str) -> Result<String> {
        self.buffer.push_str(chunk);
        let mut output = String::new();

        while let Some(boundary) = self.buffer.find("\n\n") {
            let frame = self.buffer[..boundary].to_string();
            self.buffer.drain(..boundary + 2);
            output.push_str(&self.translate_frame(frame.trim())?);
        }

        Ok(output)
    }

    fn translate_frame(&mut self, frame: &str) -> Result<String> {
        let payload = extract_sse_payload(frame);
        if payload.is_empty() {
            return Ok(String::new());
        }

        if payload == "[DONE]" {
            return Ok(self.finish_stream());
        }

        let parsed: Value = serde_json::from_str(&payload)?;
        let choice = parsed
            .pointer("/choices/0")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let delta = choice
            .get("delta")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();

        let mut output = String::new();
        if has_relevant_delta(&delta) || choice.get("finish_reason").is_some() {
            self.ensure_message_started(&mut output);
        }

        if let Some(text) = delta.get("content").and_then(Value::as_str) {
            if !text.is_empty() {
                self.ensure_text_block_started(&mut output);
                output.push_str(&format_sse_event(
                    "content_block_delta",
                    json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": {
                            "type": "text_delta",
                            "text": text,
                        }
                    }),
                ));
            }
        }

        if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for tool_call in tool_calls {
                output.push_str(&self.translate_tool_call_delta(tool_call));
            }
        }

        if let Some(finish_reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.close_active_block(&mut output);
            output.push_str(&format_sse_event(
                "message_delta",
                json!({
                    "type": "message_delta",
                    "delta": {
                        "stop_reason": map_stop_reason(
                            Some(finish_reason),
                            !self.tool_blocks.is_empty(),
                        )
                    }
                }),
            ));
        }

        Ok(output)
    }

    fn finish_stream(&mut self) -> String {
        if self.emitted_message_stop {
            return String::new();
        }

        let mut output = String::new();
        self.close_active_block(&mut output);
        output.push_str(&format_sse_event(
            "message_stop",
            json!({ "type": "message_stop" }),
        ));
        self.emitted_message_stop = true;
        output
    }

    fn translate_tool_call_delta(&mut self, tool_call: &Value) -> String {
        let upstream_index = tool_call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        let new_id = tool_call
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        let new_name = tool_call
            .pointer("/function/name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty());
        let arguments = tool_call
            .pointer("/function/arguments")
            .and_then(Value::as_str)
            .filter(|arguments| !arguments.is_empty())
            .map(str::to_string);

        let (anthropic_index, id, name, should_start) = {
            let block = self
                .tool_blocks
                .entry(upstream_index)
                .or_insert_with(|| ToolBlockState {
                    anthropic_index: upstream_index + usize::from(self.has_text_block),
                    id: String::new(),
                    name: String::new(),
                    started: false,
                });

            if let Some(id) = new_id {
                block.id = id.to_string();
            }
            if let Some(name) = new_name {
                block.name = name.to_string();
            }

            let should_start = !block.started;
            if should_start {
                block.started = true;
            }

            (
                block.anthropic_index,
                block.id.clone(),
                block.name.clone(),
                should_start,
            )
        };

        let mut output = String::new();
        if should_start {
            self.close_active_block(&mut output);
            self.active_block = Some(ActiveBlock::Tool { upstream_index });
            output.push_str(&format_sse_event(
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": anthropic_index,
                    "content_block": {
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": {},
                    }
                }),
            ));
        } else if self.active_block != Some(ActiveBlock::Tool { upstream_index }) {
            self.close_active_block(&mut output);
            self.active_block = Some(ActiveBlock::Tool { upstream_index });
        }

        if let Some(arguments) = arguments {
            output.push_str(&format_sse_event(
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": anthropic_index,
                    "delta": {
                        "type": "input_json_delta",
                        "partial_json": arguments,
                    }
                }),
            ));
        }

        output
    }

    fn ensure_message_started(&mut self, output: &mut String) {
        if self.started_message {
            return;
        }

        output.push_str(&format_sse_event(
            "message_start",
            json!({
                "type": "message_start",
                "message": {
                    "type": "message",
                    "role": "assistant",
                    "content": [],
                }
            }),
        ));
        self.started_message = true;
    }

    fn ensure_text_block_started(&mut self, output: &mut String) {
        if self.active_block == Some(ActiveBlock::Text) {
            return;
        }

        self.close_active_block(output);
        self.active_block = Some(ActiveBlock::Text);
        self.has_text_block = true;
        output.push_str(&format_sse_event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {
                    "type": "text",
                    "text": "",
                }
            }),
        ));
    }

    fn close_active_block(&mut self, output: &mut String) {
        let Some(active_block) = self.active_block.take() else {
            return;
        };

        let index = match active_block {
            ActiveBlock::Text => 0,
            ActiveBlock::Tool { upstream_index } => self
                .tool_blocks
                .get(&upstream_index)
                .map(|block| block.anthropic_index)
                .unwrap_or(upstream_index + usize::from(self.has_text_block)),
        };

        output.push_str(&format_sse_event(
            "content_block_stop",
            json!({
                "type": "content_block_stop",
                "index": index,
            }),
        ));
    }
}

#[cfg(test)]
pub fn translate_openai_sse_frame(frame: &str) -> Result<String> {
    let mut translator = OpenAiSseTranslator::default();
    translator.push_chunk(&format!("{frame}\n\n"))
}

fn extract_sse_payload(frame: &str) -> String {
    frame
        .lines()
        .filter_map(|line| {
            line.strip_prefix("data: ")
                .or_else(|| line.strip_prefix("data:"))
        })
        .map(str::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_sse_event(event: &str, data: Value) -> String {
    format!("event: {event}\ndata: {}\n\n", data)
}

fn has_relevant_delta(delta: &serde_json::Map<String, Value>) -> bool {
    delta.get("role").is_some()
        || delta.get("content").is_some()
        || delta.get("tool_calls").is_some()
}

#[cfg(test)]
mod tests {
    use super::{translate_openai_sse_frame, OpenAiSseTranslator};

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
        assert!(translated.contains("event: message_stop"));
    }

    #[test]
    fn buffers_partial_frames_until_they_are_complete() {
        let mut translator = OpenAiSseTranslator::default();
        let first = translator
            .push_chunk(r#"data: {"choices":[{"delta":{"content":"Hel"#)
            .expect("first fragment");
        assert!(first.is_empty());

        let second = translator
            .push_chunk("lo\"}}]}\n\n")
            .expect("second fragment");
        assert!(second.contains("event: message_start"));
        assert!(second.contains("\"text\":\"Hello\""));
    }

    #[test]
    fn streams_tool_calls_as_tool_use_blocks() {
        let mut translator = OpenAiSseTranslator::default();
        let start = translator
            .push_chunk(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_lookup_weather\",\"type\":\"function\",\"function\":{\"name\":\"lookup_weather\",\"arguments\":\"\"}}]}}]}\n\n",
            )
            .expect("start chunk");
        assert!(start.contains("event: message_start"));
        assert!(start.contains("event: content_block_start"));
        assert!(start.contains("\"type\":\"tool_use\""));
        assert!(start.contains("\"id\":\"call_lookup_weather\""));
        assert!(start.contains("\"name\":\"lookup_weather\""));

        let delta = translator
            .push_chunk(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"city\\\":\\\"Mad\"}}]}}]}\n\n",
            )
            .expect("delta chunk");
        assert!(delta.contains("\"type\":\"input_json_delta\""));
        assert!(delta.contains("\"partial_json\":\"{\\\"city\\\":\\\"Mad\""));

        let stop = translator
            .push_chunk(
                "data: {\"choices\":[{\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n",
            )
            .expect("stop chunk");
        assert!(stop.contains("event: content_block_stop"));
        assert!(stop.contains("event: message_delta"));
        assert!(stop.contains("\"stop_reason\":\"tool_use\""));
        assert!(stop.contains("event: message_stop"));
    }

    #[test]
    fn closes_text_block_before_starting_a_tool_use_block() {
        let mut translator = OpenAiSseTranslator::default();
        let text = translator
            .push_chunk("data: {\"choices\":[{\"delta\":{\"content\":\"Checking...\"}}]}\n\n")
            .expect("text chunk");
        assert!(text.contains("\"index\":0"));

        let tool = translator
            .push_chunk(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_lookup_weather\",\"type\":\"function\",\"function\":{\"name\":\"lookup_weather\",\"arguments\":\"\"}}]}}]}\n\n",
            )
            .expect("tool chunk");
        assert!(tool.contains("event: content_block_stop"));
        assert!(tool.contains("\"index\":0"));
        assert!(tool.contains("\"index\":1"));
        assert!(tool.contains("\"type\":\"tool_use\""));
    }
}

#[cfg(test)]
mod regression_tests {
    use super::translate_openai_sse_frame;

    #[test]
    fn done_marker_produces_message_stop_event() {
        let payload = translate_openai_sse_frame("data: [DONE]").expect("translation");
        assert!(payload.contains("event: message_stop"));
    }
}
