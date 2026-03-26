use std::collections::BTreeMap;

use anyhow::Result;
use serde_json::{json, Value};

use crate::protocol::mapper::map_stop_reason;

#[derive(Debug, Default)]
pub struct OpenAiSseTranslator {
    buffer: Vec<u8>,
    started_message: bool,
    next_content_index: usize,
    text_block: Option<TextBlockState>,
    tool_blocks: BTreeMap<usize, ToolBlockState>,
    emitted_message_stop: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextBlockState {
    anthropic_index: usize,
    open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ToolBlockState {
    anthropic_index: usize,
    id: Option<String>,
    name: Option<String>,
    started: bool,
    open: bool,
    pending_arguments: String,
}

impl OpenAiSseTranslator {
    pub fn push_bytes(&mut self, chunk: &[u8]) -> Result<String> {
        self.buffer.extend_from_slice(chunk);
        let mut output = String::new();

        while let Some((frame_end, delimiter_len)) = find_frame_boundary(&self.buffer) {
            let frame_bytes = self.buffer[..frame_end].to_vec();
            self.buffer.drain(..frame_end + delimiter_len);
            let frame = String::from_utf8(frame_bytes)?;
            output.push_str(&self.translate_frame(frame.trim())?);
        }

        Ok(output)
    }

    #[cfg(test)]
    pub fn push_chunk(&mut self, chunk: &str) -> Result<String> {
        self.push_bytes(chunk.as_bytes())
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
                let index = self.ensure_text_block_started(&mut output);
                output.push_str(&format_sse_event(
                    "content_block_delta",
                    json!({
                        "type": "content_block_delta",
                        "index": index,
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
            self.close_text_block(&mut output);
            self.close_open_tool_blocks(&mut output);
            output.push_str(&format_sse_event(
                "message_delta",
                json!({
                    "type": "message_delta",
                    "delta": {
                        "stop_reason": map_stop_reason(
                            Some(finish_reason),
                            self.tool_blocks.values().any(|block| block.started),
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
        self.close_text_block(&mut output);
        self.close_open_tool_blocks(&mut output);
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
            .filter(|id| !id.is_empty())
            .map(str::to_string);
        let new_name = tool_call
            .pointer("/function/name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(str::to_string);
        let new_arguments = tool_call
            .pointer("/function/arguments")
            .and_then(Value::as_str)
            .filter(|arguments| !arguments.is_empty())
            .map(str::to_string);

        let (anthropic_index, id, name, should_start, pending_delta) = {
            let next_index = self.next_content_index;
            let block = self
                .tool_blocks
                .entry(upstream_index)
                .or_insert_with(|| ToolBlockState {
                    anthropic_index: next_index,
                    id: None,
                    name: None,
                    started: false,
                    open: false,
                    pending_arguments: String::new(),
                });

            if block.anthropic_index == next_index {
                self.next_content_index += 1;
            }

            if let Some(id) = new_id {
                block.id = Some(id);
            }
            if let Some(name) = new_name {
                block.name = Some(name);
            }

            let mut pending_delta = None;
            if block.started {
                if let Some(arguments) = new_arguments {
                    pending_delta = Some(arguments);
                }
            } else if let Some(arguments) = new_arguments {
                block.pending_arguments.push_str(&arguments);
            }

            let should_start = !block.started && block.id.is_some() && block.name.is_some();
            if should_start {
                block.started = true;
                block.open = true;
                if !block.pending_arguments.is_empty() {
                    pending_delta = Some(std::mem::take(&mut block.pending_arguments));
                }
            }

            (
                block.anthropic_index,
                block.id.clone().unwrap_or_default(),
                block.name.clone().unwrap_or_default(),
                should_start,
                pending_delta,
            )
        };

        let mut output = String::new();
        if should_start {
            self.close_text_block(&mut output);
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
        }

        if let Some(arguments) = pending_delta {
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

    fn ensure_text_block_started(&mut self, output: &mut String) -> usize {
        if let Some(block) = &self.text_block {
            if block.open {
                return block.anthropic_index;
            }
        }

        let anthropic_index = self.next_content_index;
        self.next_content_index += 1;
        self.text_block = Some(TextBlockState {
            anthropic_index,
            open: true,
        });
        output.push_str(&format_sse_event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": anthropic_index,
                "content_block": {
                    "type": "text",
                    "text": "",
                }
            }),
        ));
        anthropic_index
    }

    fn close_text_block(&mut self, output: &mut String) {
        let Some(block) = &mut self.text_block else {
            return;
        };
        if !block.open {
            return;
        }

        output.push_str(&format_sse_event(
            "content_block_stop",
            json!({
                "type": "content_block_stop",
                "index": block.anthropic_index,
            }),
        ));
        block.open = false;
    }

    fn close_open_tool_blocks(&mut self, output: &mut String) {
        let mut open_blocks = self
            .tool_blocks
            .iter()
            .filter(|(_, block)| block.open)
            .map(|(upstream_index, block)| (*upstream_index, block.anthropic_index))
            .collect::<Vec<_>>();
        open_blocks.sort_by_key(|(_, anthropic_index)| *anthropic_index);

        for (upstream_index, anthropic_index) in open_blocks {
            if let Some(block) = self.tool_blocks.get_mut(&upstream_index) {
                block.open = false;
            }
            output.push_str(&format_sse_event(
                "content_block_stop",
                json!({
                    "type": "content_block_stop",
                    "index": anthropic_index,
                }),
            ));
        }
    }
}

#[cfg(test)]
pub fn translate_openai_sse_frame(frame: &str) -> Result<String> {
    let mut translator = OpenAiSseTranslator::default();
    translator.push_chunk(&format!("{frame}\n\n"))
}

fn find_frame_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    for index in 0..buffer.len().saturating_sub(1) {
        if buffer[index..].starts_with(b"\r\n\r\n") {
            return Some((index, 4));
        }
        if buffer[index..].starts_with(b"\n\n") {
            return Some((index, 2));
        }
    }
    None
}

fn extract_sse_payload(frame: &str) -> String {
    frame
        .lines()
        .map(|line| line.trim_end_matches('\r'))
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
    fn buffers_utf8_multibyte_characters_across_byte_chunks() {
        let mut translator = OpenAiSseTranslator::default();
        let chunk = "data: {\"choices\":[{\"delta\":{\"content\":\"Málaga\"}}]}\n\n"
            .as_bytes()
            .to_vec();
        let accent = "á".as_bytes();
        let split_index = chunk
            .windows(accent.len())
            .position(|window| window == accent)
            .expect("accent should exist")
            + 1;

        let first = translator
            .push_bytes(&chunk[..split_index])
            .expect("first bytes");
        assert!(first.is_empty());

        let second = translator
            .push_bytes(&chunk[split_index..])
            .expect("second bytes");
        assert!(second.contains("\"text\":\"Málaga\""));
        assert!(!second.contains("\u{fffd}"));
    }

    #[test]
    fn waits_for_tool_metadata_before_starting_tool_use_block() {
        let mut translator = OpenAiSseTranslator::default();
        let first = translator
            .push_chunk(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"city\\\":\\\"Mad\"}}]}}]}\n\n",
            )
            .expect("first chunk");
        assert!(first.contains("event: message_start"));
        assert!(!first.contains("event: content_block_start"));

        let second = translator
            .push_chunk(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_lookup_weather\",\"type\":\"function\",\"function\":{\"name\":\"lookup_weather\"}}]}}]}\n\n",
            )
            .expect("second chunk");
        assert!(second.contains("event: content_block_start"));
        assert!(second.contains("\"id\":\"call_lookup_weather\""));
        assert!(second.contains("\"name\":\"lookup_weather\""));
        assert!(second.contains("\"partial_json\":\"{\\\"city\\\":\\\"Mad\""));
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

    #[test]
    fn supports_interleaved_tool_call_deltas_without_stopping_earlier_blocks() {
        let mut translator = OpenAiSseTranslator::default();
        let tool0 = translator
            .push_chunk(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_lookup_weather\",\"type\":\"function\",\"function\":{\"name\":\"lookup_weather\",\"arguments\":\"{\\\"city\\\":\\\"Mad\"}}]}}]}\n\n",
            )
            .expect("tool0 start");
        assert!(tool0.contains("\"index\":0"));

        let tool1 = translator
            .push_chunk(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"id\":\"call_lookup_country\",\"type\":\"function\",\"function\":{\"name\":\"lookup_country\",\"arguments\":\"{\\\"code\\\":\\\"ES\\\"}\"}}]}}]}\n\n",
            )
            .expect("tool1 start");
        assert!(!tool1.contains("event: content_block_stop"));
        assert!(tool1.contains("\"index\":1"));

        let tool0_more = translator
            .push_chunk(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"rid\\\"}\"}}]}}]}\n\n",
            )
            .expect("tool0 more");
        assert!(!tool0_more.contains("event: content_block_stop"));
        assert!(tool0_more.contains("\"index\":0"));
        assert!(tool0_more.contains("\"partial_json\":\"rid\\\"}\""));
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
