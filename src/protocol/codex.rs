use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{json, Value};

use crate::protocol::openai::{OpenAiChatMessage, OpenAiChatRequest, OpenAiToolChoice};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexEffortLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CodexResponsesRequest {
    pub model: String,
    pub store: bool,
    pub stream: bool,
    pub instructions: String,
    pub input: Vec<CodexInputMessage>,
    pub reasoning: CodexReasoningOptions,
    pub text: CodexTextOptions,
    pub include: Vec<String>,
    pub tool_choice: String,
    pub parallel_tool_calls: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<CodexToolDecl>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CodexTextOptions {
    pub verbosity: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CodexReasoningOptions {
    pub effort: CodexEffortLevel,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CodexInputMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CodexToolDecl {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub parameters: Value,
}

pub fn build_codex_request(
    request: &OpenAiChatRequest,
    effort: CodexEffortLevel,
) -> CodexResponsesRequest {
    let mut instructions = Vec::new();
    let mut input = Vec::new();

    for message in &request.messages {
        match message.role.as_str() {
            "system" => {
                if let Some(content) = message
                    .content
                    .as_deref()
                    .filter(|content| !content.is_empty())
                {
                    instructions.push(content.to_string());
                }
            }
            "user" | "assistant" => {
                if let Some(content) = message
                    .content
                    .as_deref()
                    .filter(|content| !content.is_empty())
                {
                    input.push(CodexInputMessage {
                        role: message.role.clone(),
                        content: content.to_string(),
                    });
                }

                for tool_call in &message.tool_calls {
                    input.push(CodexInputMessage {
                        role: "assistant".to_string(),
                        content: format!(
                            "[called tool: {} with {}]",
                            tool_call.function.name, tool_call.function.arguments
                        ),
                    });
                }
            }
            "tool" => input.push(CodexInputMessage {
                role: "user".to_string(),
                content: summarize_tool_result(message),
            }),
            _ => {
                if let Some(content) = message
                    .content
                    .as_deref()
                    .filter(|content| !content.is_empty())
                {
                    input.push(CodexInputMessage {
                        role: message.role.clone(),
                        content: content.to_string(),
                    });
                }
            }
        }
    }

    let tools = request
        .tools
        .iter()
        .map(|tool| CodexToolDecl {
            kind: "function".to_string(),
            name: tool.function.name.clone(),
            description: tool.function.description.clone(),
            parameters: tool.function.parameters.clone(),
        })
        .collect();

    CodexResponsesRequest {
        model: request.model.clone(),
        store: false,
        stream: true,
        instructions: instructions.join("\n\n"),
        input,
        reasoning: CodexReasoningOptions { effort },
        text: CodexTextOptions {
            verbosity: "medium".to_string(),
        },
        include: vec!["reasoning.encrypted_content".to_string()],
        tool_choice: map_tool_choice(request.tool_choice.as_ref()),
        parallel_tool_calls: true,
        tools,
    }
}

fn summarize_tool_result(message: &OpenAiChatMessage) -> String {
    let content = message.content.as_deref().unwrap_or_default();
    let call_id = message.tool_call_id.as_deref().unwrap_or("unknown");
    format!("[tool result for call {}: {}]", call_id, content)
}

fn map_tool_choice(tool_choice: Option<&OpenAiToolChoice>) -> String {
    match tool_choice {
        Some(OpenAiToolChoice::Required) => "required".to_string(),
        Some(OpenAiToolChoice::Auto) | None => "auto".to_string(),
    }
}

#[derive(Debug, Default)]
pub struct CodexSseToOpenAiBridge {
    buffer: Vec<u8>,
    text_output: String,
    pending_tool_arguments: String,
    tool_calls: Vec<CodexCompletedToolCall>,
    emitted_finish: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexCompletedToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl CodexSseToOpenAiBridge {
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

    pub fn finish_stream(&mut self) -> String {
        if self.emitted_finish {
            return String::new();
        }

        self.emitted_finish = true;
        let finish_reason = if self.tool_calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        };
        format!(
            "data: {}\n\n\
             data: [DONE]\n\n",
            json!({
                "choices": [{
                    "finish_reason": finish_reason
                }]
            })
        )
    }

    pub fn into_chat_response(mut self) -> Value {
        let _ = self.finish_stream();
        json!({
            "id": "chatcmpl_codex_proxy",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": self.text_output,
                    "tool_calls": self.tool_calls.iter().map(|tool_call| json!({
                        "id": tool_call.id,
                        "type": "function",
                        "function": {
                            "name": tool_call.name,
                            "arguments": tool_call.arguments,
                        }
                    })).collect::<Vec<_>>()
                },
                "finish_reason": if self.tool_calls.is_empty() { "stop" } else { "tool_calls" }
            }]
        })
    }

    fn translate_frame(&mut self, frame: &str) -> Result<String> {
        let payload = extract_sse_payload(frame);
        if payload.is_empty() {
            return Ok(String::new());
        }

        if payload == "[DONE]" {
            return Ok(self.finish_stream());
        }

        let value: Value = serde_json::from_str(&payload)?;
        let event_type = extract_event_type(frame, &value);
        let mut output = String::new();

        match event_type.as_str() {
            "response.output_text.delta" => {
                if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                    self.text_output.push_str(delta);
                    output.push_str(&format_openai_stream_delta(json!({
                        "content": delta
                    })));
                }
            }
            "response.function_call_arguments.delta" => {
                if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                    self.pending_tool_arguments.push_str(delta);
                }
            }
            "response.output_item.done" => {
                if let Some(item) = value.get("item") {
                    if item.get("type").and_then(Value::as_str) == Some("function_call") {
                        let id = item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .unwrap_or("call_codex_proxy")
                            .to_string();
                        let name = item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_string();
                        let arguments = item
                            .get("arguments")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .filter(|arguments| !arguments.is_empty())
                            .unwrap_or_else(|| std::mem::take(&mut self.pending_tool_arguments));
                        let index = self.tool_calls.len();
                        self.tool_calls.push(CodexCompletedToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            arguments: arguments.clone(),
                        });
                        output.push_str(&format_openai_stream_delta(json!({
                            "tool_calls": [{
                                "index": index,
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": ""
                                }
                            }]
                        })));
                        if !arguments.is_empty() {
                            output.push_str(&format_openai_stream_delta(json!({
                                "tool_calls": [{
                                    "index": index,
                                    "function": {
                                        "arguments": arguments
                                    }
                                }]
                            })));
                        }
                    }
                }
            }
            "response.completed" => output.push_str(&self.finish_stream()),
            "response.failed" => {
                let message = value
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("codex responses request failed");
                bail!(message.to_string());
            }
            _ => {}
        }

        Ok(output)
    }
}

fn extract_event_type(frame: &str, value: &Value) -> String {
    frame
        .lines()
        .find_map(|line| line.strip_prefix("event: ").map(str::to_string))
        .or_else(|| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

fn extract_sse_payload(frame: &str) -> String {
    frame
        .lines()
        .filter_map(|line| line.strip_prefix("data: ").map(str::trim))
        .collect::<Vec<_>>()
        .join("\n")
}

fn find_frame_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    for delimiter in [b"\n\n".as_slice(), b"\r\n\r\n".as_slice()] {
        if let Some(index) = buffer
            .windows(delimiter.len())
            .position(|window| window == delimiter)
        {
            return Some((index, delimiter.len()));
        }
    }

    None
}

fn format_openai_stream_delta(delta: Value) -> String {
    format!(
        "data: {}\n\n",
        json!({
            "choices": [{
                "delta": delta
            }]
        })
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{build_codex_request, CodexEffortLevel, CodexSseToOpenAiBridge};
    use crate::protocol::openai::{
        OpenAiChatMessage, OpenAiChatRequest, OpenAiToolChoice, OpenAiToolDefinition,
        OpenAiToolFunction,
    };

    #[test]
    fn builds_codex_requests_from_chat_completions_requests() {
        let request = OpenAiChatRequest {
            model: "gpt-4o".to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: Some("You are concise.".to_string()),
                    tool_call_id: None,
                    tool_calls: vec![],
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: Some("Hello".to_string()),
                    tool_call_id: None,
                    tool_calls: vec![],
                },
            ],
            tools: vec![OpenAiToolDefinition {
                kind: "function".to_string(),
                function: OpenAiToolFunction {
                    name: "lookup_weather".to_string(),
                    description: Some("Lookup the weather".to_string()),
                    parameters: json!({"type":"object"}),
                },
            }],
            tool_choice: Some(OpenAiToolChoice::Auto),
            stream: false,
            max_tokens: Some(128),
        };

        let built = build_codex_request(&request, CodexEffortLevel::Medium);
        assert_eq!(built.model, "gpt-4o");
        assert_eq!(built.instructions, "You are concise.");
        assert_eq!(built.input.len(), 1);
        assert_eq!(built.input[0].role, "user");
        assert_eq!(built.tools[0].name, "lookup_weather");
        assert_eq!(built.reasoning.effort, CodexEffortLevel::Medium);
    }

    #[test]
    fn builds_codex_requests_with_low_effort() {
        let request = OpenAiChatRequest {
            model: "gpt-5.4".to_string(),
            messages: vec![],
            tools: vec![],
            tool_choice: None,
            stream: false,
            max_tokens: None,
        };

        let built = build_codex_request(&request, CodexEffortLevel::Low);
        assert_eq!(built.reasoning.effort, CodexEffortLevel::Low);
    }

    #[test]
    fn builds_codex_requests_with_high_effort() {
        let request = OpenAiChatRequest {
            model: "gpt-5.4".to_string(),
            messages: vec![],
            tools: vec![],
            tool_choice: None,
            stream: false,
            max_tokens: None,
        };

        let built = build_codex_request(&request, CodexEffortLevel::High);
        assert_eq!(built.reasoning.effort, CodexEffortLevel::High);
    }

    #[test]
    fn translates_codex_text_deltas_into_openai_stream_chunks() {
        let mut bridge = CodexSseToOpenAiBridge::default();
        let output = bridge
            .push_bytes(
                b"event: response.output_text.delta\ndata: {\"delta\":\"Hello\"}\n\n\
                  event: response.completed\ndata: {\"type\":\"response.completed\"}\n\n",
            )
            .expect("bridge should translate");

        assert!(output.contains("\"content\":\"Hello\""));
        assert!(output.contains("\"finish_reason\":\"stop\""));
        assert!(output.contains("data: [DONE]"));
    }

    #[test]
    fn accumulates_codex_tool_calls_into_chat_completions_shape() {
        let mut bridge = CodexSseToOpenAiBridge::default();
        bridge
            .push_bytes(
                b"event: response.function_call_arguments.delta\ndata: {\"delta\":\"{\\\"city\\\":\\\"Madrid\\\"}\"}\n\n\
                  event: response.output_item.done\ndata: {\"item\":{\"type\":\"function_call\",\"call_id\":\"call_weather\",\"name\":\"lookup_weather\"}}\n\n\
                  data: [DONE]\n\n",
            )
            .expect("bridge should accumulate tool calls");

        let response = bridge.into_chat_response();
        assert_eq!(
            response["choices"][0]["message"]["tool_calls"][0]["function"]["name"],
            "lookup_weather"
        );
        assert_eq!(
            response["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
            "{\"city\":\"Madrid\"}"
        );
    }
}
